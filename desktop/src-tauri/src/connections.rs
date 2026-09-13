use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use tauri::{AppHandle, Manager, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder};

const CHOOSER: &str = "server-connection";
static NEXT_WINDOW: AtomicU64 = AtomicU64::new(1);

pub(crate) fn server_url(address: &str) -> Result<Url, String> {
    let url = Url::parse(address.trim()).map_err(|_| {
        "Enter a full server address, such as https://studio.local:8443".to_string()
    })?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("Use an HTTP or HTTPS server address.".into());
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err("Enter the server address without credentials; sign in on the server.".into());
    }
    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        return Err("Enter the server's base address without a path, query, or fragment.".into());
    }
    Ok(url)
}

fn chooser_url(url: &Url) -> bool {
    url.path() == "/connect.html"
        && url.query().is_none()
        && url.fragment().is_none()
        && match url.scheme() {
            "tauri" => url.host_str() == Some("localhost"),
            "http" | "https" => url.host_str() == Some("tauri.localhost") && url.port().is_none(),
            _ => false,
        }
}

pub fn show(app: &AppHandle) -> tauri::Result<()> {
    show_address(app, None)
}

pub fn show_address(app: &AppHandle, address: Option<String>) -> tauri::Result<()> {
    let script = address.map(|address| format!(
        "document.getElementById('address').value={};document.getElementById('connect-form').requestSubmit()",
        serde_json::to_string(&address).unwrap()
    ));
    if let Some(window) = app.get_webview_window(CHOOSER) {
        window.show()?;
        if let Some(script) = script {
            window.eval(script)?;
        }
        return window.set_focus();
    }
    WebviewWindowBuilder::new(app, CHOOSER, WebviewUrl::App("connect.html".into()))
        .title("Connect to pr0former Server")
        .inner_size(560., 540.)
        .min_inner_size(420., 320.)
        .on_navigation(chooser_url)
        .on_page_load(move |window, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished {
                if let Some(script) = &script {
                    let _ = window.eval(script);
                }
            }
        })
        .build()?;
    Ok(())
}

#[tauri::command]
pub async fn connect_server(
    app: AppHandle,
    window: WebviewWindow,
    address: String,
    trust_fingerprint: Option<String>,
) -> Result<(), crate::trust::ConnectError> {
    // Only the bundled chooser has this permission. Check its actual origin too.
    if window.label() != CHOOSER || !chooser_url(&window.url().map_err(|e| e.to_string())?) {
        return Err("Open Connect to Server from the native Server menu.".into());
    }
    let url = server_url(&address)?;
    // Use the OS trust store, with no cookies or bundled session. WKWebView can
    // otherwise fail navigation without surfacing a useful error to the caller.
    let pin = crate::trust::check(&app, &url, trust_fingerprint.as_deref()).await?;
    let title = format!("pr0former — {}", url.origin().ascii_serialization());
    let label = format!("remote-{}", NEXT_WINDOW.fetch_add(1, Ordering::Relaxed));
    let (loaded, ready) = mpsc::sync_channel(1);
    let remote = WebviewWindowBuilder::new(&app, label, WebviewUrl::App("index.html".into()))
        .title(title)
        .inner_size(1440., 960.)
        .min_inner_size(640., 480.)
        .disable_drag_drop_handler()
        // Cookies are host-scoped, not port-scoped: even another loopback server
        // must never inherit the bundled engine's automatically granted session.
        .incognito(true)
        .initialization_script(crate::windows::initialization_script())
        .on_document_title_changed(crate::windows::loaded)
        .visible(false)
        .on_navigation(|url| matches!(url.scheme(), "http" | "https" | "tauri"))
        .on_page_load(move |_, payload| {
            if payload.event() == tauri::webview::PageLoadEvent::Finished
                && matches!(payload.url().scheme(), "http" | "https")
            {
                let _ = loaded.try_send(());
            }
        })
        .build()
        .map_err(|e| format!("Could not open the server: {e}"))?;
    crate::trust::install(&remote, pin)?;
    remote.navigate(url).map_err(|e| e.to_string())?;
    let result =
        tauri::async_runtime::spawn_blocking(move || ready.recv_timeout(Duration::from_secs(20)))
            .await;
    if !matches!(result, Ok(Ok(()))) {
        let _ = remote.close();
        return Err("The server did not load in the app. Check its address, network access, and HTTPS certificate trust. The bundled engine is still available.".into());
    }
    remote.show().map_err(|e| e.to_string())?;
    remote.set_focus().map_err(|e| e.to_string())?;
    let _ = window.close();
    Ok(())
}

pub(crate) async fn check_server(url: &Url) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .redirect(reqwest::redirect::Policy::limited(5))
        .build()
        .map_err(|e| e.to_string())?;
    client.get(url.clone()).send().await
        .and_then(|response| response.error_for_status())
        .map(|_| ())
        .map_err(|error| {
            use std::error::Error;
            let mut details = error.to_string();
            let mut source = error.source();
            while let Some(reason) = source { details.push_str(&format!(": {reason}")); source = reason.source(); }
            format!("Could not connect to {}. {details}. For HTTPS, the certificate must be trusted by this Mac and match the server name.", url.origin().ascii_serialization())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unreachable_servers_return_an_actionable_error_before_opening_a_window() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap())
            .parse()
            .unwrap();
        drop(listener);
        let error = tauri::async_runtime::block_on(check_server(&url)).unwrap_err();
        assert!(error.contains("Could not connect"));
    }
    #[test]
    fn addresses_keep_the_selected_server_and_reject_ambiguous_targets() {
        for address in [
            "https://studio.local:8443",
            "http://192.168.1.3:8080",
            "http://[::1]:8080/",
        ] {
            assert!(server_url(address).is_ok(), "{address}");
        }
        assert_eq!(
            server_url("  https://studio.local  ").unwrap().as_str(),
            "https://studio.local/"
        );
        for address in [
            "",
            "studio.local",
            "file:///tmp/index.html",
            "javascript:alert(1)",
            "https://user:password@host",
            "https://host/project",
            "https://host/?token=x",
            "https://host/#x",
        ] {
            assert!(server_url(address).is_err(), "{address}");
        }
    }
    #[test]
    fn only_bundled_chooser_origins_can_connect() {
        for address in [
            "tauri://localhost/connect.html",
            "http://tauri.localhost/connect.html",
            "https://tauri.localhost/connect.html",
        ] {
            assert!(chooser_url(&address.parse().unwrap()));
        }
        for address in [
            "https://example.com/connect.html",
            "http://127.0.0.1:9000/connect.html",
            "tauri://localhost/index.html",
            "https://tauri.localhost:8080/connect.html",
            "tauri://localhost/connect.html?x=1",
        ] {
            assert!(!chooser_url(&address.parse().unwrap()));
        }
    }
}
