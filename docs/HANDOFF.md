# Migration and development handoff

> Current conducted-performance development is documented in
> [CONDUCTED_PERFORMANCE_HANDOFF.md](CONDUCTED_PERFORMANCE_HANDOFF.md). Read that
> file first when continuing the `codex/conducted-performance` branch.

Checkpoint date: 2026-09-07. Read AGENTS.md, ARCHITECTURE.md and STATUS.md before continuing. This document transfers with Git; the Codex conversation itself does not. A new session can start with: “Read docs/HANDOFF.md and continue the MIDI/OSC settings work.”

## Moving the application

1. Clone the repository and check out the intended commit on the destination server. Install the prerequisites described in README.md, then run `./init.sh --update` (or interactive `./init.sh` on a fresh machine).
2. Application state is separate from Git. During the planned cutover, stop the old server before copying the entire actual `PR0_DATA` directory (default `data/`). It contains the SQLite database, project samples/caches, audio settings and library data. Copying only the repository does not copy accounts, projects or saved library versions. Preserve a backup. Do not copy just a live SQLite main file while omitting its WAL.
3. Transfer the required environment configuration and TLS certificate/key through your normal secure administration process. Generated `.local/start-pr0former.sh` and OS services are not in Git and use absolute paths; recreate them on the destination with `--startup` if needed.
4. Review native input/output selections in System settings on the new machine. Device IDs depend on device names; old selections may be unavailable. Review project routes accordingly.
5. Start with `./init.sh --start`, check the printed compiled Git revision and warnings, then verify login, saved projects/library, audio routes and actual playback. Building does not restart an existing process. No server migration or restart was performed by this checkpoint.

## Completed checkpoint

- Git build identity, status API metadata and manual startup remote-version warnings.
- Full-space node/subgraph library accordion; removed hint footer.
- Selection-wide D duplication and group right-click Make subgraph / Duplicate / Delete.
- Compact Graphical control node with Bang, Integer, Float, Slider and Text, optional hidden chrome and change-only output. Numeric limits are in the modal; connected values remain editable, with manual override until the next incoming change/event. Slider gestures preview over the project WebSocket and persist on release without graph recompilation.
- Existing live editing, nested subgraphs/versioned libraries, native/browser input selection, FM and spectral tools remain in the branch. See STATUS.md for verification limits.

## Next accepted work: MIDI and OSC

The user requested System Settings tabs for MIDI and OSC, moving their existing settings into those sections. MIDI should show devices attached to the performance server, including inputs and outputs with clear empty/error states. Current discovery lists only outputs. Existing per-part MIDI port/channel and OSC destination/address controls are in App.vue's score routing row; preserve their project-specific semantics when moving them into the settings UI.

OSC must **send and receive**, explicitly confirmed by the user. Add real network interface discovery, an interface checkbox list with `0.0.0.0` for all IPv4 interfaces, port settings, persistence and applied socket bindings. Do not ship UI-only binding controls. Keep networking outside DSP/device callbacks, receive disabled by default, validate incoming messages and only affect the active project through bounded orchestration commands. Report binding/discovery failures to the user. Define and test a useful incoming transport/node-control contract; mapping UX and exact addresses are not yet agreed or implemented. Current performance.rs only sends pitch/velocity OSC messages for score events and handles panic note-offs. Current STATUS correctly lists incoming OSC as unfinished.

A preliminary, uncompiled OSC module and if-addrs dependency were saved on the original workstation in the local Git stash named `WIP OSC send-receive settings before production checkpoint`. It is not wired into the application, not production code, and is not transferred by a normal push/clone. Implement from the requirements above or explicitly recover that draft locally; do not mistake it for verified functionality.

The user is considering browser MIDI over WebSockets, concerned that devices appear only after login. Treat this as an open design question. A proposed approach is explicit post-login browser permission and session device announcements, with device changes/disconnects updating a live inventory and missing routes shown as unavailable. Browser MIDI transport has not been requested as a definite implementation scope yet.

## Ableton Link investigation

The user asked to look into support, referencing [Ableton's Link documentation](https://ableton.github.io/link/). No Link toggle or dependency has been added.

Proposed integration: one native Link participant on the server, exchanging tempo/beat/phase with a configurable quantum and optional start/stop synchronization. The browser should present state and commands. Define how a connected global clock tempo input interacts with Link to avoid repeated competing tempo proposals. Preserve phase across tempo changes and implement quantized launch.

The current engine is paced by output queues or a software clock. Accurate Link integration needs a mapping from engine samples to the time those samples reach physical output, accounting for queue and device latency. Use Link's realtime-safe session-state API at the rendering boundary; do not use its potentially blocking application-thread API inside render or callbacks. Test two-peer joins/leaves, tempo changes, pause/resume, quantum alignment and actual hardware playback before claiming synchronization quality. [Ableton's integration and timing guidance](https://github.com/Ableton/link) describes the native C++ API, host-time mapping and latency compensation.

The upstream implementation offers GPLv2+ or proprietary licensing; this repository is currently MIT. Resolve the distribution approach before integrating a native dependency. Link Audio is a separate additional scope; it is not needed to investigate tempo/phase synchronization.

## Checks

Run `cargo test --workspace --locked`, `npm --prefix web run build`, `npm --prefix web test`, and `npm --prefix web run test:e2e` for relevant changes. Browser tests use isolated data and port 3101. Do not test against the user's active port 4000. For launcher work run `/bin/bash -n init.sh`, help and `python3 tests/test_init.py`; never enable/install services during automated checks. Hardware listening and input/latency verification must be recorded as manual unless actually performed.

The user authorized Git commands and requested commit/push of completed work. The latest instruction narrowed the immediate task to pushing solid changes for migration; resume feature development only when asked after the checkpoint.
