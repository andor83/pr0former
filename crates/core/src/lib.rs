//! Versioned project model and graph validation. No device or UI dependencies.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA_VERSION: u32 = 1;
pub const SAMPLE_RATE: u32 = 48_000;
pub const MAX_CHANNELS: usize = 8;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Structured,
    Conducted,
    Freeform,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Signal {
    Audio,
    Control,
    Spectral,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Parameter {
    pub id: String,
    pub label: String,
    pub unit: String,
    pub min: f64,
    pub max: f64,
    pub default: f64,
    pub logarithmic: bool,
    pub structural: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Port {
    pub id: String,
    pub label: String,
    pub signal: Signal,
    #[serde(default)]
    pub fixed_channels: Option<usize>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Descriptor {
    pub kind: String,
    pub label: String,
    pub symbol: String,
    pub category: String,
    pub description: String,
    pub aliases: Vec<String>,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub parameters: Vec<Parameter>,
}
/// Control messages are separate from audio samples and spectral frames.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ControlValue {
    Number(f64),
    Text(String),
}
pub const MAX_CONTROL_TEXT_BYTES: usize = 256;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LibraryRef {
    pub id: String,
    pub version: u64,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct IoConfig {
    pub port: String,
    pub address: String,
    pub destination: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub io: Option<IoConfig>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<LibraryRef>,
    /// Container node ID; absent means the root editor. Flat storage permits arbitrary nesting.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<String>,
    pub id: String,
    pub kind: String,
    pub label: String,
    pub x: f64,
    pub y: f64,
    pub channels: usize,
    #[serde(default)]
    pub parameters: BTreeMap<String, f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub control_value: Option<ControlValue>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Edge {
    pub id: String,
    pub source: String,
    pub source_port: String,
    pub target: String,
    pub target_port: String,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Graph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Note {
    pub id: String,
    pub pitch: u8,
    pub beat: f64,
    pub duration: f64,
    pub velocity: u8,
    #[serde(default)]
    pub rest: bool,
    #[serde(default)]
    pub tied: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Part {
    pub id: String,
    pub name: String,
    pub performer: Option<String>,
    pub view: String,
    pub clef: String,
    #[serde(default)]
    pub key_signature: Option<String>,
    #[serde(default = "default_show_time")]
    pub show_time_signature: bool,
    pub notes: Vec<Note>,
    pub loop_beats: f64,
    pub instrument_node: Option<String>,
    #[serde(default)]
    pub midi_port: Option<String>,
    #[serde(default = "default_midi_channel")]
    pub midi_channel: u8,
    #[serde(default)]
    pub osc_destination: Option<String>,
    #[serde(default = "default_osc")]
    pub osc_address: String,
}
fn default_midi_channel() -> u8 {
    1
}
fn default_beat_unit() -> u8 {
    4
}
fn default_show_time() -> bool {
    true
}
fn default_osc() -> String {
    "/pr0former/note".into()
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub mode: Mode,
    pub revision: u64,
    pub bpm: f64,
    pub beats_per_bar: u8,
    #[serde(default = "default_beat_unit")]
    pub beat_unit: u8,
    pub graph: Graph,
    pub parts: Vec<Part>,
}

fn param(id: &str, label: &str, unit: &str, min: f64, max: f64, default: f64) -> Parameter {
    Parameter {
        id: id.into(),
        label: label.into(),
        unit: unit.into(),
        min,
        max,
        default,
        logarithmic: false,
        structural: false,
    }
}
fn port(id: &str, signal: Signal) -> Port {
    Port {
        id: id.into(),
        label: id.replace('_', " "),
        signal,
        fixed_channels: None,
    }
}

pub fn catalog() -> Vec<Descriptor> {
    use Signal::*;
    let mut result = Vec::new();
    let mut add = |kind: &str,
                   label: &str,
                   symbol: &str,
                   category: &str,
                   description: &str,
                   inputs: Vec<Port>,
                   outputs: Vec<Port>,
                   parameters: Vec<Parameter>,
                   aliases: &[&str]| {
        result.push(Descriptor {
            kind: kind.into(),
            label: label.into(),
            symbol: symbol.into(),
            category: category.into(),
            description: description.into(),
            inputs,
            outputs,
            parameters,
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
        });
    };
    add(
        "subgraph",
        "Subgraph",
        "▣",
        "Subgraphs",
        "Open a nested graph. Named boundary nodes define its ports.",
        vec![],
        vec![],
        vec![],
        &[],
    );
    for (suffix, signal) in [
        ("audio", Audio),
        ("control", Control),
        ("spectral", Spectral),
    ] {
        for direction in ["input", "output"] {
            let parameters = if signal == Spectral {
                vec![
                    Parameter {
                        structural: true,
                        ..param("size", "FFT size", "samples", 256., 8192., 1024.)
                    },
                    Parameter {
                        structural: true,
                        ..param("overlap", "Overlap", "", 2., 4., 4.)
                    },
                ]
            } else {
                vec![]
            };
            add(
                &format!("subgraph_{direction}_{suffix}"),
                &format!("Subgraph {suffix} {direction}"),
                if direction == "input" { "↳" } else { "↱" },
                if signal == Spectral {
                    "Spectral"
                } else {
                    "Subgraphs"
                },
                "Rename this node to name its parent subgraph port. Set channel width and spectral frame format here.",
                vec![port("in", signal)],
                vec![port("out", signal)],
                parameters,
                &[],
            );
        }
    }
    for (kind, label, symbol) in [
        ("add", "Add", "+"),
        ("subtract", "Subtract", "−"),
        ("multiply", "Multiply", "×"),
        ("divide", "Divide", "÷"),
        ("modulo", "Modulo", "%"),
        ("power", "Power", "xʸ"),
        ("min", "Minimum", "min"),
        ("max", "Maximum", "max"),
        ("greater", "Greater than", ">"),
        ("less", "Less than", "<"),
        ("equal", "Equal", "="),
        ("and", "And", "∧"),
        ("or", "Or", "∨"),
    ] {
        add(
            kind,
            label,
            symbol,
            "Math",
            "Control arithmetic. Both inputs update the effective value. Invalid results become zero.",
            vec![],
            vec![port("out", Control)],
            vec![
                param("a", "A", "", -100000., 100000., 0.),
                param("b", "B", "", -100000., 100000., 1.),
            ],
            &[symbol],
        );
    }
    for (kind, label, symbol) in [
        ("abs", "Absolute", "|x|"),
        ("sqrt", "Square root", "√"),
        ("sin", "Sine", "sin"),
        ("cos", "Cosine", "cos"),
        ("log", "Logarithm", "ln"),
        ("exp", "Exponential", "eˣ"),
        ("not", "Not", "¬"),
        ("mtof", "MIDI to frequency", "Hz"),
        ("ftom", "Frequency to MIDI", "♪"),
        ("dbtoa", "dB to amplitude", "dB"),
        ("atodb", "Amplitude to dB", "dB"),
    ] {
        add(
            kind,
            label,
            symbol,
            "Math",
            "Single-input conversion.",
            vec![],
            vec![port("out", Control)],
            vec![param("a", "Input", "", -100000., 100000., 0.)],
            &[kind],
        );
    }
    add(
        "clamp",
        "Clamp",
        "⊏⊐",
        "Math",
        "Limit a control value.",
        vec![],
        vec![port("out", Control)],
        vec![
            param("a", "Input", "", -100000., 100000., 0.),
            param("min", "Minimum", "", -100000., 100000., 0.),
            param("max", "Maximum", "", -100000., 100000., 1.),
        ],
        &["clip"],
    );
    add(
        "scale",
        "Scale",
        "↗",
        "Math",
        "Map a normalized input to a range.",
        vec![],
        vec![port("out", Control)],
        vec![
            param("a", "Input", "", -100000., 100000., 0.),
            param("min", "Minimum", "", -100000., 100000., 0.),
            param("max", "Maximum", "", -100000., 100000., 1.),
        ],
        &[],
    );
    add(
        "clock",
        "Global clock",
        "◷",
        "Timing",
        "Authoritative project clock. Connect tempo to control project BPM (1–400), preserving beat phase. Disconnect to retain the last tempo.",
        vec![port("tempo", Control)],
        vec![
            port("tick", Control),
            port("beat", Control),
            port("bpm", Control),
        ],
        vec![],
        &["metro"],
    );
    add(
        "clock_ratio",
        "Clock ratio",
        "×/÷",
        "Timing",
        "Multiply/divide the project quarter-note pulse. Preserves phase through tempo and ratio edits; pauses with transport. Stop/rewind resets phase. Outputs pulse, fractional phase, and completed cycle count.",
        vec![],
        vec![
            port("tick", Control),
            port("phase", Control),
            port("count", Control),
        ],
        vec![
            param("multiply", "Multiply", "", 1., 16., 1.),
            param("divide", "Divide", "", 1., 16., 1.),
        ],
        &["clock divide", "clock multiply", "subdivision"],
    );
    add(
        "metro",
        "Metronome",
        "◴",
        "Timing",
        "Independent metronome; period may be driven from the graph.",
        vec![],
        vec![port("tick", Control)],
        vec![
            param("bpm", "Tempo", "BPM", 1., 400., 120.),
            param("enabled", "Enabled", "", 0., 1., 1.),
        ],
        &["metro"],
    );
    add(
        "counter",
        "Counter",
        "#",
        "Timing",
        "Increment on rising trigger. Reset wins over simultaneous trigger.",
        vec![port("trigger", Control), port("reset", Control)],
        vec![port("out", Control)],
        vec![
            param("step", "Step", "", -1000., 1000., 1.),
            param("min", "Minimum", "", -100000., 100000., 0.),
            param("max", "Maximum", "", -100000., 100000., 15.),
        ],
        &["float", "+ 1"],
    );
    let mut steps = vec![
        param("length", "Steps", "", 1., 16., 8.),
        param("enabled", "Enabled", "", 0., 1., 1.),
    ];
    for i in 1..=16 {
        steps.push(param(
            &format!("step_{i}"),
            &format!("Step {i}"),
            "",
            -100000.,
            100000.,
            0.,
        ));
    }
    add(
        "step_sequencer",
        "Step sequencer",
        "▤",
        "Timing",
        "Up to 16 control values, advancing on rising trigger. First trigger selects step 1. Rising reset wins and selects step 1 even when disabled. Fractional length is rounded down; step output is zero-based.",
        vec![port("trigger", Control), port("reset", Control)],
        vec![
            port("out", Control),
            port("step", Control),
            port("pulse", Control),
        ],
        steps,
        &["sequencer", "table", "step"],
    );
    add(
        "adsr",
        "ADSR envelope",
        "⏢",
        "Control",
        "Sample-timed linear ADSR. Positive gate attacks; gate release ramps from the current level. Rising retrigger restarts attack while gated. Stage times latch at entry; sustain edits are smoothed.",
        vec![port("retrigger", Control)],
        vec![port("out", Control)],
        vec![
            param("gate", "Gate", "", 0., 1., 0.),
            param("attack", "Attack", "ms", 0., 10000., 10.),
            param("decay", "Decay", "ms", 0., 10000., 100.),
            param("sustain", "Sustain", "", 0., 1., 0.7),
            param("release", "Release", "ms", 0., 10000., 200.),
        ],
        &["adsr", "vline", "envelope"],
    );
    add(
        "gate",
        "Control gate",
        "⊣",
        "Control",
        "Pass a control value only when open.",
        vec![port("in", Control)],
        vec![port("out", Control)],
        vec![param("open", "Open", "", 0., 1., 1.)],
        &["spigot"],
    );
    for input in [true, false] {
        let structural = |mut p: Parameter| {
            p.structural = true;
            p
        };
        add(
            if input { "midi_input" } else { "midi_output" },
            if input { "MIDI input" } else { "MIDI output" },
            "♪",
            "External",
            if input {
                "Server MIDI Note or CC input. Select a port, message type and channel in the modal. Outputs number, value and a one-sample message trigger."
            } else {
                "Send MIDI notes or CC to a server port. Rising trigger sends number/value; falling trigger releases a Note. Values are rounded to 0–127."
            },
            if input {
                vec![]
            } else {
                vec![
                    port("number", Control),
                    port("value", Control),
                    port("trigger", Control),
                ]
            },
            if input {
                vec![
                    port("number", Control),
                    port("value", Control),
                    port("trigger", Control),
                ]
            } else {
                vec![]
            },
            vec![
                structural(param("mode", "Message type (0 Note, 1 CC)", "", 0., 1., 0.)),
                structural(param(
                    "channel",
                    "MIDI channel (0 all inputs)",
                    "",
                    if input { 0. } else { 1. },
                    16.,
                    1.,
                )),
            ],
            &[],
        );
        add(
            if input { "osc_input" } else { "osc_output" },
            if input { "OSC input" } else { "OSC output" },
            "↔",
            "External",
            if input {
                "Receive one numeric or text argument at an exact OSC address. Enable OSC reception in System settings."
            } else {
                "Send one control value to an OSC destination/address. With trigger connected, send on its rising edge; otherwise send changed values. Rate is bounded."
            },
            if input {
                vec![]
            } else {
                vec![port("in", Control), port("trigger", Control)]
            },
            if input {
                vec![port("out", Control)]
            } else {
                vec![]
            },
            if input {
                vec![structural(param("text", "Text mode", "", 0., 1., 0.))]
            } else {
                vec![structural(param(
                    "rate",
                    "Maximum message rate",
                    "Hz",
                    1.,
                    200.,
                    60.,
                ))]
            },
            &[],
        );
    }
    add(
        "control_input",
        "Graphical control",
        "▤",
        "Control",
        "Interactive graph control. A connected input is read-only and passes through unchanged. Unconnected Bang emits one engine-sample pulse; other modes use the stored literal.",
        vec![port("in", Control)],
        vec![port("out", Control)],
        vec![
            Parameter {
                structural: true,
                ..param("mode", "Control type", "", 0., 4., 2.)
            },
            Parameter {
                structural: true,
                ..param("min", "Minimum", "", -100000., 100000., -100000.)
            },
            Parameter {
                structural: true,
                ..param("max", "Maximum", "", -100000., 100000., 100000.)
            },
        ],
        &[],
    );
    add(
        "value",
        "Value",
        "ƒ",
        "Control",
        "Stored numeric value or constant.",
        vec![],
        vec![port("out", Control)],
        vec![param("value", "Value", "", -100000., 100000., 0.)],
        &["float", "int"],
    );
    add(
        "random",
        "Random",
        "⚄",
        "Control",
        "Deterministic seeded random on rising trigger.",
        vec![port("trigger", Control)],
        vec![port("out", Control)],
        vec![
            param("max", "Maximum", "", 1., 100000., 127.),
            param("seed", "Seed", "", 1., 100000., 1.),
        ],
        &["random"],
    );
    add(
        "lfo",
        "LFO",
        "∿",
        "Control",
        "Sine modulation with range and rate controls.",
        vec![],
        vec![port("out", Control)],
        vec![
            param("rate", "Rate", "Hz", 0.01, 50., 0.2),
            param("min", "Minimum", "", -100000., 100000., 0.),
            param("max", "Maximum", "", -100000., 100000., 1.),
        ],
        &["osc~"],
    );
    add(
        "input",
        "Audio input",
        "↳",
        "Audio",
        "Selected server interface input; channel index is zero-based.",
        vec![],
        vec![port("out", Audio)],
        vec![
            Parameter {
                structural: true,
                ..param("interface", "Input interface", "", 0., 999999999., 0.)
            },
            param("offset", "First channel", "", 0., 63., 0.),
        ],
        &["adc~"],
    );
    add(
        "browser_input",
        "Browser input",
        "◉",
        "Audio",
        "WebRTC microphone source assigned to a performer.",
        vec![],
        vec![port("out", Audio)],
        vec![],
        &[],
    );
    let mono_ports = || {
        (1..=8)
            .map(|i| {
                let mut p = port(&format!("ch_{i}"), Audio);
                p.label = format!("Ch {i} · mono");
                p.fixed_channels = Some(1);
                p
            })
            .collect()
    };
    add(
        "channel_split",
        "Channel split",
        "⋔",
        "Audio",
        "Split a 1–8-channel bundle into mono ports. Ports above the bundle width emit silence.",
        vec![port("in", Audio)],
        mono_ports(),
        vec![],
        &["split", "unpack~"],
    );
    add(
        "channel_merge",
        "Channel merge",
        "⋏",
        "Audio",
        "Merge mono ports into a 1–8-channel bundle. Unconnected inputs are silent; inputs above the bundle width are ignored.",
        mono_ports(),
        vec![port("out", Audio)],
        vec![],
        &["merge", "pack~"],
    );
    let channel_routes = (1..=8)
        .map(|i| {
            param(
                &format!("output_{i}"),
                &format!("Output {i} source"),
                "channel",
                0.,
                8.,
                i as f64,
            )
        })
        .collect();
    add(
        "channel_map",
        "Channel map",
        "⇄",
        "Audio",
        "Reorder or duplicate channels within a 1–8 channel bundle. Source 0 or a source outside the bundle mutes that output. Source numbers are rounded down. Live changes crossfade over 5 ms.",
        vec![port("in", Audio)],
        vec![port("out", Audio)],
        channel_routes,
        &["route~", "matrix", "channel"],
    );
    add(
        "monitor_output",
        "Monitor output",
        "◉",
        "Audio",
        "Dedicated browser monitor feed; does not feed server speakers. Mono is duplicated to stereo; wider bundles use channels 1 and 2. Route a custom mix here and select it in Browser audio.",
        vec![port("in", Audio)],
        vec![],
        vec![param("gain", "Monitor gain", "dB", -90., 6., -12.)],
        &["monitor", "headphones"],
    );
    add(
        "output",
        "Audio output",
        "↗",
        "Audio",
        "Output to selected server interface.",
        vec![port("in", Audio)],
        vec![],
        vec![
            param("gain", "Output gain", "dB", -90., 6., -12.),
            Parameter {
                structural: true,
                ..param("interface", "Audio interface", "", 0., 999999999., 0.)
            },
        ],
        &["dac~"],
    );
    add(
        "gain",
        "Gain",
        "−/+",
        "Audio",
        "Smoothed multichannel gain.",
        vec![port("in", Audio)],
        vec![port("out", Audio)],
        vec![param("gain", "Gain", "dB", -90., 24., 0.)],
        &["*~"],
    );
    add(
        "mixer",
        "Mixer",
        "Σ",
        "Audio",
        "Sum two equal-width audio connections.",
        vec![port("a", Audio), port("b", Audio)],
        vec![port("out", Audio)],
        vec![param("gain", "Gain", "dB", -90., 12., -6.)],
        &["+~"],
    );
    add(
        "control_visualizer",
        "Control visualizer",
        "123",
        "Control",
        "Passes numeric or UTF-8 string control data through unchanged. Disconnected nodes use the input value set in the modal.",
        vec![port("in", Control)],
        vec![port("out", Control)],
        vec![],
        &["visualizer", "display", "message"],
    );
    add(
        "audio_visualizer",
        "Audio visualizer",
        "▥",
        "Audio",
        "Transparent multichannel audio pass-through with block-rate spectral analysis and a spectrogram. Display updates are throttled; audio is unchanged.",
        vec![port("in", Audio)],
        vec![port("out", Audio)],
        vec![Parameter {
            structural: true,
            ..param("size", "Analysis FFT size", "samples", 256., 8192., 1024.)
        }],
        &["visualizer", "spectrogram", "scope"],
    );
    add(
        "spectral_visualizer",
        "Spectral visualizer",
        "▤",
        "Spectral",
        "Transparent spectral-frame pass-through. Shows a spectrogram, FFT-bin magnitudes, and phase in radians for every channel; Cartesian and polar data are preserved.",
        vec![port("in", Spectral)],
        vec![port("out", Spectral)],
        vec![
            Parameter {
                structural: true,
                ..param("size", "FFT size", "samples", 256., 8192., 1024.)
            },
            Parameter {
                structural: true,
                ..param("overlap", "Overlap", "", 2., 4., 4.)
            },
        ],
        &["visualizer", "fft", "phase"],
    );
    add(
        "audio_to_control",
        "Audio to control",
        "∿→#",
        "Audio",
        "Converts the first audio channel to a numeric control value every sample: out = input × scale + offset. For FM, set scale to deviation in Hz and offset to carrier frequency; connect out to oscillator Frequency. Use Channel map to select another channel.",
        vec![port("in", Audio)],
        vec![port("out", Control)],
        vec![
            param("scale", "Scale", "", -100000., 100000., 1.),
            param("offset", "Offset", "", -100000., 100000., 0.),
        ],
        &["signal to control", "FM", "HFO", "audio rate"],
    );
    add(
        "oscillator",
        "Oscillator",
        "∿",
        "Audio",
        "Sine, triangle, sawtooth, square, or noise. Manual frequency edits are smoothed; connected frequency follows every sample for FM (0–20000 Hz). Sawtooth and square edges use band-limiting.",
        vec![],
        vec![port("out", Audio)],
        vec![
            param("frequency", "Frequency", "Hz", 0., 20000., 220.),
            param("waveform", "Waveform", "", 0., 4., 0.),
            param("amplitude", "Amplitude", "", 0., 1., 0.2),
        ],
        &["osc~"],
    );
    add(
        "synth",
        "Polyphonic synth",
        "♪",
        "Audio",
        "64-voice sine instrument driven by scheduled part notes.",
        vec![],
        vec![port("out", Audio)],
        vec![
            param("amplitude", "Amplitude", "", 0., 1., 0.2),
            param("release", "Release", "ms", 1., 2000., 80.),
        ],
        &["poly"],
    );
    add(
        "noise",
        "Noise",
        "≋",
        "Audio",
        "Deterministic white noise generator.",
        vec![],
        vec![port("out", Audio)],
        vec![param("amplitude", "Amplitude", "", 0., 1., 0.1)],
        &["noise~"],
    );
    add(
        "envelope",
        "Envelope follower",
        "⌁",
        "Control",
        "Peak envelope of multichannel input.",
        vec![port("in", Audio)],
        vec![port("out", Control)],
        vec![param("release", "Release", "ms", 1., 2000., 100.)],
        &["env~"],
    );
    for (kind, label) in [("lowpass", "Low-pass"), ("highpass", "High-pass")] {
        let mut poles = param("poles", "Poles (1, 2, 3, 4, 5)", "", 1., 5., 3.);
        poles.structural = true;
        add(
            kind,
            label,
            "⌁",
            "Filter",
            "Butterworth response; supports true third- and fifth-order cascades.",
            vec![port("in", Audio)],
            vec![port("out", Audio)],
            vec![param("cutoff", "Cutoff", "Hz", 10., 20000., 1000.), poles],
            &[if kind == "lowpass" { "lop~" } else { "hip~" }],
        );
    }
    for (kind, label) in [
        ("compressor", "Compressor"),
        ("noise_gate", "Noise gate"),
        ("limiter", "Limiter"),
        ("expander", "Expander"),
    ] {
        add(
            kind,
            label,
            "⋈",
            "Dynamics",
            "Linked-channel dynamics with optional external sidechain.",
            vec![port("in", Audio), port("sidechain", Audio)],
            vec![port("out", Audio)],
            vec![
                param("threshold", "Threshold", "dB", -90., 0., -24.),
                param("ratio", "Ratio", ":1", 1., 30., 4.),
                param("attack", "Attack", "ms", 0.1, 500., 10.),
                param("release", "Release", "ms", 1., 3000., 150.),
                param("makeup", "Makeup", "dB", 0., 24., 0.),
            ],
            &[],
        );
    }
    add(
        "delay",
        "Delay",
        "↶",
        "Effect",
        "Bounded delay line with feedback and wet/dry mix.",
        vec![port("in", Audio)],
        vec![port("out", Audio)],
        vec![
            param("time", "Time", "ms", 1., 2000., 250.),
            param("feedback", "Feedback", "", 0., 0.95, 0.3),
            param("mix", "Mix", "", 0., 1., 0.3),
        ],
        &["delread~", "delwrite~"],
    );
    add(
        "meter",
        "Meter",
        "▥",
        "Audio",
        "Pass-through audio with peak telemetry.",
        vec![port("in", Audio)],
        vec![port("out", Audio)],
        vec![],
        &["env~"],
    );
    for (kind, label) in [("eq3", "3-band EQ"), ("eq5", "5-band EQ")] {
        let mut parameters = Vec::new();
        let frequencies = if kind == "eq3" {
            vec![100., 1000., 8000.]
        } else {
            vec![80., 300., 1000., 3500., 10000.]
        };
        for (i, hz) in frequencies.iter().enumerate() {
            parameters.push(param(
                &format!("frequency_{i}"),
                &format!("Band {} frequency", i + 1),
                "Hz",
                20.,
                20000.,
                *hz,
            ));
            parameters.push(param(
                &format!("gain_{i}"),
                &format!("Band {} gain", i + 1),
                "dB",
                -24.,
                24.,
                0.,
            ));
            parameters.push(param(
                &format!("q_{i}"),
                &format!("Band {} Q", i + 1),
                "",
                0.1,
                20.,
                1.,
            ));
        }
        add(
            kind,
            label,
            "≋",
            "Filter",
            "Parametric peaking EQ. Bands have independent frequency, gain and Q.",
            vec![port("in", Audio)],
            vec![port("out", Audio)],
            parameters,
            &[],
        );
    }
    for (kind, label) in [("bandpass", "Band-pass"), ("notch", "Notch")] {
        add(
            kind,
            label,
            "⌁",
            "Filter",
            "Two-pole filter with adjustable center frequency and Q.",
            vec![port("in", Audio)],
            vec![port("out", Audio)],
            vec![
                param("cutoff", "Frequency", "Hz", 10., 20000., 1000.),
                param("q", "Q", "", 0.1, 20., 1.),
            ],
            &["bp~"],
        );
    }
    add(
        "vocoder",
        "Vocoder",
        "⋈",
        "Effect",
        "Sixteen-band carrier/modulator vocoder. Both inputs use matching channel widths.",
        vec![port("carrier", Audio), port("modulator", Audio)],
        vec![port("out", Audio)],
        vec![
            param("attack", "Attack", "ms", 0.1, 500., 10.),
            param("release", "Release", "ms", 1., 3000., 100.),
            param("mix", "Mix", "", 0., 1., 1.),
        ],
        &[],
    );
    add(
        "pan",
        "Stereo pan",
        "↔",
        "Audio",
        "Constant-power stereo placement; first two channels are panned, other channels pass through.",
        vec![port("in", Audio)],
        vec![port("out", Audio)],
        vec![param("pan", "Pan", "", -1., 1., 0.)],
        &[],
    );
    add(
        "crossfade",
        "Crossfade",
        "⇄",
        "Audio",
        "Equal-power crossfade between two audio inputs.",
        vec![port("a", Audio), port("b", Audio)],
        vec![port("out", Audio)],
        vec![param("mix", "Mix", "", 0., 1., 0.5)],
        &[],
    );
    add(
        "audio_gate",
        "Audio gate",
        "⊣",
        "Audio",
        "Smoothed audio gate controlled by a numeric signal.",
        vec![port("in", Audio)],
        vec![port("out", Audio)],
        vec![param("open", "Open", "", 0., 1., 1.)],
        &["*~"],
    );
    add(
        "reverb",
        "Reverb",
        "≋",
        "Effect",
        "Four parallel damped feedback combs with bounded decay.",
        vec![port("in", Audio)],
        vec![port("out", Audio)],
        vec![
            param("decay", "Decay", "", 0., 0.95, 0.7),
            param("mix", "Mix", "", 0., 1., 0.25),
        ],
        &[],
    );
    add(
        "trigger",
        "Trigger",
        "!",
        "Control",
        "Single-sample trigger on a rising edge.",
        vec![port("in", Control)],
        vec![port("out", Control)],
        vec![],
        &["bang", "trigger"],
    );
    add(
        "change",
        "Change",
        "≠",
        "Control",
        "Single-sample trigger when input changes.",
        vec![port("in", Control)],
        vec![port("out", Control)],
        vec![],
        &["change"],
    );
    add(
        "select",
        "Select",
        "= ?",
        "Control",
        "Trigger when input equals the selected value.",
        vec![port("in", Control)],
        vec![port("out", Control)],
        vec![param("value", "Match", "", -100000., 100000., 0.)],
        &["select", "sel"],
    );
    add(
        "sample_hold",
        "Sample & hold",
        "▔",
        "Control",
        "Capture input value at a rising trigger.",
        vec![port("in", Control), port("trigger", Control)],
        vec![port("out", Control)],
        vec![],
        &["samphold~"],
    );
    add(
        "ramp",
        "Ramp",
        "╱",
        "Control",
        "Slew continuously toward a target over an adjustable time.",
        vec![],
        vec![port("out", Control)],
        vec![
            param("target", "Target", "", -100000., 100000., 0.),
            param("time", "Time", "ms", 1., 10000., 100.),
        ],
        &["line"],
    );
    for (kind, label, input, output) in [
        ("fft", "FFT", Audio, Spectral),
        ("rfft", "Real FFT", Audio, Spectral),
        ("ifft", "Inverse FFT", Spectral, Audio),
        ("rifft", "Inverse real FFT", Spectral, Audio),
        ("spectral_gain", "Spectral gain", Spectral, Spectral),
        ("spectral_math", "Spectral math", Spectral, Spectral),
        ("spectral_curve", "Spectral curve", Spectral, Spectral),
        ("to_polar", "Magnitude / phase", Spectral, Spectral),
        ("to_cartesian", "Real / imaginary", Spectral, Spectral),
    ] {
        let mut size = param("size", "FFT size", "samples", 256., 8192., 1024.);
        size.structural = true;
        let mut overlap = param("overlap", "Overlap", "", 2., 4., 4.);
        overlap.structural = true;
        let mut parameters = vec![size, overlap];
        if kind == "spectral_gain" {
            parameters.push(param("gain", "Gain", "", 0., 10., 1.));
        }
        if kind == "spectral_math" {
            parameters.extend([
                param("magnitude_scale", "Magnitude multiply", "×", 0., 16., 1.),
                param("magnitude_offset", "Magnitude add", "", -1000., 1000., 0.),
                param("phase_scale", "Phase multiply", "×", -16., 16., 1.),
                param(
                    "phase_offset",
                    "Phase add",
                    "rad",
                    -std::f64::consts::PI,
                    std::f64::consts::PI,
                    0.,
                ),
            ]);
        }
        if kind == "spectral_curve" {
            for (prefix, min, max, default) in [
                ("magnitude_curve", 0., 2., 1.),
                (
                    "phase_curve",
                    -std::f64::consts::PI,
                    std::f64::consts::PI,
                    0.,
                ),
            ] {
                for i in 0..33 {
                    parameters.push(Parameter {
                        structural: true,
                        ..param(
                            &format!("{prefix}_{i}"),
                            &format!("{prefix} point {i}"),
                            "",
                            min,
                            max,
                            default,
                        )
                    });
                }
            }
        }
        let description = match kind {
            "spectral_math" => {
                "Per-bin magnitude and phase: multiply then add. Negative magnitudes clamp to zero; phase wraps. Negative-frequency bins mirror the positive bins for real audio; DC/Nyquist retain phase. Accepts Cartesian or polar frames. Match FFT size, overlap and channels."
            }
            "spectral_curve" => {
                "Draw 33-point curves from DC to Nyquist: multiply magnitudes (0–2×), add phase (−π…π). Linear interpolation per bin, shared by all channels. Negative-frequency bins mirror for real audio; DC/Nyquist retain phase. Match FFT size, overlap and channels."
            }
            _ => {
                "Windowed spectral frames. Connected spectral nodes must share FFT size, overlap and channel width."
            }
        };
        add(
            kind,
            label,
            "▥",
            "Spectral",
            description,
            vec![port("in", input)],
            vec![port("out", output)],
            parameters,
            &[],
        );
    }
    for (kind, label) in [
        ("sample", "Sample player"),
        ("phase_vocoder", "Phase vocoder"),
    ] {
        let mut asset = param("asset", "Sample ID", "", 0., 1000000000., 0.);
        asset.structural = true;
        let mut parameters = vec![
            asset,
            param("amplitude", "Amplitude", "", 0., 1., 0.5),
            param("loop", "Loop", "", 0., 1., 1.),
        ];
        if kind == "phase_vocoder" {
            let mut speed = param("speed", "Playback speed", "×", 0.25, 4., 1.);
            speed.structural = true;
            let mut pitch = param("pitch", "Pitch shift", "semitones", -24., 24., 0.);
            pitch.structural = true;
            parameters.push(speed);
            parameters.push(pitch);
        }
        add(
            kind,
            label,
            "▶",
            "Audio",
            "Upload a WAV file in the node modal. Sample preparation happens before graph activation.",
            vec![],
            vec![port("out", Audio)],
            parameters,
            &["tabplay~"],
        );
    }
    result
}

impl Graph {
    /// Return a stable topological schedule after validating all port contracts.
    pub fn validate(&self) -> Result<Vec<usize>, String> {
        self.flatten()?.validate_flat()
    }
    /// Validate and resolve subgraph ports without recursion or runtime containers.
    pub fn flatten(&self) -> Result<Graph, String> {
        // Validate metadata and limits before resolving any references.
        let mut metadata = self.clone();
        metadata.edges.clear();
        metadata.validate_flat()?;
        let nodes: BTreeMap<_, _> = self.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
        for node in &self.nodes {
            if let Some(reference) = &node.library {
                if node.kind != "subgraph"
                    || reference.id.is_empty()
                    || reference.id.len() > 128
                    || reference.version == 0
                {
                    return Err("Invalid subgraph library reference".into());
                }
            }
            if node.kind.starts_with("subgraph_") && node.parent.is_none() {
                return Err("Subgraph input/output nodes belong inside a subgraph".into());
            }
            if node.label.trim().is_empty() || node.label.len() > 256 {
                return Err("Node names must be 1–256 bytes".into());
            }
            let mut parent = node.parent.as_deref();
            let mut seen = BTreeSet::from([node.id.as_str()]);
            while let Some(id) = parent {
                if !seen.insert(id) {
                    return Err("Subgraph containment cannot contain cycles".into());
                }
                let container = nodes.get(id).ok_or("Missing parent subgraph")?;
                if container.kind != "subgraph" {
                    return Err("Parent must be a subgraph".into());
                }
                parent = container.parent.as_deref();
            }
        }
        let mut flat = self.clone();
        flat.nodes.retain(|n| n.kind != "subgraph");
        for n in &mut flat.nodes {
            n.parent = None;
        }
        for edge in &mut flat.edges {
            let source = nodes
                .get(edge.source.as_str())
                .ok_or("Missing source node")?;
            let target = nodes
                .get(edge.target.as_str())
                .ok_or("Missing target node")?;
            if source.parent != target.parent {
                return Err("Connections must stay in one graph; use subgraph ports".into());
            }
            if source.kind.starts_with("subgraph_output_")
                || target.kind.starts_with("subgraph_input_")
            {
                return Err("Subgraph boundary port points in the wrong direction".into());
            }
            if source.kind == "subgraph" {
                let port = nodes
                    .get(edge.source_port.as_str())
                    .ok_or("Missing subgraph output port")?;
                if port.parent.as_deref() != Some(source.id.as_str())
                    || !port.kind.starts_with("subgraph_output_")
                {
                    return Err("Invalid subgraph output port".into());
                }
                edge.source = port.id.clone();
                edge.source_port = "out".into();
            }
            if target.kind == "subgraph" {
                let port = nodes
                    .get(edge.target_port.as_str())
                    .ok_or("Missing subgraph input port")?;
                if port.parent.as_deref() != Some(target.id.as_str())
                    || !port.kind.starts_with("subgraph_input_")
                {
                    return Err("Invalid subgraph input port".into());
                }
                edge.target = port.id.clone();
                edge.target_port = "in".into();
            }
        }
        // Validate actual widths, FFT formats, strings, drivers and cycles across every boundary.
        flat.validate_flat()?;
        Ok(flat)
    }
    pub fn validate_flat(&self) -> Result<Vec<usize>, String> {
        if self.nodes.len() > 256 || self.edges.len() > 2048 {
            return Err("Graph exceeds 256 nodes / 2048 edges".into());
        }
        if self
            .nodes
            .iter()
            .filter(|n| n.kind == "monitor_output")
            .count()
            > 32
        {
            return Err("At most 32 monitor outputs are supported".into());
        }
        let descriptors = catalog();
        let mut ids = BTreeMap::new();
        for (i, n) in self.nodes.iter().enumerate() {
            if ids.insert(n.id.clone(), i).is_some() {
                return Err("Duplicate node ID".into());
            }
            if !(1..=MAX_CHANNELS).contains(&n.channels) {
                return Err("Audio width must be 1–8 channels".into());
            }
            if !n.x.is_finite() || !n.y.is_finite() {
                return Err("Invalid node position".into());
            }
            let d = descriptors
                .iter()
                .find(|d| d.kind == n.kind)
                .ok_or(format!("Unknown node {}", n.kind))?;
            if let Some(io) = &n.io {
                if !matches!(
                    n.kind.as_str(),
                    "midi_input" | "midi_output" | "osc_input" | "osc_output"
                ) {
                    return Err("I/O routes belong to MIDI/OSC nodes".into());
                }
                if io.port.len() > 256 || io.port.chars().any(char::is_control) {
                    return Err("Invalid MIDI port name".into());
                }
                if !io.address.is_empty()
                    && (!io.address.starts_with('/')
                        || io.address.len() > 256
                        || io
                            .address
                            .chars()
                            .any(|c| c.is_whitespace() || "*?[]{}#,\\".contains(c)))
                {
                    return Err(
                        "Choose a literal OSC address starting with / (at most 256 bytes)".into(),
                    );
                }
                if !io.destination.is_empty()
                    && io
                        .destination
                        .parse::<std::net::SocketAddr>()
                        .map_or(true, |a| !a.is_ipv4() || a.port() == 0)
                {
                    return Err("OSC node destination must be IPv4:port with a nonzero port".into());
                }
            }
            if matches!(n.kind.as_str(), "midi_input" | "midi_output")
                && n.parameters
                    .iter()
                    .any(|(k, v)| (k == "mode" || k == "channel") && v.fract() != 0.)
            {
                return Err("MIDI mode/channel must be whole numbers".into());
            }
            if n.kind == "osc_input" && n.parameters.get("text").is_some_and(|v| v.fract() != 0.) {
                return Err("Choose numeric or text OSC input".into());
            }
            if let Some(value) = &n.control_value {
                if !matches!(n.kind.as_str(), "control_visualizer" | "control_input") {
                    return Err(
                        "Input literals belong to graphical controls or control visualizers".into(),
                    );
                }
                match value {
                    ControlValue::Number(v) if !v.is_finite() => {
                        return Err("Control numbers must be finite".into());
                    }
                    ControlValue::Text(v) if v.len() > MAX_CONTROL_TEXT_BYTES => {
                        return Err("Control strings must be at most 256 UTF-8 bytes".into());
                    }
                    _ => {}
                }
            }
            if n.kind == "control_input" {
                let mode = n.parameters.get("mode").copied().unwrap_or(2.);
                let min = n.parameters.get("min").copied().unwrap_or(-100000.);
                let max = n.parameters.get("max").copied().unwrap_or(100000.);
                if mode.fract() != 0.
                    || min > max
                    || (mode == 1. && (min.fract() != 0. || max.fract() != 0.))
                {
                    return Err("Choose a control type and valid minimum/maximum (whole numbers for Integer)".into());
                }
                match &n.control_value {
                    Some(ControlValue::Text(_)) if mode != 4. => {
                        return Err("Only Text mode accepts a text literal".into());
                    }
                    Some(ControlValue::Number(v))
                        if mode == 4.
                            || (mode != 0.
                                && (*v < min || *v > max || (mode == 1. && v.fract() != 0.))) =>
                    {
                        return Err(
                            "Manual control value must match its type and minimum/maximum".into(),
                        );
                    }
                    None if mode != 0. && mode != 4. && !(min..=max).contains(&0.) => {
                        return Err("Set a manual value inside the selected range".into());
                    }
                    _ => {}
                }
            }
            if d.category == "Spectral" || n.kind == "audio_visualizer" {
                let size = n.parameters.get("size").copied().unwrap_or(1024.);
                let overlap = n.parameters.get("overlap").copied().unwrap_or(4.);
                if size.fract() != 0.
                    || !(size as usize).is_power_of_two()
                    || ![2., 4.].contains(&overlap)
                {
                    return Err("FFT size must be a power of two; overlap must be 2 or 4".into());
                }
            }
            for (key, value) in &n.parameters {
                let p = d
                    .parameters
                    .iter()
                    .find(|p| &p.id == key)
                    .ok_or(format!("Unknown parameter {key}"))?;
                if n.kind == "oscillator" && key == "waveform" && value.fract() != 0. {
                    return Err("Waveform must be an integer from 0 to 4".into());
                }
                if !value.is_finite() || *value < p.min || *value > p.max {
                    return Err(format!(
                        "{} must be between {} and {}",
                        p.label, p.min, p.max
                    ));
                }
                if p.id == "poles" && value.fract() != 0. {
                    return Err("Pole count must be an integer".into());
                }
            }
        }
        if self
            .edges
            .iter()
            .filter(|e| {
                e.target_port == "tempo"
                    && self
                        .nodes
                        .iter()
                        .any(|n| n.id == e.target && n.kind == "clock")
            })
            .count()
            > 1
        {
            return Err("Only one global clock tempo input may be connected".into());
        }
        let mut indegree = vec![0; self.nodes.len()];
        let mut outgoing = vec![vec![]; self.nodes.len()];
        let mut occupied = BTreeSet::new();
        let mut edge_ids = BTreeSet::new();
        for edge in &self.edges {
            if !edge_ids.insert(&edge.id) {
                return Err("Duplicate edge ID".into());
            }
            let s = *ids.get(&edge.source).ok_or("Missing source node")?;
            let t = *ids.get(&edge.target).ok_or("Missing target node")?;
            let sd = descriptors
                .iter()
                .find(|d| d.kind == self.nodes[s].kind)
                .unwrap();
            let td = descriptors
                .iter()
                .find(|d| d.kind == self.nodes[t].kind)
                .unwrap();
            let signal = sd
                .outputs
                .iter()
                .find(|p| p.id == edge.source_port)
                .ok_or("Unknown source port")?
                .signal;
            let target_signal = if td
                .parameters
                .iter()
                .any(|p| p.id == edge.target_port && !p.structural)
            {
                Signal::Control
            } else {
                td.inputs
                    .iter()
                    .find(|p| p.id == edge.target_port)
                    .ok_or("Unknown target port")?
                    .signal
            };
            if signal != target_signal {
                return Err("Signal types do not match".into());
            }
            if signal == Signal::Spectral {
                for key in ["size", "overlap"] {
                    let default = if key == "size" { 1024. } else { 4. };
                    if self.nodes[s]
                        .parameters
                        .get(key)
                        .copied()
                        .unwrap_or(default)
                        != self.nodes[t]
                            .parameters
                            .get(key)
                            .copied()
                            .unwrap_or(default)
                    {
                        return Err("Spectral frame configurations do not match".into());
                    }
                }
                if self.nodes[s].channels != self.nodes[t].channels {
                    return Err("Spectral channel widths do not match".into());
                }
            }
            if signal == Signal::Audio
                && sd
                    .outputs
                    .iter()
                    .find(|p| p.id == edge.source_port)
                    .unwrap()
                    .fixed_channels
                    .unwrap_or(self.nodes[s].channels)
                    != td
                        .inputs
                        .iter()
                        .find(|p| p.id == edge.target_port)
                        .unwrap()
                        .fixed_channels
                        .unwrap_or(self.nodes[t].channels)
            {
                return Err("Audio channel widths do not match".into());
            }
            if !occupied.insert((t, edge.target_port.clone())) {
                return Err("Input already has a driver; use a mixer or math node".into());
            }
            outgoing[s].push(t);
            indegree[t] += 1;
        }
        let mut ready: BTreeSet<usize> = indegree
            .iter()
            .enumerate()
            .filter_map(|(i, n)| (*n == 0).then_some(i))
            .collect();
        let mut order = Vec::new();
        while let Some(i) = ready.pop_first() {
            order.push(i);
            for &t in &outgoing[i] {
                indegree[t] -= 1;
                if indegree[t] == 0 {
                    ready.insert(t);
                }
            }
        }
        if order.len() != self.nodes.len() {
            return Err("Cycles require an explicit feedback scheduler; this engine currently accepts acyclic graphs".into());
        }
        if self
            .nodes
            .iter()
            .filter(|n| n.kind.ends_with("_visualizer"))
            .count()
            > 16
        {
            return Err("At most 16 visualizers per graph".into());
        }
        let mut text = vec![false; self.nodes.len()];
        for &i in &order {
            if matches!(
                self.nodes[i].kind.as_str(),
                "control_visualizer" | "control_input" | "osc_input"
            ) || (self.nodes[i].kind.starts_with("subgraph_")
                && self.nodes[i].kind.ends_with("_control"))
            {
                text[i] = if let Some(edge) = self
                    .edges
                    .iter()
                    .find(|e| e.target == self.nodes[i].id && e.target_port == "in")
                {
                    text[*ids.get(&edge.source).unwrap()]
                } else {
                    matches!(self.nodes[i].control_value, Some(ControlValue::Text(_)))
                        || (self.nodes[i].kind == "osc_input"
                            && self.nodes[i].parameters.get("text") == Some(&1.))
                        || (self.nodes[i].kind == "control_input"
                            && self.nodes[i].parameters.get("mode") == Some(&4.))
                };
            }
        }
        for edge in &self.edges {
            let source = *ids.get(&edge.source).unwrap();
            let target = *ids.get(&edge.target).unwrap();
            if text[source]
                && !matches!(
                    self.nodes[target].kind.as_str(),
                    "control_visualizer" | "control_input" | "osc_output"
                )
                && !(self.nodes[target].kind.starts_with("subgraph_")
                    && self.nodes[target].kind.ends_with("_control"))
            {
                return Err(
                    "String control data cannot connect to a numeric-only input or parameter"
                        .into(),
                );
            }
        }
        Ok(order)
    }
}

impl Project {
    /// Musical positions and BPM use quarter notes regardless of meter.
    pub fn quarter_beats_per_bar(&self) -> f64 {
        self.beats_per_bar as f64 * 4. / self.beat_unit as f64
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != SCHEMA_VERSION {
            return Err("Unsupported project schema".into());
        }
        if self.name.trim().is_empty() || self.name.len() > 120 {
            return Err("Project name must be 1–120 characters".into());
        }
        if !self.bpm.is_finite()
            || !(1.0..=400.0).contains(&self.bpm)
            || !(1..=16).contains(&self.beats_per_bar)
            || ![1, 2, 4, 8, 16, 32].contains(&self.beat_unit)
        {
            return Err("Invalid tempo or meter".into());
        }
        if self.parts.len() > 32 {
            return Err("At most 32 parts".into());
        }
        let mut ids = BTreeSet::new();
        for p in &self.parts {
            if p.name.trim().is_empty() || p.name.len() > 120 {
                return Err("Part name must contain 1–120 bytes".into());
            }
            if !["notation", "grid"].contains(&p.view.as_str()) {
                return Err("Part display must be notation or grid".into());
            }
            if !(1..=16).contains(&p.midi_channel)
                || p.midi_port
                    .as_ref()
                    .is_some_and(|port| port.is_empty() || port.len() > 256)
            {
                return Err(
                    "MIDI channel must be 1–16 and port names must contain 1–256 bytes".into(),
                );
            }
            if !p.osc_address.starts_with('/')
                || p.osc_address.len() > 256
                || p.osc_address
                    .chars()
                    .any(|c| c.is_control() || c.is_whitespace() || "#*,?[]{}".contains(c))
            {
                return Err(
                    "OSC address must be a literal slash-prefixed path of at most 256 bytes".into(),
                );
            }
            if p.osc_destination.as_ref().is_some_and(|d| {
                d.parse::<std::net::SocketAddr>()
                    .map(|a| a.port() == 0)
                    .unwrap_or(true)
            }) {
                return Err("OSC destination must be a numeric IP:port with a nonzero port".into());
            }
            if !["treble", "bass", "alto", "tenor"].contains(&p.clef.as_str())
                || p.key_signature.as_ref().is_some_and(|k| {
                    ![
                        "Cb", "Gb", "Db", "Ab", "Eb", "Bb", "F", "C", "G", "D", "A", "E", "B",
                        "F#", "C#",
                    ]
                    .contains(&k.as_str())
                })
            {
                return Err("Unsupported clef or key signature".into());
            }
            if !ids.insert(&p.id)
                || p.notes.len() > 10000
                || !p.loop_beats.is_finite()
                || !(0.25..=4096.).contains(&p.loop_beats)
            {
                return Err("Invalid part".into());
            }
            for n in &p.notes {
                if n.pitch > 127
                    || n.velocity > 127
                    || !n.beat.is_finite()
                    || n.beat < 0.
                    || !n.duration.is_finite()
                    || n.duration <= 0.
                {
                    return Err("Invalid note".into());
                }
            }
        }
        self.graph.validate()?;
        Ok(())
    }
}

pub fn demo_project(id: String, name: String, mode: Mode) -> Project {
    let catalog = catalog();
    let specs = [
        ("clock", "clock", 100., 100.),
        ("count", "counter", 350., 100.),
        ("mod", "modulo", 580., 100.),
        ("tone", "synth", 350., 380.),
        ("gain", "gain", 820., 380.),
        ("out", "output", 1100., 380.),
    ];
    let nodes = specs
        .iter()
        .map(|(id, kind, x, y)| {
            let d = catalog.iter().find(|d| d.kind == *kind).unwrap();
            Node {
                io: None,
                library: None,
                parent: None,
                id: id.to_string(),
                kind: kind.to_string(),
                label: d.label.clone(),
                x: *x,
                y: *y,
                channels: 2,
                control_value: None,
                parameters: d
                    .parameters
                    .iter()
                    .map(|p| {
                        (
                            p.id.clone(),
                            if *kind == "modulo" && p.id == "b" {
                                8.
                            } else {
                                p.default
                            },
                        )
                    })
                    .collect(),
            }
        })
        .collect();
    let edges = [
        ("clock", "tick", "count", "trigger"),
        ("count", "out", "mod", "a"),
        ("tone", "out", "gain", "in"),
        ("gain", "out", "out", "in"),
    ]
    .iter()
    .enumerate()
    .map(|(i, (s, sp, t, tp))| Edge {
        id: format!("edge-{i}"),
        source: s.to_string(),
        source_port: sp.to_string(),
        target: t.to_string(),
        target_port: tp.to_string(),
    })
    .collect();
    Project {
        schema_version: 1,
        id,
        name,
        mode,
        revision: 0,
        bpm: 120.,
        beats_per_bar: 4,
        beat_unit: 4,
        graph: Graph { nodes, edges },
        parts: vec![Part {
            id: "part-1".into(),
            name: "Prepared piano".into(),
            performer: None,
            view: "notation".into(),
            clef: "treble".into(),
            key_signature: None,
            show_time_signature: true,
            notes: [60, 64, 67, 72, 69, 67, 64, 62]
                .iter()
                .enumerate()
                .map(|(i, p)| Note {
                    id: format!("note-{i}"),
                    pitch: *p,
                    beat: i as f64,
                    duration: 0.75,
                    velocity: 90,
                    rest: false,
                    tied: false,
                })
                .collect(),
            loop_beats: 8.,
            instrument_node: Some("tone".into()),
            midi_port: None,
            midi_channel: 1,
            osc_destination: None,
            osc_address: default_osc(),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn control_text_contract_is_validated_through_visualizers() {
        let mut source = demo_project("x".into(), "x".into(), Mode::Structured)
            .graph
            .nodes
            .remove(0);
        source.kind = "control_visualizer".into();
        source.id = "source".into();
        source.parameters.clear();
        source.control_value = Some(ControlValue::Text("hello".into()));
        let mut target = source.clone();
        target.id = "target".into();
        target.kind = "add".into();
        target.control_value = None;
        let mut graph = Graph {
            nodes: vec![source, target],
            edges: vec![Edge {
                id: "line".into(),
                source: "source".into(),
                source_port: "out".into(),
                target: "target".into(),
                target_port: "a".into(),
            }],
        };
        assert!(graph.validate().unwrap_err().contains("String control"));
        graph.nodes[0].control_value = Some(ControlValue::Number(42.));
        assert!(graph.validate().is_ok());
        graph.nodes[0].control_value = Some(ControlValue::Text("x".repeat(257)));
        assert!(graph.validate().is_err());
    }
    #[test]
    fn external_routes_validate_and_legacy_midi_defaults_to_channel_one() {
        let mut p = demo_project("x".into(), "x".into(), Mode::Structured);
        p.parts[0].midi_channel = 16;
        p.parts[0].osc_destination = Some("[::1]:9000".into());
        p.parts[0].osc_address = "/touchdesigner/note".into();
        assert!(p.validate().is_ok());
        for channel in [0, 17] {
            p.parts[0].midi_channel = channel;
            assert!(p.validate().is_err());
        }
        p.parts[0].midi_channel = 1;
        for destination in ["not-an-address", "127.0.0.1:0"] {
            p.parts[0].osc_destination = Some(destination.into());
            assert!(p.validate().is_err());
        }
        p.parts[0].osc_destination = None;
        for address in ["note", "/note/*", "/note value"] {
            p.parts[0].osc_address = address.into();
            assert!(p.validate().is_err());
        }
        let mut json = serde_json::to_value(p).unwrap();
        json["parts"][0]
            .as_object_mut()
            .unwrap()
            .remove("midi_channel");
        let restored: Project = serde_json::from_value(json).unwrap();
        assert_eq!(restored.parts[0].midi_channel, 1);
    }
    #[test]
    fn meter_keeps_quarter_note_timing_and_legacy_default() {
        let mut p = demo_project("x".into(), "x".into(), Mode::Structured);
        p.beats_per_bar = 6;
        p.beat_unit = 8;
        assert!(p.validate().is_ok());
        assert_eq!(p.quarter_beats_per_bar(), 3.);
        p.beats_per_bar = 7;
        p.beat_unit = 16;
        assert_eq!(p.quarter_beats_per_bar(), 1.75);
        p.beat_unit = 3;
        assert!(p.validate().is_err());
        let mut json = serde_json::to_value(p).unwrap();
        json.as_object_mut().unwrap().remove("beat_unit");
        let restored: Project = serde_json::from_value(json).unwrap();
        assert_eq!(restored.beat_unit, 4);
    }
    #[test]
    fn notation_settings_validate_and_legacy_parts_get_defaults() {
        let mut p = demo_project("x".into(), "x".into(), Mode::Structured);
        p.parts[0].clef = "alto".into();
        p.parts[0].key_signature = Some("Eb".into());
        p.parts[0].show_time_signature = false;
        assert!(p.validate().is_ok());
        p.parts[0].key_signature = Some("invalid".into());
        assert!(p.validate().is_err());
        p.parts[0].key_signature = None;
        p.parts[0].clef = "invalid".into();
        assert!(p.validate().is_err());
        p.parts[0].clef = "treble".into();
        let mut json = serde_json::to_value(p).unwrap();
        let part = json["parts"][0].as_object_mut().unwrap();
        part.remove("key_signature");
        part.remove("show_time_signature");
        let restored: Project = serde_json::from_value(json).unwrap();
        assert!(restored.parts[0].key_signature.is_none());
        assert!(restored.parts[0].show_time_signature);
    }
    #[test]
    fn demo_validates() {
        demo_project("x".into(), "Demo".into(), Mode::Conducted)
            .validate()
            .unwrap();
    }
    #[test]
    fn graph_rejects_cycle_and_duplicate_driver() {
        let mut p = demo_project("x".into(), "x".into(), Mode::Freeform);
        p.graph.edges.push(Edge {
            id: "bad".into(),
            source: "mod".into(),
            source_port: "out".into(),
            target: "count".into(),
            target_port: "trigger".into(),
        });
        assert!(p.validate().is_err());
    }
    #[test]
    fn graph_rejects_audio_to_control() {
        let mut p = demo_project("x".into(), "x".into(), Mode::Freeform);
        p.graph.edges.push(Edge {
            id: "bad".into(),
            source: "tone".into(),
            source_port: "out".into(),
            target: "gain".into(),
            target_port: "gain".into(),
        });
        assert!(p.validate().is_err());
    }
    #[test]
    fn invalid_values_rejected() {
        let mut p = demo_project("x".into(), "x".into(), Mode::Freeform);
        p.graph.nodes[0].channels = 9;
        assert!(p.validate().is_err());
    }
    #[test]
    fn part_setup_validates_name_view_and_loop() {
        let base = demo_project("x".into(), "x".into(), Mode::Freeform);
        for name in ["".to_owned(), "   ".to_owned(), "x".repeat(121)] {
            let mut p = base.clone();
            p.parts[0].name = name;
            assert!(p.validate().is_err());
        }
        for view in ["notation", "grid", "unknown"] {
            let mut p = base.clone();
            p.parts[0].view = view.into();
            assert_eq!(p.validate().is_ok(), view != "unknown");
        }
        for length in [0., 0.25, 16., 4096., 4097., f64::NAN] {
            let mut p = base.clone();
            p.parts[0].loop_beats = length;
            assert_eq!(p.validate().is_ok(), (0.25..=4096.).contains(&length));
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpectrumChannel {
    pub magnitude: Vec<f32>,
    pub phase: Vec<f32>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Visualization {
    Control {
        value: ControlValue,
    },
    Audio {
        sequence: u64,
        size: usize,
        ready: bool,
        channels: Vec<SpectrumChannel>,
        history: Vec<String>,
        columns: usize,
    },
    Spectral {
        sequence: u64,
        generation: u64,
        size: usize,
        ready: bool,
        polar: bool,
        channels: Vec<SpectrumChannel>,
        history: Vec<String>,
        columns: usize,
    },
}
