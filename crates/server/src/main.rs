mod accounts;
mod audio;
mod bind;
mod build_info;
mod desktop;
mod discovery;
mod hardware_meter;
mod hosting;
mod local_midi;
mod conductor_midi;
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
        Multipart, Path, Query, State, WebSocketUpgrade,
        ws::{Message, WebSocket},
    },
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use pr0_core::{Mode, Project, catalog, demo_project};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{broadcast, oneshot};
use tower_http::services::{ServeDir, ServeFile};
use uuid::Uuid;

/// A broadcast message serialized once by its producer. Socket tasks send the
/// prepared text instead of re-serializing the same value for every client.
#[derive(Clone)]
pub struct Event {
    pub value: Arc<Value>,
    pub text: Arc<str>,
    /// The same message without its `visualizations` payload, when it has one.
    pub stripped: Option<Arc<str>>,
}
impl From<Value> for Event {
    fn from(mut value: Value) -> Self {
        let text: Arc<str> = value.to_string().into();
        let stripped = match value.get("visualizations") {
            Some(Value::Object(map)) if !map.is_empty() => {
                if let Some(object) = value.as_object_mut() {
                    object.remove("visualizations");
                }
                Some(Arc::<str>::from(value.to_string()))
            }
            _ => None,
        };
        Self {
            value: Arc::new(value),
            text,
            stripped,
        }
    }
}
#[derive(Clone)]
struct App {
    conductor_midi: std::sync::mpsc::SyncSender<conductor_midi::Message>,
    presence: Arc<Mutex<presence::Presence>>,
    resources: Arc<Mutex<resources::Stats>>,
    osc: Arc<osc::Runtime>,
    db: Arc<Mutex<Connection>>,
    events: broadcast::Sender<Event>,
    engine: std::sync::mpsc::SyncSender<audio::Command>,
    active: Arc<Mutex<Option<String>>>,
    performance: Arc<Mutex<Option<String>>>,
    graph: Arc<Mutex<Option<String>>>,
    setup: Arc<tokio::sync::Mutex<()>>,
    logs: Arc<settings::Logs>,
    secure: bool,
    desktop_session: Option<String>,
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
fn session_token(headers: &HeaderMap) -> &str {
    let cookie = headers
        .get(header::COOKIE)
        .and_then(|x| x.to_str().ok())
        .unwrap_or("");
    cookie
        .split(';')
        .filter_map(|x| x.trim().split_once('='))
        .find(|(k, _)| *k == "pr0_session")
        .map(|(_, v)| v)
        .unwrap_or("")
}
fn is_desktop_session(app: &App, headers: &HeaderMap) -> bool {
    app.desktop_session.as_deref().is_some_and(|token| token == session_token(headers))
}
fn user(app: &App, headers: &HeaderMap) -> Api<String> {
    let token = session_token(headers);
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
#[derive(Clone, Debug, Serialize, Deserialize)]
struct LoginTitle {
    first: String,
    second: String,
}
fn default_login_titles() -> Vec<LoginTitle> {
    let mut titles = vec![LoginTitle {
        first: "Insert pithy title here".into(),
        second: "Put something funny here too".into(),
    }];
    titles.extend(
        [
            "Stop, Collaborate and Listen",
            "F*ck it, we'll do it live!",
            "A very musical hampster wheel",
            "Science b!tches",
            "ERROR....nah JK",
            "This is AI slop",
            "Injecting the Raccoons Now",
            "Now with 80% more cheese",
            "Have you considered how Carl feels?",
            "Illegal in many states",
            "She turned me into a newt!",
            "Welcome back Mr. Wick",
            "Turning the frogs gay",
            "Your bit drift is showing",
            "you forgot to return your Amazon purchase",
            "Saints be praised!",
            "TETSUOOOOOOO",
            "It's over 9000!",
        ]
        .into_iter()
        .map(|first| LoginTitle {
            first: first.into(),
            second: "Live Electroacoustic Performance Platform".into(),
        }),
    );
    titles
}
async fn login_titles(State(app): State<App>) -> Api<Json<Vec<LoginTitle>>> {
    let db = app.db.lock().unwrap();
    let mut stmt = db
        .prepare("SELECT first,second FROM login_titles ORDER BY rowid")
        .map_err(internal)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(LoginTitle {
                first: r.get(0)?,
                second: r.get(1)?,
            })
        })
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    Ok(Json(if rows.is_empty() {
        default_login_titles()
    } else {
        rows
    }))
}
async fn save_login_titles(
    State(app): State<App>,
    headers: HeaderMap,
    Json(titles): Json<Vec<LoginTitle>>,
) -> Api<Json<Vec<LoginTitle>>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    let admin: bool = app
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT is_admin FROM user_profiles WHERE user_id=?1",
            [&u],
            |r| r.get(0),
        )
        .map_err(|_| bad("Administrator required"))?;
    if !admin || titles.len() > 100 {
        return Err(Failure(
            StatusCode::FORBIDDEN,
            "Administrator required".into(),
        ));
    }
    if titles.iter().any(|t| {
        t.first.trim().is_empty()
            || t.first.len() > 200
            || t.second.trim().is_empty()
            || t.second.len() > 200
    }) {
        return Err(bad("Title lines must be 1–200 characters"));
    }
    let mut db = app.db.lock().unwrap();
    let tx = db.transaction().map_err(internal)?;
    tx.execute("DELETE FROM login_titles", [])
        .map_err(internal)?;
    for t in &titles {
        tx.execute(
            "INSERT INTO login_titles(first,second) VALUES(?1,?2)",
            [&t.first, &t.second],
        )
        .map_err(internal)?;
    }
    tx.commit().map_err(internal)?;
    Ok(Json(titles))
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
fn can_conduct(project: &Project, user: &str, role: &str) -> bool {
    project
        .conductor
        .as_deref()
        .map(|conductor| conductor == user || role == "owner")
        .unwrap_or_else(|| matches!(role, "owner" | "conductor"))
}

fn validate_conductor(app: &App, project: &Project) -> Api<()> {
    let Some(conductor) = project.conductor.as_deref() else {
        return Ok(());
    };
    let member: bool = app
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM members WHERE project_id=?1 AND user_id=?2)",
            params![project.id, conductor],
            |row| row.get(0),
        )
        .map_err(internal)?;
    if !member {
        return Err(bad("The designated conductor must be an ensemble member"));
    }
    if project
        .parts
        .iter()
        .any(|part| part.performer.as_deref() == Some(conductor))
    {
        return Err(bad(
            "The designated conductor cannot be assigned performer parts",
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
        .send(json!({"type":"project_save","project_id":id,"save":status}).into());
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
        .send(json!({"type":"project","project_id":p.id,"project":p}).into());
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
    let password = c.password.clone();
    let hash = tokio::task::spawn_blocking(move || {
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
    })
    .await
    .map_err(internal)?
    .map_err(internal)?;
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
/// A valid Argon2 hash of a throwaway password, used to keep login timing
/// independent of whether a username exists.
fn dummy_password_hash() -> &'static str {
    static HASH: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    HASH.get_or_init(|| {
        let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes()).expect("salt");
        Argon2::default()
            .hash_password(b"pr0former-dummy-password", &salt)
            .expect("hash")
            .to_string()
    })
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
    // Unknown usernames verify against a fixed hash so response time does not
    // reveal which accounts exist. Argon2 never runs on the async executor.
    let (id, hash) = match record {
        Some((id, hash)) => (Some(id), hash),
        None => (None, dummy_password_hash().to_owned()),
    };
    let password = c.password;
    let verified = tokio::task::spawn_blocking(move || {
        PasswordHash::new(&hash)
            .map(|parsed| {
                Argon2::default()
                    .verify_password(password.as_bytes(), &parsed)
                    .is_ok()
            })
            .unwrap_or(false)
    })
    .await
    .map_err(internal)?;
    let id = match (id, verified) {
        (Some(id), true) => id,
        _ => {
            return Err(Failure(
                StatusCode::UNAUTHORIZED,
                "Invalid credentials".into(),
            ));
        }
    };
    let token = uid() + &uid();
    {
        let db = app.db.lock().unwrap();
        // Expired rows are swept opportunistically; the lookup also filters on expiry.
        let _ = db.execute("DELETE FROM sessions WHERE expires<=?1", [now()]);
        db.execute(
            "INSERT INTO sessions(token,user_id,expires) VALUES(?1,?2,?3)",
            params![token, id, now() + 86400],
        )
        .map_err(internal)?;
    }
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
    if is_desktop_session(&app, &headers) {
        return Err(Failure(StatusCode::FORBIDDEN, "The bundled admin session cannot sign out. Quit the desktop app to end this session.".into()));
    }
    let id = user(&app, &headers)?;
    app.db
        .lock()
        .unwrap()
        .execute("DELETE FROM sessions WHERE user_id=?1 AND token!=?2", params![id, app.desktop_session.as_deref().unwrap_or("")])
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
    let mut profile = accounts::profile(&app.db.lock().unwrap(), &id)?;
    profile["is_desktop_session"] = json!(is_desktop_session(&app, &headers));
    Ok(Json(profile))
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
        json!({"bootstrap":count==0,"version":env!("CARGO_PKG_VERSION"),"build":build_info::json(),"active_project":ownership.value["active_project"],"graph_project":ownership.value["graph_project"],"monitor_transport":"webrtc_opus"}),
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
async fn list_revisions(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<Json<Vec<revisions::RevisionEntry>>> {
    let u = user(&app, &headers)?;
    role(&app, &id, &u)?;
    let current = load(&app, &id)?.revision;
    let entries = revisions::list(&app.db.lock().unwrap(), &id, current).map_err(internal)?;
    Ok(Json(entries))
}
async fn get_revision(
    State(app): State<App>,
    headers: HeaderMap,
    Path((id, revision)): Path<(String, u64)>,
) -> Api<Json<Value>> {
    let u = user(&app, &headers)?;
    role(&app, &id, &u)?;
    let latest = load(&app, &id)?.revision;
    let db = app.db.lock().unwrap();
    let body = revisions::body(&db, &id, revision).map_err(|_| bad("Revision not found"))?;
    let mut project: Project = serde_json::from_str(&body).map_err(internal)?;
    project.revision = latest;
    Ok(Json(
        json!({"project": project, "source_revision": revision, "latest_revision": latest}),
    ))
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
    let membership=role(&app,&id,&u)?;
    can_edit(&membership)?;
    let _guard = app.setup.lock().await;
    if p.id != id {
        return Err(bad("Project ID mismatch"));
    }
    if app.performance.lock().unwrap().as_deref() == Some(&id) {
        return Err(bad("Exit performance mode before editing"));
    }
    let previous = load(&app, &id)?;
    if p.local_audio_assignments != previous.local_audio_assignments && !matches!(membership.as_str(),"owner"|"editor") {return Err(Failure(StatusCode::FORBIDDEN,"Only owners and editors can assign local audio inputs".into()));}
    for target in p.local_audio_assignments.values() { role(&app,&id,target).map_err(|_|bad("Assign local audio inputs to ensemble members"))?; }
    sample_library::assign_roots(&app.db.lock().unwrap(), &previous, &mut p)?;
    p.validate().map_err(bad)?;
    samples::validate_choices(&p).map_err(bad)?;
    validate_conductor(&app, &p)?;
    let active = app.active.lock().unwrap().as_deref() == Some(&id);
    settings::validate_route_changes(&previous, &p, &settings::read()).map_err(bad)?;
    let previous_flat = previous.graph.flatten().map_err(bad)?;
    for node in &p.graph.nodes {
        if pr0_core::named_route(&node.kind)
            && previous
                .graph
                .nodes
                .iter()
                .any(|old| old.id == node.id && old.control_value != node.control_value)
            && previous_flat
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
    app.media.reconcile_inputs(&p).await;
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
    /// Unsaved drag preview; routing and velocity still come from the saved note.
    pitch: Option<u8>,
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
    if request.pitch.is_some_and(|pitch| pitch > 127) {
        return Err(bad("MIDI pitch must be 0–127"));
    }
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
                pitch: request.pitch.unwrap_or(note.pitch),
                velocity: note.velocity,
            },
        )?;
    }
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
struct PianoNote {
    node: String,
    pitch: Option<u8>,
    velocity: Option<u8>,
    bend: Option<u16>,
}
#[derive(Deserialize)]
struct ControllerEdit {
    #[serde(default)]
    cancel: bool,
    node: String,
    index: usize,
    value: Option<f64>,
}
async fn controller_edit(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(edit): Json<ControllerEdit>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    can_edit(&role(&app, &id, &u)?)?;
    let _guard = app.setup.lock().await;
    let p = load(&app, &id)?;
    let node = p
        .graph
        .nodes
        .iter()
        .find(|n| n.id == edit.node && matches!(n.kind.as_str(), "knobs" | "sliders"))
        .ok_or_else(|| bad("Controller node missing"))?;
    if edit.index >= node.parameters.get("count").copied().unwrap_or(4.) as usize
        || edit.value.is_some_and(|v| !v.is_finite())
    {
        return Err(bad("Invalid controller value or index"));
    }
    if !edit.cancel
        && edit.value.is_some()
        && node.parameters.get(&format!("channel_{}", edit.index + 1)) == Some(&0.)
    {
        return Err(bad(
            "Assign this knob to a MIDI channel before changing its value",
        ));
    }
    if !edit.cancel
        && node.kind == "sliders"
        && p.graph
            .edges
            .iter()
            .any(|e| e.target == edit.node && e.target_port == format!("slider_{}", edit.index + 1))
    {
        return Err(bad("Connected slider is read-only"));
    }
    if app.graph.lock().unwrap().as_deref() != Some(&id) {
        return Err(bad("Enable this project's audio engine first"));
    }
    send(
        &app,
        audio::Command::Controller {
            project: id,
            node: edit.node,
            index: edit.index,
            value: edit.value,
            cancel: edit.cancel,
        },
    )?;
    Ok(Json(json!({"ok":true})))
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
    let message = match (note.pitch, note.velocity, note.bend) {
        (Some(pitch), Some(velocity), None) if pitch <= 127 && velocity <= 127 => {
            pr0_core::midi::Message {
                status: if velocity == 0 { 0x80 } else { 0x90 },
                data1: pitch,
                data2: velocity,
            }
        }
        (None, None, Some(bend)) if bend <= 16383 => pr0_core::midi::Message {
            status: 0xe0,
            data1: (bend & 127) as u8,
            data2: (bend >> 7) as u8,
        },
        _ => return Err(bad("Invalid MIDI note or pitch bend")),
    };
    if !p.graph.nodes.iter().any(|n| {
        n.id == note.node && (n.kind == "piano" || (n.kind == "drum_pads" && note.bend.is_none()))
    }) {
        return Err(bad("Piano or drum pad node missing"));
    }
    if app.graph.lock().unwrap().as_deref() != Some(&id) {
        return Err(bad(
            "Enable this project’s audio engine before playing piano keys",
        ));
    }
    send(
        &app,
        audio::Command::Piano {
            project: id.clone(),
            node: note.node,
            message,
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
    let membership=role(&app,&id,&u)?;
    let _guard = app.setup.lock().await;
    let mut p = load(&app, &id)?;
    let owns_input = c.parameter == "mute"
        && p.graph.nodes.iter().any(|n| n.id == c.node && n.kind == "browser_input")
        && p.local_audio_assignments.get(&c.node)==Some(&u);
    if c.parameter=="mute" && p.graph.nodes.iter().any(|n|n.id==c.node&&n.kind=="browser_input") { if !owns_input {return Err(Failure(StatusCode::FORBIDDEN,"Local audio input is not assigned to you".into()));} } else { can_edit(&membership)?; }
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
    #[serde(default)]
    beat: Option<f64>,
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
    let authority = load(&app, &id)?;
    if !can_conduct(&authority, &u, &r) {
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
    if matches!(c.action.as_str(), "play" | "repeat")
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
            "seek" => {
                let beat = c.beat.ok_or_else(|| bad("Beat required"))?;
                if !beat.is_finite() || beat < 0.0 {
                    return Err(bad("Beat must be a finite non-negative number"));
                }
                send(&app, audio::Command::Seek(beat))?;
            }
            "play" | "repeat" | "pause" | "stop" => send(
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
    #[serde(default)]
    repeat: bool,
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
    if !can_conduct(&p, &u, &r)
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
            repeat: c.playing && c.repeat,
        },
    )?;
    Ok(Json(json!({"ok":true})))
}

#[derive(Deserialize)]
struct CueAction {
    action: String,
    #[serde(default)]
    request_id: Option<String>,
    #[serde(default)]
    parts: Vec<String>,
    #[serde(default)]
    repeat: bool,
    #[serde(default)]
    count_in_pulses: Option<u8>,
    #[serde(default)]
    value: Option<u8>,
}

async fn cue(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(c): Json<CueAction>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    let r = role(&app, &id, &u)?;
    let p = load(&app, &id)?;
    if p.mode != Mode::Conducted {
        return Err(bad("Cue groups are available in conducted projects"));
    }
    if !can_conduct(&p, &u, &r) && !(r == "editor" && app.performance.lock().unwrap().as_deref() != Some(&id)) {
        return Err(Failure(
            StatusCode::FORBIDDEN,
            "Conductor authority required".into(),
        ));
    }
    if app.active.lock().unwrap().as_deref() != Some(&id) {
        return Err(bad("Activate this project first"));
    }
    if c.parts.len() > 32
        || c.request_id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 128)
        || c.parts
            .iter()
            .any(|part| !p.parts.iter().any(|candidate| &candidate.id == part))
    {
        return Err(bad("Cue contains an invalid part"));
    }
    let count_in = c.count_in_pulses.unwrap_or(p.conducted.count_in_pulses);
    if count_in > 32 {
        return Err(bad("Count-in must be 0–32 pulses"));
    }
    match c.action.as_str() {
        "arm" | "unarm" => {
            if c.parts.is_empty() {
                return Err(bad("Select at least one part to arm"));
            }
            if c.action == "arm" {
                let performers: std::collections::BTreeSet<_> = p
                    .parts
                    .iter()
                    .filter(|part| c.parts.contains(&part.id))
                    .filter_map(|part| part.performer.as_ref())
                    .collect();
                let conflicts = p
                    .parts
                    .iter()
                    .filter(|part| !c.parts.contains(&part.id))
                    .filter(|part| {
                        part.performer
                            .as_ref()
                            .is_some_and(|id| performers.contains(id))
                    })
                    .map(|part| part.id.clone())
                    .collect::<Vec<_>>();
                if !conflicts.is_empty() {
                    send(
                        &app,
                        audio::Command::Arm {
                            parts: conflicts,
                            armed: false,
                        },
                    )?;
                }
            }
            send(
                &app,
                audio::Command::Arm {
                    parts: c.parts,
                    armed: c.action == "arm",
                },
            )?;
        }
        "start" | "stop" => send(
            &app,
            audio::Command::Cue {
                request_id: c.request_id,
                parts: c.parts,
                playing: c.action == "start",
                repeat: c.action == "start" && c.repeat,
                count_in_pulses: if c.action == "start" { count_in } else { 0 },
            },
        )?,
        "dynamic" => {
            if c.value.is_some_and(|value| value > 127) {
                return Err(bad("Dynamic value must be 0–127"));
            }
            send(
                &app,
                audio::Command::Dynamics {
                    parts: c.parts,
                    value: c.value,
                },
            )?;
        }
        _ => return Err(bad("Unknown cue action")),
    }
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
    let project = load(&app, &id)?;
    let db = app.db.lock().unwrap();
    let mut s=db.prepare("SELECT u.id,u.username,m.role,p.body,COALESCE(a.revision,0) FROM members m JOIN users u ON u.id=m.user_id JOIN user_profiles p ON p.user_id=u.id LEFT JOIN user_avatars a ON a.user_id=u.id WHERE m.project_id=?1 ORDER BY CASE m.role WHEN 'owner' THEN 0 WHEN 'conductor' THEN 1 WHEN 'editor' THEN 2 ELSE 3 END,u.username").map_err(internal)?;
    let rows=s.query_map([id],|r|{let user_id:String=r.get(0)?;let body:String=r.get(3)?;let fields:Value=serde_json::from_str(&body).unwrap_or(json!({}));Ok(json!({"id":user_id,"username":r.get::<_,String>(1)?,"role":r.get::<_,String>(2)?,"first_name":fields["first_name"],"last_name":fields["last_name"],"organization":fields["organization"],"avatar_revision":r.get::<_,u64>(4)?,"assigned_parts":project.parts.iter().filter(|part|part.performer.as_deref()==Some(&user_id)).count()}))}).map_err(internal)?.collect::<Result<Vec<_>,_>>().map_err(internal)?;
    Ok(Json(json!(rows)))
}
#[derive(Deserialize)]
struct MemberEdit {
    user_id: String,
    role: String,
}
async fn add_member(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(edit): Json<MemberEdit>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let actor = user(&app, &headers)?;
    if role(&app, &id, &actor)? != "owner" {
        return Err(Failure(
            StatusCode::FORBIDDEN,
            "Owner access required".into(),
        ));
    }
    if !matches!(edit.role.as_str(), "performer" | "editor" | "conductor") {
        return Err(bad("Invalid member role"));
    }
    let db = app.db.lock().unwrap();
    let count: i64 = db
        .query_row(
            "SELECT count(*) FROM members WHERE project_id=?1",
            [&id],
            |r| r.get(0),
        )
        .map_err(internal)?;
    if count >= 32 {
        return Err(bad("This project already has 32 members"));
    }
    let exists: bool = db
        .query_row(
            "SELECT enabled AND NOT deleted FROM user_profiles WHERE user_id=?1",
            [&edit.user_id],
            |r| r.get(0),
        )
        .unwrap_or(false);
    if !exists {
        return Err(bad("User unavailable"));
    }
    db.execute(
        "INSERT INTO members(project_id,user_id,role) VALUES(?1,?2,?3)",
        params![id, edit.user_id, edit.role],
    )
    .map_err(|_| bad("User is already an ensemble member"))?;
    drop(db);
    let _ = app
        .events
        .send(json!({"type":"members_changed","project_id":id}).into());
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
struct UserSearch {
    q: Option<String>,
}
async fn member_candidates(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Query(query): Query<UserSearch>,
) -> Api<Json<Value>> {
    let actor = user(&app, &headers)?;
    if role(&app, &id, &actor)? != "owner" {
        return Err(Failure(
            StatusCode::FORBIDDEN,
            "Owner access required".into(),
        ));
    }
    let pattern = format!("%{}%", query.q.unwrap_or_default().trim().to_lowercase());
    let db = app.db.lock().unwrap();
    let mut statement=db.prepare("SELECT u.id,u.username,p.body,COALESCE(a.revision,0) FROM users u JOIN user_profiles p ON p.user_id=u.id LEFT JOIN user_avatars a ON a.user_id=u.id WHERE p.enabled=1 AND p.deleted=0 AND lower(u.username) LIKE ?2 AND NOT EXISTS(SELECT 1 FROM members m WHERE m.project_id=?1 AND m.user_id=u.id) ORDER BY u.username LIMIT 50").map_err(internal)?;
    let rows=statement.query_map(params![id,pattern],|r|{let body:String=r.get(2)?;let fields:Value=serde_json::from_str(&body).unwrap_or(json!({}));Ok(json!({"id":r.get::<_,String>(0)?,"username":r.get::<_,String>(1)?,"first_name":fields["first_name"],"last_name":fields["last_name"],"organization":fields["organization"],"avatar_revision":r.get::<_,u64>(3)?}))}).map_err(internal)?.collect::<Result<Vec<_>,_>>().map_err(internal)?;
    Ok(Json(json!(rows)))
}
fn unassign_member_parts(project: &mut Project, user_id: &str) -> usize {
    let before=project.local_audio_assignments.len();
    project.local_audio_assignments.retain(|_,user|user!=user_id);
    project.parts.iter_mut().fold(before-project.local_audio_assignments.len(), |count, part| {
        if part.performer.as_deref() == Some(user_id) {
            part.performer = None;
            count + 1
        } else {
            count
        }
    })
}
async fn remove_member(
    State(app): State<App>,
    headers: HeaderMap,
    Path((id, target)): Path<(String, String)>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let actor = user(&app, &headers)?;
    if role(&app, &id, &actor)? != "owner" {
        return Err(Failure(
            StatusCode::FORBIDDEN,
            "Owner access required".into(),
        ));
    }
    let _guard = app.setup.lock().await;
    if app.performance.lock().unwrap().as_deref() == Some(&id) {
        return Err(bad("Exit performance mode before changing the ensemble"));
    }
    let target_role = role(&app, &id, &target)?;
    if target_role == "owner" {
        return Err(bad("Project owners cannot be removed"));
    }
    let mut project = load(&app, &id)?;
    let changed = unassign_member_parts(&mut project, &target);
    let conductor_changed = project.conductor.as_deref() == Some(&target);
    if conductor_changed {
        project.conductor = None;
    }
    let project_changed = changed > 0 || conductor_changed;
    let prepared = if project_changed && app.graph.lock().unwrap().as_deref() == Some(&id) {
        let next = project.clone();
        Some(
            tokio::task::spawn_blocking(move || samples::prepare(&next))
                .await
                .map_err(internal)?
                .map_err(bad)?,
        )
    } else {
        None
    };
    {
        let mut db = app.db.lock().unwrap();
        let tx = db.transaction().map_err(internal)?;
        if project_changed {
            let previous = project.revision;
            project.revision += 1;
            let body = serde_json::to_string(&project).map_err(internal)?;
            let updated = tx
                .execute(
                    "UPDATE projects SET body=?1,revision=?2 WHERE id=?3 AND revision=?4",
                    params![body, project.revision, id, previous],
                )
                .map_err(internal)?;
            if updated == 0 {
                return Err(Failure(
                    StatusCode::CONFLICT,
                    "Project changed. Reload before removing this member.".into(),
                ));
            }
            revisions::schedule(&tx, &id, now()).map_err(internal)?;
        }
        tx.execute(
            "DELETE FROM members WHERE project_id=?1 AND user_id=?2",
            params![id, target],
        )
        .map_err(internal)?;
        tx.execute(
            "DELETE FROM project_recents WHERE project_id=?1 AND user_id=?2",
            params![id, target],
        )
        .map_err(internal)?;
        tx.commit().map_err(internal)?;
    }
    if let Some(engine) = prepared {
        send(
            &app,
            audio::Command::Replace {
                project: project.clone(),
                engine: Box::new(engine),
            },
        )?;
    }
    if project_changed {
        publish(&app, &project);
    }
    app.media.close_project_user(&id, &target).await;
    let _ = app
        .events
        .send(json!({"type":"members_changed","project_id":id}).into());
    Ok(Json(json!({"project":project,"unassigned_parts":changed})))
}
async fn user_avatar(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<Response> {
    let u = user(&app, &headers)?;
    let bytes: Vec<u8> = {
        let db = app.db.lock().unwrap();
        // Avatars are visible to the user themselves and to members of a shared project.
        let shared: i64 = db
            .query_row(
                "SELECT count(*) FROM members a JOIN members b ON a.project_id=b.project_id WHERE a.user_id=?1 AND b.user_id=?2",
                params![u, id],
                |r| r.get(0),
            )
            .map_err(internal)?;
        if u != id && shared == 0 {
            return Err(Failure(StatusCode::NOT_FOUND, "Avatar not found".into()));
        }
        db.query_row(
            "SELECT body FROM user_avatars WHERE user_id=?1",
            [id],
            |r| r.get(0),
        )
        .map_err(|_| Failure(StatusCode::NOT_FOUND, "Avatar not found".into()))?
    };
    Ok((
        [
            (header::CONTENT_TYPE, "image/png"),
            (
                header::CACHE_CONTROL,
                "private, max-age=31536000, immutable",
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        ],
        bytes,
    )
        .into_response())
}
fn crop_avatar(bytes: &[u8]) -> Result<Vec<u8>, String> {
    use image::GenericImageView;
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|_| "Choose a PNG, JPEG, or WebP image".to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(8192);
    limits.max_image_height = Some(8192);
    limits.max_alloc = Some(128 * 1024 * 1024);
    reader.limits(limits);
    let source = reader
        .decode()
        .map_err(|_| "Choose a PNG, JPEG, or WebP image".to_string())?;
    let (width, height) = source.dimensions();
    if width == 0 || height == 0 || width > 8192 || height > 8192 {
        return Err("Avatar dimensions must be between 1 and 8192 pixels".into());
    }
    let side = width.min(height);
    let square = source
        .crop_imm((width - side) / 2, (height - side) / 2, side, side)
        .resize_exact(256, 256, image::imageops::FilterType::Lanczos3);
    let mut cursor = std::io::Cursor::new(Vec::new());
    square
        .write_to(&mut cursor, image::ImageFormat::Png)
        .map_err(|_| "Could not process avatar".to_string())?;
    Ok(cursor.into_inner())
}
async fn upload_user_avatar(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    mut form: Multipart,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let actor = user(&app, &headers)?;
    if actor != id {
        return Err(Failure(
            StatusCode::FORBIDDEN,
            "You can only change your own avatar".into(),
        ));
    }
    let field = form
        .next_field()
        .await
        .map_err(bad)?
        .ok_or_else(|| bad("Select an image"))?;
    let bytes = field.bytes().await.map_err(bad)?;
    if bytes.len() > 4 * 1024 * 1024 {
        return Err(bad("Avatar images must be smaller than 4 MB"));
    }
    let png = tokio::task::spawn_blocking(move || crop_avatar(&bytes))
        .await
        .map_err(internal)?
        .map_err(bad)?;
    let db = app.db.lock().unwrap();
    db.execute("INSERT INTO user_avatars(user_id,body,revision) VALUES(?1,?2,1) ON CONFLICT(user_id) DO UPDATE SET body=excluded.body,revision=user_avatars.revision+1",params![id,png]).map_err(internal)?;
    let revision: u64 = db
        .query_row(
            "SELECT revision FROM user_avatars WHERE user_id=?1",
            [&id],
            |r| r.get(0),
        )
        .map_err(internal)?;
    let projects = db
        .prepare("SELECT project_id FROM members WHERE user_id=?1")
        .map_err(internal)?
        .query_map([&id], |r| r.get::<_, String>(0))
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    drop(db);
    for project_id in projects {
        let _ = app
            .events
            .send(json!({"type":"members_changed","project_id":project_id,"user_id":id}).into());
    }
    Ok(Json(json!({"revision":revision})))
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
    // Client frames are small JSON commands; cap them well below tungstenite defaults.
    Ok(ws
        .max_message_size(64 * 1024)
        .max_frame_size(64 * 1024)
        .on_upgrade(move |socket| stream(socket, app, id, u, headers)))
}
// Snapshot both ownership states; graph processing and show transport are independent.
fn engine_status(app: &App) -> Event {
    let graph = app.graph.lock().unwrap();
    let active = app.active.lock().unwrap();
    json!({"type":"engine_status","performance_project":*app.performance.lock().unwrap(),"active_project":*active,"graph_project":*graph,"server_time":audio::monotonic_ms()}).into()
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
    socket_text(socket, status.text.to_string()).await.is_ok()
}

fn browser_midi_message(value: &Value) -> Option<pr0_core::midi::Message> {
    let data = value.get("data")?.as_array()?;
    let status = u8::try_from(data.first()?.as_u64()?).ok()?;
    let short = matches!(status >> 4, 12 | 13);
    if data.len() != 3 && !(short && data.len() == 2) {
        return None;
    }
    let data1 = u8::try_from(data[1].as_u64()?).ok()?;
    let data2 = if data.len() == 2 {
        0
    } else {
        u8::try_from(data[2].as_u64()?).ok()?
    };
    ((0x80..=0xef).contains(&status) && data1 <= 127 && data2 <= 127).then_some(
        pr0_core::midi::Message {
            status,
            data1,
            data2,
        },
    )
}

#[cfg(test)]
mod browser_midi_tests {
    use super::*;

    #[test]
    fn browser_midi_accepts_only_bounded_channel_messages() {
        let note = browser_midi_message(&json!({"data":[0x90,60,100]})).unwrap();
        assert_eq!((note.status, note.data1, note.data2), (0x90, 60, 100));
        assert!(browser_midi_message(&json!({"data":[0xf0,1,2]})).is_none());
        assert!(browser_midi_message(&json!({"data":[0x90,128,2]})).is_none());
        assert!(browser_midi_message(&json!({"data":[0x90,1]})).is_none());
    }

    #[test]
    fn explicit_conductor_replaces_legacy_role_authority_but_owner_can_recover() {
        let mut project =
            pr0_core::demo_project("authority".into(), "Authority".into(), Mode::Conducted);
        project.conductor = Some("alice".into());
        assert!(can_conduct(&project, "alice", "performer"));
        assert!(can_conduct(&project, "owner", "owner"));
        assert!(!can_conduct(&project, "legacy", "conductor"));
    }
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
    let mut local_midi = local_midi::Session::new(
        app.clone(),
        id.clone(),
        u.clone(),
        visualization_session.clone(),
    );
    let mut visualizers = false;
    let mut midi_window = tokio::time::Instant::now();
    let mut midi_messages = 0_u16;
    // Performer authorization (mode, assigned parts) is cached per socket and
    // refreshed on project/member broadcasts instead of reloading the project
    // from SQLite for every inbound MIDI message.
    let mut performer: Option<(bool, Vec<String>)> = None;
    loop {
        tokio::select! {
            event=events.recv()=>match event{Ok(event)=>{
                let v=&event.value;
                if v["type"]=="session_revoked" && v["user_id"]==u && user(&app, &headers).is_err() {let _=socket_text(&mut socket,event.text.to_string()).await;break;}
                if v["type"].as_str().is_some_and(|s|s.starts_with("conductor_midi_")) && v.get("user_id").is_some_and(|target|target!=&u) { continue; }
                if v.get("session_id").is_some_and(|session|session!=&visualization_session) && v["type"].as_str().is_some_and(|s|s.starts_with("conductor_midi_")) {continue;}
                let mine=v.get("project_id").and_then(Value::as_str)==Some(&id);
                if mine && (v["type"]=="project" || v["type"]=="members_changed") {performer=None;local_midi.invalidate();}
                // Serialized once by the producer; non-subscribers take the variant without visualizations.
                let text=if visualizers {&event.text} else {event.stripped.as_ref().unwrap_or(&event.text)};
                if (v["type"]=="hardware_levels" || v["type"]=="audio_engine_status" || v["type"]=="system_audio" || v["type"]=="engine_status" || mine)&&socket_text(&mut socket, text.to_string()).await.is_err(){break;}
            },Err(broadcast::error::RecvError::Lagged(_))=>{if !send_project_snapshot(&mut socket, &app, &id, &u).await {break;}},Err(_)=>break},
            msg=socket.recv()=>match msg{
                Some(Ok(Message::Text(text)))=>{
                    last_received=tokio::time::Instant::now();
                    if let Ok(v)=serde_json::from_str::<Value>(&text){
                        if v["type"].as_str().is_some_and(|t| t.starts_with("conductor_midi_")) && v["type"] != "conductor_midi_disconnect" {
                            if midi_window.elapsed() >= Duration::from_secs(1) { midi_window=tokio::time::Instant::now();midi_messages=0; }
                            midi_messages=midi_messages.saturating_add(1);
                            if (v["type"] != "conductor_midi_data" || midi_messages <= 512) && text.len() <= 32768 {
                                if let Err(error)=conductor_midi::enqueue(&app.conductor_midi,&id,&u,&visualization_session,v.clone()) {
                                    let _=socket_text(&mut socket,json!({"type":"conductor_midi_error","error":error}).to_string()).await;
                                }
                            }
                        }
                        if v["type"]=="visualizers"{
                            visualizers=v["enabled"]==true;
                            let _=send(&app,audio::Command::Visualizers{session:visualization_session.clone(),project:id.clone(),enabled:visualizers});
                        }
                        if v["type"]=="ping"{
                            if visualizers{let _=send(&app,audio::Command::Visualizers{session:visualization_session.clone(),project:id.clone(),enabled:true});}
                            let _=socket_text(&mut socket, json!({"type":"pong","client_time":v["client_time"],"server_time":audio::monotonic_ms()}).to_string()).await;
                        }
                        if matches!(v["type"].as_str(), Some("local_midi" | "local_midi_connect" | "local_midi_reset")) {
                            let result = local_midi.handle(&v);
                            if v["type"] != "local_midi" || result.is_err() {
                                let (connected, error) = match result { Ok(connected) => (connected, None), Err(error) => (false, Some(error)) };
                                let _ = socket_text(&mut socket,json!({"type":"local_midi_status","node":v["node"],"connected":connected,"error":error}).to_string()).await;
                            }
                        }
                        if v["type"]=="performance_midi"{
                            if midi_window.elapsed() >= Duration::from_secs(1) { midi_window=tokio::time::Instant::now();midi_messages=0; }
                            midi_messages=midi_messages.saturating_add(1);
                            let part=v["part"].as_str();
                            if performer.is_none(){
                                performer=load(&app,&id).ok().map(|project| (project.mode != Mode::Structured, project.parts.iter().filter(|candidate| candidate.performer.as_deref()==Some(&u)).map(|candidate| candidate.id.clone()).collect::<Vec<_>>()));
                            }
                            let authorized=performer.as_ref().is_some_and(|(performable, parts)| {
                                *performable && app.active.lock().unwrap().as_deref()==Some(&id)
                                    && part.is_some_and(|part| parts.iter().any(|candidate| candidate==part))
                            });
                            if midi_messages<=2048 && authorized {
                                if let (Some(part),Some(message))=(part,browser_midi_message(&v)) {
                                    let _=send(&app,audio::Command::BrowserMidi{part:part.to_string(),message});
                                }
                            } else if !authorized {
                                let _=socket_text(&mut socket,json!({"type":"midi_error","error":"MIDI target is not an active assigned performer part"}).to_string()).await;
                            }
                        }
                        if v["type"]=="performance_midi_panic"{
                            if let Ok(project)=load(&app,&id) {
                                let parts=project.parts.iter().filter(|part| part.performer.as_deref()==Some(&u)).map(|part|part.id.clone()).collect();
                                let _=send(&app,audio::Command::BrowserMidiPanic{parts});
                            }
                        }
                        if v["type"]=="midi_devices" {
                            let valid=v["devices"].as_array().is_some_and(|devices| devices.len()<=32 && devices.iter().all(|name|name.as_str().is_some_and(|name|name.len()<=120)));
                            if valid { app.logs.push(&id,"info","Browser MIDI devices announced"); }
                        }
                    }
                },
                Some(Ok(Message::Close(_)))|None|Some(Err(_))=>break,
                Some(Ok(_))=>{last_received=tokio::time::Instant::now();}
            },
            _=check.tick()=>{
                performer=None;local_midi.invalidate();
                let _=conductor_midi::enqueue(&app.conductor_midi,&id,&u,&visualization_session,json!({"type":"conductor_midi_heartbeat"}));
                if user(&app,&headers).is_err() || role(&app,&id,&u).is_err() || last_received.elapsed() >= Duration::from_secs(30) {break;}
                if tokio::time::timeout(Duration::from_secs(5), socket.send(Message::Ping(Vec::new().into()))).await.map_or(true, |r| r.is_err()) {break;}
            }
        }
    }
    let _ = conductor_midi::enqueue(&app.conductor_midi,&id,&u,&visualization_session,json!({"type":"conductor_midi_disconnect"}));
    let _ = send(
        &app,
        audio::Command::Visualizers {
            session: visualization_session,
            project: id.clone(),
            enabled: false,
        },
    );
    if let Ok(project) = load(&app, &id) {
        let parts = project
            .parts
            .iter()
            .filter(|part| part.performer.as_deref() == Some(&u))
            .map(|part| part.id.clone())
            .collect();
        let _ = send(&app, audio::Command::BrowserMidiPanic { parts });
    }
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
        CREATE TABLE IF NOT EXISTS invites(token TEXT PRIMARY KEY,project_id TEXT REFERENCES projects(id),role TEXT,expires INTEGER,used INTEGER);
        CREATE TABLE IF NOT EXISTS login_titles(first TEXT NOT NULL,second TEXT NOT NULL);").expect("Database migration");
    db.execute("UPDATE login_titles SET first='Insert pithy title here',second='Put something funny here too' WHERE first='Compose the System' OR (first='Insert pithy title here' AND second='Also something funny here')", []).expect("Login title migration");
    accounts::migrate(&db).expect("Account migration");
    sample_library::migrate(&db).expect("Sample library migration");
    sample_library::seed_bundled(&db).expect("Bundled sample library");
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
    let (conductor_midi, conductor_rx) = std::sync::mpsc::sync_channel(1024);
    let app = App {
        conductor_midi,
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
        desktop_session: desktop_session.clone(),
        media,
    };
    conductor_midi::start(app.clone(), conductor_rx);
    app.osc.listen(&app);
    start_autosave(app.clone());
    let desktop_app = app.clone();
    let assets = std::env::var("PR0_WEB_ROOT").unwrap_or("web/dist".into());
    async fn security_headers(
        request: axum::extract::Request,
        next: axum::middleware::Next,
    ) -> Response {
        let mut response = next.run(request).await;
        let headers = response.headers_mut();
        for (name, value) in [
            (header::X_FRAME_OPTIONS, "DENY"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::REFERRER_POLICY, "same-origin"),
        ] {
            headers
                .entry(name)
                .or_insert_with(|| header::HeaderValue::from_static(value));
        }
        response
    }
    let router = Router::new()
        .layer(axum::middleware::from_fn(security_headers))
        .route(
            "/api/login-titles",
            get(login_titles).put(save_login_titles),
        )
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
        .route("/api/projects/{id}/controller", put(controller_edit))
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
        .route("/api/me", get(me).put(accounts::update_self))
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
        .route("/api/projects/{id}/revisions", get(list_revisions))
        .route("/api/projects/{id}/revisions/{revision}", get(get_revision))
        .route(
            "/api/projects/{id}/save",
            get(save_status).post(save_project),
        )
        .route("/api/projects/{id}/parameter", put(parameter))
        .route("/api/projects/{id}/transport", post(transport))
        .route("/api/projects/{id}/invite", post(invite))
        .route("/api/projects/{id}/members", get(members).post(add_member))
        .route(
            "/api/projects/{id}/members/candidates",
            get(member_candidates),
        )
        .route(
            "/api/projects/{id}/members/{user}",
            axum::routing::delete(remove_member),
        )
        .route(
            "/api/users/{id}/avatar",
            get(user_avatar)
                .put(upload_user_avatar)
                .layer(axum::extract::DefaultBodyLimit::max(4 * 1024 * 1024)),
        )
        .route("/api/projects/{id}/audio", post(audio_enable))
        .route("/api/projects/{id}/events", get(websocket))
        .route(
            "/api/projects/{id}/media",
            get(media::inputs).post(media::offer).delete(media::disconnect),
        )
        .route("/api/projects/{id}/clip", post(clip))
        .route("/api/projects/{id}/cue", post(cue))
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
        let _discovery = discovery::advertise(socket, true);
        tokio::try_join!(
            axum_server::bind_rustls(socket, tls).serve(router.into_make_service()),
            async { axum::serve(listener, tls::redirects(socket.port())).await }
        )
        .expect("Serve HTTPS and HTTP redirect");
    } else {
        let listener = tokio::net::TcpListener::bind(address).await.unwrap();
        let _discovery = discovery::advertise(listener.local_addr().unwrap(), false);
        axum::serve(listener, router).await.unwrap();
    }
}

#[cfg(test)]
mod live_edit_tests {
    use super::*;
    #[test]
    fn member_removal_unassigns_only_that_members_parts() {
        let mut project = demo_project("x".into(), "x".into(), Mode::Freeform);
        project.parts[0].performer = Some("removed".into());
        let mut other = project.parts[0].clone();
        other.id = "other".into();
        other.performer = Some("kept".into());
        project.parts.push(other);
        project.local_audio_assignments.insert("mic".into(),"removed".into());
        project.local_audio_assignments.insert("other-mic".into(),"kept".into());
        assert_eq!(unassign_member_parts(&mut project, "removed"), 2);
        assert!(!project.local_audio_assignments.contains_key("mic"));
        assert_eq!(project.local_audio_assignments["other-mic"],"kept");
        assert_eq!(project.parts[0].performer, None);
        assert_eq!(project.parts[1].performer.as_deref(), Some("kept"));
    }
    #[test]
    fn avatars_are_center_cropped_to_a_small_png() {
        let source = image::DynamicImage::new_rgb8(400, 200);
        let mut encoded = std::io::Cursor::new(Vec::new());
        source
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let cropped = crop_avatar(&encoded.into_inner()).unwrap();
        let image = image::load_from_memory(&cropped).unwrap();
        assert_eq!((image.width(), image.height()), (256, 256));
    }
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
