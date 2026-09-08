# Writing a source plugin

Start with `plugins/gain/src/lib.rs`. It implements the small `pr0_dsp::AudioPlugin` trait: prepare, reset, process, and latency reporting. It demonstrates smoothed gain over interleaved buffers. This is a source extension API; there is no runtime dynamic library loader or untrusted plugin sandbox.

## Adding a graph node today

1. Add a descriptor to `pr0_core::catalog()`: stable kind ID, display name/symbol/category, documentation, ports, numeric parameters, limits, defaults, units, and Pd search aliases where relevant.
2. Allocate processor state during `Engine::prepare`. Add a processing branch to `RuntimeNode::process`, or call a reusable processor like `effects::Vocoder`. The trait example is not automatically registered by linking its crate; the current built-in registry needs this explicit integration.
3. Use compiled bindings and preallocated buffers inside rendering. Do not allocate a vector, format strings, open a file, log, or acquire a lock in `process` or `render`.
4. Expose numeric parameters through the descriptor; the Vue modal generates controls and matching control ports automatically. Mark settings that require rebuilding state as structural.
5. Add tests with known signals, silence, all supported channel widths, nonfinite inputs, parameter extremes, and discontinuous edits. Measure CPU cost before expanding the reference graph.
6. Document parameter units, channel behavior, reset behavior, and algorithmic latency. Keep `docs/STATUS.md` accurate if the node only supports part of the planned contract.

## Signal conventions

DSP samples are planar-at-the-frame level: each frame has eight reserved channel slots; only the declared width is active. Core processing uses f64 internally, with f32 device/media buffers. Do not silently broadcast mono or truncate eight-channel inputs: require an explicit router when widths differ.

Control values are separate from audio samples. A connected numeric parameter receives a finite clamped driver value; the UI shows the engine's effective value. For smoothed gain, telemetry reports the actual applied gain rather than the target. Spectral connections carry matching FFT size, overlap, channel width, and frame generation. `Fourier` provides allocation-free transforms; `spectral::Spectral` handles streaming windowing/overlap-add. Current spectral processors include FFT/inverse, gain, and Cartesian/polar conversion.

## Example AI-assisted request

> Read AGENTS.md, docs/ARCHITECTURE.md and docs/PLUGINS.md. Add a tremolo node using the existing LFO/routing conventions. Support 1–8 channels, frequency in Hz, depth in [0,1], phase-preserving rate changes, and smoothed depth. Allocate all state during preparation. Add descriptor-generated modal controls and tests for zero depth, silence, channel isolation, and block-boundary continuity. Do not claim hard realtime validation without measuring it.

## Verification

```sh
cargo test -p pr0-dsp
cargo test -p pr0-core
cargo fmt --all --check
cd web && npm run build
```

Current limitations include a closed built-in processing registry, numeric-only modal metadata, and incomplete latency metadata for custom processors. The next SDK revision should make registration declarative, generate frontend types, and expose spectral frames without making plugin authors modify the engine dispatcher.


## ADSR control envelope

The `adsr` node emits a normalized control signal (0–1). Connect its output to a synth amplitude or another normalized parameter. The modal exposes Gate, Attack, Decay, Sustain, and Release; Gate can be driven by another control node. A positive gate starts attack, and a falling gate starts release from the current level. A rising signal on the Retrigger inlet restarts attack from the current level while the gate remains positive.

Attack/decay/release are linear, in milliseconds, and latch their duration at stage entry. Zero-duration stages complete immediately. Sustain is normalized and live changes use 5 ms smoothing. Processing continues on engine samples independently of musical tempo; show pause and stop leave graph processing and output running. Unchanged-graph state migration retains the envelope. The allocation-free implementation in `crates/dsp/src/envelope.rs` is a small example of a reusable processor integrated through catalog metadata and a DSP dispatch branch.

## Step sequencer

Connect a global clock Tick output (or any rising control pulse) to `step_sequencer.trigger`. The modal holds Length (1–16), Enabled, and all 16 step values. The first pulse selects step 1; subsequent pulses advance and wrap. A rising Reset input selects step 1 even while disabled and takes priority over a simultaneous trigger. Holding either input high does not repeat it. Edges seen while disabled are consumed. Fractional length is rounded down, and shrinking length wraps the current zero-based index into the new range.

Outputs are the selected numeric value, the zero-based step index, and a one-sample pulse on advance/reset. The selected value updates live when its parameter changes, including when driven by another node. Values can represent MIDI pitches, modulation, or other control quantities; this node does not directly emit external MIDI/OSC. State transfers across unchanged graph replacements. See `crates/dsp/src/sequence.rs` for the allocation-free selector.

## Channel map

The `channel_map` node preserves bundle width (1–8 channels). Each modal parameter selects the input source for one output channel: 1–8 are channel numbers; 0 or a number outside the active bundle mutes that output. Fractional values are rounded down. Selecting the same source for multiple outputs explicitly duplicates it. Parameters for outputs beyond the bundle width are inactive.

The initial map applies immediately. Live changes use an exponential crossfade with a 5 ms time constant (they approach the target rather than completing exactly at 5 ms). Routing weights stay in preallocated storage. `crates/dsp/src/channels.rs` demonstrates a reusable multichannel processor; it preserves connection width; use split/merge for branches with different widths.

## Channel split and merge

`channel_split` takes a bundle at its node width and exposes eight fixed-mono `ch_1` through `ch_8` outputs. Outputs above the configured width are silent. `channel_merge` accepts eight fixed-mono inputs and packs the first N into its configured N-channel output; missing inputs are silent and higher inputs are ignored. Set branch effects to one channel in their modals. For example, split an eight-channel source, process channels 1 and 2 through mono effects, then wire them to a two-channel merge for a stereo feed.

Audio port metadata now includes optional `fixed_channels`; otherwise width comes from the node. Graph validation checks port widths, not just node widths, and browser cables display the source port width. DSP keeps eight preallocated input-port slots. Split selection happens before per-edge latency compensation, preserving mono isolation. These are DSP tests, not physical device verification.

## Dedicated browser monitors

Route a custom mix into `monitor_output`, then choose that node under Monitor feed in Browser audio before connecting. This output has smoothed gain in its modal and never contributes to the server speaker mix. Mono input is duplicated to stereo; wider bundles send their first two channels, so use split/map/merge for other selections. Up to 32 monitor outputs are allowed. Any project member can select an available feed; automatic personal assignments and mix-minus generation remain pending.

## Clock ratio

`clock_ratio` derives pulses from the project quarter-note clock. Multiply/Divide parameters (each 1–16, including fractional ratios) set cycles per quarter note; for sixteenth notes use 4/1, for one pulse every two quarters use 1/2. Outputs are Tick, fractional Phase, and completed Count. Connect Tick to a step sequencer or trigger input.

The first running sample emits a tick. The node follows the engine’s graph clock: show pause, stop and rewind leave its phase and count running. Tempo and ratio edits preserve accumulated phase. Loading a fresh engine resets phase and count. Like the existing global clock, pulses are one-sample numeric controls, not typed message queues; a discontinuous position jump coalesces crossed cycles into one pulse while Count records the crossings.


## Note-control nodes

Use the numeric `pitch`, `velocity`, `gate`, `trigger`, `note_off` contract for Part MIDI, MIDI input/output, the polyphonic sampler and the two MIDI/OSC converters. A trigger/release is one sample wide with a low sample between events; read pitch and velocity on that same sample. Gate reports any held notes and does not encode individual polyphonic releases. `crates/dsp/src/midi_controls.rs` provides the bounded event source; `note_inputs.rs` decodes consumer edges. Keep MIDI callbacks, device connections and OSC encoding in `pr0-server`, outside DSP rendering. See ARCHITECTURE.md for the exact OSC wire protocol and queue/voice limits.
