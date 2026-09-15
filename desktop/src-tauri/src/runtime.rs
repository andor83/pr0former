//! The pr0former application runtime, embedded in the application process.
//!
//! The launcher no longer starts a bundled `pr0-server` executable or speaks a
//! stdin/stdout protocol to it. It calls `pr0_runtime::start` with typed
//! configuration and keeps the returned handle as managed application state, so
//! hosting control and shutdown are ordinary method calls on the same runtime
//! every host uses (see `docs/IPAD_RUNTIME_PLAN.md`).
//!
//! What this preserves from the child-process launcher: the exclusive profile
//! lock held for the whole application lifetime, the private owner session
//! installed as an `HttpOnly; SameSite=Strict` cookie before the interface
//! loads, the private loopback URL, and a bounded wait for device closure and
//! the loop/recording writer barriers at exit.
//!
//! Both hosts share this module. The differences are narrow and explicit: the
//! profile lock and the bundled external converter are desktop-only, because a
//! second instance of a sandboxed mobile application cannot open the same
//! container and no executable may be referenced or bundled there; and iOS
//! installs the [`crate::audio_session`] policy before the runtime can open a
//! device, observes the system's audio and application lifecycle events for the
//! life of the process, and hands the session back after the ordered shutdown.
//!
//! How the session reaches the webview: not through the webview's own cookie
//! API. `WebviewWindow::set_cookie` aborts the process on iOS — `wry` 0.55's
//! WKWebView cookie helper waits for its reply by pumping the main run loop,
//! that re-enters Tao's Core Foundation control-flow observer, and the
//! resulting panic cannot unwind through an `extern "C"` callback. Instead the
//! runtime publishes a one-time, Host-guarded loopback URL
//! (`RuntimeHandle::bootstrap_url`); this host navigates to it once, the
//! runtime replies with the same cookie and a redirect to a clean path, and the
//! URL stops working. The long-lived session never passes through the host or
//! the webview layer at all. See `crates/server/src/runtime.rs` and
//! `docs/IPAD_RUNTIME_PLAN.md`.
#[cfg(desktop)]
use fs2::FileExt;
use pr0_runtime::{RuntimeConfig, RuntimeHandle};
use std::{
    fs::{File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tauri::{AppHandle, Manager, WebviewWindow};

/// How long quitting waits for the runtime's ordered shutdown — device closure
/// and the loop/recording writer barriers — before reporting that finalization
/// is still in progress. Unchanged from the child-process launcher's ceiling.
const SHUTDOWN_CEILING: Duration = Duration::from_secs(30);

/// The application's managed runtime state.
///
/// On desktop, holding it also keeps the profile's exclusive `desktop.lock` for
/// the process lifetime: the file is released only when Tauri drops this state
/// at exit, so a second launch still finds the profile locked. Mobile hosts run
/// one instance against one sandbox container, so they hold no lock.
pub struct Embedded {
    handle: RuntimeHandle,
    #[cfg(desktop)]
    _lock: File,
}

impl Embedded {
    /// A cloned control handle. Every clone drives the same runtime, so callers
    /// can await hosting or shutdown without holding Tauri's state borrow
    /// across an await point.
    pub fn handle(&self) -> RuntimeHandle {
        self.handle.clone()
    }
}

/// The running engine's control handle, or the reason there is not one yet.
///
/// Resolved synchronously and returned by value so that no Tauri state borrow
/// is ever held across an await point in an async command.
pub fn engine(app: &AppHandle) -> Result<RuntimeHandle, String> {
    app.try_state::<Embedded>()
        .map(|embedded| embedded.handle())
        .ok_or_else(|| "The bundled engine is still starting.".to_string())
}

#[cfg(desktop)]
fn bundled_executable(directory: &Path, name: &str) -> PathBuf {
    #[cfg(target_os = "windows")]
    {
        return directory.join(format!("{name}.exe"));
    }
    #[cfg(not(target_os = "windows"))]
    directory.join(name)
}

/// The bundled external converter beside the application executable.
#[cfg(desktop)]
fn bundled_converter(bin: &Path) -> PathBuf {
    bundled_executable(bin, "ffmpeg")
}

/// What every application host configures identically: a private loopback
/// runtime, profile-owned data and recordings roots, the bundled production
/// interface, and in-process audio import that needs no executable.
///
/// Inherited `PR0_*` server overrides cannot point an application at production
/// data any more, and nothing here has to remove or set them: the embedded
/// runtime reads no environment at all, so every filesystem root and capability
/// is supplied explicitly.
fn embedded_configuration(data: &Path, resources: &Path) -> RuntimeConfig {
    let mut config = RuntimeConfig::embedded(data.join("data"));
    config.recordings_dir = data.join("recordings");
    config.web_root = resources.join("resources/web");
    config.importer = pr0_runtime::import::pure_rust();
    config
}

/// The desktop profile's typed runtime configuration.
///
/// Desktop keeps the bundled external converter *behind* the in-process
/// decoder, and `PR0_DESKTOP_DISABLE_NATIVE_DEVICES` keeps its documented
/// desktop-only meaning — read here, never forwarded as a `PR0_*` variable.
#[cfg(desktop)]
fn configuration(data: &Path, resources: &Path) -> Result<RuntimeConfig, String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let bin = executable
        .parent()
        .ok_or("Missing application directory")?;
    let mut config = embedded_configuration(data, resources);
    // Audio import decodes in process first. FFmpeg remains the one external
    // binary beside the application executable, used only for the formats the
    // in-process decoder does not cover — never for a file it refused.
    config.importer = pr0_runtime::import::standard(Some(
        bundled_converter(bin).into_os_string(),
    ));
    config.native_devices =
        std::env::var("PR0_DESKTOP_DISABLE_NATIVE_DEVICES").as_deref() != Ok("1");
    Ok(config)
}

/// The iOS/iPadOS sandbox's typed runtime configuration.
///
/// Import stays exactly as [`embedded_configuration`] leaves it: in process,
/// with no external converter named, discovered, launched or bundled. Native
/// devices are enabled, so the runtime opens CoreAudio through CPAL; the audio
/// session category, preferred format, interruption handling and
/// foreground/background policy that surrounds it are in
/// [`crate::audio_session`], installed before this configuration can open a
/// device and never inside a device callback.
/// Runtime mDNS advertisement is off because this slice has no hosting control,
/// and asking for local-network permission before a feature uses it would be
/// both a review problem and a user-facing surprise.
#[cfg(mobile)]
fn configuration(data: &Path, resources: &Path) -> Result<RuntimeConfig, String> {
    let mut config = embedded_configuration(data, resources);
    config.native_devices = true;
    config.discovery = false;
    // The session policy is also a runtime capability, not only a launch-time
    // install: the recording category follows an actually enabled input, and a
    // performer can enable one while the application is running. The runtime
    // calls this immediately before it opens a device, with the settings it is
    // about to use — see `crate::audio_session::policy`.
    #[cfg(target_os = "ios")]
    {
        config.audio_policy = Some(crate::audio_session::policy());
        // The route capability. Without it the runtime would have to ask CPAL
        // what the platform's devices support, and CPAL's iOS backend answers
        // that by initializing a RemoteIO audio unit — the call AudioToolbox
        // aborts the process from when its RPC times out. With it, the runtime
        // presents two stable logical routes, resolves them to the system
        // default, and refuses capture with an actionable message instead of
        // opening a microphone this application may not use. See
        // `crate::audio_session::routes` and `docs/IPAD_RUNTIME_PLAN.md`.
        config.audio_routes = Some(crate::audio_session::routes());
    }
    Ok(config)
}

/// A `MakeWriter` target that writes to the already open profile log.
struct Log(Arc<File>);
impl Write for Log {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut file: &File = &self.0;
        file.write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        let mut file: &File = &self.0;
        file.flush()
    }
}

fn log_path(data: &Path) -> PathBuf {
    data.join("desktop-server.log")
}

/// Sends runtime diagnostics to the profile log, as the child process used to
/// do through its inherited stderr. Startup failures are still reported in the
/// window; this file keeps the detail behind them. Logging is best effort: an
/// unwritable profile log must not stop the application from starting.
///
/// Called only after the profile lock is held, so a second launch that is
/// refused the lock cannot truncate the running application's log.
fn start_logging(data: &Path) {
    let Ok(file) = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(log_path(data))
    else {
        return;
    };
    let file = Arc::new(file);
    let _ = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_writer(move || Log(file.clone()))
        .try_init();
}

/// Takes the profile's exclusive lock. The lock is released only by dropping
/// the returned file, which the managed [`Embedded`] state does at exit.
#[cfg(desktop)]
fn lock_profile(data: &Path) -> Result<File, String> {
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(data.join("desktop.lock"))
        .map_err(|e| e.to_string())?;
    lock.try_lock_exclusive()
        .map_err(|_| "pr0former is already running. Switch to its existing window.".to_string())?;
    Ok(lock)
}

/// Starts the embedded runtime for `data`, publishes it (and, on desktop, the
/// profile lock) as application state, and navigates `window` once to the
/// runtime's one-time session handoff, which installs the private session
/// cookie and redirects to the interface.
///
/// A runtime that starts but reports unusable readiness is shut down here and
/// the profile lock is released with it, so a relaunch can take both again.
/// Once the runtime is published, a later navigation failure is reported in the
/// window without stopping the engine — on desktop the Server menu's **Bundled
/// Engine** item can still open a window onto it.
pub async fn start(
    app: AppHandle,
    data: PathBuf,
    resources: PathBuf,
    window: WebviewWindow,
) -> Result<(), String> {
    // Only a desktop profile can be opened by a second launch; a sandboxed
    // mobile application owns its container alone.
    #[cfg(desktop)]
    let lock = {
        let profile = data.clone();
        tauri::async_runtime::spawn_blocking(move || lock_profile(&profile))
            .await
            .map_err(|e| e.to_string())??
    };
    start_logging(&data);
    let config = configuration(&data, &resources)?;
    // iOS owns the audio hardware: the session category, mode and driver
    // preferences have to be installed before CoreAudio opens a stream, and the
    // observers that suspend and resume it need the runtime's own saved audio
    // settings to re-read (see `audio_session`).
    #[cfg(target_os = "ios")]
    let audio = config.clone();
    #[cfg(target_os = "ios")]
    crate::audio_session::prepare(&audio);
    let runtime = pr0_runtime::start(config).await.map_err(|error| {
        format!(
            "The bundled engine did not start: {error}. See {}",
            log_path(&data).display()
        )
    })?;
    let (url, handoff) = match private_readiness(&runtime) {
        Ok(ready) => ready,
        Err(error) => {
            let _ = runtime.shutdown().await;
            return Err(error);
        }
    };
    #[cfg(target_os = "ios")]
    let lifecycle = runtime.clone();
    app.manage(Embedded {
        handle: runtime,
        #[cfg(desktop)]
        _lock: lock,
    });
    app.manage(crate::BundledUrl(url.clone()));
    // Installed only once the runtime exists, so an interruption or background
    // event never reaches a handle that is still starting. Every transition is
    // idempotent, so the events that arrive before the engine is enabled cost a
    // comparison and close nothing.
    #[cfg(target_os = "ios")]
    crate::audio_session::install(lifecycle, audio);
    // `navigate` waits on the window's event loop, so it runs on a blocking
    // thread rather than an async worker.
    tauri::async_runtime::spawn_blocking(move || bootstrap(&window, handoff))
        .await
        .map_err(|e| e.to_string())?
}

/// The same shape the child-process handshake was checked for — a private
/// loopback listener on an OS-selected port, with a full-length private session
/// — plus the one-time handoff this host navigates to.
fn private_readiness(runtime: &RuntimeHandle) -> Result<(tauri::Url, tauri::Url), String> {
    let ready = runtime.readiness();
    validate_readiness(
        &ready.url,
        ready.session.as_deref(),
        runtime.bootstrap_url().as_deref(),
    )
}

/// The readiness contract itself, separated from the handle so every host's
/// expectations are checked by one testable function: nothing but an ephemeral
/// `http://127.0.0.1:<port>` listener, with a full-length private session and a
/// one-time handoff on that very origin, is accepted — on desktop or on iOS.
///
/// The session is checked, never carried: this host keeps the loopback origin
/// and the handoff, and the credential itself stays inside the runtime. A
/// handoff on any other origin, or one carrying a query string, is refused
/// rather than navigated to.
fn validate_readiness(
    url: &str,
    session: Option<&str>,
    handoff: Option<&str>,
) -> Result<(tauri::Url, tauri::Url), String> {
    let full_length_session = session
        .ok_or("The bundled engine produced no private session")?
        .len()
        == 72;
    let url: tauri::Url = url.parse().map_err(|_| "Invalid engine URL")?;
    let handoff: tauri::Url = handoff
        .ok_or("The bundled engine produced no session handoff")?
        .parse()
        .map_err(|_| "Invalid engine session handoff")?;
    let secret = handoff
        .path_segments()
        .and_then(|mut segments| segments.next_back())
        .unwrap_or_default();
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || url.port().is_none()
        || !full_length_session
        || handoff.origin() != url.origin()
        || handoff.query().is_some()
        || secret.len() != 72
    {
        return Err("Invalid bundled engine readiness".into());
    }
    Ok((url, handoff))
}

/// Opens the interface by redeeming the runtime's one-time session handoff.
///
/// One navigation, and nothing else: the runtime answers it with the private
/// owner session as an `HttpOnly; SameSite=Strict` cookie plus a redirect to
/// `/`, so the credential is installed by the same origin that issues login
/// cookies and the single-use URL is not the address the interface ends up at.
///
/// Deliberately not `WebviewWindow::set_cookie`: that call aborts the process
/// on iOS (see this module's documentation), and routing the session through
/// the webview API put a long-lived credential in the host's hands for no
/// benefit on any platform.
fn bootstrap(window: &WebviewWindow, handoff: tauri::Url) -> Result<(), String> {
    // The reported failure deliberately omits the URL. A navigation that did not
    // happen leaves the handoff unredeemed and briefly still live, and this
    // string is shown in the application window; there is no reason for a
    // redeemable credential to appear in it.
    window
        .navigate(handoff)
        .map_err(|_| "The bundled engine interface could not be opened.".to_string())
}

/// Stops the embedded runtime at application exit.
///
/// The runtime's own ordered sequence runs — listeners, then LAN hosting and
/// mDNS, then device closure and the loop/recording writer barriers, then the
/// private session — and the wait is bounded exactly as the child-process
/// launcher's was. Nothing is killed: there is no process to terminate, so an
/// overrunning shutdown is reported rather than forced.
pub fn stop(app: &AppHandle) {
    // Nothing to stop if startup never published a runtime.
    let Ok(handle) = engine(app) else {
        return;
    };
    let stopped = tauri::async_runtime::block_on(async move {
        tokio::time::timeout(SHUTDOWN_CEILING, handle.shutdown()).await
    });
    match stopped {
        Ok(Ok(())) => (),
        Ok(Err(error)) => eprintln!("Engine shutdown failed: {error}"),
        Err(_) => eprintln!(
            "The engine did not finish shutting down within {} seconds; recordings and loops may be incomplete",
            SHUTDOWN_CEILING.as_secs()
        ),
    }
    // Ordered after the runtime's own shutdown: the devices are already closed,
    // so handing the session back cannot interrupt a stream that is still
    // draining, and whatever this application interrupted may resume.
    #[cfg(target_os = "ios")]
    crate::audio_session::release();
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch(PathBuf);
    impl Scratch {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!(
                "pr0-desktop-{label}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&path);
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// Checked on every platform: whatever host starts the runtime, it is a
    /// private loopback runtime whose roots stay inside the container it was
    /// given, and its baseline import capability needs no executable at all.
    #[test]
    fn every_host_shares_private_roots_and_in_process_import() {
        let scratch = Scratch::new("embedded-configuration");
        let data = scratch.0.join("container");
        let resources = scratch.0.join("Resources");
        let config = embedded_configuration(&data, &resources);
        assert!(config.is_embedded());
        assert_eq!(config.data_dir, data.join("data"));
        assert_eq!(config.recordings_dir, data.join("recordings"));
        assert_eq!(config.web_root(), resources.join("resources/web"));
        assert_eq!(config.database_path(), data.join("data/pr0former.sqlite"));
        assert!(config.listener.bind.is_none() && config.listener.tls.is_none());
        // The shared baseline is pure Rust. Only the desktop host adds an
        // external converter behind it.
        assert_eq!(config.importer().name(), "symphonia");
    }

    /// Readiness is validated identically on both hosts, so an engine that came
    /// up on anything but its own ephemeral loopback port, or without a
    /// full-length private session, is refused before the host navigates
    /// anywhere.
    #[test]
    fn only_ephemeral_private_loopback_readiness_is_accepted() {
        let session = "s".repeat(72);
        let handoff = format!("http://127.0.0.1:52341/__session-bootstrap/{}", "t".repeat(72));
        let (url, redeemed) = validate_readiness(
            "http://127.0.0.1:52341",
            Some(&session),
            Some(&handoff),
        )
        .unwrap();
        assert_eq!(url.port(), Some(52341));
        assert_eq!(redeemed.as_str(), handoff);
        assert!(validate_readiness("http://127.0.0.1:52341", None, Some(&handoff)).is_err());
        for refused in [
            // No port: not the runtime's own ephemeral listener.
            "http://127.0.0.1",
            // Not loopback, or not the runtime's own scheme.
            "http://0.0.0.0:52341",
            "http://localhost:52341",
            "https://127.0.0.1:52341",
            "http://[::1]:52341",
            "not a url",
        ] {
            assert!(
                validate_readiness(refused, Some(&session), Some(&handoff)).is_err(),
                "{refused}"
            );
        }
        assert!(
            validate_readiness("http://127.0.0.1:52341", Some("short"), Some(&handoff)).is_err()
        );
    }

    /// The host navigates to the handoff, so it refuses anything that is not a
    /// full-length single-use path on the engine's own private origin: a
    /// credential must never be presented to another origin, and a handoff that
    /// arrived as a query string is not the contract this host implements.
    #[test]
    fn a_session_handoff_is_refused_unless_it_is_on_the_engines_own_origin() {
        let session = "s".repeat(72);
        let secret = "t".repeat(72);
        let url = "http://127.0.0.1:52341";
        assert!(validate_readiness(url, Some(&session), None).is_err());
        for refused in [
            // A different port, host, or scheme is a different origin.
            format!("http://127.0.0.1:52342/__session-bootstrap/{secret}"),
            format!("http://localhost:52341/__session-bootstrap/{secret}"),
            format!("https://127.0.0.1:52341/__session-bootstrap/{secret}"),
            // A credential in a query string is explicitly not the contract.
            format!("http://127.0.0.1:52341/?session={secret}"),
            // Nothing single-use about it.
            format!("http://127.0.0.1:52341/__session-bootstrap/{secret}?x=1"),
            "http://127.0.0.1:52341/".into(),
            "http://127.0.0.1:52341/__session-bootstrap/short".into(),
            "not a url".into(),
        ] {
            assert!(
                validate_readiness(url, Some(&session), Some(&refused)).is_err(),
                "{refused}"
            );
        }
    }


    /// The iOS/iPadOS host names no converter, so nothing on that platform can
    /// reference or bundle an executable; it opens native devices and keeps the
    /// runtime's own mDNS advertisement off.
    #[cfg(mobile)]
    #[test]
    fn the_mobile_sandbox_configuration_has_no_external_converter() {
        let scratch = Scratch::new("mobile-configuration");
        let data = scratch.0.join("Application Support");
        let resources = scratch.0.join("pr0former.app");
        let config = configuration(&data, &resources).unwrap();
        assert!(config.is_embedded());
        assert_eq!(config.data_dir, data.join("data"));
        assert_eq!(config.recordings_dir, data.join("recordings"));
        assert_eq!(config.web_root(), resources.join("resources/web"));
        assert_eq!(config.importer().name(), "symphonia");
        assert!(config.native_devices);
        assert!(!config.discovery);
        // iOS installs the audio-session policy as a runtime capability, so the
        // category is reconsidered whenever a device is opened rather than only
        // at launch.
        #[cfg(target_os = "ios")]
        assert!(config.audio_policy.is_some());
    }

    #[cfg(desktop)]
    #[test]
    fn desktop_roots_stay_inside_the_profile_and_resources() {
        let scratch = Scratch::new("configuration");
        let data = scratch.0.join("profile");
        let resources = scratch.0.join("Resources");
        let config = configuration(&data, &resources).unwrap();
        assert!(config.is_embedded());
        assert_eq!(config.data_dir, data.join("data"));
        assert_eq!(config.recordings_dir, data.join("recordings"));
        assert_eq!(config.web_root(), resources.join("resources/web"));
        assert_eq!(config.database_path(), data.join("data/pr0former.sqlite"));
        // Embedded runtimes bind private loopback, so no listener policy or TLS
        // pair can be inherited from a developer's server environment.
        assert!(config.listener.bind.is_none() && config.listener.tls.is_none());
        // Import decodes in process first and only then reaches the bundled
        // converter, so a format the in-process decoder covers never launches
        // a child process.
        assert_eq!(config.importer().name(), "symphonia+ffmpeg");
        // Desktop platforms need no session policy, and must not acquire one.
        assert!(config.audio_policy.is_none());
        // The desktop keeps FFmpeg as an external binary beside the executable.
        let bin = std::env::current_exe().unwrap();
        let ffmpeg = bundled_converter(bin.parent().unwrap());
        assert!(ffmpeg.is_absolute(), "{}", ffmpeg.display());
        assert!(
            ffmpeg
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("ffmpeg")
        );
    }

    #[cfg(desktop)]
    #[test]
    fn the_profile_lock_is_exclusive_and_outlives_only_its_holder() {
        let scratch = Scratch::new("lock");
        let first = lock_profile(&scratch.0).unwrap();
        assert!(
            lock_profile(&scratch.0)
                .unwrap_err()
                .contains("already running")
        );
        drop(first);
        // A leftover lock file after a crash never blocks the next launch.
        lock_profile(&scratch.0).unwrap();
    }
}
