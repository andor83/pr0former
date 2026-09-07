# Working on pr0former

Read `docs/ARCHITECTURE.md` and `docs/STATUS.md` before changing engine or scheduling behavior. Keep the status document accurate; do not label a stub, UI-only control, untested device path, or software-clock timing result as a verified performance feature.

- Rust code lives in `crates/`; the frontend is Vue 3/TypeScript in `web/`.
- Define node metadata in `pr0-core`, processing in `pr0-dsp`, and device/network orchestration in `pr0-server`.
- Validate graph changes on the server. Do not trust browser validation for roles, limits, or connections.
- DSP `Engine::render` and device callbacks must not allocate, block, acquire locks, log, or touch files/network. Preparation and telemetry serialization occur elsewhere.
- Audio connections carry 1–8 channels. Control values and spectral frames must have separate type contracts.
- Preserve phase across tempo changes. Use engine sample time for musical events; browser timers are presentation only.
- All editable node parameters belong in the modal. Connected parameters show engine values and their source, never an editable fake value. Do not persist telemetry or add it to undo history.
- Preserve the custom charcoal/cyan/amber/violet node styling, compact math nodes, reduced-motion support, and touch targets.
- Run appropriate Rust tests, frontend tests/build, and browser integration tests for affected paths. Hardware tests must be identified as manual unless actually performed.
- `init.sh` must remain compatible with macOS Bash 3.2. Never install/enable startup services during automated tests. Test `bash -n`, help, and the unchanged branch of `--startup` without changing user services.
- Use local fonts/assets so a deployed performance server works without internet access.
- Audio plugins here are Rust DSP plugins, not Codex extension plugins. See `docs/PLUGINS.md`.

