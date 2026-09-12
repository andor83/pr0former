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
| UI | Custom graph node theme, paper score, visible save/conflict status, keyboard-focusable ramp points, coarse-pointer touch targets and reduced-motion support. Save coordination, MIDI device lifetime and curve gesture transforms are separate modules. | Frontend tests/build and browser tests. Physical touch/Safari validation remains manual. |
| Ensemble | Member browser with profile summaries, direct existing-user addition, invitation links and self-service square avatars. Owners can remove non-owners; assigned parts become unassigned in the same server transaction. | Rust crop/assignment tests and isolated HTTPS/API workflow. Role editing and the broader user profile editor remain future work. |
| MIDI/OSC | System settings and receiving/sending paths exist. Every graph MIDI source (Part MIDI, MIDI input, OSC-to-MIDI, Piano) receives raw channel messages and derives its five scalar outlets from the same frame that its typed `midi` output forwards, so the two paths cannot disagree. MIDI input keeps the channel byte; OSC-to-MIDI and Piano use channel 1. Piano decodes typed input (keys light, outlets move) and republishes scalar-driven notes. MIDI inputs accept several cables, concatenated in connection order per sample. A MIDI output forwards a typed cable raw and decodes scalar cables, never both. | Engine tests cover each source→sink pair over typed cables, fan-in order, reset propagation, scalar override and the single-send contract; external delivery remains best effort and physical MIDI hardware is unverified. |
| Deployment/assets | Local fonts/assets, native HTTP/HTTPS and desktop packaging, Bash 3.2-compatible launcher. | Existing launcher/desktop validation is historical evidence; startup services are never installed/enabled by automated tests. |

## Remaining performance work

- Dedicated real-time scheduling, command preparation and monitor conversion isolation; incompatible-graph crossfades.
- Adaptive per-device clock drift correction, physical loopback/latency measurements, real MIDI hardware and iPad/Safari runs.
- Worst-case device refill measurements under edit/storage/visualizer stress, a 60-minute soak, and the physical 32-player acceptance run.
- Worker telemetry now includes maximum observed work and block-start gap in microseconds. These software measurements include scheduling/configuration effects and are not a hardware latency or deadline guarantee.

## Validation

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
  per channel; the footer monitor
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
