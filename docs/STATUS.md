# Current implementation status

pr0former is a development alpha. Software validation does not establish physical audio latency, deadline reliability, iPad compatibility, or readiness for a 32-player performance.

## Capabilities and evidence

| Area | Current behavior | Evidence / limitations |
| --- | --- | --- |
| Graph engine | Server-validated audio (1–8 channels), control, spectral and typed MIDI contracts; nested graphs; prepared DSP storage and live compatible-state transfer. | Rust render allocation/deallocation guards and routing tests. Incompatible replacements have no crossfade. |
| Native audio | Bounded callback rings, driver-consumption pacing, device/channel routing and meters. | Synthetic callback-size tests. Multiple independent hardware clocks are not synchronized; physical latency and endurance remain manual. |
| Persistence during playback | Ordered bounded queue sends archive data, loop chunks and retired engines to a persistence worker. Queue pressure delays command admission while rendering continues. Loop collection copies at most 4,096 samples per block; assembly, writer barriers and retired-engine destruction run off the audio-producing worker. Disable, clear and shutdown acknowledgments wait for preceding writes. | `persistence::tests` holds storage blocked while rendering advances; bounded snapshot reconstruction test; looper/recording browser tests. Real disk stalls and hardware refill deadlines remain unverified. |
| Browser monitoring | WebRTC/Opus master and selected cue feeds; only subscribed dedicated feeds are collected. Feed leases expire after eight seconds without refresh. | Count-in tests receive actual master/cue audio. Conversion and telemetry still share the orchestration worker. |
| Score and transport | Shared score, independent part launches, repeats/navigation, tempo map, notation/piano roll, staff routes and MIDI automation. Meter-aware metronome follows written position and bar origins through changes/repeats. | Sequencer/DSP tests and Chromium integration. External MIDI delivery is best effort. |
| Editing and saves | Project-owned score draft survives tab unmounts; flush waits for current and newer edits. Export, Save revision, project switching, performance entry and sign-out use the barrier. Failures retain the newest draft for explicit reapplication/discard. | Delayed/rejected save unit and browser regressions. Drafts are in browser memory, not offline durable storage. |
| Dynamics and phrasing | Staff overrides preserve part defaults for other staves. Ramp conversion preserves authored holds and curve shapes. New hairpins own an identifiable dynamics event and retain any authored starting mark for restoration; gesture previews are transient, with one commit/undo on release. Moving/removing owned playback updates its wedge. | Value-level ramp and hairpin tests plus browser editing. Hairpins crossing existing ramps/interior points are rejected instead of overwriting them; older wedges without owned events remain independent notation. |
| UI | Custom graph node theme, paper score, visible save/conflict status, keyboard-focusable ramp points, coarse-pointer touch targets and reduced-motion support. Save coordination, MIDI device lifetime and curve gesture transforms are separate modules. | Frontend tests/build and browser tests. Physical touch/Safari validation remains manual. |
| MIDI/OSC | System settings and receiving/sending paths exist. Typed channel-event cables currently cover part MIDI → MIDI output/decoder and graph boundaries; most note nodes retain scalar ports. | See architecture and tests. Universal MIDI cable coverage is **planned**, not implemented: [MIDI_CONNECTION_PLAN.md](MIDI_CONNECTION_PLAN.md). |
| Deployment/assets | Local fonts/assets, native HTTP/HTTPS and desktop packaging, Bash 3.2-compatible launcher. | Existing launcher/desktop validation is historical evidence; startup services are never installed/enabled by automated tests. |

## Remaining performance work

- Dedicated real-time scheduling, command preparation and monitor conversion isolation; incompatible-graph crossfades.
- Adaptive per-device clock drift correction, physical loopback/latency measurements, real MIDI hardware and iPad/Safari runs.
- Worst-case device refill measurements under edit/storage/visualizer stress, a 60-minute soak, and the physical 32-player acceptance run.
- Worker telemetry now includes maximum observed work and block-start gap in microseconds. These software measurements include scheduling/configuration effects and are not a hardware latency or deadline guarantee.

## Validation

Stabilization validation (2026-09-10):

- `cargo test --workspace --quiet`: **179 passed**, one opt-in throughput benchmark ignored. Includes render allocation/deallocation guards, bounded loop chunks, slow-storage command backpressure, writer ordering, retained hairpin metadata validation, and meter/repeat scheduling.
- `npm --prefix web test`: **58 passed**; `npm --prefix web run build`: passed. Vite still reports the existing large-chunk warning (score/VexFlow bundle); no tablet startup-performance claim is made.
- `npx playwright test` in `web`: the **68-case full Chromium/API suite passed**. Twelve focused score cases then passed after final keyboard/dynamics changes. The final five-case phrasing/save run passed, including the newly added keyboard-only ramp test (69 distinct standard cases now exist).
- [Score save/browser regressions](../web/e2e/score-save.spec.ts) delay/fail requests while editing, verify export/revision barriers and draft recovery across tabs, and navigate/nudge ramp points by keyboard. [Phrasing regression](../web/e2e/score-phrasing.spec.ts) verifies multi-frame drag, playback endpoint and single-step undo. [Metronome regression](../web/e2e/metronome.spec.ts) receives actual WebRTC audio energy with score instruments silent.
- The generated score screenshot was inspected. Original documentation artwork was preserved. All browser servers used isolated test data with native devices disabled; no startup services, production server restart or hardware test was performed.

See [VALIDATION_HISTORY.md](VALIDATION_HISTORY.md) for dated prior runs, [AUDIO_ENGINE_AUDIT.md](AUDIO_ENGINE_AUDIT.md) for the September 8 audit, and [ARCHITECTURE.md](ARCHITECTURE.md) / [SCORE_EDITOR.md](SCORE_EDITOR.md) for current contracts and usage.
