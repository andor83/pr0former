# Current implementation status

Native device routing follow-up: Linux discovers individually named PulseAudio
sinks/sources (including PipeWire's Pulse server), opens exact endpoint PCMs, and
exposes explicit ALSA card/device pairs including nonzero HDMI devices. These new
Linux routes are opt-in. The details panel shows ports, profiles and defaults;
native `pw-dump` fallback is diagnostic only. Windows selections now use opaque
WASAPI endpoint IDs while showing friendly labels, preserving unambiguous legacy
numeric route IDs. macOS CoreAudio behavior is unchanged. A local CPAL 0.16 patch
skips the absent `/dev/dsp` OSS probe; native inventory is cached for 30 seconds.
See [NATIVE_AUDIO.md](NATIVE_AUDIO.md) for dependencies, routing workflow and limits.
299 core/DSP/server tests and 117 frontend tests pass, along with the production
build and five focused browser fixture/API cases. CPAL's Windows backend passes
Windows-target type checking; this is not a full Windows application build.
Physical Linux/Windows audio, pro-audio gear, hotplug and latency remain unverified.
This does not add ASIO, exclusive WASAPI, or integer-format direct ALSA streams.

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

## Container frames and sample credits page (2026-09-15)

`container` is a UI-only Layout node: no ports, no engine role, structural
`color` (0–15 palette index), `width` and `height` parameters, and an optional
text `control_value` (≤ 2,000 bytes) shown as the info hover beside its title.
That text is documentation, not a control signal: core exempts containers from
the 256-byte control-string bound and the engine skips the fixed control-text
buffer for them (a long explanation used to fail the save in debug builds and
truncate in release), covered by core and engine tests.
Membership is geometric and nothing extra is persisted: a node whose anchor lies
inside a frame is rendered as a Vue Flow child with frame-relative position, so
dragging the frame carries it (the drop handler moves members by the frame's
delta). Dropping a node into a frame snaps it to the frame grid (228 px columns,
24 px rows below a 44 px header, 16 px padding) and grows the frame around the
node's measured size; frames never shrink automatically and never nest. The
bottom-right corner resizes with live feedback and persists on release. Frames
render behind other nodes (`z-index: -1`, enforced in CSS so selection does not
lift them over members). Options offers sixteen colour swatches and an
explanation textarea; the documentation example graph renders the frame too.
Verified by container geometry unit tests, the core/server suites, the build,
and a Chromium case covering stacking, engine tolerance, carrying members,
snapping and growth on drop, corner resizing, palette and explanation hover.

The Help center gained a "Sample credits" page rendering `docs/SAMPLE_CREDITS.md`
(also at `?help=1&topic=credits`). That document already attributed the bundled
menegass CC0 drum kit; it now adds the recommended orchestral source, VSCO 2
Community Edition (CC0, GitHub), with fallbacks and their attribution
obligations. No orchestral samples are bundled.

## Console header and Shift+` (2026-09-15)

The engine console (modal and standalone `?console=` page) now has a single
compact header line: the title on the left with Autoscroll, Clear, Open in new
tab and Close on the right. The "PROJECT ENGINE" eyebrow and the "Recent server
activity · up to 2,000 entries" caption are gone; the server's 2,000-entry bound
is unchanged. Shift+` opens and closes the console modal whenever a project is
open and the focus is not in a text field (checkboxes and buttons do not block
it, so it also closes the console from its own controls); the gear menu item and
Help list it. Verified by the new console Chromium case (header height, control
alignment, toggle open/close, standalone page) plus the scripting GUI-console and
control-step cases.

## Presence badge, canvas descriptions and sample browser (2026-09-15)

Presence now records the user behind each project event socket and broadcasts a
`presence` message (`users`: distinct connected users) on every join and leave,
plus once to each new socket after its snapshot. The header shows that count as a
badge beside the server indicator only when it exceeds one, with a "Connected
users: N" hover; a second tab of the same user does not raise it. Node descriptions
no longer render on graph nodes in either help state; they remain in the Options
modal header and the documentation example settings. The full sample browser is a
pinned-header/pinned-pager flex dialog: the search spans the modal, the tag row
shows the eight most-used tags with counts, `tag:a,b` query terms match either tag
case-insensitively and rank samples carrying more of the listed tags first, cloud
and row chips edit that term, the list is paginated (10/25/50/100 per page) under
a sticky column header, and rows are one line (name · details · tags) above 740 px
and stacked below. Verified by presence unit tests, the server suite, 125 frontend
unit tests, the build, and Chromium: presence-badge, sample-browser (18-sample
catalog including the bundled kit), documentation, link-quality, sample-library
(two of three), sample-organizer and graph-controls. Two failures predate this
work and reproduce on the base tree: documentation:70 stops at the guide's
"Architecture: server and clients" heading, and sample-library:11 fails to import
a six-channel FLAC ("The audio stream declares no channel layout").

## Graphical control step, inline checkboxes and engine hotkey (2026-09-15)

`control_input` gained a structural `step` parameter ("Step (0 automatic)", shown
for Integer, Float and Slider controls as a plain number field). A non-zero step
snaps slider dragging to that grid and moves the arrow keys, the value field's
up/down keys and the ▴▾ buttons by one step; 0 keeps the previous defaults (1 for
Integer and Float, free dragging and 0.01 nudges for Slider). Core rejects
negative or non-finite steps and fractional steps in Integer mode. The compact
fader now takes keyboard focus on pointer down, so a click followed by arrow keys
works, and exposes its step as an attribute. On/off parameters (MIDI passthrough,
hide chrome, changes only) render as a checkbox beside their label instead of a
separate row. The ` (backtick) key toggles the audio engine when the workspace has
focus, mirroring the transport button's enabled state; the button title and Help
list it. Verified by the core/server suites, the frontend build and unit tests,
and Chromium: the new control-step case (three repeats, cold start included) plus
the graph-controls, live-controls, control-routing, toggle, route-targets,
script-modal and scripts cases. `slider-precision.spec.ts` was already failing on
the base tree at its fader step assertion; that assertion now passes and the case
stops later at a driven-value `output` element the compact control has not
rendered since the control-mode rework. Hardware controllers remain manual.

## JavaScript control: MIDI passthrough and Options layout (2026-09-15)

`js_control` gained a structural `midi_passthru` parameter (default on) shown as a
"MIDI passthrough" checkbox in Options. With it on, the engine leaves the block's
incoming MIDI in the node's frame, still forwards every message to the script
worker, and appends whatever the script sends after the relayed messages; with it
off the script consumes incoming MIDI as before. Relaying continues while a worker
has faulted. Toggling the flag saves the project without restarting the script.
Existing scripts that forwarded input explicitly will double-send until the flag is
turned off. The script Options dialog is now a flex column whose parameter list
scrolls, so the runtime log no longer spills over the footer. Verified by a DSP
engine test, the core/server suites, and Chromium runs of the new script-modal
case (layout bounds, default checked, persistence when unchecked) plus the three
existing scripting cases. Hardware MIDI remains manual.

## Header link-quality badge (2026-09-15)

The header's server indicator now rates the event socket from the existing
once-a-second ping/pong. When the median of the last five round trips reaches
250 ms, or a pong has been outstanding for 750 ms, the indicator becomes an amber
"Slow connection" badge showing the worst recent round trip, with a hover note
that the link is only suitable for editing the graph and parts. It clears once
every recent round trip is under 200 ms. This is a presentation-only browser↔server
round-trip measurement; it does not measure audio or MIDI latency and makes no
timing guarantee. Verified by unit tests and a Playwright run that proxies the
socket with a 400 ms hold; the badge is not shown in performance mode or on
narrow layouts, where the indicator was already hidden.

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

- Mobile-safe audio importer (2026-09-14): sample import is a host-owned
  capability instead of a hard-coded FFmpeg invocation. `RuntimeConfig` carries
  an `Arc<dyn AudioImporter>`; `crates/server/src/import/` defines the
  object-safe trait, the bounded `ImportSource`/`ImportLimits` inputs and the
  `DecodedAudio` output contract, a pure-Rust Symphonia importer, the retained
  hardened FFmpeg process importer, and a chain that falls back only when an
  importer reports an unknown container or codec — never after a limit or policy
  refusal. The standalone server and the desktop application decode in process
  first and keep their configured FFmpeg behind it; an embedded or mobile host
  can select `import::pure_rust()` and import audio with no executable at all.
  `samples::upload` runs the importer on a blocking worker and keeps the setup
  lock, the 64 MB/30 s/1–8 channel limits, the canonical 32-bit float WAV, the
  catalog registration, the rate-cache preparation and every existing response.
  The portable in-process format list is WAV (PCM, IEEE float, ADPCM),
  AIFF/AIFF-C, CAF, FLAC, MP3, MP4/M4A (AAC-LC, ALAC), ADTS AAC and Ogg (Vorbis,
  FLAC); MKV/WebM, Opus and MPEG Layer I/II are deliberately not enabled.
  **Unverified:** this change was written in an environment where neither Cargo
  nor Python could run, so no `cargo check`, `cargo test`, desktop Cargo check,
  Python build test or browser test was performed against it, and neither
  `Cargo.lock` nor `desktop/src-tauri/Cargo.lock` has been regenerated for the
  new `symphonia` dependency — both must be before any `--locked` build. The
  generated corpus tests are written but have never executed. No iOS
  cross-compilation, mobile build or device import was attempted, and nothing
  here is a claim about decoder behavior on real hardware.

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

- Desktop in-process runtime (2026-09-14): the Tauri application no longer spawns
  the bundled `pr0-server` executable or speaks a stdin/stdout protocol to it. It
  calls `pr0_runtime::start(RuntimeConfig::embedded(…))` on the Tauri async
  runtime and keeps the returned `RuntimeHandle` plus the exclusive `desktop.lock`
  as managed state for the application's lifetime. The hosting dialog's command
  calls `host_on_lan`/`stop_hosting`/`hosting_status` directly; the pending-reply
  bridge is gone. Quitting awaits the runtime's one ordered shutdown under the
  same 30-second ceiling, which now bounds a wait rather than a process kill.
  Application data, recordings, the bundled frontend and the bundled FFmpeg path
  are passed as typed configuration, and `PR0_DESKTOP_DISABLE_NATIVE_DEVICES` is
  read by the application without setting or forwarding any `PR0_*` variable.
  `pr0-server` is removed from `externalBin` and from both build entry points'
  staging; FFmpeg remains the one bundled external binary, and standalone server
  packaging and `init.sh` are unchanged. **Unverified:** this change was written
  in an environment where Cargo could not run, so no `cargo check`, `cargo test`,
  Bash syntax check, Python build/desktop test or packaged-application run was
  performed against it. `desktop/src-tauri/Cargo.lock` still predates the new
  `pr0_runtime` path dependency and must be regenerated before any `--locked`
  desktop build or `cargo metadata --locked` license collection can succeed.

- Tauri iOS host slice (2026-09-14): the Tauri package now uses the Tauri 2
  library entry pattern. `src/lib.rs` exposes `run()` behind
  `#[cfg_attr(mobile, tauri::mobile_entry_point)]`, `src/main.rs` is only a
  desktop launcher, and the manifest declares a `staticlib`/`cdylib`/`rlib`
  library target. The whole desktop host moved unchanged into `src/desktop.rs`
  behind `cfg(desktop)`, together with the application menus, connection chooser,
  certificate pinning, saved window layouts, LAN discovery browsing, hosting
  dialog, exclusive profile lock and bundled FFmpeg converter; `fs2`, `mdns-sd`,
  `reqwest` and `sha2` are no longer dependencies of a mobile build. `src/mobile.rs`
  is the new iOS/iPadOS host: it creates the one `main` webview, takes its
  writable roots from `app_data_dir()` and its read-only web assets from
  `resource_dir()`, builds `RuntimeConfig::embedded` with native devices enabled,
  runtime mDNS advertisement off and `import::pure_rust()`, starts `pr0_runtime`
  in process, validates the private loopback readiness, installs the private
  owner session as an HttpOnly/SameSite=Strict cookie, navigates to the loopback
  URL, and at exit runs `RuntimeHandle::shutdown` under the same 30-second
  ceiling with no process API. No executable is referenced or bundled on iOS.
  `tauri.ios.conf.json` sets a distinct `org.pr0former.mobile` identifier, empties
  `externalBin` and the security capabilities, pins iOS 14.0 and points at
  `Info.ios.plist`, which carries the microphone, local-network and Bonjour
  purpose strings, `NSAllowsLocalNetworking`, the iPhone/iPad device family and
  the orientation policy, and deliberately no background-audio entitlement.
  Desktop `tauri.conf.json` still ships `binaries/ffmpeg`. **Unverified:** neither
  Cargo nor Python could run in this environment, so no `cargo check`, `cargo
  test`, desktop Cargo check or Python build test was performed against this
  change, and the new build tests are written but have never executed. `tauri ios
  init` was not run, no Xcode project exists, and the workspace has never been
  compiled for `aarch64-apple-ios`: `midir`, `cpal`, `webrtc`, `opus`, `rquickjs`
  and bundled `rusqlite` are unconfirmed for that target and may need gating in
  `pr0_runtime` before the shell links. Whether iOS WKWebView accepts the
  `set_cookie` call and the cleartext loopback navigation is likewise unverified.
  No simulator or device run, no audio-session policy, and therefore no claim
  about iPad behavior of any kind.

- iOS cross-target dependency gating (2026-09-14): two narrow target gates in
  `pr0_runtime`, plus an audit of the remaining ones.

  `crates/server/Cargo.toml` adds a
  `[target.'cfg(any(target_os = "ios", target_os = "android"))'.dependencies]`
  section that re-declares `rquickjs` with its `bindgen` feature. `rquickjs-sys`
  0.13 ships pregenerated QuickJS bindings only for a fixed list of triples and
  stops the build on any other, recommending that feature; `aarch64-apple-ios`
  and `aarch64-apple-ios-sim` are both outside the list. Because Cargo unions a
  target section's features only when the expression matches the target being
  built, desktop, server and CI builds resolve `rquickjs` exactly as before and
  gain no libclang build requirement. Scripting is preserved rather than
  stubbed: same engine, same `rquickjs` API, same `scripts.rs`, no second code
  path.

  `crates/server/src/import/ffmpeg.rs` is no longer compiled for those targets,
  `FfmpegImporter` does not exist there, and `import::standard` returns the
  in-process Symphonia chain whatever converter name it is given — so no host
  configuration on a platform without process spawning can reach
  `std::process::Command`. The signature is unchanged, so desktop and standalone
  callers are untouched, and desktop behavior (in-process decoding first, the
  bundled converter behind it) is identical. Pure-Rust import is unchanged on
  every platform.

  Audited and deliberately *not* gated, because none was shown to be
  unavailable: `midir`/`coremidi`, `opus`/`audiopus_sys`, `webrtc`, `mdns-sd`,
  `hostname`, `rcgen`, bundled `rusqlite` and the vendored `cpal` (whose
  platform module already covers `target_os = "ios"`). Removing MIDI, the WebRTC
  monitor or LAN hosting to make a check pass would delete shipped behavior, so
  they stay in and are listed as blockers below instead.
  `crates/server/src/linux_audio.rs` still compiles everywhere but cannot spawn
  anything off Linux: `discover_routes` is `cfg(target_os = "linux")` and the
  `/api/system/audio/linux` handler returns `{"supported":false}` before
  reaching a process. `resources.rs` already degrades to `resident_bytes() ==
  None` outside macOS/Linux.

  **Unverified — nothing here was compiled.** Cargo could not run in this
  environment, so none of `cargo check --target aarch64-apple-ios`, `cargo check
  --target aarch64-apple-ios-sim`, the root `cargo check`, the desktop
  `cargo check`, `cargo test` or the Python build tests was executed against
  this change. Neither `Cargo.lock` was regenerated, and enabling `bindgen`
  adds `bindgen` and its dependencies to the iOS resolve, so both lockfiles are
  stale for a `--locked` build. That `rquickjs` 0.13 forwards a `bindgen`
  feature to `rquickjs-core`/`rquickjs-sys` is taken from that crate's
  documented remedy and has not been confirmed against the vendored manifest
  here. Nor has the generation itself been exercised: `bindgen` must find
  libclang and must be given the iOS target triple and SDK sysroot by
  `rquickjs-sys`' build script, and if it instead reads the host macOS headers
  the failure will surface as bad QuickJS bindings rather than as a missing
  feature. The following are therefore the *expected next* iOS blockers, not
  observed ones: `opus` 0.3 builds libopus from C source through
  `audiopus_sys` 0.2.2 and `cmake`, which has no iOS toolchain configuration in
  this repository; `midir` 0.10.4 has not been confirmed to select its CoreMIDI
  backend for `target_os = "ios"` rather than only `macos`; and `webrtc` 0.14's
  transitive crates are unconfirmed for the target. Each needs its own design
  decision if it fails, and none of them is addressed by this change.

- GitHub visibility (2026-09-14): `andor83/pr0former` is public. Before the
  initial public release, tracked filenames, current tracked contents, and Git
  history were checked for common private-key/token patterns with no matches.
  Ignored local data, certificates, recordings, dependencies, and build outputs
  were not uploaded.
  by either visibility change.

- iOS native linkage (2026-09-14): `tauri ios init` has been run, and the whole
  tree now compiles for `aarch64-apple-ios-sim` — `midir`/`coremidi`, the
  vendored `cpal`, `webrtc`, `opus`/`audiopus_sys`, bundled `rusqlite` and
  `rquickjs` with generated bindings all built, so none of the crates listed as
  expected blockers in the entry above needed gating. The unsigned simulator
  archive then failed at the final Xcode link with undefined CoreMIDI symbols
  (`MIDIClientCreate`, `MIDIObjectGetStringProperty`, the `kMIDIProperty*`
  constants) and undefined AudioToolbox symbols (`AudioComponentFindNext`,
  `AudioUnitInitialize`, `AudioOutputUnitStart`), because Cargo produces a
  `staticlib` and a static archive cannot carry the framework link directives
  its crates emit.

  The fix is `bundle.iOS.frameworks` in `desktop/src-tauri/tauri.ios.conf.json`,
  which Tauri renders into the generated XcodeGen `project.yml`: CoreMIDI,
  CoreAudio, AudioToolbox, CoreFoundation and Foundation. `gen/apple` is ignored
  by Git and regenerated from that configuration, so nothing was edited in the
  generated project. No MIDI, WebRTC or audio feature was disabled to make the
  link proceed, and desktop packaging gained no iOS bundle section, so desktop
  and standalone linking are untouched. `tests/test_build.py` asserts the
  framework list, that `midir` and `webrtc` remain dependencies, and that the
  desktop configuration has no iOS section.

  **Unverified:** the framework list was derived by reading the symbol tables of
  the built `libapp.a`, not from a successful link. Neither Cargo, Node nor
  `xcodebuild` could run in the environment that made this change, so the
  project was not regenerated, no archive was attempted, and the Python build
  tests were not executed. Two items are open and neither is addressed here.
  First, the C deployment target: the Opus `CMakeCache.txt` records
  `-mios-simulator-version-min=26.5`, the installed SDK version rather than the
  shipped 14.0 minimum, so every Opus object links with a deployment-target
  warning. Those objects were built by a standalone `cargo build` before the
  Xcode project existed and then reused from the Cargo cache, so whether the
  Tauri Xcode script path supplies the variable is untested either way. The
  lever that covers both paths is a one-line `IPHONEOS_DEPLOYMENT_TARGET` entry
  in the existing `[env]` table of `.cargo/config.toml` — the same table that
  already carries `CMAKE_POLICY_VERSION_MINIMUM` for this build — plus one
  `cargo clean -p audiopus_sys`, because that crate declares no
  `rerun-if-env-changed` for it. That file could not be written from this
  environment; the exact snippet is in `docs/IPAD_RUNTIME_PLAN.md`, and the
  build test skips with a pointer to it until the key exists. Second, the
  archive also references `AudioSessionGetProperty`, a `coreaudio-rs` 0.13
  reference to an AudioSession C API Apple deprecated in iOS 7; whether
  AudioToolbox still exports it has not been checked against the SDK. No
  simulator run, no audio-session policy and therefore still no claim about iPad
  behavior of any kind.

- iPad audio lifecycle (2026-09-15): the shared runtime gained one idempotent
  suspend/resume of native audio hardware, and the Tauri iOS shell gained the
  `AVAudioSession` policy and lifecycle bridge that drives it.

  `RuntimeHandle::suspend_audio`/`resume_audio` (plus `_blocking` forms for a
  host on an operating-system callback thread, and `audio_suspended`) close and
  reopen the output/input streams on the orchestration worker, between DSP
  blocks, through the same ordered command queue as engine enablement and
  shutdown. Nothing is added to `Engine::render` or to a device callback.
  Suspension stops rendering rather than falling back to the software schedule,
  so the engine sample clock, transport position, prepared graph, loop buffers
  and open recordings are exactly where resume finds them; enabling the engine
  while suspended opens no device, and resume reopens exactly the directions
  that were open when the hardware closed, so hardware output an owner had
  stopped is not restarted by a lifecycle event; a device that refuses to
  reopen leaves the runtime resumed with a device error. `/api/devices` and engine telemetry
  publish `audio_suspended`. Standalone-server and desktop behavior is
  unchanged — neither host calls these methods.

  `desktop/src-tauri/src/audio_session.rs` sets category, mode, preferred sample
  rate and preferred I/O buffer duration before the runtime can open a device,
  selecting `playAndRecord` (speaker, Bluetooth A2DP, AirPlay) only when the
  saved settings enable a native input and `playback` otherwise, so the
  microphone prompt stays attached to a feature in use. It then observes
  interruption begin/end, route loss, media-service resets and
  foreground/background for the life of the process, completing each transition
  before the handler returns within bounded deadlines (2 s to close, 3 s to
  reopen). Nothing reopens the hardware while the application is not in the
  foreground, so a route change or reset arriving in the background closes and
  stays closed until the application becomes active. Route loss reopens on the replacement route rather than pausing,
  because this is an instrument under the performer's control. Exit runs the
  ordered shutdown and only then hands the session back. Its Objective-C is
  behind `cfg(target_os = "ios")` and its dependencies (`objc2`,
  `objc2-avf-audio`, `objc2-foundation`, `objc2-ui-kit`, `block2`) are declared
  in a `cfg(target_os = "ios")` target section, so no desktop or server build
  acquires them. `AVFAudio` joins `bundle.iOS.frameworks`. There is still no
  background-audio entitlement and no keep-awake policy. `UIDeviceFamily` was
  removed from `Info.ios.plist`: Xcode generates it from the target's
  `TARGETED_DEVICE_FAMILY` and warns that a user-supplied key is ignored.

  Validation: 327 workspace Rust tests passed (one opt-in benchmark ignored),
  including new coverage for idempotent suspend/resume, the reported state
  through `/api/devices`, transitions completed from a plain thread with no
  async runtime, suspend/resume after shutdown, shared device opening, and the
  audio preferences a host reads before opening anything. 15 desktop Rust tests
  passed, including three that cover the session-policy decisions on a build
  machine, and 21 build tests passed. Both desktop process tests passed
  against a fresh release `pr0-server`, so the `--desktop` adapter is
  unaffected. The workspace and the desktop package
  both type-check for `aarch64-apple-ios-sim`, and both lockfiles now resolve
  with `--locked`, which earlier entries recorded as stale. `tauri ios init`
  was rerun and a
  debug simulator bundle built and linked: `AVFoundation` is now among the
  application's load commands where the previous build had no AVF framework at
  all, the merged `Info.plist` still declares `UIDeviceFamily` `[1, 2]` and no
  `UIBackgroundModes`, and the deployment-target warnings from the Opus objects
  are gone now that `.cargo/config.toml` pins `IPHONEOS_DEPLOYMENT_TARGET`. The
  previously open `AudioSessionGetProperty` question is answered for the
  simulator SDK: the reference resolves from AudioToolbox and the link
  succeeds; the device SDK has not been linked.

  Simulator behavior, with an important qualification — *superseded by the
  2026-09-15 entry below, which removed this blocker*: launching the build as it
  stood **aborted** before the interface appeared, in
  `wry`'s WKWebView `set_cookie` helper, which pumps the main run loop
  re-entrantly until Tao's Core Foundation observer panics through an
  `extern "C"` boundary (`SIGABRT`, `panic_cannot_unwind`). That is the private
  session bootstrap this repository already listed as unverified, not anything
  in this change, and it is the blocker for every remaining Phase 4 gate item.
  To exercise the audio lifecycle at all, the cookie call was bypassed in a
  **temporary local patch that was reverted and is not in the tree**; with that
  patch the application ran, `pr0_runtime` served its private loopback listener
  (answered from the host), the audio session was configured with no error
  reported, and three background/foreground cycles each logged a completed
  suspend and a completed resume with the runtime still serving afterwards.

  **Unverified:** no physical iPad, no audio was heard, and no interruption,
  route change or media-services reset was exercised in any form — the
  simulator provides no way to raise them. The engine was never enabled during
  the simulator run, so suspend/resume there closed and reopened nothing, and
  the state-preservation contract is covered by design and by host-side tests
  rather than by an observed hardware transition. Microphone permission, the
  `playAndRecord` path, WKWebView's own use of the audio session during a
  WebRTC uplink, Bluetooth and USB-C routes, and every latency claim remain
  untested. Nothing here is a statement about how iPad audio behaves.

- Webview session bootstrap and audio-lifecycle state machine (2026-09-15): the
  iOS startup abort is fixed, and the platform suspend/resume path was corrected
  where it lost the owner's hardware intent.

  **Session handoff.** Host-side `WebviewWindow::set_cookie` is gone from
  `desktop/src-tauri/src/runtime.rs`. `pr0_runtime` now publishes a one-time
  `RuntimeHandle::bootstrap_url()` — `http://127.0.0.1:<port>/__session-bootstrap/<72-char token>`
  — and the shell navigates its webview there once. The runtime replies with the
  same `pr0_session` HttpOnly/SameSite=Strict/Path=/ cookie login issues, plus
  `Cache-Control: no-store` and a `303` redirect to `/`, so the interface's own
  address holds no credential. The token is two v4 UUIDs, compared in constant
  time, destroyed by its first successful redemption and otherwise after eight
  wrong tokens, 120 seconds, or `shutdown()`. It is merged into the router
  *after* the local-network hosting router is cloned and *inside* the private
  listener's exact-authority Host guard, so it exists only on the runtime's own
  ephemeral loopback listener, never on the LAN listener, and never in
  `RuntimeMode::Server`. The long-lived session no longer passes through the
  host or the webview API at all. Authentication, CSRF, project authorization
  and the DNS-rebinding guard are unchanged; there is no JavaScript-readable
  token and no persistent query credential. Desktop multi-window cookie copying
  in `windows.rs` is a separate per-origin feature and is untouched.

  **Audio lifecycle.** The orchestration worker kept two "restore" flags
  snapshotted at suspension, which produced four real defects: enabling the
  engine while the hardware was closed left the runtime enabled and permanently
  silent after resume; `Command::Hardware` while suspended was dropped in both
  directions, so resume could restart output the owner had stopped and could not
  start output the owner had asked for; a failed reopen discarded the intent, so
  the *next* background/foreground cycle silently reported success with nothing
  open; and a repeat resume could never retry a failed open. The flags are
  replaced by a standing `Wanted` intent that every command maintains and that
  suspension does not touch. Resume reconciles — it opens whichever wanted
  direction is closed, independently, so a capture device the system will not
  hand back no longer keeps the loudspeaker closed either. `Unload` withdraws
  the capture intent with the capture it closes, so returning to the foreground
  cannot reopen a microphone with no project loaded; `Shutdown` withdraws both.
  A lifecycle transition that loses a race with shutdown is now answered
  (a suspend is satisfied by closed hardware) instead of returning a channel
  error.

  **Audio-session category.** `RuntimeConfig` gained an optional `AudioPolicy`
  capability, which the worker calls immediately before it opens a stream with
  the exact settings it is about to use. iOS supplies it, so a performer who
  enables an input in the *running* application gets `playAndRecord` installed
  before the input is opened, instead of opening against a playback-only session
  until the next background/foreground cycle. Launch-time `prepare` and the
  bridge's own reapply on resume are kept: they cover activation after an
  interruption or a media-services reset, when nothing is being opened. No other
  host supplies a policy, and none is reached from a render or device callback.

  Validation: 339 workspace Rust tests passed, including seven new
  audio-lifecycle cases driven through the real orchestration worker with
  `native_devices` cleared — so a *selected* interface refuses to open and an
  unselected one opens trivially, which is what makes the state machine
  observable with no hardware, no permission prompt and no audio. Four of those
  seven fail against the previous logic, which was checked by reinstating it.
  Five new runtime cases cover the handoff: it installs the session once with
  the right cookie attributes and redirect, stops working immediately after, is
  refused for a foreign `Host`, is absent from the local-network router and from
  server mode, is bounded to eight attempts, expires, is revoked by shutdown,
  and never renders its secrets through `Debug`. 16 desktop Rust tests and 32
  build tests passed.

  **Simulator run, without any bypass.** A debug `aarch64-sim` bundle was built
  and launched on an iPad Pro 13-inch (M5) simulator running iPadOS 26.5. The
  application reached its signed-in interface — the first time the iOS shell has
  displayed anything — created a project through its own API, enabled the
  engine, and opened CoreAudio output (`hardware_enabled: true`, `error: ""`,
  one enumerated `Default Device`). Four background/foreground cycles each
  logged a completed suspend and a completed resume; with the engine enabled,
  `/api/devices` reported `audio_suspended` and `hardware_enabled` flipping
  together each time and returned to open output. Stopping hardware output while
  suspended left it closed after resume, and asking for it while suspended
  opened it — the two cases the previous code got backwards. After a full
  application restart the project created in the earlier launch reopened, the
  engine re-enabled and output opened again, and the profile log recorded the
  worker-side policy install (`category=playback, 48000 Hz, 0.0027 s buffer`)
  before each open.

  **New blocking defect found on iOS, not introduced here: enabling a native
  input aborts the process.** *Resolved in the next entry below — the diagnosis
  below is correct as far as it goes, but the same abort also reaches stream
  opening, so the resolution is a safe refusal rather than a working input.* With one input interface enabled in the saved
  settings, the `playAndRecord` category is installed as designed — the log
  shows `category=playAndRecord` — and the process then dies with `SIGABRT` from
  `AudioToolbox`'s `_ReportRPCTimeout` → `abort()`, under
  `AURemoteIO::Initialize` ← `AudioUnitInitialize` ← CPAL's iOS
  `supported_output_configs` ← `pr0_runtime::audio::device_details` ←
  `settings::discover`, on a Tokio blocking worker. CPAL's iOS backend
  *initializes an AudioUnit just to enumerate supported configurations*, and
  AudioToolbox aborts the whole process when that RPC times out. It reproduces
  at launch with the engine never enabled, so it is the pre-existing launch-time
  `audio_session::prepare` plus the pre-existing enumeration path, not the
  capability added here; the capability only makes the same category reachable
  from a mid-session settings change as well. Removing the input from the saved
  settings restores a working application immediately. **No iOS build should
  enable a native input until this is resolved** — likely by requesting record
  permission before activating `playAndRecord`, and by not initializing an
  AudioUnit to enumerate on that platform. It is Phase 5 work and is not fixed
  here.

  **Unverified.** No physical iPad, and no audio was heard. The simulator cannot
  raise an interruption, a route change or a media-services reset, so none of
  those paths has been exercised anywhere, and the simulator has no microphone
  at all, so the abort above may or may not reproduce on hardware. The remaining
  Phase 4 gate items — importing fixtures, surviving process termination,
  touch/rotation/keyboard — were not run, and no latency or reliability claim is
  supported.

  **Out of scope, found during review and not changed here:**
  `crates/server/src/lib.rs` applies its `security_headers` middleware with
  `Router::layer` on an empty `Router::new()`, before any route is added, so
  `X-Frame-Options`, `X-Content-Type-Options` and `Referrer-Policy` reach no
  response on any host. This predates the branch — it is the same on `main` —
  and was confirmed against the running simulator build. *Fixed in the next
  entry below.*

- iOS logical audio routes, capture capability gating, and response hardening
  (2026-09-15): the iOS device path no longer enumerates, native capture fails
  safely instead of aborting the process, and the baseline security headers
  reach responses for the first time.

  **Platform device boundary.** `crates/server/src/native_audio.rs` holds two
  device models. Enumerated devices — macOS, Linux, Windows, the standalone
  server — are unchanged in behavior and in code; their implementation is now
  behind `cfg(not(target_os = "ios"))`. Logical routes are new and iOS-only: one
  output route `ios:default-output` and one input route `ios:default-input`,
  with stable IDs derived from those fixed names, resolved with CPAL's
  `default_output_device`/`default_input_device` (zero-sized values on that
  backend — no audio unit, no AudioToolbox RPC), and a stream configuration
  synthesized from the host's `AVAudioSession` reading instead of
  `supported_input_configs`/`supported_output_configs`. Discovery, settings
  merging, route validation, latency compensation and the settings API are
  otherwise unchanged; the iOS capture route is added *disabled* by discovery,
  under the same opt-in rule the explicit Linux PCM routes already used.

  **Capture capability.** `RuntimeConfig::audio_routes` is a new typed
  `AudioRoutes` capability: the current route description, `CaptureSupport`
  (whether the host's backend can record at all), `CaptureAuthorization`
  (granted / denied / restricted / undetermined / not-required) and a
  non-blocking permission request. The device layer checks support first and
  authorization second, once, before any capture device is touched, on the
  orchestration worker and never in a render or device callback. Hosts that
  enumerate devices install no capability and are unaffected — verified by a
  test rather than asserted.

  **What the investigation found, and why capture is off.** Not enumerating was
  necessary but not sufficient. `AudioUnitInitialize` is also what *opening* a
  stream calls, and the deadlock belongs to the session category rather than the
  direction. On an iPad Pro 13-inch (M5) simulator running iPadOS 26.5, with
  microphone access granted through `simctl privacy` and an input route present,
  installing `playAndRecord` and then initializing RemoteIO logged
  `Initialize: RPC timeout. Apparently deadlocked. Aborting now.` and `abort()`ed
  — in `open_outputs`, in `build_output_stream_raw`, before any capture unit
  existed. The host Mac had a default input device throughout, so this is not
  "the simulator has no microphone". Under `Playback` the same output opened
  normally, repeatedly. Separately, reading `AVAudioSession`'s input-availability
  property was itself enough to put the microphone prompt on screen at launch,
  confirmed by counting `kTCCServiceMicrophone` requests in `tccd`'s log.

  So the iOS host reports `CaptureSupport::Unimplemented`. Native input is
  refused with that sentence in `/api/devices` and in the engine error, no
  microphone prompt is produced, the input-availability read does not happen, and
  `playAndRecord` is never selected — which is what keeps *playback* safe, not
  only capture honest. Turning capture on is one constant in
  `desktop/src-tauri/src/audio_session.rs`; the permission gate, refusal
  messages, opt-in discovery rule and category decision downstream of it are
  implemented and tested, and a desktop test asserts the shipped value so it
  cannot change silently. `docs/IPAD_RUNTIME_PLAN.md` Phase 5 lists the custom
  CoreAudio work and the hardware evidence that would have to come first.

  **Security headers.** `crates/server/src/lib.rs` applied its `security_headers`
  middleware to `Router::new()` before any route was registered. `axum`'s
  `Router::layer` wraps only the routes that exist when it is called, so
  `X-Frame-Options`, `X-Content-Type-Options` and `Referrer-Policy` reached *no*
  response, on any host, on any route. The layer is now applied after every route
  and the fallback. The one-time session handoff, which is merged into the router
  after `assemble` returns, carries the same layer explicitly and additionally
  sets `Referrer-Policy: no-referrer` on both its redirect and its 404 — stricter
  than the application-wide `same-origin` floor, because the single-use path is
  itself the credential — while keeping `no-store` and single use unchanged.

  Validation: 352 workspace Rust tests (164 in `pr0_runtime`), 19 desktop Rust
  tests, 32 build tests, 117 frontend tests and the production frontend build all
  pass. New coverage: ten `native_audio` cases (stable route identities without
  enumeration, each refusal's wording, no prompt when the backend cannot record,
  the no-capability case, the opt-in rule, channel clamping, and that enumerating
  hosts are untouched); one `audio` case proving `open_inputs` refuses on
  authorization before it resolves a device; four `audio_session` cases over the
  category decision including the shipped capture constant; and three `runtime`
  cases over response headers — every route class on the private listener, the
  router local-network hosting serves, and the handoff's own responses. The
  header test was confirmed to fail against the previous layer ordering by
  reinstating it. `cargo check` is clean for `aarch64-apple-ios-sim` and
  `aarch64-apple-ios`, and both lockfiles still resolve with `--locked`.

  **Simulator run, without any bypass.** A debug `aarch64-sim` bundle was built
  from this tree and launched on an *erased* iPad Pro 13-inch (M5) simulator
  running iPadOS 26.5. `/api/system/audio` listed exactly the two logical routes
  with the output enabled and the input disabled; `/api/devices` showed two
  channels out, one in, and the capture refusal on the input row. A project was
  created, the engine enabled, and CoreAudio output opened through the logical
  route (`hardware_enabled: true`, no error, no underruns).
  Background/foreground cycles closed and reopened that output, with
  `audio_suspended` and `hardware_enabled` flipping together each time.
  Selecting the capture route and re-enabling the engine returned HTTP 400 with
  the capture sentence and the process stayed alive, where the previous build
  aborted. `tccd` recorded zero `kTCCServiceMicrophone` requests across the whole
  sequence, and no permission alert appeared. `X-Frame-Options: DENY`,
  `X-Content-Type-Options: nosniff` and `Referrer-Policy: same-origin` were
  present on the interface, on static assets, on a 404, on the API routes checked
  and on an unauthenticated 401.

  The aborting behavior above was reproduced deliberately, on an earlier build of
  this same change with capture enabled, to establish what it is: microphone
  granted via `simctl privacy`, `playAndRecord` installed, and the crash report
  showing `AudioUnitInitialize` → `AURemoteIO::Initialize` → `_ReportRPCTimeout`
  → `abort()` on the `pr0-orchestrator` thread inside `open_outputs`.

  **Unverified.** No physical iPad and no audio heard. A simulator is not audio
  hardware and is not evidence about physical output, latency, interruptions,
  route changes or media-services resets, none of which it can raise. Whether
  `AudioUnitInitialize` under `playAndRecord` deadlocks on real hardware is
  unknown, and nothing here should be read as an answer. The Phase 4 gate items
  not run remain not run: importing fixtures, surviving process termination, and
  touch/rotation/keyboard checks.

See [VALIDATION_HISTORY.md](VALIDATION_HISTORY.md) for dated prior runs, [AUDIO_ENGINE_AUDIT.md](AUDIO_ENGINE_AUDIT.md) for the September 8 audit, and [ARCHITECTURE.md](ARCHITECTURE.md) / [SCORE_EDITOR.md](SCORE_EDITOR.md) for current contracts and usage.
