use fs2::FileExt;
use serde::Deserialize;
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
    process::{Child, Command, Stdio},
    sync::{Arc, Mutex, mpsc},
    time::{Duration, Instant},
};
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

#[derive(Deserialize)]
struct Ready {
    url: String,
    session: String,
}
struct Server {
    child: Option<Child>,
    _lock: File,
}
impl Server {
    fn stop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        // Closing the pipe also works if the launcher crashes. The server acknowledges
        // shutdown by exiting only after the archive/loop writer barriers complete.
        drop(child.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        eprintln!("Server shutdown failed: {status}");
                    }
                    break;
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                _ => {
                    eprintln!("Server did not shut down within 30 seconds; forcing exit");
                    let _ = child.kill();
                    let _ = child.wait();
                    break;
                }
            }
        }
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stop();
    }
}
type SharedServer = Arc<Mutex<Server>>;

fn spawn_server(
    data: &Path,
    resources: &Path,
    lock: File,
) -> Result<(SharedServer, Ready), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let bin = executable.parent().ok_or("Missing application directory")?;
    let log = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(data.join("desktop-server.log"))
        .map_err(|e| e.to_string())?;
    let mut command = Command::new(bin.join("pr0-server"));
    // Inherited server overrides must never point a desktop app at production data.
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("PR0_") {
            command.env_remove(key);
        }
    }
    if std::env::var("PR0_DESKTOP_DISABLE_NATIVE_DEVICES").as_deref() == Ok("1") {
        command.env("PR0_DISABLE_NATIVE_DEVICES", "1");
    }
    let mut child = command
        .arg("--desktop")
        .current_dir(data)
        .env("PR0_DATA", data.join("data"))
        .env("PR0_RECORDINGS_ROOT", data.join("recordings"))
        .env("PR0_WEB_ROOT", resources.join("resources/web"))
        .env("PR0_FFMPEG", bin.join("ffmpeg"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::from(log.try_clone().map_err(|e| e.to_string())?))
        .spawn()
        .map_err(|e| format!("Cannot start bundled server: {e}"))?;
    let stdout = child.stdout.take().ok_or("Missing server pipe")?;
    let server = Arc::new(Mutex::new(Server {
        child: Some(child),
        _lock: lock,
    }));
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut log = log;
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Some(json) = line.strip_prefix("PR0_DESKTOP_READY ") {
                let _ = tx.send(serde_json::from_str::<Ready>(json).map_err(|e| e.to_string()));
            } else {
                let _ = writeln!(log, "{line}");
            }
        }
    });
    let ready = rx.recv_timeout(Duration::from_secs(45)).map_err(|_| {
        format!(
            "Server failed to start. See {}",
            data.join("desktop-server.log").display()
        )
    })??;
    let url: tauri::Url = ready.url.parse().map_err(|_| "Invalid server URL")?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none()
        || ready.session.len() != 72
    {
        return Err("Invalid desktop server handshake".into());
    }
    Ok((server, ready))
}

fn main() {
    let app = tauri::Builder::default()
        .setup(|app| {
            let window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("pr0former")
                    .inner_size(1440., 960.)
                    .min_inner_size(640., 480.)
                    .disable_drag_drop_handler()
                    .build()?;
            let data = std::env::var_os("PR0_DESKTOP_DATA")
                .map(std::path::PathBuf::from)
                .unwrap_or(app.path().app_data_dir()?);
            if !data.is_absolute() {
                return Err("PR0_DESKTOP_DATA must be an absolute directory".into());
            }
            std::fs::create_dir_all(&data)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700))?;
            }
            let resources = app.path().resource_dir()?;
            let handle = app.handle().clone();
            tauri::async_runtime::spawn_blocking(move || {
                let result = (|| -> Result<(), String> {
                    let lock = OpenOptions::new()
                        .create(true)
                        .truncate(false)
                        .read(true)
                        .write(true)
                        .open(data.join("desktop.lock"))
                        .map_err(|e| e.to_string())?;
                    lock.try_lock_exclusive().map_err(|_| {
                        "pr0former is already running. Switch to its existing window.".to_string()
                    })?;
                    let (server, ready) = spawn_server(&data, &resources, lock)?;
                    handle.manage(server);
                    let cookie = tauri::webview::Cookie::build(("pr0_session", ready.session))
                        .domain("127.0.0.1")
                        .path("/")
                        .http_only(true)
                        .same_site(tauri::webview::cookie::SameSite::Strict)
                        .build();
                    window.set_cookie(cookie).map_err(|e| e.to_string())?;
                    window
                        .navigate(ready.url.parse().map_err(|_| "Invalid server URL")?)
                        .map_err(|e| e.to_string())?;
                    Ok(())
                })();
                if let Err(error) = result {
                    let text = serde_json::to_string(&error).unwrap();
                    let _ = window.eval(format!(
                        "document.getElementById('status').textContent={text}"
                    ));
                }
            });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("Build pr0former desktop");
    app.run(|app, event| {
        if let tauri::RunEvent::Exit = event {
            if let Some(server) = app.try_state::<SharedServer>() {
                server.lock().unwrap().stop();
            }
        }
    });
}
