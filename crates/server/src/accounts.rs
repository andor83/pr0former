use crate::*;

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS user_profiles(user_id TEXT PRIMARY KEY REFERENCES users(id),is_admin INTEGER NOT NULL DEFAULT 0,enabled INTEGER NOT NULL DEFAULT 1,deleted INTEGER NOT NULL DEFAULT 0,body TEXT NOT NULL DEFAULT '{}',revision INTEGER NOT NULL DEFAULT 0,created INTEGER NOT NULL DEFAULT 0);
        INSERT OR IGNORE INTO user_profiles(user_id,is_admin,created) SELECT id,CASE WHEN rowid=(SELECT min(rowid) FROM users) THEN 1 ELSE 0 END,strftime('%s','now') FROM users;
        CREATE TRIGGER IF NOT EXISTS initialize_user_profile AFTER INSERT ON users BEGIN INSERT INTO user_profiles(user_id,is_admin,created) VALUES(new.id,CASE WHEN (SELECT count(*) FROM users)=1 THEN 1 ELSE 0 END,strftime('%s','now')); END;
        CREATE TABLE IF NOT EXISTS user_avatars(user_id TEXT PRIMARY KEY REFERENCES users(id),body BLOB NOT NULL,revision INTEGER NOT NULL DEFAULT 1);
        CREATE TABLE IF NOT EXISTS project_recents(user_id TEXT REFERENCES users(id),project_id TEXT REFERENCES projects(id),opened INTEGER NOT NULL,PRIMARY KEY(user_id,project_id));")
}
pub fn is_admin(db: &Connection, id: &str) -> bool {
    db.query_row(
        "SELECT is_admin AND enabled AND NOT deleted FROM user_profiles WHERE user_id=?1",
        [id],
        |r| r.get(0),
    )
    .unwrap_or(false)
}
pub fn admin(app: &App, headers: &HeaderMap) -> Api<String> {
    let id = user(app, headers)?;
    if !is_admin(&app.db.lock().unwrap(), &id) {
        return Err(Failure(
            StatusCode::FORBIDDEN,
            "Administrator access required".into(),
        ));
    }
    Ok(id)
}
pub fn profile(db: &Connection, id: &str) -> Api<Value> {
    db.query_row("SELECT u.username,p.is_admin,p.enabled,p.body,p.revision,p.created FROM users u JOIN user_profiles p ON p.user_id=u.id WHERE u.id=?1 AND p.deleted=0",[id],|r| {
        let body:String=r.get(3)?;
        let mut value:Value=serde_json::from_str(&body).unwrap_or(json!({}));
        value["id"]=json!(id); value["username"]=json!(r.get::<_,String>(0)?);
        value["is_admin"]=json!(r.get::<_,bool>(1)?);value["enabled"]=json!(r.get::<_,bool>(2)?);
        value["revision"]=json!(r.get::<_,u64>(4)?);value["created"]=json!(r.get::<_,u64>(5)?);
        Ok(value)
    }).map_err(|_|Failure(StatusCode::NOT_FOUND,"User unavailable".into()))
}
#[derive(Deserialize, serde::Serialize, Default)]
#[serde(default, deny_unknown_fields)]
pub struct Fields {
    pub prefix: String,
    pub first_name: String,
    pub middle_name: String,
    pub last_name: String,
    pub suffix: String,
    pub email: String,
    pub phone: String,
    pub organization: String,
    pub notes: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Edit {
    username: String,
    is_admin: bool,
    enabled: bool,
    revision: Option<u64>,
    password: Option<String>,
    fields: Fields,
}
fn validate(edit: &mut Edit) -> Api<()> {
    edit.username = edit.username.trim().to_lowercase();
    if !(3..=64).contains(&edit.username.len()) || edit.username.chars().any(char::is_control) {
        return Err(bad("Username must contain 3–64 characters"));
    }
    let f = &edit.fields;
    for text in [
        &f.prefix,
        &f.first_name,
        &f.middle_name,
        &f.last_name,
        &f.suffix,
        &f.email,
        &f.phone,
        &f.organization,
    ] {
        if text.len() > 256 || text.chars().any(char::is_control) {
            return Err(bad(
                "Profile fields must be at most 256 characters without control characters",
            ));
        }
    }
    if f.notes.len() > 4096
        || (!f.email.is_empty()
            && (!f.email.contains('@') || f.email.chars().any(char::is_whitespace)))
    {
        return Err(bad("Invalid email or notes"));
    }
    Ok(())
}
async fn password_hash(password: String) -> Api<String> {
    if !(8..=256).contains(&password.len()) {
        return Err(bad("Password must contain 8–256 characters"));
    }
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes()).map_err(internal)?;
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|v| v.to_string())
            .map_err(internal)
    })
    .await
    .map_err(internal)?
}
pub async fn list(State(app): State<App>, headers: HeaderMap) -> Api<Json<Value>> {
    admin(&app, &headers)?;
    let db = app.db.lock().unwrap();
    let ids = db
        .prepare("SELECT user_id FROM user_profiles WHERE deleted=0 ORDER BY created,user_id")
        .map_err(internal)?
        .query_map([], |r| r.get::<_, String>(0))
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    Ok(Json(json!(
        ids.iter()
            .map(|id| profile(&db, id))
            .collect::<Result<Vec<_>, _>>()?
    )))
}
pub async fn create(
    State(app): State<App>,
    headers: HeaderMap,
    Json(mut edit): Json<Edit>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let actor = admin(&app, &headers)?;
    validate(&mut edit)?;
    let hash = password_hash(
        edit.password
            .take()
            .ok_or_else(|| bad("An initial password is required"))?,
    )
    .await?;
    let id = uid();
    let mut db = app.db.lock().unwrap();
    let tx = db.transaction().map_err(internal)?;
    if !is_admin(&tx, &actor) {
        return Err(Failure(
            StatusCode::FORBIDDEN,
            "Administrator access required".into(),
        ));
    }
    tx.execute(
        "INSERT INTO users(id,username,password) VALUES(?1,?2,?3)",
        params![id, edit.username, hash],
    )
    .map_err(|_| bad("Username already exists"))?;
    tx.execute(
        "UPDATE user_profiles SET body=?2,is_admin=?3,enabled=?4 WHERE user_id=?1",
        params![
            id,
            serde_json::to_string(&edit.fields).map_err(internal)?,
            edit.is_admin,
            edit.enabled
        ],
    )
    .map_err(internal)?;
    tx.commit().map_err(internal)?;
    profile(&db, &id).map(Json)
}
pub async fn update(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(mut edit): Json<Edit>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let actor = admin(&app, &headers)?;
    validate(&mut edit)?;
    let hash = match edit.password.take().filter(|p| !p.is_empty()) {
        Some(p) => Some(password_hash(p).await?),
        None => None,
    };
    let (value, revoke) = {
        let mut db = app.db.lock().unwrap();
        let tx = db.transaction().map_err(internal)?;
        if !is_admin(&tx, &actor) {
            return Err(Failure(
                StatusCode::FORBIDDEN,
                "Administrator access required".into(),
            ));
        }
        let old = profile(&tx, &id)?;
        if old["revision"].as_u64() != edit.revision {
            return Err(Failure(
                StatusCode::CONFLICT,
                "User changed; reload before saving".into(),
            ));
        }
        protect_admin(&tx, &id, edit.is_admin && edit.enabled)?;
        let revoke =
            hash.is_some() || old["is_admin"] != edit.is_admin || old["enabled"] != edit.enabled;
        tx.execute(
            "UPDATE users SET username=?2,password=COALESCE(?3,password) WHERE id=?1",
            params![id, edit.username, hash],
        )
        .map_err(|_| bad("Username already exists"))?;
        tx.execute("UPDATE user_profiles SET body=?2,is_admin=?3,enabled=?4,revision=revision+1 WHERE user_id=?1",params![id,serde_json::to_string(&edit.fields).map_err(internal)?,edit.is_admin,edit.enabled]).map_err(internal)?;
        if revoke {
            tx.execute("DELETE FROM sessions WHERE user_id=?1", [&id])
                .map_err(internal)?;
        }
        let value = profile(&tx, &id)?;
        tx.commit().map_err(internal)?;
        (value, revoke)
    };
    if revoke {
        revoke_live(&app, &id).await;
    }
    Ok(Json(value))
}
fn protect_admin(db: &Connection, id: &str, remains: bool) -> Api<()> {
    if is_admin(db, id) && !remains {
        let count: i64 = db
            .query_row(
                "SELECT count(*) FROM user_profiles WHERE is_admin=1 AND enabled=1 AND deleted=0",
                [],
                |r| r.get(0),
            )
            .map_err(internal)?;
        if count <= 1 {
            return Err(bad(
                "The last active administrator cannot be removed, disabled or demoted",
            ));
        }
    }
    Ok(())
}
async fn revoke_live(app: &App, id: &str) {
    let _ = app
        .events
        .send(json!({"type":"session_revoked","user_id":id}));
    app.media.close_user(id).await;
}
pub async fn revoke(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    admin(&app, &headers)?;
    {
        let db = app.db.lock().unwrap();
        profile(&db, &id)?;
        db.execute("DELETE FROM sessions WHERE user_id=?1", [&id])
            .map_err(internal)?;
    }
    revoke_live(&app, &id).await;
    Ok(Json(json!({"ok":true})))
}
#[derive(Deserialize)]
pub struct Delete {
    confirm: String,
    revision: u64,
}
pub async fn delete(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<Delete>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let actor = admin(&app, &headers)?;
    {
        let mut db = app.db.lock().unwrap();
        let tx = db.transaction().map_err(internal)?;
        let old = profile(&tx, &id)?;
        if actor == id {
            return Err(bad("You cannot delete your signed-in account"));
        }
        if old["username"] != request.confirm || old["revision"] != request.revision {
            return Err(bad(
                "Enter the current username and reload if the account changed",
            ));
        }
        protect_admin(&tx, &id, false)?;
        let owned:i64=tx.query_row("SELECT (SELECT count(*) FROM members WHERE user_id=?1 AND role='owner')+(SELECT count(*) FROM sample_library WHERE owner=?1 AND deleted=0)",[&id],|r|r.get(0)).map_err(internal)?;
        if owned > 0 {
            return Err(bad(
                "This user owns projects or samples. Deactivate the account to preserve ownership instead",
            ));
        }
        tx.execute("DELETE FROM sessions WHERE user_id=?1", [&id])
            .map_err(internal)?;
        tx.execute("DELETE FROM members WHERE user_id=?1", [&id])
            .map_err(internal)?;
        tx.execute("DELETE FROM project_recents WHERE user_id=?1", [&id])
            .map_err(internal)?;
        tx.execute("UPDATE user_profiles SET deleted=1,enabled=0,body='{}',revision=revision+1 WHERE user_id=?1",[&id]).map_err(internal)?;
        tx.execute(
            "UPDATE users SET username=?2,password='' WHERE id=?1",
            params![id, format!("deleted-{id}")],
        )
        .map_err(internal)?;
        tx.commit().map_err(internal)?;
    }
    revoke_live(&app, &id).await;
    Ok(Json(json!({"ok":true})))
}
pub async fn opened(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    role(&app, &id, &u)?;
    let db = app.db.lock().unwrap();
    // A per-user monotonic sequence gives deterministic ordering even within a second.
    db.execute("INSERT INTO project_recents(user_id,project_id,opened) VALUES(?1,?2,(SELECT COALESCE(MAX(opened),0)+1 FROM project_recents WHERE user_id=?1)) ON CONFLICT(user_id,project_id) DO UPDATE SET opened=excluded.opened",params![u,id]).map_err(internal)?;
    Ok(Json(json!({"ok":true})))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_legacy_user_is_admin_once_and_last_admin_is_protected() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE users(id TEXT PRIMARY KEY,username TEXT,password TEXT);CREATE TABLE projects(id TEXT);INSERT INTO users VALUES('a','first','hash');INSERT INTO users VALUES('b','second','hash');").unwrap();
        migrate(&db).unwrap();
        assert!(is_admin(&db, "a"));
        assert!(!is_admin(&db, "b"));
        assert!(protect_admin(&db, "a", false).is_err());
        db.execute("UPDATE user_profiles SET is_admin=1 WHERE user_id='b'", [])
            .unwrap();
        db.execute("UPDATE user_profiles SET is_admin=0 WHERE user_id='a'", [])
            .unwrap();
        migrate(&db).unwrap();
        assert!(!is_admin(&db, "a"));
        assert!(is_admin(&db, "b"));
        db.execute("INSERT INTO users VALUES('c','third','hash')", [])
            .unwrap();
        assert!(!is_admin(&db, "c"));
    }
}
