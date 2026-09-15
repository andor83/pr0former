//! The pr0former application shell.
//!
//! Tauri 2 starts a mobile application from a library entry point, so the host
//! lives here and [`run`] is the single entry both platforms use: `src/main.rs`
//! is a desktop launcher that calls it, and the generated iOS entry point calls
//! the `#[tauri::mobile_entry_point]` wrapper around the same function.
//!
//! Everything that is a *desktop* affordance — the application menu, the
//! connection chooser, per-origin certificate pinning, multi-window layout
//! persistence, LAN discovery browsing, the hosting dialog, the exclusive
//! profile lock, and the bundled FFmpeg converter — is compiled only for
//! desktop targets. What both hosts share is [`runtime`]: one embedded
//! `pr0_runtime` started in process, its private loopback readiness, the
//! one-time session handoff that installs the private owner cookie, and one
//! ordered shutdown.
//!
//! See `docs/IPAD_RUNTIME_PLAN.md` for the host-adapter plan and the phase
//! gates this shell has and has not passed.
/// The iOS/iPadOS audio-session policy and hardware lifecycle bridge. Its
/// policy decisions compile everywhere so a build machine can test them; every
/// Objective-C call inside it is `cfg(target_os = "ios")`.
mod audio_session;
#[cfg(desktop)]
mod connections;
#[cfg(desktop)]
mod desktop;
#[cfg(desktop)]
mod discovery;
#[cfg(desktop)]
mod hosting;
#[cfg(mobile)]
mod mobile;
mod runtime;
#[cfg(desktop)]
mod trust;
#[cfg(desktop)]
mod windows;

/// The embedded runtime's private loopback origin, published once the engine is
/// ready. Window titles, layout keys and documentation windows use it to tell
/// the bundled engine apart from a remote server; it never carries the session.
///
/// The mobile shell publishes it too, but has no second window and no remote
/// origin to distinguish it from, so nothing reads it there yet.
#[cfg_attr(mobile, allow(dead_code))]
pub(crate) struct BundledUrl(pub(crate) tauri::Url);

/// Starts the application for this platform.
///
/// On iOS/iPadOS this is the function the generated entry point calls, so it
/// must not assume a desktop event loop, a menu bar, or more than one window.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(desktop)]
    desktop::run();
    #[cfg(mobile)]
    mobile::run();
}
