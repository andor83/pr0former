//! Host-owned runtime configuration.
//!
//! Every filesystem root and capability switch the runtime needs is a typed
//! field here, supplied by whichever host starts the runtime. Nothing below the
//! host adapter reads `PR0_*` environment variables for these settings, and
//! embedded hosts never have to mutate the process environment to configure a
//! runtime. There is deliberately no global instance: tests and future hosts
//! (desktop, iPadOS) must be able to run isolated runtimes side by side, so the
//! configuration travels through runtime/app state instead.
use crate::import::{self, AudioImporter};
use crate::settings::AudioPreferences;
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Which host started this runtime.
///
/// `Server` is the standalone performance server: it binds the configured
/// [`ListenerPolicy`] and has no private owner. `Embedded` is a runtime owned
/// by an application process (today the desktop launcher, later the iPadOS
/// shell): it binds a private ephemeral loopback listener, owns a local session,
/// and never uses the listener policy, TLS pair or privileged ports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RuntimeMode {
    Server,
    Embedded,
}

/// Standalone listener and transport settings.
///
/// Every field is a host decision, not an environment lookup. Embedded
/// runtimes ignore this entirely — see [`RuntimeMode::Embedded`].
#[derive(Clone, Debug, Default)]
pub struct ListenerPolicy {
    /// Base `host:port` to bind. `None` keeps the historical default of
    /// `0.0.0.0:443` with TLS and `0.0.0.0:80` without it.
    pub bind: Option<String>,
    /// Replaces the host component of `bind`.
    pub host: Option<String>,
    /// Replaces the port component of `bind`.
    pub port: Option<String>,
    /// Port for the plain-HTTP redirect listener that accompanies HTTPS.
    /// `None` keeps the historical default of `80`.
    pub http_redirect_port: Option<String>,
    /// Certificate and key. `None` serves plain HTTP.
    pub tls: Option<(PathBuf, PathBuf)>,
}

impl ListenerPolicy {
    pub fn secure(&self) -> bool {
        self.tls.is_some()
    }

    /// The address this policy binds, after applying host/port overrides.
    pub fn address(&self) -> Result<String, String> {
        let base = self.bind.clone().unwrap_or_else(|| {
            if self.secure() {
                "0.0.0.0:443".into()
            } else {
                "0.0.0.0:80".into()
            }
        });
        crate::bind::address(&base, self.host.as_deref(), self.port.as_deref())
    }

    /// The plain-HTTP redirect address derived from an already resolved HTTPS
    /// address.
    pub fn redirect_address(&self, address: &str) -> Result<String, String> {
        crate::bind::address(
            address,
            None,
            Some(self.http_redirect_port.as_deref().unwrap_or("80")),
        )
    }
}

/// How a host prepares the platform's audio stack immediately before the
/// runtime opens a native stream.
///
/// iOS is the reason this exists. An `AVAudioSession` category, mode and driver
/// preferences have to be installed *before* CoreAudio opens a unit, and the
/// correct category depends on whether an input is about to be opened — which
/// only the settings the orchestration worker is about to use can answer. A
/// host-side read at launch cannot: a performer who enables a microphone in the
/// running application changes the answer without any lifecycle event.
///
/// Called on the orchestration worker, immediately before devices are opened
/// and never from a render or device callback, so it may block briefly and
/// allocate exactly as opening a device already does. Failure is reported and
/// the open still proceeds: the device layer's own error is the more useful one.
///
/// Hosts with no such requirement — the standalone server, macOS, Linux and
/// Windows — supply no policy and pay nothing for it.
pub trait AudioPolicy: Send + Sync + std::fmt::Debug {
    fn prepare(&self, preferences: &AudioPreferences) -> Result<(), String>;
}

/// Whether this process may capture audio, as the platform reports it.
///
/// A distinct type rather than a boolean because the three "not yet" answers
/// need three different behaviors: one is worth prompting for, one is a
/// settings trip the user has to make, and one cannot be changed at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureAuthorization {
    /// This platform does not gate capture behind a user decision. Every host
    /// but iOS/iPadOS reports this.
    NotRequired,
    /// The user has allowed it.
    Granted,
    /// The user has not been asked yet. Asking is worthwhile and is what
    /// [`AudioRoutes::request_capture_authorization`] does.
    Undetermined,
    /// The user refused. Only a trip to the system settings changes it, so the
    /// runtime must say so rather than prompt again — iOS shows nothing for a
    /// second request.
    Denied,
    /// Policy — parental controls, a mobile-device-management profile — forbids
    /// capture. Asking cannot change it and the message must not suggest it
    /// can.
    Restricted,
}

impl CaptureAuthorization {
    /// Whether capture may be opened right now.
    pub fn permits_capture(self) -> bool {
        matches!(self, Self::NotRequired | Self::Granted)
    }
}

/// Whether a host's audio backend can open a native capture stream at all,
/// independently of whether the user has allowed one.
///
/// Two different questions get confused otherwise. [`CaptureAuthorization`] is
/// about a person's decision and can change while the application runs;
/// this is about whether the code underneath is able to do the thing safely,
/// and changes only when that code does. A performer told "allow the
/// microphone" when the real answer is "this build cannot record" has been
/// given a instruction that cannot work.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureSupport {
    /// The backend opens capture streams.
    Supported,
    /// It does not, and asking the user for anything would be pointless. The
    /// sentence is shown to the performer and must not suggest a permission or
    /// a setting they could change.
    Unimplemented(&'static str),
}

impl CaptureSupport {
    pub fn is_supported(self) -> bool {
        self == Self::Supported
    }
}

/// What the platform says about its own current audio route.
///
/// Every field is a *fact about the route*, never a device handle: producing
/// one must not open a device, initialize an audio unit or prompt for anything.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RouteDescription {
    /// Channels the current output route carries.
    pub output_channels: u16,
    /// Channels the current input route carries. Platforms report zero until a
    /// recording session is active, so callers treat zero as "unknown", not as
    /// "none".
    pub input_channels: u16,
    /// Whether an input route exists at all.
    pub input_available: bool,
    /// The rate the route is running at, when the platform will say.
    pub sample_rate: Option<u32>,
}

/// The host's native audio *route* capability, for platforms that present
/// logical routes instead of enumerable devices.
///
/// iOS/iPadOS is the reason this exists, and the reason is a process abort, not
/// a presentation preference. CPAL's iOS backend answers
/// `supported_input_configs` by constructing a RemoteIO audio unit and calling
/// `AudioUnitInitialize` on it — the same call opening a capture stream makes.
/// When AudioToolbox's RPC to the audio server times out it calls `abort()`, so
/// merely *enumerating devices to populate a settings page* can kill the
/// process, and no Rust-side error handling can intercept it. The runtime
/// therefore never enumerates on that platform: it asks the host, which answers
/// from `AVAudioSession` without touching an audio unit.
///
/// Called from the orchestration worker and from blocking discovery tasks,
/// never from a render or device callback, so an implementation may allocate
/// and send Objective-C messages exactly as opening a device already does. It
/// must not block on a user decision.
///
/// Hosts on platforms that enumerate devices — the standalone server, macOS,
/// Linux and Windows — install no capability and keep their existing
/// multi-device enumeration unchanged.
pub trait AudioRoutes: Send + Sync + std::fmt::Debug {
    /// The platform's current route. Must not open a device or prompt.
    fn describe(&self) -> RouteDescription;

    /// Whether this host's audio backend can open native capture at all.
    ///
    /// Checked before authorization, and before anything is asked of the user:
    /// a host that cannot record must not produce a microphone prompt. There is
    /// deliberately no default implementation — a new host has to state its
    /// answer rather than inherit an optimistic one.
    fn capture_support(&self) -> CaptureSupport;

    /// Current capture authorization. Must not prompt and must not block.
    fn capture_authorization(&self) -> CaptureAuthorization;

    /// Asks the platform to put its capture-permission prompt on screen, if the
    /// user has not decided yet. Returns immediately; the answer arrives at
    /// [`AudioRoutes::capture_authorization`] later, so the caller reports "not
    /// granted yet" rather than waiting for a person.
    fn request_capture_authorization(&self);
}

/// Typed startup settings owned by the host.
///
/// Construct with [`RuntimeConfig::new`] and override individual fields, or use
/// [`RuntimeConfig::from_env`] for the standalone/CLI compatibility path.
#[derive(Clone, Debug)]
pub struct RuntimeConfig {
    /// Root for the database and all runtime-managed project state.
    pub data_dir: PathBuf,
    /// Directory served for non-API requests.
    pub web_root: PathBuf,
    /// Root for recording archives; deliberately outside `data_dir` and
    /// outside static serving.
    pub recordings_dir: PathBuf,
    /// How uploaded audio becomes canonical float WAV.
    ///
    /// A capability, not a path: the host decides whether import runs entirely
    /// in process, falls back to an external converter, or is unavailable. A
    /// mobile or otherwise sandboxed host installs [`import::pure_rust`] and no
    /// executable is ever involved; a host with no import capability installs
    /// [`import::unavailable`] and upload reports that rather than failing
    /// somewhere deeper. The runtime never discovers or launches a converter by
    /// itself.
    ///
    /// On iOS/iPadOS and Android the external-converter implementation is not
    /// compiled at all, so [`import::standard`] yields the in-process decoder
    /// there however it is called and no configuration can reach a child
    /// process.
    pub importer: Arc<dyn AudioImporter>,
    /// Whether native audio/MIDI devices may be enumerated and opened.
    pub native_devices: bool,
    /// Optional platform audio preparation, run just before a native stream is
    /// opened. See [`AudioPolicy`]; `None` on every host but iOS/iPadOS.
    pub audio_policy: Option<Arc<dyn AudioPolicy>>,
    /// Optional platform route and capture-authorization capability.
    ///
    /// See [`AudioRoutes`]. `None` on every host that enumerates devices; an
    /// iOS/iPadOS host installs one, and an iOS runtime *without* one lists its
    /// logical routes with conservative defaults and refuses to open capture
    /// rather than guessing at a permission it cannot read.
    pub audio_routes: Option<Arc<dyn AudioRoutes>>,
    /// Whether this runtime may advertise itself over mDNS. Loopback-only
    /// listeners are never advertised regardless of this switch.
    pub discovery: bool,
    /// Standalone listener settings; unused by embedded runtimes.
    pub listener: ListenerPolicy,
    pub mode: RuntimeMode,
}

impl RuntimeConfig {
    /// Defaults matching the historical standalone layout, rooted at
    /// `data_dir`. `recordings_dir` intentionally does not default under
    /// `data_dir`; it mirrors the standalone `recordings/` sibling.
    pub fn new(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            data_dir: data_dir.into(),
            web_root: PathBuf::from("web/dist"),
            recordings_dir: PathBuf::from("recordings"),
            importer: import::standard(Some(OsString::from("ffmpeg"))),
            native_devices: true,
            audio_policy: None,
            audio_routes: None,
            discovery: true,
            listener: ListenerPolicy::default(),
            mode: RuntimeMode::Server,
        }
    }

    /// A private runtime owned by an application process: ephemeral loopback
    /// listener, local owner session, no LAN listener policy. Native devices
    /// stay enabled; hosts that cannot open them clear `native_devices`.
    pub fn embedded(data_dir: impl Into<PathBuf>) -> Self {
        Self {
            mode: RuntimeMode::Embedded,
            ..Self::new(data_dir)
        }
    }

    /// The standalone/CLI compatibility adapter: the same `PR0_*` variables the
    /// server read before configuration was typed. Only the process host calls
    /// this.
    ///
    /// `PR0_DISABLE_NATIVE_DEVICES=1` is the documented switch. Before this
    /// existed as one field the audio paths tested for exactly `1` while the
    /// MIDI paths tested only for presence; both now follow the documented `1`.
    pub fn from_env(mode: RuntimeMode) -> Self {
        Self {
            data_dir: std::env::var_os("PR0_DATA")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("data")),
            web_root: std::env::var_os("PR0_WEB_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("web/dist")),
            recordings_dir: std::env::var_os("PR0_RECORDINGS_ROOT")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("recordings")),
            // Standalone hosts prefer the in-process decoder and keep the
            // configured external converter for the formats it does not cover.
            importer: import::standard(Some(
                std::env::var_os("PR0_FFMPEG").unwrap_or_else(|| OsString::from("ffmpeg")),
            )),
            native_devices: std::env::var("PR0_DISABLE_NATIVE_DEVICES").as_deref() != Ok("1"),
            // A process host has no platform audio session to install, and
            // every platform it runs on enumerates devices.
            audio_policy: None,
            audio_routes: None,
            discovery: std::env::var("PR0_DISCOVERY").as_deref() != Ok("0"),
            // TLS material and the port default it implies are resolved by the
            // process host (`tls::Config::load`), which also performs the
            // legacy certificate migration; embedded mode never touches it.
            listener: match mode {
                RuntimeMode::Server => ListenerPolicy {
                    bind: std::env::var("PR0_BIND").ok(),
                    host: std::env::var("PR0_HOST").ok(),
                    port: None,
                    http_redirect_port: std::env::var("PR0_HTTP_PORT").ok(),
                    tls: None,
                },
                // Embedded runtimes bind a private ephemeral loopback listener,
                // so the LAN bind/port/TLS variables never apply to them.
                RuntimeMode::Embedded => ListenerPolicy::default(),
            },
            mode,
        }
    }

    pub fn is_embedded(&self) -> bool {
        self.mode == RuntimeMode::Embedded
    }

    pub fn database_path(&self) -> PathBuf {
        self.data_dir.join("pr0former.sqlite")
    }

    pub fn audio_settings_path(&self) -> PathBuf {
        self.data_dir.join("audio-settings.json")
    }

    pub fn osc_settings_path(&self) -> PathBuf {
        self.data_dir.join("osc-settings.json")
    }

    pub fn loops_dir(&self) -> PathBuf {
        self.data_dir.join("loops")
    }

    pub fn sample_library_dir(&self) -> PathBuf {
        self.data_dir.join("sample-library")
    }

    /// Per-project imported sample storage. `project` is an opaque identifier
    /// supplied by the caller; callers keep the existing validation contract.
    pub fn project_samples_dir(&self, project: &str) -> PathBuf {
        self.data_dir.join("samples").join(project)
    }

    /// Desktop LAN-hosting certificate material.
    pub fn hosting_dir(&self) -> PathBuf {
        self.data_dir.join("hosting")
    }

    pub fn web_root(&self) -> &Path {
        &self.web_root
    }

    pub fn web_index(&self) -> PathBuf {
        self.web_root.join("index.html")
    }

    /// The host's audio import capability.
    pub fn importer(&self) -> &Arc<dyn AudioImporter> {
        &self.importer
    }

    /// The host's platform audio preparation, if it installs one.
    pub fn audio_policy(&self) -> Option<&Arc<dyn AudioPolicy>> {
        self.audio_policy.as_ref()
    }

    /// The host's platform route capability, if it installs one.
    pub fn audio_routes(&self) -> Option<&Arc<dyn AudioRoutes>> {
        self.audio_routes.as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_paths_stay_under_the_configured_roots() {
        let config = RuntimeConfig::new("/tmp/one");
        assert_eq!(config.database_path(), Path::new("/tmp/one/pr0former.sqlite"));
        assert_eq!(
            config.audio_settings_path(),
            Path::new("/tmp/one/audio-settings.json")
        );
        assert_eq!(
            config.osc_settings_path(),
            Path::new("/tmp/one/osc-settings.json")
        );
        assert_eq!(config.loops_dir(), Path::new("/tmp/one/loops"));
        assert_eq!(
            config.sample_library_dir(),
            Path::new("/tmp/one/sample-library")
        );
        assert_eq!(
            config.project_samples_dir("p"),
            Path::new("/tmp/one/samples/p")
        );
        assert_eq!(config.hosting_dir(), Path::new("/tmp/one/hosting"));
        // Recordings are a separate root, not a data_dir child.
        assert_eq!(config.recordings_dir, Path::new("recordings"));
    }

    #[test]
    fn two_configurations_stay_isolated() {
        let one = RuntimeConfig::new("/tmp/one");
        let mut two = RuntimeConfig::new("/tmp/two");
        two.recordings_dir = PathBuf::from("/tmp/two-recordings");
        two.native_devices = false;
        two.mode = RuntimeMode::Embedded;
        assert_ne!(one.database_path(), two.database_path());
        assert_ne!(one.recordings_dir, two.recordings_dir);
        assert!(one.native_devices && !two.native_devices);
        assert!(!one.is_embedded() && two.is_embedded());
    }

    #[test]
    fn an_embedded_configuration_carries_no_listener_policy() {
        let config = RuntimeConfig::embedded("/tmp/embedded");
        assert!(config.is_embedded());
        assert!(config.listener.bind.is_none() && config.listener.tls.is_none());
        assert_eq!(config.data_dir, Path::new("/tmp/embedded"));
    }

    #[test]
    fn listener_defaults_follow_the_transport_and_accept_overrides() {
        let mut policy = ListenerPolicy::default();
        assert_eq!(policy.address().unwrap(), "0.0.0.0:80");
        policy.tls = Some((PathBuf::from("cert"), PathBuf::from("key")));
        assert_eq!(policy.address().unwrap(), "0.0.0.0:443");
        assert_eq!(policy.redirect_address("0.0.0.0:443").unwrap(), "0.0.0.0:80");
        policy.http_redirect_port = Some("8080".into());
        assert_eq!(
            policy.redirect_address("0.0.0.0:443").unwrap(),
            "0.0.0.0:8080"
        );
        policy.bind = Some("127.0.0.1:4321".into());
        assert_eq!(policy.address().unwrap(), "127.0.0.1:4321");
        policy.port = Some("4567".into());
        policy.host = Some("::1".into());
        assert_eq!(policy.address().unwrap(), "[::1]:4567");
        policy.port = Some("0".into());
        assert!(policy.address().is_err());
    }

    #[test]
    fn defaults_match_the_historical_standalone_layout() {
        let config = RuntimeConfig::new("data");
        assert_eq!(config.web_root(), Path::new("web/dist"));
        assert_eq!(config.web_index(), Path::new("web/dist/index.html"));
        assert_eq!(config.recordings_dir, Path::new("recordings"));
        // In-process decoding first, then the configured external converter —
        // except on hosts without process spawning, where the converter is not
        // compiled and in-process decoding is the whole chain.
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        assert_eq!(config.importer().name(), "symphonia+ffmpeg");
        #[cfg(any(target_os = "ios", target_os = "android"))]
        assert_eq!(config.importer().name(), "symphonia");
        assert!(config.native_devices);
        assert!(config.discovery);
        assert_eq!(config.mode, RuntimeMode::Server);
    }

    #[test]
    fn hosts_choose_their_own_import_capability() {
        let limits = import::ImportLimits::for_rate(48000);
        let source = import::ImportSource::bytes(Vec::<u8>::new());

        // A host that cannot run executables keeps a working importer.
        let mut mobile = RuntimeConfig::embedded("data");
        mobile.importer = import::pure_rust();
        assert_eq!(mobile.importer().name(), "symphonia");

        // A host with no import capability at all reports it explicitly rather
        // than launching a process that does not exist.
        let mut none = RuntimeConfig::new("data");
        none.importer = import::unavailable();
        assert_eq!(
            none.importer().decode(&source, &limits),
            Err(import::ImportError::Unavailable(
                import::NO_IMPORTER.into()
            ))
        );
    }
}
