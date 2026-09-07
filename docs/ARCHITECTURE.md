# Architecture

## Current execution model

`pr0-core` owns serde project types, the node catalog, deterministic graph validation, and the starter project. `pr0-dsp` compiles a graph into a stable topological schedule with preallocated node buffers, state, filter sections, delay lines, and bindings. `pr0-server` owns accounts, SQLite revisions, transport, device I/O, WebRTC, and external MIDI/OSC.

The orchestration worker renders 128-frame blocks at 48 kHz. Without hardware output it follows a monotonic software schedule. With hardware output, a bounded SPSC ring is filled according to device consumption, targeting a small queue. CPAL input/output callbacks only move samples through bounded rings; they do not run codecs or access application locks. The DSP worker is **not yet a dedicated OS-priority realtime callback engine**. Allocation-free rendering alone does not establish deadline reliability; see STATUS.

The scheduler sends internal synth events before rendering the corresponding sample. External MIDI/OSC messages go through a bounded queue to a separate worker, so external delivery is best effort rather than sample-accurate at the receiving device. Tempo changes preserve beat position and can be queued for the next beat. Prepared live graph replacements can be submitted through the API in conducted/freeform modes and activate at a bar boundary, but the UI currently limits structural edits to inactive shows. Replacement preserves unchanged parts’ event cursors, launch origins, pending cues, and suspended notes. Compatible synth voices retain phase and release envelopes. Changed/deleted parts release their own external notes, while new or changed conducted/freeform parts start idle. Unchanged nodes transfer their prepared runtime storage, including delay/reverb history, filter state, spectral buffers, sample cursors, and control state. Compatibility requires matching node kind, channels, parameter defaults, incoming source/port contracts, and compensation lengths/latency. Compiled binding indices remain those of the new graph, including after node reordering. Changed configurations start with prepared state. Graph crossfades are not yet implemented.

Internal synth voices carry a part scope and note-instance ID. Releasing a note or stopping a part releases only its held instances, including when multiple parts or overlapping notes use the same pitch on one synth. Relaunch releases the previous instances before restarting, and transport Stop explicitly resets event cursors even at beat zero. While transport runs, launch/stop/relaunch waits for the next integer engine beat; a later command for the same part replaces its pending command. While paused or stopped, commands apply immediately. Structured parts begin with transport, while conducted/freeform parts start idle. Transport Stop cancels pending cues and returns independent parts to idle. Pause saves the held note instances in prepared storage and releases their voices. Resume retriggers only those instances whose original release time is still ahead; it does not replay previous score attacks. A part stop/relaunch or transport Stop clears suspended notes. Queued cues remain attached to engine beats across pause. Internal oscillator phase and release-envelope continuity are not preserved across pause.

MIDI 1.0 on each part’s selected channel and the OSC pitch/velocity output contract do not carry note-instance identity. The external worker therefore merges overlapping instances of the same MIDI port/channel/pitch or OSC destination/address/pitch into one held note: first note-on, last note-off. Overlapping attacks are not independently retriggered on those external routes. Panic clears held pitches and sends OSC note-offs as well as MIDI all-notes-off. The bounded external queue can still drop events under overload; delivery and recovery under saturation are not verified.

The WebRTC adapter encodes the selected master mix or dedicated monitor output as 10 ms stereo Opus packets. Projects can contain up to 32 monitor outputs. Dedicated outputs are not summed into hardware speakers; mono duplicates to stereo and wider bundles explicitly select their first two channels. Monitor PCM collection occurs outside Engine::render on the orchestration worker. A project member selects `monitor_node` when offering SDP; the server validates the node. Removal of a selected output yields silence, never fallback to the master. Feed changes currently require reconnecting. Optional microphone uplinks decode into bounded browser-input queues. Codec/network work occurs outside device callbacks. Native and browser sources are separately identifiable nodes. There is no external STUN/TURN dependency. Audio admission reserves one of 32 slots before negotiation and keeps it through peer cleanup; one project/user may have only one pending or live session. Duplicate offers return 409 and require explicit disconnect. Cancellation marks pending offers and final registration rechecks cancellation and active-show ownership. Peer closure happens outside the registry lock; reservation identity prevents delayed cleanup from removing a replacement.

## Graph contracts

- Node IDs and edge IDs are unique. Unknown node kinds/ports/parameters are rejected.
- Widths are 1–8 channels; audio edges require matching per-port widths. Ports normally inherit node width; catalog `fixed_channels` overrides it for mono split outputs and merge inputs. Split/merge ports beyond the active bundle width are silent/ignored.
- One source may drive each input or parameter. Use a mixer/math node to combine values.
- Numeric parameters enforce finite values and declared limits. Structural parameters cannot be driven.
- The current scheduler accepts acyclic graphs. The delay node has internal feedback; graph-level feedback scheduling remains pending.
- Graph limit: 256 nodes and 2,048 connections. Part limit: 32, with at most 10,000 notes per part.
- Streaming spectral branches carry matching transform size/overlap/channel configurations; incompatible connections are rejected. FFT latency is propagated and shorter parallel audio paths are delayed before a merge.
- A control parameter connected to a node is read-only through the API and UI.
- Control numeric nodes currently recompute each sample; rising-edge nodes detect transitions. This is not an emulation of Pd's full hot/cold-inlet message semantics.

## Persistence and collaboration

SQLite uses WAL and foreign keys. Tables hold users, expiring sessions, projects, memberships, invitation tokens, and revision snapshots. Each project update checks an expected revision and atomically writes its new snapshot. Stale edits return HTTP 409. Project updates broadcast to authorized WebSocket subscribers. Each connection subscribes before loading and sending the current project snapshot; broadcast-queue overflow also sends a fresh snapshot. Browsers ignore older revisions and stale socket callbacks, preserving the current stage view and part selection through reconnects.

Current collaboration is optimistic revision-based; fine-grained edit leases and separate published performance revisions remain pending. Owners create single-use invitations valid for seven days. The first account bootstraps without an invite; subsequent account registration requires an unused invitation. Tokens are random and passwords use Argon2. Session cookies are HttpOnly/SameSite=Strict and Secure when native HTTPS is enabled. Mutating API requests require `X-Pr0former: 1`; WebSocket requests require a same-origin Origin.

## HTTP and realtime contracts

All paths below are under `/api`:

| Endpoint | Purpose |
|---|---|
| `GET status`, `GET catalog` | Bootstrap/server status and node descriptors |
| `POST register/login/logout`, `GET me` | Local authentication |
| `GET/POST projects` | List/create authorized projects |
| `GET/PUT projects/:id` | Load/update with revision check |
| `PUT projects/:id/parameter` | Validated `{node, parameter, value, revision}` mutation |
| `POST projects/:id/transport` | Activate/deactivate/play/pause/stop/tempo |
| `POST projects/:id/clip` | Launch/stop an authorized part |
| `POST projects/:id/invite`, `POST join` | Invitation creation/redemption |
| `GET projects/:id/members` | Project membership |
| `POST projects/:id/samples` | Upload a project-scoped WAV sample |
| `GET devices`, `POST projects/:id/audio` | Native device discovery and input/output enablement |
| `POST/DELETE projects/:id/media` | WebRTC SDP negotiation and disconnect |
| `WS projects/:id/events` | Project revisions, timing, and telemetry |

Telemetry carries project ID/revision, session epoch, sequence number, monotonic server timestamp, sample/beat/BPM/running state, device diagnostics, per-part playing/start/local-position/pending-cue state, and effective per-node parameter values. It is emitted at 20 Hz. Browser offset estimation uses the smallest observed ping round trip; the animation interpolates from the latest engine timestamp. Telemetry older than 500 ms is marked stale. A separate `engine_status` message publishes the active project after accepted activation/deactivation commands and in connection/recovery snapshots. Its monotonic server timestamp is sampled while holding the active-project lock, so browsers can reject queued statuses older than their initial snapshot. The client clears telemetry and disconnects its monitor when its project becomes inactive; subsequent telemetry cannot reactivate it. This ownership status does not acknowledge physical device completion.

Modal history is kept locally for 100 observations (five seconds). Graph layout data does not contain telemetry. Individual nodes consume the shared shallow telemetry reference rather than recreating the entire graph on each packet. Detailed server-side telemetry subscriptions and drift regression remain pending.

The browser types in `web/src/types.ts` currently mirror the Rust model manually; automatic generation remains pending. Schema version 1 is validated on import.

## Startup

The server and startup-generation prompt default to `0.0.0.0:4000` (all IPv4 interfaces). `PR0_BIND` or a saved startup script can select a different address. `./init.sh --start` runs the release server in the foreground, using the saved startup script when present.

`--start --host HOST --port PORT` exports `PR0_HOST`/`PR0_PORT` component overrides, applied by the server after reading `PR0_BIND`. Either component can be omitted to retain the saved/environment value. IPv6 hosts are bracketed when forming the address; TLS also resolves hostnames before binding. Overrides do not rewrite saved startup settings.

Manual `--start` launches record their PID, owner, and process start time under `.local/manual-runs`. `--stop` checks that identity before sending SIGTERM and waits up to ten seconds per process. Stale records are discarded. Only launches made with tracking enabled are covered; startup services remain managed by `--startup`.

`init.sh` builds the application and can generate `.local/start-pr0former.sh`. On macOS it installs `~/Library/LaunchAgents/org.pr0former.server.plist`; on Linux it installs `~/.config/systemd/user/pr0former.service`. The startup script sets absolute paths and optional TLS variables. Startup is per-user at login, not system-wide at boot. Disabling it does not delete project data.

## Score settings

Parts store treble/bass/alto/tenor clefs, an optional major key signature, and time-signature visibility. Legacy parts default to no key and a visible signature. The project meter has a numerator of 1–16 and a denominator of 1, 2, 4, 8, 16, or 32 (legacy default: 4). Engine beats, note positions, launch quantization, and BPM remain quarter-note based. Bar lengths are numerator × 4 / denominator; transport beat labels/lights use denominator units. Score edits do not rescale existing note times. These settings are server-validated and retained in the supported MusicXML subset. Mid-score clef/key changes and non-major mode labels produce import warnings. The score editor disables edits during a pending save to avoid overlapping revision writes. Changing/conflicting meters within MusicXML are rejected; full engraving and mid-score meter changes remain pending.

Notation uses a linear 100-pixel quarter-beat scale beginning after the actual clef/signature width. Each notehead is placed at its stored onset and the playhead uses the same origin and scale. Optional viewport following moves the horizontal scroll position without smooth animation while the active score advances. This presentation mapping does not change engine scheduling and does not establish network or display latency. Dotted durations and tied decompositions are supported for the subset documented in STATUS. Dense-note collision handling, measure-aware splitting, and full tuplet engraving remain pending.

Part routing validates MIDI channels 1–16 (legacy default: 1), nonempty port names up to 256 bytes, numeric OSC IP/port destinations with nonzero ports, and literal slash-prefixed OSC paths up to 256 bytes. Channel and address edits are available in inactive score routing settings. Physical MIDI-port delivery remains manually unverified.

Part preparation edits names (1–120 UTF-8 bytes), loop lengths (0.25–4096 quarter beats), and default display (`notation` or `grid`). These contracts are server-validated. Display-tab changes are local viewing choices; Default display is the persisted setting used when selecting a part or opening stage view. Shortening a loop retains all notes: the sequencer skips onsets outside the loop and releases held notes at the end. The editor warns about affected notes.
