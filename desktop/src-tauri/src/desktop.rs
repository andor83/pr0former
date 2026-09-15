//! The macOS/Linux/Windows host.
//!
//! Unchanged desktop behavior: the Server/Window/Help menus, the connection
//! chooser, LAN discovery browsing, the hosting dialog, saved multi-window
//! layouts, and the bundled engine window. None of it is compiled for mobile
//! targets, which have no menu bar and exactly one window.
use crate::BundledUrl;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

pub(crate) struct DiscoveryMenu(pub(crate) tauri::menu::Submenu<tauri::Wry>);

pub fn run() {
    let app = tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![crate::connections::connect_server, crate::hosting::hosting_control])
        .menu(|app| {
            use tauri::menu::{Menu, MenuItem, Submenu};
            let menu = Menu::default(app)?;
            let connect = MenuItem::with_id(
                app,
                "connect-server",
                "Connect to Server…",
                true,
                Some("CmdOrCtrl+Shift+K"),
            )?;
            let local =
                MenuItem::with_id(app, "bundled-engine", "Bundled Engine", true, None::<&str>)?;
            let host = MenuItem::with_id(app,"host-performance","Host on Local Network…",true,None::<&str>)?;
            let discovered = Submenu::with_items(app, "Local Network", true,
                &[&MenuItem::new(app, "Searching for servers…", false, None::<&str>)?])?;
            app.manage(DiscoveryMenu(discovered.clone()));
            menu.append(&Submenu::with_items(
                app,
                "Server",
                true,
                &[&connect, &local, &host, &discovered],
            )?)?;
            if let Some(window_menu) = menu.items()?.into_iter().find_map(|item| match item {
                tauri::menu::MenuItemKind::Submenu(menu) if menu.text().ok().as_deref() == Some("Window") => Some(menu),
                _ => None,
            }) {
                window_menu.append(&MenuItem::with_id(app, "new-project-window", "New Window for This Performance", true, Some("CmdOrCtrl+Shift+N"))?)?;
                window_menu.append(&MenuItem::with_id(app, "tile-project-windows", "Tile Windows", true, None::<&str>)?)?;
            }
            let documentation = MenuItem::with_id(
                app,
                "documentation",
                "pr0former Help",
                true,
                None::<&str>,
            )?;
            if let Some(help_menu) = menu.items()?.into_iter().find_map(|item| match item {
                tauri::menu::MenuItemKind::Submenu(menu) if menu.text().ok().as_deref() == Some("Help") => Some(menu),
                _ => None,
            }) {
                help_menu.append(&documentation)?;
            } else {
                menu.append(&Submenu::with_items(app, "Help", true, &[&documentation])?)?;
            }
            Ok(menu)
        })
        .on_menu_event(|app, event| {
            let result = match event.id().as_ref() {
                "connect-server" => crate::connections::show(app),
                "host-performance" => crate::hosting::show(app),
                "new-project-window" | "tile-project-windows" => {
                    if let Err(error) = crate::windows::menu(app, event.id().as_ref()) { eprintln!("Window menu: {error}"); }
                    Ok(())
                }
                "documentation" => {
                    if let Err(error) = crate::windows::help(app) { eprintln!("Help menu: {error}"); }
                    Ok(())
                }
                "bundled-engine" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.unminimize();
                        window.set_focus()
                    } else if let Some(url) = app.try_state::<BundledUrl>() {
                        WebviewWindowBuilder::new(app, "main", WebviewUrl::External(url.0.clone()))
                            .title("pr0former")
                            .inner_size(1440., 960.)
                            .min_inner_size(640., 480.)
                            .disable_drag_drop_handler()
                            .initialization_script(crate::windows::initialization_script())
                            .on_document_title_changed(crate::windows::loaded)
                            .build()
                            .map(|_| ())
                    } else {
                        Ok(())
                    }
                }
                id => if let Some(address) = crate::discovery::address(app, id) {
                    crate::connections::show_address(app, Some(address))
                } else { Ok(()) },
            };
            if let Err(error) = result {
                eprintln!("Server menu: {error}");
            }
        })
        .setup(|app| {
            if let Err(error) = crate::discovery::start(app.handle(), app.state::<DiscoveryMenu>().0.clone()) {
                eprintln!("LAN discovery: {error}");
                let menu = &app.state::<DiscoveryMenu>().0;
                for item in menu.items()? { menu.remove(&item)?; }
                menu.append(&tauri::menu::MenuItem::new(app, "Discovery unavailable — connect manually", false, None::<&str>)?)?;
            }
            let window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .title("pr0former")
                    .inner_size(1440., 960.)
                    .min_inner_size(640., 480.)
                    .disable_drag_drop_handler()
                    .initialization_script(crate::windows::initialization_script())
                    .on_document_title_changed(crate::windows::loaded)
                    .build()?;
            let data = std::env::var_os("PR0_DESKTOP_DATA")
                .map(std::path::PathBuf::from)
                .unwrap_or(app.path().app_data_dir()?);
            if !data.is_absolute() {
                return Err("PR0_DESKTOP_DATA must be an absolute directory".into());
            }
            std::fs::create_dir_all(&data)?;
            crate::windows::start(app.handle(), data.join("window-layouts.json"));
            crate::trust::start(app.handle(), data.join("trusted-servers.json"));
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o700))?;
            }
            let resources = app.path().resource_dir()?;
            let handle = app.handle().clone();
            let reporter = window.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = crate::runtime::start(handle, data, resources, window).await {
                    let text = serde_json::to_string(&error).unwrap();
                    let _ = reporter.eval(format!(
                        "document.getElementById('status').textContent={text}"
                    ));
                }
            });
            Ok(())
        })
        .on_window_event(|window, event| {
            match event {
                tauri::WindowEvent::Moved(_) | tauri::WindowEvent::Resized(_) | tauri::WindowEvent::CloseRequested { .. } => {
                    if let Some(webview) = window.app_handle().get_webview_window(window.label()) { crate::windows::record(&webview); }
                }
                tauri::WindowEvent::Destroyed => crate::windows::destroyed(window.app_handle(), window.label()),
                _ => (),
            }
        })
        .build(tauri::generate_context!())
        .expect("Build pr0former desktop");
    app.run(|app, event| {
        if let tauri::RunEvent::ExitRequested { .. } = &event { crate::windows::quitting(app); }
        if let tauri::RunEvent::Exit = event {
            // Discovery browsing stops first, then the runtime runs its own
            // ordered shutdown: listeners, hosting and mDNS, device closure and
            // the loop/recording writer barriers, then the private session.
            if let Some(discovery) = app.try_state::<crate::discovery::Discovery>() { discovery.stop(); }
            crate::runtime::stop(app);
        }
    });
}
