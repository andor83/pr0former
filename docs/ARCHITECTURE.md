# Architecture

## Current execution model

`pr0-core` owns serde project types, the node catalog, deterministic graph validation, and the starter project. `pr0-dsp` compiles a graph into a stable topological schedule with preallocated node buffers, state, filter sections, delay lines, and bindings. `pr0-server` owns accounts, SQLite revisions, transport, device I/O, WebRTC, and external MIDI/OSC.

The orchestration worker groups rendering into configurable 32, 64, 128, 256, 512, or 1024-frame blocks at 44.1, 48, 88.2, or 96 kHz (defaults: 128 frames / 48 kHz). Internal event boundaries still split processing at individual samples. The block choice controls worker scheduling; CPAL uses the driver’s default callback buffer. Native output queues target at least two observed driver callbacks, two DSP blocks, and 10 ms of samples, capped below the 16,384-frame ring capacity. The driver callback size is observed through an atomic maximum; an initial 1,024-frame callback estimate primes the queue with silence before stream startup. This avoids repeatedly starving callbacks larger than the DSP block, at the cost of additional queued latency. It does not guarantee deadlines under arbitrary worker stalls or callback sizes beyond the bounded capacity. Without hardware output it follows a monotonic software schedule. With hardware output, a bounded SPSC ring is filled according to device consumption, targeting a small queue. CPAL input/output callbacks only move samples through bounded rings; they do not run codecs or access application locks. The DSP worker is **not yet a dedicated OS-priority realtime callback engine**. Allocation-free rendering alone does not establish deadline reliability; see STATUS.

The engine owns two clocks: `clock` tracks the show timeline and `graph_clock` advances while the loaded graph renders. Transport Play (and count-in completion) snaps `graph_clock` to the show beat and bumps its reset generation; Stop rewinds it to zero while it keeps running; Seek moves both. Graph timing nodes therefore agree with the score while a show plays and free-run during development. Enabling a project's engine prepares and loads its graph without activating the show. Graph clocks, oscillators, samples, piano/MIDI controls and audio routes run during development. Score scheduling alone follows show Play/Pause/Stop. The scheduler sends internal synth events before rendering the corresponding sample. External MIDI/OSC messages go through a bounded queue to a separate worker, so external delivery is best effort rather than sample-accurate at the receiving device. Tempo changes preserve beat position and can be queued for the next beat. Prepared live graph replacements are available through the UI and API in all modes. The server validates and prepares each revision off-thread, then the orchestration worker installs it between DSP blocks in command order, without waiting for a musical boundary. Following parameter commands therefore target the new graph. The setup mutex serializes graph preparation and parameter edits. Preparation permits score and graph edits during playback. A conductor-controlled runtime performance lock rejects project score/graph saves until preparation resumes; player stage views stay locally read-only. Active transport still prevents changing the scheduling mode. The legacy activate/deactivate API remains compatible, while Play can claim the enabled graph directly. Engine disablement stops transport and unloads the graph. The interface permits adding, moving, deleting, connecting, disconnecting, structural modal edits, and undo while active. Replacement preserves unchanged parts’ event cursors, launch origins, pending cues, and suspended notes. Compatible synth voices retain phase and release envelopes. Changed/deleted parts release their own external notes, while new or changed conducted/freeform parts start idle. Unchanged nodes transfer their prepared runtime storage, including delay/reverb history, filter state, spectral buffers, sample cursors, and control state. Compatibility requires matching node kind, channels, parameter defaults, incoming source/port contracts, and compensation lengths/latency. Compiled binding indices remain those of the new graph, including after node reordering. Changed configurations start with prepared state. Graph crossfades are not yet implemented.

Count-in playback holds the performance clock at beat zero while a prepared DSP count-in advances once per engine sample. Each click interval is one denominator beat (4 / beat_unit quarter notes); tempo edits preserve the current count-in phase. The first performance sample starts the sequencer at beat zero after the final complete count-in interval. Click PCM goes to the master browser monitor and every dedicated monitor feed, added to the continuing graph audio; hardware graph routes remain audible but never receive count-in clicks. Listeners must connect their browser monitors before starting. Monitor/network buffering still adds listening latency. Pause, Stop, unload and engine reconfiguration cancel pending count-ins; duplicate Play does not restart one. Resume after the performance has advanced skips the count-in.

The transport Play request accepts optional `count_in_beats` (integer 0–32, default 0 for existing API/OSC clients). The browser's compact Count in selector defaults to one current-meter bar and offers Off or 1–32 beats; this is a local playback choice reset when switching projects, not persisted project data. The existing Play flow activates and starts the audio engine first if needed, then requests the count-in. Telemetry reports nullable `count_in_remaining`; it is presentation state, never revision or undo data.

Internal synth voices carry a part scope and note-instance ID. Releasing a note or stopping a part releases only its held instances, including when multiple parts or overlapping notes use the same pitch on one synth. Relaunch releases the previous instances before restarting, and transport Stop explicitly resets event cursors even at beat zero. While transport runs, launch/stop/relaunch waits for the next integer engine beat; a later command for the same part replaces its pending command. While paused or stopped, commands apply immediately. Structured parts begin with transport, while conducted/freeform parts start idle. Transport Stop cancels pending cues and returns independent parts to idle. Pause saves the held note instances in prepared storage and releases their voices. Resume retriggers only those instances whose original release time is still ahead; it does not replay previous score attacks. A part stop/relaunch or transport Stop clears suspended notes. Queued cues remain attached to engine beats across pause. Internal oscillator phase and release-envelope continuity are not preserved across pause.

MIDI 1.0 on each part’s selected channel and the OSC pitch/velocity output contract do not carry note-instance identity. The external worker therefore merges overlapping instances of the same MIDI port/channel/pitch or OSC destination/address/pitch into one held note: first note-on, last note-off. Overlapping attacks are not independently retriggered on those external routes. Panic clears held pitches and sends OSC note-offs as well as MIDI all-notes-off. The bounded external queue can still drop events under overload; delivery and recovery under saturation are not verified.

The WebRTC adapter encodes the selected master mix or dedicated monitor output as 10 ms stereo Opus packets. Projects can contain up to 32 monitor outputs. Dedicated outputs are not summed into hardware speakers; mono duplicates to stereo and wider bundles explicitly select their first two channels. Monitor PCM collection occurs outside Engine::render on the orchestration worker. A project member selects `monitor_node` when offering SDP; the server validates the node. Removal of a selected output yields silence, never fallback to the master. Feed changes currently require reconnecting. Optional microphone uplinks decode into bounded browser-input queues. Codec/network work occurs outside device callbacks. Native and browser sources are separately identifiable nodes. There is no external STUN/TURN dependency. Audio admission reserves one of 32 slots before negotiation and keeps it through peer cleanup; one project/user may have only one pending or live session. Duplicate offers return 409 and require explicit disconnect. Cancellation marks pending offers and final registration rechecks cancellation and active-show ownership. Peer closure happens outside the registry lock; reservation identity prevents delayed cleanup from removing a replacement.

## Graph contracts

Knobs and Sliders contain 1–8 MIDI CC controls. Each control has a channel and CC number, a numeric outlet, and (for Sliders) a matching numeric input. Received MIDI is forwarded unchanged; GUI gestures emit assigned CCs. Slider values apply range, step and decimal rounding in DSP. Connected slider inputs are read-only in the UI and controller API. Live values and learn state use fixed prepared arrays; telemetry serialization occurs outside rendering. MIDI Learn captures the next incoming CC and the initiating browser saves its channel/number as configuration. GUI values are transient; learned assignments and display options are project data.

Local audio input starts the shared WebRTC/Opus microphone session automatically while the engine is enabled, selecting the user's first assigned input or the owner's first unassigned input. Microphone permission is still required. One input uplink is available per project/user session; a node accepts only one sender. The on-node microphone button controls the `mute` parameter (default 0); a positive connected value silences every channel in DSP and locks manual editing. Muting retains capture and the peer connection. Device and machine-name choices live locally in the modal. Tauri supplies its hostname; browsers use an editable local device name because browser APIs do not expose hostnames. `GET projects/:id/media` exposes transient, project-authorized source identity and connection state to all members; the graph and Monitor display user/machine names and sampled per-channel output levels. Metadata is removed when the sender lease ends and never creates a project revision.

- Node IDs and edge IDs are unique. Unknown node kinds/ports/parameters are rejected.
- Widths are 1–8 channels; audio edges require matching per-port widths. Ports normally inherit node width; catalog `fixed_channels` overrides it for mono split outputs and merge inputs. Split/merge ports beyond the active bundle width are silent/ignored.
- Control inputs, parameters and MIDI inputs accept multiple sources; audio and spectral inputs accept one source. See the control-routing and MIDI-source rules below.
- Numeric parameters enforce finite values and declared limits. Structural parameters cannot be driven.
- Feedback loops are accepted. The scheduler orders nodes topologically and, when a loop remains, starts it at the loop's node drawn furthest left (then highest); the connections still feeding that node from inside the loop become feedback edges, which the engine reads one sample late (the standard single-sample feedback delay). Only audio and control wires may close a loop; MIDI and spectral cycles are rejected. Feedback edges skip latency compensation, are reported in telemetry as `feedback_edges`, and are drawn dashed with a return arrow. A loop whose gain exceeds unity grows until the master clamp; keep loop gain below 0 dB.
- Graph limit: 256 nodes and 2,048 connections. Part limit: 32, with at most 10,000 notes per part.
- Streaming spectral branches carry matching transform size/overlap/channel configurations; incompatible connections are rejected. FFT latency is propagated and shorter parallel audio paths are delayed before a merge.
- A control parameter connected to a node is read-only through the API and UI.
- Control numeric nodes currently recompute each sample; rising-edge nodes detect transitions. This is not an emulation of Pd's full hot/cold-inlet message semantics.

## Persistence and collaboration

SQLite uses WAL and foreign keys. Tables hold users, expiring sessions, projects, memberships, invitation tokens, and revision snapshots. Each project update checks an expected change counter (`Project.revision`) and atomically replaces its working copy. Saved revision history is separate and coalesces edits into a snapshot once per minute or on manual save. Stale edits return HTTP 409. Project updates broadcast to authorized WebSocket subscribers. Each connection subscribes before loading and sending the current project snapshot; broadcast-queue overflow also sends a fresh snapshot. Browsers ignore older revisions and stale socket callbacks, preserving the current stage view and part selection through reconnects.

Current collaboration is optimistic revision-based; fine-grained edit leases and separate published performance revisions remain pending. Owners can add an existing enabled account directly or create a single-use invitation valid for seven days. Removing a non-owner membership atomically clears that user's part assignments, advances the project working revision when assignments changed, and closes that user's project monitor. User avatars are stored in SQLite and server-normalized to center-cropped 256×256 PNGs; users can currently change only their own avatar. The first account bootstraps without an invite; subsequent account registration requires an unused invitation. Tokens are random and passwords use Argon2. Session cookies are HttpOnly/SameSite=Strict and Secure when native HTTPS is enabled. Mutating API requests require `X-Pr0former: 1`; WebSocket requests require a same-origin Origin.

## HTTP and realtime contracts

All paths below are under `/api`:

| Endpoint | Purpose |
|---|---|
| `GET status`, `GET catalog` | Bootstrap/server status and node descriptors |
| `POST register/login/logout`, `GET/PUT me` | Local authentication |
| `GET/POST projects` | List/create authorized projects |
| `GET/PUT projects/:id` | Load/update with revision check |
| `PUT projects/:id/parameter` | Validated `{node, parameter, value, revision}` mutation |
| `POST projects/:id/transport` | Activate/deactivate/play/pause/stop/tempo |
| `POST projects/:id/clip` | Launch/stop an authorized part |
| `POST projects/:id/cue` | Arm, launch, stop, repeat, or override dynamics for conducted parts |
| `POST projects/:id/invite`, `POST join` | Invitation creation/redemption |
| `GET/POST projects/:id/members`, `DELETE projects/:id/members/:user` | Browse, add and remove project membership |
| `GET projects/:id/members/candidates`, `GET/PUT users/:id/avatar` | Existing-user selection and normalized profile avatars |
| `GET/POST projects/:id/samples` | List project samples / import audio through FFmpeg |
| `GET devices`, `POST projects/:id/audio` | Native device discovery and input/output enablement |
| `GET/POST/DELETE projects/:id/media` | Connected input identities, WebRTC SDP negotiation and disconnect |
| `WS projects/:id/events` | Project revisions, timing, and telemetry |

Telemetry carries project ID/revision, session epoch, sequence number, monotonic server timestamp, sample/beat/BPM/running state, device diagnostics, per-part playing/start/local-position/pending-cue state, and effective per-node parameter values. It is emitted at 20 Hz. Browser offset estimation uses the smallest observed ping round trip; the animation interpolates from the latest engine timestamp. Telemetry older than 500 ms is marked stale. A separate `engine_status` message publishes `active_project` (the show) and `graph_project` (the loaded development graph), including connection/recovery snapshots. Its monotonic server timestamp is sampled while holding the active-project lock, so browsers can reject queued statuses older than their initial snapshot. The client clears graph telemetry and disconnects its monitor when its graph is unloaded, disabled or assigned to another project; show deactivation alone keeps graph monitoring connected. This ownership status does not acknowledge physical device completion.

Modal history is kept locally for 100 observations (five seconds). Graph layout data does not contain telemetry. Individual nodes consume the shared shallow telemetry reference rather than recreating the entire graph on each packet. Detailed server-side telemetry subscriptions and drift regression remain pending.

The browser types in `web/src/types.ts` mirror the Rust model manually; automatic generation remains pending. Schema version 1 is validated on import.

## Startup

The server and startup-generation prompt default to HTTP `0.0.0.0:80` or HTTPS `0.0.0.0:443` (all IPv4 interfaces). `PR0_BIND` or a saved startup script can select a different address. `./init.sh --start` runs the release server in the foreground, using the saved startup script when present.

`--uas` pulls the current branch from its configured upstream with `git pull --ff-only`, disabling automatic rebase/stash. It invokes the newly pulled script's `--update` followed by `--start`, preserving host/port overrides and the existing saved-script/environment behavior. Any pull/build failure prevents launch; it does not stop existing processes or configure startup services. Its server process uses the usual manual PID tracking.

`--start --host HOST --port PORT` exports `PR0_HOST`/`PR0_PORT` component overrides, applied by the server after reading `PR0_BIND`. Either component can be omitted to retain the saved/environment value. IPv6 hosts are bracketed when forming the address; TLS also resolves hostnames before binding. Overrides do not rewrite saved startup settings.

Manual `--start` launches record their PID, owner, and process start time under `.local/manual-runs`. `--stop` checks that identity before sending SIGTERM and waits up to ten seconds per process. Stale records are discarded. Only launches made with tracking enabled are covered; startup services remain managed by `--startup`.

`init.sh` builds the application and can generate `.local/start-pr0former.sh`. On macOS it installs `~/Library/LaunchAgents/org.pr0former.server.plist`; on Linux it installs `~/.config/systemd/user/pr0former.service`. The startup script sets absolute paths and optional TLS variables. Startup is per-user at login, not system-wide at boot. Disabling it does not delete project data.

## Score editor and scheduling

Write-tool clicks choose an insertion point relative to note onsets in the selected measure/staff/voice. Entry starts at the measure boundary or immediately after the preceding note group, avoiding implied leading rests from pointer coordinates. Later chord groups shift right only as needed, consuming existing gaps. The insertion is planned before committing, and any resulting measure overflow rejects the complete edit without shortening the entered duration. Explicit chord entry remains a separate operation. Clicking after a full measure's final note rejects overflow; keyboard entry at the next measure remains available.

`pr0-core::score` owns the optional version-1 score timeline, staff configuration, written note metadata, dynamics, and MIDI lanes. Existing schema-1 projects remain readable: absent timeline means legacy independent part loops; absent staff/notation data receives a presentation default without rewriting stored notes. Converting to a shared score is explicit. Authored note metadata records rational onset/duration (denominator bounded to 1,000,000,000), diatonic spelling, accidental, staff, voice, base value, dots and tuplet ratio alongside compatible numeric playback fields. The server checks agreement, reference integrity, staff routing, MIDI ranges and rhythmic overlaps. Four voices and eight staves are supported per part; different onsets that overlap need separate voices. Equal-onset chords are allowed. Legacy unnotated overlap is retained.

`ScoreWorkspace` renders selected parts in one continuous horizontal viewport, with a collapsible part list. `ScoreStaff` uses local VexFlow SVG glyphs, measures, automatic display rests, chords, ties, tuplets, grace notes, articulations and octave lines. Beat anchors are shared across parts, signatures, automation and playheads; minimum spacing and signature reservations expand dense sections without changing musical time. Horizontal overscan and vertical visibility reduce engraving work. This is scrolling notation, without pagination or a full collision-optimized engraving engine. The piano roll uses a uniform beat grid with drag-to-draw, move and resize gestures; overlapping entries choose available notation voices. A floating palette, modal settings and collapsible MIDI inspectors keep the viewer dominant. Semantic SVG targets distinguish marks from note hit boxes. Staff hidden-rest ranges are bounded and server-validated display metadata, ignored by playback; editing a generated rest materializes a native note. Browser following is presentation only.

Write and Select modes distinguish entry from selection; keyboard edits, Ctrl/Cmd toggle selection, marquee, drag, copy/paste, deletion and local undo/redo persist through revision-checked project saves. Rapid keyboard edits apply immediately to the project draft; requests are serialized by the save coordinator. A failed validation reports the server error; stale/network failures retain a draft for explicit reapplication or discard. Undo restores score fields against the latest project, preserving graph edits. Remote score changes clear local history. Preparation permits editing during playback; the conductor’s performance lock rejects project saves and player stage views remain read-only. Performance view defaults to only the current user's assigned parts; Show all is reset on entry and places assigned parts first without changing stored order, scheduling, or permissions.

The frontend `scoreBars` module applies a time splice to every part for bar insertion/deletion, updating rational note times, clipped/split automation, staff changes, hidden rests and shared navigation in one server-validated project revision. Appending completes a partial final bar before adding whole blank bars. The dedicated structure dialog also exposes beat-positioned meter and clef edits.

The timeline also carries an optional tempo map (`tempos`: written beat → quarter-note BPM, ordered, 1–400) and each staff an optional list of text marks (`marks`: id, beat, kind ∈ text/rehearsal/cue/expression/tempo/lyric, ≤256 bytes). The sequencer applies tempo entries edge-triggered as the shared written position crosses them, re-applies the tempo in force on start, resume or rewind, and leaves manual tempo commands in force until the next entry; loading a project uses a beat-0 entry, else `Project.bpm`, for the clock and count-in. Staves also carry phrasing curves (`curves`: id, kind ∈ slur/bracket/crescendo/decrescendo, optional start/end note anchors, start/end beats, height and independent start/end lifts within ±200 staff pixels); anchors must reference notes of the part. Marks and curves are engraved metadata; new hairpins also author an owned `hairpin:<curve-id>` dynamics event, which drives playback. Endpoint/whole-wedge edits update that event, while deleting a wedge removes its owned event and restores an authored starting mark retained in optional `start_dynamic` metadata. Existing wedges without that identity remain independent notation. ties remain `tie_to` note links because they merge playback durations. Timeline meter/key changes preserve quarter-beat note positions. Meter numerators are 1–16 and denominators 1/2/4/8/16/32. Staff clefs are treble/bass/alto/tenor, with timed changes, static key overrides, major/minor labels, sounding transposition and optional instrument/MIDI route overrides. Initial meter overrides also govern count-in. Staff routing validates local instrument references, channels 1–16 and bounded port names. Part OSC routes retain their existing numeric-address/path contracts.

`Sequencer::new` prepares bounded traversal spans, ties, grace timing, articulation gates and velocities outside rendering. Ordered disjoint repeats support 2–32 passes and a first ending skipped on the final pass. One D.C./D.S. jump may finish at Fine or use one coda pair; repeats are not replayed after that jump. Nested repeats are rejected. Structured scores traverse together and stop at their end unless whole-score looping is selected; Play after the end restarts. Conducted/freeform launches are one-shot by default, using score spans clipped to their own part length; only an explicit conducted Play + Repeat cue forces another pass. Telemetry exposes written position and its current span endpoint so browser extrapolation cannot run past a repeat boundary. Notes outside a shortened independent part remain stored.

Conducted projects persist one designated conductor and an ordered layout of at most 64 named sets. A conductor cannot also own a performer part. Set and tile order, membership, count-in pulse count, and the independent pulse denominator are server validated. The conductor editor uses pointer reorder plus measured FLIP animation (disabled for reduced motion). The conductor stage is a locked touch-first surface: single-part and armed-group cues are assigned request IDs and quantized by the sequencer to the next global pulse. A performer entering from rest receives the configured count-in before music begins. If that performer already has an active part, another cue enters a fixed-capacity successor queue with an explicit engine-beat start, and the repeating part exits at its next pass boundary. Only one part for a performer can sound at once. A Monitor output may associate with any part; cue clicks go only to feeds associated with the counting performer, while shared transport/metronome clicks retain their all-monitor behavior. Cue state, scheduled start, queue position, progress, count-in, repeat state, arming, and live dynamic overrides are telemetry, not revision or undo data.

Each conducted part can define ordered notated meter changes; each segment's denominator beat is scaled to the global pulse without changing the engine's quarter-note clock. The prepared piecewise map schedules notes and maps engine telemetry back to local written position. The performer view composes retained, active, repeated, and scheduled part segments into one synthetic scrolling score; engine-time gaps remain engraved rests and queued notation appears with reduced-motion-aware animation before its start. Live dynamic commands target explicit parts or the armed/playing selection, emit CC11 on their routed staff/external MIDI paths, and override subsequent note velocities until Return to score is requested. The performer meter follows written dynamics when there is no override and glows violet while the conductor owns the value.

Web MIDI is permissioned only after an explicit conductor or performer action. Learned conductor note/CC controls are stored in that browser and send semantic, server-quantized cue requests. A performer browser announces its inputs and sends three-byte channel messages over the authenticated project WebSocket to its currently selected assigned part. The server rejects structured/inactive/unassigned targets, SysEx/system messages and invalid data, and limits each connection to 2,048 MIDI messages per second before forwarding to the bounded orchestration/Part MIDI path. Stage exit, socket disconnect and show reset release browser-held notes and emit external all-notes-off. This is not a hardware-timed path; physical Web MIDI devices, reconnect behavior and iPad/Safari remain manual/unverified.

Dynamics can scale attack velocity, produce a configurable MIDI CC (default 11), or do both. They are stored per staff (`Staff.dynamics`); a staff without its own dynamics falls back to the legacy part-level `dynamics`, and each staff's generated CC lane uses the staff MIDI channel. Raw lanes cover notes, CC, 14-bit bend, program change, channel pressure and poly pressure. Curves are step, linear, ease-in, ease-out and S-curve. One lane per channel/message/controller target avoids ambiguous raw writers; explicit raw events override generated dynamics on their event intervals. Continuous values emit at a bounded 100 Hz plus exact event boundaries, suppressing unchanged quantized values. Note on/off dispatch and all evaluation use engine sample/beat time. Pause/stop releases active automation notes and damper/sostenuto/hold-2 pedal controllers; resume evaluates the current written position. No browser timer emits score MIDI.

`Signal::Midi` is separate from audio, scalar control and spectral contracts. The raw message path is the source of truth: producers (`part_midi`, `midi_input`, `osc_to_midi`, `piano`) only ever receive raw channel messages, and each derives its five scalar note outlets from the same per-sample frame that its typed `midi` outlet forwards. Several cables into one `midi` input concatenate in connection order within the sample and share that node's 256-event frame. Consumers release decoded notes on CC 120/123 from any channel, including the all-notes-off messages injected on overflow. Part MIDI keeps an optional staff filter. Every future instrument node that receives MIDI must decode the raw frame rather than adding a second entry point. MIDI boundaries carry ordered channel messages through subgraphs; `midi_output.midi` reaches the bounded device worker. `midi_to_control` filters and exposes the last matching event in each sample, including full 14-bit bend values; it is not a polyphonic scalar event serializer. Prepared per-node buffers hold 256 events; overflow records telemetry and replaces queued messages with channel all-notes-off messages. MIDI render dispatch uses prepared fixed storage. Physical output remains best effort through the external worker and is not a verified device deadline path.

MusicXML supports uncompressed score-partwise documents with multiple parts/staves, spelling, voices/chords, dots, ties, tuplets, grace notes, articulations, octave shifts, clefs/transposition, shared meter/key changes, repeats/endings, navigation and written dynamics. Import rejects conflicting meters and unsupported structures or reports losses; export reports omitted MIDI mappings/lanes and timing rounded to its 20,160 divisions per quarter beat. Native project JSON retains graph routing and exact authored fractions. See [SCORE_EDITOR.md](SCORE_EDITOR.md) for controls and interchange limits.

## System audio and diagnostics

The header gear opens project settings, a system settings dialog with a left-side System audio tab, and a project console. Server-wide sample rate, DSP block size, enabled output interfaces, and manual latency values live in `PR0_DATA/audio-settings.json`. Project owners may change system settings while all shows are inactive. A setup mutex serializes conversion, activation, and graph preparation. Settings changes disable the engine; enabling it or activating a show opens every selected output and input and waits for the device-start result before accepting activation. With no outputs selected, the engine intentionally operates for browser monitoring. Native inputs are selected in `input_interfaces` and start automatically with the audio engine. Startup fails with a device error if a selected input cannot open; no inputs selected is valid. Each checked input opens its own f32 stream and bounded input queue at the global rate. Nodes route by structural `interface` ID; zero chooses the first enabled input in saved order. Each queue is consumed once per engine sample and its frame is shared with matching nodes. Capture stops when the engine stops, on deactivation, or on stream failure. Up to 64 physical channels are captured; graph nodes select up to eight of them through their channel routing tables. Device-name-derived IDs have the same identical-name limitation as output IDs. No independent input clock drift correction is implemented.

Output nodes use the structural numeric `interface` parameter: zero routes to all enabled interfaces; other IDs select one checked interface. IDs are derived from device names; identical device names cannot currently be distinguished. Server validation rejects new unselected/unknown route IDs and refuses activation while any route is unavailable. Unchanged unavailable routes in dormant projects are retained so users can repair one node at a time. Each device has its own bounded queue. The first selected output provides pacing; independent device clocks can drift, so synchronized physical multidevice use remains unverified. When correction is checked, faster interfaces receive a prepared delay equal to the largest checked latency minus their entered latency. The manual 120 BPM test emits a short tone and broadcasts engine-sample position for a bouncing browser reference, stopping after 60 seconds even if the browser disappears. This is not an automatic physical loopback measurement and does not eliminate browser/network display latency.

Uploaded WAV originals remain `<asset>.wav`. Windowed-sinc conversion creates versioned `<asset>-<rate>-v1.wav` float caches via a temporary file and atomic rename. Upload prepares the current-rate cache; settings changes prepare every imported clip in the current project; opening another project prepares its missing caches. Preparation uses blocking-task workers, outside render and device callbacks. Loading, activation, and conversion share an indeterminate task modal. Cached files and originals are retained without automatic eviction. WebRTC remains 48 kHz Opus; stateful rate adapters preserve fractional phase and filter history across packets when the engine uses another rate.

The authenticated project console polls bounded server history (2,000 entries across projects), with autoscroll and a standalone same-origin tab. It records project preparation/revisions, engine and transport commands, parameter/cue changes, device errors/underruns, sample conversion, and browser audio negotiation/disconnection. It is an in-memory activity log, not a persistent process stdout/stderr archive. No logs or waveform data enter project revisions or undo.

Audio-connection hover or keyboard focus requests 256 consecutive source samples per channel through the authorized preview endpoint. Requests only run while a preview is open; at most 32 captures are pending, with a two-second HTTP timeout. Capture storage is allocated outside `Engine::render`; samples are read after rendering and serialized outside it. Previews show source-side audio before downstream compensation, with per-channel automatic display scaling and numeric peaks. They are short snapshots, not calibrated oscilloscopes.

The oscillator defaults to sine and also supports triangle, sawtooth, square, and noise. Sawtooth/square use PolyBLEP edge correction; triangle is the basic piecewise waveform. Manual frequency edits retain 5 ms smoothing. A connected Frequency parameter bypasses this smoothing and follows each engine sample for FM; phase remains continuous across frequency changes. The numeric frequency contract remains 0–20000 Hz. Stored waveform selections are validated integers 0–4; connected waveform controls are rounded to an index. A sine source can still sound distorted after modulation, summed-output clipping, downstream processing, or playback/device problems; source waveform tests do not establish physical listening quality.


## Pass-through visualizers

`control_visualizer`, `audio_visualizer`, and `spectral_visualizer` have distinct typed input/output contracts and introduce no signal delay. A graph may contain at most 16 visualizers. The control visualizer forwards numeric values or UTF-8 strings (up to 256 bytes) unchanged. Its optional `control_value` is a configured fallback literal used only while disconnected and edited in the modal. Runtime text uses fixed-capacity copied storage; no String allocation or destruction occurs in render. Graph validation infers the type through visualizer chains and rejects text into numeric-only processors or parameters. Connected fallback values are read-only. This does not implement general text message operators or external text input routing.

The audio visualizer captures a prepared rolling analysis window in render and computes a Hann-window FFT at the 20 Hz telemetry cadence only while viewers are subscribed. The spectral visualizer forwards Cartesian/polar bins and generation unchanged, including phase, without adding FFT latency. Its display analysis reads the latest incoming frame at telemetry cadence; frame generation remains distinct from display analysis sequence. Each channel retains 128 displayed columns of 32 linear frequency bands, with 8-bit intensity over −100 to 0 dB for the compact spectrogram. The transmitted history is hex-encoded; spectral bin/phase plots carry every positive-frequency bin without modifying the original frames. Graph/modal viewers explicitly subscribe; stage clients do not receive these payloads. The worker permits 32 diagnostic subscriptions, renewed by socket pings and expired after ten seconds. Analysis and snapshot construction stop without subscribers, while rolling capture and audio processing continue. The visible history represents up to 6.4 seconds of subscribed analysis, with hidden intervals omitted. Graph nodes and modals render all 1–8 channels with stale-state marking. No visualization history is persisted or recorded in undo.

Analysis size (256–8192 samples) and spectral overlap (2 or 4) are structural modal settings. Diagnostic FFT sizes do not change the configured render block size. Visualizer analysis, serialization, and browser load have not passed the 32-player/endurance acceptance run; prior load baselines did not include visualizers.


## Live editing and keyboard input

Right-click a node to open Edit/Delete; native keyboard context-menu invocation works on focused nodes. Edit opens the parameter modal, including read-only effective values for connected parameters. Delete removes attached connections and clears instrument references to the removed node. Backspace/Delete removes selected nodes and connections in the graph. Ctrl/Cmd+Z restores the previous saved revision's graph, including structure, through server validation and preparation. Failed full-project saves do not create undo entries. Pending parameter gestures prevent concurrent structural writes.

Space activates an inactive show and then plays it; once active it toggles play/pause. The Play button uses the same sequence. The adjacent Play + Repeat button applies a runtime-only repeat override: structured autoplay lanes loop their prepared traversal, and a non-structured score-editor preview relaunches the focused part with its existing repeat override. Stop clears the temporary override without changing the saved whole-score loop setting. Transport commands remain limited to owners/conductors, held-key repeats are ignored, and activation failure prevents Play. Shortcuts do not consume input in dialogs, editable fields, links, or buttons. VueFlow's Space-to-pan binding is disabled to avoid competing transport actions. Native button keyboard activation remains available.

## FM and spectral processing

`audio_to_control` converts the first audio channel into a numeric control on every sample: `out = input * scale + offset`. Channel widths remain validated on its audio inlet. Other channels can be selected upstream with Channel map. This is not a block snapshot, meter, or browser timer. An oscillator at amplitude 1 feeding Scale 100 / Offset 440 produces 340–540 Hz at a connected carrier Frequency. Inputs beyond the carrier's 0–20000 Hz range clamp there; through-zero FM and oversampling are not implemented.

`spectral_math` applies magnitude multiply/add and phase multiply/add once per fresh spectral frame, with nonnegative magnitude and wrapped phase. `spectral_curve` linearly interpolates two sets of 33 points across DC–Nyquist: magnitude multipliers (0–2) and phase offsets (−π…π). The modal supports pointer/touch drawing and keyboard point editing; releasing a stroke submits one structural graph edit and one undo entry. Curve points are bounded structural parameters, server-validated and persisted with the project. No per-frame curve construction occurs in render.

Both processors accept and retain Cartesian/polar representation, FFT size, overlap, generation, and channels without adding framing latency. They operate on the positive-frequency half and mirror negative frequencies for real audio; DC/Nyquist retain their original phase. Phase operations therefore have no effect on DC/Nyquist. Math magnitudes use raw FFT units, not normalized dB. Curves apply to every channel identically. Recompiled curve settings take effect after preparation and fresh frame delivery, with no crossfade; physical latency and heavy spectral-editing load are unverified.


## Nested subgraphs and tempo input

Nodes optionally store a `parent` subgraph node ID; absent/null means root. All nodes and edges remain in flat project arrays with globally unique IDs, so JSON depth and recursive processing do not grow with graph nesting. There is no separate nesting-depth limit; the existing total budget of 256 nodes (including containers/boundaries) and 2,048 edges still applies. The editor shows one parent scope and breadcrumb navigation, while device routing, sample preparation, performer assignments, and telemetry retain global node IDs.

Six `subgraph_input_*` / `subgraph_output_*` node kinds expose named control, audio, or spectral ports. A parent port ID is its boundary node ID, so renaming its label preserves connections. Channel widths and FFT size/overlap belong to each boundary, allowing different formats on different ports. Authoring validation rejects missing/non-container parents, containment cycles, root boundary nodes, cross-scope cables and reversed boundary directions. Preparation redirects parent ports to boundary processors and validates the flattened topology, including cycles, duplicate drivers, audio widths, spectral contracts, and numeric/string controls. Boundary processors pass through samples or frames without additional latency, runtime recursion, allocation, or locking. Audio edge previews resolve through the same flattening. Deletion removes descendants and attached parent-port edges; duplication generates new IDs for the whole subtree, including internal connections and port references, while preserving stored node settings. External cables are not duplicated.

The Global clock `tempo` control input sets engine BPM at sample time, clamped to 1–400, before advancing the clock; beat/sample phase is preserved. Only one clock tempo input can be connected across the entire flattened project. With a driver, transport tempo is read-only and manual API tempo edits are rejected. Disconnecting retains the last engine BPM for the current activation. Telemetry is not persisted as project tempo. Scheduling reads engine beat positions; browser interpolation remains presentation only.

Local audio input device discovery uses `enumerateDevices`, refresh/device-change notifications, and an explicit microphone-permission button that stops its temporary stream immediately. Device IDs and per-node selections stay in local browser session state. Monitor capture uses the selected exact device ID, avoiding silent fallback if it disappears. Inputs are locked while capture/connection is in use; disconnect to change the selection. UI messages distinguish unsupported/insecure contexts, permission or policy denial, unavailable devices, and native input enablement/capture state. Physical device paths remain manual validation.


## Versioned subgraph library

`subgraph_library` stores owner and permanent public status; `subgraph_versions` stores immutable graph snapshots and per-version publication state. Authenticated users see their own versions and public versions of others. Saving to an existing entry requires its owner and creates the next version; a public entry cannot be made private. Publishing a new version does not expose older private drafts. A new library ID permits a private fork. There is no version overwrite or unpublish endpoint.

The library APIs are `GET /api/subgraphs`, `GET /api/subgraphs/:id/versions/:version`, `POST /api/projects/:id/subgraphs` (save), and `POST /api/projects/:id/subgraphs/insert`. Project editing rights and revisions are checked for save/insert, library visibility is checked server-side, and existing-entry saves check ownership. Versions bundle original WAV bytes in `subgraph_assets`; insertion creates new project-local asset IDs and prepares caches through the normal project update path. Missing originals fail saving instead of creating a broken library version.

Insertion generates new IDs for every node/edge and rewrites internal port references, preserving the chosen version as optional `library: { id, version }` provenance on its root. The graph snapshot is embedded in the project, so playback, local editing, exports, and existing dependencies never follow a mutable latest-version pointer. Nested library references are provenance too; all nested graph contents and samples are snapshotted. Saving an edited imported subgraph creates a new version for its library owner or a new entry for another user.

The subgraph library sits below the node library and hides with it. Both support click-to-add and HTML drag/drop mapped through the canvas transform. Empty-canvas dragging selects a rectangle; Control-click (including macOS's native context-click behavior) and Command-click toggle individual selections. Multi-node drags persist all moved positions in one project revision, and deletion/undo apply to the selected subtrees. Right/middle drag pans the canvas; Space remains transport control.


## Graphical controls and editor grouping

`control_input` provides Bang, Integer, Float, Slider, and Text widgets in the graph. Type and numeric minimum/maximum are structural modal parameters. The manual literal is stored in `control_value`; Integer requires whole-number limits and values. Connected input passes through unchanged and shows read-only engine telemetry, including incoming text independent of the widget mode. Disconnected Text defaults to an empty string. Strings retain the 256-byte UTF-8 contract.

`PUT /api/projects/:id/control` validates role, revision, type/range, and flattened connectivity. Manual value changes save a revision and send a small prepared-value command to the orchestration worker without recompiling the graph. Bang requires an active show, emits a single engine-sample pulse, and is not persisted. Telemetry remains outside revisions and undo history. Browser edits queue the latest pending value during a save.

D duplicates the whole selection and its internal wires, including nested subgraphs. A selects every node in the current graph. Right-clicking multiple selected nodes offers Auto-space, Make subgraph, Duplicate, and Delete. Auto-space (L) lays the selection out in signal-flow columns: ranks follow the longest path through the selection's own connections, each column is ordered to reduce wire crossings with port-aware weights and a pairwise transpose pass, rows are pulled toward the ports they connect so multi-input nodes get straight wires, and the selection keeps its original top-left corner. It is one undoable save; nodes outside the selection do not move. The ADSR node draws its envelope on the canvas (attack, decay, a sustain plateau and release on an orange grid, with the live output level as a dotted line); its modal shows a larger copy with draggable handles for attack, decay/sustain and release/sustain that emit ordinary parameter edits, with keyboard nudging and locked handles for connected parameters. The time axis is a stepped scale (50 ms doubling to 51.2 s, auto-chosen so the timed stages fill at most 80% of it) that freezes during a drag, so the release ends at the right edge and its handle, the start of the release ramp, lengthens the release when dragged left. The editable graph has a Time span slider that overrides the auto scale (remembered for the session, Auto restores it); zoomed in past the release, the curve is clipped at the frame with an overflow label, the plateau keeps an 8% minimum width and the release handle edits only sustain. The ADSR node itself is edge triggered: a rising `gate` or `retrigger` starts or restarts the attack, and a falling `gate` or rising `note_off` releases, even when the gate parameter is left high, so pulse sources such as Piano wire trigger→retrigger and note_off→note_off while held sources wire gate→gate. Attacks ramp from the current level unless the structural `reset` option (Retrigger from zero) is on, in which case every attack drops to 0 first. The Meter node is a pass-through whose runtime keeps one peak follower per channel (instant rise, exponential fall over its fall-time parameter) in its control outputs `level_1`…`level_8`; telemetry publishes them as `_level1`…`_levelN` for the node's thin per-channel dBFS strips, which reuse the monitor tab's scale and colour zones, and the descriptor only exposes as many Level outputs as the node has channels (narrowing the node drops cables from the removed outputs). The Drum Sampler library subgraph mirrors the FM Drum Machine with six one-shot `poly_sampler` voices preloaded with a bundled CC0 acoustic kit (see [SAMPLE_CREDITS.md](SAMPLE_CREDITS.md)). The server seeds that kit into the shared sample library as global samples on every start, so all accounts can browse and add them; inserting the subgraph links the project to those global samples (falling back to embedded copies if an administrator removed them), and each voice's sample can be swapped from its modal. Sample tags are the comma-separated `tags` field parsed into trimmed, case-insensitively unique chips; the project sample list, the full sample browser and the organizer show each sample's tags as chips and a usage-counted tag bar, clicking a chip filters (every selected tag must match), and the metadata editor replaces the tags text box with a chip editor plus a tag cloud of every tag the user can see, sized by use, where clicking adds or removes the tag. Grouping creates typed, named boundaries for crossing connections and preserves existing outer boundary connections through proxies when selected boundary nodes move inward. The server validates the resulting graph through the normal revision endpoint. Node and subgraph libraries share a single-open accordion; the open library fills the sidebar. Explanatory notes across modals, settings and dialogs are `HelpNote` components: with help shown they render as ordinary notes, and with help hidden (the `i` key anywhere outside a text field, or the topbar info button, persisted per browser) each collapses to a small info icon whose text appears in a body-teleported tooltip on hover or focus. Status messages and empty states stay visible in both modes. The ADSR graph's zoom is sticky: dragging or nudging a handle never rescales the graph, and it refits only when the parameters change from outside the graph.

## Compiled version identity

The server build script embeds Git HEAD, worktree dirty state, and build time. Git metadata and tracked source changes invalidate the stamp. Direct startup, `--version`, and `/api/status` expose it. Manual `init.sh --start` additionally compares the binary with the checkout and tracked remote tip, using a bounded read-only remote query. Missing upstream, unavailable network, dirty builds, or outdated binaries produce red warnings; launch continues. Rebuild after committing to stamp a clean revision. Startup services print the binary identity but do not run the manual launcher's remote check.


## Native device channel routing

Native `input` and `output` nodes have eight structural `route_1`…`route_8` parameters, one for each possible graph signal channel. A route of 1–64 selects a one-based physical device channel; 0 disconnects it. The default −1 preserves sequential routing, including the input node’s existing zero-based `offset`. Explicit input routes bypass that offset. Server graph validation rejects fractional and out-of-range routes and prevents control connections to these structural parameters. Routes above the node width remain stored but unused.

The modal displays one row per signal channel and the selected interfaces’ channel counts at the configured sample rate. Each input row selects one physical source; sources can be duplicated. Each output row selects one physical destination; repeated destinations sum after Output gain. None produces input silence or ignores that output signal channel. The stereo preset routes odd/even channels to physical outputs 1/2 and sets gain to −20 log10(ceil(width/2)) dB to leave headroom. Manual edits retain gain. Presets apply together in one working-copy update and undo entry. This is per-channel routing, not a weighted crosspoint matrix with independent gains.

Native rings, callback frames and output latency queues carry up to 64 physical channels independently of graph buffers, which remain 1–8 channels. Input mapping occurs in DSP processing; hardware output mapping occurs after rendering and before the interface delay/ring. Mapping and callback sample movement use fixed storage without allocation or locks. Missing input channels are silent; destinations beyond an interface’s width are ignored. Route 0 for the interface itself still selects the first enabled input or all enabled outputs, with the same output mapping applied to each device. The browser master and dedicated monitor feeds retain their existing graph-channel contracts and are taken before physical routing.

Discovery and stream startup select the widest supported f32 configuration of 1–64 channels at the configured sample rate. `/api/devices` includes `channels` and an error when no such configuration exists. Disconnected/unavailable device routes remain editable and persist for later use. Live route changes use the existing prepared graph replacement path; they do not add a crossfade. Physical interface verification and load/endurance testing of the larger native buffers remain manual/pending.


## Saved revisions and live working copies

`Project.revision` remains a monotonically increasing edit counter for HTTP optimistic concurrency, WebSocket ordering and telemetry filtering. It is no longer displayed as the saved revision number. Accepted graph, parameter and graphical-control changes update the SQLite working copy immediately for live collaboration and crash recovery; they do not append history rows. Undo remains local editing history and does not itself force a saved revision.

The first dirty edit schedules a server-side save for 60 seconds later in persistent `pending_saves`. Further edits coalesce into that deadline, so continuous gestures produce at most one automatic history snapshot per minute. The worker checks deadlines once a second outside the audio thread; closing the browser does not cancel autosave, and server restart retains pending deadlines. Manual `POST /api/projects/:id/save` saves the latest accepted working copy immediately and clears its pending deadline. A clean save is idempotent. Existing history is preserved without deletion or rewriting. `GET` on the same endpoint reports the saved revision ordinal, saved change counter and dirty state. Editing roles are required to save; project members may read status.

The header shows the saved revision count and Saved/Unsaved changes beneath the project name, with a Save revision button on the right. The redundant upper-right Ensemble button is removed; the Ensemble tab remains. Cmd+S and Ctrl+S work inside node modals and text inputs, commit changed inputs and wait for queued live parameter/control updates before requesting the snapshot. Unapplied structural/dialog drafts still require their own Apply/Save action. Server save events and reconnect snapshots update collaborators' revision indicators without incrementing the edit counter, replacing graph state or disturbing the engine. The underlying working copy remains durable between history saves; the one-minute cadence governs saved revisions rather than audio updates.


## Monitor workspace and process statistics

The Monitor tab uses a scrollable left sidebar for clock snapshot age, server CPU/resident-memory measurements, browser audio controls and device diagnostics. The remaining area has two meter rows grouped by physical device, with one vertical bar per physical channel. Inputs include every discovered channel, independent of graph input nodes; disabled or unavailable capture shows no data. Outputs show only channels routed from connected native Output nodes in the active graph to enabled physical devices, retaining sparse physical channel numbering. Local audio input and dedicated monitor nodes do not appear as hardware meters. Wide groups scroll horizontally.

Global `hardware_levels` events publish per-device/channel sample peaks at 20 Hz on the orchestration worker. Input peaks are collected before graph routing, including channels unused by any graph. Output peaks are collected after summing all graph contributions to each physical channel, before the output delay/ring. Fixed peak arrays observe every worker frame between updates; serialization occurs outside DSP render and device callbacks. Paused graph outputs are silent; enabled capture can continue during pause or with no graph loaded. This measures captured inputs and the active graph's outgoing signal, not OS loopback, other applications, true peaks, or analog hardware levels. Physical device verification remains manual.

Bars span −60 to 0 dBFS: green below −12 dBFS, yellow at −12 dBFS and above, red with a CLIP label at or above 0 dBFS. Stale or unavailable hardware telemetry clears the bar and shows an unavailable readout. Color is accompanied by numeric/accessible level and clipping text. Reduced motion disables bar transitions. The browser monitor component stays mounted across tabs and stage navigation, preserving its WebRTC session.

`GET /api/system/stats` requires authentication and returns server-process CPU percentage, resident bytes and sample time. A separate thread samples once a second; the visible monitor sidebar fetches the cached values every two seconds. CPU uses process user+system CPU-time deltas (100% equals one fully used core; multicore values may exceed 100%). Current resident memory uses Mach task_info on macOS and /proc/self/statm on Linux. Unsupported or failed measurements are null, never fabricated zeroes. OS queries and memory reads never run in audio rendering or device callbacks. Clock sync displays telemetry age, not a verified physical score/audio alignment result.

### Discovered device preferences

Authenticated audio settings/device discovery and engine activation add previously unseen native inputs and outputs to persisted system settings with enablement on. Existing entries, including explicitly disabled and disconnected devices, retain their choices. Discovery alone does not reopen streams; startup applies saved choices. Device checkboxes immediately PUT only direction, ID and enablement to the owner-only project system-audio/device endpoint; unsaved sample-rate/latency drafts are not included. Changes require an inactive show. An unplugged unrelated device does not prevent saving a checkbox choice. Atomic settings writes are serialized by the setup mutex. `PR0_DISABLE_NATIVE_DEVICES=1` disables native discovery/opening in isolated automated tests; those tests do not validate hardware capture or playback.


## Note-control graph routing

`part_midi`, `midi_input` and `osc_to_midi` expose five numeric controls decoded from the node's own raw frame: `pitch` (0–127), `velocity` (0–127), `gate` (any held notes), `trigger` (one-sample note-on) and `note_off` (one-sample release). MIDI input's Note/CC mode and channel filter are applied by the orchestration worker before a message is enqueued, so the typed outlet carries the physical channel byte; OSC-to-MIDI messages use channel 1. Pitch identifies the event, including the released note; it is not a last-held-note selector. A prepared 1,024-event queue serializes chords one event every two engine samples, inserting a low pulse sample. Overflow initiates held-note release and exposes dropped-event telemetry. Connect all five controls for polyphony. A gate-only patch supports monophonic edge triggering.

Part assignment is project-local and server-validated. Scheduling publishes only the selected part's events, including held-note restoration; reassignment releases the old source. Library insertion clears part assignments for selection in the destination project. Existing direct score instrument/external routing remains independent of these graph sources.

`poly_sampler` consumes this contract using 64 preallocated voices, velocity scaling, semitone pitch relative to a modal root note, optional looping and a linear release. Repeated equal pitches release the oldest matching held voice; voice exhaustion steals the oldest voice. WAV loading/resampling is prepared outside rendering. The original `sample` player is unchanged.

`midi_output` and `midi_to_osc` consume the same five controls. A typed cable into `midi_output` is forwarded raw to the device worker and is not decoded, while scalar cables are decoded into note events; the engine keeps the two disjoint so a note is never sent twice. `midi_to_osc` decodes typed note messages only. MIDI input/output retain `number`/`value` aliases and Note/CC mode. MIDI input callbacks enqueue fixed-size bytes into bounded rings; the orchestration worker routes selected ports/channels into prepared node queues. A separate bounded output worker performs MIDI sends and OSC encoding, tracks held pitches per node/route, and releases them when the engine unloads, output wiring changes or routes retire. Graph output preserves repeated attacks; this differs from the existing direct score external worker's merged ownership. Physical MIDI device operation and unplug/reconnect recovery have not been hardware-verified.

The converter wire protocol is an exact configured OSC address with two integer arguments `[pitch, velocity]`, each 0–127; velocity zero means note-off. Enable the system OSC sender/receiver as appropriate. OSC to MIDI produces graph controls, which can feed a sampler or MIDI output. `osc_input` receives one numeric argument (or one text argument in text mode) at its exact address and holds it as its control value; `osc_output` sends its input as one float or string argument to its destination on a trigger edge when `trigger` is connected, otherwise whenever the value changes, coalesced to the node's maximum rate by the orchestration worker. Arbitrary MIDI bytes and OSC bundles are outside this implementation. External delivery is best effort; only engine-side scheduling uses sample time.


## Piano test node

`piano` is a one-octave graphical note source and pass-through with the same five numeric note inputs and outputs (`pitch`, `velocity`, `gate`, `trigger`, `note_off`) plus a typed `midi` inlet and outlet. Key clicks and typed cables arrive as raw channel-one messages; the node decodes them for its key lights and scalar outlets, forwards them on the typed outlet, and republishes notes driven by its scalar inlets so the typed outlet mirrors everything the keyboard shows. Patch a given source to either the five scalar inlets or the typed inlet, never both. Incoming notes are queued through the prepared MIDI control serializer, including pitches outside the visible octave. Engine held-note counts drive per-key telemetry for the displayed range; key lights are presentation state and never enter graph revisions or undo. Very short notes may fall between telemetry frames.

The modal octave selector covers C−1 through C9; pitches above MIDI 127 are disabled. The default is C4–B4 (60–71). Octave changes preserve received held notes and input edge state. Browser pointer/keyboard presses send velocity 100 and release the originally pressed pitch on pointer-up/cancel, blur, view removal, or octave change. Two touches can hold independent notes. Browser requests are serialized per project/node so releases follow attacks. The transient `/piano` endpoint checks CSRF, editor role, loaded graph, node kind and MIDI ranges under the setup lock; it never persists notes. Engine commands are project-scoped and attacks require a loaded, enabled graph. Show Pause/Stop releases only score-owned notes. Abrupt network loss can prevent browser release delivery; deactivate the show and disable the engine to clear held manual notes. This is a manual testing control, not a hardware-timed performance keyboard.

`drum_pads` is the percussion counterpart: six clickable pads (bass drum, snare, tom 1, tom 2, hi-hat, cymbal) with structural per-pad MIDI notes defaulting to General MIDI numbers 36, 38, 45, 50, 42 and 49 on channel 10. Clicks go through the same `/piano` endpoint and become raw note-on/off messages on the node's channel; the typed `midi` outlet forwards them together with any received typed MIDI, pads light for held matching notes, and each pad's control outlet pulses with the note velocity for one engine sample. The built-in FM Drum Machine subgraph decodes the same six notes with `midi_to_control` number filters (its `note_on`/`note_off` outlets drive each voice's trigger and release) and exposes six matching trigger inlets that accept a velocity value (1–127, return to 0 to rearm), so a Drum pads node connects to it either by one MIDI cable or by its six trigger wires. Each voice is a one-shot: `synth` and `fm_synth` have a `decay` parameter (0 holds until release) that fades every voice exponentially from its attack, so a held pad or a missing note-off never leaves a drum ringing, and decayed voices free themselves.


## Development engine and show ownership

The server tracks loaded graph ownership separately from active show ownership. Only one project owns the graph at a time; switching development projects requires owner/conductor authority on both, and an active show prevents takeover. Engine enablement prepares assets before installation and opens selected interfaces; failure clears graph ownership. Show activation reuses an already running development graph. Show deactivation resets score transport and releases its scheduled notes but preserves graph state and browser monitors. Disable the engine after deactivation to unload it, silence outputs and release manual MIDI notes. System-audio changes and latency calibration unload the development graph; configuration changes require re-enablement.

Live graph edits are prepared for the loaded project in development as well as performance. Score/project-setting edits remain available until show activation. `graph_clock` beat and control phases survive show stop, tempo changes and compatible live graph replacements; musical controls no longer restart at show beat zero. Telemetry `running`/`beat` still describe the show, with `graph_beat` exposed separately. No browser timer schedules graph or score events.

## Sample catalog and conversion

SQLite `sample_library` stores ownership, origin project, editable name/description/category/tags/BPM/key, global sharing and metadata revision, plus immutable technical audio information. `project_samples` maps a catalog UUID to each project's numeric DSP asset ID. Imported float WAV masters live under `sample-library/`; each project keeps a WAV copy under its existing sample directory and prepares rate-specific playback caches. Legacy project WAVs and samples bundled in inserted subgraphs are registered lazily when listing project samples, attributed to that project's owner when no original uploader is recorded.

The sidebar lists only the current project's associations. The full browser lists samples owned by the signed-in user or marked global. Audio and metadata endpoints require project membership plus sample ownership, global visibility or an existing association with that project. Project association preserves access for all members, even after global sharing is removed. The sample owner or editors of its origin project can edit metadata; only its owner can change global visibility. Metadata uses a separate optimistic revision check and does not mutate graph undo/history. Audio is immutable, so imported project copies remain stable.

FFmpeg is a runtime/setup dependency with the seekable `fd` input protocol. Upload conversion reads only its supplied file descriptor, with nested file/network protocols excluded. The first audio stream becomes 32-bit float WAV at the current configured engine sample rate; channel count is preserved. Uploads remain limited to 64 MB, 30 seconds and 1–8 channels. Conversion is serialized under the setup lock, runs in a separate child process with a 60-second timeout and bounded output size, and never runs in DSP/device callbacks. Original compressed upload bytes are temporary; the normalized WAV master is retained. Existing rate-cache preparation remains off-thread.

Browser preview is local audition audio independent of show transport: sidebar/full-browser buttons play at most three seconds, while the metadata modal decodes the complete waveform for every channel and supports full playback and seeking. Native output routing and hardware-clock timing are not claimed for these browser previews.

## Local HTTPS setup

`init.sh --setup-ssl` generates a self-signed local CA and a SAN server certificate using OpenSSL in `certs/`. Initial setup offers the same action. The server discovers managed certificates at startup; managed modes select 443/80 even with an older saved bind port, while `PR0_PORT` remains an explicit override. TLS configuration determines Secure session-cookie behavior. Rustls explicitly enables TLS 1.2 alongside TLS 1.3 for client compatibility. HTTPS runs alongside a separate HTTP redirect router (default port 80, `PR0_HTTP_PORT` override), returning temporary 307 redirects (so switching back to HTTP is not permanently cached) with path/query preservation and IPv6-aware authority parsing. Listener or certificate failures fail startup rather than silently falling back to HTTP.

`--no-ssl` forwards `PR0_NO_SSL=1` through both manual and update/start launches. `--remove-ssl` deletes managed keys/certificates and writes a disabled marker that suppresses old external TLS variables; setup removes the marker. These commands affect the next server start, never modify running services or install OS trust. The CA must be installed and explicitly trusted on clients; Linux privileged-port permission must be configured by the operator or alternate ports selected. Certificate generation and loading occur outside all audio paths.

## Polyphonic sine and FM instruments

`synth` and `fm_synth` accept numeric MIDI `pitch`, `velocity`, `gate`, `trigger` and pitch-specific `note_off` inputs. They also accept direct score events through the existing part/note-instance scheduler. MIDI controls use the shared edge detector and a reserved voice-owner scope; releasing a graph note cannot release a score-owned note at the same pitch. Repeated MIDI pitches release oldest-held-first. Gate is a monophonic fallback when explicit trigger inputs are absent; unconnected velocity defaults to 100. Both use 64 fixed voice slots, reuse silent slots first and steal the quietest at capacity. Compatible live replacements preserve graph-held voices and both oscillator phases while scheduled voices still migrate only for surviving parts.

The FM voice has independent carrier and modulator phase. Carrier/modulator frequency and FM depth are specified in Hz at A4 (MIDI 69), then transposed by each voice’s MIDI pitch. Modulator waveform output times depth is added to carrier frequency; signed increments support through-zero FM. Phase increments are capped below Nyquist; saw/square use the existing polyBLEP waveform implementation, but this basic synth is not oversampled and arbitrary high-depth FM is not guaranteed alias-free. Both waveforms offer sine, triangle, sawtooth, square and noise. Frequency inputs and depth are numeric, drivable parameters evaluated on every engine sample. All storage is prepared; note dispatch and rendering allocate nothing and perform no device/network operations.

The catalog supplies `default_channels` (one for both synths, two for other current nodes). Newly inserted synths use mono; the modal allows 1–8 channels, each carrying the same polyphonic mix. Channel routing/spatialization remains an explicit graph operation. Existing projects retain their stored widths.


## Loop capture and performance archives

The `looper` node preallocates eight interleaved track buffers during engine preparation. `loop_mode=0` starts immediately and captures until stopped/capacity; positive values count the project's denominator beats, with record/play queued to the next running **graph-clock** bar. The graph clock remains active while the show timeline is stopped. Stop/clear are immediate; same-track commands must return to zero to rearm. Mode and beat count latch on the command. Tempo changes preserve phase and affect pending musical boundaries; existing PCM plays at its recorded speed without time stretching. Per-track capacity is 1–300 seconds, default 30, with a 512 MiB aggregate looper-memory preparation limit. Clear wins simultaneous commands. Starting playback can finish a recording; stopping recording alone does not start playback.

Completed loop tracks are 32-bit float WAVs under `PR0_DATA/loops` (default `data/loops`), scoped by encoded project and node IDs, track 1–8. Encoding prevents path traversal. The orchestration worker copies dirty snapshots outside `Engine::render`; a bounded file-worker queue performs replacement using temporary files and atomic rename. Starting a new capture removes the prior saved track; completion writes its replacement. Clear cancels pending actions and removes the saved WAV in queue order. Engine disable/project retirement finalizes partial captures and flushes writes before releasing buffers. Compatible live edits transfer buffers. Restoration occurs during preparation; sample-rate changes use offline linear interpolation, channel changes retain matching channels and zero-fill extras. Reducing capacity below saved duration requires clearing the track or increasing capacity. Deleting a node leaves its saved recordings available if the same ID is restored. Abrupt termination can lose an unfinished capture. The clear endpoint requires CSRF and project edit permission; it does not mutate graph revisions or undo history.

The `record` node archives one incoming 1–8 channel bundle as a multichannel WAV. The server validates its audio/control contracts; recording width derives from the actual source port, including fixed-width and subgraph boundary ports, rather than the node's editable width. Start/Stop accept positive rising edges, with Stop winning ties; Start includes the current sample and Stop excludes it. A fixed 65,536-event buffer per recorder holds start/audio/stop markers without allocation in render. Up to 16 record nodes are admitted. The orchestration worker drains buffers through a bounded queue to a dedicated file writer. Buffer exhaustion stops capture and marks the archive incomplete; disk errors surface in engine telemetry. No callback or DSP render code opens/writes files, acquires locks, or allocates. Worker copies, telemetry, file flush barriers and graph preparation do not establish hardware deadline safety.

Archives live in the server working directory's Git-ignored `recordings/` folder (`PR0_RECORDINGS_ROOT` overrides it for isolated tests). Encoded project directories contain sanitized node-name prefixes, unique take IDs, segment numbers, and Unix-millisecond timestamps. WAVs preserve channels and use the configured engine sample rate with 32-bit float PCM, matching current project sample storage; there is no separate project bit-depth setting. Takes split at 1 GiB to stay below RIFF limits. JSON sidecars retain project/node ownership, name, sample format, timestamp, shared session ID, segment, final frame count and completion state. Unix directories/files use 0700/0600. This directory is outside static serving; no recording list/download route or UI exists yet. Future access must authorize current project membership using the sidecar's project identity, never a guessed file path. Operating-system administrators retain filesystem access.

Stop, engine disable, node retirement or project switch finalize open archives. Compatible graph edits preserve the active take; rewiring or changing its input width retires the old take. Pausing/stopping the show timeline alone leaves graph recording active. WAV headers flush approximately once per captured second; an abrupt process/host failure may leave an incomplete archive and lose buffered audio. Sustained physical-device recording, power-loss recovery, hardware scheduling and disk endurance remain manual/unverified.


## Compact trigger and toggle controls

`trigger` is a numeric pass-through node with a momentary manual override. A click queues a Bang through the existing authenticated, CSRF-protected project control route, including when `in` is connected. DSP outputs exactly 1 for the next sample and then resumes the input, or zero when disconnected. Nonzero input is preserved, including negative and fractional values; a held value is not converted to an edge pulse. A runtime-only sequence counter catches short activations between telemetry snapshots. The radio-style button uses an 80 ms presentation flash for short events plus the current nonzero output; this does not lengthen the DSP pulse. Trigger clicks do not change project revisions or undo history.

`toggle` is a compact checkbox with a numeric-or-text `in` and binary numeric event `out`. Its manual literal is validated as 0 or 1, saved through the project control route, and available while the engine is off. Changed input sets live state (positive numbers or any text on; zero or negative numbers off); manual clicks override until the input changes again. On preparation, the first connected input sets the state. Output emits only when the checkbox state changes; restoring an unconnected saved checkbox does not emit an activation. Input-driven state is telemetry only and never written to project history. Compatible graph edits preserve live state. The compact nodes retain connection handles, 44 px controls, keyboard operation, context menus and modal settings without visible titles/readouts. Trigger remains numeric; Toggle accepts text on its input but always emits a number. Server validation rejects incompatible audio/spectral connections and invalid toggle literals.


## Control merging, Value, and named routes

Multiple physical drivers are allowed only for Control inputs and parameters. Prepared merge groups retain each source's last Datum and the winning value; changes are resolved per engine sample. Simultaneous changes use absolute graph Y, then X, then node ID. Zero is a valid change. Triggered Value emissions additionally carry a per-sample event flag, so repeated identical commands participate in arbitration. Transparent control nodes and named control routes preserve these events. Losing simultaneous changes are consumed rather than replayed. Runtime driver indices identify the actual source in modal telemetry without entering saved state or undo. Value retains the existing `value` parameter/input ID (displayed as Set value) and adds numeric `trigger`: connected nonzero emits the stored value, including zero, while output holds its last emission between triggers. An unconnected trigger preserves constant-source behavior. Toggle accepts an explicit input event even when its payload equals the previous input, so repeated commands override manual state. Event flags are fixed-size runtime state, never persisted or added to undo.

Six typed `send_*`/`receive_*` nodes store a bounded text Target in `control_value`, with a text Control input for live overrides. Empty targets are disconnected. Core validation checks target types, propagated string contracts, static channel/FFT compatibility and a maximum of 64 endpoints. Preparation allocates routing state and spectral snapshots. Senders publish after the sample's graph processing; receivers read the preceding publication, giving every virtual hop one sample of delay and allowing bounded dynamic feedback without introducing wired DAG cycles. Audio receivers sum compatible senders; Control receivers retain the latest changed value with spatial tie priority; Spectral receivers select the highest compatible sender and retain frame metadata with receiver-local generation counters. Dynamic incompatible formats are skipped and reported in telemetry. DSP routing performs no allocation, locking or I/O.

Compatible replacements retain routing state when node ordering is unchanged; insertion, deletion or reordering resets it. Receiver latency includes its one-sample hop, but upstream latency through virtual routes is not propagated for automatic branch alignment. Dynamic-route load, long feedback chains and hardware deadline behavior remain unverified.

Sample metadata stores nullable `root_note` with integer MIDI bounds 0–127 and an idempotent SQLite migration. Server project/parameter updates initialize an unconnected polyphonic sampler root only on new sample assignment, using a project-authorized sample lookup. Existing assignments and manually overridden roots are preserved; later metadata edits do not retune existing assignments.

Matching-port connection is an editor operation that produces ordinary graph edges in one revision. Explicit selection order determines direction; selection rectangles use left-to-right order. The browser filters names/types and occupied audio/spectral inputs, while the server remains authoritative for all resulting graph contracts, limits and cycles.

Input-handle drags are owned by the editor rather than Vue Flow's reverse-connection gesture. Pointer capture moves the full incoming edge bundle as a presentation preview; the graph changes only on release in one server-validated revision. Input drops retain source ports and edge IDs; non-input drops remove the bundle. Invalid destinations restore the original graph through the existing save/error path. Escape/pointer cancellation restores the preview without saving. Output gestures remain ordinary new-edge creation.


## Expanded piano display and sample placement

Piano `octaves` is a structural integer display parameter (1–8, default 1), alongside the starting `octave`. Display configuration preserves MIDI source state across compatible engine replacement. Telemetry emits held-key state across the selected range, capped at MIDI 127; the Vue keyboard widens without shrinking keys and releases local presses when its display range changes. Received out-of-range notes still forward. Authored polyphonic-sampler `root_note` values must be integers; UI sliders and number fields use step 1 and reject fractional entry. The existing continuous control-input contract is unchanged.

Sample drag payloads contain only a library ID. The editor resolves it against the current project's sample list, assigns nodes exposing an asset parameter, or creates a polyphonic sampler on an empty-canvas drop. Assignment/creation uses ordinary server-validated project saves and undo; sample permissions, prepared format and root defaults remain server responsibilities. Touch placement reuses the library's scroll-versus-drag gesture and preserves metadata editing on tap.

## LAN WebRTC discovery

The server uses ICE-lite with numeric host candidates and disables mDNS querying. Browser ICE agents initiate checks to the advertised server addresses; peer-reflexive candidates allow transport even when browser SDP contains unresolved `.local` host names. This avoids the dependency's indefinitely repeated multicast query sends on unreachable routes. SDP advertises `a=ice-lite`; UDP connectivity and DTLS/SRTP authentication are unchanged. This is a LAN host-candidate configuration, not a TURN/NAT traversal implementation. Tests exercise received monitor audio with deliberately unresolvable browser candidates; physical network changes, multicast environments and iOS devices remain manual/unverified.


Toggle separates its held checkbox state (`_checked` telemetry) from one-sample output events. A fixed event-only flag accompanies its numeric payload; inactive frames are not zero commands. Bindings suppress idle events, retain driven parameter values, and feed zero to pulse consumers between events. Toggle ignores those idle input frames. Merge groups consume explicit off events without treating idle payloads as new arrivals; transparent control nodes, Trigger pass-through and named control publications preserve event validity. Named routes retain their one-sample delay. Manual or incoming commands that leave the checkbox state unchanged emit nothing. These runtime flags and visual state never enter project history; only manual checkbox settings are saved.


## FFT pitch tracker

`pitch_tracker` takes a single 1–8-channel audio bundle and infers its width from the source port, including fixed-width channel and subgraph outputs. Core validation allows this inferred input width but preserves ordinary signal, driver and channel limits. Structural `slots` selects 1–4 outputs; edges targeting disabled source slots are rejected by the server. `fft_size` accepts powers of two from 2048 through 8192. Detection threshold is a connected-capable numeric parameter. The editor presents only enabled outputs; reducing slots deletes their outgoing edges in one saved/undoable edit.

Preparation allocates mono history, Hann window, complex FFT buffer, magnitudes and FFT scratch. Rendering averages channels and analyzes every quarter-window after a complete window is available. It retains at most 64 local spectral peaks above both the absolute threshold and a relative floor, refines peak frequency with log-magnitude parabolic interpolation, groups approximate integer harmonics under observed lower fundamentals, and selects up to four distinct rounded MIDI notes by amplitude-based harmonic score. These scores are not calibrated probabilities. Outputs hold the last frame's ranked notes; absent slots use −1. Frame analysis uses fixed storage without allocations, locks or I/O. Telemetry serialization and display labels remain outside rendering; compatible unchanged configurations retain tracker history.

Window duration and hop are analysis timing, not verified end-to-end device latency. Spectral resolution limits closely spaced/low notes; octave doubling, absent fundamentals, transients and noise can confuse harmonic grouping. This does not create note-on/off events or guarantee general polyphonic transcription. Synthetic-tone tests do not establish real-instrument accuracy, callback deadlines or multi-tracker load capacity.


## Continuous convolution and granular processors

`convolution` performs per-channel, continuous short-frame convolution. Its two audio inputs must match the node's 1–8-channel width. Every half-window hop, it takes a Hann-windowed A frame and the latest unwindowed B frame, zero pads each to twice the 128–2048-sample window, multiplies their prepared FFTs and overlap-adds the linear-convolution result. Optional B normalization uses the frame's absolute sample sum. Both histories, FFT buffers/scratch and overlap ring are prepared before rendering. The dry path and graph latency metadata use one window of delay. Changing B changes the effective response continuously; this is not the convolution of two indefinitely accumulated streams or a captured long IR. Window changes rebuild the processor; compatible unchanged configurations preserve histories.

`granular_synth` uses project sample preparation, caches, integer root defaults and subgraph sample bundling. It accepts the standard five graph MIDI controls, including repeated-pitch note-off handling, from Piano/MIDI/Part MIDI. Sixteen fixed voice records drive a fixed 128-grain pool; capacity steals bounded slots. Births latch sample position plus deterministic random spray, pitch ratio and duration. Hann grains read the wrapped sample with linear interpolation and share channel timing; density compensates average overlap gain. Released voices stop spawning and fade existing grains. All storage is prepared and rendering performs no allocations, locks or I/O. Direct score-instrument scheduling is not added; scores use Part MIDI routing.

`granular_pitch_shift` writes live audio to a prepared ring and launches two half-overlapped Hann read heads. Grain duration is structural; pitch ratio is latched at each birth, preserving each active grain's trajectory. Mix is live. All channels share head positions and windows. The ring supports ratios 0.25–4; the dry delay is three grain lengths plus two samples, also used as nominal graph latency. This is exact alignment at zero shift; shifted instantaneous delay varies with grain age and ratio, so latency metadata is not a fixed shifted-path group-delay guarantee. There is no offline stretching, file access, render allocation or callback lock.

These implementations have synthetic/reference and graph/browser validation, not hardware acceptance or comparative performance benchmarks. Grain boundaries can cause sidebands/modulation; interpolation and high ratios are not guaranteed alias-free. Dense clouds can steal grains. Convolution normalization bounds each response window but does not guarantee peak normalization of arbitrary changing responses. CPU deadlines, sustained multi-node load and physical listening remain manual/unverified.

## Desktop distribution

The independent `desktop/src-tauri` workspace packages the existing server as a
sidecar and serves the same Vue build. Desktop startup binds `127.0.0.1:0`,
ignores bind/TLS overrides, and creates a persistent local owner only in a fresh
or previously initialized desktop database. A `desktop_owner` identity mapping
is created only in desktop mode. It never upgrades an existing server user into
an administrator. A random expiring session crosses the private stdout pipe and
is installed as an HttpOnly cookie by the native shell. `/api/me` identifies this
exact session as `is_desktop_session`; its account menu disables Sign out and the logout API
refuses to revoke it. Ordinary logout preserves this private session, while app
shutdown revokes it. User creation, invitations, remote sign-out and project
authorization remain available. The performance frontend has no Tauri capabilities. Host checks restrict requests
to the assigned loopback authority. Frontend and FFmpeg locations can be set with
`PR0_WEB_ROOT` and `PR0_FFMPEG` while standalone defaults remain unchanged.

Opt-in desktop hosting adds a separate HTTPS LAN listener over the same router
and engine. Private pipe commands start/stop it; remote webviews have no hosting
capability. The LAN listener rejects the launcher's private session and preserves
normal account/project authorization. A persistent profile-local CA signs host
certificates; a separate HTTP listener serves only public certificate bootstrap
resources. Certificate generation, TLS, mDNS advertisement and discovery run off
audio threads. The local listener remains private and available when hosting stops.
A future iPad Tauri shell is client-only and must omit the server sidecar and host
controls; no mobile build is supplied here.

Packaged connection/hosting dialogs have narrowly scoped native permissions and
validate their actual local origin. Performance webviews have no native IPC
capabilities. The macOS connection workflow can remember per-origin SHA-256 leaf
certificate pins, checked again by WKWebView's server-trust challenge. The OS
trust store is unchanged. Native window metadata is derived from the current
server/project URL and saved outside project state; same-project windows copy
only that server's cookies. Remote sessions use private storage.

The launcher owns a per-profile OS file lock and child stdin. Pipe EOF requests a
new orchestration Shutdown command; outside render/device callbacks, it closes
devices, releases notes, finalizes loops/recordings and waits for disk barriers.
The launcher bounds its exit wait at 30 seconds. No DSP scheduling or device
callback work moves into the webview. Profile data and recordings live outside
the application bundle. `build.sh` builds the native host target, pins and builds
FFmpeg without GPL/nonfree additions, stages local assets/licenses, and invokes
Tauri packaging. See DESKTOP for packaging constraints and manual validation.

## Project presence and idle shutdown

Authenticated project event WebSockets define presence. Each connection has a
separate lease, including multiple tabs for the same account and non-owner project
members. Lease registration and final vacancy checks share the setup mutex with
graph ownership changes. Dropping the last lease starts a five-second reconnect
grace period; a new connection changes the generation and invalidates the older
shutdown task. Late cleanup for one project cannot disable another project's graph.

After the grace period, an empty project that still owns the graph receives ordered
Show(false), Enable(false), and Unload commands. This stops transport/count-in,
releases notes, closes devices, flushes loops/recordings, and frees graph state.
Shutdown enqueueing waits for command capacity off runtime/render threads instead
of dropping the request on a full queue. Ownership updates broadcast after the
device-disable acknowledgement; pending and connected project WebRTC sessions are
cancelled/closed. Returning users must explicitly enable the engine again. Project
content and revisions are not changed by presence. HTTP-only/OSC workflows that
have never opened a project event connection retain their existing behavior; HTTP
requests alone do not count as persistent users.

Server protocol Ping frames are sent every ten seconds. Thirty seconds without an
inbound frame expires an unresponsive event connection; sends have five-second
timeouts so a stalled writer cannot retain presence indefinitely. Browser protocol
Pong responses work independently of the frontend's JavaScript timer. Network-loss
cleanup follows heartbeat expiry plus the reconnect grace and engine/disk cleanup,
not an instantaneous physical-disconnect guarantee. Presence lives entirely in
server memory and is neither project data nor undo history.

## Device discovery and playback continuity

Audio/MIDI inventory enumeration runs in a blocking-pool task on the HTTP side,
not on the audio orchestration worker. GET /api/devices combines that inventory
with a small runtime-status reply from the worker, retaining the existing response
contract. Discovery still serializes setup operations and preserves saved device
preferences, but an inventory refresh no longer directly occupies the thread that
refills native output and supplies browser monitor PCM. Native-disabled test mode
also skips MIDI enumeration. The Monitor screen discovers inventory on opening;
its two-second timer fetches only process statistics. Explicit settings/device
refreshes continue to rediscover hardware.

The orchestration loop processes at most 16 queued commands before checking output
pacing/rendering again, preserving FIFO order without allowing a continuous
command producer to monopolize the loop. This is not a dedicated realtime render
thread: preparation/install work, telemetry and other existing between-block work
can still cause stalls. The discovery change does not modify fixed native latency
compensation, WebRTC jitter buffering, output queue targets, DSP sample clocks or
score event timing. Physical playback validation remains required.

The presentation clock keeps a nondecreasing displayed beat during normal playback.
Late snapshots or a clock-offset correction can hold the display until the engine
estimate catches up; stale telemetry freezes the last displayed position instead
of snapping back to the older snapshot. Paused/stopped snapshots, a lower
authoritative beat, changed project/epoch and deactivation still reset/reanchor
the display. The existing 500 ms extrapolation cap remains. This affects only
browser presentation and does not correct, resample or drive audio timing.

## Independent MIDI sources and monitor sizing (2026-09-08)

Complete direct `pitch`, `velocity`, `gate`, `trigger`, and `note_off` connections
from Part MIDI, Piano, MIDI input, or OSC-to-MIDI to sine/FM synths are prepared
as independent note decoders. Each source keeps its pitch/velocity and pulse
history together, and graph voices carry a source-local lane identity. A release
only releases that source's oldest held instance of the pitch. Simultaneous
sources are both processed; these complete note bundles bypass ordinary scalar
winner arbitration. Partial/custom control wiring retains the existing scalar
rules. Compatible graph replacement remaps compiled source indices while
preserving decoder history and held voices. Render uses prepared storage only.

Monitor channel strips use fixed equal widths and fixed text rows, with the
meter track taking the remaining panel height. Device groups scroll horizontally
for additional channels, without vertical meter scrolling. Short windows place
input/output banks side by side to preserve meter height. Changing dB text,
clipping state, or missing-data labels does not change strip geometry.

## Audio efficiency and browser buffering (2026-09-08)

Monitor conversion prepares the repeating rational-rate windowed-sinc kernels once,
retaining phase and filter history across blocks. Converted master/cue PCM is
assembled into 480-frame stereo packets before entering a 64-slot broadcast queue.
Its burst capacity is therefore 640 ms at all DSP block sizes; consumers send each
packet as soon as available, without a new prebuffer delay. Partial PCM and filter
history reset together on project, rate or feed-topology changes and when no monitor
listeners remain. All feeds remain collected while any monitor is connected.

Physical MIDI queues are polled once per DSP block, preserving queued order, overflow
release and bounded batch draining; input arriving during rendering waits for the
next block. Internal score events still use engine sample time. Sine/FM voice pitch
ratios are computed at note-on; sampler increments are cached until root changes.

See [AUDIO_ENGINE_AUDIT.md](AUDIO_ENGINE_AUDIT.md) for measurements, regression scope,
and the remaining disk-retirement, independent-device-clock and shared-worker risks.


## Project browsing, administration and quick insertion (2026-09-09)

Project summaries include accessible project metadata (owner, role, mode, tempo,
meter, revision, part/node counts). `project_recents` stores a per-user opening
sequence in SQLite, updated only after authorized creation/opening; the header
shows the latest five. The project browser searches these summaries without
case sensitivity and offers table/icon views. Its sample organizer lists owned,
global, and project-shared samples, preserving access for project collaborators.

`user_profiles` extends existing accounts with an administrator role, activation,
contact/name fields and an optimistic revision. Migration grants administration
to the oldest account once; a database trigger makes the first new installation
account an admin. This also covers the desktop bootstrap identity. Subsequent
accounts are regular users. System audio/OSC configuration and user management
require server-validated administration. Authenticated graph clients receive a
minimal `/api/audio/config` read contract; project settings retain project roles.
Admins can create/update accounts, reset passwords, revoke sessions, deactivate,
and delete accounts without owned projects or live samples. Deletion retains a
scrubbed identity tombstone for historical references. The last active admin and
self-deletion are protected. Credential, role and activation changes revoke
sessions and close affected WebSocket/WebRTC connections.

Sample deletion requires the owner for private samples or an admin for samples
that have ever been global. Owners can unshare global samples while preserving
existing project access, but cannot bypass the admin deletion rule by unsharing.
A review endpoint returns total affected project count and only authorized project
names. Deletion requires exact-name confirmation, risk acknowledgement, current
metadata revision and an unchanged association token. It is rejected while an
affected project's engine is loaded. Catalog tombstones prevent legacy discovery
from restoring deleted entries. Original WAVs, linked project WAVs and conversion
caches are removed; filesystem cleanup failures are reported. Graph references
and saved revisions deliberately remain, so deletion can break projects. Copies
already embedded in separately published subgraph versions are independent assets.

In an editable graph, N opens the quick node browser. It searches node descriptors,
latest saved node-group versions and accessible samples, with type icons. Arrow
and Page keys navigate, Ctrl+Home/End select endpoints, Enter inserts into the
current graph, Escape closes, and Tab cycles the search/close controls. A compact
fade respects reduced-motion settings. Pointer dragging uses graph screen-to-flow
placement for mouse/touch, including nested graph scope; samples are attached to
the current project before inserting a polyphonic sampler. Text entry, modal and
non-graph contexts suppress the shortcut. All insertions use existing server
validation and revision/undo paths.


Send/Receive target modal fields maintain a local draft until commit, so telemetry
rerenders do not overwrite text in progress. A native datalist deduplicates target
names across all project nodes of the same signal type, including nested groups.
Connected nodes contribute only fresh engine target telemetry to suggestions;
stored fallback names are not presented as active dynamic routes. Driven fields
retain the live target/source display and expose per-source disconnect controls.
Names remain case-sensitive and normal server graph validation applies on commit.

Score entry audition is an authorized request naming an already saved part/note. The worker sends scoped instrument notes plus staff-filtered Part MIDI events, then releases them after a bounded count of rendered samples. Replacement retires the current preview; unload clears it. This path does not use browser note-off timers or directly address legacy part MIDI/OSC device routes. Persisted part mute and exclusive-solo choices remove the affected prepared note and automation streams; replacement releases existing held notes. Special barline styles are validated layout metadata and do not change traversal.


## Stabilization contracts (2026-09-10)

Score drafts are owned by the application project session, independent of the mounted editor. A flush awaits the current request and every draft created while it runs; validation, conflict and network failures retain the newest draft. Export and manual revision saves stop on failure. Project switching, performance entry and sign-out await the same barrier. Score draft failures remain visible across tabs; browser-memory retention is not offline durable storage.

Ramp handles retain authored event identity, interpolation and endpoint identity in transient editor metadata. Unchanged event round trips preserve holds, gaps, pre-event defaults and step/linear/ease/S-curve shapes. Editing one staff writes a local override and retains part-level fallback for other staves. Gesture previews derive from their starting snapshot and do not create saves or history until release. New hairpins reject overlaps with existing ramps/interior points; they do not silently take ownership of authored marks. Ramp point Left/Right/Home/End navigates, Up/Down changes values (Shift ×10), and Delete removes the selected point.

A bounded persistence queue (eight jobs plus one retained job) serializes archive chunks, loop chunks, engine retirement and barriers. If full, the orchestration worker defers further commands and continues rendering. Live loop collection copies at most 4,096 samples per DSP block; the persistence worker assembles complete snapshots and sends them to file workers. Engine retirement extracts remaining data, waits for ordered writers and destroys retired DSP storage on the persistence thread. Clear, disable and shutdown acknowledgments follow preceding persistence work. Recording buffers remain bounded and can report overflow during sustained storage stalls; this is not lossless storage under arbitrary overload.

Dedicated monitor subscriptions are refreshed by active WebRTC senders every two seconds and expire after eight seconds. Only selected dedicated feeds are collected, while the master packet timeline continues. Removal of a selected node still yields silence. Monitor conversion, telemetry serialization, routing configuration and sequencer replacement still execute on the orchestration worker. `worker_max_work_us` measures maximum observed command/block work up to telemetry generation; `worker_max_block_gap_us` measures the maximum interval between rendered block starts, including ordinary pacing. Neither establishes native callback deadlines.

The playback metronome reads the sequencer’s written position and prepared meter map. Each meter entry sets the bar origin; denominator beats determine click spacing and the numerator determines accents. Repeated written ranges restart their appropriate clicks. Count-in remains its separate pre-performance clock and monitor-only routing contract.


Piano pointer gestures send ordered transient notes and 14-bit channel-one pitch
bend through the existing authorized piano endpoint. Vertical travel of one displayed
keyboard height spans center to either bend limit; horizontal moves over white or
black keys send the previous note-off before the next note-on. Release, cancellation,
focus loss, and component cleanup release held notes and recenter bend. Unsent bend
moves are coalesced without crossing note boundaries. Gestures never save revisions.
Pitch bend travels over typed MIDI cables; the five legacy scalar note outlets retain
integer note identity. Internal synth/FM, sampler, and granular MIDI decoders use a
fixed ±2-semitone range shared across their merged MIDI input (not MPE/per-channel
voices); granular pitch updates apply to newly launched grains. External MIDI receivers
choose their own bend range. Multiple pointers share the piano's channel bend.


The `browser_input` node is displayed as **Local audio input** in the catalog and
capture UI. Its persisted kind and graph contract are unchanged, so existing
projects still load. Capture is local to the current browser or Tauri session and
uses the same WebRTC uplink to whichever pr0former server the session has loaded.
The Tauri Server menu opens a packaged connection dialog with one scoped native
command. Valid HTTP(S) server origins open in separate private webviews with no
native command permissions and no bundled-engine session cookie. The bundled engine
continues in its own window; its normal shutdown protocol is unchanged.


## Local MIDI and controller assignment (2026-09-12)

`local_midi_input` is a typed MIDI source for devices on the client. Its modal
requests Web MIDI access, lists connected inputs, and explicitly connects one.
The device ID and capture state belong to the client session, not project data.
Listeners coexist with score MIDI input, continue while the modal is closed, and
detach on explicit disconnect, device loss, project/role change, socket loss,
engine disablement, or app unmount. Unsupported browser/webview sessions show an
availability message; this does not provide native Web MIDI support to WebKit.

Local channel messages use the authenticated project WebSocket, with server-side
editor-role, loaded-project, node-kind and byte validation. SysEx/system messages
are excluded; two-byte program/pressure and three-byte channel messages are
accepted. Authorization caches invalidate on project/member updates. Each source
node has one socket owner; disconnect releases it and sends all-notes-off on all
16 channels. A per-socket 2,048-message/second limit bounds ingress. This path is
best effort over the network, not sample-timestamped device scheduling.

Server `midi_input` callbacks now admit every valid MIDI 1.0 channel message.
The selected channel still filters input, while Notes/CC mode affects only scalar
decoding. The typed outlet forwards CC, pitch bend, pressure and program changes
in either mode. New nodes default to channel 0 (all); existing explicit channel
settings remain respected. Both input kinds report received message count and
last status/data bytes in transient telemetry. Nodes show a larger 200 ms activity light
and an orange-bordered table of the last five observed channel/detail/value rows,
with no message count or last-message tooltip. Input modals decode note, CC,
pitch bend, program and pressure messages, show raw bytes/counts/drops and retain
at most 20 observations locally, discarding older entries. These are sampled at 20 Hz, after channel filtering;
intermediate messages can be missed. History freezes when telemetry is stale or
the engine is off, and never enters project state or undo history.

Controller learn cancellation is an explicit transient command, not a synthetic
CC or parameter edit. Escape, a pointer press outside the selected control, and
component cleanup cancel the pending learn. Ordinary controller gestures also
cancel it. Channel-mode all-notes-off/all-sound-off messages cannot become learned
assignments. Only successfully learned channel/controller metadata is persisted;
input activity and cancellation never enter revision or undo history.

Knob bindings use persisted `channel_N = 0` for unassigned (1–16 for assigned).
Prepared controller channels use 255 as the unassigned sentinel, which cannot
match a MIDI channel. The controller API and engine reject GUI values for an
unassigned knob, while raw MIDI still passes through and MIDI Learn remains
available. Double-click clears the binding through ordinary graph validation/save;
it cancels pending learn first. The options channel selector can explicitly
unassign or reassign; default and existing bindings retain their previous values.

## Control range conversions (2026-09-12)

Scale normalizes `a` using `input_min` / `input_max` before mapping to `min` /
`max`. Defaults are 0–1 input and -90–6 output, matching output gain's dB range.
Stored output endpoints retain their values. Equal input endpoints return the
output minimum; reversed ranges are supported and out-of-range inputs extrapolate.
Amplitude to dB remains `20 * log10(abs(a))`, with the existing numerical floor,
and now clamps to editable/connectable `min` / `max` dB bounds (defaults -90/+6).
Amplitude 1 yields 0 dB; larger magnitudes yield positive dB. Reversed dB limits
are ordered before clamping, so live crossings cannot panic. All range controls
use server-validated parameter ports and the standard modal sliders/numeric fields.

## OSC node hostname destinations (2026-09-12)

OSC Output and MIDI to OSC accept `hostname:port` as well as `IPv4:port`.
Core graph validation checks hostname syntax and port bounds without DNS/network
access. Prepared routes retain the destination string; only the external node
output worker resolves it, using the server's system resolver and the first IPv4
result, matching the existing IPv4 OSC socket. The separate resolver cache holds
at most 128 entries, with 60-second successful results and 5-second failed results.
Resolution errors flow through node I/O telemetry. No DNS runs on the rendering
worker or device callbacks. DNS delays can stall the external output worker;
OSC delivery remains best effort. Local interface binds and legacy direct part
OSC destinations retain their existing numeric-address contracts.

Console Out captures numeric/text control changes and explicit events into 128 prepared
slots per engine; rendering does no formatting, logging, allocation or locking. The
20 Hz orchestration pass drains these into the project console and reports dropped
entries on overload. Held numeric values are not logged on every sample. MIDI bridge
debugging uses the existing bounded MIDI observations; OSC input/output debugging
reports the latest accepted/prepared value and a counter. These are sampled debug
views, not lossless packet logs or delivery acknowledgements, and never enter revisions.
