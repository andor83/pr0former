# Current implementation status

Score creation now opens an instrument-preset modal with orchestral, keyboard,
percussion and guitar choices, editable single/grand staff layout, clef,
transposition and MIDI channel. Presets do not create synths or assign sounds.
Percussion clefs and per-note normal/cross/circled-cross/diamond/triangle/ghost
heads plus roll slashes and a buzz mark persist through server validation.
Entry settings select defaults; Edit selected notes applies them to selections.
These are visual notation only, not automatic GM drum mapping or roll playback;
ordinary rhythmic flags still follow note duration.

Project browser export now downloads a `.pr0` ZIP containing `project.json` and
project-local sample WAVs under `samples/` (including unused samples, excluding
resampling caches). Imports allocate fresh asset IDs and rewrite sampler and
shortlist references. Overwrites retain previous audio files for old revisions.
The manifest records archive/schema versions, software version, Git commit,
dirty-build status and build timestamp. Newer software or unknown archive/schema
versions are rejected with compatibility errors; older supported-schema projects
pass the normal project validation, not a general migration system. Legacy JSON
remains supported, but missing audio now fails import with an actionable message.
Bundles have a 256 MiB expanded/upload limit and reject unexpected paths,
duplicate entries, malformed WAVs and missing referenced samples. Sample catalog
names/tags and live recorder-track files are not currently bundled; imported WAVs
are discovered by the sample catalog as `Sample <id>`.

Piano gestures now use the ordered authenticated project WebSocket, with held-note
cleanup and bounded admission instead of serialized HTTP requests. Footer tempo
edits are protected from live telemetry while typing. Graphical controls are
compact, optionally hide all chrome, and support change-only output and manual
override until the next incoming change/event. Integer/Float inputs have ±1
arrows. Graphical sliders share the custom Sliders fader, preview over WebSocket,
and persist once on release; library controls remain available during movement.
Sliders-node controls were moved upward to clear their footer. Hardware timing
and physical touch-device performance remain manual/unverified.

Control follow-up validation: 292 Rust tests passed across core, DSP and server
(one manual server benchmark ignored), including allocation guards and
change-only/override regressions. All 117 frontend tests, the production build,
and four focused Chromium cases passed. Browser checks cover piano bursts and
disconnect release, drag/bend/touch gestures without HTTP, footer tempo editing,
live slider previews without revision writes or library disabling, connected
manual override and incoming resumption, compact chrome and ±1 arrows. Both
slider orientations passed footer-clearance checks and their screenshots were
visually inspected. These are software-clock checks, not hardware latency claims.

Earlier score/import validation: 22 core and 104 server tests passed (one server
benchmark ignored), 117 frontend tests and the production build passed, and seven
focused Chromium cases passed. Browser coverage includes notation editing,
instrument presets, bar extension, JSON compatibility, `.pr0` download, exact WAV
round-trip, cancel/rename/overwrite, and recovery of a target with missing audio.
The percussion engraving screenshot was inspected. No hardware timing was tested.

Quick entry requests a new bar immediately after a successful note/rest fills
the final bar. It shares the right-arrow confirmation and session suppression
preference; completing an interior bar continues into the existing next bar.

The score context menu offers Delete bar / Delete bars for the clicked bar or
current bar selection. It uses the existing score-wide time splice and undo
history, closes the gap across all parts, and requires at least one remaining bar.

Drum pads have six numeric trigger inputs: either sign triggers from zero,
with velocity 100; zero releases/rearms. DSP coverage checks all six notes,
negative values, held values, sign changes and repeated triggers. Physical
MIDI timing remains unverified.

pr0former is a development alpha. Software validation does not establish physical audio latency, deadline reliability, iPad compatibility, or readiness for a 32-player performance.

## Linux standard-port setup (2026-09-13)

Linux source installs can grant the release server `CAP_NET_BIND_SERVICE` with
`./init.sh --allow-low-ports`, run as the normal project user. The script requests
`sudo` only for the `setcap` command, verifies the resulting file
capability, and continues to reject running the installer or server as root. Fresh
interactive Linux setup offers the grant after building. A later `init.sh` build
detects an existing grant before compilation and restores it if the rebuilt binary
lost the capability. Linux dependency installation includes the distribution's
libcap tools. Start warns with the exact remedy when its selected/default address
may use ports below 1024; unprivileged 8443/8080 operation remains available.

Validation: Bash syntax/help checks passed. All five automated launcher tests
passed (one opt-in real-server smoke check remained skipped), including a simulated
Linux `sudo`/`setcap` grant, verification, idempotent rerun, rebuild loss and
restoration, plus the unchanged startup-service branch. No service was installed
or enabled. The server suite passed 92 tests (one manual throughput benchmark
ignored), all 111 frontend tests passed, and the production frontend build passed
with the existing large-chunk warning. The capability flow was simulated on macOS;
an actual Linux filesystem capability and privileged-port bind remain manually
unverified.

## Conducted editor controls and MIDI bindings (2026-09-13)

The Conductor editor now exposes the same arm/play/repeat/stop and live-dynamic
controls as the performance view. Editors may rehearse cues during preparation;
locked performances retain conductor authority. Playing tiles fill green from
left to right, and names, performer labels, arming and dynamics use separate rows.
Blank space beside ARM remains part of the tile's click target; dragging in the
editor does not accidentally launch a part.

Bind MIDI highlights on-screen targets and captures the first supported message
from a selected local/server source or the first available device. Device,
channel and note/controller identity are saved as validated project settings.
Escape, End bind mode and outside clicks cancel pending capture and leave bind
mode. MIDI cues are suppressed throughout bind mode. Part/arm toggles, individual
sets, continuous set selection, next-set advance/wrap, global play/repeat/stop
and group dynamics are supported. Bindings can be changed during performance
without unlocking score/graph edits. Help → Performance modes and
[conducted MIDI usage](CONDUCTED_MIDI.md) describe these controls.

Local input automatically belongs to the designated conductor (owner fallback
when undesignated), with one browser source lease and a server acknowledgment.
The conductor and editors can learn from that source and change device choices;
other members cannot spoof source data or assign bindings. The App retains local
MIDI across editor/stage changes and reconnects previously enabled inputs after
reload. Server-connected inputs reuse bounded native MIDI rings, with discovery,
authorization, persistence and routing on a separate control worker. Cues still
use engine pulse/count-in scheduling; toggle decisions use current sequencer
state. Selected sets, device presence and learn/playback state are transient.

Validation: 266 Rust checks passed across the workspace (one manual throughput
benchmark ignored); the final core/server pass also passed all 114 tests. All
111 frontend tests, the production build, and 10 affected browser cases passed.
Browser coverage includes editor rehearsal, queue/count-in/repeat behavior,
progress direction/layout, first-message capture, cancellation, held-button
suppression, wrong device/channel rejection, set selection and advance, learning
under the performance lock, reload, shared conductor/editor device assignment,
unauthorized source/binding rejection, reduced-motion reorder, existing graph
local MIDI and Help. The rendered editor was visually reviewed. Native devices
were disabled for browser tests; physical local/server MIDI, driver hotplug and
hardware latency remain manual and are not verified performance claims.

## Bar-ending notation spacing (2026-09-13)

The shared notation layout reserves extra room before barlines so a final
sixteenth-note head/flag no longer overlaps the barline at compact spacing.
Musical onsets and durations are unchanged. The browser regression reproduced
an overlap before the fix and now verifies clearance at minimum, default and
maximum spacing, note selection and unchanged saved timing. All 108 frontend
tests, three affected browser cases and the production build passed; the
rendered score was visually reviewed.

## Arrow-key bar creation and jazz chord symbols (2026-09-13)

Quick-entry Right/Shift-Right now offers to append one bar when navigation would
enter a bar beyond the score. Enter or Add bar confirms; Cancel/Escape keeps the
caret in place. A session checkbox permits subsequent automatic appends; its
browser-tab preference survives view changes and reload but is not saved in the
project. Held-key repeats cannot repeatedly append. Creation reuses the existing
undoable bar splice, respects the current final meter, completes partial final
bars, and retains edit locks and the 4096-quarter-beat limit. Left remains bounded
at zero. The Score entry help page and repository guide document this workflow.

The C⁷ Chord tool places jazz chord symbols above an individual staff at a beat.
The editor offers a live preview, root/slash-bass selectors and common quality
buttons, plus free text (64 characters). The tool stays armed for a progression;
placing at an occupied chord beat edits it. Symbols support existing mark edit,
drag, delete, undo, save, bar editing and region-copy behavior. They are notation
only and generate no MIDI or chord voicings. Core validation accepts the new
`chord` staff-mark kind and bounds its single-line text. Existing read-only staff
renderers display the same symbols.

MusicXML exports pitch-root symbols as harmony with the literal displayed quality
and optional slash bass, and imports harmony symbols. Unknown qualities use the
MusicXML `other` kind; custom non-root labels export as text with an interchange
notice. Native JSON retains all labels exactly. Chord entry and these interchange
limits are included in Help → Score entry and docs/SCORE_EDITOR.md.

Validation: all 21 core Rust tests, 108 frontend tests and the production build
passed. Eight distinct affected browser cases passed, including keyboard-only
bar confirmation, cancellation, session/reload behavior, changing meter and
partial bars, existing quick entry/deletion, chord editing/drag/delete/undo,
server text validation, and chord/tempo MusicXML round trips. The confirmation,
chord editor and rendered chart were visually reviewed. No hardware timing or
physical-device behavior is claimed by these software checks.

## Score deletion and quick-entry documentation (2026-09-13)

Deleting selected notes/explicit rests or the entry before the Write caret now
closes the newly freed duration within its starting bar, staff and voice. Chord
occupancy is counted once, surviving pitches retain their occupied time, and
later bars stay fixed. Automatic rests between notes close their gap; trailing
automatic rests retain the existing hide-symbol behavior. Exact onsets,
hidden-rest ranges, grace attachments and note references are updated in the
same undoable score draft. Invalidated ties are removed. Bar-region clearing
still keeps the selected bars. Scheduling and engine processing are unchanged.

Documentation now includes a dedicated **Score entry** page, with mouse and
quick-entry workflows, the duration-key mapping, rest-length examples (`R → 5 → R`
and `0` for the current duration), persistent dot settings, keyboard ties, chords,
voices, MIDI entry, deletion and saving. The repository score guide and caret
shortcut hint are updated as well.

Validation: 105 frontend unit tests and the production build passed. Eight
browser cases passed, covering selection/rest/caret deletion, multi-selection,
undo/reload, rest lengths/dots/ties, documentation at desktop and narrow widths,
and existing insertion, entry tools, note dragging and phrasing. Documentation
screenshots were visually reviewed. These changes were tested in software;
physical keyboard/MIDI devices and touch gestures were not tested.

## Voice envelopes, playback viewer and Granular Cloud (2026-09-13)

Polyphonic sampler reuses the standalone ADSR’s processing, parameter metadata,
and editable envelope graph, with an independent envelope in each of its 64
voices. The node displays the shape and the maximum active envelope level. Options
provides graph handles and numeric Attack, Decay, Sustain and Release controls;
connected settings use engine values and lock their handles. Stage times latch on
entry and sustain edits are smoothed. Voice occupancy remains independent of
amplitude, so silent sustain and newly starting attacks are not lost. Defaults
retain immediate attack/full sustain and the existing 80 ms release. One-shot
samples still finish at their sample boundary. Graph gestures and numeric controls
now share draft handling, fixing stale numeric fields after graph edits.

Clicking a Part player’s bar/beat panel opens its selected part in a read-only
modal. It reuses the score staff and piano-roll renderers, shows all staves,
switches between notation and piano roll, and follows the node’s authoritative
written position. Scrolling/view changes are local only. Stale telemetry hides
playheads; the modal does not edit notes or control show transport.

Granular Cloud derives its sample and numeric controls from Granular synth,
removes the typed MIDI inlet, and relabels `position` as Cloud center (0–1).
It shares the fixed 16-voice/128-grain Hann-window scheduler, sample preparation,
shortlist switching and reusable sample packaging. With no Gate/Trigger/note-off
wires it plays continuously while the engine runs, with optional numeric Pitch
and Velocity; wiring note controls uses the synth’s existing numeric note behavior.
Spray randomizes grain starts around center and wraps at sample edges. Center,
size and density changes affect newly launched grains. This is sample-based DSP,
not a physical MIDI or external-device feature.

Validation: 262 Rust tests passed (one ignored), including render-allocation
guards, independent voice envelopes, cloud center/sample switching and numeric
gates. All 98 frontend unit tests and the production build passed. Eleven
affected browser cases passed, covering the three features, existing Part player
and sample selector behavior, MIDI/OSC sampler playback, and all 131 authored
help examples. Notation, piano roll and envelope layouts were visually reviewed.
These are software checks; no physical audio or MIDI hardware was tested.

## Independent part player (2026-09-13)

The Control library includes **Part player**, with a project-local Source part
selector in Options and Play, Play & repeat, and Stop trigger inputs. Play queues
one complete pass at the next metronome beat; repeat queues continuous looping;
Stop releases held notes and cancels pending launches. Retriggering starts over at
the next beat without count-in. Coincident inputs prioritize Stop, then repeat.
A pending launch is fulfilled before a new trigger on that beat, so periodic
trigger streams do not postpone playback indefinitely.

Each node advances independently using engine samples and the current BPM, even
when the performance is stopped. While the show runs, launch boundaries share its
metronome position, including meter changes and written repeat jumps; otherwise
launches follow the continuously running graph beat grid. Already running clips
keep their local phase through show play/pause/seek/stop and tempo changes. The
node shows the selected part, written bar/beat, and waiting/playing/repeating state
from telemetry. It has standard typed MIDI and scalar note outputs for instruments
or MIDI output nodes. The engine must be enabled.

Server preparation reuses score ties, articulation, grace timing, notation repeats,
staff channels, dynamics and MIDI automation. Explicit graph playback ignores
show mute/solo and performer queues; authored tempo marks do not override current
BPM. Prepared player storage is limited to one million note/automation events per
graph. Compatible replacements preserve playback; changed/deleted clips release
held notes into surviving typed consumers. Preparation allocates outside rendering;
rendering and state transfer use prepared storage. Help includes a freeform
algorithmic-snippet example wired to Play, repeat, Stop and MIDI output.

Validation: 256 Rust workspace tests passed (one opt-in test ignored), including
sample-index assertions for starts, repeats, retriggers, completion, Stop,
positive-edge behavior, coincident periodic triggers, meter/repeat boundaries,
tempo/show changes and graph replacement. A server test checks notation, channels,
automation and independence from muted show parts. The allocation/deallocation
guard covers looping, retriggering, stopping and retiring players. All 97 frontend
tests and the production build passed, with the existing bundle-size warning.
Six Chromium cases passed: the player workflow with synth output and persistence,
MIDI/OSC/sampler routing, and four help workflows including all 130 node examples.
The node screenshot was inspected. Physical MIDI hardware and timing remain
manual and unverified; no hardware latency or deadline claim is made.

## Sample selector (2026-09-13)

The Control library now includes Sample selector: an ordered, zero-based shortlist
of up to 64 project samples, configured by autocomplete in the modal. Entries can
be nicknamed, reordered and removed. Orange rounded buttons display the nickname
or sample name; clicks select the normal saved Index parameter. A connected Index
uses engine values and disables manual buttons. Fractional indices round down;
empty/out-of-range selections output 0.

Its numeric Sample ID output connects to the new Sample ID input on Sample player,
Polyphonic sampler and Granular synth. Preparation loads matching-channel shortlist
assets before rendering within the existing 256 MB prepared-audio limit. Switching
swaps retained buffers and clears old playback/voices; note triggering remains
separate and root MIDI note retains its configured value. Invalid/unprepared IDs
select silence with missing-sample telemetry. List fields and project asset access
are server-validated. Reusable subgraphs bundle and remap shortlist-only samples.
The in-app reference includes a Kick/Snare/Hat palette example and numeric control.

Validation: the Rust workspace suite passed (one opt-in test ignored), followed by
all three focused selector regressions; render allocation/deallocation guards
include rapid sample switching. Tests cover all three consumers at 1–8 channels,
invalid indices, empty banks and graph replacement. All 97 frontend tests, the
production build and seven focused Chromium cases passed, including actual sampler
output, list persistence, connected-input locking, asset remapping and rendering
all 129 authored help examples. The node screenshot was inspected. The build keeps
the existing bundle-size warning. Physical audio/MIDI and touch devices were not
used; no hardware timing or CPU-throughput result is claimed.

## Score note dragging and playback editing (2026-09-13)

Dragging an existing note now previews its staff position and onset without a
selection rectangle. The grabbed note auditions on pickup and pitch changes
through its saved instrument and matching Part MIDI routes when the engine is
enabled. The server validates the optional preview pitch; previews do not save
revisions, and note-offs retain the existing rendered-sample countdown. Release
saves one undoable move. Escape, pointer cancellation/capture loss, window blur,
and playback start discard the position preview. Empty-space box selection is
preserved. Rapid successive drags no longer open the note-edit dialog.

The score editor disables editing during playback/count-in and restores it on
pause/stop. This is a UI editing lock, not a new restriction on preparation APIs.
Rust workspace tests passed 244 cases (one opt-in test ignored), frontend tests
passed 97 cases, and the production build passed with the existing bundle-size
warning. Fourteen focused Chromium cases passed across note dragging, entry,
mass edits, phrasing, workspace and save workflows. The new real-server case
observed Part MIDI pitch previews and release, verified no save before dropping,
one-step undo, cancellation, playback locking and engine-disabled movement.
Physical MIDI/audio devices and touch hardware were not tested.

## Contextual help and documentation (2026-09-13)

Explanatory prose in nodes, settings, score dialogs, libraries and menus now uses
info buttons beside the relevant titles/fields. The `i` key and header control
expand/collapse descriptions in place; the preference persists locally. Info icons
and hovers remain available in both states. Expanded node descriptions sit below
the control body so the ports retain their positions. Native popovers
are mounted inside the active dialog so both stacking and modal inertness are
handled, with fixed-position fallback for older webviews. Hover, focus, keyboard
activation, tap, Escape and outside dismissal are supported. Errors, connection
states and essential action status remain visible.

The guide now includes a fuller first-sound walkthrough, troubleshooting,
server/client architecture, the desktop private engine and remote/LAN hosting,
and separate `init.sh` and `build.sh` references. All 128 built-in catalog nodes
supply authored usage/examples from `pr0-core`, including required sample/device
setup. Examples use the main Vue Flow node/edge components, dynamic port metadata
and measured layout in an isolated read-only flow. They expose example settings
without subscribing to audio, requesting waveform previews or changing projects.
The server validator checks every example graph. See [DOCUMENTATION.md](DOCUMENTATION.md)
for the node-author contract.

Validation: `cargo test --workspace --quiet` passed 244 tests (one opt-in test
ignored); frontend tests passed 97 cases; the production frontend build passed
with the existing bundle-size warning. All 16 affected Chromium cases passed,
including render/interaction checks for all 128 examples, modal popup hit-testing,
global preference persistence, inline expansion with stable node ports, hovers in
both toggle states, a narrow-screen/tap check, node controls, score
editing, device preferences and profile editing. Screenshots of the guide, ADSR,
spectral and channel examples, expanded modal/node descriptions, modal tooltip
and narrow layout were inspected.
`/bin/bash -n` and help checks passed for both scripts; the isolated launcher
lifecycle test passed, including the unchanged `--startup` branch. No startup
services were changed. Physical audio/MIDI, native Tauri webviews and actual touch
hardware were not tested, and the desktop application bundle was not repackaged.

## Capabilities and evidence

| Area | Current behavior | Evidence / limitations |
| --- | --- | --- |
| Graph engine | Server-validated audio (1–8 channels), control, spectral and typed MIDI contracts; nested graphs; feedback loops through audio/control wires (one-sample feedback edges); prepared DSP storage and live compatible-state transfer. | Rust render allocation/deallocation guards and routing tests. Incompatible replacements have no crossfade. |
| Native audio | Bounded callback rings, driver-consumption pacing, device/channel routing and meters. | Synthetic callback-size tests. Multiple independent hardware clocks are not synchronized; physical latency and endurance remain manual. |
| Persistence during playback | Ordered bounded queue sends archive data, loop chunks and retired engines to a persistence worker. Queue pressure delays command admission while rendering continues. Loop collection copies at most 4,096 samples per block; assembly, writer barriers and retired-engine destruction run off the audio-producing worker. Disable, clear and shutdown acknowledgments wait for preceding writes. | `persistence::tests` holds storage blocked while rendering advances; bounded snapshot reconstruction test; looper/recording browser tests. Real disk stalls and hardware refill deadlines remain unverified. |
| Browser monitoring | WebRTC/Opus master and selected cue feeds; only subscribed dedicated feeds are collected. Feed leases expire after eight seconds without refresh. | Count-in tests receive actual master/cue audio. Conversion and telemetry still share the orchestration worker. |
| Score and transport | Shared score, independent part launches, repeats/navigation, tempo map, notation/piano roll, staff routes and MIDI automation. Meter-aware metronome follows written position and bar origins through changes/repeats. | Sequencer/DSP tests and Chromium integration. External MIDI delivery is best effort. |
| Conducted performance | One designated non-performing conductor; ordered animated set/tile editor; locked touch stage; next-pulse single and armed-group cues; one-shot or forced-repeat playback; timed per-performer successor queues; continuous synthesized performer notation/rest lane; part-local polymeter; targeted cue clicks; conductor-authoritative dynamics; MIDI Learn; and authenticated performer Web MIDI ingress. | Scheduler/core/frontend and focused Chromium coverage. Physical touch/MIDI/iPad, browser MIDI reconnect behavior, multi-client latency and performance-scale acceptance remain manual/unverified. |
| Editing and saves | Project-owned score draft survives tab unmounts; flush waits for current and newer edits. The project browser exports portable JSON and imports it as a new project or, after a same-name warning, a renamed or overwritten accessible project. New-project import validates before inserting it. Transfer excludes workspace people, local devices, local MIDI bindings and sample audio (keep server sample backups separately). Save revision, project switching, performance entry and sign-out use the barrier. Failures retain the newest draft for explicit reapplication/discard. | Delayed/rejected save unit and browser regressions. Drafts are in browser memory, not offline durable storage. |
| Dynamics and phrasing | Staff overrides preserve part defaults for other staves. Ramp conversion preserves authored holds and curve shapes. New hairpins own an identifiable dynamics event and retain any authored starting mark for restoration; gesture previews are transient, with one commit/undo on release. Moving/removing owned playback updates its wedge. | Value-level ramp and hairpin tests plus browser editing. Hairpins crossing existing ramps/interior points are rejected instead of overwriting them; older wedges without owned events remain independent notation. |
| UI | Custom graph node theme, paper score, visible save/conflict status, keyboard-focusable ramp points, coarse-pointer touch targets and reduced-motion support. The footer offers transient Play + Repeat and swaps Performance mode for End performance in place; the conducted cue deck scrolls vertically in short viewports. Save coordination, MIDI device lifetime and curve gesture transforms are separate modules. | Frontend tests/build and browser tests. Physical touch/Safari validation remains manual. |
| Ensemble | Member browser with profile summaries, direct existing-user addition, invitation links and self-service square avatars. Owners can remove non-owners; assigned parts become unassigned in the same server transaction. | Rust crop/assignment tests and isolated HTTPS/API workflow. Role editing and the broader user profile editor remain future work. |
| MIDI/OSC | System settings and receiving/sending paths exist. Every graph MIDI source (Part MIDI, MIDI input, OSC-to-MIDI, Piano) receives raw channel messages and derives its five scalar outlets from the same frame that its typed `midi` output forwards, so the two paths cannot disagree. MIDI input keeps the channel byte; OSC-to-MIDI and Piano use channel 1. Piano decodes typed input (keys light, outlets move) and republishes scalar-driven notes. MIDI inputs accept several cables, concatenated in connection order per sample. A MIDI output forwards a typed cable raw and decodes scalar cables, never both. | Engine tests cover each source→sink pair over typed cables, fan-in order, reset propagation, scalar override and the single-send contract; external delivery remains best effort and physical MIDI hardware is unverified. |
| Deployment/assets | Local fonts/assets, native HTTP/HTTPS and desktop packaging, Bash 3.2-compatible launcher. | Existing launcher/desktop validation is historical evidence; startup services are never installed/enabled by automated tests. |

## JavaScript control scripting

JavaScript control nodes (2026-09-13): implemented separate QuickJS workers,
server-verified dynamic numeric ports, typed MIDI handlers/output, raw typed OSC
message subscriptions, engine-block ticks, metronome subscriptions, named control
observation/publication, engine-time timers and scheduled numeric/MIDI output.
The modal includes a locally bundled CodeMirror editor with syntax highlighting,
helper completion, examples, Check/Apply during engine operation, live values and
runtime diagnostics. `console.log/info/warn/error/debug` reach the GUI Console.
The full [Scripting guide](SCRIPTING.md) is also available in Options and Help.

Verified: the Rust workspace suite passes (one existing manual software-throughput test
ignored), including JS runtime/sandbox/API and server-manifest tests, exact engine
sample scheduling, reset cancellation, named-publication replacement/fallback,
unchanged worker migration across insertion/reordering, and allocation/deallocation
guards for the bridge. Additional regressions cover live-start 96 kHz timers,
command ordering and bounded MIDI/sustain releases. The 115 frontend tests and
production build passed (existing
large-chunk warnings remain); seven focused Chromium cases passed, including
three scripting cases plus existing control/named-route regressions. These use a
real isolated server, graph MIDI and local UDP OSC, and confirm live compilation,
failure/restart behavior, GUI Console output and guide access. Hardware MIDI,
audio latency, iPad/Safari, sustained multi-script loads and deadline reliability
remain manual/unverified. Reactive JS outputs have asynchronous worker latency;
this is not an audio-processing or sample-synchronous control feature.

## Remaining performance work

- Dedicated real-time scheduling, command preparation and monitor conversion isolation; incompatible-graph crossfades.
- Adaptive per-device clock drift correction, physical loopback/latency measurements, real MIDI hardware and iPad/Safari runs.
- Worst-case device refill measurements under edit/storage/visualizer stress, a 60-minute soak, and the physical 32-player acceptance run.
- Worker telemetry now includes maximum observed work and block-start gap in microseconds. These software measurements include scheduling/configuration effects and are not a hardware latency or deadline guarantee.

## Validation

Explicit local input ownership (2026-09-12): owners/editors assign Local audio
input nodes to ensemble members in node options. Assignments persist separately
from score parts. Unassigned nodes stay disconnected; performers and privileged
users alike can configure/send only their own assigned inputs. Device selection
remains local; “Use this device” transfers the user's capture to their current
instance. Reassignment revokes the old sender and clears its queued audio.
Disconnected or non-sending microphones use a gray ring/icon, based on server
connection state and recent decoded packet arrival. Browser capture permission,
physical inputs and iPad behavior remain manual/unverified.
Validation: 243 Rust tests passed (one opt-in ignored), 97 frontend tests and the
production web build passed. Three focused Chromium cases passed, including live
reassignment, sender revocation/reconnection, owner/editor/performer/conductor
permissions, rejection of non-members, local device control visibility, and gray
disconnected versus active microphone states. The audio fixture is generated,
and the assignment UI fixture denies microphone access explicitly. The release
server build and two isolated desktop process tests passed. Packaging this change
is pending: the signing wrapper stopped at its Keychain-profile preflight because
the login Keychain is locked; the project-root app remains the previous build.


Local input controls, profiles and branding (2026-09-12): Local audio input defaults
unmuted and automatically starts its authorized microphone uplink when the engine
is enabled. The flat, orange-accented microphone button on the node toggles graph mute;
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

- Native desktop build entry points (2026-09-13): `build.sh` now selects the
  current macOS/Linux host and forwards Windows Git Bash to the new `build.ps1`.
  Native x86-64 Windows builds stage target-suffixed `.exe` server/FFmpeg
  sidecars and request NSIS/MSI bundles. All platforms preflight required tool
  versions and native libraries, print exact package-manager commands, and can
  install supported missing prerequisites after an interactive confirmation or
  `--install-deps`. Eleven isolated build-routing/dependency tests pass, along
  with Bash syntax/help checks, the desktop Cargo check, all 111 frontend tests,
  the production frontend build (with the existing large-chunk warning), and a
  check of the cached pinned FFmpeg helper. No Windows or Linux package was
  produced or launched in this validation; native WebView, installer, FFmpeg
  import, and physical audio/MIDI behavior on those systems remain manual and
  unverified.

- Native Windows release automation (2026-09-14): the GitHub Actions workflow
  builds an NSIS installer on a GitHub-hosted Windows x86-64 runner using the
  native PowerShell entry point; macOS and Linux remain local builds. It runs
  manually or for `v*` tags, never for pull requests. The build job has a
  read-only repository token; tag publishing is isolated in a dependent
  GitHub-hosted Linux job with write permission. The workflow retains the run
  artifact for seven days, and turns a tag result into a visible prerelease with
  the installer attached. Windows signing remains unconfigured. Its static
  routing, prerequisite, cache, retention and release contract is included in
  the build tests. The earlier self-hosted run `34801066915` completed
  successfully in 26m22s and uploaded the unsigned 32,372,687-byte
  `pr0former_0.1.0_x64-setup.exe` installer (SHA-256
  `609bdc2a792320a83ed6465f669f61950fe45aec283ce227f527fcb4e03c1701`). The
  GitHub-hosted run `34836051960` completed the Windows bundle job successfully
  in 22m18s and produced the unsigned 32,380,070-byte
  `pr0former_0.1.0_x64-setup.exe`, which is attached to the `v0.1.0`
  prerelease. Its initial publishing job could not infer a repository because it
  has no checkout; the current workflow explicitly passes `$GITHUB_REPOSITORY`
  to every `gh release` command and has focused test coverage, but that corrected
  publishing path has not yet run for a new tag. Windows installation, WebView,
  FFmpeg import, and hardware behavior remain manual and unverified.

- GitHub visibility (2026-09-14): `andor83/pr0former` is public. Before the
  initial public release, tracked filenames, current tracked contents, and Git
  history were checked for common private-key/token patterns with no matches.
  Ignored local data, certificates, recordings, dependencies, and build outputs
  were not uploaded.
  by either visibility change.

See [VALIDATION_HISTORY.md](VALIDATION_HISTORY.md) for dated prior runs, [AUDIO_ENGINE_AUDIT.md](AUDIO_ENGINE_AUDIT.md) for the September 8 audit, and [ARCHITECTURE.md](ARCHITECTURE.md) / [SCORE_EDITOR.md](SCORE_EDITOR.md) for current contracts and usage.
