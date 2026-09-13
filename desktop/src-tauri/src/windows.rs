use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc,
    },
    time::Duration,
};
use tauri::{
    AppHandle, Manager, PhysicalPosition, PhysicalSize, Url, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};

const LIMIT: usize = 8;
static NEXT: AtomicU64 = AtomicU64::new(1);
pub fn initialization_script() -> String {
    static SCRIPT: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    SCRIPT.get_or_init(|| {
        let name = std::process::Command::new("hostname").output().ok()
            .filter(|out| out.status.success()).map(|out| String::from_utf8_lossy(&out.stdout).trim().to_owned())
            .unwrap_or_else(|| "Desktop computer".into());
        format!("window.__PR0_DESKTOP__=true;window.__PR0_MACHINE_NAME__={};", serde_json::to_string(&name).unwrap())
    }).clone()
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Placement {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
    view: String,
}
#[derive(Clone)]
struct Context {
    key: String,
    view: String,
}
#[derive(Default)]
struct Catalog {
    saved: BTreeMap<String, Vec<Placement>>,
    active: BTreeMap<String, (Context, Placement)>,
}
pub struct Windows {
    catalog: Mutex<Catalog>,
    save: mpsc::Sender<()>,
    quitting: AtomicBool,
    path: PathBuf,
    writer: Mutex<()>,
}

pub fn start(app: &AppHandle, path: PathBuf) {
    let saved = std::fs::read(&path)
        .ok()
        .filter(|bytes| bytes.len() < 1024 * 1024)
        .and_then(|bytes| serde_json::from_slice::<BTreeMap<String, Vec<Placement>>>(&bytes).ok())
        .unwrap_or_default();
    let (save, receiver) = mpsc::channel();
    app.manage(Windows {
        catalog: Mutex::new(Catalog {
            saved,
            ..Default::default()
        }),
        save,
        quitting: AtomicBool::new(false),
        path,
        writer: Mutex::new(()),
    });
    let app = app.clone();
    std::thread::spawn(move || {
        while receiver.recv().is_ok() {
            while receiver.recv_timeout(Duration::from_millis(300)).is_ok() {}
            persist(&app.state::<Windows>());
        }
    });
}

fn persist(state: &Windows) {
    let _writer = state.writer.lock().unwrap();
    let bytes = serde_json::to_vec_pretty(&state.catalog.lock().unwrap().saved);
    if let Ok(bytes) = bytes {
        let temp = state.path.with_extension("tmp");
        if let Err(error) =
            std::fs::write(&temp, bytes).and_then(|_| std::fs::rename(temp, &state.path))
        {
            eprintln!("Save window layout: {error}");
        }
    }
}

fn valid_view(view: &str) -> &str {
    match view {
        "score" | "conductor" | "ensemble" | "monitor" | "stage" => view,
        _ => "graph",
    }
}
fn context(url: &Url, bundled: Option<&Url>) -> Option<Context> {
    if !matches!(url.scheme(), "http" | "https") {
        return None;
    }
    let project = url
        .query_pairs()
        .find(|(key, _)| key == "project")?
        .1
        .into_owned();
    if project.is_empty()
        || project.len() > 128
        || !project
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_'))
    {
        return None;
    }
    let view = url
        .query_pairs()
        .find(|(key, _)| key == "view")
        .map(|(_, value)| valid_view(&value).to_owned())
        .unwrap_or("graph".into());
    let origin = if bundled.is_some_and(|local| local.origin() == url.origin()) {
        "bundled".into()
    } else {
        url.origin().ascii_serialization()
    };
    Some(Context {
        key: format!("{origin}/{project}"),
        view,
    })
}
fn placement(window: &WebviewWindow, view: &str) -> Option<Placement> {
    let position = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    Some(Placement {
        x: position.x,
        y: position.y,
        width: size.width,
        height: size.height,
        view: valid_view(view).into(),
    })
}
fn save_group(catalog: &mut Catalog, key: &str) {
    let placements: Vec<_> = catalog
        .active
        .values()
        .filter(|(ctx, _)| ctx.key == key)
        .take(LIMIT)
        .map(|(_, p)| p.clone())
        .collect();
    if !placements.is_empty() {
        catalog.saved.insert(key.into(), placements);
    }
}
pub fn record(window: &WebviewWindow) {
    let Some(state) = window.app_handle().try_state::<Windows>() else {
        return;
    };
    if state.quitting.load(Ordering::Relaxed) {
        return;
    }
    let context = state
        .catalog
        .lock()
        .unwrap()
        .active
        .get(window.label())
        .map(|(ctx, _)| ctx.clone());
    if let Some(ctx) = context {
        if let Some(p) = placement(window, &ctx.view) {
            let mut catalog = state.catalog.lock().unwrap();
            catalog
                .active
                .insert(window.label().into(), (ctx.clone(), p));
            save_group(&mut catalog, &ctx.key);
            let _ = state.save.send(());
        }
    }
}

fn server_title(title: &str, url: &Url, bundled: Option<&Url>) -> String {
    let title: String = title
        .chars()
        .filter(|c| !c.is_control())
        .take(160)
        .collect();
    if !matches!(url.scheme(), "http" | "https") {
        return title;
    }
    let server = if bundled.is_some_and(|local| local.origin() == url.origin()) {
        "bundled".to_owned()
    } else {
        let host = url.host_str().unwrap_or("");
        format!(
            "{}://{}:{}",
            url.scheme(),
            host,
            url.port_or_known_default().unwrap_or(0)
        )
    };
    let prefix = title.strip_suffix("pr0former").unwrap_or(&title);
    if title.ends_with("pr0former") {
        format!("{prefix}pr0former ({server})")
    } else if title.is_empty() {
        format!("pr0former ({server})")
    } else {
        format!("{title} — pr0former ({server})")
    }
}

pub fn loaded(window: WebviewWindow, title: String) {
    let app = window.app_handle();
    let local = app.try_state::<crate::BundledUrl>();
    if let Ok(url) = window.url() {
        let _ = window.set_title(&server_title(&title, &url, local.as_ref().map(|v| &v.0)));
    }
    let Some(state) = app.try_state::<Windows>() else {
        return;
    };
    let Some(ctx) = window
        .url()
        .ok()
        .and_then(|url| context(&url, local.as_ref().map(|v| &v.0)))
    else {
        return;
    };
    let Some(p) = placement(&window, &ctx.view) else {
        return;
    };
    let restore = {
        let mut catalog = state.catalog.lock().unwrap();
        let first = !catalog
            .active
            .values()
            .any(|(current, _)| current.key == ctx.key);
        if let Some((old, _)) = catalog.active.get(window.label()).cloned() {
            if old.key != ctx.key {
                save_group(&mut catalog, &old.key);
            }
        }
        let restore = if first {
            catalog.saved.get(&ctx.key).cloned()
        } else {
            None
        };
        catalog.active.insert(window.label().into(), (ctx, p));
        restore
    };
    if let Some(saved) = restore {
        if let Some(first) = saved.first() {
            let _ = apply(&window, first);
            let view = serde_json::to_string(valid_view(&first.view)).unwrap();
            let _ = window.eval(format!(
                "window.dispatchEvent(new CustomEvent('pr0-desktop-view',{{detail:{view}}}))"
            ));
        }
        for p in saved.iter().skip(1).take(LIMIT - 1) {
            if let Err(error) = duplicate(&window, Some(p.clone())) {
                eprintln!("Restore project window: {error}");
            }
        }
    }
    record(&window);
}

fn apply(window: &WebviewWindow, p: &Placement) -> tauri::Result<()> {
    let monitors = window.available_monitors()?;
    let monitor = monitors
        .iter()
        .find(|m| {
            let a = m.work_area();
            p.x >= a.position.x
                && p.y >= a.position.y
                && (p.x as i64) < a.position.x as i64 + a.size.width as i64
                && (p.y as i64) < a.position.y as i64 + a.size.height as i64
        })
        .or_else(|| monitors.first());
    if let Some(monitor) = monitor {
        let area = monitor.work_area();
        let width = p.width.clamp(320, area.size.width.max(320));
        let height = p
            .height
            .clamp(240, area.size.height.saturating_sub(40).max(240));
        let x = (p.x as i64).clamp(
            area.position.x as i64,
            area.position.x as i64 + area.size.width.saturating_sub(width) as i64,
        ) as i32;
        let y = (p.y as i64).clamp(
            area.position.y as i64,
            area.position.y as i64 + area.size.height.saturating_sub(height + 40) as i64,
        ) as i32;
        window.unmaximize()?;
        window.set_min_size(Some(PhysicalSize::new(320, 240)))?;
        window.set_size(PhysicalSize::new(width, height))?;
        window.set_position(PhysicalPosition::new(x, y))?;
    }
    Ok(())
}

fn duplicate(source: &WebviewWindow, saved: Option<Placement>) -> Result<(), String> {
    let app = source.app_handle();
    let state = app.state::<Windows>();
    let url = source.url().map_err(|e| e.to_string())?;
    let local = app.try_state::<crate::BundledUrl>();
    let ctx = context(&url, local.as_ref().map(|v| &v.0))
        .ok_or("Load a performance before opening another project window.")?;
    if state
        .catalog
        .lock()
        .unwrap()
        .active
        .values()
        .filter(|(c, _)| c.key == ctx.key)
        .count()
        >= LIMIT
    {
        return Err("A performance can have up to eight windows.".into());
    }
    let mut target = url.clone();
    let mut p = saved.unwrap_or_else(|| {
        let mut p = placement(source, &ctx.view).unwrap_or(Placement {
            x: 80,
            y: 80,
            width: 1000,
            height: 700,
            view: ctx.view.clone(),
        });
        p.x = p.x.saturating_add(32);
        p.y = p.y.saturating_add(32);
        p
    });
    p.view = valid_view(&p.view).into();
    let params: Vec<_> = target
        .query_pairs()
        .filter(|(k, _)| k != "view")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    target.set_query(None);
    target
        .query_pairs_mut()
        .extend_pairs(params)
        .append_pair("view", &p.view);
    // Explicitly copy only this server's cookies, never the bundled session to a remote origin.
    let cookies = source.cookies_for_url(url).map_err(|e| e.to_string())?;
    let label = format!("project-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let remote = !ctx.key.starts_with("bundled/");
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title("pr0former — loading performance")
        .inner_size(1000., 700.)
        .min_inner_size(320., 240.)
        .disable_drag_drop_handler()
        .incognito(remote)
        .initialization_script(initialization_script())
        .on_document_title_changed(loaded)
        .on_navigation(|url| matches!(url.scheme(), "http" | "https" | "tauri"))
        .build()
        .map_err(|e| e.to_string())?;
    state.catalog.lock().unwrap().active.insert(
        label,
        (
            Context {
                key: ctx.key,
                view: p.view.clone(),
            },
            p.clone(),
        ),
    );
    for cookie in cookies {
        window.set_cookie(cookie).map_err(|e| e.to_string())?;
    }
    apply(&window, &p).map_err(|e| e.to_string())?;
    crate::trust::install(&window, crate::trust::pin(app, &target))?;
    window.navigate(target).map_err(|e| e.to_string())?;
    Ok(())
}

fn documentation_url(source: &Url) -> Result<Url, String> {
    if !matches!(source.scheme(), "http" | "https") {
        return Err("Documentation is available after the server has started.".into());
    }
    let mut target = source.clone();
    target.set_path("/");
    target.set_query(None);
    target.set_fragment(None);
    target.query_pairs_mut().append_pair("help", "1");
    Ok(target)
}

pub fn help(app: &AppHandle) -> Result<(), String> {
    let source = app
        .webview_windows()
        .into_values()
        .find(|window| {
            window.is_focused().unwrap_or(false)
                && window
                    .url()
                    .is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
        })
        .or_else(|| {
            app.webview_windows().into_values().find(|window| {
                window
                    .url()
                    .is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
            })
        })
        .ok_or("Documentation is available after the server has started.")?;
    let source_url = source.url().map_err(|error| error.to_string())?;
    let target = documentation_url(&source_url)?;
    let local = app.try_state::<crate::BundledUrl>();
    let remote = !local
        .as_ref()
        .is_some_and(|url| url.0.origin() == target.origin());
    let cookies = source
        .cookies_for_url(source_url)
        .map_err(|error| error.to_string())?;
    let label = format!("documentation-{}", NEXT.fetch_add(1, Ordering::Relaxed));
    let window = WebviewWindowBuilder::new(app, &label, WebviewUrl::App("index.html".into()))
        .title(server_title(
            "Documentation — pr0former",
            &target,
            local.as_ref().map(|v| &v.0),
        ))
        .on_document_title_changed(loaded)
        .inner_size(1180., 820.)
        .min_inner_size(640., 480.)
        .disable_drag_drop_handler()
        .incognito(remote)
        .initialization_script(initialization_script())
        .on_navigation(|url| matches!(url.scheme(), "http" | "https" | "tauri"))
        .build()
        .map_err(|error| error.to_string())?;
    for cookie in cookies {
        window
            .set_cookie(cookie)
            .map_err(|error| error.to_string())?;
    }
    crate::trust::install(&window, crate::trust::pin(app, &target))?;
    window.navigate(target).map_err(|error| error.to_string())?;
    Ok(())
}

pub fn menu(app: &AppHandle, id: &str) -> Result<(), String> {
    let window = app
        .webview_windows()
        .into_values()
        .find(|w| w.is_focused().unwrap_or(false))
        .ok_or("Select a project window first.")?;
    if id == "new-project-window" {
        return duplicate(&window, None);
    }
    let state = app.state::<Windows>();
    let labels: Vec<_> = state
        .catalog
        .lock()
        .unwrap()
        .active
        .keys()
        .cloned()
        .collect();
    let windows: Vec<_> = labels
        .iter()
        .filter_map(|label| app.get_webview_window(label))
        .collect();
    let monitor = window
        .current_monitor()
        .map_err(|e| e.to_string())?
        .ok_or("No active display")?;
    let area = monitor.work_area();
    let columns = (windows.len() as f64).sqrt().ceil().max(1.) as u32;
    let rows = (windows.len() as u32).div_ceil(columns).max(1);
    for (index, w) in windows.iter().enumerate() {
        let outer = w.outer_size().map_err(|e| e.to_string())?;
        let inner = w.inner_size().map_err(|e| e.to_string())?;
        let p = Placement {
            x: area.position.x + (index as u32 % columns * area.size.width / columns) as i32,
            y: area.position.y + (index as u32 / columns * area.size.height / rows) as i32,
            width: (area.size.width / columns)
                .saturating_sub(outer.width.saturating_sub(inner.width)),
            height: (area.size.height / rows)
                .saturating_sub(outer.height.saturating_sub(inner.height)),
            view: "graph".into(),
        };
        w.unminimize().map_err(|e| e.to_string())?;
        apply(w, &p).map_err(|e| e.to_string())?;
        record(w);
    }
    Ok(())
}

pub fn destroyed(app: &AppHandle, label: &str) {
    let Some(state) = app.try_state::<Windows>() else {
        return;
    };
    if state.quitting.load(Ordering::Relaxed) {
        return;
    }
    let mut catalog = state.catalog.lock().unwrap();
    if let Some((ctx, _)) = catalog.active.remove(label) {
        save_group(&mut catalog, &ctx.key);
        let _ = state.save.send(());
    }
}
pub fn quitting(app: &AppHandle) {
    if let Some(state) = app.try_state::<Windows>() {
        state.quitting.store(true, Ordering::Relaxed);
        persist(&state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn titles_show_the_actual_server_after_the_app_name() {
        let local: Url = "http://127.0.0.1:8000".parse().unwrap();
        assert_eq!(
            server_title("Show — score — pr0former", &local, Some(&local)),
            "Show — score — pr0former (bundled)"
        );
        for (address, expected) in [
            ("https://studio.local", "https://studio.local:443"),
            ("https://192.168.1.2:8443", "https://192.168.1.2:8443"),
            ("http://127.0.0.1:9000", "http://127.0.0.1:9000"),
            ("https://[::1]:8443", "https://[::1]:8443"),
        ] {
            assert_eq!(
                server_title("Sign in", &address.parse().unwrap(), Some(&local)),
                format!("Sign in — pr0former ({expected})")
            );
        }
    }
    #[test]
    fn layout_keys_survive_local_port_changes_and_isolate_remote_servers() {
        let local: Url = "http://127.0.0.1:8000".parse().unwrap();
        assert_eq!(
            context(
                &"http://127.0.0.1:8000/?project=abc&view=score"
                    .parse()
                    .unwrap(),
                Some(&local)
            )
            .unwrap()
            .key,
            "bundled/abc"
        );
        assert_eq!(
            context(
                &"http://studio.local:8000/?project=abc".parse().unwrap(),
                Some(&local)
            )
            .unwrap()
            .key,
            "http://studio.local:8000/abc"
        );
        assert!(
            context(
                &"http://studio.local/?project=../bad".parse().unwrap(),
                None
            )
            .is_none()
        );
        assert_eq!(valid_view("javascript:alert(1)"), "graph");
    }
    #[test]
    fn documentation_url_keeps_the_server_but_drops_project_state() {
        let url: Url = "https://studio.local:8443/workspace?project=abc&view=score#selection"
            .parse()
            .unwrap();
        let documentation = documentation_url(&url).unwrap();
        assert_eq!(documentation.as_str(), "https://studio.local:8443/?help=1");
        assert!(documentation_url(&"tauri://localhost/index.html".parse().unwrap()).is_err());
    }
}
