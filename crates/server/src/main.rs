mod accounts;
mod audio;
mod bind;
mod build_info;
mod desktop;
mod hardware_meter;
mod loops;
mod media;
mod monitor_packets;
mod node_io;
mod osc;
mod output_buffer;
mod performance;
mod persistence;
mod presence;
mod recordings;
mod resources;
mod revisions;
mod sample_library;
mod samples;
mod score_automation;
mod settings;
mod subgraphs;
mod tls;
use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use axum::{
    Json, Router,
    extract::{
        Path, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use pr0_core::{Mode, Project, catalog, demo_project};
use rusqlite::{Connection, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{broadcast, oneshot};
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

#[derive(Clone)]
struct App {
    presence: Arc<Mutex<presence::Presence>>,
    resources: Arc<Mutex<resources::Stats>>,
    osc: Arc<osc::Runtime>,
    db: Arc<Mutex<Connection>>,
    events: broadcast::Sender<Value>,
    engine: std::sync::mpsc::SyncSender<audio::Command>,
    active: Arc<Mutex<Option<String>>>,
    performance: Arc<Mutex<Option<String>>>,
    graph: Arc<Mutex<Option<String>>>,
    setup: Arc<tokio::sync::Mutex<()>>,
    logs: Arc<settings::Logs>,
    secure: bool,
    media: Arc<media::Media>,
}
type Api<T> = Result<T, Failure>;
struct Failure(StatusCode, String);
impl IntoResponse for Failure {
    fn into_response(self) -> Response {
        (self.0, Json(json!({"error":self.1}))).into_response()
    }
}
fn bad(e: impl ToString) -> Failure {
    Failure(StatusCode::BAD_REQUEST, e.to_string())
}
fn internal(e: impl ToString) -> Failure {
    tracing::error!("{}", e.to_string());
    Failure(
        StatusCode::INTERNAL_SERVER_ERROR,
        "Internal server error".into(),
    )
}
fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}
fn uid() -> String {
    Uuid::new_v4().to_string()
}
fn csrf(headers: &HeaderMap) -> Api<()> {
    if headers.get("x-pr0former").and_then(|v| v.to_str().ok()) != Some("1") {
        return Err(Failure(
            StatusCode::FORBIDDEN,
            "Missing request header".into(),
        ));
    }
    Ok(())
}
fn user(app: &App, headers: &HeaderMap) -> Api<String> {
    let cookie = headers
        .get(header::COOKIE)
        .and_then(|x| x.to_str().ok())
        .unwrap_or("");
    let token = cookie
        .split(';')
        .filter_map(|x| x.trim().split_once('='))
        .find(|(k, _)| *k == "pr0_session")
        .map(|(_, v)| v)
        .unwrap_or("");
    app.db
        .lock()
        .unwrap()
        .query_row(
            "SELECT s.user_id FROM sessions s JOIN user_profiles p ON p.user_id=s.user_id WHERE s.token=?1 AND s.expires>?2 AND p.enabled=1 AND p.deleted=0",
            params![token, now()],
            |r| r.get(0),
        )
        .map_err(|_| Failure(StatusCode::UNAUTHORIZED, "Please sign in".into()))
}
fn role(app: &App, project: &str, user: &str) -> Api<String> {
    app.db
        .lock()
        .unwrap()
        .query_row(
            "SELECT role FROM members WHERE project_id=?1 AND user_id=?2",
            params![project, user],
            |r| r.get(0),
        )
        .map_err(|_| Failure(StatusCode::FORBIDDEN, "Project access required".into()))
}
fn can_edit(role: &str) -> Api<()> {
    if !matches!(role, "owner" | "editor" | "conductor") {
        return Err(Failure(
            StatusCode::FORBIDDEN,
            "Editor access required".into(),
        ));
    }
    Ok(())
}
fn load(app: &App, id: &str) -> Api<Project> {
    let body: String = app
        .db
        .lock()
        .unwrap()
        .query_row("SELECT body FROM projects WHERE id=?1", [id], |r| r.get(0))
        .map_err(|_| Failure(StatusCode::NOT_FOUND, "Project not found".into()))?;
    serde_json::from_str(&body).map_err(internal)
}
fn update_working_copy(app: &App, project: &mut Project) -> Api<()> {
    let mut db = app.db.lock().unwrap();
    let tx = db.transaction().map_err(internal)?;
    let previous = project.revision;
    project.revision += 1;
    let body = serde_json::to_string(project).map_err(internal)?;
    let changed = tx
        .execute(
            "UPDATE projects SET body=?1,revision=?2 WHERE id=?3 AND revision=?4",
            params![body, project.revision, project.id, previous],
        )
        .map_err(internal)?;
    if changed == 0 {
        return Err(Failure(
            StatusCode::CONFLICT,
            "Project changed. Reload before editing.".into(),
        ));
    }
    revisions::schedule(&tx, &project.id, now()).map_err(internal)?;
    tx.commit().map_err(internal)?;
    Ok(())
}
fn publish_save(app: &App, id: &str, status: &revisions::SaveStatus, automatic: bool) {
    app.logs.push(
        id,
        "info",
        &format!(
            "Revision {} {}",
            status.revision,
            if automatic { "autosaved" } else { "saved" }
        ),
    );
    let _ = app
        .events
        .send(json!({"type":"project_save","project_id":id,"save":status}));
}
async fn resource_stats(State(app): State<App>, headers: HeaderMap) -> Api<Json<resources::Stats>> {
    user(&app, &headers)?;
    Ok(Json(app.resources.lock().unwrap().clone()))
}
async fn save_status(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<Json<revisions::SaveStatus>> {
    let u = user(&app, &headers)?;
    role(&app, &id, &u)?;
    let saved = revisions::status(&app.db.lock().unwrap(), &id).map_err(internal)?;
    Ok(Json(saved))
}
async fn save_project(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<Json<revisions::SaveStatus>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    can_edit(&role(&app, &id, &u)?)?;
    let _guard = app.setup.lock().await;
    let saved = revisions::snapshot(&mut app.db.lock().unwrap(), &id).map_err(internal)?;
    publish_save(&app, &id, &saved, false);
    Ok(Json(saved))
}
fn start_autosave(app: App) {
    tokio::spawn(async move {
        let mut timer = tokio::time::interval(Duration::from_secs(1));
        loop {
            timer.tick().await;
            // SQLite serialization is off the audio worker and device callbacks.
            let worker = app.clone();
            let result = tokio::task::spawn_blocking(move || -> rusqlite::Result<()> {
                let mut db = worker.db.lock().unwrap();
                for id in revisions::due(&db, now())? {
                    let saved = revisions::snapshot(&mut db, &id)?;
                    publish_save(&worker, &id, &saved, true);
                }
                Ok(())
            })
            .await;
            if !matches!(result, Ok(Ok(()))) {
                app.logs.push(
                    "",
                    "error",
                    "Automatic revision save failed; pending saves will retry",
                );
            }
        }
    });
}

fn publish(app: &App, p: &Project) {
    let _ = app
        .events
        .send(json!({"type":"project","project_id":p.id,"project":p}));
}
fn send(app: &App, c: audio::Command) -> Api<()> {
    app.engine.try_send(c).map_err(|_| {
        Failure(
            StatusCode::SERVICE_UNAVAILABLE,
            "Audio command queue busy".into(),
        )
    })
}

#[derive(Deserialize)]
struct Credentials {
    username: String,
    password: String,
    invite: Option<String>,
}
async fn register(
    State(app): State<App>,
    headers: HeaderMap,
    Json(c): Json<Credentials>,
) -> Api<Response> {
    csrf(&headers)?;
    let username = c.username.trim().to_lowercase();
    if !(3..=64).contains(&username.len()) || c.password.len() < 8 || c.password.len() > 256 {
        return Err(bad(
            "Use a 3–64 character username and a 8–256 character password",
        ));
    }
    let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes()).map_err(internal)?;
    let hash = Argon2::default()
        .hash_password(c.password.as_bytes(), &salt)
        .map_err(internal)?
        .to_string();
    {
        let mut db = app.db.lock().unwrap();
        let tx = db.transaction().map_err(internal)?;
        let count: i64 = tx
            .query_row("SELECT count(*) FROM users", [], |r| r.get(0))
            .map_err(internal)?;
        if count > 0 {
            let valid: i64 = tx
                .query_row(
                    "SELECT count(*) FROM invites WHERE token=?1 AND expires>?2 AND used=0",
                    params![c.invite.as_deref().unwrap_or(""), now()],
                    |r| r.get(0),
                )
                .map_err(internal)?;
            if valid == 0 {
                return Err(Failure(
                    StatusCode::FORBIDDEN,
                    "An invitation is required to create an account".into(),
                ));
            }
        }
        tx.execute(
            "INSERT INTO users(id,username,password) VALUES(?1,?2,?3)",
            params![uid(), username, hash],
        )
        .map_err(|_| bad("Username is already registered"))?;
        tx.commit().map_err(internal)?;
    }
    login(
        State(app),
        headers,
        Json(Credentials {
            username,
            password: c.password,
            invite: c.invite,
        }),
    )
    .await
}
async fn login(
    State(app): State<App>,
    headers: HeaderMap,
    Json(c): Json<Credentials>,
) -> Api<Response> {
    csrf(&headers)?;
    let record: Option<(String, String)> = app
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT u.id,u.password FROM users u JOIN user_profiles p ON p.user_id=u.id WHERE u.username=?1 AND p.enabled=1 AND p.deleted=0",
            [c.username.trim().to_lowercase()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .ok();
    let (id, hash) =
        record.ok_or_else(|| Failure(StatusCode::UNAUTHORIZED, "Invalid credentials".into()))?;
    Argon2::default()
        .verify_password(
            c.password.as_bytes(),
            &PasswordHash::new(&hash).map_err(internal)?,
        )
        .map_err(|_| Failure(StatusCode::UNAUTHORIZED, "Invalid credentials".into()))?;
    let token = uid() + &uid();
    app.db
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO sessions(token,user_id,expires) VALUES(?1,?2,?3)",
            params![token, id, now() + 86400],
        )
        .map_err(internal)?;
    let cookie = format!(
        "pr0_session={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=86400{}",
        if app.secure { "; Secure" } else { "" }
    );
    Ok((
        [(header::SET_COOKIE, cookie)],
        Json(accounts::profile(&app.db.lock().unwrap(), &id)?),
    )
        .into_response())
}
async fn logout(State(app): State<App>, headers: HeaderMap) -> Api<Response> {
    csrf(&headers)?;
    let id = user(&app, &headers)?;
    app.db
        .lock()
        .unwrap()
        .execute("DELETE FROM sessions WHERE user_id=?1", [id])
        .map_err(internal)?;
    Ok((
        [(
            header::SET_COOKIE,
            "pr0_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0",
        )],
        Json(json!({"ok":true})),
    )
        .into_response())
}
async fn me(State(app): State<App>, headers: HeaderMap) -> Api<Json<Value>> {
    let id = user(&app, &headers)?;
    accounts::profile(&app.db.lock().unwrap(), &id).map(Json)
}
async fn status(State(app): State<App>) -> Json<Value> {
    let count: i64 = app
        .db
        .lock()
        .unwrap()
        .query_row("SELECT count(*) FROM users", [], |r| r.get(0))
        .unwrap_or(1);
    let ownership = engine_status(&app);
    Json(
        json!({"bootstrap":count==0,"version":env!("CARGO_PKG_VERSION"),"build":build_info::json(),"active_project":ownership["active_project"],"graph_project":ownership["graph_project"],"monitor_transport":"webrtc_opus"}),
    )
}
async fn list_projects(State(app): State<App>, headers: HeaderMap) -> Api<Json<Value>> {
    let id = user(&app, &headers)?;
    let db = app.db.lock().unwrap();
    let mut s=db.prepare("SELECT p.body,m.role,(SELECT opened FROM project_recents WHERE user_id=?1 AND project_id=p.id),(SELECT group_concat(u.username,', ') FROM members own JOIN users u ON u.id=own.user_id WHERE own.project_id=p.id AND own.role='owner') FROM projects p JOIN members m ON m.project_id=p.id WHERE m.user_id=?1 ORDER BY p.rowid DESC").map_err(internal)?;
    let rows = s
        .query_map([id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<u64>>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })
        .map_err(internal)?;
    let mut result = vec![];
    for row in rows {
        let (body, role, opened, owner) = row.map_err(internal)?;
        let p: Project = serde_json::from_str(&body).map_err(internal)?;
        result
            .push(json!({"id":p.id,"name":p.name,"mode":p.mode,"role":role,"revision":p.revision,"bpm":p.bpm,"beats_per_bar":p.beats_per_bar,"beat_unit":p.beat_unit,"schema_version":p.schema_version,"parts":p.parts.len(),"nodes":p.graph.nodes.len(),"opened":opened,"owner":owner}));
    }
    Ok(Json(json!(result)))
}
#[derive(Deserialize)]
struct Create {
    name: String,
    mode: Mode,
}
async fn create_project(
    State(app): State<App>,
    headers: HeaderMap,
    Json(c): Json<Create>,
) -> Api<Json<Project>> {
    csrf(&headers)?;
    let owner = user(&app, &headers)?;
    let p = demo_project(uid(), c.name, c.mode);
    p.validate().map_err(bad)?;
    settings::validate_routes(&p, &settings::read()).map_err(bad)?;
    let mut db = app.db.lock().unwrap();
    let tx = db.transaction().map_err(internal)?;
    let body = serde_json::to_string(&p).map_err(internal)?;
    tx.execute(
        "INSERT INTO projects(id,body,revision) VALUES(?1,?2,0)",
        params![p.id, body],
    )
    .map_err(internal)?;
    tx.execute(
        "INSERT INTO members(project_id,user_id,role) VALUES(?1,?2,'owner')",
        params![p.id, owner],
    )
    .map_err(internal)?;
    tx.execute(
        "INSERT INTO revisions(project_id,revision,body) VALUES(?1,0,?2)",
        params![p.id, body],
    )
    .map_err(internal)?;
    tx.execute("INSERT INTO project_recents(user_id,project_id,opened) VALUES(?1,?2,(SELECT COALESCE(MAX(opened),0)+1 FROM project_recents WHERE user_id=?1))",params![owner,p.id]).map_err(internal)?;
    tx.commit().map_err(internal)?;
    Ok(Json(p))
}
async fn get_project(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<Json<Value>> {
    let u = user(&app, &headers)?;
    let r = role(&app, &id, &u)?;
    let _guard = app.setup.lock().await;
    let project = load(&app, &id)?;
    let prepared = project.clone();
    let rate = settings::read().sample_rate;
    tokio::task::spawn_blocking(move || samples::cache_project(&prepared, rate))
        .await
        .map_err(internal)?
        .map_err(bad)?;
    app.logs
        .push(&id, "info", "Project loaded; sample caches ready");
    Ok(Json(json!({"project":project,"role":r})))
}
fn validate_live_update(previous: &Project, next: &Project) -> Result<(), String> {
    if next.mode != previous.mode {
        return Err("Mode changes require deactivation".into());
    }

    Ok(())
}

async fn update_project(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(mut p): Json<Project>,
) -> Api<Json<Project>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    can_edit(&role(&app, &id, &u)?)?;
    let _guard = app.setup.lock().await;
    if p.id != id {
        return Err(bad("Project ID mismatch"));
    }
    if app.performance.lock().unwrap().as_deref() == Some(&id) {
        return Err(bad("Exit performance mode before editing"));
    }
    let previous = load(&app, &id)?;
    sample_library::assign_roots(&app.db.lock().unwrap(), &previous, &mut p)?;
    p.validate().map_err(bad)?;
    let active = app.active.lock().unwrap().as_deref() == Some(&id);
    settings::validate_route_changes(&previous, &p, &settings::read()).map_err(bad)?;
    for node in &p.graph.nodes {
        if pr0_core::named_route(&node.kind)
            && previous
                .graph
                .nodes
                .iter()
                .any(|old| old.id == node.id && old.control_value != node.control_value)
            && previous
                .graph
                .flatten()
                .map_err(bad)?
                .edges
                .iter()
                .any(|e| e.target == node.id && e.target_port == "target")
        {
            return Err(bad(
                "Connected route target is read-only; disconnect it before editing",
            ));
        }
        if matches!(node.kind.as_str(), "control_visualizer" | "control_input")
            && previous
                .graph
                .nodes
                .iter()
                .any(|old| old.id == node.id && old.control_value != node.control_value)
            && previous
                .graph
                .edges
                .iter()
                .any(|edge| edge.target == node.id && edge.target_port == "in")
        {
            return Err(bad(
                "Connected visualizer input is read-only; disconnect it before editing",
            ));
        }
    }

    if active {
        validate_live_update(&previous, &p)
            .map_err(|message| Failure(StatusCode::CONFLICT, message))?;
    }
    let prepared = if app.graph.lock().unwrap().as_deref() == Some(&id) {
        let prepared_project = p.clone();
        Some(
            tokio::task::spawn_blocking(move || samples::prepare(&prepared_project))
                .await
                .map_err(internal)?
                .map_err(bad)?,
        )
    } else {
        None
    };
    update_working_copy(&app, &mut p)?;
    if let Some(engine) = prepared {
        send(
            &app,
            audio::Command::Replace {
                project: p.clone(),
                engine: Box::new(engine),
            },
        )?;
    }
    publish(&app, &p);
    Ok(Json(p))
}
#[derive(Deserialize)]
struct ControlEdit {
    node: String,
    revision: u64,
    value: Option<pr0_core::ControlValue>,
}
async fn control_input(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(c): Json<ControlEdit>,
) -> Api<Json<Project>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    can_edit(&role(&app, &id, &u)?)?;
    let _guard = app.setup.lock().await;
    let mut p = load(&app, &id)?;
    if p.revision != c.revision {
        return Err(Failure(StatusCode::CONFLICT, "Stale control edit".into()));
    }
    if let Some(node) = p
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.id == c.node && n.kind == "toggle")
    {
        let value = c.value.ok_or_else(|| bad("Toggle value required"))?;
        if !matches!(&value, pr0_core::ControlValue::Number(v) if *v == 0. || *v == 1.) {
            return Err(bad("Toggle value must be 0 or 1"));
        }
        node.control_value = Some(value.clone());
        p.validate().map_err(bad)?;
        update_working_copy(&app, &mut p)?;
        if app.graph.lock().unwrap().as_deref() == Some(&id) {
            send(
                &app,
                audio::Command::Control {
                    node: c.node,
                    value,
                    revision: p.revision,
                },
            )?;
        }
        publish(&app, &p);
        return Ok(Json(p));
    }
    if p.graph
        .nodes
        .iter()
        .any(|n| n.id == c.node && n.kind == "trigger")
    {
        if c.value.is_some() {
            return Err(bad("Trigger accepts a pulse, not a stored value"));
        }
        if app.graph.lock().unwrap().as_deref() != Some(&id) {
            return Err(bad("Enable this project's audio engine before triggering"));
        }
        send(&app, audio::Command::Bang(c.node))?;
        return Ok(Json(p));
    }
    if p.graph
        .flatten()
        .map_err(bad)?
        .edges
        .iter()
        .any(|e| e.target == c.node && e.target_port == "in")
    {
        return Err(bad("Connected graphical controls are read-only"));
    }
    let node = p
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.id == c.node && n.kind == "control_input")
        .ok_or_else(|| bad("Graphical control missing"))?;
    let active = app.graph.lock().unwrap().as_deref() == Some(&id);
    if node.parameters.get("mode") == Some(&0.) {
        if c.value.is_some() {
            return Err(bad("Use a Bang trigger, not a stored value"));
        }
        if !active {
            return Err(bad(
                "Enable this project’s audio engine before triggering Bang",
            ));
        }
        send(&app, audio::Command::Bang(c.node))?;
    } else {
        let value = c.value.ok_or_else(|| bad("Control value required"))?;
        node.control_value = Some(value.clone());
        p.validate().map_err(bad)?;
        update_working_copy(&app, &mut p)?;
        if active {
            send(
                &app,
                audio::Command::Control {
                    node: c.node,
                    value,
                    revision: p.revision,
                },
            )?;
        }
        publish(&app, &p);
    }
    Ok(Json(p))
}
#[derive(Deserialize)]
struct ClearLoop {
    node: String,
    track: u8,
}
async fn clear_loop(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(command): Json<ClearLoop>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    can_edit(&role(&app, &id, &u)?)?;
    let _guard = app.setup.lock().await;
    let p = load(&app, &id)?;
    if !(1..=8).contains(&command.track) {
        return Err(bad("Loop track must be 1–8"));
    }
    if !p
        .graph
        .nodes
        .iter()
        .any(|n| n.id == command.node && n.kind == "looper")
    {
        return Err(bad("Looper node missing"));
    }
    if app.graph.lock().unwrap().as_deref() == Some(&id) {
        let (tx, rx) = oneshot::channel();
        send(
            &app,
            audio::Command::ClearLoop {
                project: id,
                node: command.node,
                track: command.track,
                reply: tx,
            },
        )?;
        rx.await.map_err(internal)?.map_err(bad)?;
    } else {
        tokio::task::spawn_blocking(move || loops::clear_saved(&id, &command.node, command.track))
            .await
            .map_err(internal)?
            .map_err(bad)?;
    }
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
struct Audition {
    part: String,
    note: String,
}
async fn audition(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<Audition>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    can_edit(&role(&app, &id, &u)?)?;
    let _guard = app.setup.lock().await;
    if app.performance.lock().unwrap().as_deref() == Some(&id) {
        return Err(bad("Note entry is disabled in performance mode"));
    }
    let p = load(&app, &id)?;
    let part = p
        .parts
        .iter()
        .find(|p| p.id == request.part)
        .ok_or_else(|| bad("Part missing"))?;
    let note = part
        .notes
        .iter()
        .find(|n| n.id == request.note)
        .ok_or_else(|| bad("Note missing"))?;
    if app.graph.lock().unwrap().as_deref() == Some(&id)
        && !note.rest
        && !part.muted
        && (!p.parts.iter().any(|p| p.solo) || part.solo)
    {
        let staff = note
            .notation
            .as_ref()
            .and_then(|v| part.staves.iter().position(|s| s.id == v.staff))
            .unwrap_or(0);
        let config = part.staves.get(staff);
        send(
            &app,
            audio::Command::Audition {
                project: id,
                part: part.id.clone(),
                staff: (staff + 1) as u8,
                node: config
                    .and_then(|s| s.instrument_node.clone())
                    .or_else(|| part.instrument_node.clone()),
                channel: config
                    .and_then(|s| s.midi_channel)
                    .unwrap_or(part.midi_channel),
                pitch: note.pitch,
                velocity: note.velocity,
            },
        )?;
    }
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
struct PianoNote {
    node: String,
    pitch: u8,
    velocity: u8,
}
async fn piano_note(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(note): Json<PianoNote>,
) -> Api<Json<serde_json::Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    can_edit(&role(&app, &id, &u)?)?;
    let _guard = app.setup.lock().await;
    let p = load(&app, &id)?;
    if note.pitch > 127 || note.velocity > 127 {
        return Err(bad("Invalid MIDI note"));
    }
    if !p
        .graph
        .nodes
        .iter()
        .any(|n| n.id == note.node && n.kind == "piano")
    {
        return Err(bad("Piano node missing"));
    }
    if app.graph.lock().unwrap().as_deref() != Some(&id) {
        return Err(bad(
            "Enable this project’s audio engine before playing piano keys",
        ));
    }
    send(
        &app,
        audio::Command::Piano {
            project: id,
            node: note.node,
            pitch: note.pitch,
            velocity: note.velocity,
        },
    )?;
    Ok(Json(serde_json::json!({"ok":true})))
}
#[derive(Deserialize)]
struct ParameterEdit {
    node: String,
    parameter: String,
    value: f64,
    revision: u64,
}
async fn parameter(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(c): Json<ParameterEdit>,
) -> Api<Json<Project>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    can_edit(&role(&app, &id, &u)?)?;
    let _guard = app.setup.lock().await;
    let mut p = load(&app, &id)?;
    let previous = p.clone();
    if p.revision != c.revision {
        return Err(Failure(StatusCode::CONFLICT, "Stale parameter edit".into()));
    }
    if p.graph
        .edges
        .iter()
        .any(|e| e.target == c.node && e.target_port == c.parameter)
    {
        return Err(bad("Connected parameters are read-only"));
    }
    let n = p
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.id == c.node)
        .ok_or_else(|| bad("Node missing"))?;
    let d = catalog().into_iter().find(|d| d.kind == n.kind).unwrap();
    let meta = d
        .parameters
        .iter()
        .find(|x| x.id == c.parameter)
        .ok_or_else(|| bad("Parameter missing"))?;
    let active = app.graph.lock().unwrap().as_deref() == Some(&id);
    if active && meta.structural {
        return Err(bad("Deactivate before changing structural parameters"));
    }
    n.parameters.insert(c.parameter.clone(), c.value);
    sample_library::assign_roots(&app.db.lock().unwrap(), &previous, &mut p)?;
    p.validate().map_err(bad)?;
    settings::validate_route_changes(&previous, &p, &settings::read()).map_err(bad)?;
    // Serialize revisions before enqueueing; failed runtime delivery is reported explicitly.
    update_working_copy(&app, &mut p)?;
    if active {
        send(
            &app,
            audio::Command::Parameter {
                node: c.node,
                key: c.parameter,
                value: c.value,
                revision: p.revision,
            },
        )?;
    }
    publish(&app, &p);
    Ok(Json(p))
}
#[derive(Deserialize)]
struct Transport {
    action: String,
    #[serde(default)]
    count_in_beats: u8,
    bpm: Option<f64>,
    #[serde(default)]
    enabled: Option<bool>,
}
async fn transport(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(c): Json<Transport>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    let r = role(&app, &id, &u)?;
    if !matches!(r.as_str(), "owner" | "conductor") {
        return Err(Failure(
            StatusCode::FORBIDDEN,
            "Transport authority required".into(),
        ));
    }
    if c.count_in_beats > 32 {
        return Err(bad("Count-in must be 0–32 beats"));
    }
    let _guard = app.setup.lock().await;
    if c.action == "performance" || c.action == "prepare" {
        if c.action == "performance" {
            *app.performance.lock().unwrap() = Some(id.clone());
        } else if app.performance.lock().unwrap().as_deref() == Some(&id) {
            *app.performance.lock().unwrap() = None;
        }
        let _ = app.events.send(engine_status(&app));
        return Ok(Json(json!({"ok":true})));
    }
    if c.action == "play"
        && app.graph.lock().unwrap().as_deref() == Some(&id)
        && app.active.lock().unwrap().as_deref() != Some(&id)
    {
        send(&app, audio::Command::Show(true))?;
        *app.active.lock().unwrap() = Some(id.clone());
        let _ = app.events.send(engine_status(&app));
    }
    if c.action == "activate" {
        if app
            .active
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|other| other != &id)
        {
            return Err(bad("Another show is active"));
        }
        if app.active.lock().unwrap().as_deref() == Some(&id) {
            return Ok(Json(json!({"ok":true})));
        }
        can_switch_graph(&app, &id, &u)?;
        start_graph(&app, &id).await?;
        send(&app, audio::Command::Show(true))?;
        *app.active.lock().unwrap() = Some(id.clone());
        let _ = app.events.send(engine_status(&app));
    } else if c.action == "deactivate" {
        if app.active.lock().unwrap().as_deref() != Some(&id) {
            return Err(bad("Project is not active"));
        }
        send(&app, audio::Command::Show(false))?;
        *app.active.lock().unwrap() = None;
        let _ = app.events.send(engine_status(&app));
    } else {
        if (!matches!(c.action.as_str(), "tempo" | "metronome")
            && app.active.lock().unwrap().as_deref() != Some(&id))
            || app.graph.lock().unwrap().as_deref() != Some(&id)
        {
            return Err(bad("Enable this project’s audio engine first"));
        }
        match c.action.as_str() {
            "play" | "pause" | "stop" => send(
                &app,
                audio::Command::Transport {
                    action: c.action,
                    count_in_beats: c.count_in_beats,
                },
            )?,
            "metronome" => {
                send(&app, audio::Command::Metronome(c.enabled.unwrap_or(true)))?;
            }
            "tempo" => {
                let project = load(&app, &id)?;
                if project.graph.edges.iter().any(|e| {
                    e.target_port == "tempo"
                        && project
                            .graph
                            .nodes
                            .iter()
                            .any(|n| n.id == e.target && n.kind == "clock")
                }) {
                    return Err(bad("Project tempo is driven by a global clock connection"));
                }
                let bpm = c.bpm.ok_or_else(|| bad("Tempo required"))?;
                if !bpm.is_finite() || !(1.0..=400.0).contains(&bpm) {
                    return Err(bad("Tempo must be 1–400 BPM"));
                }
                send(&app, audio::Command::Tempo(bpm))?;
            }
            _ => return Err(bad("Unknown transport action")),
        }
    }
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
struct ClipAction {
    part: String,
    playing: bool,
}
async fn clip(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(c): Json<ClipAction>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    let r = role(&app, &id, &u)?;
    let p = load(&app, &id)?;
    if app.active.lock().unwrap().as_deref() != Some(&id) {
        return Err(bad("Activate this project first"));
    }
    if p.mode == Mode::Structured {
        return Err(bad("Structured projects use the shared transport"));
    }
    if !matches!(r.as_str(), "owner" | "conductor")
        && !(p.mode == Mode::Freeform
            && p.parts
                .iter()
                .any(|part| part.id == c.part && part.performer.as_deref() == Some(&u)))
    {
        return Err(Failure(
            StatusCode::FORBIDDEN,
            "You cannot launch this part".into(),
        ));
    }
    if !p.parts.iter().any(|p| p.id == c.part) {
        return Err(bad("Part missing"));
    }
    send(
        &app,
        audio::Command::Clip {
            part: c.part,
            playing: c.playing,
        },
    )?;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
struct Invite {
    role: String,
}
async fn invite(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(c): Json<Invite>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    if role(&app, &id, &u)? != "owner" {
        return Err(Failure(
            StatusCode::FORBIDDEN,
            "Owner access required".into(),
        ));
    }
    if !matches!(c.role.as_str(), "performer" | "editor" | "conductor") {
        return Err(bad("Invalid invitation role"));
    }
    let token = uid() + &uid();
    app.db
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO invites(token,project_id,role,expires,used) VALUES(?1,?2,?3,?4,0)",
            params![token, id, c.role, now() + 7 * 86400],
        )
        .map_err(internal)?;
    Ok(Json(
        json!({"token":token,"path":format!("/?invite={token}")}),
    ))
}
#[derive(Deserialize)]
struct Join {
    token: String,
}
async fn join(State(app): State<App>, headers: HeaderMap, Json(c): Json<Join>) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    let mut db = app.db.lock().unwrap();
    let tx = db.transaction().map_err(internal)?;
    let (id, r): (String, String) = tx
        .query_row(
            "SELECT project_id,role FROM invites WHERE token=?1 AND used=0 AND expires>?2",
            params![c.token, now()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|_| bad("Invitation expired or already used"))?;
    tx.execute("INSERT INTO members(project_id,user_id,role) VALUES(?1,?2,?3) ON CONFLICT(project_id,user_id) DO NOTHING",params![id,u,r]).map_err(internal)?;
    tx.execute("UPDATE invites SET used=1 WHERE token=?1", [c.token])
        .map_err(internal)?;
    tx.commit().map_err(internal)?;
    Ok(Json(json!({"project_id":id})))
}
async fn members(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<Json<Value>> {
    let u = user(&app, &headers)?;
    role(&app, &id, &u)?;
    let db = app.db.lock().unwrap();
    let mut s=db.prepare("SELECT u.id,u.username,m.role FROM members m JOIN users u ON u.id=m.user_id WHERE m.project_id=?1").map_err(internal)?;
    let rows=s.query_map([id],|r|Ok(json!({"id":r.get::<_,String>(0)?,"username":r.get::<_,String>(1)?,"role":r.get::<_,String>(2)?}))).map_err(internal)?.collect::<Result<Vec<_>,_>>().map_err(internal)?;
    Ok(Json(json!(rows)))
}
async fn devices(State(app): State<App>, headers: HeaderMap) -> Api<Json<Value>> {
    let u = user(&app, &headers)?;
    let mut inventory = {
        let _setup = app.setup.lock().await;
        let admin = accounts::is_admin(&app.db.lock().unwrap(), &u);
        let settings = if admin {
            settings::discover().await?
        } else {
            settings::read()
        };
        tokio::task::spawn_blocking(move || audio::device_inventory(settings.sample_rate))
            .await
            .map_err(internal)?
    };
    let (tx, rx) = oneshot::channel();
    send(&app, audio::Command::Devices(tx))?;
    let runtime = tokio::time::timeout(Duration::from_secs(5), rx)
        .await
        .map_err(internal)?
        .map_err(internal)?;
    inventory
        .as_object_mut()
        .unwrap()
        .extend(runtime.as_object().unwrap().clone());
    Ok(Json(inventory))
}
#[derive(Deserialize)]
struct DeviceAction {
    enabled: bool,
    #[serde(default)]
    input: bool,
}
async fn audio_enable(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(c): Json<DeviceAction>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    if role(&app, &id, &u)? != "owner" {
        return Err(Failure(
            StatusCode::FORBIDDEN,
            "Owner access required".into(),
        ));
    }
    if app.graph.lock().unwrap().as_deref() != Some(&id) {
        return Err(bad("Enable the audio engine for this project first"));
    }
    if c.input {
        return Err(bad("Native inputs start and stop with the audio engine"));
    }
    send(&app, audio::Command::Hardware(c.enabled))?;
    Ok(Json(json!({"ok":true})))
}
async fn enable_engine(app: &App, id: &str, enabled: bool) -> Api<()> {
    if enabled {
        settings::discover().await?;
    }
    let (tx, rx) = oneshot::channel();
    send(
        app,
        audio::Command::Enable(id.into(), enabled, settings::read(), tx),
    )?;
    let result = tokio::time::timeout(Duration::from_secs(10), rx)
        .await
        .map_err(internal)?
        .map_err(internal)?;
    match result {
        Ok(()) => {
            app.logs.push(
                id,
                "info",
                if enabled {
                    "Audio engine enabled; selected interfaces opened"
                } else {
                    "Audio engine disabled"
                },
            );
            Ok(())
        }
        Err(e) => {
            send(app, audio::Command::Unload)?;
            *app.graph.lock().unwrap() = None;
            let _ = app.events.send(engine_status(app));
            app.logs.push(id, "error", &e);
            Err(bad(e))
        }
    }
}
// Called under setup lock. Preparing first leaves the current graph intact on failure.
async fn start_graph(app: &App, id: &str) -> Api<()> {
    if app.graph.lock().unwrap().as_deref() == Some(id) {
        return Ok(());
    }
    if app
        .active
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|other| other != id)
    {
        return Err(bad("Another show owns the audio engine"));
    }
    if let Some(other) = app.graph.lock().unwrap().clone() {
        // Development ownership may only be switched by someone authorized for both projects.
        // The caller verifies the previous project's role before reaching this helper.
        app.logs.push(&other, "info", "Development graph released");
    }
    settings::discover().await?;
    let project = load(app, id)?;
    let prepared = project.clone();
    let engine = tokio::task::spawn_blocking(move || samples::prepare(&prepared))
        .await
        .map_err(internal)?
        .map_err(bad)?;
    enable_engine(app, id, true).await?;
    send(app, audio::Command::Load(project, Box::new(engine)))?;
    *app.graph.lock().unwrap() = Some(id.to_owned());
    let _ = app.events.send(engine_status(app));
    Ok(())
}
fn can_switch_graph(app: &App, id: &str, u: &str) -> Api<()> {
    if let Some(other) = app.graph.lock().unwrap().clone() {
        if other != id && !matches!(role(app, &other, u)?.as_str(), "owner" | "conductor") {
            return Err(bad("Another project owns the audio engine"));
        }
    }
    Ok(())
}
async fn engine_enable(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(c): Json<DeviceAction>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    if !matches!(role(&app, &id, &u)?.as_str(), "owner" | "conductor") {
        return Err(bad("Transport authority required"));
    }
    let _guard = app.setup.lock().await;
    if app.active.lock().unwrap().as_deref() == Some(&id) && !c.enabled {
        send(&app, audio::Command::Show(false))?;
        *app.active.lock().unwrap() = None;
    }
    can_switch_graph(&app, &id, &u)?;
    if c.enabled {
        start_graph(&app, &id).await?;
    } else {
        enable_engine(&app, &id, false).await?;
        send(&app, audio::Command::Unload)?;
        *app.graph.lock().unwrap() = None;
        let _ = app.events.send(engine_status(&app));
    }
    Ok(Json(json!({"ok":true})))
}
async fn latency_test(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(c): Json<DeviceAction>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    accounts::admin(&app, &headers)?;
    let u = user(&app, &headers)?;
    if role(&app, &id, &u)? != "owner" {
        return Err(bad("Owner access required"));
    }
    let _guard = app.setup.lock().await;
    if c.enabled && app.active.lock().unwrap().is_some() {
        return Err(bad("Deactivate the show before calibration"));
    }
    if c.enabled {
        if !settings::read().interfaces.iter().any(|i| i.enabled) {
            return Err(bad("Select and save an output interface before testing"));
        }
        can_switch_graph(&app, &id, &u)?;
        send(&app, audio::Command::Unload)?;
        *app.graph.lock().unwrap() = None;
        let _ = app.events.send(engine_status(&app));
        enable_engine(&app, &id, true).await?;
    }
    send(&app, audio::Command::Test(c.enabled))?;
    app.logs.push(
        &id,
        "info",
        if c.enabled {
            "Latency metronome started"
        } else {
            "Latency metronome stopped"
        },
    );
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
struct PreviewQuery {
    edge: String,
}
async fn preview(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    axum::extract::Query(query): axum::extract::Query<PreviewQuery>,
) -> Api<Json<Value>> {
    let u = user(&app, &headers)?;
    role(&app, &id, &u)?;
    let p = load(&app, &id)?;
    let graph = p.graph.flatten().map_err(bad)?;
    let edge = graph
        .edges
        .iter()
        .find(|e| e.id == query.edge)
        .ok_or_else(|| bad("Connection missing"))?;
    let source = graph
        .nodes
        .iter()
        .find(|n| n.id == edge.source)
        .ok_or_else(|| bad("Source missing"))?;
    let catalog = catalog();
    let descriptor = catalog
        .iter()
        .find(|d| d.kind == source.kind)
        .ok_or_else(|| bad("Source type missing"))?;
    let port = descriptor
        .outputs
        .iter()
        .find(|port| port.id == edge.source_port && port.signal == pr0_core::Signal::Audio)
        .ok_or_else(|| bad("Waveforms require an audio connection"))?;
    let (tx, rx) = oneshot::channel();
    send(
        &app,
        audio::Command::Preview {
            project: id,
            source: edge.source.clone(),
            port: edge.source_port.clone(),
            channels: port.fixed_channels.unwrap_or(source.channels),
            reply: tx,
        },
    )?;
    let result = tokio::time::timeout(Duration::from_secs(2), rx)
        .await
        .map_err(internal)?
        .map_err(internal)?
        .map_err(bad)?;
    Ok(Json(result))
}
async fn websocket(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    ws: WebSocketUpgrade,
) -> Api<Response> {
    let origin = headers.get(header::ORIGIN).and_then(|h| h.to_str().ok());
    let host = headers.get(header::HOST).and_then(|h| h.to_str().ok());
    if origin
        .and_then(|o| o.parse::<axum::http::Uri>().ok())
        .and_then(|u| u.authority().map(|a| a.to_string()))
        .as_deref()
        != host
    {
        return Err(Failure(
            StatusCode::FORBIDDEN,
            "WebSocket origin does not match this server".into(),
        ));
    }
    let u = user(&app, &headers)?;
    role(&app, &id, &u)?;
    Ok(ws.on_upgrade(move |socket| stream(socket, app, id, u, headers)))
}
// Snapshot both ownership states; graph processing and show transport are independent.
fn engine_status(app: &App) -> Value {
    let graph = app.graph.lock().unwrap();
    let active = app.active.lock().unwrap();
    json!({"type":"engine_status","performance_project":*app.performance.lock().unwrap(),"active_project":*active,"graph_project":*graph,"server_time":audio::monotonic_ms()})
}

async fn socket_text(socket: &mut WebSocket, value: String) -> Result<(), ()> {
    tokio::time::timeout(
        Duration::from_secs(5),
        socket.send(Message::Text(value.into())),
    )
    .await
    .map_err(|_| ())?
    .map_err(|_| ())
}

async fn send_project_snapshot(socket: &mut WebSocket, app: &App, id: &str, u: &str) -> bool {
    if role(app, id, u).is_err() {
        return false;
    }
    let Ok(project) = load(app, id) else {
        return false;
    };
    if socket_text(
        socket,
        json!({"type":"project","project_id":id,"project":project}).to_string(),
    )
    .await
    .is_err()
    {
        return false;
    }
    let saved = revisions::status(&app.db.lock().unwrap(), id).ok();
    if let Some(saved) = saved {
        if socket_text(
            socket,
            json!({"type":"project_save","project_id":id,"save":saved}).to_string(),
        )
        .await
        .is_err()
        {
            return false;
        }
    }
    let status = engine_status(&app);
    socket_text(socket, status.to_string()).await.is_ok()
}

async fn stream(mut socket: WebSocket, app: App, id: String, u: String, headers: HeaderMap) {
    let _presence = presence::Lease::join(&app, &id).await;
    // Subscribe before reading SQLite so concurrent commits are either in this
    // snapshot or queued below. Clients discard older/equal revisions.
    let mut events = app.events.subscribe();
    if !send_project_snapshot(&mut socket, &app, &id, &u).await {
        return;
    }
    let mut check = tokio::time::interval(Duration::from_secs(10));
    let mut last_received = tokio::time::Instant::now();
    let visualization_session = Uuid::new_v4().to_string();
    let mut visualizers = false;
    loop {
        tokio::select! {
            event=events.recv()=>match event{Ok(mut v)=>{if v["type"]=="session_revoked" && v["user_id"]==u {let _=socket_text(&mut socket,v.to_string()).await;break;} if !visualizers{if let Some(o)=v.as_object_mut(){o.remove("visualizations");}}if (v["type"]=="hardware_levels" || v["type"]=="audio_engine_status" || v["type"]=="system_audio" || v["type"]=="engine_status" || v.get("project_id").and_then(Value::as_str)==Some(&id))&&socket_text(&mut socket, v.to_string()).await.is_err(){break;}},Err(broadcast::error::RecvError::Lagged(_))=>{if !send_project_snapshot(&mut socket, &app, &id, &u).await {break;}},Err(_)=>break},
            msg=socket.recv()=>match msg{Some(Ok(Message::Text(text)))=>{last_received=tokio::time::Instant::now();if let Ok(v)=serde_json::from_str::<Value>(&text){if v["type"]=="visualizers"{visualizers=v["enabled"]==true;let _=send(&app,audio::Command::Visualizers{session:visualization_session.clone(),project:id.clone(),enabled:visualizers});}if v["type"]=="ping"{if visualizers{let _=send(&app,audio::Command::Visualizers{session:visualization_session.clone(),project:id.clone(),enabled:true});}let _=socket_text(&mut socket, json!({"type":"pong","client_time":v["client_time"],"server_time":audio::monotonic_ms()}).to_string()).await;}}},Some(Ok(Message::Close(_)))|None|Some(Err(_))=>break,Some(Ok(_))=>{last_received=tokio::time::Instant::now();}},
            _=check.tick()=>{
                if user(&app,&headers).is_err() || role(&app,&id,&u).is_err() || last_received.elapsed() >= Duration::from_secs(30) {break;}
                if tokio::time::timeout(Duration::from_secs(5), socket.send(Message::Ping(Vec::new().into()))).await.map_or(true, |r| r.is_err()) {break;}
            }
        }
    }
    let _ = send(
        &app,
        audio::Command::Visualizers {
            session: visualization_session,
            project: id,
            enabled: false,
        },
    );
}

#[tokio::main]
async fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--version" | "-V") => {
            println!("{}", build_info::display());
            return;
        }
        Some("--build-info") => {
            println!(
                "pr0former-build-info-v1 {} {} {}",
                build_info::GIT,
                build_info::DIRTY,
                build_info::BUILT
            );
            return;
        }
        _ => {}
    }
    println!("{}", build_info::display());
    let _ = rustls::crypto::ring::default_provider().install_default();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let desktop_mode = std::env::args().any(|arg| arg == "--desktop");
    let directory = std::env::var("PR0_DATA").unwrap_or("data".into());
    std::fs::create_dir_all(&directory).expect("Create data directory");
    let mut db = Connection::open(format!("{directory}/pr0former.sqlite")).expect("Open database");
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
        CREATE TABLE IF NOT EXISTS users(id TEXT PRIMARY KEY,username TEXT UNIQUE NOT NULL,password TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS sessions(token TEXT PRIMARY KEY,user_id TEXT REFERENCES users(id),expires INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS projects(id TEXT PRIMARY KEY,body TEXT NOT NULL,revision INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS members(project_id TEXT REFERENCES projects(id),user_id TEXT REFERENCES users(id),role TEXT NOT NULL,PRIMARY KEY(project_id,user_id));
        CREATE TABLE IF NOT EXISTS revisions(project_id TEXT REFERENCES projects(id),revision INTEGER,body TEXT NOT NULL,PRIMARY KEY(project_id,revision));
        CREATE TABLE IF NOT EXISTS invites(token TEXT PRIMARY KEY,project_id TEXT REFERENCES projects(id),role TEXT,expires INTEGER,used INTEGER);").expect("Database migration");
    accounts::migrate(&db).expect("Account migration");
    sample_library::migrate(&db).expect("Sample library migration");
    subgraphs::migrate(&db).expect("Subgraph library migration");
    revisions::migrate(&db).expect("Revision save migration");
    let desktop_session = desktop_mode
        .then(|| desktop::session(&mut db))
        .transpose()
        .expect("Prepare desktop owner");
    let (events, _) = broadcast::channel(128);
    let media = Arc::new(media::Media::new());
    let logs = Arc::new(settings::Logs::default());
    let osc = osc::Runtime::load();
    let engine = audio::start(
        events.clone(),
        media.audio.clone(),
        logs.clone(),
        osc.clone(),
    );
    let tls_config = if desktop_mode {
        tls::Config {
            pair: None,
            port: None,
        }
    } else {
        tls::Config::load()
    };
    let app = App {
        presence: Arc::new(Mutex::new(presence::Presence::default())),
        resources: resources::start(),
        osc,
        db: Arc::new(Mutex::new(db)),
        events,
        engine,
        active: Arc::new(Mutex::new(None)),
        performance: Arc::new(Mutex::new(None)),
        graph: Arc::new(Mutex::new(None)),
        setup: Arc::new(tokio::sync::Mutex::new(())),
        logs,
        secure: tls_config.pair.is_some(),
        media,
    };
    app.osc.listen(&app);
    start_autosave(app.clone());
    let desktop_app = app.clone();
    let assets = std::env::var("PR0_WEB_ROOT").unwrap_or("web/dist".into());
    let router = Router::new()
        .route("/api/system/osc", get(osc::get))
        .route("/api/projects/{id}/system/osc", put(osc::put))
        .route("/api/status", get(status))
        .route("/api/system/audio", get(settings::get))
        .route("/api/system/stats", get(resource_stats))
        .route(
            "/api/projects/{id}/system/audio",
            axum::routing::put(settings::put),
        )
        .route(
            "/api/projects/{id}/system/audio/device",
            put(settings::device),
        )
        .route("/api/projects/{id}/logs", get(settings::logs))
        .route("/api/projects/{id}/preview", get(preview))
        .route("/api/projects/{id}/control", put(control_input))
        .route("/api/projects/{id}/piano", put(piano_note))
        .route("/api/projects/{id}/audition", post(audition))
        .route("/api/projects/{id}/loops/clear", put(clear_loop))
        .route("/api/subgraphs", get(subgraphs::list))
        .route(
            "/api/subgraphs/{id}/versions/{version}",
            get(subgraphs::get),
        )
        .route("/api/projects/{id}/subgraphs", post(subgraphs::save))
        .route(
            "/api/projects/{id}/subgraphs/insert",
            post(subgraphs::insert),
        )
        .route("/api/projects/{id}/engine", post(engine_enable))
        .route("/api/projects/{id}/latency-test", post(latency_test))
        .route("/api/register", post(register))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/me", get(me))
        .route("/api/catalog", get(|| async { Json(catalog()) }))
        .route("/api/projects", get(list_projects).post(create_project))
        .route("/api/projects/{id}/opened", post(accounts::opened))
        .route("/api/samples", get(sample_library::organize))
        .route(
            "/api/samples/{id}",
            put(sample_library::organize_edit).delete(sample_library::delete),
        )
        .route(
            "/api/samples/{id}/audio",
            get(sample_library::organize_audio),
        )
        .route("/api/samples/{id}/impact", get(sample_library::impact))
        .route(
            "/api/admin/users",
            get(accounts::list).post(accounts::create),
        )
        .route(
            "/api/admin/users/{id}",
            put(accounts::update).delete(accounts::delete),
        )
        .route(
            "/api/admin/users/{id}/sessions",
            axum::routing::delete(accounts::revoke),
        )
        .route("/api/audio/config", get(settings::public_config))
        .route("/api/join", post(join))
        .route("/api/devices", get(devices))
        .route("/api/projects/{id}", get(get_project).put(update_project))
        .route(
            "/api/projects/{id}/save",
            get(save_status).post(save_project),
        )
        .route("/api/projects/{id}/parameter", put(parameter))
        .route("/api/projects/{id}/transport", post(transport))
        .route("/api/projects/{id}/invite", post(invite))
        .route("/api/projects/{id}/members", get(members))
        .route("/api/projects/{id}/audio", post(audio_enable))
        .route("/api/projects/{id}/events", get(websocket))
        .route(
            "/api/projects/{id}/media",
            post(media::offer).delete(media::disconnect),
        )
        .route("/api/projects/{id}/clip", post(clip))
        .route(
            "/api/projects/{id}/samples",
            get(sample_library::list)
                .post(samples::upload)
                .layer(axum::extract::DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        .route(
            "/api/projects/{id}/sample-library",
            get(sample_library::browse),
        )
        .route(
            "/api/projects/{id}/samples/{sample}",
            put(sample_library::edit),
        )
        .route(
            "/api/projects/{id}/samples/{sample}/add",
            post(sample_library::add),
        )
        .route(
            "/api/projects/{id}/samples/{sample}/audio",
            get(sample_library::audio),
        )
        .layer(axum::extract::DefaultBodyLimit::max(4 * 1024 * 1024))
        .fallback_service(ServeDir::new(&assets).not_found_service(ServeFile::new(
            std::path::Path::new(&assets).join("index.html"),
        )))
        .with_state(app);
    if let Some(session) = desktop_session {
        desktop::serve(router, desktop_app, session).await;
        return;
    }
    let address = bind::address(
        &std::env::var("PR0_BIND").unwrap_or_else(|_| {
            if tls_config.pair.is_some() {
                "0.0.0.0:443"
            } else {
                "0.0.0.0:80"
            }
            .into()
        }),
        std::env::var("PR0_HOST").ok().as_deref(),
        tls_config.port.as_deref(),
    )
    .expect("Bind address");
    println!(
        "pr0former listening on {}://{address}",
        if tls_config.pair.is_some() {
            "https"
        } else {
            "http"
        }
    );
    if let Some((cert, key)) = tls_config.pair {
        let _ = rustls::crypto::ring::default_provider().install_default();
        let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key)
            .await
            .expect("Load TLS certificate");
        let socket = tokio::net::lookup_host(&address)
            .await
            .expect("Resolve bind address")
            .next()
            .expect("Bind host resolved to no addresses");
        let http_address = bind::address(
            &address,
            None,
            Some(&std::env::var("PR0_HTTP_PORT").unwrap_or("80".into())),
        )
        .expect("HTTP redirect address");
        let listener = tokio::net::TcpListener::bind(&http_address).await.expect("Bind HTTP redirect listener (ports 80/443 may require OS permission; use PR0_HTTP_PORT/PR0_PORT for alternate ports)");
        println!("HTTP redirect listening on http://{http_address}");
        tokio::try_join!(
            axum_server::bind_rustls(socket, tls).serve(router.into_make_service()),
            async { axum::serve(listener, tls::redirects(socket.port())).await }
        )
        .expect("Serve HTTPS and HTTP redirect");
    } else {
        let listener = tokio::net::TcpListener::bind(address).await.unwrap();
        axum::serve(listener, router).await.unwrap();
    }
}

#[cfg(test)]
mod live_edit_tests {
    use super::*;
    #[test]
    fn existing_missing_instrument_does_not_block_unrelated_live_graph_edits() {
        let mut previous = pr0_core::demo_project("x".into(), "x".into(), Mode::Structured);
        previous.graph.nodes.retain(|n| n.id != "tone");
        previous
            .graph
            .edges
            .retain(|e| e.source != "tone" && e.target != "tone");
        // Older projects can retain a score route to an already deleted node.
        assert_eq!(previous.parts[0].instrument_node.as_deref(), Some("tone"));
        let mut added = previous.clone();
        let mut node = added.graph.nodes[0].clone();
        node.id = "new-clock".into();
        added.graph.nodes.push(node);
        assert!(validate_live_update(&previous, &added).is_ok());
        let mut deleted = added.clone();
        deleted.graph.nodes.retain(|n| n.id != "new-clock");
        assert!(validate_live_update(&added, &deleted).is_ok());
        assert_eq!(
            deleted.parts[0].instrument_node,
            previous.parts[0].instrument_node
        );
    }
    #[test]
    fn structured_live_edits_allow_graphs_and_score_but_protect_mode() {
        let previous = pr0_core::demo_project("x".into(), "x".into(), Mode::Structured);
        let mut next = previous.clone();
        next.graph.nodes[0].x += 10.;
        next.revision += 1;
        assert!(validate_live_update(&previous, &next).is_ok());
        next.parts[0].name = "Changed part".into();
        assert!(validate_live_update(&previous, &next).is_ok());
        let mut deleted = previous.clone();
        deleted.graph.nodes.retain(|n| n.id != "tone");
        deleted
            .graph
            .edges
            .retain(|e| e.source != "tone" && e.target != "tone");
        for part in &mut deleted.parts {
            if part.instrument_node.as_deref() == Some("tone") {
                part.instrument_node = None;
            }
        }
        assert!(validate_live_update(&previous, &deleted).is_ok());
        assert!(
            validate_live_update(&deleted, &previous).is_ok(),
            "undo can restore the deleted instrument route"
        );
        next = previous.clone();
        next.mode = Mode::Freeform;
        assert!(validate_live_update(&previous, &next).is_err());
    }
}
