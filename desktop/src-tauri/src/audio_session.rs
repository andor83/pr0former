//! The iOS/iPadOS audio-session policy and hardware lifecycle bridge.
//!
//! iOS owns the audio hardware. An application declares what it intends to do
//! with a category, mode and preferences, and the system then interrupts,
//! reroutes and deactivates that session as calls arrive, devices come and go
//! and the application leaves the foreground. `docs/IPAD_RUNTIME_PLAN.md`
//! Phase 5 is this bridge: the policy that goes in before CoreAudio opens a
//! stream, and the four events that have to reach the runtime's
//! [`RuntimeHandle::suspend_audio`]/[`RuntimeHandle::resume_audio`] pair.
//!
//! Two deliberate properties:
//!
//! * **Nothing here runs in a device callback.** Every call is made on the
//!   thread the notification arrived on, and the transition itself is performed
//!   by the runtime's orchestration worker between DSP blocks. No allocation,
//!   lock, log statement or Objective-C message is added to a render or device
//!   callback by any of it.
//! * **The category is reconsidered whenever a device is opened.** [`policy`]
//!   hands this same decision to the runtime as a capability, so a performer
//!   who enables a microphone in the running application gets the recording
//!   category installed before the engine opens the input — not only after the
//!   next background/foreground cycle.
//! * **Transitions are completed, not scheduled.** iOS gives an application a
//!   few seconds after `didEnterBackground`, and the hardware must actually be
//!   closed inside that window; a task posted to an async runtime could be
//!   frozen before it ran. Each handler therefore completes its transition
//!   before returning, which also means a later background event can never be
//!   overtaken by an earlier resume.
//!
//! There is deliberately no background-audio entitlement, so leaving the
//! foreground closes the hardware and deactivates the session rather than
//! claiming continued playback.
//!
//! **Mostly unverified.** The policy decisions in [`SessionPolicy`] are covered
//! by tests that run on a build machine. An iPad simulator has exercised
//! [`prepare`] and three background/foreground suspend/resume cycles, with the
//! engine disabled, so nothing opened or closed a real device even there. No
//! interruption, route change, media-services reset or physical audio has been
//! observed anywhere; the simulator cannot raise them. Nothing here is a claim
//! about how iPad audio behaves.

// The policy decisions are compiled for every host so a build machine can test
// them. Only iOS calls AVFoundation with the result.
#![cfg_attr(not(target_os = "ios"), allow(dead_code))]

use pr0_runtime::{AudioPreferences, CaptureAuthorization, CaptureSupport};

/// How long a lifecycle callback waits for the hardware to close. Well inside
/// the few seconds iOS allows after `didEnterBackground`, and long enough for
/// the orchestration worker to answer between DSP blocks.
const SUSPEND_DEADLINE: std::time::Duration = std::time::Duration::from_secs(2);

/// How long a lifecycle callback waits for the hardware to reopen. Longer than
/// closing, because it opens devices; still bounded, because a wedged worker
/// must not be able to hang the application's foreground transition.
const RESUME_DEADLINE: std::time::Duration = std::time::Duration::from_secs(3);

/// What this application asks iOS for, derived from the engine's own saved
/// audio settings and what the platform says about capture.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SessionPolicy {
    /// Selects `AVAudioSessionCategoryPlayAndRecord` instead of
    /// `AVAudioSessionCategoryPlayback`.
    ///
    /// The recording category is what puts this process on the audio server's
    /// input path, so it is selected only when capture is both *configured* and
    /// *actually possible* — see [`SessionPolicy::new`].
    pub(crate) record: bool,
    /// The engine's configured rate, offered to the driver as a preference. The
    /// driver decides; the engine does not resample to follow it.
    pub(crate) preferred_sample_rate: f64,
    /// The engine's block expressed as a buffer duration. Also a preference:
    /// the runtime's output queue already adapts to the callback size the
    /// driver picks.
    pub(crate) preferred_io_duration: f64,
}

/// Whether this host offers native audio capture on iPadOS.
///
/// `Unimplemented`, and the reason is the backend rather than a permission or a
/// route. Measured on an iPadOS 26.5 simulator with microphone access granted
/// and an input route present: with `playAndRecord` installed, initializing a
/// RemoteIO audio unit logs `Initialize: RPC timeout. Apparently deadlocked.
/// Aborting now.` and calls `abort()`. It aborted while opening the *output*
/// stream, before any capture unit existed, so the hazard belongs to the
/// category and not to the direction — installing `playAndRecord` at all is
/// what costs playback too. Under `Playback` the same output opens normally.
///
/// An `abort()` cannot be caught, retried or reported, and a performance
/// application that can die mid-show is worse than one that cannot record. So
/// the runtime is told capture is unavailable, it refuses to open one, the
/// recording category is never selected, and no microphone prompt is produced
/// for a feature that cannot work.
///
/// To turn capture on, this constant becomes `Supported` — and everything
/// downstream of it, the permission gate included, is already in place and
/// tested. What has to exist first is a capture path that does not depend on
/// initializing RemoteIO under `playAndRecord`, and evidence from physical
/// hardware rather than a simulator. See `docs/IPAD_RUNTIME_PLAN.md` Phase 5.
const CAPTURE: CaptureSupport = CaptureSupport::Unimplemented(
    "Native audio input is not available on iPadOS in this build. \
     Audio output, imported samples and network sources are unaffected",
);

impl SessionPolicy {
    /// The policy for `preferences`, given what the platform currently reports
    /// about capture.
    ///
    /// The recording category needs all four of: a backend that can record, a
    /// performer who enabled a native input, a permission that allows capture,
    /// and an input route that exists. None of these is tidiness —
    /// `playAndRecord` is what puts this process on the audio server's input
    /// path, and that is the configuration whose RemoteIO initialization
    /// deadlocks and aborts. An application that *can* record still launches
    /// under `Playback`.
    pub(crate) fn new(
        preferences: &AudioPreferences,
        support: CaptureSupport,
        capture: CaptureAuthorization,
        input_available: bool,
    ) -> Self {
        Self {
            record: preferences.capture_inputs
                && support.is_supported()
                && capture.permits_capture()
                && input_available,
            preferred_sample_rate: f64::from(preferences.sample_rate),
            preferred_io_duration: preferences.io_duration(),
        }
    }
}

#[cfg(target_os = "ios")]
pub(crate) use ios::{install, prepare, release};

/// The session policy as a *runtime capability*.
///
/// [`prepare`] installs the policy once, at launch, from the settings saved
/// then. That is not enough on its own: the recording category — the thing that
/// makes iOS ask for the microphone — follows an actually enabled input, and a
/// performer can enable one in the running application without any lifecycle
/// event. Before this existed, such an input opened against a `Playback`
/// session and simply did not work until the application was backgrounded and
/// brought forward again.
///
/// So the same decision is also handed to the runtime, which calls it on the
/// orchestration worker immediately before it opens a native stream, with the
/// settings it is about to open with. That is the only place that knows both
/// the current settings and the moment a device is about to exist. It is never
/// reached from a render or device callback.
#[cfg(target_os = "ios")]
#[derive(Debug)]
struct Policy;

#[cfg(target_os = "ios")]
impl pr0_runtime::AudioPolicy for Policy {
    fn prepare(&self, preferences: &AudioPreferences) -> Result<(), String> {
        let policy = ios::current_policy(preferences);
        // Logged because the category is the decision with a user-visible
        // consequence — it is what makes iOS ask for the microphone — and the
        // profile log is where a device diagnostic has to be able to show that
        // it followed the settings the engine actually opened with.
        tracing::info!(
            "Audio session prepared for an engine device: category={}, {} Hz, {:.4} s buffer",
            if policy.record {
                "playAndRecord"
            } else {
                "playback"
            },
            policy.preferred_sample_rate,
            policy.preferred_io_duration,
        );
        ios::apply(policy)
    }
}

/// The capability an iOS host installs on its [`pr0_runtime::RuntimeConfig`].
#[cfg(target_os = "ios")]
pub(crate) fn policy() -> std::sync::Arc<dyn pr0_runtime::AudioPolicy> {
    std::sync::Arc::new(Policy)
}

/// The platform *route* capability an iOS host installs on its
/// [`pr0_runtime::RuntimeConfig`].
///
/// This is what lets the runtime describe its audio routes and decide whether
/// capture may be opened without ever asking CPAL, whose iOS backend answers
/// those questions by initializing a RemoteIO audio unit — the call that aborts
/// the process when AudioToolbox's RPC times out. Everything it reports comes
/// from `AVAudioSession`, which is a property read and a permission read, not
/// an audio unit.
#[cfg(target_os = "ios")]
pub(crate) fn routes() -> std::sync::Arc<dyn pr0_runtime::AudioRoutes> {
    std::sync::Arc::new(Routes)
}

#[cfg(target_os = "ios")]
#[derive(Debug)]
struct Routes;

#[cfg(target_os = "ios")]
impl pr0_runtime::AudioRoutes for Routes {
    fn describe(&self) -> pr0_runtime::RouteDescription {
        ios::describe_route()
    }

    fn capture_support(&self) -> CaptureSupport {
        CAPTURE
    }

    fn capture_authorization(&self) -> CaptureAuthorization {
        ios::record_permission()
    }

    fn request_capture_authorization(&self) {
        ios::request_record_permission();
    }
}

#[cfg(target_os = "ios")]
mod ios {
    use super::{RESUME_DEADLINE, SUSPEND_DEADLINE, SessionPolicy};
    use block2::RcBlock;
    use objc2::rc::Retained;
    use objc2::runtime::Bool;
    use objc2_avf_audio::{
        AVAudioSession, AVAudioSessionCategory, AVAudioSessionCategoryOptions,
        AVAudioSessionCategoryPlayAndRecord, AVAudioSessionCategoryPlayback,
        AVAudioSessionInterruptionNotification, AVAudioSessionInterruptionOptionKey,
        AVAudioSessionInterruptionOptions, AVAudioSessionInterruptionType,
        AVAudioSessionInterruptionTypeKey, AVAudioSessionMediaServicesWereResetNotification,
        AVAudioSessionModeDefault, AVAudioSessionRecordPermission,
        AVAudioSessionRouteChangeNotification, AVAudioSessionRouteChangeReason,
        AVAudioSessionRouteChangeReasonKey, AVAudioSessionSetActiveOptions,
    };
    use objc2_foundation::{
        NSNotification, NSNotificationCenter, NSNotificationName, NSNumber, NSString, NSUInteger,
    };
    use objc2_ui_kit::{
        UIApplicationDidBecomeActiveNotification, UIApplicationDidEnterBackgroundNotification,
    };
    use pr0_runtime::{
        AudioPreferences, CaptureAuthorization, RouteDescription, RuntimeConfig, RuntimeHandle,
    };

    use std::{
        ptr::NonNull,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering},
        },
    };

    /// Installs the session policy before the runtime can open a device.
    ///
    /// Reported rather than fatal: an application that cannot configure its
    /// audio session still opens projects, edits scores and serves its
    /// interface, and the failure belongs in the profile log next to whatever
    /// the device layer reports afterwards.
    pub(crate) fn prepare(config: &RuntimeConfig) {
        if let Err(error) = apply(current_policy(&pr0_runtime::audio_preferences(config))) {
            tracing::warn!("The iOS audio session was not configured: {error}");
        }
    }

    /// The policy for `preferences` against what the platform reports *now*.
    ///
    /// Read at every install rather than cached, because both inputs change
    /// under a running application: a person answers the microphone prompt, and
    /// a route with an input appears or disappears when an interface is plugged
    /// in. Neither raises anything this bridge observes, so the answer has to be
    /// fetched at the moment it is used.
    pub(super) fn current_policy(preferences: &AudioPreferences) -> SessionPolicy {
        let route = describe_route();
        SessionPolicy::new(
            preferences,
            super::CAPTURE,
            record_permission(),
            route.input_available,
        )
    }

    /// What `AVAudioSession` says about the current route.
    ///
    /// Property reads only. Nothing here constructs an audio unit, so none of
    /// it can reach `AudioUnitInitialize` and the AudioToolbox RPC timeout that
    /// aborts the process.
    ///
    /// The *input* properties are read only when this host can actually record.
    /// They are not free: observed on iPadOS 26.5, asking `AVAudioSession`
    /// about input availability is itself enough to put the system's microphone
    /// prompt on screen — at launch, with no feature in use and nothing opened.
    /// An application that cannot record must not ask a person for a microphone
    /// it will never use, so when [`super::CAPTURE`] is `Unimplemented` those
    /// reads do not happen and the runtime is told there is no input route,
    /// which is the truth as far as this build is concerned.
    pub(super) fn describe_route() -> RouteDescription {
        let session = unsafe { AVAudioSession::sharedInstance() };
        let channels = |count: objc2_foundation::NSInteger| u16::try_from(count).unwrap_or(0);
        let rate = unsafe { session.sampleRate() };
        let capture = super::CAPTURE.is_supported();
        RouteDescription {
            output_channels: channels(unsafe { session.outputNumberOfChannels() }),
            // iOS reports zero input channels outside an active recording
            // session, and the runtime reads zero as "not known yet".
            input_channels: capture
                .then(|| channels(unsafe { session.inputNumberOfChannels() }))
                .unwrap_or(0),
            input_available: capture && unsafe { session.isInputAvailable() },
            sample_rate: (rate.is_finite() && rate > 0.).then(|| rate as u32),
        }
    }

    /// Microphone authorization, mapped onto the runtime's vocabulary.
    ///
    /// Uses `AVAudioSession`'s accessor rather than `AVAudioApplication`'s: the
    /// latter is the supported spelling from iOS 17, and this application's
    /// deployment target is 14.0, so reaching it would need a runtime
    /// availability check for no behavioral difference. It reports three
    /// states, not four — `Restricted` has no `AVAudioSession` spelling at all
    /// — and the runtime still distinguishes them, because a host that can
    /// report a policy restriction should not have to flatten it into a
    /// refusal the user is told to fix in Settings.
    #[allow(deprecated)]
    pub(super) fn record_permission() -> CaptureAuthorization {
        let session = unsafe { AVAudioSession::sharedInstance() };
        match unsafe { session.recordPermission() } {
            AVAudioSessionRecordPermission::Granted => CaptureAuthorization::Granted,
            AVAudioSessionRecordPermission::Denied => CaptureAuthorization::Denied,
            AVAudioSessionRecordPermission::Undetermined => CaptureAuthorization::Undetermined,
            // A state this SDK does not define. Treated as undecided rather
            // than granted: the conservative direction is the one that does not
            // open a capture unit.
            other => {
                tracing::warn!("Unrecognised microphone authorization state: {}", other.0);
                CaptureAuthorization::Undetermined
            }
        }
    }

    /// Puts the system's microphone prompt on screen if nobody has answered it.
    ///
    /// Returns immediately. The completion block runs on whatever thread the
    /// system chooses and only records the outcome in the profile log — the
    /// authoritative read is [`record_permission`], made again the next time
    /// capture is about to be opened. Nothing waits on a person here: this is
    /// called from the orchestration worker, and blocking it would stall every
    /// command behind it, shutdown included.
    #[allow(deprecated)]
    pub(super) fn request_record_permission() {
        let session = unsafe { AVAudioSession::sharedInstance() };
        let answered = RcBlock::new(move |granted: Bool| {
            tracing::info!(
                "Microphone access was {}",
                if granted.as_bool() {
                    "granted"
                } else {
                    "refused"
                }
            );
        });
        unsafe { session.requestRecordPermission(&answered) };
    }

    /// Hands the audio session back at application exit, after the runtime's
    /// ordered shutdown has already closed the devices. Reported rather than
    /// fatal: the process is going away either way.
    pub(crate) fn release() {
        if let Err(error) = deactivate() {
            tracing::warn!("The iOS audio session was not released at exit: {error}");
        }
    }

    /// Observes the audio-session and application lifecycle events that have to
    /// reach the runtime, for the remaining life of the process.
    pub(crate) fn install(handle: RuntimeHandle, config: RuntimeConfig) {
        // Installed while the application is starting in the foreground.
        let bridge = Arc::new(Bridge {
            handle,
            config,
            active: AtomicBool::new(true),
        });
        let center = NSNotificationCenter::defaultCenter();

        // A phone call, a timer, another application taking the session: the
        // system has already stopped this application's audio, so the hardware
        // is closed to match rather than left in a half-dead state.
        let interrupted = bridge.clone();
        observe(
            &center,
            unsafe { AVAudioSessionInterruptionNotification },
            move |notification| {
                let kind = user_info_number(notification, unsafe {
                    AVAudioSessionInterruptionTypeKey
                });
                if kind == Some(AVAudioSessionInterruptionType::Began.0) {
                    interrupted.suspend("an audio session interruption began");
                } else if kind == Some(AVAudioSessionInterruptionType::Ended.0) {
                    // Without the resume hint the system does not want audio
                    // back yet. Becoming active again resumes it, so the
                    // application is never left permanently silent.
                    let options = user_info_number(notification, unsafe {
                        AVAudioSessionInterruptionOptionKey
                    })
                    .map(AVAudioSessionInterruptionOptions)
                    .unwrap_or(AVAudioSessionInterruptionOptions::empty());
                    if options.contains(AVAudioSessionInterruptionOptions::ShouldResume) {
                        interrupted.resume("an audio session interruption ended");
                    }
                }
            },
        );

        // The route this application was playing to disappeared — a cable or
        // interface removed, or a Bluetooth device lost. The stream opened on
        // the old route cannot survive it, so it is reopened on the new one.
        // This is an instrument whose transport the performer controls, not a
        // media player, so it continues on the replacement route rather than
        // pausing and waiting for an affordance this application does not have.
        let rerouted = bridge.clone();
        observe(
            &center,
            unsafe { AVAudioSessionRouteChangeNotification },
            move |notification| {
                let reason =
                    user_info_number(notification, unsafe { AVAudioSessionRouteChangeReasonKey });
                if reason == Some(AVAudioSessionRouteChangeReason::OldDeviceUnavailable.0) {
                    rerouted.reopen("the audio route was removed");
                }
            },
        );

        // Media services died and restarted: every audio object this process
        // holds is invalid, and the session has to be configured from scratch.
        let reset = bridge.clone();
        observe(
            &center,
            unsafe { AVAudioSessionMediaServicesWereResetNotification },
            move |_| reset.reopen("the system media services were reset"),
        );

        // No background-audio entitlement: leaving the foreground closes the
        // hardware and hands the session back, rather than claiming playback
        // the application is not entitled to continue.
        let backgrounded = bridge.clone();
        observe(
            &center,
            Some(unsafe { UIApplicationDidEnterBackgroundNotification }),
            move |_| {
                backgrounded.active.store(false, Ordering::SeqCst);
                backgrounded.suspend("the application entered the background");
                if let Err(error) = deactivate() {
                    tracing::warn!("The iOS audio session was not deactivated: {error}");
                }
            },
        );

        // Becoming active covers returning from the background and the end of a
        // transient interruption alike. Resuming is idempotent, so the frequent
        // harmless case costs one lock and one comparison.
        let foregrounded = bridge;
        observe(
            &center,
            Some(unsafe { UIApplicationDidBecomeActiveNotification }),
            move |_| {
                foregrounded.active.store(true, Ordering::SeqCst);
                foregrounded.resume("the application became active");
            },
        );
    }

    /// The runtime handle plus the configuration its saved audio preferences are
    /// read from, shared by every observer.
    struct Bridge {
        handle: RuntimeHandle,
        config: RuntimeConfig,
        /// Whether the application is in the foreground. Without a
        /// background-audio entitlement, nothing may reopen the hardware while
        /// it is not, so a route change or a media-services reset arriving in
        /// the background closes and stays closed.
        active: AtomicBool,
    }

    impl Bridge {
        fn policy(&self) -> SessionPolicy {
            current_policy(&pr0_runtime::audio_preferences(&self.config))
        }

        fn suspend(&self, reason: &str) {
            match self.handle.suspend_audio_blocking(SUSPEND_DEADLINE) {
                Ok(()) => tracing::info!("Audio hardware suspended: {reason}"),
                Err(error) => tracing::warn!("Audio hardware not suspended ({reason}): {error}"),
            }
        }

        fn resume(&self, reason: &str) {
            if !self.active.load(Ordering::SeqCst) {
                // There is no background-audio entitlement, so becoming active
                // again is the only thing that reopens the hardware.
                tracing::info!("Audio hardware left suspended in the background ({reason})");
                return;
            }
            // The policy goes back in first: an interrupted session needs
            // reactivating, and a session that came back from a media-services
            // reset has no policy at all. The runtime applies it again through
            // [`policy`] just before it opens a device, from the settings it is
            // actually opening with; this call is what makes the session usable
            // even when nothing is going to be opened.
            if let Err(error) = apply(self.policy()) {
                tracing::warn!("The iOS audio session was not reconfigured ({reason}): {error}");
            }
            match self.handle.resume_audio_blocking(RESUME_DEADLINE) {
                Ok(()) => tracing::info!("Audio hardware resumed: {reason}"),
                Err(error) => tracing::warn!("Audio hardware not resumed ({reason}): {error}"),
            }
        }

        /// Closes and reopens the hardware, which is how a stream moves to a
        /// different route or recovers from a media services reset.
        fn reopen(&self, reason: &str) {
            self.suspend(reason);
            self.resume(reason);
        }
    }

    /// Applies category, mode and the driver preferences, then activates the
    /// session. Safe to repeat: iOS treats it as the current policy, not as an
    /// accumulating one.
    pub(super) fn apply(policy: SessionPolicy) -> Result<(), String> {
        let session = unsafe { AVAudioSession::sharedInstance() };
        let category: &AVAudioSessionCategory = unsafe {
            if policy.record {
                AVAudioSessionCategoryPlayAndRecord
            } else {
                AVAudioSessionCategoryPlayback
            }
        }
        .ok_or("This system has no AVAudioSession categories")?;
        let mode = unsafe { AVAudioSessionModeDefault }.ok_or("This system has no default mode")?;
        // Options are only meaningful for the recording category: play to the
        // speaker rather than the receiver, and let paired Bluetooth output and
        // AirPlay carry the mix. Bluetooth is explicitly not a low-latency
        // target — it is here so the application is usable on that route, not
        // dependable on it.
        let options = if policy.record {
            AVAudioSessionCategoryOptions::DefaultToSpeaker
                | AVAudioSessionCategoryOptions::AllowBluetoothA2DP
                | AVAudioSessionCategoryOptions::AllowAirPlay
        } else {
            AVAudioSessionCategoryOptions::empty()
        };
        unsafe { session.setCategory_mode_options_error(category, mode, options) }
            .map_err(|error| describe("set the audio session category", &error))?;
        // Preferences, not guarantees: the driver answers with its own rate and
        // callback size, and the engine's output queue already follows whatever
        // it chooses.
        unsafe { session.setPreferredSampleRate_error(policy.preferred_sample_rate) }
            .map_err(|error| describe("set the preferred sample rate", &error))?;
        unsafe { session.setPreferredIOBufferDuration_error(policy.preferred_io_duration) }
            .map_err(|error| describe("set the preferred I/O buffer duration", &error))?;
        unsafe { session.setActive_error(true) }
            .map_err(|error| describe("activate the audio session", &error))
    }

    /// Hands the session back, telling anything the application interrupted
    /// that it may resume.
    fn deactivate() -> Result<(), String> {
        let session = unsafe { AVAudioSession::sharedInstance() };
        unsafe {
            session.setActive_withOptions_error(
                false,
                AVAudioSessionSetActiveOptions::NotifyOthersOnDeactivation,
            )
        }
        .map_err(|error| describe("deactivate the audio session", &error))
    }

    fn describe(action: &str, error: &objc2_foundation::NSError) -> String {
        format!("Could not {action}: {error:?}")
    }

    /// Registers `handler` for `name` for the life of the process.
    ///
    /// `None` skips the observation: the audio-session notification names are
    /// weakly linked, and a system that does not provide one cannot deliver it
    /// either. A `None` queue runs the handler on the thread that posted the
    /// notification, which is what lets a background transition complete before
    /// the system freezes the process.
    fn observe<F>(center: &NSNotificationCenter, name: Option<&NSNotificationName>, handler: F)
    where
        F: Fn(&NSNotification) + Send + Sync + 'static,
    {
        let Some(name) = name else {
            tracing::warn!("An audio session notification name is unavailable on this system");
            return;
        };
        let block = RcBlock::new(move |notification: NonNull<NSNotification>| {
            // SAFETY: the notification is valid for the duration of the call.
            handler(unsafe { notification.as_ref() });
        });
        // SAFETY: the block only reads the notification it is given, and is
        // sendable — it captures an `Arc<Bridge>`, whose contents are `Send +
        // Sync`.
        let token = unsafe {
            center.addObserverForName_object_queue_usingBlock(Some(name), None, None, &block)
        };
        // The notification center does not keep the returned observer alive.
        // These observations last as long as the process, so the token is
        // deliberately leaked instead of being parked in a global that would
        // never be read.
        std::mem::forget(token);
    }

    /// One `NSNumber` out of a notification's `userInfo`, or `None` if the key
    /// is unavailable, absent or not a number.
    fn user_info_number(
        notification: &NSNotification,
        key: Option<&NSString>,
    ) -> Option<NSUInteger> {
        let info = notification.userInfo()?;
        let value: Retained<objc2::runtime::AnyObject> = info.objectForKey(key?)?;
        Some(value.downcast::<NSNumber>().ok()?.unsignedIntegerValue())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The category decision is the one with a user-visible consequence: the
    /// recording category is what makes this process a recording client, so it
    /// must follow an actually enabled input rather than the application's
    /// capabilities.
    #[test]
    fn the_recording_category_follows_an_enabled_input_not_the_applications_capability() {
        let mut preferences = AudioPreferences {
            sample_rate: 48000,
            block_size: 128,
            capture_inputs: false,
        };
        let playback = SessionPolicy::new(&preferences, CaptureSupport::Supported, CaptureAuthorization::Granted, true);
        assert!(!playback.record);

        preferences.capture_inputs = true;
        assert!(SessionPolicy::new(&preferences, CaptureSupport::Supported, CaptureAuthorization::Granted, true).record);
    }

    /// An enabled input is necessary but not sufficient. `playAndRecord` puts
    /// this process on the audio server's input path, and doing that with no
    /// permission or no input route is exactly the configuration whose
    /// AudioToolbox RPC timed out and aborted the application.
    #[test]
    fn the_recording_category_also_requires_permission_and_a_connected_input() {
        let wants_capture = AudioPreferences {
            sample_rate: 48000,
            block_size: 128,
            capture_inputs: true,
        };
        for refused in [
            CaptureAuthorization::Denied,
            CaptureAuthorization::Restricted,
            CaptureAuthorization::Undetermined,
        ] {
            assert!(
                !SessionPolicy::new(&wants_capture, CaptureSupport::Supported, refused, true)
                    .record,
                "{refused:?} must not select the recording category"
            );
        }
        // Permitted, but nothing connected to record from.
        assert!(
            !SessionPolicy::new(
                &wants_capture,
                CaptureSupport::Supported,
                CaptureAuthorization::Granted,
                false
            )
            .record
        );
        // A host on a platform that does not gate capture is permitted too.
        assert!(
            SessionPolicy::new(
                &wants_capture,
                CaptureSupport::Supported,
                CaptureAuthorization::NotRequired,
                true
            )
            .record
        );
    }

    /// The measured iPadOS finding, expressed as a test: a backend that cannot
    /// record must not reach the recording category however permissive
    /// everything else is. Installing `playAndRecord` is itself what aborts the
    /// process when RemoteIO initializes, so this is what keeps *playback*
    /// working, not only what keeps capture honest.
    #[test]
    fn an_unsupported_capture_backend_never_selects_the_recording_category() {
        let policy = SessionPolicy::new(
            &AudioPreferences {
                sample_rate: 48000,
                block_size: 128,
                capture_inputs: true,
            },
            CaptureSupport::Unimplemented("not in this build"),
            CaptureAuthorization::Granted,
            true,
        );
        assert!(!policy.record);
        // This host's shipped answer, so the test fails the moment capture is
        // turned on without revisiting it.
        assert_eq!(CAPTURE, CaptureSupport::Unimplemented(
            "Native audio input is not available on iPadOS in this build. \
             Audio output, imported samples and network sources are unaffected",
        ));
    }

    /// Output must never be held hostage by a capture problem: an application
    /// with a denied microphone still plays.
    #[test]
    fn a_refused_microphone_still_leaves_a_usable_playback_session() {
        let policy = SessionPolicy::new(
            &AudioPreferences {
                sample_rate: 44100,
                block_size: 128,
                capture_inputs: true,
            },
            CaptureSupport::Supported,
            CaptureAuthorization::Denied,
            false,
        );
        assert!(!policy.record);
        assert_eq!(policy.preferred_sample_rate, 44100.);
    }

    /// The driver preferences are the engine's own configured format, so the
    /// session asks for the buffer the engine actually renders.
    #[test]
    fn the_session_asks_for_the_engines_configured_format() {
        let policy = SessionPolicy::new(
            &AudioPreferences {
                sample_rate: 44100,
                block_size: 256,
                capture_inputs: false,
            },
            CaptureSupport::Supported,
            CaptureAuthorization::Granted,
            true,
        );
        assert_eq!(policy.preferred_sample_rate, 44100.);
        assert!((policy.preferred_io_duration - 256. / 44100.).abs() < f64::EPSILON);
        // A 128-frame block at 48 kHz is the default, and is under three
        // milliseconds — a low-latency request, not a media-playback one.
        let default = SessionPolicy::new(
            &AudioPreferences {
                sample_rate: 48000,
                block_size: 128,
                capture_inputs: false,
            },
            CaptureSupport::Supported,
            CaptureAuthorization::Granted,
            true,
        );
        assert!(default.preferred_io_duration < 0.003);
    }

    /// Both deadlines have to fit inside the few seconds iOS allows an
    /// application after it leaves the foreground, and closing must not be the
    /// slower of the two.
    #[test]
    fn lifecycle_deadlines_stay_inside_the_systems_own_window() {
        assert!(SUSPEND_DEADLINE <= RESUME_DEADLINE);
        assert!(RESUME_DEADLINE < std::time::Duration::from_secs(5));
    }
}
