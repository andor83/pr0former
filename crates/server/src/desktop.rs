//! Desktop-only lifecycle. Authentication is delivered over the parent's private pipe,
//! never an HTTP login bypass, URL credential, default password, or log entry.
use crate::*;
use std::io::{BufRead, Write};

pub fn session(db: &mut Connection) -> Result<String, String> {
    accounts::migrate(db).map_err(|e| e.to_string())?;
    (|| -> Result<String, Box<dyn std::error::Error>> {
        let tx = db.transaction()?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS desktop_owner (singleton INTEGER PRIMARY KEY CHECK(singleton=1), user_id TEXT NOT NULL REFERENCES users(id));")?;
        let owner: Option<String> = tx.query_row("SELECT user_id FROM desktop_owner WHERE singleton=1", [], |r| r.get(0)).optional()?;
        let owner = if let Some(owner) = owner { owner } else {
            let count: i64 = tx.query_row("SELECT count(*) FROM users", [], |r| r.get(0))?;
            if count != 0 { return Err("Desktop mode requires its own data directory; refusing to adopt existing server accounts".into()); }
            let owner = uid();
            // Random, discarded password: only the private launcher session signs in.
            let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes()).map_err(|e| e.to_string())?;
            let password = Argon2::default().hash_password(uid().as_bytes(), &salt).map_err(|e| e.to_string())?.to_string();
            tx.execute("INSERT INTO users(id,username,password) VALUES(?1,'admin',?2)", params![owner,password])?;
            tx.execute("INSERT INTO desktop_owner VALUES(1,?1)", [&owner])?;
            owner
        };
        let token = uid() + &uid();
        tx.execute("DELETE FROM sessions WHERE expires<=?1", [now()])?;
        tx.execute("INSERT INTO sessions(token,user_id,expires) VALUES(?1,?2,?3)", params![token,owner,now()+86400])?;
        tx.commit()?;
        Ok(token)
    })().map_err(|e| e.to_string())
}
use rusqlite::OptionalExtension;

pub async fn serve(router: Router, app: App, token: String) {
    // Deliberately ignore PR0_BIND/PORT/TLS in desktop mode. No privileged ports or LAN bypass.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("Bind desktop server");
    let address = listener.local_addr().unwrap();
    let expected_host = address.to_string();
    let shared_router = router.clone();
    let router = router.layer(axum::middleware::from_fn(
        move |req: axum::extract::Request, next: axum::middleware::Next| {
            let valid = req
                .headers()
                .get(header::HOST)
                .and_then(|v| v.to_str().ok())
                == Some(expected_host.as_str());
            async move {
                if valid {
                    next.run(req).await
                } else {
                    StatusCode::FORBIDDEN.into_response()
                }
            }
        },
    ));
    let (control_tx, mut control_rx) = tokio::sync::mpsc::unbounded_channel();
    // A standard thread avoids Tokio stdin's uncancellable blocking read at runtime exit.
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else {
                break;
            };
            if let Ok(value) = serde_json::from_str::<Value>(&line) {
                if value["hosting"].is_boolean() {
                    if control_tx.send(Some(value)).is_err() {
                        return;
                    }
                    continue;
                }
            }
            break; // Preserve the legacy stop-on-command behavior for other input.
        }
        let _ = control_tx.send(None); // EOF (including parent crash) stops both listeners.
    });
    println!(
        "PR0_DESKTOP_READY {}",
        json!({"url":format!("http://{address}"),"session":token})
    );
    std::io::stdout().flush().expect("Notify desktop launcher");
    let serving = axum::serve(listener, router).into_future();
    tokio::pin!(serving);
    let mut hosting: Option<crate::hosting::Hosting> = None;
    loop {
        tokio::select! {
            result = &mut serving => { result.expect("Serve desktop"); break; }
            control = control_rx.recv() => {
                let Some(Some(value)) = control else { break; };
                let status = if value["hosting"].as_bool() == Some(false) {
                    hosting = None;
                    json!({"enabled":false})
                } else if let Some(host) = &hosting {
                    host.status.clone()
                } else {
                    let port = value["port"].as_u64().unwrap_or(8443);
                    if port > u16::MAX as u64 { json!({"enabled":false,"error":"Invalid hosting port"}) }
                    else { match crate::hosting::start(shared_router.clone(), token.clone(), port as u16).await {
                        Ok(host) => { let status=host.status.clone(); hosting=Some(host); status }
                        Err(error) => json!({"enabled":false,"error":error}),
                    } }
                };
                println!("PR0_DESKTOP_HOSTING {status}");
                let _ = std::io::stdout().flush();
            }
        }
    }
    drop(hosting);
    let (tx, rx) = oneshot::channel();
    let engine = app.engine.clone();
    tokio::task::spawn_blocking(move || engine.send(audio::Command::Shutdown(tx)))
        .await
        .unwrap()
        .expect("Request engine shutdown");
    match rx.await {
        Ok(Ok(())) => {}
        result => {
            eprintln!("Desktop audio shutdown failed: {result:?}");
            std::process::exit(1);
        }
    }
    app.db
        .lock()
        .unwrap()
        .execute("DELETE FROM sessions WHERE token=?1", [&token])
        .expect("Revoke desktop session");
}

#[cfg(test)]
mod tests {
    use super::*;
    fn db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE users(id TEXT PRIMARY KEY,username TEXT UNIQUE,password TEXT); CREATE TABLE sessions(token TEXT PRIMARY KEY,user_id TEXT REFERENCES users(id),expires INTEGER);").unwrap();
        db
    }
    #[test]
    fn desktop_identity_is_stable_sessions_are_fresh_and_expire() {
        let mut db = db();
        let first = session(&mut db).unwrap();
        let second = session(&mut db).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            db.query_row("SELECT count(*) FROM users", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        let owners: i64 = db
            .query_row(
                "SELECT count(DISTINCT user_id) FROM sessions WHERE expires > ?1",
                [now()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(owners, 1);
    }
    #[test]
    fn desktop_never_adopts_existing_server_users() {
        let mut db = db();
        db.execute("INSERT INTO users VALUES('existing','admin','hash')", [])
            .unwrap();
        assert!(session(&mut db).unwrap_err().contains("refusing"));
        assert_eq!(
            db.query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
