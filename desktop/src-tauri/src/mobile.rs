//! The iOS/iPadOS host.
//!
//! This is the first mobile slice described in `docs/IPAD_RUNTIME_PLAN.md`
//! Phase 4: a single-webview shell around the same embedded `pr0_runtime` the
//! desktop application runs. Exactly what it does, in order:
//!
//! 1. creates the one `main` webview on the bundled shell page, which shows
//!    startup progress and any startup failure;
//! 2. derives the writable application-sandbox roots and the read-only bundled
//!    web assets from Tauri's path APIs — never from the working directory, an
//!    environment variable, or a path beside the executable;
//! 3. builds `RuntimeConfig::embedded` with native devices enabled, runtime
//!    mDNS advertisement off, and `import::pure_rust()`, so no external
//!    converter is referenced, discovered, launched or bundled;
//! 4. installs the `AVAudioSession` policy before the runtime can open a
//!    device, and observes interruptions, route loss, media-services resets and
//!    foreground/background transitions, mapping each onto the runtime's
//!    idempotent audio suspend/resume (see [`crate::audio_session`]);
//! 5. starts the runtime in process, validates its private loopback readiness,
//!    and navigates the webview once to the runtime's one-time session handoff,
//!    which answers with the private owner session as an HttpOnly/SameSite=Strict
//!    cookie and a redirect to the interface — the webview's own cookie API is
//!    never used, because that call aborts the process on iOS and because the
//!    long-lived session then never leaves the runtime; and
//! 6. at application exit, runs `RuntimeHandle::shutdown` — the same ordered
//!    sequence as the desktop host, with no process API involved — and then
//!    hands the audio session back.
//!
//! Deliberately absent, and desktop-only: the application menu, the connection
//! chooser, certificate pinning, saved window layouts, LAN discovery browsing,
//! the hosting dialog, the exclusive profile lock, and every Tauri command.
//!
//! Deliberately not implemented: any background or screen-lock audio policy.
//! There is no background-audio entitlement, so leaving the foreground closes
//! the hardware. None of the audio lifecycle behavior has been exercised on a
//! device or in a simulator, so nothing here is a claim about how iPad audio
//! actually behaves.
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            // Mobile hosts get exactly one webview, and no desktop window
            // geometry, drag-and-drop policy or title handling applies to it.
            let window =
                WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                    .build()?;
            // The application sandbox container, which is the only writable
            // location on this platform. Created here because a fresh install
            // has no Application Support directory yet.
            let data = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data)?;
            // The read-only application bundle, holding the production Vue
            // interface the embedded runtime serves.
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
        .build(tauri::generate_context!())
        .expect("Build pr0former for iOS")
        .run(|app, event| {
            // iOS may terminate a suspended application without delivering this
            // event, so it is the orderly path, not a guarantee. Recording and
            // loop durability across a forced termination is part of the Phase 4
            // simulator gate and is unverified.
            if let tauri::RunEvent::Exit = event {
                crate::runtime::stop(app);
            }
        });
}
