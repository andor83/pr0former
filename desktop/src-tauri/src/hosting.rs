use serde_json::{Value, json};
use std::{
    io::Write,
    sync::{Mutex, mpsc},
    time::Duration,
};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

pub struct Bridge {
    status: Mutex<Value>,
    pending: Mutex<Option<mpsc::Sender<Value>>>,
}
pub fn start(app: &AppHandle) {
    app.manage(Bridge {
        status: Mutex::new(json!({"enabled":false})),
        pending: Mutex::new(None),
    });
}
pub fn reply(app: &AppHandle, status: Value) {
    let bridge = app.state::<Bridge>();
    *bridge.status.lock().unwrap() = status.clone();
    if let Some(sender) = bridge.pending.lock().unwrap().take() {
        let _ = sender.send(status);
    }
}
fn allowed(window: &WebviewWindow) -> bool {
    if window.label() != "hosting" {
        return false;
    }
    window.url().is_ok_and(|url| {
        url.path() == "/host.html"
            && url.query().is_none()
            && url.fragment().is_none()
            && ((url.scheme() == "tauri" && url.host_str() == Some("localhost"))
                || (matches!(url.scheme(), "http" | "https")
                    && url.host_str() == Some("tauri.localhost")
                    && url.port().is_none()))
    })
}
pub fn show(app: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = app.get_webview_window("hosting") {
        window.show()?;
        return window.set_focus();
    }
    WebviewWindowBuilder::new(app, "hosting", WebviewUrl::App("host.html".into()))
        .title("Host a Performance on the Local Network")
        .inner_size(640., 700.)
        .min_inner_size(480., 480.)
        .on_navigation(|url| {
            url.path() == "/host.html"
                && ((url.scheme() == "tauri" && url.host_str() == Some("localhost"))
                    || (matches!(url.scheme(), "http" | "https")
                        && url.host_str() == Some("tauri.localhost")))
        })
        .build()?;
    Ok(())
}
#[tauri::command]
pub async fn hosting_control(
    app: AppHandle,
    window: WebviewWindow,
    enabled: Option<bool>,
    port: Option<u16>,
) -> Result<Value, String> {
    if !allowed(&window) {
        return Err("Open hosting settings from the desktop Server menu.".into());
    }
    let bridge = app.state::<Bridge>();
    let Some(enabled) = enabled else {
        return Ok(bridge.status.lock().unwrap().clone());
    };
    let (sender, receiver) = mpsc::channel();
    {
        let mut pending = bridge.pending.lock().unwrap();
        if pending.is_some() {
            return Err("A hosting change is already in progress.".into());
        }
        *pending = Some(sender);
    }
    let sent = (|| -> Result<(), String> {
        let server = app
            .try_state::<crate::SharedServer>()
            .ok_or("The bundled engine is still starting.")?;
        let mut server = server.lock().map_err(|e| e.to_string())?;
        let stdin = server
            .child
            .as_mut()
            .and_then(|child| child.stdin.as_mut())
            .ok_or("The bundled engine is not running.")?;
        writeln!(
            stdin,
            "{}",
            json!({"hosting":enabled,"port":port.unwrap_or(8443)})
        )
        .and_then(|_| stdin.flush())
        .map_err(|e| e.to_string())
    })();
    if let Err(error) = sent {
        bridge.pending.lock().unwrap().take();
        return Err(error);
    }
    let result = tauri::async_runtime::spawn_blocking(move || {
        receiver.recv_timeout(Duration::from_secs(30))
    })
    .await;
    bridge.pending.lock().unwrap().take();
    match result {
        Ok(Ok(status)) => Ok(status),
        _ => Err("Hosting did not respond. Reopen this dialog to check its status.".into()),
    }
}
