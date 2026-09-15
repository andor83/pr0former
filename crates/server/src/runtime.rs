//! The embeddable runtime lifecycle: start, readiness, local-network hosting
//! control, and one ordered shutdown shared by every host.
//!
//! Nothing in this module reads command-line arguments or `PR0_*` variables,
//! writes process-global state, or touches stdin/stdout. Hosts that need those
//! — the `pr0-server` process host in [`crate::run`] and the legacy
//! `--desktop` protocol adapter in `crate::desktop` — sit above this boundary
//! and drive the same API a future in-process host will use.
use crate::*;
use rusqlite::OptionalExtension;
use std::{
    net::SocketAddr,
    sync::atomic::{AtomicBool, Ordering},
    time::Instant,
};
use tokio::sync::watch;

/// What a host learns once a runtime is listening.
#[derive(Clone)]
pub struct Readiness {
    /// The runtime's own listener. Embedded runtimes report an ephemeral
    /// `http://127.0.0.1:<port>`; standalone servers report their configured
    /// address and scheme.
    pub url: String,
    /// The private owner session. Present only for embedded runtimes, where it
    /// is delivered to the owning host in process rather than over the network,
    /// and revoked by [`RuntimeHandle::shutdown`].
    pub session: Option<String>,
    /// The plain-HTTP redirect listener that accompanies a standalone HTTPS
    /// listener, when one was started.
    pub redirect_url: Option<String>,
}

/// Redacts the private session: readiness is routinely logged by hosts, and the
/// owner credential must never reach a log line.
impl std::fmt::Debug for Readiness {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Readiness")
            .field("url", &self.url)
            .field("session", &self.session.as_ref().map(|_| "<redacted>"))
            .field("redirect_url", &self.redirect_url)
            .finish()
    }
}

/// A started runtime. Cheap to clone; every clone controls the same runtime.
#[derive(Clone)]
pub struct RuntimeHandle(Arc<Runtime>);

struct Runtime {
    config: Arc<RuntimeConfig>,
    app: App,
    /// The router *without* the private host-header guard: what local-network
    /// hosting serves, and never the private listener's own application.
    shared_router: Router,
    readiness: Readiness,
    /// Asks the runtime's own listeners to stop.
    stop: watch::Sender<bool>,
    /// Set by the listener task once it has stopped.
    finished: Arc<watch::Sender<bool>>,
    serve_result: Arc<Mutex<Option<Result<(), String>>>>,
    discovery: Mutex<Option<discovery::Advertisement>>,
    hosting: tokio::sync::Mutex<Option<hosting::Hosting>>,
    /// `Some` once shutdown has run; repeat calls replay its outcome.
    shutdown: tokio::sync::Mutex<Option<Result<(), String>>>,
    /// The one-time session handoff for an embedded runtime's own webview.
    /// `None` for a standalone server, which has no webview and no private
    /// session to hand anywhere.
    bootstrap: Option<Arc<Bootstrap>>,
    /// Whether the host has asked for native audio hardware to be closed. A
    /// mirror of the orchestration worker's own state, published so a host can
    /// read it without a round trip; the worker remains authoritative.
    audio_suspended: AtomicBool,
    /// Serializes suspend/resume so an interruption and a foreground/background
    /// event arriving on different threads cannot interleave. A plain mutex,
    /// never held across an await, because hosts complete these transitions on
    /// operating-system callback threads with no async runtime.
    audio_lifecycle: Mutex<()>,
    /// Set before shutdown closes the devices, so a lifecycle event racing
    /// application exit is answered immediately instead of waiting for a worker
    /// that will never reply.
    audio_stopped: AtomicBool,
}

/// How long [`RuntimeHandle::suspend_audio`] and its siblings wait for the
/// orchestration worker to acknowledge a transition. The worker answers between
/// DSP blocks, so this expires only when it is wedged — and an iOS host is
/// holding a lifecycle callback while it waits.
pub const AUDIO_TRANSITION_TIMEOUT: Duration = Duration::from_secs(5);

/// Where an embedded runtime answers its own webview's one-time session
/// handoff. Under `/api/` would put it behind nothing extra, but keeping it out
/// of the API namespace makes it obvious in a route list that it is not part of
/// the application protocol.
const BOOTSTRAP_PREFIX: &str = "/__session-bootstrap/";

/// How long the handoff stays redeemable. The host navigates to it within
/// milliseconds of [`start`] returning; this only has to survive a slow first
/// webview, and every second past that is a second in which a URL that can
/// produce the owner session still exists.
const BOOTSTRAP_LIFETIME: Duration = Duration::from_secs(120);

/// How many presented tokens may be wrong before the handoff is destroyed. The
/// token is unguessable and the listener is a Host-guarded ephemeral loopback
/// port, so this is a backstop, not the defence: it bounds anything that does
/// reach the route into a handful of tries.
const BOOTSTRAP_ATTEMPTS: u8 = 8;

/// The one-time HTTP handoff that installs an embedded runtime's private owner
/// session in that application's own webview.
///
/// Why it exists: the host used to inject the cookie through the webview's own
/// API, which aborts the process on iOS (`wry` 0.55's WKWebView cookie helper
/// pumps the main run loop re-entrantly and panics through Tao's Core
/// Foundation observer). This replaces that with something every host can use
/// and that never hands the long-lived session to the webview layer at all: the
/// host navigates once to an unguessable URL, the runtime answers with the
/// existing `HttpOnly; SameSite=Strict` session cookie and a redirect to a
/// clean path, and the URL stops working.
///
/// What keeps it safe:
///
/// * it is registered only on an embedded runtime's private, Host-guarded,
///   ephemeral loopback listener — never on the router local-network hosting
///   serves, and never in [`RuntimeMode::Server`] at all;
/// * the token is two v4 UUIDs of operating-system randomness, compared in
///   constant time, and is *not* the session;
/// * redeeming it consumes it, so the reply that carries the session destroys
///   the only way to ask for it again; and
/// * it expires after [`BOOTSTRAP_LIFETIME`] and after [`BOOTSTRAP_ATTEMPTS`]
///   mismatches, and is destroyed by [`RuntimeHandle::shutdown`].
///
/// What it deliberately is not: a token JavaScript can read, a credential that
/// stays in a query string, or any relaxation of the authentication and
/// DNS-rebinding guards. The session cookie it sets is the same one login
/// issues.
struct Bootstrap {
    /// `None` once redeemed, expired, exhausted, or shut down.
    credential: Mutex<Option<Credential>>,
    /// The redeemable path. Public in the sense that the owning host is told
    /// it; it is a secret, so nothing derives a log line or a `Debug` from it.
    path: String,
}

struct Credential {
    token: String,
    session: String,
    expires: Instant,
    attempts: u8,
}

/// Never renders the token or the session. `Bootstrap` is reachable from
/// `Runtime`, and one careless `{:?}` must not be able to print either.
impl std::fmt::Debug for Bootstrap {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Bootstrap")
            .field("redeemable", &self.credential.lock().is_ok_and(|c| c.is_some()))
            .finish()
    }
}

impl Bootstrap {
    fn new(session: String) -> Self {
        let token = uid() + &uid();
        Self {
            path: format!("{BOOTSTRAP_PREFIX}{token}"),
            credential: Mutex::new(Some(Credential {
                token,
                session,
                expires: Instant::now() + BOOTSTRAP_LIFETIME,
                attempts: 0,
            })),
        }
    }

    /// Exchanges a correct token for the session, once.
    fn redeem(&self, presented: &str) -> Option<String> {
        let mut held = self
            .credential
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let credential = held.as_mut()?;
        if Instant::now() >= credential.expires {
            *held = None;
            return None;
        }
        if !constant_time_eq(presented.as_bytes(), credential.token.as_bytes()) {
            credential.attempts += 1;
            if credential.attempts >= BOOTSTRAP_ATTEMPTS {
                *held = None;
            }
            return None;
        }
        held.take().map(|credential| credential.session)
    }

    /// Destroys the handoff whether or not it was ever redeemed.
    fn revoke(&self) {
        *self
            .credential
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

/// Compares in time independent of how many leading bytes match, so presenting
/// tokens to the route cannot be used to learn the secret one byte at a time.
/// Lengths are fixed and equal for every real token, so an early length
/// rejection reveals nothing.
fn constant_time_eq(presented: &[u8], expected: &[u8]) -> bool {
    if presented.len() != expected.len() {
        return false;
    }
    let mut difference = 0_u8;
    for (a, b) in presented.iter().zip(expected) {
        difference |= a ^ b;
    }
    std::hint::black_box(difference) == 0
}

/// The one route the handoff adds, with its own state so nothing else in the
/// application can reach the credential.
fn bootstrap_router(bootstrap: Arc<Bootstrap>) -> Router {
    Router::new()
        .route(
            &format!("{BOOTSTRAP_PREFIX}{{token}}"),
            get(redeem_bootstrap),
        )
        // This route is merged into the application *after* `assemble` applied
        // the baseline headers to its own routes, so it carries them itself.
        // `security_headers` only fills in headers the handler left unset, so
        // the stricter `no-referrer` below survives it.
        .layer(axum::middleware::from_fn(crate::security_headers))
        .with_state(bootstrap)
}

/// Sets the session cookie and sends the webview to a clean URL.
///
/// `303 See Other` rather than a page: the document the webview ends up
/// displaying is `/`, so the single-use token is never the address of the
/// running interface, never a `Referer`, and never reloaded by a refresh. The
/// reply is `no-store` so no cache anywhere keeps a response that carries a
/// credential, and a wrong or spent token is an ordinary 404 that says nothing
/// about which it was.
async fn redeem_bootstrap(
    State(bootstrap): State<Arc<Bootstrap>>,
    Path(token): Path<String>,
) -> Response {
    let Some(session) = bootstrap.redeem(&token) else {
        return (
            StatusCode::NOT_FOUND,
            [
                (header::CACHE_CONTROL, "no-store"),
                (header::REFERRER_POLICY, "no-referrer"),
            ],
        )
            .into_response();
    };
    (
        StatusCode::SEE_OTHER,
        [
            (header::LOCATION, "/".to_owned()),
            (
                header::SET_COOKIE,
                // Host-only (no `Domain`), so it is scoped to this listener's
                // own loopback authority; identical in every other respect to
                // the cookie `POST /api/login` issues. Not `Secure`: the
                // private listener is plain HTTP on 127.0.0.1, exactly as it
                // was when the host installed this cookie through the webview.
                format!("pr0_session={session}; HttpOnly; SameSite=Strict; Path=/; Max-Age=86400"),
            ),
            (header::CACHE_CONTROL, "no-store".to_owned()),
            (header::PRAGMA, "no-cache".to_owned()),
            // Stricter than the application-wide `same-origin` floor. The
            // single-use path *is* the credential, and `same-origin` would
            // still send it as a `Referer` to same-origin requests made from
            // whatever this response leads to. `no-referrer` means no request
            // anywhere can carry it. The redirect target is `/`, so the
            // interface that loads afterwards is governed by the baseline
            // policy again.
            (header::REFERRER_POLICY, "no-referrer".to_owned()),
        ],
    )
        .into_response()
}

/// Resolves as soon as `receiver` holds `true`, and immediately if it already
/// does. Cancel-safe, so callers may use it inside `select!`.
async fn signalled(mut receiver: watch::Receiver<bool>) {
    loop {
        if *receiver.borrow_and_update() {
            return;
        }
        if receiver.changed().await.is_err() {
            return;
        }
    }
}

/// Starts a runtime from typed configuration and returns once it is listening.
///
/// [`RuntimeMode::Embedded`] binds an ephemeral private loopback listener that
/// only answers requests addressed to that exact `127.0.0.1:<port>` authority,
/// creates the private owner session, and reports both in [`Readiness`].
/// [`RuntimeMode::Server`] binds the configured [`ListenerPolicy`] instead and
/// has no private session.
pub async fn start(config: RuntimeConfig) -> Result<RuntimeHandle, String> {
    // Idempotent; hosts that never serve TLS still pay nothing for it.
    let _ = rustls::crypto::ring::default_provider().install_default();
    let config = Arc::new(config);
    let Assembled {
        app,
        router,
        session,
    } = crate::assemble(config.clone())?;
    let shared_router = router.clone();
    let (stop, _) = watch::channel(false);
    let finished = Arc::new(watch::channel(false).0);
    let serve_result: Arc<Mutex<Option<Result<(), String>>>> = Arc::new(Mutex::new(None));

    let (readiness, discovery, bootstrap) = match config.mode {
        RuntimeMode::Embedded => {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
                .await
                .map_err(|e| format!("Bind private loopback listener: {e}"))?;
            let address = listener
                .local_addr()
                .map_err(|e| format!("Read private listener address: {e}"))?;
            // Only requests addressed to the private listener's own authority
            // are served, so a DNS-rebinding page cannot reach it by name.
            let expected_host = address.to_string();
            // The handoff is added *after* `shared_router` was cloned and
            // *inside* the private listener's host guard, so it exists only on
            // this runtime's own ephemeral loopback listener: local-network
            // hosting serves the router without it, and a standalone server
            // never reaches this branch at all.
            let bootstrap = session
                .as_ref()
                .map(|session| Arc::new(Bootstrap::new(session.clone())));
            let router = match bootstrap.clone() {
                Some(endpoint) => router.merge(bootstrap_router(endpoint)),
                None => router,
            };
            let guarded = router.layer(axum::middleware::from_fn(
                move |req: axum::extract::Request, next: axum::middleware::Next| {
                    let valid = req
                        .headers()
                        .get(header::HOST)
                        .and_then(|v| v.to_str().ok())
                        == Some(expected_host.as_str());
                    async move {
                        if valid {
                            next.run(req).await
                        } else {
                            StatusCode::FORBIDDEN.into_response()
                        }
                    }
                },
            ));
            spawn_listener(
                serve_result.clone(),
                finished.clone(),
                stop.subscribe(),
                axum::serve(listener, guarded).into_future(),
            );
            (
                Readiness {
                    url: format!("http://{address}"),
                    session: session.clone(),
                    redirect_url: None,
                },
                None,
                bootstrap,
            )
        }
        RuntimeMode::Server => {
            let (readiness, discovery) =
                start_server_listeners(&config, router, &stop, &finished, &serve_result).await?;
            (readiness, discovery, None)
        }
    };

    Ok(RuntimeHandle(Arc::new(Runtime {
        config,
        app,
        shared_router,
        readiness,
        stop,
        finished,
        serve_result,
        discovery: Mutex::new(discovery),
        hosting: tokio::sync::Mutex::new(None),
        shutdown: tokio::sync::Mutex::new(None),
        bootstrap,
        audio_suspended: AtomicBool::new(false),
        audio_lifecycle: Mutex::new(()),
        audio_stopped: AtomicBool::new(false),
    })))
}

/// Serves `serving` until it ends or the runtime is asked to stop, then records
/// the outcome. Stopping drops in-flight connections rather than draining them,
/// which is what the desktop host has always done: a held-open WebSocket must
/// not be able to postpone shutdown indefinitely.
fn spawn_listener<E, F>(
    result: Arc<Mutex<Option<Result<(), String>>>>,
    finished: Arc<watch::Sender<bool>>,
    stop: watch::Receiver<bool>,
    serving: F,
) where
    E: std::fmt::Display + Send + 'static,
    F: Future<Output = Result<(), E>> + Send + 'static,
{
    tokio::spawn(async move {
        let outcome = tokio::select! {
            served = serving => served.map_err(|e| e.to_string()),
            _ = signalled(stop) => Ok(()),
        };
        *result.lock().unwrap() = Some(outcome);
        // `send_replace` rather than `send`: the stored value must change even
        // when nobody is currently waiting, or a later `serving`/`shutdown`
        // call would wait for an edge that already happened.
        let _ = finished.send_replace(true);
    });
}

/// The standalone listener(s) described by [`ListenerPolicy`]: either plain
/// HTTP, or HTTPS plus its plain-HTTP redirect companion.
async fn start_server_listeners(
    config: &Arc<RuntimeConfig>,
    router: Router,
    stop: &watch::Sender<bool>,
    finished: &Arc<watch::Sender<bool>>,
    serve_result: &Arc<Mutex<Option<Result<(), String>>>>,
) -> Result<(Readiness, Option<discovery::Advertisement>), String> {
    let address = config.listener.address()?;
    let scheme = if config.listener.secure() {
        "https"
    } else {
        "http"
    };
    let url = format!("{scheme}://{address}");
    let Some((certificate, key)) = config.listener.tls.clone() else {
        let listener = tokio::net::TcpListener::bind(&address)
            .await
            .map_err(|e| format!("Bind server listener (on Linux run ./init.sh --allow-low-ports as your normal user, or use PR0_PORT for an alternate port): {e}"))?;
        let local = listener
            .local_addr()
            .map_err(|e| format!("Read server listener address: {e}"))?;
        let discovery = discovery::advertise(local, false, config.discovery);
        spawn_listener(
            serve_result.clone(),
            finished.clone(),
            stop.subscribe(),
            axum::serve(listener, router).into_future(),
        );
        return Ok((
            Readiness {
                url,
                session: None,
                redirect_url: None,
            },
            discovery,
        ));
    };

    let tls = axum_server::tls_rustls::RustlsConfig::from_pem_file(certificate, key)
        .await
        .map_err(|e| format!("Load TLS certificate: {e}"))?;
    let socket: SocketAddr = tokio::net::lookup_host(&address)
        .await
        .map_err(|e| format!("Resolve bind address: {e}"))?
        .next()
        .ok_or("Bind host resolved to no addresses")?;
    let http_address = config.listener.redirect_address(&address)?;
    let redirect_listener = tokio::net::TcpListener::bind(&http_address)
        .await
        .map_err(|e| format!("Bind HTTP redirect listener (on Linux run ./init.sh --allow-low-ports as your normal user, or use PR0_HTTP_PORT/PR0_PORT for alternate ports): {e}"))?;
    let discovery = discovery::advertise(socket, true, config.discovery);
    let https = axum_server::bind_rustls(socket, tls).serve(router.into_make_service());
    let redirect =
        axum::serve(redirect_listener, tls::redirects(socket.port())).into_future();
    // Both listeners live in one task: `spawn_listener` drops the pair as soon
    // as the runtime is asked to stop, exactly as it does for a single listener.
    spawn_listener(
        serve_result.clone(),
        finished.clone(),
        stop.subscribe(),
        async move {
            tokio::try_join!(
                async move { https.await.map_err(|e| e.to_string()) },
                async move { redirect.await.map_err(|e| e.to_string()) },
            )
            .map(|_| ())
        },
    );
    Ok((
        Readiness {
            url,
            session: None,
            redirect_url: Some(format!("http://{http_address}")),
        },
        discovery,
    ))
}

impl RuntimeHandle {
    pub fn readiness(&self) -> &Readiness {
        &self.0.readiness
    }

    pub fn url(&self) -> &str {
        &self.0.readiness.url
    }

    /// The private owner session, for embedded runtimes only.
    ///
    /// An application host embedding this runtime should not need it: use
    /// [`RuntimeHandle::bootstrap_url`] to hand the session to its own webview
    /// without the credential ever passing through the host or the webview API.
    pub fn session(&self) -> Option<&str> {
        self.0.readiness.session.as_deref()
    }

    /// The one-time URL that installs this runtime's private owner session in
    /// the owning application's webview, or `None` for a standalone server.
    ///
    /// Navigate a webview to it exactly once. The runtime replies with the
    /// session as an `HttpOnly; SameSite=Strict` cookie and a redirect to `/`,
    /// and the URL immediately stops working; it also expires on its own and is
    /// destroyed by [`RuntimeHandle::shutdown`]. It is only reachable on this
    /// runtime's private, Host-guarded loopback listener — not from
    /// local-network hosting, and it does not exist in
    /// [`RuntimeMode::Server`].
    ///
    /// The returned string is a secret for as long as it is redeemable: do not
    /// log it, persist it, or put it anywhere a page can read it back.
    pub fn bootstrap_url(&self) -> Option<String> {
        let bootstrap = self.0.bootstrap.as_ref()?;
        Some(format!("{}{}", self.0.readiness.url, bootstrap.path))
    }

    /// Resolves once the runtime's own listeners have stopped, reporting why.
    /// Cancel-safe and repeatable: it never consumes the outcome.
    pub async fn serving(&self) -> Result<(), String> {
        signalled(self.0.finished.subscribe()).await;
        self.0
            .serve_result
            .lock()
            .unwrap()
            .clone()
            .unwrap_or(Ok(()))
    }

    /// Starts opt-in local-network HTTPS hosting and returns its status, or the
    /// status of the hosting already running. `port` may be `0` for an
    /// ephemeral port. Only embedded runtimes can host: a standalone server
    /// already owns its LAN listener.
    pub async fn host_on_lan(&self, port: u16) -> Result<Value, String> {
        let Some(session) = self.0.readiness.session.clone() else {
            return Err("Local-network hosting is only available to embedded runtimes".into());
        };
        let mut hosting = self.0.hosting.lock().await;
        if let Some(host) = hosting.as_ref() {
            return Ok(host.status.clone());
        }
        let host = hosting::start(&self.0.config, self.0.shared_router.clone(), session, port)
            .await?;
        let status = host.status.clone();
        *hosting = Some(host);
        Ok(status)
    }

    /// Stops local-network hosting. Idempotent; returns the resulting status.
    pub async fn stop_hosting(&self) -> Value {
        self.0.hosting.lock().await.take();
        json!({"enabled":false})
    }

    pub async fn hosting_status(&self) -> Value {
        self.0
            .hosting
            .lock()
            .await
            .as_ref()
            .map(|host| host.status.clone())
            .unwrap_or_else(|| json!({"enabled":false}))
    }

    /// Whether native audio hardware is currently suspended at this host's
    /// request. Cheap and lock-free, so a lifecycle callback can check it
    /// before asking for a transition it does not need.
    pub fn audio_suspended(&self) -> bool {
        self.0.audio_suspended.load(Ordering::SeqCst)
    }

    /// Closes the native audio hardware streams, keeping everything above them
    /// intact: the loaded graph, prepared DSP state, transport position and
    /// engine sample clock, loop buffers and open recordings all stay exactly
    /// as they are, and rendering stops rather than free-running on a software
    /// clock. [`RuntimeHandle::resume_audio`] continues from the same engine
    /// sample.
    ///
    /// Idempotent: suspending an already suspended runtime closes no device and
    /// returns `Ok`. The transition happens on the orchestration worker, so no
    /// device callback ever observes it mid-buffer, and no allocation, lock or
    /// log statement is added to a callback by it.
    ///
    /// This is a platform lifecycle control — an iOS audio interruption, or the
    /// application leaving the foreground — not a user-facing transport action.
    pub async fn suspend_audio(&self) -> Result<(), String> {
        self.transition(true).await
    }

    /// Reopens the native audio hardware closed by [`RuntimeHandle::suspend_audio`]
    /// and lets rendering continue from the engine sample it stopped at.
    ///
    /// Exactly the directions that were open when the hardware closed are
    /// reopened, so an owner who had stopped hardware output, or an input that
    /// had already failed, is not restarted by a platform lifecycle event.
    ///
    /// Idempotent. If the engine is disabled nothing is opened and the call
    /// still succeeds; the devices open when the engine is next enabled. A
    /// device that refuses to reopen leaves the runtime resumed and reports the
    /// device error, exactly as enabling the engine does, rather than leaving a
    /// runtime suspended that no host asked to suspend.
    pub async fn resume_audio(&self) -> Result<(), String> {
        self.transition(false).await
    }

    /// [`RuntimeHandle::suspend_audio`] for a host without an async runtime.
    ///
    /// iOS delivers interruption and background notifications on an
    /// operating-system callback thread with a short deadline, and suspension
    /// has to have actually happened before that callback returns. Never call
    /// this from the orchestration worker or a device callback: it waits for
    /// that worker.
    pub fn suspend_audio_blocking(&self, timeout: Duration) -> Result<(), String> {
        self.set_audio_suspended(true, timeout)
    }

    /// [`RuntimeHandle::resume_audio`] for a host without an async runtime, with
    /// the same threading rules as [`RuntimeHandle::suspend_audio_blocking`].
    pub fn resume_audio_blocking(&self, timeout: Duration) -> Result<(), String> {
        self.set_audio_suspended(false, timeout)
    }

    async fn transition(&self, suspended: bool) -> Result<(), String> {
        let handle = self.clone();
        // The blocking core is the same one an operating-system callback uses,
        // so both hosts drive one implementation.
        tokio::task::spawn_blocking(move || {
            handle.set_audio_suspended(suspended, AUDIO_TRANSITION_TIMEOUT)
        })
        .await
        .map_err(|e| format!("Audio lifecycle task failed: {e}"))?
    }

    fn set_audio_suspended(&self, suspended: bool, timeout: Duration) -> Result<(), String> {
        let action = if suspended { "suspend" } else { "resume" };
        let runtime = &self.0;
        let _ordered = runtime
            .audio_lifecycle
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if runtime.audio_stopped.load(Ordering::SeqCst) {
            // Shutdown has already closed the devices: suspension is satisfied,
            // and a resume says so instead of waiting for a stopped worker.
            return if suspended {
                Ok(())
            } else {
                Err("The runtime has shut down".into())
            };
        }
        let (reply, answers) = std::sync::mpsc::sync_channel(1);
        let mut pending = audio::Command::Suspend { suspended, reply };
        let deadline = std::time::Instant::now() + timeout;
        // A bounded send: the orchestration queue is drained continuously, but a
        // lifecycle callback must not be able to block on it indefinitely.
        loop {
            match runtime.app.engine.try_send(pending) {
                Ok(()) => break,
                Err(std::sync::mpsc::TrySendError::Disconnected(_)) => {
                    return if suspended {
                        // As above: nothing is open, which is the requested
                        // state.
                        Ok(())
                    } else {
                        Err(format!("The audio worker stopped before the {action} request"))
                    };
                }
                Err(std::sync::mpsc::TrySendError::Full(returned)) => {
                    if std::time::Instant::now() >= deadline {
                        return Err(format!("The audio worker did not accept the {action} request"));
                    }
                    pending = returned;
                    std::thread::sleep(Duration::from_millis(2));
                }
            }
        }
        // The worker applies the request whether or not this host keeps waiting,
        // so the published state follows the request rather than the device
        // outcome — the same contract engine enablement has.
        runtime.audio_suspended.store(suspended, Ordering::SeqCst);
        match answers.recv_timeout(deadline.saturating_duration_since(std::time::Instant::now())) {
            Ok(outcome) => outcome,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Err(format!(
                "The audio worker did not acknowledge the {action} request within {} seconds",
                timeout.as_secs()
            )),
            // The worker stopped while this was in flight — application exit
            // raced a lifecycle event. Closed hardware is what a suspend asked
            // for, so it is satisfied; a resume has nothing left to reopen and
            // says so.
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) if suspended => Ok(()),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(format!(
                "The audio worker stopped during the {action} request"
            )),
        }
    }

    /// Stops the runtime in one ordered sequence shared by every host, and
    /// never calls `std::process::exit`.
    ///
    /// Idempotent: the first call performs the shutdown and later calls replay
    /// its outcome, so a host may stop a runtime from more than one place.
    pub async fn shutdown(&self) -> Result<(), String> {
        let mut done = self.0.shutdown.lock().await;
        if let Some(outcome) = done.as_ref() {
            return outcome.clone();
        }
        // Published before the devices close, so a lifecycle event arriving
        // during exit is answered rather than left waiting on a worker that is
        // going away.
        self.0.audio_stopped.store(true, Ordering::SeqCst);
        // A stopped runtime leaves no usable credential behind, including an
        // unredeemed webview handoff.
        if let Some(bootstrap) = &self.0.bootstrap {
            bootstrap.revoke();
        }
        let outcome = self.0.perform_shutdown().await;
        *done = Some(outcome.clone());
        outcome
    }
}

impl Runtime {
    async fn perform_shutdown(&self) -> Result<(), String> {
        // 1. Stop listening, so nothing new reaches the engine or database.
        //    `send_replace` so the request sticks even with no live receiver.
        let _ = self.stop.send_replace(true);
        signalled(self.finished.subscribe()).await;
        // 2. Drop local-network hosting and mDNS advertisement.
        self.hosting.lock().await.take();
        self.discovery.lock().unwrap().take();
        // 3. Ask the orchestrator to close devices and retire the engine, then
        //    wait on its persistence barrier so loop and recording writes are
        //    finalized before the host goes away.
        let (tx, rx) = oneshot::channel();
        let engine = self.app.engine.clone();
        let requested = tokio::task::spawn_blocking(move || engine.send(audio::Command::Shutdown(tx)))
            .await
            .map_err(|e| format!("Audio shutdown task failed: {e}"))?;
        let barrier = match requested {
            Ok(()) => rx
                .await
                .map_err(|e| format!("Audio shutdown reply lost: {e}"))
                .and_then(|result| result),
            Err(error) => Err(format!("Audio shutdown request failed: {error}")),
        };
        // 4. Revoke the private session whatever the engine reported: a stopped
        //    runtime must never leave a usable local credential behind.
        let revoked = match &self.readiness.session {
            Some(token) => self
                .app
                .db
                .lock()
                .unwrap()
                .execute("DELETE FROM sessions WHERE token=?1", [token])
                .map(|_| ())
                .map_err(|e| format!("Revoke private session: {e}")),
            None => Ok(()),
        };
        barrier.and(revoked)
    }
}

/// Creates (once) and signs in the private owner of an embedded runtime.
///
/// The credential is delivered to the owning host in process — never over an
/// HTTP login bypass, a URL credential, a default password, or a log line. The
/// `desktop_owner` table keeps its original name so existing profiles migrate
/// untouched.
pub(crate) fn private_owner_session(db: &mut Connection) -> Result<String, String> {
    accounts::migrate(db).map_err(|e| e.to_string())?;
    (|| -> Result<String, Box<dyn std::error::Error>> {
        let tx = db.transaction()?;
        tx.execute_batch("CREATE TABLE IF NOT EXISTS desktop_owner (singleton INTEGER PRIMARY KEY CHECK(singleton=1), user_id TEXT NOT NULL REFERENCES users(id));")?;
        let owner: Option<String> = tx.query_row("SELECT user_id FROM desktop_owner WHERE singleton=1", [], |r| r.get(0)).optional()?;
        let owner = if let Some(owner) = owner { owner } else {
            let count: i64 = tx.query_row("SELECT count(*) FROM users", [], |r| r.get(0))?;
            if count != 0 { return Err("Desktop mode requires its own data directory; refusing to adopt existing server accounts".into()); }
            let owner = uid();
            // Random, discarded password: only the private launcher session signs in.
            let salt = SaltString::encode_b64(Uuid::new_v4().as_bytes()).map_err(|e| e.to_string())?;
            let password = Argon2::default().hash_password(uid().as_bytes(), &salt).map_err(|e| e.to_string())?.to_string();
            tx.execute("INSERT INTO users(id,username,password) VALUES(?1,'admin',?2)", params![owner,password])?;
            tx.execute("INSERT INTO desktop_owner VALUES(1,?1)", [&owner])?;
            owner
        };
        let token = uid() + &uid();
        tx.execute("DELETE FROM sessions WHERE expires<=?1", [now()])?;
        tx.execute("INSERT INTO sessions(token,user_id,expires) VALUES(?1,?2,?3)", params![token,owner,now()+86400])?;
        tx.commit()?;
        Ok(token)
    })().map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner_db() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("PRAGMA foreign_keys=ON; CREATE TABLE users(id TEXT PRIMARY KEY,username TEXT UNIQUE,password TEXT); CREATE TABLE sessions(token TEXT PRIMARY KEY,user_id TEXT REFERENCES users(id),expires INTEGER);").unwrap();
        db
    }

    #[test]
    fn private_identity_is_stable_sessions_are_fresh_and_expire() {
        let mut db = owner_db();
        let first = private_owner_session(&mut db).unwrap();
        let second = private_owner_session(&mut db).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            db.query_row("SELECT count(*) FROM users", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            1
        );
        let owners: i64 = db
            .query_row(
                "SELECT count(DISTINCT user_id) FROM sessions WHERE expires > ?1",
                [now()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(owners, 1);
    }

    #[test]
    fn private_owner_never_adopts_existing_server_users() {
        let mut db = owner_db();
        db.execute("INSERT INTO users VALUES('existing','admin','hash')", [])
            .unwrap();
        assert!(private_owner_session(&mut db).unwrap_err().contains("refusing"));
        assert_eq!(
            db.query_row("SELECT count(*) FROM sessions", [], |r| r.get::<_, i64>(0))
                .unwrap(),
            0
        );
    }

    /// Lifecycle tests never open native audio/MIDI devices, so they are safe
    /// on build machines and say nothing about physical device behavior.
    fn lifecycle_config(directory: &std::path::Path) -> RuntimeConfig {
        let mut config = RuntimeConfig::embedded(directory.join("data"));
        config.recordings_dir = directory.join("recordings");
        config.web_root = directory.join("web");
        config.native_devices = false;
        config.discovery = false;
        // No child process and no executable: lifecycle tests never convert
        // audio, and this is the configuration a mobile host would use.
        config.importer = crate::import::unavailable();
        config
    }

    struct Scratch(std::path::PathBuf);
    impl Scratch {
        fn new(label: &str) -> Self {
            let path = std::env::temp_dir().join(format!("pr0-{label}-{}", Uuid::new_v4()));
            std::fs::create_dir_all(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    async fn get(url: &str, host: Option<&str>, cookie: Option<&str>) -> StatusCode {
        fetch(url, "/api/me", host, cookie).await.0
    }

    /// The whole response, so a test can read the status line, the headers a
    /// credential handoff has to carry, and the body.
    async fn request(
        url: &str,
        path: &str,
        host: Option<&str>,
        cookie: Option<&str>,
    ) -> (StatusCode, Vec<(String, String)>, String) {
        let listener: SocketAddr = url
            .trim_start_matches("http://")
            .parse()
            .expect("loopback readiness URL");
        let authority = host.map_or_else(|| listener.to_string(), str::to_string);
        let mut request = format!("GET {path} HTTP/1.1\r\nHost: {authority}\r\n");
        if let Some(cookie) = cookie {
            request.push_str(&format!("Cookie: pr0_session={cookie}\r\n"));
        }
        request.push_str("Connection: close\r\n\r\n");
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(listener).await.unwrap();
        stream.write_all(request.as_bytes()).await.unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).await.unwrap();
        let response = String::from_utf8_lossy(&response).into_owned();
        let status = response
            .split_whitespace()
            .nth(1)
            .and_then(|code| code.parse::<u16>().ok())
            .expect("HTTP status line");
        let (head, body) = response
            .split_once("\r\n\r\n")
            .map(|(head, body)| (head.to_owned(), body.to_owned()))
            .unwrap_or((response.clone(), String::new()));
        let headers = head
            .lines()
            .skip(1)
            .filter_map(|line| line.split_once(':'))
            .map(|(name, value)| (name.trim().to_lowercase(), value.trim().to_owned()))
            .collect();
        (StatusCode::from_u16(status).unwrap(), headers, body)
    }

    async fn fetch(
        url: &str,
        path: &str,
        host: Option<&str>,
        cookie: Option<&str>,
    ) -> (StatusCode, String) {
        let (status, _, body) = request(url, path, host, cookie).await;
        (status, body)
    }

    /// The first value of `name`, if the response carried one.
    fn header_value<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
        headers
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// Serves `router` on an ephemeral loopback listener and returns its
    /// `http://host:port` origin.
    ///
    /// Deliberately *without* the private listener's host guard, because that
    /// is the shape local-network hosting serves: `RuntimeHandle::host_on_lan`
    /// hands `shared_router` straight to a TLS listener. Serving it here
    /// exercises the same router value over plain HTTP, with no certificate, no
    /// `0.0.0.0` bind and no firewall prompt.
    async fn serve(router: Router) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let _ = axum::serve(listener, router).await;
        });
        format!("http://{address}")
    }

    /// The headers every response this runtime serves must carry, whichever
    /// listener and whichever route produced it.
    fn assert_baseline_headers(headers: &[(String, String)], what: &str) {
        assert_eq!(
            header_value(headers, "x-frame-options"),
            Some("DENY"),
            "{what} must refuse to be framed"
        );
        assert_eq!(
            header_value(headers, "x-content-type-options"),
            Some("nosniff"),
            "{what} must forbid content-type sniffing"
        );
        assert!(
            header_value(headers, "referrer-policy").is_some(),
            "{what} must carry a referrer policy"
        );
    }

    /// The path component of a bootstrap URL, which is what the private
    /// listener is asked for.
    fn bootstrap_path(url: &str, handoff: &str) -> String {
        handoff
            .strip_prefix(url)
            .expect("the handoff is on the runtime's own origin")
            .to_owned()
    }

    #[tokio::test]
    async fn embedded_readiness_is_private_and_shutdown_is_ordered_and_idempotent() {
        let scratch = Scratch::new("embedded-readiness");
        let mut config = lifecycle_config(&scratch.0);
        // A listener policy left over from a process host must not leak into an
        // embedded runtime: it binds loopback whatever the policy says.
        config.listener.bind = Some("0.0.0.0:1".into());
        config.listener.port = Some("1".into());
        let database = config.database_path();
        let runtime = start(config).await.expect("start embedded runtime");
        let ready = runtime.readiness().clone();
        assert!(
            ready.url.starts_with("http://127.0.0.1:"),
            "embedded runtimes bind private loopback, got {}",
            ready.url
        );
        let bound: SocketAddr = ready.url.trim_start_matches("http://").parse().unwrap();
        assert_ne!(
            bound.port(),
            1,
            "the host's listener policy leaked into an embedded runtime"
        );
        assert!(ready.redirect_url.is_none());
        let session = ready.session.clone().expect("private owner session");
        assert_eq!(
            get(&ready.url, None, Some(session.as_str())).await,
            StatusCode::OK
        );
        assert_eq!(get(&ready.url, None, None).await, StatusCode::UNAUTHORIZED);
        assert_eq!(
            get(&ready.url, Some("attacker.invalid"), Some(session.as_str())).await,
            StatusCode::FORBIDDEN
        );
        // Standalone runtimes own their LAN listener, so hosting is embedded-only
        // and its status stays reportable without any stdin/stdout protocol.
        assert_eq!(runtime.hosting_status().await, json!({"enabled":false}));
        assert_eq!(runtime.stop_hosting().await, json!({"enabled":false}));

        runtime.shutdown().await.expect("first shutdown");
        runtime.shutdown().await.expect("repeat shutdown");
        assert!(
            tokio::net::TcpStream::connect(bound).await.is_err(),
            "shutdown must stop the private listener"
        );
        let db = Connection::open(&database).unwrap();
        let live: i64 = db
            .query_row(
                "SELECT count(*) FROM sessions WHERE token=?1",
                [&session],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(live, 0, "shutdown must revoke the private session");
    }

    #[tokio::test]
    async fn two_embedded_runtimes_share_no_configuration_or_identity() {
        let one_scratch = Scratch::new("isolation-one");
        let two_scratch = Scratch::new("isolation-two");
        let one = start(lifecycle_config(&one_scratch.0)).await.expect("first runtime");
        let two = start(lifecycle_config(&two_scratch.0)).await.expect("second runtime");
        assert_ne!(one.url(), two.url());
        assert_ne!(one.session(), two.session());
        assert!(one_scratch.0.join("data/pr0former.sqlite").exists());
        assert!(two_scratch.0.join("data/pr0former.sqlite").exists());
        // Stopping one runtime leaves the other serving.
        one.shutdown().await.expect("stop first runtime");
        let url = two.readiness().url.clone();
        let session = two.session().unwrap().to_string();
        assert_eq!(get(&url, None, Some(session.as_str())).await, StatusCode::OK);
        two.shutdown().await.expect("stop second runtime");
    }

    #[tokio::test]
    async fn a_standalone_runtime_has_no_private_session_and_cannot_host() {
        let scratch = Scratch::new("standalone-readiness");
        let mut config = lifecycle_config(&scratch.0);
        config.mode = RuntimeMode::Server;
        config.listener.bind = Some("127.0.0.1:0".into());
        let runtime = start(config).await.expect("start standalone runtime");
        // Standalone readiness echoes the configured bind address, which is what
        // the process host has always printed.
        assert_eq!(runtime.readiness().url, "http://127.0.0.1:0");
        assert!(runtime.readiness().redirect_url.is_none());
        assert!(runtime.session().is_none());
        assert!(runtime.host_on_lan(0).await.is_err());
        runtime.shutdown().await.expect("stop standalone runtime");
    }

    /// Baseline response hardening is a *response* property, so it is asserted
    /// on real responses rather than on the shape of the router.
    ///
    /// This is the regression test for an ordering defect: the middleware was
    /// applied to `Router::new()` before any route was registered. `axum`'s
    /// `Router::layer` wraps only the routes that exist when it is called, so
    /// every route in the application — the whole API, the WebSocket upgrade
    /// and the static interface — was served with none of these headers.
    #[tokio::test]
    async fn every_served_route_carries_the_baseline_security_headers() {
        let scratch = Scratch::new("security-headers");
        let config = lifecycle_config(&scratch.0);
        // A real interface directory, so the static fallback answers 200 from
        // `ServeDir` rather than only exercising its not-found service.
        std::fs::create_dir_all(&config.web_root).unwrap();
        std::fs::write(config.web_root.join("index.html"), "<!doctype html>").unwrap();
        let runtime = start(config).await.expect("start embedded runtime");
        let url = runtime.readiness().url.clone();
        let session = runtime.session().expect("private session").to_owned();

        // One request per *registration style* in `assemble`, because the
        // defect was structural: a plain route, a route carrying its own
        // per-route layer, the WebSocket upgrade, an unauthenticated rejection,
        // a 200 from the static fallback, and the fallback's not-found service.
        for path in [
            "/api/status",
            "/api/me",
            "/api/catalog",
            "/api/devices",
            "/api/projects/nonexistent/samples",
            "/api/projects/nonexistent/events",
            "/index.html",
            "/",
            "/not-a-route",
        ] {
            let (_, headers, _) = request(&url, path, None, Some(session.as_str())).await;
            assert_baseline_headers(&headers, path);
        }

        // Rejections carry them too: an unauthenticated 401 is still a response
        // an attacker's page can try to frame or sniff.
        let (status, headers, _) = request(&url, "/api/me", None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_baseline_headers(&headers, "an unauthenticated rejection");

        // The default policy is the same-origin floor; only the credential
        // handoff overrides it.
        let (_, headers, _) = request(&url, "/api/status", None, None).await;
        assert_eq!(header_value(&headers, "referrer-policy"), Some("same-origin"));

        runtime.shutdown().await.expect("shutdown");
    }

    /// Local-network hosting serves `shared_router`, which is cloned out of the
    /// assembled application before the private listener adds its host guard
    /// and its session handoff. The hardening has to be in that clone, not only
    /// in whatever the private listener wraps around it.
    #[tokio::test]
    async fn the_router_local_network_hosting_serves_is_hardened_too() {
        let scratch = Scratch::new("security-headers-shared");
        let runtime = start(lifecycle_config(&scratch.0))
            .await
            .expect("start embedded runtime");
        let lan = serve(runtime.0.shared_router.clone()).await;
        for path in ["/api/status", "/api/me", "/not-a-route"] {
            let (_, headers, _) = request(&lan, path, None, None).await;
            assert_baseline_headers(&headers, &format!("the hosted router at {path}"));
        }
        // And the private handoff is still absent from it, unchanged by the
        // hardening layer sitting in front of both.
        let handoff = runtime.bootstrap_url().expect("an embedded webview handoff");
        let path = bootstrap_path(&runtime.readiness().url.clone(), &handoff);
        let (status, _, _) = request(&lan, &path, None, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        runtime.shutdown().await.expect("shutdown");
    }

    /// The webview session handoff replaces host-side cookie injection, so it
    /// has to install exactly the credential that used to be injected, once,
    /// and then stop existing.
    #[tokio::test]
    async fn the_webview_handoff_installs_the_session_once_and_then_stops_working() {
        let scratch = Scratch::new("bootstrap");
        let runtime = start(lifecycle_config(&scratch.0))
            .await
            .expect("start embedded runtime");
        let url = runtime.readiness().url.clone();
        let handoff = runtime.bootstrap_url().expect("an embedded webview handoff");
        assert!(
            handoff.starts_with(&format!("{url}/")),
            "the handoff must be on the runtime's own private origin: {handoff}"
        );
        let path = bootstrap_path(&url, &handoff);
        // The URL carries a secret, not the session.
        let session = runtime.session().expect("private session").to_owned();
        assert!(!handoff.contains(&session));

        // Unredeemed, the runtime answers nothing without a cookie.
        assert_eq!(get(&url, None, None).await, StatusCode::UNAUTHORIZED);

        let (status, headers, _) = request(&url, &path, None, None).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
        assert_eq!(header_value(&headers, "location"), Some("/"));
        let cookie = header_value(&headers, "set-cookie").expect("the session cookie");
        assert!(cookie.contains("HttpOnly"), "{cookie}");
        assert!(cookie.contains("SameSite=Strict"), "{cookie}");
        assert!(cookie.contains("Path=/"), "{cookie}");
        // Host-only: no `Domain` widens it beyond this listener's authority.
        assert!(!cookie.to_lowercase().contains("domain="), "{cookie}");
        // A reply carrying a credential must not be stored anywhere.
        assert_eq!(
            header_value(&headers, "cache-control").map(str::to_lowercase),
            Some("no-store".into())
        );
        // The single-use path *is* the credential, so this response overrides
        // the application-wide `same-origin` floor: nothing this reply leads to
        // may carry the token in a `Referer`, not even to this same origin.
        assert_eq!(
            header_value(&headers, "referrer-policy"),
            Some("no-referrer")
        );
        assert_baseline_headers(&headers, "the session handoff");

        // It really is the owner session: it authenticates the API.
        let installed = cookie
            .split(';')
            .next()
            .and_then(|pair| pair.strip_prefix("pr0_session="))
            .expect("the cookie names the session");
        assert_eq!(installed, session);
        assert_eq!(get(&url, None, Some(installed)).await, StatusCode::OK);

        // Single use: the reply that carried the session destroyed the URL.
        let (status, headers, _) = request(&url, &path, None, None).await;
        assert_eq!(status, StatusCode::NOT_FOUND);
        assert!(header_value(&headers, "set-cookie").is_none());
        // The refusal is hardened identically, and still withholds the
        // referrer: a presented-but-spent token is as sensitive as a live one.
        assert_eq!(
            header_value(&headers, "cache-control").map(str::to_lowercase),
            Some("no-store".into())
        );
        assert_eq!(
            header_value(&headers, "referrer-policy"),
            Some("no-referrer")
        );
        assert_baseline_headers(&headers, "a spent session handoff");

        runtime.shutdown().await.expect("shutdown");
    }

    /// Everything that must *not* be able to reach the handoff: a wrong token,
    /// a page that found the port but cannot address the listener by its own
    /// authority, local-network hosting, and a standalone server.
    #[tokio::test]
    async fn the_webview_handoff_is_private_bounded_and_absent_from_every_other_listener() {
        let scratch = Scratch::new("bootstrap-guards");
        let runtime = start(lifecycle_config(&scratch.0))
            .await
            .expect("start embedded runtime");
        let url = runtime.readiness().url.clone();
        let handoff = runtime.bootstrap_url().expect("an embedded webview handoff");
        let path = bootstrap_path(&url, &handoff);

        // The DNS-rebinding guard still runs in front of it: a page served from
        // a name that resolves to loopback cannot redeem the handoff even with
        // the right token.
        let (status, headers, _) =
            request(&url, &path, Some("attacker.invalid"), None).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert!(header_value(&headers, "set-cookie").is_none());

        // Local-network hosting serves the router as it was before the private
        // listener's guard and this handoff were added to it, so a LAN client
        // cannot reach the owner session by URL at all.
        let shared = runtime.0.shared_router.clone();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let shared_url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            let _ = axum::serve(listener, shared).await;
        });
        let (status, headers, _) = request(&shared_url, &path, None, None).await;
        assert_ne!(status, StatusCode::SEE_OTHER);
        assert!(header_value(&headers, "set-cookie").is_none());

        // A wrong token is an ordinary 404, and repeated wrong tokens destroy
        // the handoff rather than allowing an unbounded search.
        let wrong = format!("{BOOTSTRAP_PREFIX}{}", "0".repeat(72));
        for _ in 0..BOOTSTRAP_ATTEMPTS {
            let (status, headers, _) = request(&url, &wrong, None, None).await;
            assert_eq!(status, StatusCode::NOT_FOUND);
            assert!(header_value(&headers, "set-cookie").is_none());
        }
        let (status, _, _) = request(&url, &path, None, None).await;
        assert_eq!(
            status,
            StatusCode::NOT_FOUND,
            "an exhausted handoff must stay destroyed even for the right token"
        );
        // Nothing was signed in by any of it.
        assert_eq!(get(&url, None, None).await, StatusCode::UNAUTHORIZED);
        runtime.shutdown().await.expect("shutdown");

        // A standalone server has no private session, no webview and therefore
        // no handoff of any kind.
        let server_scratch = Scratch::new("bootstrap-standalone");
        let mut config = lifecycle_config(&server_scratch.0);
        config.mode = RuntimeMode::Server;
        config.listener.bind = Some("127.0.0.1:0".into());
        let server = start(config).await.expect("start standalone runtime");
        assert!(server.bootstrap_url().is_none());
        assert!(server.0.bootstrap.is_none());
        server.shutdown().await.expect("stop standalone runtime");
    }

    /// A stopped runtime leaves no usable credential behind, including a
    /// handoff nobody redeemed.
    #[tokio::test]
    async fn shutdown_destroys_an_unredeemed_webview_handoff() {
        let scratch = Scratch::new("bootstrap-shutdown");
        let runtime = start(lifecycle_config(&scratch.0))
            .await
            .expect("start embedded runtime");
        let handoff = runtime.bootstrap_url().expect("an embedded webview handoff");
        let path = bootstrap_path(&runtime.readiness().url.clone(), &handoff);
        let bootstrap = runtime.0.bootstrap.clone().unwrap();
        runtime.shutdown().await.expect("shutdown");
        assert!(bootstrap.redeem(path.trim_start_matches(BOOTSTRAP_PREFIX)).is_none());
        // And `Debug` never renders either secret.
        let rendered = format!("{bootstrap:?}");
        assert!(!rendered.contains(&path), "{rendered}");
        assert!(rendered.contains("redeemable: false"), "{rendered}");
    }

    /// Expiry is time-based rather than use-based, so a handoff the host never
    /// navigated to does not stay redeemable for the life of the process.
    #[test]
    fn an_unused_webview_handoff_expires_on_its_own() {
        let bootstrap = Bootstrap::new("session".into());
        let token = bootstrap
            .path
            .strip_prefix(BOOTSTRAP_PREFIX)
            .unwrap()
            .to_owned();
        assert!(BOOTSTRAP_LIFETIME <= Duration::from_secs(300));
        bootstrap
            .credential
            .lock()
            .unwrap()
            .as_mut()
            .unwrap()
            .expires = Instant::now();
        assert!(bootstrap.redeem(&token).is_none());
    }

    /// The token is compared in constant time, and two runtimes never share one.
    #[test]
    fn handoff_tokens_are_unguessable_and_compared_without_a_timing_oracle() {
        let one = Bootstrap::new("session".into());
        let two = Bootstrap::new("session".into());
        assert_ne!(one.path, two.path);
        let token = one.path.strip_prefix(BOOTSTRAP_PREFIX).unwrap().to_owned();
        assert_eq!(token.len(), 72);
        assert!(constant_time_eq(token.as_bytes(), token.as_bytes()));
        assert!(!constant_time_eq(b"", token.as_bytes()));
        // Differing only in the last byte is as rejected as differing in the
        // first; the loop reads every byte either way.
        let mut near = token.clone().into_bytes();
        let last = near.len() - 1;
        near[last] ^= 1;
        assert!(!constant_time_eq(&near, token.as_bytes()));
        assert_eq!(one.redeem(&token).as_deref(), Some("session"));
        assert!(one.redeem(&token).is_none());
    }

    /// The platform lifecycle contract: suspending and resuming native audio is
    /// idempotent, visible to the host and to the API, and changes nothing else
    /// about a running runtime. No device is opened in this test — it says
    /// nothing about hardware behavior on any platform.
    #[tokio::test]
    async fn audio_suspension_is_idempotent_visible_and_leaves_the_runtime_serving() {
        let scratch = Scratch::new("audio-suspension");
        let runtime = start(lifecycle_config(&scratch.0))
            .await
            .expect("start embedded runtime");
        let url = runtime.readiness().url.clone();
        let session = runtime.session().expect("private session").to_string();
        assert!(!runtime.audio_suspended());

        runtime.suspend_audio().await.expect("suspend");
        runtime
            .suspend_audio()
            .await
            .expect("repeat suspend closes nothing");
        assert!(runtime.audio_suspended());
        // The worker applied it and keeps answering: an audio lifecycle event
        // leaves the HTTP API, the database and the private session alone.
        let (status, body) = fetch(&url, "/api/devices", None, Some(&session)).await;
        assert_eq!(status, StatusCode::OK);
        let reported: Value = serde_json::from_str(&body).expect("devices JSON");
        assert_eq!(reported["audio_suspended"], json!(true));
        assert_eq!(reported["hardware_enabled"], json!(false));

        runtime.resume_audio().await.expect("resume");
        runtime.resume_audio().await.expect("repeat resume");
        assert!(!runtime.audio_suspended());
        let (_, body) = fetch(&url, "/api/devices", None, Some(&session)).await;
        let reported: Value = serde_json::from_str(&body).expect("devices JSON");
        assert_eq!(reported["audio_suspended"], json!(false));

        runtime.shutdown().await.expect("shutdown");
        // Shutdown has already closed the devices, so a late background event
        // is satisfied and a late resume reports that there is nothing to
        // resume instead of waiting for a stopped worker.
        runtime
            .suspend_audio()
            .await
            .expect("suspend after shutdown is satisfied");
        assert!(
            runtime
                .resume_audio()
                .await
                .unwrap_err()
                .contains("shut down")
        );
    }

    /// iOS delivers interruption and background notifications on an
    /// operating-system callback thread, with no async runtime in sight and a
    /// deadline to meet, so the transition has a synchronous form that runs to
    /// completion on whatever thread calls it.
    #[tokio::test]
    async fn audio_lifecycle_transitions_complete_without_an_async_runtime() {
        let scratch = Scratch::new("audio-callback-thread");
        let runtime = start(lifecycle_config(&scratch.0))
            .await
            .expect("start embedded runtime");
        let callback = runtime.clone();
        std::thread::spawn(move || callback.suspend_audio_blocking(AUDIO_TRANSITION_TIMEOUT))
            .join()
            .expect("callback thread")
            .expect("suspend from an operating-system callback thread");
        assert!(runtime.audio_suspended());
        let callback = runtime.clone();
        std::thread::spawn(move || callback.resume_audio_blocking(AUDIO_TRANSITION_TIMEOUT))
            .join()
            .expect("callback thread")
            .expect("resume from an operating-system callback thread");
        assert!(!runtime.audio_suspended());
        runtime.shutdown().await.expect("stop runtime");
    }
}
