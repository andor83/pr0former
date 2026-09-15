//! The platform native-audio backend boundary.
//!
//! Two device models exist below the runtime, and they are not variations of
//! one another:
//!
//! * **Enumerated devices** — macOS, Linux, Windows and the standalone server.
//!   The machine has a list of physical endpoints, each with its own supported
//!   formats; a performer picks any number of them. That model lives in
//!   [`crate::audio`] and is unchanged by this module.
//! * **Logical routes** — iOS/iPadOS. The application does not choose hardware.
//!   It declares an `AVAudioSession` category and the system hands it whichever
//!   route is current: the built-in speaker, a connected USB-C interface, wired
//!   headphones. There is exactly one output route and at most one input route
//!   at a time, they change underneath a running process, and capture is gated
//!   by a user permission.
//!
//! ## Why this is a correctness boundary, not a presentation preference
//!
//! CPAL's iOS backend answers `supported_input_configs`/`supported_output_configs`
//! by *constructing a RemoteIO audio unit* — and, for the input direction,
//! calling `AudioUnitInitialize` on it, which is the same call that opening a
//! capture stream makes. `DeviceTrait::supports_input` is defined in terms of
//! `supported_input_configs`, so the ordinary "list the devices to populate the
//! settings page" path reaches it too.
//!
//! When AudioToolbox's RPC to the audio server times out, it calls `abort()`.
//! Not an error return — `SIGABRT` from `_ReportRPCTimeout`, through
//! `AURemoteIO::Initialize`, with no Rust frame able to intercept it. Under the
//! `playAndRecord` category with no usable microphone, the timeout is what
//! happens. So on iOS, enumerating devices in order to *describe* them can kill
//! the process, and it did: see `docs/IPAD_RUNTIME_PLAN.md` Phase 5.
//!
//! This module is therefore the rule that on iOS the runtime **never
//! enumerates and never asks CPAL what a device supports**. It publishes two
//! fixed logical routes, resolves them straight to CPAL's default device (a
//! zero-sized value on that backend — no audio unit, no RPC), and synthesizes
//! the stream configuration from what the host's [`AudioRoutes`] capability
//! reports from `AVAudioSession`.
//!
//! ## Capture is a separate question, and the answer is currently "no"
//!
//! Not enumerating is necessary but not sufficient. `AudioUnitInitialize` is
//! also what *opening* a stream calls, through
//! `coreaudio::AudioUnit::new(RemoteIO)`, and the deadlock is a property of the
//! session category rather than of the direction. Measured on an iPadOS 26.5
//! simulator with microphone access granted and an input route present: with
//! the `playAndRecord` category installed, initializing RemoteIO logs
//! `Initialize: RPC timeout. Apparently deadlocked. Aborting now.` and
//! `abort()`s — and it did so while opening the *output* stream, before any
//! capture unit existed. Under `Playback` the same output opens normally.
//!
//! So this platform's capture is gated by
//! [`crate::config::CaptureSupport`], a host statement about its backend that
//! is checked before any permission question is asked. The iOS host reports
//! `Unimplemented`, the runtime refuses capture with that sentence, the
//! recording category is therefore never installed, and playback is unaffected.
//! No microphone prompt is produced for a feature that cannot work.
//!
//! The permission path below is not speculative scaffolding: it is the gate
//! capture has to pass *after* the backend question is answered, it runs today
//! for any host that reports `Supported`, and it is covered by tests. What
//! remains before iOS capture can be turned on is a backend that does not
//! depend on initializing RemoteIO under `playAndRecord` — see
//! `docs/IPAD_RUNTIME_PLAN.md` Phase 5 for the options and what each would
//! have to prove on hardware.
//!
//! Nothing in this module is reachable from a render or device callback.

// The logical-route model is compiled for every host so a build machine can
// test its decisions — which routes exist, which are opt-in, and exactly what a
// performer is told when capture is refused. Only iOS calls it at run time.
#![cfg_attr(not(target_os = "ios"), allow(dead_code))]

use crate::config::{CaptureAuthorization, CaptureSupport, RouteDescription, RuntimeConfig};

/// The durable settings key of the iOS logical output route.
///
/// Stable across launches, routes and reboots by construction: it names the
/// *role*, not the hardware, so switching from the built-in speaker to a USB-C
/// interface keeps the performer's saved choice and its latency compensation.
pub(crate) const IOS_OUTPUT_KEY: &str = "ios:default-output";

/// The durable settings key of the iOS logical input route.
pub(crate) const IOS_INPUT_KEY: &str = "ios:default-input";

/// Channels assumed for the output route when no host capability can be asked.
/// iOS's built-in route is stereo.
const ASSUMED_OUTPUT_CHANNELS: u16 = 2;

/// Channels assumed for the input route when the platform will not say. iOS
/// reports zero input channels until a recording session is active, and the
/// built-in microphone is mono.
const ASSUMED_INPUT_CHANNELS: u16 = 1;

/// Whether this build resolves audio routes logically instead of enumerating
/// devices. Compile-time, because it is a property of the platform's audio
/// architecture rather than of a particular machine.
pub(crate) const LOGICAL_ROUTES: bool = cfg!(target_os = "ios");

/// Routes a performer has to choose deliberately rather than inherit from
/// discovery.
///
/// Two unrelated reasons, one rule: the explicit ALSA/PulseAudio route names
/// can reserve a physical PCM, and the iOS capture route makes the system ask
/// the user for the microphone. Neither may be switched on merely because a
/// discovery pass noticed it — in the iOS case, defaulting it to enabled is
/// what selected the `playAndRecord` category on a first launch and reached the
/// aborting enumeration path before anyone had asked for an input at all.
pub(crate) fn opt_in_route(name: &str) -> bool {
    name.starts_with("pulse:DEVICE=") || name.starts_with("hw:CARD=") || name == IOS_INPUT_KEY
}

/// What the platform reports about its current route, or the conservative
/// assumption used when the host installed no capability.
///
/// Zero channels means "the platform will not say yet" — iOS answers
/// `inputNumberOfChannels` with zero outside an active recording session — so
/// it becomes the assumption rather than a refusal.
fn description(config: &RuntimeConfig) -> RouteDescription {
    let mut route = config
        .audio_routes()
        .map(|routes| routes.describe())
        .unwrap_or(RouteDescription {
            output_channels: ASSUMED_OUTPUT_CHANNELS,
            input_channels: ASSUMED_INPUT_CHANNELS,
            input_available: false,
            sample_rate: None,
        });
    route.output_channels = clamp_channels(route.output_channels, ASSUMED_OUTPUT_CHANNELS);
    route.input_channels = clamp_channels(route.input_channels, ASSUMED_INPUT_CHANNELS);
    route
}

/// Keeps a reported channel count inside the engine's device contract, falling
/// back to `assumed` for the "not yet known" zero.
fn clamp_channels(reported: u16, assumed: u16) -> u16 {
    if reported == 0 {
        assumed
    } else {
        reported.min(pr0_core::MAX_DEVICE_CHANNELS as u16)
    }
}

/// Why capture cannot be opened right now, or `None` when it can be attempted.
///
/// Split from [`authorize_capture`] so the settings list can show the same
/// sentence beside the route that the engine would report on refusing to open
/// it — one explanation, written once.
///
/// The three questions are asked in the order that stops as early as possible:
/// is there a capability at all, can the backend record, and only then has the
/// user decided. Reading a permission is not free on iOS — it is a TCC round
/// trip — and a message about Settings would send a performer somewhere that
/// cannot help when the real answer is that this build cannot record.
fn capture_obstacle(config: &RuntimeConfig) -> Option<String> {
    let Some(routes) = config.audio_routes() else {
        return Some(
            "This build has no platform capture capability, so native audio input is unavailable"
                .into(),
        );
    };
    if let CaptureSupport::Unimplemented(reason) = routes.capture_support() {
        return Some(reason.to_owned());
    }
    match routes.capture_authorization() {
        CaptureAuthorization::Denied => Some(
            "Microphone access is denied. Allow it in Settings › Privacy & Security › Microphone, then enable this input again"
                .into(),
        ),
        CaptureAuthorization::Restricted => Some(
            "Microphone access is restricted on this device by a policy this application cannot change"
                .into(),
        ),
        CaptureAuthorization::Undetermined => Some(
            "Microphone access has not been granted yet. Enable this input to be asked for it"
                .into(),
        ),
        CaptureAuthorization::Granted | CaptureAuthorization::NotRequired => {
            (!description(config).input_available)
                .then(|| "No audio input route is connected".to_owned())
        }
    }
}

/// Checks that capture may be opened, and asks for permission if nobody has.
///
/// Called on the orchestration worker immediately before the first input is
/// opened, and never from a render or device callback. It does not wait for the
/// user: requesting permission puts the system prompt on screen and returns, so
/// this attempt reports "not granted yet" and the next one — after the
/// performer answers — succeeds. Blocking the worker on a person would stall
/// every command behind it, including shutdown.
///
/// A host that installs no capture capability on an enumerating platform keeps
/// its existing behavior exactly: there is nothing for the runtime to check,
/// and the operating system enforces its own permission at open time. A
/// logical-route platform with no capability refuses, because guessing at a
/// permission it cannot read is how the process died.
pub(crate) fn authorize_capture(config: &RuntimeConfig) -> Result<(), String> {
    if !LOGICAL_ROUTES && config.audio_routes().is_none() {
        return Ok(());
    }
    // Fire and continue: the answer arrives at `capture_authorization` later,
    // and the obstacle below is what the performer is told now.
    request_capture(config);
    match capture_obstacle(config) {
        Some(reason) => Err(reason),
        None => Ok(()),
    }
}

/// Puts the platform's capture-permission prompt on screen, if and only if the
/// user has not already decided.
///
/// Called from two places, both away from any audio thread: the moment a
/// performer enables a capture route in the settings, which is when asking
/// makes sense to them, and again on the orchestration worker before capture is
/// opened, which is the moment that actually needs the answer. Idempotent by
/// construction — the system shows one prompt per installation, and asking
/// again after a refusal displays nothing, so the refusal message has to carry
/// the instructions instead.
pub(crate) fn request_capture(config: &RuntimeConfig) {
    let Some(routes) = config.audio_routes() else {
        return;
    };
    // A prompt for a permission this build cannot use is a dead end for the
    // performer and, on iOS, an App Review problem: the purpose string promises
    // a feature that does not exist. So support is checked before asking.
    if !routes.capture_support().is_supported() {
        return;
    }
    if routes.capture_authorization() == CaptureAuthorization::Undetermined {
        routes.request_capture_authorization();
    }
}

/// The logical routes this platform publishes, as the settings layer's rows:
/// `[outputs, inputs]`.
///
/// Opens nothing, enumerates nothing and initializes no audio unit. The row
/// identities are derived from the fixed route keys, so they are the same on
/// every launch and a saved choice survives a route change.
pub(crate) fn logical_rows(config: &RuntimeConfig, rate: u32) -> [Vec<serde_json::Value>; 2] {
    let route = description(config);
    // A route running at a different rate than the engine is a diagnostic, not
    // a refusal: the session rate is a *preference* the system may decline, and
    // RemoteIO converts between the client format and the hardware one.
    let mismatch = route
        .sample_rate
        .filter(|reported| *reported != rate)
        .map(|reported| format!("The system audio route is running at {reported} Hz"));
    let output = serde_json::json!({
        "id": crate::audio::device_id(IOS_OUTPUT_KEY),
        "name": IOS_OUTPUT_KEY,
        "label": "System audio output",
        "backend": "CoreAudio (system route)",
        "channels": route.output_channels,
        "error": mismatch,
    });
    let obstacle = capture_obstacle(config);
    let input = serde_json::json!({
        "id": crate::audio::device_id(IOS_INPUT_KEY),
        "name": IOS_INPUT_KEY,
        "label": "System audio input",
        "backend": "CoreAudio (system route)",
        // Still reported when capture is blocked: the route exists and the
        // performer is choosing it, the error says why it will not open yet.
        "channels": route.input_channels,
        "error": obstacle,
    });
    [vec![output], vec![input]]
}

/// Resolves a saved logical route key to a CPAL device.
///
/// On the iOS backend `default_output_device`/`default_input_device` are
/// zero-sized values: no audio unit is constructed, no property is read and no
/// RPC is made, so this cannot reach the aborting path. Which hardware the
/// route actually is remains the system's decision, made when the stream opens.
#[cfg(target_os = "ios")]
pub(crate) fn resolve_logical(name: &str, input: bool) -> Result<cpal::Device, String> {
    use cpal::traits::HostTrait;
    let expected = if input { IOS_INPUT_KEY } else { IOS_OUTPUT_KEY };
    if name != expected {
        return Err(format!(
            "Unknown audio route: {name}. This platform presents one logical {} route; refresh devices",
            if input { "input" } else { "output" },
        ));
    }
    let host = cpal::default_host();
    if input {
        host.default_input_device()
            .ok_or_else(|| "The system reports no audio input route".to_string())
    } else {
        host.default_output_device()
            .ok_or_else(|| "The system reports no audio output route".to_string())
    }
}

/// The stream configuration for a logical route, synthesized rather than
/// negotiated.
///
/// Deliberately does *not* call `supported_input_configs` or
/// `supported_output_configs`: those are the aborting path on this platform.
/// The channel count comes from the host's `AVAudioSession` reading, the rate
/// is the engine's own configured rate — RemoteIO converts if the hardware
/// disagrees — and the buffer size is left to the driver, which is the only
/// value CPAL's iOS backend accepts.
#[cfg(target_os = "ios")]
pub(crate) fn logical_stream_config(
    config: &RuntimeConfig,
    rate: u32,
    input: bool,
) -> Result<cpal::StreamConfig, String> {
    let route = description(config);
    let channels = if input {
        route.input_channels
    } else {
        route.output_channels
    };
    Ok(cpal::StreamConfig {
        channels,
        sample_rate: cpal::SampleRate(rate),
        buffer_size: cpal::BufferSize::Default,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AudioRoutes;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    /// A host capability with whatever answers a test needs, counting the
    /// prompts it was asked for.
    #[derive(Debug)]
    struct Host {
        support: CaptureSupport,
        authorization: CaptureAuthorization,
        route: RouteDescription,
        prompts: AtomicUsize,
    }

    impl Host {
        /// A host whose backend can record, so the permission gate is what the
        /// test is looking at.
        fn new(authorization: CaptureAuthorization, input_available: bool) -> Arc<Self> {
            Self::with_support(CaptureSupport::Supported, authorization, input_available)
        }

        fn with_support(
            support: CaptureSupport,
            authorization: CaptureAuthorization,
            input_available: bool,
        ) -> Arc<Self> {
            Arc::new(Self {
                support,
                authorization,
                route: RouteDescription {
                    output_channels: 2,
                    input_channels: 1,
                    input_available,
                    sample_rate: Some(48000),
                },
                prompts: AtomicUsize::new(0),
            })
        }
    }

    impl AudioRoutes for Host {
        fn describe(&self) -> RouteDescription {
            self.route
        }
        fn capture_support(&self) -> CaptureSupport {
            self.support
        }
        fn capture_authorization(&self) -> CaptureAuthorization {
            self.authorization
        }
        fn request_capture_authorization(&self) {
            self.prompts.fetch_add(1, Ordering::SeqCst);
        }
    }

    fn config(host: Option<Arc<Host>>) -> RuntimeConfig {
        let mut config = RuntimeConfig::embedded(std::env::temp_dir().join("pr0-routes"));
        config.audio_routes = host.map(|host| host as Arc<dyn AudioRoutes>);
        config
    }

    /// The whole point of the logical model: describing the routes a settings
    /// page shows must not consult CPAL, because on iOS that is the call that
    /// aborts the process. Everything a row needs comes from the host
    /// capability or from a fixed constant.
    #[test]
    fn logical_rows_describe_two_stable_routes_without_enumerating_anything() {
        let config = config(Some(Host::new(CaptureAuthorization::Granted, true)));
        let [outputs, inputs] = logical_rows(&config, 48000);
        assert_eq!(outputs.len(), 1);
        assert_eq!(inputs.len(), 1);
        assert_eq!(outputs[0]["name"], IOS_OUTPUT_KEY);
        assert_eq!(inputs[0]["name"], IOS_INPUT_KEY);
        assert_eq!(outputs[0]["channels"], 2);
        assert_eq!(inputs[0]["channels"], 1);
        assert!(outputs[0]["error"].is_null());
        assert!(inputs[0]["error"].is_null());

        // Identity is derived from the route's role, so it survives a reboot, a
        // reroute and an application update: a performer's saved choice and its
        // latency compensation are still about the same route afterwards.
        let again = logical_rows(&config, 48000);
        assert_eq!(again[0][0]["id"], outputs[0]["id"]);
        assert_ne!(outputs[0]["id"], inputs[0]["id"]);
        assert_ne!(outputs[0]["id"], 0);
    }

    /// A rate the system is not running at is worth saying, but it is not a
    /// reason to refuse the route: the session rate is a preference and the
    /// unit converts.
    #[test]
    fn a_route_running_at_another_rate_is_reported_but_still_selectable() {
        let config = config(Some(Host::new(CaptureAuthorization::Granted, true)));
        let [outputs, _] = logical_rows(&config, 44100);
        assert!(
            outputs[0]["error"]
                .as_str()
                .is_some_and(|error| error.contains("48000")),
            "{:?}",
            outputs[0]["error"]
        );
        assert_eq!(outputs[0]["channels"], 2);
    }

    /// Each refusal has to name something the performer can act on, and the two
    /// that asking cannot fix must not claim it can.
    #[test]
    fn every_capture_refusal_names_what_the_performer_can_do_about_it() {
        for (authorization, expected) in [
            (CaptureAuthorization::Denied, "Settings"),
            (CaptureAuthorization::Restricted, "restricted"),
            (CaptureAuthorization::Undetermined, "not been granted yet"),
        ] {
            let host = Host::new(authorization, true);
            let config = config(Some(host.clone()));
            let reason = capture_obstacle(&config).expect("a refusal");
            assert!(reason.contains(expected), "{authorization:?}: {reason}");
            // The refusal is on the input row too, so the settings page and the
            // engine tell the same story.
            let [_, inputs] = logical_rows(&config, 48000);
            assert_eq!(inputs[0]["error"].as_str(), Some(reason.as_str()));
        }
    }

    /// A granted permission is not enough on its own: with no input route
    /// connected there is nothing to open, and the abort this whole boundary
    /// exists to avoid is exactly what initializing a capture unit with no
    /// microphone produced.
    #[test]
    fn capture_is_refused_when_no_input_route_is_connected() {
        let config = config(Some(Host::new(CaptureAuthorization::Granted, false)));
        assert!(
            capture_obstacle(&config).is_some_and(|reason| reason.contains("No audio input route"))
        );
    }

    /// A runtime whose host installed no capture capability must fail safely,
    /// not guess. The output route still works, because output does not need a
    /// permission.
    #[test]
    fn a_runtime_without_a_capture_capability_refuses_capture_and_keeps_output() {
        let config = config(None);
        let [outputs, inputs] = logical_rows(&config, 48000);
        assert!(outputs[0]["error"].is_null());
        assert_eq!(outputs[0]["channels"], ASSUMED_OUTPUT_CHANNELS);
        assert!(
            inputs[0]["error"]
                .as_str()
                .is_some_and(|error| error.contains("no platform capture capability"))
        );
    }

    /// Asking is worth exactly one prompt, at the moment the performer enables
    /// an input — and only in the state where the system will actually show
    /// one. A second request against a denied permission displays nothing on
    /// iOS, so it must not be made in place of a usable message.
    #[test]
    fn permission_is_requested_only_when_the_user_has_not_decided() {
        let undecided = Host::new(CaptureAuthorization::Undetermined, true);
        assert!(authorize_capture(&config(Some(undecided.clone()))).is_err());
        assert_eq!(undecided.prompts.load(Ordering::SeqCst), 1);

        for authorization in [
            CaptureAuthorization::Denied,
            CaptureAuthorization::Restricted,
            CaptureAuthorization::Granted,
        ] {
            let host = Host::new(authorization, true);
            let outcome = authorize_capture(&config(Some(host.clone())));
            assert_eq!(outcome.is_ok(), authorization == CaptureAuthorization::Granted);
            assert_eq!(host.prompts.load(Ordering::SeqCst), 0, "{authorization:?}");
        }
    }

    /// A host that installs no capture capability on a platform that enumerates
    /// devices keeps exactly its existing behavior: the runtime checks nothing
    /// and the operating system enforces its own permission when the stream
    /// opens. This is the guarantee that the standalone server, macOS, Linux
    /// and Windows are untouched by any of this.
    #[test]
    fn enumerating_hosts_without_a_capability_are_unaffected() {
        let unchanged = authorize_capture(&config(None));
        assert_eq!(unchanged.is_ok(), !LOGICAL_ROUTES);
    }

    /// A backend that cannot record is a different answer from a permission
    /// that has not been given, and the two must not be confused: the refusal
    /// has to be the backend's sentence, and no prompt may be produced for a
    /// feature that cannot work whatever the user says.
    #[test]
    fn an_unsupported_capture_backend_refuses_without_asking_the_user_for_anything() {
        const REASON: &str = "Native audio input is not available in this build";
        for authorization in [
            CaptureAuthorization::Granted,
            CaptureAuthorization::Undetermined,
            CaptureAuthorization::Denied,
            CaptureAuthorization::NotRequired,
        ] {
            let host = Host::with_support(
                CaptureSupport::Unimplemented(REASON),
                authorization,
                true,
            );
            let config = config(Some(host.clone()));
            assert_eq!(capture_obstacle(&config).as_deref(), Some(REASON));
            assert_eq!(
                authorize_capture(&config).err().as_deref(),
                Some(REASON),
                "{authorization:?}"
            );
            assert_eq!(
                host.prompts.load(Ordering::SeqCst),
                0,
                "{authorization:?} must not produce a microphone prompt"
            );
            // The settings list says the same thing, and output is untouched.
            let [outputs, inputs] = logical_rows(&config, 48000);
            assert_eq!(inputs[0]["error"].as_str(), Some(REASON));
            assert!(outputs[0]["error"].is_null());
        }
    }

    /// Discovery may never switch on a route that asks the user for the
    /// microphone. Defaulting the iOS capture route to enabled is what selected
    /// the recording category on a first launch and reached the aborting
    /// enumeration path before any input had been asked for.
    #[test]
    fn the_capture_route_is_opt_in_and_the_output_route_is_not() {
        assert!(opt_in_route(IOS_INPUT_KEY));
        assert!(!opt_in_route(IOS_OUTPUT_KEY));
        // The existing explicit Linux routes keep the same rule.
        assert!(opt_in_route("pulse:DEVICE=hdmi"));
        assert!(opt_in_route("hw:CARD=USB,DEV=0"));
        assert!(!opt_in_route("Built-in Output"));
    }

    /// Reported channel counts are clamped into the engine's device contract,
    /// and the platform's "not known yet" zero becomes the assumption rather
    /// than an unopenable route.
    #[test]
    fn unreported_and_oversized_channel_counts_are_replaced_by_usable_ones() {
        assert_eq!(clamp_channels(0, ASSUMED_INPUT_CHANNELS), 1);
        assert_eq!(clamp_channels(0, ASSUMED_OUTPUT_CHANNELS), 2);
        assert_eq!(clamp_channels(8, 2), 8);
        assert_eq!(
            clamp_channels(1000, 2),
            pr0_core::MAX_DEVICE_CHANNELS as u16
        );
    }
}
