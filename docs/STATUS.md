# Current implementation status

pr0former is a development alpha. Software validation does not establish physical audio latency, deadline reliability, iPad compatibility, or readiness for a 32-player performance.

## Capabilities and evidence

| Area | Current behavior | Evidence / limitations |
| --- | --- | --- |
| Graph engine | Server-validated audio (1–8 channels), control, spectral and typed MIDI contracts; nested graphs; feedback loops through audio/control wires (one-sample feedback edges); prepared DSP storage and live compatible-state transfer. | Rust render allocation/deallocation guards and routing tests. Incompatible replacements have no crossfade. |
| Native audio | Bounded callback rings, driver-consumption pacing, device/channel routing and meters. | Synthetic callback-size tests. Multiple independent hardware clocks are not synchronized; physical latency and endurance remain manual. |
| Persistence during playback | Ordered bounded queue sends archive data, loop chunks and retired engines to a persistence worker. Queue pressure delays command admission while rendering continues. Loop collection copies at most 4,096 samples per block; assembly, writer barriers and retired-engine destruction run off the audio-producing worker. Disable, clear and shutdown acknowledgments wait for preceding writes. | `persistence::tests` holds storage blocked while rendering advances; bounded snapshot reconstruction test; looper/recording browser tests. Real disk stalls and hardware refill deadlines remain unverified. |
| Browser monitoring | WebRTC/Opus master and selected cue feeds; only subscribed dedicated feeds are collected. Feed leases expire after eight seconds without refresh. | Count-in tests receive actual master/cue audio. Conversion and telemetry still share the orchestration worker. |
| Score and transport | Shared score, independent part launches, repeats/navigation, tempo map, notation/piano roll, staff routes and MIDI automation. Meter-aware metronome follows written position and bar origins through changes/repeats. | Sequencer/DSP tests and Chromium integration. External MIDI delivery is best effort. |
| Conducted performance | One designated non-performing conductor; ordered animated set/tile editor; locked touch stage; next-pulse single and armed-group cues; one-shot or forced-repeat playback; timed per-performer successor queues; continuous synthesized performer notation/rest lane; part-local polymeter; targeted cue clicks; conductor-authoritative dynamics; MIDI Learn; and authenticated performer Web MIDI ingress. | Scheduler/core/frontend and focused Chromium coverage. Physical touch/MIDI/iPad, browser MIDI reconnect behavior, multi-client latency and performance-scale acceptance remain manual/unverified. |
| Editing and saves | Project-owned score draft survives tab unmounts; flush waits for current and newer edits. Export, Save revision, project switching, performance entry and sign-out use the barrier. Failures retain the newest draft for explicit reapplication/discard. | Delayed/rejected save unit and browser regressions. Drafts are in browser memory, not offline durable storage. |
| Dynamics and phrasing | Staff overrides preserve part defaults for other staves. Ramp conversion preserves authored holds and curve shapes. New hairpins own an identifiable dynamics event and retain any authored starting mark for restoration; gesture previews are transient, with one commit/undo on release. Moving/removing owned playback updates its wedge. | Value-level ramp and hairpin tests plus browser editing. Hairpins crossing existing ramps/interior points are rejected instead of overwriting them; older wedges without owned events remain independent notation. |
| UI | Custom graph node theme, paper score, visible save/conflict status, keyboard-focusable ramp points, coarse-pointer touch targets and reduced-motion support. The footer offers transient Play + Repeat and swaps Performance mode for End performance in place; the conducted cue deck scrolls vertically in short viewports. Save coordination, MIDI device lifetime and curve gesture transforms are separate modules. | Frontend tests/build and browser tests. Physical touch/Safari validation remains manual. |
| Ensemble | Member browser with profile summaries, direct existing-user addition, invitation links and self-service square avatars. Owners can remove non-owners; assigned parts become unassigned in the same server transaction. | Rust crop/assignment tests and isolated HTTPS/API workflow. Role editing and the broader user profile editor remain future work. |
| MIDI/OSC | System settings and receiving/sending paths exist. Every graph MIDI source (Part MIDI, MIDI input, OSC-to-MIDI, Piano) receives raw channel messages and derives its five scalar outlets from the same frame that its typed `midi` output forwards, so the two paths cannot disagree. MIDI input keeps the channel byte; OSC-to-MIDI and Piano use channel 1. Piano decodes typed input (keys light, outlets move) and republishes scalar-driven notes. MIDI inputs accept several cables, concatenated in connection order per sample. A MIDI output forwards a typed cable raw and decodes scalar cables, never both. | Engine tests cover each source→sink pair over typed cables, fan-in order, reset propagation, scalar override and the single-send contract; external delivery remains best effort and physical MIDI hardware is unverified. |
| Deployment/assets | Local fonts/assets, native HTTP/HTTPS and desktop packaging, Bash 3.2-compatible launcher. | Existing launcher/desktop validation is historical evidence; startup services are never installed/enabled by automated tests. |

## Remaining performance work

- Dedicated real-time scheduling, command preparation and monitor conversion isolation; incompatible-graph crossfades.
- Adaptive per-device clock drift correction, physical loopback/latency measurements, real MIDI hardware and iPad/Safari runs.
- Worst-case device refill measurements under edit/storage/visualizer stress, a 60-minute soak, and the physical 32-player acceptance run.
- Worker telemetry now includes maximum observed work and block-start gap in microseconds. These software measurements include scheduling/configuration effects and are not a hardware latency or deadline guarantee.

## Validation

Local input controls, profiles and branding (2026-09-12): Local audio input defaults
unmuted and automatically starts its authorized microphone uplink when the engine
is enabled. The orange dome microphone button on the node toggles graph mute;
a Mute control input overrides it. Shared monitor/source labels identify the user
and machine, with native hostnames or an editable browser device name. Metadata is
session-only. The existing single uplink per project/user limit remains; duplicate
senders to one node are refused. Physical microphone permission, iPad capture and
Wi-Fi timing remain manual/unverified.
The updated arm64 app was signed, notarized (Accepted submission
`0a07ed2b-df0e-4204-80bf-474b05c06d94`), stapled and accepted by Gatekeeper,
then exported to the ignored project-root `pr0former.app`. Both release-server
desktop process tests passed. The native bundle icon matches the supplied artwork.

The account button now opens a profile menu, shows uploaded avatars, and keeps
Sign out disabled for the private bundled admin. Self-editing includes profile
fields, avatar upload and password changes; ordinary password changes require the
current password and revoke other sessions. Avatar upload moved from Ensemble.
The supplied artwork is used for desktop, web header, favicon, Apple touch and
manifest icons. OSC history's explanatory paragraph was removed.

Console Out captures control changes/text/pulses in a bounded preallocated queue,
then logs outside rendering. MIDI/OSC bridge and OSC input/output nodes show
orange/dark message histories on the node and in options. Telemetry is sampled;
OSC output history means prepared values, not confirmed network delivery.

Validation so far: workspace Rust tests passed 243 cases (one opt-in ignored),
including all-channel local input mute and render allocation guards; 8 desktop
Rust tests and 97 frontend tests passed. The production web build passed. Real
Chromium WebRTC tests confirmed automatic capture, mute/unmute without reconnecting,
and shared identities in a second user's Monitor with unauthorized reads rejected.
Profile and desktop session browser regressions also passed. The assigned performer can mute their own input but cannot edit other node parameters. Real loopback UDP verified OSC/MIDI histories and Console Out text logging without revisions; the simulated-hardware Monitor layout regression passed. Generated test audio
was used; physical devices were not exercised.


OSC node DNS destinations (2026-09-12): OSC Output and MIDI to OSC accept DNS
hostnames with ports. Resolution runs on the external output worker, selects an
IPv4 result and uses a bounded cache; errors appear in node I/O telemetry.
Core/server tests passed 110 cases (one opt-in ignored), including syntax checks
and real UDP delivery through `localhost` for values, notes and note releases.
All 97 frontend tests and the production build passed. The Chromium OSC
round-trip/sample-playback regression also passed using a hostname destination.
Remote DNS, LAN mDNS availability and physical network latency remain unverified.

MIDI history sizing and conversion ranges (2026-09-12): options retain only the
last 20 MIDI observations, and the node retains five channel/detail/value rows
in an orange-bordered dark panel with a larger activity light. Chromium verified
old-entry eviction and both bounds. Scale now has input endpoints (default 0–1)
and output endpoints (default -90/+6). Amplitude to dB exposes min/max sliders,
numeric fields and control ports, defaulting to -90/+6 dB. The logarithmic
conversion retains amplitude 1 = 0 dB. Rust core/DSP tests passed 147 cases,
including equal/reversed ranges and connected dB bounds; 97 frontend tests and
the production build passed. Two focused Chromium cases passed for MIDI history
and live Scale/dB editing. Hardware timing and physical MIDI remain unverified.

Help and documentation center (2026-09-12): the authenticated gear menu now
opens a two-column documentation modal with getting-started, first-project,
performance-mode, interface, and server-catalog-backed node reference sections.
Every node entry exposes its description, typed ports, parameters and aliases,
with an on-demand illustrative compatible graph; the artwork is explicitly a
mock and not a running-engine claim. The modal opens in a browser tab on request,
and the desktop Help menu creates a dedicated webview window at the same server
origin. Frontend tests passed 97 cases, the production build passed with the
existing large-chunk warning, all 7 desktop Rust tests passed, and the focused
Chromium workspace case passed through the modal, navigation, node search,
example graph and browser pop-out. The native Help menu/window was compiled and
URL-tested but not manually exercised in a packaged Tauri app.

MIDI debugging and knob unassignment (2026-09-12): MIDI input options now decode
CC, pitch bend, program change, channel/poly pressure and notes, with raw bytes,
received/drop counts and up to 20 locally observed messages. History samples the
existing 20 Hz telemetry after channel filtering; it is not a complete event log.
Nodes show a larger activity light and an orange-glowing table of the last five
observed channel/detail/value rows. Both histories are bounded in memory only. Double-clicking a
knob clears its channel binding; unassigned knobs are dimmed and reject value
changes in both the server API and DSP. Clicking starts MIDI Learn; the modal's
channel selector also restores a binding. Existing bindings remain unchanged.
The workspace Rust suite passed 234 tests (one opt-in ignored); frontend tests
passed 97 cases and the production build passed with the existing chunk warning.
Three focused Chromium cases passed, including debug decoding, activity flashing,
history clearing without revisions, unassignment, blocked values and both manual
and MIDI Learn reassignment. MIDI devices were simulated; physical Twister and
Tauri/WebKit hardware behavior remain manual/unverified.

Local MIDI / MIDI Learn correction (2026-09-12): added a Local MIDI Input node
with session-local Web MIDI selection, explicit connect/disconnect and authenticated
server ingress. Corrected native MIDI input's Notes-mode filter, which previously
dropped ordinary CC messages before they could reach Knobs. Typed MIDI now forwards
all channel-message types while scalar decoding retains its selected mode. New
server MIDI inputs default to all channels; existing explicit filters are preserved.
Both MIDI input kinds expose transient message count and last-message telemetry. Escape, clicking
away, or control cleanup cancels MIDI Learn without changing its assignment.
The workspace Rust suite passed 231 tests (one opt-in ignored), including native
packet/routing and learn-cancel tests; render allocation guards include local MIDI.
Frontend tests passed 92 cases and the production build passed with the existing
large-chunk warning. Four focused Chromium cases passed, including a simulated
Twister → real WebSocket/server → Knobs flow, assignment persistence, cancellation
without revisions, exclusive input ownership, program messages and unplug note
release. The physical MIDI Fighter Twister was not exercised. Physical Web MIDI,
Tauri/WebKit MIDI availability, and network-overload timing remain unverified.


Local audio input / desktop connections (2026-09-12): renamed the catalog title and
capture/device UI; retained the persisted `browser_input` kind. The native Server
menu offers Connect to Server and Bundled Engine. A restricted packaged dialog
validates HTTP(S) base addresses, then opens a separate private remote session.
Local automatic login is not forwarded. Core tests (18), desktop URL/origin tests
(2), frontend tests (90), the frontend build and desktop debug build pass. Four
focused Chromium cases pass, including the generated-audio WebRTC input regression.
An isolated macOS native smoke check exercised the menu, shortcut, invalid-address
feedback, connection to a second server, login isolation even at the bundled
server's exact origin, and return to the bundled admin window. No production server
was restarted. Physical audio/MIDI capture, remote HTTPS certificate/device prompts
and Linux native webviews remain manual/unverified.


Desktop server identity and account editing (2026-09-12): native performance,
login and documentation titles show `pr0former (bundled)` or the actual remote
scheme/host/port after the app name, including default ports. The private bundled
admin session is identified by its exact token, with sign-out disabled in the UI
and refused by the server. Ordinary logout preserves the private session; quitting
the app still revokes it. User creation, invitations and remote sign-out remain
available. User administration now unwraps Vue reactive rows before cloning,
provides explicit Edit buttons, highlights the selection and focuses its form.

Validation: 8 desktop Rust tests and 97 frontend unit tests passed, along with the
production frontend build, 2 desktop process tests, and 2 focused Chromium cases.
These cover local logout refusal, remote account creation and logout, disabled sign-out
in multiple project views, and selecting an existing user row to edit and
persist profile details. Native title formatting covers bundled, hostnames, IPv4,
IPv6, explicit ports and default HTTPS ports. Physical devices were not used.
The updated arm64 app was signed, notarized (Accepted submission
`c616df9e-21fe-49bf-8480-8782ac0ee69c`), stapled, verified by Gatekeeper, and
exported to the ignored project-root `pr0former.app`.

Desktop LAN hosting and windows (2026-09-12): the Mac Server menu now discovers
compatible LAN servers via embedded mDNS/DNS-SD and offers opt-in HTTPS hosting
of the bundled engine. Hosting creates a persistent profile-local CA and supplies
an iPad certificate profile/setup address. Normal invited accounts and project
roles apply; the private desktop admin session is rejected on the LAN listener.
Stopping hosting leaves the private engine available. A future iPad Tauri app is
client-only; no iPad binary or server sidecar for iOS is implemented.

The Mac connection dialog surfaces failed navigation and supports explicit
per-origin leaf-certificate pins without changing Keychain trust. Certificate
changes require another approval. Window → New Window for This Performance and
Tile Windows support multiple views, with count/view/size/position restored per
performance (up to eight); layouts are outside project revisions.

Verification: 7 desktop Rust tests, 3 server discovery/certificate tests, 97 frontend
unit tests, production frontend build, the desktop Chromium integration case,
2 real desktop process tests, and 9 build-script tests passed. The process tests
validate generated TLS chains with normal certificate checking, public CA/profile
consistency, invited-client project access, Secure cookies, desktop-session
rejection on LAN, setup HTTP isolation, CA reuse, stop/restart, and recording
finalization on exit. An isolated native macOS app verified LAN discovery and
HTTP remote login isolation, duplicate/tiled windows and restoration after relaunch,
the certificate prompt, a matching test pin loading HTTPS and secure WebSockets,
and a changed test certificate requiring renewed approval. The host dialog started
an isolated HTTPS listener successfully. No production server was restarted.

`./build.sh --notarize` built, signed, notarized and stapled the updated arm64 app;
Apple submission `3866ebd1-7b66-41d4-a334-ebe33019d473` was Accepted. Signature,
stapler and Gatekeeper checks passed, and the ignored project-root
`pr0former.app` was refreshed. The previously built DMG is not this new build.
Physical iPad profile installation, Wi-Fi performance, microphone/WebRTC media,
physical audio/MIDI, and Linux native trust/windows remain manual/unverified.
The native HTTPS/WebSocket check used a disposable fixture and a seeded test pin;
it did not modify system certificate trust or trust the production server.


Piano drag gestures (2026-09-12): vertical pointer movement emits transient MIDI
pitch bend; horizontal movement across white/black keys releases the previous note
before attacking the next. Pointer release/cancel, focus loss, and component cleanup
release notes and recenter bend. Use typed MIDI cables for bend; internal instruments
use ±2 semitones, while external receivers choose their range. The five scalar note
outlets retain integer note identity. Internal MIDI channels share bend, and granular
updates affect newly launched grains. Unsent bend movements coalesce between note
boundaries; gestures do not create project revisions.
`cargo test --workspace --quiet` passed 229 tests (one opt-in ignored), including
synth/FM phase and MIDI forwarding regression. The allocation guard also passes with
repeated bends through synth/sampler/granular typed MIDI inputs. Frontend tests passed
90 cases and the build passed with the existing large-chunk warning. Four focused
Chromium cases passed across the piano, piano-drag and FM-synth files, covering real
server telemetry, glissando ordering, bend limits/reset, focus loss, API validation,
existing multi-touch notes and independent note releases. Hardware MIDI/audio and
physical touch devices remain manual/unverified.


macOS signing setup (2026-09-12): `scripts/build-macos-signed.sh --sign-only`
built the current Apple Silicon desktop app with the installed Developer ID
Application identity and hardened runtime. Deep/strict codesign verification
passed for the sealed bundle and helpers. The native app opened to the signed-in
project browser. The production frontend/release builds and
`python3 tests/test_desktop.py` passed; wrapper Bash syntax/help and invalid-option
checks passed. After credentials were saved, `build.sh --notarize-only` passed
Apple notarization, stapler validation, and Gatekeeper assessment (`Notarized
Developer ID`). The full `build.sh --notarize --bundles dmg` workflow also
passed: both the app and arm64 DMG were accepted by Apple, stapled, and accepted
by Gatekeeper. `build.sh` now asks about signing/notarization in an interactive
macOS terminal; explicit flags support SSH/CI. Nine isolated build/PTY/key-selection
tests pass without changing real Keychains or services. SSH login to this Mac was
verified; its remote Keychain was locked, so full SSH signing/notarization remains
unverified pending the user's secure `--unlock-keychain` password entry. The setup
helper's real signing-key access update has not been run. Physical audio/MIDI and
permission prompts were not tested in this check.

Current additions (2026-09-12): Knobs/Sliders support variable control counts,
CC channel/controller assignments, pass-through, GUI gestures and MIDI Learn;
Sliders add range, step, decimals and orientation. A direct connection action in
the Local audio input modal starts capture. A generated mono audio stream traversed
the real Chromium WebRTC/Opus uplink and produced graph audio in the focused
browser regression; physical microphone/device selection remains manual.
Score click entry inserts adjacent to existing notes, shifts later chord groups
through available gaps, and rejects bar overflow atomically. Frontend unit tests
and focused score/save/browser regressions cover these changes. Live control
values are transient; MIDI Learn persists the assignment, not telemetry.
The production build passes with the existing large-chunk warning. The focused
14-case Chromium run set covers controllers/uplink, score insertion, entry tools,
score workspace and save coordination. DSP allocation guards include both new
controller nodes. Hardware MIDI, physical microphone capture and touch devices
were not exercised.

Stabilization validation (2026-09-10):

- Graph workflow pass (2026-09-11): Auto-space (right-click, L) lays out a
  selection in crossing-reduced signal-flow columns and A selects all nodes;
  feedback loops are scheduled with one-sample feedback edges (audio/control
  only, drawn dashed); new `drum_pads` node with General MIDI notes and per-pad
  trigger outlets; the FM Drum Machine preset decodes those notes per voice,
  its trigger inlets accept a velocity value, and its voices are one-shots via
  the new synth/FM synth `decay` parameter; `midi_to_control` gained
  `note_on`/`note_off` outlets; new `overdrive` effect and `control_delay`
  node (drivable millisecond delay for numeric controls); a Drum Sampler
  library subgraph with six one-shot sample voices preloaded with a bundled
  CC0 kit that is also seeded as global library samples for every account
  (docs/SAMPLE_CREDITS.md); sample tags shown as clickable filter chips in
  the sample list, browser and organizer with a tag-cloud editor in the
  metadata dialog; an on-node ADSR envelope
  graph with a draggable editor in the modal (stepped time axis, release
  handle at the start of the release ramp) and a `note_off` ADSR input so
  trigger/note-off pulse sources attack and release; the Meter node draws
  per-channel dBFS meters on the canvas and exposes one Level control output
  per channel; explanations collapse to info icons with the `i` key; the footer monitor
  button is a VU meter of the received browser stream; the output node modal
  shows its interface above the channel selector. `cargo test --workspace`
  passed **215 tests** (one opt-in ignored); `npm --prefix web test` passed
  **67** (five new layout tests); the production build passed. Hardware MIDI
  and physical monitoring remain manual.

- Review sweep (2026-09-11): unified the graph MIDI raw-message path (MIDI
  input and OSC-to-MIDI typed outlets now carry messages, Piano decodes typed
  input, MIDI output no longer double-sends, MIDI inputs accept fan-in),
  implemented the `osc_input`/`osc_output` nodes, aligned the graph clock to
  transport, made `pan` a balance, moved login/register hashing off the async
  executor with constant-time misses, restricted avatar reads to shared
  projects, capped WebSocket frames, added response security headers,
  serialized telemetry once per tick, cached performer MIDI authorization per
  socket, and removed per-sample string lookups from the orchestration worker.
  `cargo test --workspace --quiet` passed **209 tests** (one opt-in ignored),
  including 18 new regression tests; `npm --prefix web test` passed **62**; the
  production build passed. No physical MIDI or audio hardware was used.

- Piano typed-MIDI correction (2026-09-11): all **94 pr0-dsp unit tests**
  and both DSP allocation tests pass. The focused two-case FM-synth Chromium
  file passes with its pointer-driven Piano case routed over a typed MIDI cable.
  No physical MIDI or audio hardware was used.

- Conducted-performance software pass (2026-09-11): `cargo test --workspace
  --quiet` passed **190 tests** with one opt-in test ignored; `npm --prefix web
  test` passed **62 tests**; and the production frontend build passed with the
  existing Vite large-chunk warning. Added tests cover pulse/local-meter mapping,
  retained successor boundaries, repeat exit, monitor association/count-in
  targeting, browser MIDI validation/routing/panic, and synthetic performer-lane
  composition. The expanded Playwright conducted cases were authored but not run
  in this pass by explicit request. No physical-device claim is made.

- `cargo test --workspace --quiet`: **179 passed**, one opt-in throughput benchmark ignored. Includes render allocation/deallocation guards, bounded loop chunks, slow-storage command backpressure, writer ordering, retained hairpin metadata validation, and meter/repeat scheduling.
- `npm --prefix web test`: **58 passed**; `npm --prefix web run build`: passed. Vite still reports the existing large-chunk warning (score/VexFlow bundle); no tablet startup-performance claim is made.
- `npx playwright test` in `web`: the **68-case full Chromium/API suite passed**. Twelve focused score cases then passed after final keyboard/dynamics changes. The final five-case phrasing/save run passed, including the newly added keyboard-only ramp test (69 distinct standard cases now exist).
- [Score save/browser regressions](../web/e2e/score-save.spec.ts) delay/fail requests while editing, verify export/revision barriers and draft recovery across tabs, and navigate/nudge ramp points by keyboard. [Phrasing regression](../web/e2e/score-phrasing.spec.ts) verifies multi-frame drag, playback endpoint and single-step undo. [Metronome regression](../web/e2e/metronome.spec.ts) receives actual WebRTC audio energy with score instruments silent.
- The generated score screenshot was inspected. Original documentation artwork was preserved. All browser servers used isolated test data with native devices disabled; no startup services, production server restart or hardware test was performed.

See [VALIDATION_HISTORY.md](VALIDATION_HISTORY.md) for dated prior runs, [AUDIO_ENGINE_AUDIT.md](AUDIO_ENGINE_AUDIT.md) for the September 8 audit, and [ARCHITECTURE.md](ARCHITECTURE.md) / [SCORE_EDITOR.md](SCORE_EDITOR.md) for current contracts and usage.
