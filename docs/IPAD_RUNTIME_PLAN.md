# Shared runtime and iPad build plan

Status: planning, plus Phase 1 in `crates/server`, Phase 2 in the desktop
application, the import half of Phase 3, the source/configuration half of Phase
4, and the architecture half of Phase 5. The tree compiles and links for
`aarch64-apple-ios-sim`, an Xcode project is generated from
`tauri.ios.conf.json`, and an unsigned debug simulator bundle builds, launches
and reaches its signed-in interface. The webview cookie abort that previously
blocked every Phase 4 gate item is fixed: see "Webview session bootstrap" under
Phase 4. A simulator run has created a project, opened CoreAudio output and
suspended/resumed it across background/foreground cycles. The device path is now
a logical-route model on iOS rather than CPAL enumeration, and native capture is
refused behind a typed capability rather than aborting the process: see "The
input blocker, resolved into a safe refusal" under Phase 5. Native audio input
is therefore not available on iPadOS in this build. No device run, no physical
audio, no TestFlight build and no App Store submission is implemented or
verified.

Implemented so far (see "Phase 1", "Phase 2" and "Phase 3" below for what
remains):

- the server crate builds a `pr0_runtime` library target beside the `pr0-server`
  binary;
- typed `RuntimeConfig` (filesystem roots, capabilities, discovery, listener
  policy) replaces `PR0_*` lookups below the process host;
- a public embeddable lifecycle API — `pr0_runtime::start(RuntimeConfig)`
  returning a `RuntimeHandle` with `readiness()`, `host_on_lan`/`stop_hosting`/
  `hosting_status`, and one ordered idempotent `shutdown()`;
- `pr0-server --desktop` reduced to a stdin/stdout adapter over that API, with
  its handshake and control protocol unchanged; and
- the Tauri desktop application embedding that runtime in-process: no child
  process, no control pipe, and no bundled `pr0-server` executable; and
- a typed `AudioImporter` capability on `RuntimeConfig` with a pure-Rust
  Symphonia implementation as the cross-platform baseline, the hardened FFmpeg
  process importer retained as a separate desktop/server fallback, and a chain
  that falls back only for unknown formats — so an embedded host can import
  audio with no executable at all; and
- the Tauri package converted to the Tauri 2 library entry pattern, with an
  iOS/iPadOS host module beside the desktop one: `src/lib.rs` exposes `run()`
  behind `#[cfg_attr(mobile, tauri::mobile_entry_point)]`, `src/main.rs` is only
  the desktop launcher, every desktop affordance is compiled behind
  `cfg(desktop)`, and `tauri.ios.conf.json`/`Info.ios.plist` carry the iOS
  packaging overrides; and
- an `AVAudioSession` policy and lifecycle bridge in that iOS shell, over one
  idempotent `suspend_audio`/`resume_audio` pair on `RuntimeHandle` that every
  host shares, with the session policy also installed as a typed
  `RuntimeConfig::audio_policy` capability the orchestration worker calls
  immediately before it opens a stream (see Phase 5 below); and
- a platform device boundary (`crates/server/src/native_audio.rs`) with two
  models — enumerated devices everywhere the runtime already ran, and logical
  routes on iOS — plus a typed `RuntimeConfig::audio_routes` capability carrying
  the route description, whether the host's backend can capture at all, and the
  microphone authorization; and
- a one-time HTTP session handoff (`RuntimeHandle::bootstrap_url`) that installs
  the private owner session in an embedded host's own webview without the host
  or the webview API ever touching the credential (see Phase 4 below).

## Goal

Run the same pr0former application runtime from three hosts:

1. the standalone `pr0-server` executable;
2. the Tauri desktop application; and
3. a Tauri iOS/iPadOS application.

The first iPad milestone is a foreground-only app that can run its own local
engine and optionally host authenticated clients on the local network. Reliable
operation after screen lock or while suspended is not part of that milestone.
Android should use the same runtime boundaries later, but iPadOS is the first
mobile target.

## Current constraints

- ~~`crates/server/src/main.rs` currently combines application state, database
  migration, engine startup, router construction, listener policy, TLS,
  discovery, desktop authentication, and process lifecycle.~~ The binary is a
  thin process host over `pr0_runtime`.
- ~~The desktop Tauri process starts `pr0-server` as a child executable and
  controls it through stdin/stdout.~~ It embeds the runtime. It still packages a
  separate FFmpeg executable, now only as the fallback behind the in-process
  decoder; whether to stop packaging it is a separate decision.
- ~~Runtime paths and capabilities are selected in many modules through `PR0_*`
  environment variables.~~ `RuntimeConfig` carries them, including the typed
  `AudioImporter` capability.
- ~~Sample import launches FFmpeg~~, decodes the first audio stream, limits it
  to 30 seconds and eight channels, resamples it, and stores canonical 32-bit
  float WAV. Import now goes through the host's `AudioImporter`; FFmpeg is one
  optional implementation of it rather than the only path.
- The DSP and device callbacks must retain their existing no-allocation,
  no-locking, no-I/O contract. Mobile lifecycle work must stay outside those
  callbacks.
- Mobile audio, MIDI, local networking, WebRTC, recording endurance, app
  suspension, and physical latency are all unverified device paths.

## Target architecture

### `pr0-runtime`

Create a library crate that owns the application runtime now living in
`pr0-server`:

- database opening, migrations, accounts, revisions, samples, and subgraphs;
- engine orchestration and persistence workers;
- Axum API/WebSocket/WebRTC router construction;
- local-owner session creation;
- optional OSC, MIDI, discovery, TLS, and LAN hosting services; and
- coordinated shutdown, including device closure and loop/recording barriers.

Its public API should be small and host-oriented:

```rust
pub struct RuntimeConfig {
    pub data_dir: PathBuf,
    pub recordings_dir: PathBuf,
    pub web_assets: WebAssets,
    pub identity: IdentityMode,
    pub network: NetworkPolicy,
    pub capabilities: RuntimeCapabilities,
    pub importer: Arc<dyn AudioImporter>,
}

pub struct RuntimeHandle {
    pub local_url: Url,
    pub local_session: Option<String>,
    // private task and shutdown state
}

impl RuntimeHandle {
    pub async fn host_on_lan(&self, options: HostingOptions) -> Result<HostingStatus>;
    pub async fn stop_hosting(&self) -> Result<()>;
    pub async fn shutdown(self) -> Result<()>;
}
```

The exact types can change during extraction, but the properties should not:

- no environment-variable reads below the host adapter;
- no process-global working-directory assumptions;
- all filesystem roots come from `RuntimeConfig`;
- unavailable platform functions are explicit capabilities, not runtime panics;
- every host reaches the same router and engine implementation; and
- shutdown has one ordered implementation shared by all hosts.

### Host adapters

`pr0-server` becomes a thin executable that parses environment/CLI settings,
constructs `RuntimeConfig`, starts standalone HTTP/HTTPS listeners, handles
signals, and awaits `RuntimeHandle::shutdown`.

The desktop Tauri host embeds `pr0-runtime` in-process. Its current file lock,
local owner, session bootstrap, windows, discovery menu, and hosting dialog remain,
but the child-process protocol and bundled `pr0-server` executable disappear.
Desktop exit should invoke the same ordered shutdown path and retain the current
bounded finalization behavior.

The Tauri package exposes a library entry point with
`#[cfg_attr(mobile, tauri::mobile_entry_point)]`. Desktop menus, the connection
chooser, certificate pinning, saved window layouts, LAN discovery browsing, the
hosting dialog, the profile lock and the bundled converter stay behind
`cfg(desktop)`; iOS receives a focused single-window shell and platform-specific
Tauri configuration. Both hosts share one `runtime` module, so the loopback URL,
readiness contract, private owner cookie and ordered shutdown have a single
implementation. The local Vue interface continues to use the loopback HTTP API
so the API, authentication, WebSocket, and browser integration paths do not fork.

The iPad host supplies app-container paths, configures native audio before engine
startup, maps foreground/background and interruption events into runtime state,
and requests microphone/local-network permissions only when their features are
used. LAN hosting is opt-in for each launch.

## Audio-device plan

The existing CPAL dependency contains CoreAudio support for iOS and AAudio
support for Android, so the first implementation should keep CPAL rather than
fork the DSP engine.

Add a narrow platform-audio preparation layer outside the callback path
(implemented for iOS in Phase 5 below; Android's half is untouched):

- ~~iOS configures `AVAudioSession` category, mode, preferred sample rate and
  preferred I/O duration before CPAL opens a stream;~~ it does, in
  `desktop/src-tauri/src/audio_session.rs`, from the runtime's own saved audio
  settings;
- ~~route changes, interruptions, media-service resets and permission changes
  produce orchestration commands and visible diagnostics;~~ route loss,
  interruptions and media-service resets become ordered `Suspend` commands on
  the orchestration worker, and `audio_suspended` is reported through
  `/api/devices` and telemetry. Permission changes do not yet produce anything;
- ~~iOS presents logical routes rather than assuming desktop-style device
  enumeration.~~ It does, in `crates/server/src/native_audio.rs`. This was a
  correctness requirement, not a presentation preference: CPAL's iOS backend
  initializes an AudioUnit merely to enumerate supported configurations, and
  AudioToolbox aborts the whole process if that RPC times out. iOS now publishes
  one fixed output route and one fixed input route, resolves them to CPAL's
  default device, and takes channel counts from an `AVAudioSession` reading
  supplied by the host, with the enumerating code not compiled for the target at
  all. See Phase 5 below;
- Android later supplies its activity/context setup and foreground-service policy;
  and
- unsupported sample-rate/channel combinations fail before graph activation.

The foreground iPad milestone must pass with built-in output first. USB-C audio
interfaces, wired headphones, Bluetooth, microphone input, and multichannel
interfaces are separate manual gates. Bluetooth is not a low-latency performance
target.

Background audio should be a later, explicit scope. iPadOS audio background mode
can support an actively audible app, but it is not a general-purpose guarantee
that LAN server work will run forever. Hosting must surface suspension and
interruption state rather than claiming uninterrupted server availability.

## Audio import and FFmpeg

### Recommendation

Do not make statically linked FFmpeg the first mobile solution. It adds a large C
cross-build, App Store packaging work, codec/license review, and ongoing security
maintenance to the most fragile part of the port. It also preserves a much
broader format promise than pr0former needs for short samples.

Instead, introduce an `AudioImporter` interface and make a bounded in-process
decoder the cross-platform baseline. Implemented in `crates/server/src/import/`
as:

```rust
pub trait AudioImporter: Send + Sync + Debug {
    fn name(&self) -> &str;
    fn formats(&self) -> Vec<&'static str>;
    fn decode(&self, source: &ImportSource, limits: &ImportLimits)
        -> Result<DecodedAudio, ImportError>;
}
```

`ImportSource` is one already-received local blob or file, never a URL or a
directory. `decode` is synchronous and CPU-bound, so callers run it on a
blocking worker. `RuntimeConfig::importer` is an `Arc<dyn AudioImporter>`, so a
host chooses the capability rather than handing the runtime an executable path.

The pinned Symphonia release covers the common formats needed by musicians:
WAV (PCM, IEEE float, ADPCM), AIFF/AIFF-C, CAF, FLAC, MP3, MP4/M4A (AAC-LC,
ALAC), ADTS AAC and Ogg (Vorbis, FLAC). The enabled feature list is explicit;
MKV/WebM, Opus, MPEG Layer I/II and the nightly SIMD features are deliberately
off and should be enabled only after their decoder maturity and corpus tests
pass. `DecodedAudio` writes the canonical float WAV with `hound` and both
importers reuse the existing windowed-sinc resampler in `samples::resample`.

The process-based FFmpeg implementation is retained as an optional
standalone/desktop compatibility adapter. The runtime receives the adapter; it
never discovers or launches FFmpeg itself. Once the common decoder passes the
golden corpus on all supported hosts, decide whether desktop FFmpeg's extra format
coverage is worth retaining. Mobile never depends on an executable sidecar.

The importer must preserve the current security contract, and both shipped
implementations do:

- inspect bytes rather than trust filename extensions;
- select one audio stream and ignore video/subtitles/metadata;
- reject zero channels, more than eight channels, nonfinite PCM, and output over
  30 seconds;
- enforce compressed-input, decoded-frame, memory, time, and temporary-file
  limits during decoding rather than after allocation;
- never resolve playlists, nested files, or network URLs; and
- perform decoding, resampling, and WAV writing away from render/device callbacks.

Build a corpus containing canonical fixtures for every supported
container/codec, channel count, sample rate, bit depth, malformed/truncated input,
oversized duration, metadata, and files containing video plus audio. Compare the
new decoder with the current FFmpeg result for frame count, channel order, DC,
RMS, peak, frequency response, and finite output. Exact PCM equality is not
required for lossy codecs; documented numeric tolerances are.

`crates/server/src/import/tests.rs` builds the uncompressed part of that corpus
at run time rather than checking in binaries: every RIFF/WAVE fixture is
assembled byte by byte, so the expected PCM is known exactly and no encoder's
header choices are involved. It covers 1/2/8 channels at 16/24/32-bit integer
and 32-bit float, resampling checked against `samples::resample` directly, the
channel/duration/memory/input-size limits, an unidentifiable blob, an empty
container, a truncated header, the no-importer configuration, chain fallback
classification, the hardened converter argument list, and the canonical output
read back through `hound`. The compressed and differential halves — real MP3,
AAC, ALAC, Vorbis, CAF and video-plus-audio fixtures compared against FFmpeg
with documented tolerances — are still open, as are fuzz targets.

If a required production format cannot be covered reliably, the fallback choices
are, in order: add a focused in-process codec crate, use AVFoundation/MediaCodec
behind the same importer contract, or build audited FFmpeg libraries for mobile.
The final option needs a separate license and App Store review before adoption.

## Delivery phases and gates

### Phase 0: baseline and decisions

- Record current standalone, desktop, Rust, frontend, browser, sample-import, and
  shutdown test results.
- Choose the permanent iOS bundle identifier before the first App Store Connect
  upload. `tauri.ios.conf.json` currently proposes `org.pr0former.mobile`, distinct
  from the desktop `org.pr0former.desktop` because an iPad application should not
  be registered under a desktop identifier. Nothing has been registered with
  Apple, so this is still changeable; it stops being changeable at the first
  upload.
- Decide whether the initial iPad app exposes LAN hosting or only its local engine;
  keep the capability switch either way.
- Add architecture tests that start and stop an isolated runtime without a child
  process.

Gate: no behavior changes and a clean baseline on existing platforms.

### Phase 1: extract the shared runtime

- ~~Move server modules into `pr0-runtime`~~ — done as a library target
  (`pr0_runtime`) inside the existing `pr0-server` package; a separate crate is
  still open.
- ~~Introduce typed paths, listener policy, identity policy and capabilities.~~
  `RuntimeConfig`/`ListenerPolicy`/`RuntimeMode` carry them; a typed
  `AudioImporter` capability is still Phase 3.
- ~~Split router construction from listener startup.~~ `assemble` builds the
  application; `start` binds listeners.
- ~~Return startup information directly instead of printing a desktop
  handshake.~~ `Readiness` carries the URL and private session; only the
  `--desktop` adapter prints.
- ~~Centralize graceful shutdown and remove runtime calls to
  `std::process::exit`.~~ `RuntimeHandle::shutdown` is the one ordered path.
- Keep `pr0-server` behavior, routes, defaults, TLS, discovery and status output
  compatible.
- Still open: the route-parity test against an embedded runtime.

Gate: standalone tests and browser integration tests pass unchanged; a parity
test exercises every registered API route from the embedded runtime.

### Phase 2: embed the desktop runtime

- ~~Make the Tauri crate depend on `pr0-runtime`.~~ `desktop/src-tauri` takes a
  path dependency on the `pr0-server` package as `pr0_runtime`, and its separate
  workspace repeats the repository's vendored CPAL patch.
- ~~Replace `Child`, stdin/stdout handshake and forced child termination with a
  managed `RuntimeHandle`.~~ Managed `runtime::Embedded` state exposes the handle
  and, on desktop, holds the profile lock; the hosting dialog calls `host_on_lan`/
  `stop_hosting`/`hosting_status` directly.
- ~~Keep profile locking, local authentication, private loopback binding, cookie
  bootstrap, LAN hosting controls and 30-second shutdown ceiling.~~ All retained;
  the ceiling now bounds a wait rather than a kill, because there is no process
  to terminate.
- ~~Stop packaging `pr0-server`.~~ Removed from `externalBin` and from both
  build entry points' staging. FFmpeg stays the one external binary; Phase 3
  made it a fallback rather than the only import path, and removing it from
  packaging is still a separate decision. Standalone packaging and `init.sh`
  are unchanged.
- ~~The desktop `Cargo.lock` must be regenerated for the `pr0_runtime` path
  dependency and for `symphonia` before a `--locked` build.~~ Both lockfiles are
  current: `cargo metadata --locked` succeeds in the workspace and in
  `desktop/src-tauri`. The gate below is still unperformed.

Gate: macOS, Linux and Windows builds pass; desktop integration verifies local
login, remote windows, hosting start/stop, recording finalization and clean exit.
None of these have been run against a packaged application yet.

### Phase 3: replace the common import path

- ~~Add the importer contract and Symphonia implementation.~~
  `crates/server/src/import/` holds the object-safe `AudioImporter` trait,
  `ImportSource`, `ImportLimits`, `DecodedAudio`, the pure-Rust
  `SymphoniaImporter`, the retained `FfmpegImporter`, and the `ImporterChain`
  that falls through only on unknown formats. `RuntimeConfig::importer` carries
  the capability; `import::pure_rust()` is the no-executable host choice.
- ~~Move canonical PCM validation/resampling/WAV writing into shared code.~~
  `DecodedAudio` owns the output contract (1–8 channels, non-empty, finite, at
  most 30 seconds) and the canonical float-WAV writer; both importers resample
  through the existing offline windowed-sinc path in `samples::resample`.
- ~~Retain an optional FFmpeg adapter for compatibility.~~ `FfmpegImporter`
  keeps the `fd`-only protocol whitelist, first-audio-stream mapping, metadata
  removal, duration/output-size caps, single-threaded child, 60-second timeout
  with a kill, and its actionable diagnostics. It is now compiled only for hosts
  that can spawn a process: `#[cfg(not(any(target_os = "ios", target_os =
  "android")))]` on the module, so on those targets `FfmpegImporter` does not
  exist and `import::standard` yields the Symphonia chain regardless of its
  argument.
- Partly done: generated corpus tests cover integer and float WAV at 1/2/8
  channels and 16/24/32 bits, resampling against the offline filter, the
  channel/duration/memory/input-size limits, unidentifiable and empty input,
  the no-importer configuration, fallback classification, the hardened
  converter argument list, and the canonical output read back through `hound`.
  Still open: a *differential* corpus comparing Symphonia against FFmpeg on
  lossy fixtures with documented numeric tolerances, and fuzz targets.
- Still open: publishing the portable format list in the UI. It is published in
  `README.md` and `docs/ARCHITECTURE.md`, and is available programmatically
  from `AudioImporter::formats`.
- ~~Adding `symphonia` to the repository and desktop lockfiles, and building the
  workspace with it.~~ Both lockfiles resolve with `--locked`, and the workspace
  and its tests build with it.

Gate: unperformed. Portable formats have not been run on macOS/Linux/Windows
from this change, and the iOS cross-compilation has only been built, never run.
Desktop FFmpeg
removal remains a separate decision, and the residual risk that justifies
keeping it is Symphonia's narrower container coverage (no MKV/WebM, no Opus, no
MPEG Layer I/II, and less tolerance of malformed or unusual files) plus the
uneven decoder maturity across its codecs.

### Phase 4: create the iOS shell

- ~~Add `tauri.ios.conf.json`.~~ It overrides the bundle identifier
  (`org.pr0former.mobile`), empties
  `externalBin` and the security capabilities, pins the minimum iOS version to
  Tauri 2's own default (14.0), points at `Info.ios.plist`, and names the Apple
  frameworks the Rust dependencies link (see "Native linkage" below).
- ~~Run `tauri ios init`.~~ `desktop/src-tauri/gen/apple` now holds a generated
  XcodeGen `project.yml` and Xcode project. That directory is ignored by Git and
  is regenerated from `tauri.ios.conf.json`, so every durable iOS project change
  belongs in the Tauri configuration, not in the generated files.
- ~~Add the mobile Rust entry point, app-container paths, local session
  bootstrap, one mobile window and orientation policy.~~ `src/lib.rs` is the
  library entry (`run()` under `#[cfg_attr(mobile, tauri::mobile_entry_point)]`),
  `src/mobile.rs` is the iOS host, and `src/desktop.rs` keeps the unchanged
  desktop host. iPad icon assets already exist in `icons/ios`; launch assets come
  from the Tauri iOS template and have not been reviewed.
- ~~Add microphone, local-network and Bonjour purpose strings.~~ In
  `Info.ios.plist`, together with the iPhone/iPad device family, the
  portrait/landscape orientation policy and `NSAllowsLocalNetworking` for the
  application's own loopback listener. There is deliberately no background-audio
  entitlement.
- Still open: `PrivacyInfo.xcprivacy`, after auditing Rust/Tauri dependencies for
  required reason APIs and collected-data declarations.
- Changed from the original plan: this slice enables native devices rather than
  starting with them disabled, so the first simulator/device run exercises the
  real CPAL path. If that proves to block UI/persistence testing, clear
  `native_devices` in `runtime::configuration` for mobile — it is one field.
- Partly done: cross-target dependency gating in `pr0_runtime`, from the reported
  `rquickjs-sys` failure. `crates/server/Cargo.toml` re-declares `rquickjs` with
  its `bindgen` feature under
  `[target.'cfg(any(target_os = "ios", target_os = "android"))'.dependencies]`,
  because `rquickjs-sys` 0.13 has no pregenerated bindings for
  `aarch64-apple-ios`/`aarch64-apple-ios-sim` and names that feature as the
  remedy. Cargo applies a target section only to matching targets, so desktop and
  server builds keep the pregenerated bindings and acquire no libclang
  requirement, and QuickJS scripting on iOS is the same engine and the same
  `scripts.rs` rather than a stub. In the same pass `import/ffmpeg.rs` stopped
  being compiled for those targets and `import::standard` returns the Symphonia
  chain there whatever converter name it is handed, so a platform without
  process spawning cannot reach `std::process::Command` through any
  configuration; pure-Rust import is unchanged everywhere. `linux_audio` is
  already unreachable off Linux and needed no gate.

- Resolved — the workspace compiles for an iOS target. `cargo` has now
  built the whole tree for `aarch64-apple-ios-sim`, including `midir`/`coremidi`,
  the vendored `cpal`, `webrtc`, `opus`/`audiopus_sys`, bundled `rusqlite` and
  `rquickjs` with generated bindings. None of the previously listed crates needed
  gating; the remaining iOS work is linkage and runtime behavior, not
  compilation.

#### Native linkage

Cargo builds the iOS application as a `staticlib` and Xcode performs the final
link. A static archive cannot carry link directives, so the
`cargo:rustc-link-lib=framework=…` line that `coremidi-sys` emits and the
`#[link(kind = "framework")]` attributes in `core-foundation`, `objc2-foundation`,
`objc2-core-audio` and `objc2-audio-toolbox` are all dropped on the way to Xcode.
The Tauri iOS template links only CoreGraphics, Metal, MetalKit, QuartzCore,
Security, UIKit and WebKit, so the first archive failed on the undefined CoreMIDI
symbols. Reading the archive's symbol tables shows AudioToolbox and
CoreFoundation references left undefined by the same mechanism, whether or not
the reported error list reached them.

`bundle.iOS.frameworks` in `tauri.ios.conf.json` is the supported fix: Tauri
renders it into the generated XcodeGen `project.yml` as extra `sdk:` dependencies.
The list is CoreMIDI, CoreAudio, AudioToolbox, CoreFoundation and Foundation:

| Framework | Undefined symbols it resolves | Source |
| --- | --- | --- |
| CoreMIDI | `MIDIClientCreate`, `MIDIInputPortCreateWithBlock`, `MIDIObjectGetStringProperty`, `MIDISendEventList`, the `kMIDIProperty*` constants | `midir` → `coremidi` |
| AudioToolbox | `AudioComponentFindNext`, `AudioComponentInstanceNew`, `AudioUnitInitialize`, `AudioUnitRender`, `AudioOutputUnitStart` | vendored `cpal` → `coreaudio-rs` |
| CoreAudio | the `objc2-core-audio` bindings compiled alongside them | `coreaudio-rs` |
| CoreFoundation | `CFStringCreateWithBytes`, `CFRunLoop*`, `CFURL*`, `kCFAllocatorDefault` | `core-foundation`, `coremidi` |
| Foundation | the Objective-C classes `objc2-foundation` references | `objc2-foundation` |
| AVFAudio | the `AVAudioSession` class, category/mode names, notification names and `userInfo` keys | `objc2-avf-audio`, from `src/audio_session.rs` |

Observed in the linked simulator bundle: naming `AVFAudio` makes the linker
record `AVFoundation` among the load commands — AVFAudio is re-exported through
it — and the `AVAudioSession*` symbols bind there. A build made before that
entry existed has no AVF framework of any kind in its load commands.

Because `gen/apple` is ignored by Git, this change only takes effect after the
project is regenerated: delete `desktop/src-tauri/gen/apple` and run
`npm run -- tauri ios init` from `desktop/`. Editing the generated `project.yml`
or `project.pbxproj` directly does not survive that.

Both previously open linkage items are now resolved, and are kept here with
what actually closed them:

- **C deployment target.** `audiopus_sys` builds libopus through the `cmake`
  crate, which copies the `cc` crate's flags into `CMAKE_C_FLAGS`. Its
  `CMakeCache.txt` records
  `-mios-simulator-version-min=26.5 -isysroot …/iPhoneSimulator26.5.sdk`, so
  `cc` saw no `IPHONEOS_DEPLOYMENT_TARGET` and fell back to the installed SDK
  version; the link against a 14.0 minimum then warns for every Opus object.

  Those objects were produced by a standalone `cargo build` run before the Xcode
  project existed and were reused from the Cargo cache by the Xcode build, so
  this does *not* establish whether Tauri's `ios xcode-script` path supplies the
  variable — the Tauri CLI binary does carry `IPHONEOS_DEPLOYMENT_TARGET` among
  the environment variables its Rust interface sets. What it does establish is
  that a plain Cargo invocation for an iOS target gets the wrong value, and that
  the cached result survives into the Xcode build.

  The lever that reaches those build scripts on every path is Cargo's own `[env]`
  table, which is already how `CMAKE_POLICY_VERSION_MINIMUM` reaches this exact
  build. Add to the existing `[env]` table in the repository's
  `.cargo/config.toml`:

  ```toml
  # Keep equal to bundle.iOS.minimumSystemVersion in
  # desktop/src-tauri/tauri.ios.conf.json. Read only when compiling for an Apple
  # mobile target, so macOS, Linux, Windows and server builds are unaffected.
  IPHONEOS_DEPLOYMENT_TARGET = { value = "14.0", force = true }
  ```

  `audiopus_sys` declares no `cargo:rerun-if-env-changed` for that variable, so
  the already-built objects must be invalidated once with
  `cargo clean -p audiopus_sys` inside `desktop/src-tauri`.
  `tests/test_build.py` asserts the value agrees with the shipped iOS minimum
  once the key is present.

  **Done.** The key is in `.cargo/config.toml`, and the debug simulator bundle
  now links with no deployment-target warning from any Opus object.

- **`AudioSessionGetProperty`.** `coreaudio-rs` 0.13 leaves an undefined
  reference to the AudioSession C API, which Apple deprecated in iOS 7.

  **Answered for the simulator SDK.** The reference is still undefined in the
  built application and binds to AudioToolbox, and the link succeeds against
  the iPhoneSimulator 26.5 SDK. The iPhoneOS SDK has not been linked, and
  whether the deprecated function does anything useful if `coreaudio-rs` ever
  calls it at run time is a separate question this does not answer.

#### Webview session bootstrap

**Resolved.** `WebviewWindow::set_cookie`, which used to install the private
owner session before the first navigation, aborts the process on iOS: `wry`
0.55's WKWebView cookie helper waits for its reply by pumping the main run loop
with `acceptInputForMode:beforeDate:`, that re-enters Tao 0.35's Core Foundation
control-flow observer, and the resulting panic cannot unwind through the
`extern "C"` callback (`SIGABRT`, `panic_cannot_unwind`, in
`control_flow_end_handler`). A simulator launch reached the runtime — the
database and sample library were created in the container and the private
loopback listener answered `/api/status` — and then aborted before the interface
appeared.

The fix hands the session to the webview over HTTP instead, so no host-side
cookie API is involved on any platform. `pr0_runtime` publishes

```
RuntimeHandle::bootstrap_url() -> Option<String>
  http://127.0.0.1:<ephemeral port>/__session-bootstrap/<72-character token>
```

and the host navigates its one webview there exactly once. The runtime answers
`303 See Other` to `/` with `Set-Cookie: pr0_session=…; HttpOnly; SameSite=Strict;
Path=/; Max-Age=86400` and `Cache-Control: no-store`.

Why this does not weaken the security contract:

- **It cannot exist outside an embedded runtime.** The route is created only in
  the `RuntimeMode::Embedded` branch of `runtime::start`, from the private owner
  session that only embedded mode creates. A standalone server has no session,
  no webview and no route.
- **It cannot be reached from the network.** It is merged into the router
  *after* `shared_router` — what local-network hosting serves — has been cloned,
  and *inside* the private listener's exact-authority `Host` guard. So it lives
  on one ephemeral `127.0.0.1` port, is refused for any other `Host` value
  (DNS-rebinding protection unchanged), and is absent from the LAN listener,
  which separately still rejects the private session cookie.
- **The secret is not the session.** The token is two v4 UUIDs of
  operating-system randomness, compared in constant time, and carries no
  authority of its own once spent.
- **It is single use and bounded.** The reply that carries the session destroys
  the only way to ask for it again. It also expires after 120 seconds, is
  destroyed after eight wrong tokens, and is revoked by `RuntimeHandle::shutdown`
  along with the session itself. A wrong or spent token is an ordinary `404`.
- **Nothing durable holds a credential.** The redirect means the document the
  webview ends up displaying is `/`, so the token is not the address of the
  running interface, is not a `Referer`, and is not reloaded by a refresh. The
  response is `no-store`. `Bootstrap` has a hand-written `Debug` that renders
  neither the token nor the session, and `Readiness`'s existing redaction is
  unchanged.
- **The excluded alternatives stay excluded.** No JavaScript-readable token, no
  credential that persists in a query string, no URL credential, no default
  password, no login bypass. The cookie installed is byte-for-byte the one
  `POST /api/login` issues.

The host's readiness check was extended rather than relaxed: it still requires
an ephemeral `http://127.0.0.1:<port>` listener and a full-length private
session, and additionally refuses a handoff that is not a 72-character
single-use path on that exact origin, or that carries a query string. The
session is checked, never carried — `desktop/src-tauri` no longer holds it at
all.

Desktop is unaffected in behavior and gains the same property: its multi-window
cookie copying in `windows.rs` is a separate per-origin feature and is unchanged.

Gate: partly performed. A debug simulator bundle launches on an iPad simulator
  and reaches its signed-in interface, and a project was created through the
  running application's own API. Still open: reopening a project, importing
  portable fixtures, surviving process termination, and touch/rotation/keyboard
  checks. This gate does not claim physical audio.

### Phase 5: enable iPad audio

The architecture half of this phase is implemented. The verification half is
open apart from a simulator run: no statement about iPad audio behavior —
including "it plays" — is supported by this repository, because nothing has been
heard on any device and a simulator is not audio hardware.

- ~~Add the iOS audio-session bridge and CPAL lifecycle adapter.~~
  `RuntimeHandle::suspend_audio`/`resume_audio` (with `_blocking` forms for an
  operating-system callback thread, and `audio_suspended`) close and reopen the
  native streams on the orchestration worker, between DSP blocks, through the
  same ordered command queue as enablement and shutdown. They are idempotent
  and add nothing to `Engine::render` or a device callback. Suspension stops
  rendering rather than free-running the software schedule, so the engine
  sample clock, transport, prepared graph, loop buffers and open recordings are
  where resume finds them. `desktop/src-tauri/src/audio_session.rs` installs
  category, mode, preferred sample rate and preferred I/O duration before the
  runtime can open a device — `playAndRecord` only when the saved settings
  enable a native input, so the microphone prompt follows a feature in use —
  and maps interruption begin/end, route loss, media-service resets and
  foreground/background onto those calls, completing each transition inside the
  handler within bounded deadlines. Route loss reopens on the replacement route
  rather than pausing. `pr0_runtime::audio_preferences` is the host-side read
  of the engine format that needs no engine and opens no device.
- ~~Whether reconfiguring the session when a user enables an input mid-session
  is worth the extra state.~~ It is required, not optional, and the smallest
  correct place for it is a capability rather than more host-side state. The
  category follows an *actually enabled* input, so reading the settings once at
  launch is wrong the moment a performer enables one in the running
  application: the engine would open that input against a `Playback` session
  and it would simply not work until the next background/foreground cycle.
  `RuntimeConfig::audio_policy` is an optional `AudioPolicy` the orchestration
  worker calls immediately before it opens a native stream, with the exact
  settings it is about to open with — the only point that knows both. iOS
  supplies it; every other host supplies `None`. It is not reached from a render
  or device callback, and it is reported rather than fatal, because the device
  layer's own error about the open that follows is the more useful one.
  `prepare` at launch and the bridge's reapply on resume are both kept: they
  activate a session when nothing is being opened, which is what an interruption
  end and a media-services reset need.
- The suspend/resume state machine keeps a standing *intent* (`Wanted`) rather
  than a snapshot of what was open when the system interrupted. Suspension does
  not touch it; enabling the engine or toggling hardware output while the
  hardware is closed writes it; resume reconciles by opening whichever wanted
  direction is closed, independently, so a capture device the system will not
  hand back does not keep the loudspeaker closed. A failed open leaves the
  intent set, so a repeat resume or the next foreground retries — which is also
  how a stream that failed under a vanished route reopens on its replacement.
  Deactivating a show withdraws the capture intent with the capture it closes,
  and shutdown withdraws both, so nothing reopens a microphone for a runtime
  with no project loaded or a runtime on its way out.
#### The input blocker, resolved into a safe refusal (2026-09-15)

The previous entry recorded that enabling a native input aborted the process on
iOS. Both halves it named are now implemented, and investigating them turned the
diagnosis into something narrower and worse than "enumeration is unsafe".

**What was implemented.**

- *Logical routes.* `crates/server/src/native_audio.rs` is the platform device
  boundary. On iOS the runtime publishes one output route
  (`ios:default-output`) and one input route (`ios:default-input`) with stable
  identities, resolves them with `default_output_device`/`default_input_device`
  — zero-sized values on that backend, no audio unit and no RPC — and
  synthesizes the stream configuration from the host's `AVAudioSession` reading
  rather than from `supported_*_configs`. The enumerating implementation is
  behind `cfg(not(target_os = "ios"))`, so no later edit can reintroduce the
  aborting call by relaxing a runtime condition. Desktop and server enumeration
  is untouched.
- *Capture authorization.* `RuntimeConfig::audio_routes` is a typed
  `AudioRoutes` capability carrying `CaptureSupport`, `CaptureAuthorization`
  (granted/denied/restricted/undetermined/not-required), the current route
  description, and a non-blocking permission request. The device layer checks it
  once, before any capture device is touched, on the orchestration worker and
  never in a callback. Denied, restricted and undetermined each produce their own
  sentence; the permission is requested when a performer enables a capture route,
  and the request never blocks the worker on a person. Discovery never enables a
  capture route by itself, so a first launch cannot select the recording category
  on its own.

**What the investigation found.** Not enumerating is necessary but not
sufficient. `AudioUnitInitialize` is also what *opening* a stream calls, through
`coreaudio::AudioUnit::new(IOType::RemoteIO)`, and the deadlock belongs to the
session category rather than to the direction. Measured on an iPad Pro 13-inch
(M5) simulator running iPadOS 26.5, with microphone access granted through
`simctl privacy` and an input route present: with `playAndRecord` installed,
initializing RemoteIO logs

```
Initialize: RPC timeout. Apparently deadlocked. Aborting now.
```

and calls `abort()` — and the crash report puts it in `open_outputs`, in
`build_output_stream_raw`, before any capture unit existed. Under `Playback` the
same output opens normally, repeatedly, across background/foreground cycles.
The host Mac had a default input device throughout, so this is not "the
simulator has no microphone".

**The resulting contract.** `CaptureSupport` is a host statement about its own
audio backend, checked *before* any permission question. The iOS host reports
`Unimplemented`, so:

- native input is refused with that sentence, in `/api/devices` beside the route
  and in the engine's own error;
- no microphone prompt is produced, for a feature that could not work whatever
  the user answered — and `describe_route` therefore does not read
  `AVAudioSession`'s input properties either, because on iPadOS 26.5 asking
  about input availability is itself enough to put the prompt on screen at
  launch; and
- the `playAndRecord` category is never selected, which is what keeps *playback*
  safe, not only capture honest.

An `abort()` cannot be caught, retried or reported. A performance application
that can die mid-show is worse than one that cannot record.

**What custom CoreAudio work remains**, in the order it would have to be done:

1. Establish on physical hardware whether `AudioUnitInitialize` under
   `playAndRecord` is a simulator defect or an iOS one. The simulator cannot
   answer this and no claim here should be read as an answer. If hardware is
   fine, the remaining work is a device-gate and a soak, not a new backend.
2. If hardware reproduces it, replace the capture path rather than the runtime
   around it. In increasing order of cost: open RemoteIO with input enabled
   *before* activating the session and on a dedicated thread with its own
   bounded initialization attempt; or use `AVAudioEngine`'s input node, which is
   Apple's supported high-level capture API and does not expose
   `AudioUnitInitialize` to the caller; or write a CoreAudio capture backend
   behind `cpal`'s `DeviceTrait` for this target alone. None of these changes
   anything above `native_audio.rs` — the route model, the authorization gate,
   the settings contract and the suspend/resume state machine are all already in
   place and would be reused unchanged.
3. Whichever path is taken, the switch is `const CAPTURE` in
   `desktop/src-tauri/src/audio_session.rs` becoming `CaptureSupport::Supported`.
   Everything downstream of it — the permission gate, the refusal messages, the
   opt-in discovery rule, the category decision — is implemented and covered by
   tests today, and a desktop test asserts the shipped value so turning it on
   cannot be done silently.
4. Before it is turned on: physical allow/deny/revoke behavior, interruption and
   route-change behavior with an input open, and a sustained soak. Nothing in
   this repository establishes any of that.
- Still open: verify built-in output on hardware, input permission behavior,
  interruptions, route changes, sample-rate negotiation, graph replacement and
  shutdown. Simulator runs have covered startup configuration, engine
  enablement opening CoreAudio output through the logical route,
  background/foreground cycles that each closed and reopened that output, the
  hardware toggle honoured in both directions across a suspension, a project
  reopened after an application restart, and the capture refusal with no
  microphone prompt and no abort. They say nothing about physical audio: nothing
  was heard, and the simulator cannot raise an interruption, a route change or a
  media-services reset at all.
- Still open: whether the recording category should use
  `AVAudioSessionModeMeasurement` to bypass system input processing.
- Still open: an on-device diagnostic export containing app/runtime versions,
  route, sample rate, callback sizes, underruns and worker timing without
  recording user audio. `audio_suspended` in `/api/devices` and telemetry is
  the only lifecycle diagnostic so far.
- Still open: foreground keep-awake behavior while hosting or performing.

Gate: unperformed. Physical iPad tests pass for built-in output/input and a
sustained soak; all latency and reliability results are recorded as
device-specific measurements.

### Phase 6: LAN hosting and ensemble tests

- Request local-network permission at the first hosting/discovery action.
- Validate loopback and LAN listeners, mDNS advertisement, HTTPS certificates,
  authentication, WebSockets, WebRTC monitor feeds, microphone uplinks and OSC
  policy on real Wi-Fi.
- Suspend, lock, rotate, disconnect Wi-Fi, change access points, attach power and
  reconnect clients while observing explicit runtime state.
- Run 1-, 8- and 32-client protocol/audio fixtures against the iPad, then repeat
  with real client devices.

Gate: foreground hosting has a documented supported client count and soak result.
Background operation remains unsupported unless separately validated and approved.

### Phase 7: TestFlight hardening

- Add signed release configuration, monotonically increasing build numbers and a
  reproducible archive command.
- Run dependency/license, privacy-manifest, export-compliance and symbol audits.
- Add crash reporting only if its data handling is explicitly accepted and
  documented; otherwise use user-exported local diagnostics.
- Prepare review/demo data that exercises the app without requiring access to a
  private production server.

Gate: an archived build validates locally, uploads cleanly, and completes internal
TestFlight testing before external beta review.

### Phase 8: App Store submission

- Complete product metadata, screenshots, age rating, support URL, privacy policy,
  App Privacy answers, export-compliance answers and review notes.
- Provide App Review a fully functional demo path and explain local-network,
  microphone, audio background behavior (if enabled), JavaScript control scripts,
  sample import and LAN hosting.
- Resolve account deletion before submission. The current server permits invited
  account creation but forbids deleting the signed-in account, which conflicts
  with Apple's requirement to offer in-app deletion when account creation exists.
- Submit a release-quality build, not a development-alpha or beta-labelled build.

Gate: App Review approval and a monitored manual or phased release.

## TestFlight workflow

1. Enroll in the Apple Developer Program and create an App ID whose bundle ID
   exactly matches the iOS Tauri/Xcode target.
2. Create the App Store Connect app record before the first upload.
3. Configure the development team and automatic signing in the generated Xcode
   project. Use a registered iPad for direct development builds.
4. Install the Rust iOS device and simulator targets, initialize Tauri iOS, and
   use `tauri ios dev` for simulator/device iteration.
5. Open the generated Xcode project with `tauri ios build --open`, select a generic
   iOS device, Archive, validate, and distribute to App Store Connect. The Tauri
   CLI can alternatively generate an App Store Connect IPA.
6. After processing, resolve export-compliance questions and distribute first to
   an internal TestFlight group. Internal testers must be App Store Connect users.
7. Add `What to Test`, beta description, contact and review information, then
   submit the build for TestFlight App Review before external testing.
8. Treat each TestFlight build as expiring after 90 days and increment the build
   number for every upload.

The current development machine has Xcode 26.6 and the Rust iOS targets
installed; the command-line keychain check still reports no valid code-signing
identity. That is a setup prerequisite, not a repository defect, and it is why
the simulator archive is built unsigned.

## Physical test matrix

- At least one currently supported iPadOS version and one older supported version.
- Apple Silicon simulator for UI/persistence only.
- Built-in speaker and microphone, wired headphones, and at least one USB-C audio
  interface; Bluetooth is tested for graceful behavior, not low latency.
- 44.1 and 48 kHz first, then 88.2 and 96 kHz where the route supports them.
- Permission allow/deny/revoke, first launch, upgrade, low storage, thermal load,
  low battery, power connection, route changes and interruptions.
- Foreground 15-minute smoke, one-hour soak and performance-length soak.
- Local engine only, one remote client, eight clients and the existing 32-client
  protocol fixture, with WebRTC uplink/downlink where applicable.
- Project/sample import-export, loop persistence, recording finalization, app
  termination during writes and recovery after an interrupted launch.

Simulator and software-clock results must not be described as physical latency or
performance verification.

## App Store readiness checklist

- Stable bundle ID, semantic version and monotonically increasing build number.
- Apple distribution signing and App Store provisioning.
- Microphone and local-network purpose strings that match actual feature timing.
- Bonjour service declarations for advertised/browsed services.
- Background audio entitlement only if the shipped behavior genuinely requires
  and continuously uses it.
- Privacy manifest and accurate App Privacy declarations for account/profile,
  audio, diagnostics and network data.
- Privacy policy available both in App Store Connect and inside the app.
- In-app signed-in account deletion or removal of in-app account creation.
- Export-compliance determination for Rustls/TLS and other cryptography.
- Complete licenses and notices for Rust crates, codecs, samples and any retained
  FFmpeg components.
- Review credentials or a complete local demo mode, plus precise review notes.
- iPad screenshots showing the real editor/performance experience, support URL,
  age rating, category, description and contact information.
- On-device crash, interruption, thermal, storage and network validation.

## Open product decisions

1. Is the App Store iPad app primarily a self-contained workstation, a client for
   another pr0former host, or both? The plan supports both, but review messaging
   and first-launch UX should choose a clear primary purpose.
2. Is foreground-only LAN hosting acceptable for version 1? This is the safest
   initial contract.
3. Which imported formats are contractual? A focused portable list makes removing
   mobile FFmpeg practical.
4. Should the iPad owner use the current automatic local `admin` identity, or an
   ordinary account flow? This affects account deletion and review behavior.
5. What minimum iPadOS version and hardware class are supported? Decide before
   performance claims and App Store metadata.

## Current external references

- [Tauri App Store distribution](https://v2.tauri.app/distribute/app-store/)
- [Tauri mobile prerequisites](https://v2.tauri.app/start/prerequisites/)
- [Apple: upload builds](https://developer.apple.com/help/app-store-connect/manage-builds/upload-builds)
- [Apple: internal TestFlight testers](https://developer.apple.com/help/app-store-connect/test-a-beta-version/add-internal-testers)
- [Apple App Review Guidelines](https://developer.apple.com/app-store/review/guidelines/)
- [Apple privacy manifests](https://developer.apple.com/documentation/bundleresources/privacy-manifest-files)
- [Apple local-network usage description](https://developer.apple.com/documentation/bundleresources/information-property-list/nslocalnetworkusagedescription)
- [Apple background execution modes](https://developer.apple.com/documentation/xcode/configuring-background-execution-modes)
- [Symphonia format and codec support](https://github.com/pdeljanov/Symphonia)
