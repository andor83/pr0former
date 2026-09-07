# pr0former handoff

The current production checkpoint includes live graph editing, nested subgraphs and library, multi-selection and group context menus, graphical controls, spectral processing, native/browser audio selection, clock tempo inputs, MIDI/OSC score output, and bidirectional OSC transport/control messages.

Before deploying on another server:

1. Clone or pull `main`.
2. Copy the existing `PR0_DATA` directory if you need the users, projects, revisions, samples, and library entries.
3. Run `./init.sh --update`.
4. Start with `./init.sh --start`; the launcher prints and checks the compiled Git revision.

The Codex conversation itself is not stored in the repository. A new Codex session can use this file, `docs/STATUS.md`, and `git log` as its handoff context. The next planned work is to finish MIDI input and graph-level MIDI/OSC node routing, then evaluate Ableton Link integration. Link is a native C++ header-only library with GPLv2+/proprietary licensing; reliable integration also needs mapping the audio device's hardware/system clock to Link's timeline and quantized transport.

Validation for this checkpoint: `cargo test --workspace --locked`, `npm --prefix web run build`, `npm --prefix web test`, and the Playwright suite (10 tests) pass. Physical MIDI/audio devices and network timing remain manual checks.
