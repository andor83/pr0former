use mdns_sd::{ResolvedService, ServiceDaemon, ServiceEvent};
use std::{
    collections::BTreeMap,
    hash::{Hash, Hasher},
    sync::Mutex,
};
use tauri::{
    AppHandle, Manager,
    menu::{MenuItem, Submenu},
};

const SERVICE: &str = "_pr0former._tcp.local.";
pub struct Discovery {
    daemon: ServiceDaemon,
    entries: Mutex<BTreeMap<String, (String, String)>>,
    menu: Submenu<tauri::Wry>,
}
impl Discovery {
    pub fn stop(&self) {
        let _ = self.daemon.stop_browse(SERVICE);
        let _ = self.daemon.shutdown();
    }
}

fn endpoint(info: &ResolvedService) -> Option<(String, String)> {
    if info.ty_domain != SERVICE
        || info.get_property_val_str("version") != Some("1")
        || info.port == 0
        || info.addresses.is_empty()
    {
        return None;
    }
    let scheme = info.get_property_val_str("scheme")?;
    if !matches!(scheme, "http" | "https") {
        return None;
    }
    let host = info.host.trim_end_matches('.');
    if !host.ends_with(".local")
        || host.len() > 253
        || !host
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'.'))
    {
        return None;
    }
    let url = crate::connections::server_url(&format!("{scheme}://{host}:{}", info.port)).ok()?;
    let name: String = info
        .fullname
        .strip_suffix(&format!(".{SERVICE}"))?
        .chars()
        .filter(|c| !c.is_control())
        .take(80)
        .collect();
    Some((name.replace('&', "&&"), url.to_string()))
}

pub fn start(app: &AppHandle, menu: Submenu<tauri::Wry>) -> Result<(), String> {
    let daemon = ServiceDaemon::new().map_err(|e| e.to_string())?;
    let receiver = match daemon.browse(SERVICE) {
        Ok(receiver) => receiver,
        Err(error) => {
            let _ = daemon.shutdown();
            return Err(error.to_string());
        }
    };
    app.manage(Discovery {
        daemon,
        entries: Mutex::new(BTreeMap::new()),
        menu,
    });
    let app = app.clone();
    std::thread::spawn(move || {
        while let Ok(event) = receiver.recv() {
            let state = app.state::<Discovery>();
            let mut entries = state.entries.lock().unwrap();
            let changed = match event {
                ServiceEvent::ServiceResolved(info) => {
                    if let Some(value) = endpoint(&info) {
                        if entries.len() < 32 || entries.contains_key(&info.fullname) {
                            entries.insert(info.fullname.clone(), value.clone()) != Some(value)
                        } else {
                            false
                        }
                    } else {
                        entries.remove(&info.fullname).is_some()
                    }
                }
                ServiceEvent::ServiceRemoved(_, fullname) => entries.remove(&fullname).is_some(),
                _ => false,
            };
            drop(entries);
            if changed {
                let _ = refresh(&app);
            }
        }
    });
    Ok(())
}

fn menu_id(fullname: &str) -> String {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    fullname.hash(&mut hash);
    format!("discovered-{:x}", hash.finish())
}
fn refresh(app: &AppHandle) -> tauri::Result<()> {
    let state = app.state::<Discovery>();
    let entries = state.entries.lock().unwrap().clone();
    for item in state.menu.items()? {
        state.menu.remove(&item)?;
    }
    if entries.is_empty() {
        state.menu.append(&MenuItem::new(
            app,
            "No servers detected",
            false,
            None::<&str>,
        )?)?;
    }
    for (key, (name, url)) in entries {
        state.menu.append(&MenuItem::with_id(
            app,
            menu_id(&key),
            format!("{name} — {url}"),
            true,
            None::<&str>,
        )?)?;
    }
    Ok(())
}
pub fn address(app: &AppHandle, id: &str) -> Option<String> {
    let state = app.try_state::<Discovery>()?;
    let entries = state.entries.lock().ok()?;
    entries
        .iter()
        .find(|(key, _)| menu_id(key) == id)
        .map(|(_, (_, url))| url.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn service() -> ResolvedService {
        mdns_sd::ServiceInfo::new(
            SERVICE,
            "Studio",
            "studio.local.",
            "192.168.1.3",
            8443,
            &[("scheme", "https"), ("version", "1")][..],
        )
        .unwrap()
        .as_resolved_service()
    }
    #[test]
    fn accepts_only_supported_local_service_endpoints() {
        let mut info = service();
        assert_eq!(endpoint(&info).unwrap().1, "https://studio.local:8443/");
        info.host = "evil.example".into();
        assert!(endpoint(&info).is_none());
        info.host = "user@studio.local".into();
        assert!(endpoint(&info).is_none());
        info = service();
        info.port = 0;
        assert!(endpoint(&info).is_none());
    }
}
