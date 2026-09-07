mod audio;
mod bind;
mod media;
mod performance;
mod samples;
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
    db: Arc<Mutex<Connection>>,
    events: broadcast::Sender<Value>,
    engine: std::sync::mpsc::SyncSender<audio::Command>,
    active: Arc<Mutex<Option<String>>>,
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
            "SELECT user_id FROM sessions WHERE token=?1 AND expires>?2",
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
fn save_revision(app: &App, project: &mut Project) -> Api<()> {
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
    tx.execute(
        "INSERT INTO revisions(project_id,revision,body) VALUES(?1,?2,?3)",
        params![project.id, project.revision, body],
    )
    .map_err(internal)?;
    tx.commit().map_err(internal)?;
    Ok(())
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
            "SELECT id,password FROM users WHERE username=?1",
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
        Json(json!({"id":id,"username":c.username})),
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
    let username: String = app
        .db
        .lock()
        .unwrap()
        .query_row("SELECT username FROM users WHERE id=?1", [&id], |r| {
            r.get(0)
        })
        .map_err(internal)?;
    Ok(Json(json!({"id":id,"username":username})))
}
async fn status(State(app): State<App>) -> Json<Value> {
    let count: i64 = app
        .db
        .lock()
        .unwrap()
        .query_row("SELECT count(*) FROM users", [], |r| r.get(0))
        .unwrap_or(1);
    Json(
        json!({"bootstrap":count==0,"version":env!("CARGO_PKG_VERSION"),"active_project":*app.active.lock().unwrap(),"monitor_transport":"webrtc_opus"}),
    )
}
async fn list_projects(State(app): State<App>, headers: HeaderMap) -> Api<Json<Value>> {
    let id = user(&app, &headers)?;
    let db = app.db.lock().unwrap();
    let mut s=db.prepare("SELECT p.body,m.role FROM projects p JOIN members m ON m.project_id=p.id WHERE m.user_id=?1 ORDER BY p.rowid DESC").map_err(internal)?;
    let rows = s
        .query_map([id], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?))
        })
        .map_err(internal)?;
    let mut result = vec![];
    for row in rows {
        let (body, role) = row.map_err(internal)?;
        let p: Project = serde_json::from_str(&body).map_err(internal)?;
        result
            .push(json!({"id":p.id,"name":p.name,"mode":p.mode,"role":role,"revision":p.revision}));
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
    Ok(Json(json!({"project":load(&app,&id)?,"role":r})))
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
    if p.id != id {
        return Err(bad("Project ID mismatch"));
    }
    p.validate().map_err(bad)?;
    let active = app.active.lock().unwrap().as_deref() == Some(&id);
    let previous = load(&app, &id)?;
    if active && (previous.mode == Mode::Structured || p.mode != previous.mode) {
        return Err(Failure(
            StatusCode::CONFLICT,
            "Structured shows and mode changes require deactivation".into(),
        ));
    }
    let prepared = if active {
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
    save_revision(&app, &mut p)?;
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
    let mut p = load(&app, &id)?;
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
    let active = app.active.lock().unwrap().as_deref() == Some(&id);
    if active && meta.structural {
        return Err(bad("Deactivate before changing structural parameters"));
    }
    n.parameters.insert(c.parameter.clone(), c.value);
    p.validate().map_err(bad)?;
    // Serialize revisions before enqueueing; failed runtime delivery is reported explicitly.
    save_revision(&app, &mut p)?;
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
    bpm: Option<f64>,
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
    if c.action == "activate" {
        let project = load(&app, &id)?;
        let prepared_project = project.clone();
        let engine = tokio::task::spawn_blocking(move || samples::prepare(&prepared_project))
            .await
            .map_err(internal)?
            .map_err(bad)?;
        if load(&app, &id)?.revision != project.revision {
            return Err(Failure(
                StatusCode::CONFLICT,
                "Project changed during preparation".into(),
            ));
        }
        let mut active = app.active.lock().unwrap();
        if let Some(other) = active.as_ref() {
            if other != &id {
                return Err(bad("Another project owns the audio engine"));
            }
        }
        send(&app, audio::Command::Load(project, Box::new(engine)))?;
        *active = Some(id.clone());
        let _ = app.events.send(engine_status(&active));
    } else if c.action == "deactivate" {
        let mut active = app.active.lock().unwrap();
        if active.as_deref() != Some(&id) {
            return Err(bad("Project is not active"));
        }
        send(&app, audio::Command::Unload)?;
        *active = None;
        let _ = app.events.send(engine_status(&active));
    } else {
        if app.active.lock().unwrap().as_deref() != Some(&id) {
            return Err(bad("Activate this show first"));
        }
        match c.action.as_str() {
            "play" | "pause" | "stop" => send(&app, audio::Command::Transport(c.action))?,
            "tempo" => {
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
    user(&app, &headers)?;
    let (tx, rx) = oneshot::channel();
    send(&app, audio::Command::Devices(tx))?;
    Ok(Json(
        tokio::time::timeout(Duration::from_secs(5), rx)
            .await
            .map_err(internal)?
            .map_err(internal)?,
    ))
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
    if app.active.lock().unwrap().as_deref() != Some(&id) {
        return Err(bad("Activate this project first"));
    }
    send(
        &app,
        if c.input {
            audio::Command::Capture(c.enabled)
        } else {
            audio::Command::Hardware(c.enabled)
        },
    )?;
    Ok(Json(json!({"ok":true})))
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
    Ok(ws.on_upgrade(move |socket| stream(socket, app, id, u)))
}
// Call while holding the active-project lock to order status snapshots and changes.
fn engine_status(active: &Option<String>) -> Value {
    json!({"type":"engine_status","active_project":active,"server_time":audio::monotonic_ms()})
}

async fn send_project_snapshot(socket: &mut WebSocket, app: &App, id: &str, u: &str) -> bool {
    if role(app, id, u).is_err() {
        return false;
    }
    let Ok(project) = load(app, id) else {
        return false;
    };
    if socket
        .send(Message::Text(
            json!({"type":"project","project_id":id,"project":project})
                .to_string()
                .into(),
        ))
        .await
        .is_err()
    {
        return false;
    }
    let status = engine_status(&app.active.lock().unwrap());
    socket
        .send(Message::Text(status.to_string().into()))
        .await
        .is_ok()
}

async fn stream(mut socket: WebSocket, app: App, id: String, u: String) {
    // Subscribe before reading SQLite so concurrent commits are either in this
    // snapshot or queued below. Clients discard older/equal revisions.
    let mut events = app.events.subscribe();
    if !send_project_snapshot(&mut socket, &app, &id, &u).await {
        return;
    }
    let mut check = tokio::time::interval(Duration::from_secs(30));
    loop {
        tokio::select! {
            event=events.recv()=>match event{Ok(v)=>{if (v["type"]=="engine_status" || v.get("project_id").and_then(Value::as_str)==Some(&id))&&socket.send(Message::Text(v.to_string().into())).await.is_err(){break;}},Err(broadcast::error::RecvError::Lagged(_))=>{if !send_project_snapshot(&mut socket, &app, &id, &u).await {break;}},Err(_)=>break},
            msg=socket.recv()=>match msg{Some(Ok(Message::Text(text)))=>{if let Ok(v)=serde_json::from_str::<Value>(&text){if v["type"]=="ping"{let _=socket.send(Message::Text(json!({"type":"pong","client_time":v["client_time"],"server_time":audio::monotonic_ms()}).to_string().into())).await;}}},Some(Ok(Message::Close(_)))|None|Some(Err(_))=>break,_=>{}},
            _=check.tick()=>{if role(&app,&id,&u).is_err(){break;}}
        }
    }
}

#[tokio::main]
async fn main() {
    let _ = rustls::crypto::ring::default_provider().install_default();
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();
    let directory = std::env::var("PR0_DATA").unwrap_or("data".into());
    std::fs::create_dir_all(&directory).expect("Create data directory");
    let db = Connection::open(format!("{directory}/pr0former.sqlite")).expect("Open database");
    db.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;
        CREATE TABLE IF NOT EXISTS users(id TEXT PRIMARY KEY,username TEXT UNIQUE NOT NULL,password TEXT NOT NULL);
        CREATE TABLE IF NOT EXISTS sessions(token TEXT PRIMARY KEY,user_id TEXT REFERENCES users(id),expires INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS projects(id TEXT PRIMARY KEY,body TEXT NOT NULL,revision INTEGER NOT NULL);
        CREATE TABLE IF NOT EXISTS members(project_id TEXT REFERENCES projects(id),user_id TEXT REFERENCES users(id),role TEXT NOT NULL,PRIMARY KEY(project_id,user_id));
        CREATE TABLE IF NOT EXISTS revisions(project_id TEXT REFERENCES projects(id),revision INTEGER,body TEXT NOT NULL,PRIMARY KEY(project_id,revision));
        CREATE TABLE IF NOT EXISTS invites(token TEXT PRIMARY KEY,project_id TEXT REFERENCES projects(id),role TEXT,expires INTEGER,used INTEGER);").expect("Database migration");
    let (events, _) = broadcast::channel(128);
    let media = Arc::new(media::Media::new());
    let engine = audio::start(events.clone(), media.audio.clone());
    let cert = std::env::var("PR0_TLS_CERT").ok();
    let app = App {
        db: Arc::new(Mutex::new(db)),
        events,
        engine,
        active: Arc::new(Mutex::new(None)),
        secure: cert.is_some(),
        media,
    };
    let router = Router::new()
        .route("/api/status", get(status))
        .route("/api/register", post(register))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/me", get(me))
        .route("/api/catalog", get(|| async { Json(catalog()) }))
        .route("/api/projects", get(list_projects).post(create_project))
        .route("/api/join", post(join))
        .route("/api/devices", get(devices))
        .route("/api/projects/{id}", get(get_project).put(update_project))
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
            post(samples::upload).layer(axum::extract::DefaultBodyLimit::max(64 * 1024 * 1024)),
        )
        .layer(axum::extract::DefaultBodyLimit::max(4 * 1024 * 1024))
        .fallback_service(
            ServeDir::new("web/dist").not_found_service(ServeFile::new("web/dist/index.html")),
        )
        .with_state(app);
    let address = bind::address(
        &std::env::var("PR0_BIND").unwrap_or("0.0.0.0:4000".into()),
        std::env::var("PR0_HOST").ok().as_deref(),
        std::env::var("PR0_PORT").ok().as_deref(),
    )
    .expect("Bind address");
    println!(
        "pr0former listening on {}://{address}",
        if cert.is_some() { "https" } else { "http" }
    );
    if let Some(cert) = cert {
        let key = std::env::var("PR0_TLS_KEY").expect("PR0_TLS_KEY required");
        let _ = rustls::crypto::ring::default_provider().install_default();
        let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(cert, key)
            .await
            .expect("Load TLS certificate");
        let socket = tokio::net::lookup_host(&address)
            .await
            .expect("Resolve bind address")
            .next()
            .expect("Bind host resolved to no addresses");
        axum_server::bind_rustls(socket, tls)
            .serve(router.into_make_service())
            .await
            .unwrap();
    } else {
        let listener = tokio::net::TcpListener::bind(address).await.unwrap();
        axum::serve(listener, router).await.unwrap();
    }
}
