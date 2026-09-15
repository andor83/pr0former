//! Opt-in local-network hosting, controlled directly on the embedded runtime.
//!
//! The dialog's command calls `RuntimeHandle::host_on_lan`/`stop_hosting`/
//! `hosting_status` in process. There is no control pipe, pending-reply bridge
//! or cached status any more: every answer is the runtime's own current state.
use serde_json::{Value, json};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

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
    // Resolved before awaiting: Tauri's state borrow must not be held across an
    // await point.
    let runtime = crate::runtime::engine(&app)?;
    Ok(match enabled {
        // The dialog opens with `null` to read the current state.
        None => runtime.hosting_status().await,
        Some(false) => runtime.stop_hosting().await,
        // A refused port or certificate failure is reported in the dialog
        // rather than as a command error, so the toggle stays usable.
        Some(true) => match runtime.host_on_lan(port.unwrap_or(8443)).await {
            Ok(status) => status,
            Err(error) => json!({"enabled": false, "error": error}),
        },
    })
}
