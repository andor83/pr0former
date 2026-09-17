//! Versioned project model and graph validation. No device or UI dependencies.
pub mod documentation;
pub mod script;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Syntax-only validation; DNS resolution belongs to the server output worker.
pub fn valid_osc_node_destination(value: &str) -> bool {
    let Some((host, port)) = value.rsplit_once(':') else {
        return false;
    };
    if port.is_empty()
        || !port.bytes().all(|c| c.is_ascii_digit())
        || port.parse::<u16>().map_or(true, |p| p == 0)
    {
        return false;
    }
    if host.parse::<std::net::Ipv4Addr>().is_ok() {
        return true;
    }
    let host = host.strip_suffix('.').unwrap_or(host);
    !host.is_empty()
        && host.len() <= 253
        && !host.bytes().all(|c| c.is_ascii_digit() || c == b'.')
        && host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                && label
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
}

pub mod midi;
pub mod score;

pub const SCHEMA_VERSION: u32 = 1;
pub const SAMPLE_RATE: u32 = 48_000;
pub const MAX_CHANNELS: usize = 8;
/// Native device frames are separate from the 1–8-channel graph contract.
pub const MAX_DEVICE_CHANNELS: usize = 64;
pub const DEVICE_ROUTE_KEYS: [&str; MAX_CHANNELS] = [
    "route_1", "route_2", "route_3", "route_4", "route_5", "route_6", "route_7", "route_8",
];

/// -1 preserves legacy sequential routing, 0 disconnects, 1–64 select a device channel.
pub fn device_channel(route: f64, channel: usize, offset: usize) -> Option<usize> {
    let index = if route < 0. {
        channel + offset
    } else if route == 0. {
        return None;
    } else {
        route as usize - 1
    };
    (index < MAX_DEVICE_CHANNELS).then_some(index)
}

fn device_routes(mut parameters: Vec<Parameter>) -> Vec<Parameter> {
    parameters.extend(
        DEVICE_ROUTE_KEYS
            .iter()
            .enumerate()
            .map(|(i, key)| Parameter {
                structural: true,
                ..param(
                    key,
                    &format!("Signal {} physical channel", i + 1),
                    "",
                    -1.,
                    MAX_DEVICE_CHANNELS as f64,
                    -1.,
                )
            }),
    );
    parameters
}

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
    Midi,
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
    #[serde(default = "descriptor_channels")]
    pub default_channels: usize,
    pub kind: String,
    pub label: String,
    pub symbol: String,
    pub category: String,
    pub description: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub documentation: Option<documentation::NodeDocumentation>,
    pub aliases: Vec<String>,
    pub inputs: Vec<Port>,
    pub outputs: Vec<Port>,
    pub parameters: Vec<Parameter>,
}
fn descriptor_channels() -> usize {
    2
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
pub const MAX_SAMPLE_CHOICES: usize = 64;
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SampleChoice {
    pub asset: u32,
    pub name: String,
    #[serde(default)]
    pub nickname: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Node {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub script: Option<script::Script>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sample_choices: Vec<SampleChoice>,
    /// Source score part for part_midi / part_player, or performer association for monitor_output.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub part_id: Option<String>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notation: Option<score::Notation>,
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
    #[serde(default)]
    pub muted: bool,
    #[serde(default)]
    pub solo: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamics: Option<score::Dynamics>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub automation: Vec<score::AutomationLane>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub staves: Vec<score::Staff>,
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
    /// Part-local meter map used by conducted performer lanes. Written positions
    /// remain quarter-note beats; an empty map inherits the project meter.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub performance_meters: Vec<score::MeterChange>,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConductedSet {
    pub id: String,
    pub name: String,
    /// Canonical tile order. A part may appear in more than one set.
    #[serde(default)]
    pub parts: Vec<String>,
}

fn default_count_in_pulses() -> u8 {
    4
}

fn default_pulse_unit() -> u8 {
    4
}

/// One concrete MIDI device/channel/control mapped to a conducted UI target.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConductedMidiBinding {
    pub action: String,
    #[serde(default)]
    pub target: String,
    pub source: String,
    pub device: String,
    pub status: u8,
    pub control: u8,
}
impl ConductedMidiBinding {
    pub fn validate(&self, project: &Project) -> Result<(), String> {
        let target_ok = match self.action.as_str() {
            "part" | "arm" => project.parts.iter().any(|p| p.id == self.target),
            "set" => project.conducted.sets.iter().any(|s| s.id == self.target),
            "select_set" | "next_set" | "play" | "repeat" | "stop" | "dynamic" => self.target.is_empty(),
            _ => false,
        };
        if !target_ok || !matches!(self.source.as_str(), "local" | "server")
            || self.device.is_empty() || self.device.len() > 256
            || !matches!(self.status >> 4, 9 | 10 | 11 | 12 | 13 | 14) || self.control > 127
            || (matches!(self.status >> 4, 13 | 14) && self.control != 0)
            || (matches!(self.action.as_str(), "select_set" | "dynamic") && !matches!(self.status >> 4, 10 | 11 | 13 | 14))
        { return Err("Invalid conducted MIDI binding".into()); }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConductedLayout {
    #[serde(default = "default_count_in_pulses")]
    pub count_in_pulses: u8,
    /// Denominator of the constant conducting pulse.
    #[serde(default = "default_pulse_unit")]
    pub pulse_unit: u8,
    #[serde(default)]
    pub sets: Vec<ConductedSet>,
    #[serde(default)]
    pub midi_bindings: Vec<ConductedMidiBinding>,
}

impl Default for ConductedLayout {
    fn default() -> Self {
        Self {
            count_in_pulses: default_count_in_pulses(),
            pulse_unit: default_pulse_unit(),
            sets: Vec::new(),
            midi_bindings: Vec::new(),
        }
    }
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
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub local_audio_assignments: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score: Option<score::Timeline>,
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub mode: Mode,
    pub revision: u64,
    pub bpm: f64,
    pub beats_per_bar: u8,
    #[serde(default = "default_beat_unit")]
    pub beat_unit: u8,
    /// The designated performance conductor. Membership and assignment rules
    /// are validated by the server because they depend on ensemble records.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conductor: Option<String>,
    #[serde(default)]
    pub conducted: ConductedLayout,
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
fn envelope_parameters(defaults: [f64; 4]) -> Vec<Parameter> {
    [("attack", "Attack", "ms", 10000.), ("decay", "Decay", "ms", 10000.),
     ("sustain", "Sustain", "", 1.), ("release", "Release", "ms", 10000.)]
        .into_iter().zip(defaults).map(|((id,label,unit,max),value)| param(id,label,unit,0.,max,value)).collect()
}
/// Sample slots and live capture rings of the Granular Field node.
pub const GRANULAR_FIELD_SAMPLES: usize = 8;
pub const GRANULAR_FIELD_LIVE: usize = 2;
/// Every Granular Field setting is an ordinary (connectable) parameter: the node has
/// only two audio inputs, so sample setters and per-source positions live here rather
/// than as ports. Fresh slots sit on a circle so a new node is already a usable field.
fn granular_field_parameters() -> Vec<Parameter> {
    let mut p = vec![
        param("x", "Field X", "", -1., 1., 0.),
        param("y", "Field Y", "", -1., 1., 0.),
        param("focus", "Focus", "", 0.05, 2., 0.5),
        param("pitch", "Pitch", "semitones", -24., 24., 0.),
        param("randomize_pitch", "Randomize pitch", "", 0., 1., 0.),
        param("position", "Position", "", 0., 1., 0.5),
        param("spray", "Spray", "ms", 0., 500., 30.),
        param("grain_ms", "Grain duration", "ms", 5., 500., 60.),
        param("density", "Density", "grains/s", 1., 100., 30.),
        param("amplitude", "Amplitude", "", 0., 1., 0.5),
    ];
    const RING: [(f64, f64); GRANULAR_FIELD_SAMPLES] = [
        (0.7, 0.), (0.49, 0.49), (0., 0.7), (-0.49, 0.49), (-0.7, 0.), (-0.49, -0.49), (0., -0.7), (0.49, -0.49),
    ];
    for n in 1..=GRANULAR_FIELD_SAMPLES {
        let (x, y) = RING[n - 1];
        p.push(param(&format!("sample_{n}"), &format!("Sample {n} ID"), "", 0., 1000000000., 0.));
        p.push(param(&format!("source_{n}_x"), &format!("Sample {n} X"), "", -1., 1., x));
        p.push(param(&format!("source_{n}_y"), &format!("Sample {n} Y"), "", -1., 1., y));
        p.push(param(&format!("source_{n}_tune"), &format!("Sample {n} tune"), "semitones", -24., 24., 0.));
        p.push(param(&format!("source_{n}_gain"), &format!("Sample {n} gain"), "", 0., 1., 1.));
    }
    for (n, (x, y)) in [(0., 0.), (0.3, 0.3)].into_iter().enumerate() {
        let n = n + 1;
        p.push(param(&format!("live_{n}_x"), &format!("Live {n} X"), "", -1., 1., x));
        p.push(param(&format!("live_{n}_y"), &format!("Live {n} Y"), "", -1., 1., y));
        p.push(param(&format!("live_{n}_tune"), &format!("Live {n} tune"), "semitones", -24., 24., 0.));
        p.push(param(&format!("live_{n}_gain"), &format!("Live {n} gain"), "", 0., 1., 1.));
        p.push(param(&format!("live_{n}_buffer_ms"), &format!("Live {n} buffer"), "ms", 100., 10000., 500.));
    }
    p
}
fn port(id: &str, signal: Signal) -> Port {
    Port {
        id: id.into(),
        label: id.replace('_', " "),
        signal,
        fixed_channels: None,
    }
}
fn midi_port() -> Port {
    Port {
        id: "midi".into(),
        label: "MIDI".into(),
        signal: Signal::Midi,
        fixed_channels: None,
    }
}

pub fn named_route(kind: &str) -> bool {
    matches!(
        kind,
        "send_control"
            | "receive_control"
            | "send_audio"
            | "receive_audio"
            | "send_spectral"
            | "receive_spectral"
    )
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
            default_channels: if matches!(kind, "synth" | "fm_synth" | "looper") {
                1
            } else {
                2
            },
            kind: kind.into(),
            label: label.into(),
            symbol: symbol.into(),
            category: category.into(),
            description: description.into(),
            documentation: documentation::for_kind(kind),
            inputs,
            outputs,
            parameters,
            aliases: aliases.iter().map(|s| s.to_string()).collect(),
        });
    };
    add(
        "container",
        "Container",
        "▭",
        "Layout",
        "Organizational frame with no audio or control role. Pick a colour, write an explanation for its info hover, and drag nodes in: they snap to the frame's grid and the frame grows to fit. Resize from the bottom-right corner; moving the frame carries its nodes along.",
        vec![],
        vec![],
        vec![
            Parameter { structural: true, ..param("color", "Color", "", 0., 15., 0.) },
            Parameter { structural: true, ..param("width", "Width", "px", 280., 4000., 520.) },
            Parameter { structural: true, ..param("height", "Height", "px", 160., 4000., 320.) },
        ],
        &["group", "frame", "box", "comment", "note"],
    );
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
        ("midi", Midi),
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
            if kind == "atodb" {
                "Convert amplitude magnitude to dB, limited by minimum and maximum dB (default -90 to +6). Amplitude 1 is 0 dB; amplitudes above 1 produce positive dB. Reversed limits are ordered automatically."
            } else {
                "Single-input conversion."
            },
            vec![],
            vec![port("out", Control)],
            if kind == "atodb" {
                vec![
                    param("a", "Input", "", -100000., 100000., 0.),
                    param("min", "Minimum dB", "dB", -180., 100., -90.),
                    param("max", "Maximum dB", "dB", -180., 100., 6.),
                ]
            } else {
                vec![param("a", "Input", "", -100000., 100000., 0.)]
            },
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
        "Linearly map an input range to an output range. Defaults map 0–1 to -90–6 dB, matching output gain. Values outside the input range extrapolate; equal input endpoints produce the output minimum.",
        vec![],
        vec![port("out", Control)],
        vec![
            param("a", "Input", "", -100000., 100000., 0.),
            param("input_min", "Input minimum", "", -100000., 100000., 0.),
            param("input_max", "Input maximum", "", -100000., 100000., 1.),
            param("min", "Output minimum", "", -100000., 100000., -90.),
            param("max", "Output maximum", "", -100000., 100000., 6.),
        ],
        &[],
    );
    add(
        "clock",
        "Global clock",
        "◷",
        "Timing",
        "Authoritative project clock. Connect tempo to control project BPM (1–400), preserving beat phase. Disconnect to retain the last tempo. Runs freely while the graph is loaded; show Play snaps it to the show beat and Stop rewinds it to zero.",
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
        "Multiply/divide the project quarter-note pulse. Preserves phase through tempo and ratio edits. Runs while the graph is loaded; show Play realigns it to the show beat and Stop/rewind resets phase. Outputs pulse, fractional phase, and completed cycle count.",
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
        "Sample-timed linear ADSR, edge triggered: a rising gate or retrigger pulse starts or restarts the attack from the current level; a falling gate or rising note_off pulse releases from the current level, even if the gate parameter is left high. Pulse sources (Piano, Part MIDI, MIDI to control) wire trigger to retrigger and note_off to note_off. Attacks normally ramp from the current level; Retrigger from zero drops to 0 first and ramps the full attack every time. Stage times latch at entry; sustain edits are smoothed.",
        vec![port("retrigger", Control), port("note_off", Control)],
        vec![port("out", Control)],
        std::iter::once(param("gate", "Gate", "", 0., 1., 0.))
            .chain(envelope_parameters([10., 100., 0.7, 200.]))
            .chain(std::iter::once(Parameter { structural: true, ..param("reset", "Retrigger from zero", "", 0., 1., 0.) })).collect(),
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
    add(
        "part_midi",
        "Part MIDI",
        "♪",
        "Control",
        "Note events from only the assigned score part: MIDI pitch and velocity (0–127), gate while any notes are held, note-on trigger and note-off trigger. Pitch identifies the event, including releases. Chords are serialized at one event per two engine samples, with a low sample between pulses. Select the source part in this modal.",
        vec![],
        vec![
            port("pitch", Control),
            port("velocity", Control),
            port("gate", Control),
            port("trigger", Control),
            port("note_off", Control),
        ],
        vec![],
        &["score", "notes", "part input"],
    );
    add(
        "part_player", "Part player", "▶♪", "Control",
        "Play a selected score part independently of show transport, at the current tempo. Rising Play starts once on the next metronome beat; Play & repeat loops until Stop. Retriggering restarts on the next beat, without count-in. Connect MIDI to an instrument or MIDI output. Select the part in Options; bar and beat are live engine values.",
        vec![port("play", Control), Port { label: "Play & repeat".into(), ..port("repeat", Control) }, port("stop", Control)],
        vec![port("pitch", Control), port("velocity", Control), port("gate", Control), port("trigger", Control), port("note_off", Control)],
        vec![], &["score", "snippet", "algorithmic", "loop", "part playback"],
    );
    add(
        "convolution",
        "Convolution",
        "∗",
        "Audio",
        "Continuously convolve overlapping audio grains from A with the latest audio window from B as a changing impulse response. Channels convolve independently. Normalize limits each B window's absolute sum; Wet/dry delays the dry path to match the analysis window. This is a live granular effect, not a fixed long reverb impulse response.",
        vec![port("a", Audio), port("b", Audio)],
        vec![port("out", Audio)],
        vec![
            Parameter {
                structural: true,
                ..param("window", "Window size", "samples", 128., 2048., 256.)
            },
            param("normalize", "Normalize response", "", 0., 1., 1.),
            param("mix", "Wet/dry", "", 0., 1., 1.),
        ],
        &["live convolution", "cross synthesis"],
    );
    add(
        "convolution_reverb",
        "Convolution reverb",
        "∗",
        "Effect",
        "Apply a selected room response or noise sample to audio. Choose the impulse or room recording in the node options; the response is continuously windowed and mixed without blocking the engine.",
        vec![port("a", Audio)],
        vec![port("out", Audio)],
        vec![
            Parameter {
                structural: true,
                ..param("asset", "Sample ID", "", 0., 1000000000., 0.)
            },
            Parameter {
                structural: true,
                ..param("window", "Window size", "samples", 128., 2048., 256.)
            },
            param("normalize", "Normalize response", "", 0., 1., 1.),
            param("mix", "Wet/dry", "", 0., 1., 1.),
        ],
        &["IR reverb", "room reverb", "impulse response"],
    );
    add(
        "granular_synth",
        "Granular synth",
        "⋮",
        "Audio",
        "Sample-based granular instrument with 16 MIDI voices and 128 overlapping Hann grains. Connect pitch, velocity, gate, trigger and pitch-specific note_off from Piano, MIDI input or Part MIDI. Position selects a point in the sample; Spray scatters grain starts around it. Density is grains per second per held voice. The fixed grain pool steals grains at capacity.",
        ["pitch", "velocity", "gate", "trigger", "note_off"]
            .into_iter()
            .map(|id| port(id, Control))
            .chain(std::iter::once(midi_port()))
            .collect(),
        vec![port("out", Audio)],
        vec![
            Parameter {
                structural: true,
                ..param("asset", "Sample ID", "", 0., 1000000000., 0.)
            },
            param("root_note", "Root MIDI note", "", 0., 127., 60.),
            param("position", "Position", "", 0., 1., 0.5),
            param("spray", "Spray", "ms", 0., 500., 30.),
            param("grain_ms", "Grain duration", "ms", 5., 500., 60.),
            param("density", "Density", "grains/s", 1., 100., 30.),
            param("amplitude", "Amplitude", "", 0., 1., 0.5),
        ]
        .into_iter()
        .chain(envelope_parameters([0., 0., 1., 120.]))
        .collect(),
        &["granular sampler", "grain cloud"],
    );
    add(
        "granular_pitch_shift",
        "Granular pitch shift",
        "⇅",
        "Audio",
        "Streaming audio pitch shifting with two overlapping Hann-windowed read heads. Grain duration sets the tradeoff between response and modulation artifacts. Dry audio is delayed to match the zero-shift wet path; pitch changes latch at grain boundaries. No sample upload or offline processing is required.",
        vec![port("in", Audio)],
        vec![port("out", Audio)],
        vec![
            param("semitones", "Pitch shift", "semitones", -24., 24., 0.),
            Parameter {
                structural: true,
                ..param("grain_ms", "Grain duration", "ms", 10., 200., 20.)
            },
            param("mix", "Wet/dry", "", 0., 1., 1.),
        ],
        &["realtime pitch shift", "live granular"],
    );
    add(
        "granular_field",
        "Granular Field",
        "⁘",
        "Audio",
        "Continuous granular cloud drawn from up to eight project samples and two live audio inputs placed on a two-dimensional field. Move the X/Y control point toward a source to hear more of it: each grain picks its source at birth with probability exp(-(distance/focus)^2), so a small Focus lets the nearest source dominate and a large one blends neighbours. Grains are Hann-windowed slices read from Position with random Spray; Density is the total grain rate. Pitch transposes every grain in semitones, or, with Randomize pitch on, sets how far each grain may wander up or down. Live inputs are captured into rings of up to ten seconds and join the field only while a cable is connected (Position 0 is the oldest audio, 1 the newest). Add samples in Options; slot N shows a Sample N ID input. Runs continuously while the engine is enabled: no note, gate or MIDI inputs and no envelope.",
        vec![port("live_1", Audio), port("live_2", Audio)],
        vec![port("out", Audio)],
        granular_field_parameters(),
        &["grain field", "sample field", "xy granular", "2d granular", "live granular", "granular morph", "texture"],
    );
    add(
        "pitch_tracker",
        "Pitch tracker",
        "♬",
        "Analysis",
        "Mix audio channels to mono and estimate up to four pitches using a windowed FFT. Slots rank strongest first and output integer MIDI notes; -1 means no detected pitch. Harmonic grouping is approximate, especially for octaves, missing fundamentals, noise and transients. Larger FFTs improve low-note resolution but add analysis delay.",
        vec![port("in", Audio)],
        (1..=4)
            .map(|i| {
                let mut p = port(&format!("pitch{i}"), Control);
                p.label = format!("Pitch {i}");
                p
            })
            .collect(),
        vec![
            Parameter {
                structural: true,
                ..param("slots", "Pitch slots", "", 1., 4., 1.)
            },
            Parameter {
                structural: true,
                ..param("fft_size", "FFT size", "samples", 2048., 8192., 8192.)
            },
            param("threshold", "Detection threshold", "dBFS", -90., -12., -55.),
        ],
        &["pitch detection", "audio to midi", "frequency tracker"],
    );
    add(
        "piano",
        "Piano",
        "♬",
        "Control",
        "Playable MIDI keyboard with a selectable 1–8 octave span. Receives and forwards pitch, velocity, gate, trigger and note_off, including notes outside the displayed octave. Held received notes light matching keys. Enable the audio engine to test outputs without playing the show; keys send velocity 100. Drag vertically to send pitch bend (centered on release), or horizontally across keys for glissando. Use the MIDI cable for bending internal instruments (±2 semitones); external instruments set their own bend range. Choose the starting octave and octave span in the modal. Display stops at MIDI 127.",
        ["pitch", "velocity", "gate", "trigger", "note_off"]
            .into_iter()
            .map(|id| port(id, Control))
            .chain(std::iter::once(midi_port()))
            .collect(),
        ["pitch", "velocity", "gate", "trigger", "note_off"]
            .into_iter()
            .map(|id| port(id, Control))
            .chain(std::iter::once(midi_port()))
            .collect(),
        vec![
            Parameter {
                structural: true,
                ..param("octave", "Octave", "", -1., 9., 4.)
            },
            Parameter {
                structural: true,
                ..param("octaves", "Octave span", "", 1., 8., 1.)
            },
        ],
        &["keyboard", "midi test"],
    );
    let pad_names = [
        ("bass_drum", "Bass drum", 36.),
        ("snare", "Snare", 38.),
        ("tom_1", "Tom 1", 45.),
        ("tom_2", "Tom 2", 50.),
        ("hi_hat", "Hi hat", 42.),
        ("cymbal", "Cymbal", 49.),
    ];
    let mut pad_parameters: Vec<Parameter> = pad_names
        .iter()
        .enumerate()
        .map(|(i, (_, label, note))| Parameter {
            structural: true,
            ..param(
                &format!("note_{}", i + 1),
                &format!("{label} note"),
                "",
                0.,
                127.,
                *note,
            )
        })
        .collect();
    pad_parameters.push(Parameter {
        structural: true,
        ..param("channel", "MIDI channel", "", 1., 16., 10.)
    });
    add(
        "drum_pads",
        "Drum pads",
        "◍",
        "Control",
        "Six clickable pads for testing drum patches. Trigger inputs 1–6 fire on zero-to-nonzero transitions (positive or negative) at velocity 100; zero releases and rearms the pad. Each pad sends note-on/off for its configured MIDI note and channel, forwards received typed MIDI, lights matching notes, and pulses its trigger output with the note velocity for one engine sample. Set pad notes and the channel in the modal.",
        (1..=6).map(|i| port(&format!("trigger_{i}"), Control)).collect(),
        pad_names
            .iter()
            .map(|(id, label, _)| {
                let mut p = port(id, Control);
                p.label = label.to_string();
                p
            })
            .collect(),
        pad_parameters,
        &["drums", "pads", "percussion", "midi test"],
    );
    let controller_parameters = |sliders: bool| {
        let mut parameters = vec![Parameter {
            structural: true,
            ..param(
                "count",
                if sliders { "Sliders" } else { "Knobs" },
                "",
                1.,
                8.,
                4.,
            )
        }];
        if sliders {
            parameters.extend([
                Parameter {
                    structural: true,
                    ..param(
                        "orientation",
                        "Orientation (0 vertical, 1 horizontal)",
                        "",
                        0.,
                        1.,
                        0.,
                    )
                },
                Parameter {
                    structural: true,
                    ..param("min", "Minimum", "", -100000., 100000., 0.)
                },
                Parameter {
                    structural: true,
                    ..param("max", "Maximum", "", -100000., 100000., 1.)
                },
                Parameter {
                    structural: true,
                    ..param("step", "Step (0 unlocked)", "", 0., 100000., 0.)
                },
                Parameter {
                    structural: true,
                    ..param("decimals", "Decimal places", "", 0., 8., 2.)
                },
            ]);
        }
        for index in 1..=8 {
            parameters.push(Parameter {
                structural: true,
                ..param(
                    &format!("channel_{index}"),
                    &format!("{index} · MIDI channel"),
                    "",
                    if sliders { 1. } else { 0. },
                    16.,
                    1.,
                )
            });
            parameters.push(Parameter {
                structural: true,
                ..param(
                    &format!("controller_{index}"),
                    &format!("{index} · MIDI controller"),
                    "CC",
                    0.,
                    127.,
                    index as f64,
                )
            });
        }
        parameters
    };
    add(
        "knobs",
        "Knobs",
        "◔",
        "Control",
        "One to eight assignable MIDI CC knobs. Incoming MIDI passes through and matching CC messages update individual normalized control outputs. Channel 0 means unassigned. Double-click a knob to clear its binding. Drag vertically to change an assigned knob and send its CC; click without dragging, then turn a hardware control to learn its MIDI channel and controller.",
        vec![],
        (1..=8)
            .map(|i| {
                let mut p = port(&format!("knob_{i}"), Control);
                p.label = format!("Knob {i}");
                p
            })
            .collect(),
        controller_parameters(false),
        &["midi controller", "rotary", "cc"],
    );
    add(
        "sliders",
        "Sliders",
        "▥",
        "Control",
        "One to eight assignable MIDI CC sliders with matching control inputs and outputs. Choose horizontal or vertical display, range, step and decimal rounding in the modal. Incoming MIDI passes through; drag a slider to send its assigned CC, or click without dragging and turn a hardware control to learn it.",
        (1..=8)
            .map(|i| {
                let mut p = port(&format!("slider_{i}"), Control);
                p.label = format!("Slider {i}");
                p
            })
            .collect(),
        (1..=8)
            .map(|i| {
                let mut p = port(&format!("slider_{i}"), Control);
                p.label = format!("Slider {i}");
                p
            })
            .collect(),
        controller_parameters(true),
        &["midi controller", "fader", "cc"],
    );
    add(
        "local_midi_input",
        "Local MIDI Input",
        "♪",
        "External",
        "Select a MIDI device connected to this browser or app session in the modal. Enable the engine and connect to forward MIDI channel messages to the server. Connect the MIDI outlet to Knobs/Sliders for MIDI Learn. Requires Web MIDI and trusted HTTPS or localhost; device selection stays local to this session.",
        vec![],
        vec![midi_port()],
        vec![],
        &["browser midi", "web midi", "controller"],
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
                "MIDI input from a device attached to the server. Select its port and channel in the modal. The MIDI outlet forwards all channel messages, including CC and pitch bend; connect it to Knobs/Sliders for MIDI Learn. The mode selects only scalar decoding. Note outputs match Part MIDI: pitch, velocity, held gate, note-on trigger and note-off trigger; number/value are legacy aliases. Chords are serialized at one event per two engine samples."
            } else {
                "Send note-on/off events to a physical MIDI port using pitch, velocity, gate, trigger and note_off. Connect trigger and note_off for polyphony. Number/value are legacy aliases; CC mode sends controller/value on trigger."
            },
            if input {
                vec![]
            } else {
                vec![
                    port("pitch", Control),
                    port("velocity", Control),
                    port("gate", Control),
                    port("trigger", Control),
                    port("note_off", Control),
                    port("number", Control),
                    port("value", Control),
                ]
            },
            if input {
                vec![
                    port("pitch", Control),
                    port("velocity", Control),
                    port("gate", Control),
                    port("trigger", Control),
                    port("note_off", Control),
                    port("number", Control),
                    port("value", Control),
                ]
            } else {
                vec![]
            },
            vec![
                structural(param(
                    "mode",
                    if input {
                        "Scalar decode (0 Note, 1 CC)"
                    } else {
                        "Message type (0 Note, 1 CC)"
                    },
                    "",
                    0.,
                    1.,
                    0.,
                )),
                structural(param(
                    "channel",
                    "MIDI channel (0 all inputs)",
                    "",
                    if input { 0. } else { 1. },
                    16.,
                    if input { 0. } else { 1. },
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
    for input in [true, false] {
        add(
            if input { "osc_to_midi" } else { "midi_to_osc" },
            if input { "OSC to MIDI" } else { "MIDI to OSC" },
            "↔",
            "External",
            if input {
                "Decode note messages at an exact OSC address into pitch, velocity, held gate, note-on trigger and note-off trigger. Messages have two integer arguments: pitch and velocity (0–127); velocity zero means note-off. Enable OSC reception in System settings. Connect these outputs to a sampler or MIDI output."
            } else {
                "Encode pitch/velocity note-on and note-off events as OSC messages at the configured destination/address. Two integer arguments: pitch and velocity; velocity zero means note-off. Connect trigger and note_off for polyphony. Enable OSC sending in System settings."
            },
            if input {
                vec![]
            } else {
                vec![
                    port("pitch", Control),
                    port("velocity", Control),
                    port("gate", Control),
                    port("trigger", Control),
                    port("note_off", Control),
                ]
            },
            if input {
                vec![
                    port("pitch", Control),
                    port("velocity", Control),
                    port("gate", Control),
                    port("trigger", Control),
                    port("note_off", Control),
                ]
            } else {
                vec![]
            },
            vec![],
            &["notes", "converter"],
        );
    }
    add(
        "control_input",
        "Graphical control",
        "▤",
        "Control",
        "Interactive compact control. Incoming changes update the displayed value; manual edits override until the next incoming change/event. Bang emits one engine-sample pulse. Change-only output emits the initial value and subsequent changes, holding its value silently between events. Hide chrome displays only the input and handles.",
        vec![port("in", Control)],
        vec![port("out", Control)],
        vec![
            Parameter {
                structural: true,
                ..param("mode", "Control type", "", 0., 4., 2.)
            },
            Parameter { structural: true, ..param("changes_only", "Output only on change", "", 0., 1., 0.) },
            Parameter { structural: true, ..param("hide_chrome", "Hide title, header and footer", "", 0., 1., 0.) },
            Parameter {
                structural: true,
                ..param("min", "Minimum", "", -100000., 100000., -100000.)
            },
            Parameter {
                structural: true,
                ..param("max", "Maximum", "", -100000., 100000., 100000.)
            },
            Parameter { structural: true, ..param("step", "Step (0 automatic)", "", 0., 100000., 0.) },
        ],
        &[],
    );
    add(
        "value",
        "Value",
        "ƒ",
        "Control",
        "Set value stores a number. With Trigger connected, any nonzero trigger emits the stored value (including zero); output holds the last emitted value between triggers. Without a Trigger connection, output is constant. The node displays the stored value.",
        vec![port("trigger", Control)],
        vec![port("out", Control)],
        vec![param("value", "Set value", "", -100000., 100000., 0.)],
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
        "Route physical interface channels into a 1–8-channel signal. Unmapped channels are silent.",
        vec![],
        vec![port("out", Audio)],
        device_routes(vec![
            Parameter {
                structural: true,
                ..param("interface", "Input interface", "", 0., 999999999., 0.)
            },
            param("offset", "Legacy channel offset", "", 0., 63., 0.),
        ]),
        &["adc~"],
    );
    add(
        "browser_input",
        "Local audio input",
        "◉",
        "Audio",
        "Audio input captured on this device and sent over WebRTC. Defaults to unmuted. Mute silences every output channel: 0 passes audio, positive values mute. A connected Mute control overrides the microphone button. Capture still requires microphone permission.",
        vec![],
        vec![port("out", Audio)],
        vec![param("mute", "Mute", "", 0., 1., 0.)],
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
        "Route signal channels to physical outputs. Channels sharing a destination are summed; None disconnects a channel.",
        vec![port("in", Audio)],
        vec![],
        device_routes(vec![
            param("gain", "Output gain", "dB", -90., 6., -12.),
            Parameter {
                structural: true,
                ..param("interface", "Audio interface", "", 0., 999999999., 0.)
            },
        ]),
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
        "console_out", "Console Out", ">_", "Control",
        "Print incoming numeric or text control values to the project Console. Logs changed values and explicit events, not every held sample. Debug capture is bounded; overload is reported. Enable the engine and open Settings → Console.",
        vec![port("in", Control)], vec![], vec![], &["console", "debug", "print", "log"],
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
        "Transparent multichannel audio pass-through with block-rate spectral analysis and a spectrogram. Enable the audio engine to visualize this signal; audio is unchanged.",
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
        "Transparent spectral-frame pass-through. Enable the audio engine to visualize this signal with spectrogram, FFT-bin magnitudes, and phase in radians for every channel; Cartesian and polar data are preserved.",
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
        "64-voice sine instrument with standard MIDI pitch, velocity, gate, trigger and pitch-specific note_off controls, or direct scheduled part notes. Repeated MIDI pitches release oldest-held-first. Mono by default; the same mix is available on 1–8 output channels.",
        ["pitch", "velocity", "gate", "trigger", "note_off"]
            .into_iter()
            .map(|id| port(id, Control))
            .chain(std::iter::once(midi_port()))
            .collect(),
        vec![port("out", Audio)],
        vec![
            param("amplitude", "Amplitude", "", 0., 1., 0.2),
        ]
        .into_iter()
        .chain(envelope_parameters([0., 0., 1., 80.]))
        .collect(),
        &["poly"],
    );
    add(
        "fm_synth",
        "Polyphonic FM synth",
        "FM",
        "Audio",
        "64-voice two-oscillator FM instrument with standard MIDI note inputs or direct score notes. Carrier and modulator frequencies are Hz at MIDI A4 (69); each voice transposes both and the FM depth with its MIDI pitch. Modulator output changes carrier frequency by FM depth in Hz, including through-zero FM. Choose each waveform in the modal. Repeated MIDI pitches release oldest-held-first. Mono by default; 1–8 channels carry the same polyphonic mix.",
        ["pitch", "velocity", "gate", "trigger", "note_off"]
            .into_iter()
            .map(|id| port(id, Control))
            .collect(),
        vec![port("out", Audio)],
        vec![
            param(
                "carrier_frequency",
                "Carrier frequency",
                "Hz at A4",
                0.,
                20000.,
                440.,
            ),
            param(
                "modulator_frequency",
                "Modulator frequency",
                "Hz at A4",
                0.,
                20000.,
                880.,
            ),
            param("carrier_waveform", "Carrier waveform", "", 0., 4., 0.),
            param("modulator_waveform", "Modulator waveform", "", 0., 4., 0.),
            param("fm_depth", "FM depth", "Hz at A4", 0., 20000., 220.),
            param("amplitude", "Amplitude", "", 0., 1., 0.2),
        ]
        .into_iter()
        .chain(envelope_parameters([0., 0., 1., 80.]))
        .collect(),
        &["poly", "FM", "frequency modulation", "two oscillator"],
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
        "Pass-through audio with a per-channel level meter drawn on the node (dBFS, like the monitor tab). Each channel also has a Level control output carrying its amplitude (0–1 peak, instant rise, falling over the fall time); outputs above the channel width read 0. The first control readout is the loudest channel.",
        vec![port("in", Audio)],
        std::iter::once(port("out", Audio))
            .chain((1..=MAX_CHANNELS).map(|i| {
                let mut p = port(&format!("level_{i}"), Control);
                p.label = format!("Level {i}");
                p
            }))
            .collect(),
        vec![param("release", "Fall time", "ms", 10., 2000., 300.)],
        &["env~", "vu", "level", "peak"],
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
        "Constant-power stereo balance: centre is unity, each extreme silences the opposite channel and the stereo image is kept. Mono and channels above two pass through.",
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
        "overdrive",
        "Overdrive",
        "⌇",
        "Effect",
        "Guitar-pedal style distortion. Drive boosts the signal into a waveshaper; Shape morphs the clipper from smooth tube-like saturation (0) to hard fuzz clipping (1); Tone is a low-pass after the clipper; Level trims the output and Mix blends with the dry signal. A DC blocker ahead of the clipper keeps input offsets from biasing the distortion, so the wet path never exceeds Level.",
        vec![port("in", Audio)],
        vec![port("out", Audio)],
        vec![
            param("drive", "Drive", "dB", 0., 60., 18.),
            param("shape", "Shape", "", 0., 1., 0.35),
            param("tone", "Tone", "Hz", 300., 20000., 3500.),
            param("level", "Level", "dB", -24., 12., -6.),
            param("mix", "Mix", "", 0., 1., 1.),
        ],
        &["distortion", "fuzz", "drive", "saturation", "clip~"],
    );
    for (suffix, signal) in [
        ("control", Control),
        ("audio", Audio),
        ("spectral", Spectral),
    ] {
        for send in [true, false] {
            let kind = format!("{}_{suffix}", if send { "send" } else { "receive" });
            let label = format!("{} {}", if send { "Send" } else { "Receive" }, suffix);
            let mut inputs = if send {
                vec![port("in", signal)]
            } else {
                vec![]
            };
            inputs.push(port("target", Control));
            let parameters = if signal == Spectral {
                vec![
                    Parameter {
                        structural: true,
                        ..param("size", "FFT size", "", 256., 8192., 1024.)
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
                &kind,
                &label,
                if send { "↗" } else { "↙" },
                "Routing",
                "Route data to matching target names within this project graph. Set the target in the modal or drive it with text. Named routes add one engine sample of delay. Audio sends mix; control changes use arrival order and uppermost-source priority on ties; spectral receives use the uppermost compatible sender. Empty targets disconnect. Dynamic format mismatches are reported in telemetry.",
                inputs,
                if send {
                    vec![]
                } else {
                    vec![port("out", signal)]
                },
                parameters,
                &[],
            );
        }
    }
    add(
        "toggle",
        "Toggle",
        "✓",
        "Control",
        "A latched checkbox that emits one event when its state changes: 1 when checked, 0 when cleared. It emits nothing while idle. Changed input sets the checkbox (positive numbers or text on; zero or negative numbers off). Manual clicks override until the input changes again. Manual settings are saved; input-driven state is runtime only.",
        vec![port("in", Control)],
        vec![port("out", Control)],
        vec![],
        &["checkbox", "switch"],
    );
    add(
        "trigger",
        "Trigger",
        "!",
        "Control",
        "Pass numeric input through unchanged. Any nonzero value lights the indicator. Clicking sends 1 for one engine sample, even with an input connected, then resumes the input (or zero when disconnected).",
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
    add(
        "control_delay",
        "Control delay",
        "↶#",
        "Control",
        "Delay a numeric control signal by a time in milliseconds, sample-accurately, including one-sample trigger pulses. Time may be driven from the graph; changes take effect immediately. Up to 5 seconds. Text control data is not delayed.",
        vec![port("in", Control)],
        vec![port("out", Control)],
        vec![param("time", "Delay time", "ms", 0., 5000., 100.)],
        &["delay", "pipe", "control line delay"],
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
    add(
        "record",
        "Record",
        "●",
        "Audio",
        "Archive the incoming 1–8 channel audio bundle to a timestamped WAV on this server. The node name becomes the filename prefix. Start and Stop accept positive rising edges; return to zero to rearm. Stop wins simultaneous commands. Channel count follows the audio input. Uses the current engine sample rate and 32-bit float PCM, matching project sample storage. Recordings belong to this project and are not publicly served. Stopping the show does not stop this node; send Stop or disable the engine.",
        vec![
            port("in", Audio),
            port("start", Control),
            port("stop", Control),
        ],
        vec![],
        vec![],
        &[],
    );
    add(
        "looper",
        "Looper",
        "↻",
        "Audio",
        "Eight independent audio loop tracks. Send track numbers 1–8 to Start loop (record), Stop loop record, Start playback or Stop playback; 0 is idle. Return to 0 before repeating the same command. Loop mode 0 records until stopped and starts immediately; a positive beat count queues record/play for the next bar of the graph clock (aligned to the show while it plays) and ends recording after that many meter beats. Stops are immediate. Playback repeats the recorded audio without tempo stretching. Starting recording replaces that track. Playback can finish an in-progress recording. Loops are saved per project and node on disk. Clear removes the selected track immediately. Capacity limits recording duration.",
        vec![
            port("in", Audio),
            port("start_loop", Control),
            port("stop_loop_record", Control),
            port("start_playback", Control),
            port("stop_playback", Control),
            port("clear", Control),
        ],
        vec![port("out", Audio)],
        vec![
            param(
                "loop_mode",
                "Loop mode (0 = freeform)",
                "beats",
                0.,
                128.,
                4.,
            ),
            Parameter {
                structural: true,
                ..param("max_seconds", "Capacity per track", "s", 1., 300., 30.)
            },
        ],
        &["loop", "record", "eight tracks"],
    );
    add(
        "sample_selector", "Sample selector", "☷", "Control",
        "Build a numbered shortlist of project samples in the options, with optional nicknames. Click an orange entry or drive Index (0 through n−1) to output its numeric Sample ID. Connect to the Sample ID input of a Polyphonic sampler, Granular synth or Sample player. A connected Index is read-only. Fractional indices round down; out-of-range indices and empty lists output 0 (no sample). Selecting does not trigger a note.",
        vec![], vec![Port { label: "Sample ID".into(), ..port("out", Control) }],
        vec![param("index", "Index", "", -1., 1000000000., 0.)],
        &["sample list", "sample picker", "sample switch"],
    );
    add(
        "poly_sampler",
        "Polyphonic sampler",
        "▶",
        "Audio",
        "64-voice pitched WAV sampler. Pitch and velocity take MIDI values (0–127); trigger starts a note and note_off releases the matching pitch. Repeated pitches release oldest-held-first. Gate can drive simple monophonic patches when triggers are unconnected. Upload a WAV and set its root MIDI note in the modal. Each voice runs its own ADSR, using the same envelope as the ADSR node. Edit the graph or Attack, Decay, Sustain and Release in Options. Stage times latch when each stage starts; live sustain changes are smoothed. Every note starts a fresh envelope, including repeated pitches. The graph’s live line shows the highest voice envelope. Defaults preserve immediate attack and full sustain. Oldest voices are stolen at capacity.",
        vec![
            port("pitch", Control),
            port("velocity", Control),
            port("gate", Control),
            port("trigger", Control),
            port("note_off", Control),
        ],
        vec![port("out", Audio)],
        vec![
            Parameter {
                structural: true,
                ..param("asset", "Sample ID", "", 0., 1000000000., 0.)
            },
            param("root_note", "Root MIDI note", "", 0., 127., 60.),
            param("amplitude", "Amplitude", "", 0., 1., 0.5),
            param("loop", "Loop while held", "", 0., 1., 0.),
        ].into_iter().chain(envelope_parameters([0., 0., 1., 80.])).collect(),
        &["sampler", "polyphonic sample"],
    );
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
    add(
        "midi_to_control",
        "MIDI to control",
        "♪→",
        "Control",
        "Decode typed MIDI channel messages. Type is the status high nibble (8 note off, 9 note on, 11 CC, 12 program, 13 pressure, 14 bend). Number identifies a note/controller; value decodes 7-bit values or full 14-bit pitch bend. Channel is 1–16. Trigger marks any matching event; note_on and note_off pulse for matching note attacks and releases (a velocity-zero note-on releases), so one decoder with a number filter drives an instrument's trigger and note_off. Filter type/number in the modal; zero type or -1 number accepts all.",
        vec![midi_port()],
        vec![
            port("type", Control),
            port("number", Control),
            port("value", Control),
            port("channel", Control),
            port("trigger", Control),
            port("note_on", Control),
            port("note_off", Control),
        ],
        vec![
            param("message_type", "Message type (0 all)", "", 0., 14., 0.),
            param("number_filter", "Number (-1 all)", "", -1., 127., -1.),
        ],
        &[],
    );
    // Keep Cloud's sample and numeric controls identical to the granular instrument.
    let mut cloud = result.iter().find(|d| d.kind == "granular_synth").unwrap().clone();
    cloud.kind = "granular_cloud".into();
    cloud.label = "Granular Cloud".into();
    cloud.symbol = "☁".into();
    cloud.description = "A continuous cloud of overlapping Hann-enveloped grains from the selected sample, without a MIDI inlet. Cloud center (0–1) scans from sample start to end; Spray randomizes grain starts around that center, wrapping at the sample edges. Grain duration, Density, Amplitude, Release, sample selection and numeric note inputs match Granular synth. With no gate/trigger/note_off wiring, the cloud runs while the engine is enabled; optional Pitch and Velocity set its pitch and strength. Connect Gate or Trigger/note_off for the same numeric note control as Granular synth. New center and grain settings affect newly launched grains.".into();
    cloud.documentation = documentation::for_kind("granular_cloud");
    cloud.aliases = vec!["sample cloud".into(), "granular texture".into(), "cloud center".into()];
    cloud.inputs.retain(|p| p.signal != Midi);
    cloud.parameters.iter_mut().find(|p| p.id == "position").unwrap().label = "Cloud center".into();
    result.push(cloud);
    for descriptor in &mut result {
        if matches!(descriptor.kind.as_str(), "sample" | "poly_sampler" | "granular_synth" | "granular_cloud") {
            descriptor.inputs.push(Port { label: "Sample ID".into(), ..port("sample_id", Control) });
        }
        let midi_input = matches!(
            descriptor.kind.as_str(),
            "midi_to_control"
                | "midi_output"
                | "midi_to_osc"
                | "synth"
                | "fm_synth"
                | "poly_sampler"
                | "granular_synth"
                | "piano"
                | "drum_pads"
                | "knobs"
                | "sliders"
        );
        let midi_output = matches!(
            descriptor.kind.as_str(),
            "part_midi"
                | "part_player"
                | "midi_input"
                | "osc_to_midi"
                | "piano"
                | "drum_pads"
                | "knobs"
                | "sliders"
        );
        if midi_input {
            descriptor.inputs.retain(|p| p.signal != Midi);
            descriptor.inputs.push(midi_port());
        }
        if midi_output {
            descriptor.outputs.retain(|p| p.signal != Midi);
            descriptor.outputs.push(midi_port());
        }
        if descriptor.kind == "part_midi" {
            descriptor.parameters.push(Parameter {
                structural: true,
                ..param("staff", "Staff (0 all, 1 first)", "", 0., 8., 0.)
            });
        }
    }
    result.push(Descriptor { default_channels: 1, kind: "js_control".into(), label: "JavaScript control".into(), symbol: "JS".into(), category: "Control".into(), description: "Event-driven JavaScript with named numeric ports, MIDI, OSC, metronome and named control bindings. Open Options for the editor and complete scripting guide. Scripts run on a separate worker; reactive output has asynchronous latency.".into(), documentation: documentation::for_kind("js_control"), aliases: vec!["script".into(), "javascript".into(), "code".into()], inputs: vec![midi_port()], outputs: vec![midi_port()], parameters: vec![Parameter { structural: true, ..param("midi_passthru", "MIDI passthrough", "", 0., 1., 1.) }] });
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
        metadata.validate_flat_inner(false)?;
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
        Ok(self.validate_flat_inner(true)?.order)
    }
    /// Validate a flat graph and return its render order together with the
    /// connections that close feedback loops (read one sample late).
    pub fn schedule(&self) -> Result<Schedule, String> {
        self.validate_flat_inner(true)
    }
    fn validate_flat_inner(&self, check_routes: bool) -> Result<Schedule, String> {
        if self.nodes.iter().filter(|n| n.kind == "record").count() > 16 {
            return Err("At most 16 record nodes are supported".into());
        }
        if self.nodes.iter().filter(|n| named_route(&n.kind)).count() > 64 {
            return Err("At most 64 named send/receive nodes are supported".into());
        }
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
        if self.nodes.iter().filter(|n| n.kind == "js_control").count() > script::MAX_SCRIPTS {
            return Err("At most sixteen JavaScript control nodes are supported".into());
        }
        for (i, n) in self.nodes.iter().enumerate() {
            if let Some(script) = &n.script {
                if n.kind != "js_control" { return Err("Script configuration belongs only to JavaScript control nodes".into()); }
                script.validate()?;
            }
            if n.kind == "js_control" && n.parameters.get("midi_passthru").is_some_and(|v| *v != 0. && *v != 1.) {
                return Err("MIDI passthrough must be on (1) or off (0)".into());
            }
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
            if !n.sample_choices.is_empty()
                && !matches!(n.kind.as_str(), "sample_selector" | "granular_field")
            {
                return Err("Sample lists belong only to Sample selector and Granular Field nodes".into());
            }
            if n.kind == "granular_field" {
                if n.sample_choices.len() > GRANULAR_FIELD_SAMPLES {
                    return Err("Granular Field holds at most 8 sample sources".into());
                }
                if n.parameters.iter().any(|(key, value)| {
                    key.starts_with("sample_") && key.len() == 8 && value.fract() != 0.
                }) {
                    return Err("Granular Field sample IDs must be whole numbers".into());
                }
            }
            if n.sample_choices.len() > MAX_SAMPLE_CHOICES || n.sample_choices.iter().any(|s|
                s.asset == 0 || s.asset > 1_000_000_000 || s.name.len() > 256 || s.nickname.len() > 80
            ) {
                return Err("Sample lists allow at most 64 entries, valid Sample IDs, names up to 256 bytes and nicknames up to 80 bytes".into());
            }
            if matches!(n.kind.as_str(), "convolution" | "convolution_reverb")
                && n.parameters
                    .get("window")
                    .is_some_and(|v| v.fract() != 0. || !(*v as usize).is_power_of_two())
            {
                return Err("Convolution window must be a power of two".into());
            }
            if n.kind == "pitch_tracker" {
                let slots = n.parameters.get("slots").copied().unwrap_or(1.);
                let size = n.parameters.get("fft_size").copied().unwrap_or(8192.);
                if slots.fract() != 0. || size.fract() != 0. || !(size as usize).is_power_of_two() {
                    return Err(
                        "Choose whole pitch slots and a power-of-two tracker FFT size".into(),
                    );
                }
            }
            if matches!(n.kind.as_str(), "poly_sampler" | "granular_synth" | "granular_cloud")
                && n.parameters
                    .get("root_note")
                    .is_some_and(|v| v.fract() != 0.)
            {
                return Err("Root MIDI note must be a whole number".into());
            }
            if n.kind == "piano"
                && ["octave", "octaves"]
                    .iter()
                    .any(|key| n.parameters.get(*key).is_some_and(|v| v.fract() != 0.))
            {
                return Err("Piano octave must be a whole number".into());
            }
            if n.kind == "drum_pads" && n.parameters.values().any(|v| v.fract() != 0.) {
                return Err("Drum pad notes and channel must be whole numbers".into());
            }
            if matches!(n.kind.as_str(), "knobs" | "sliders") {
                let whole = n.parameters.iter().any(|(key, value)| {
                    (key == "count"
                        || key == "orientation"
                        || key == "decimals"
                        || key.starts_with("channel_")
                        || key.starts_with("controller_"))
                        && value.fract() != 0.
                });
                let min = n.parameters.get("min").copied().unwrap_or(0.);
                let max = n.parameters.get("max").copied().unwrap_or(1.);
                if whole || min > max {
                    return Err("Controller counts, display options and MIDI assignments must be whole numbers, with minimum no greater than maximum".into());
                }
                if n.kind == "sliders" {
                    let scale = 10_f64.powf(n.parameters.get("decimals").copied().unwrap_or(2.));
                    if (min * scale).ceil() > (max * scale).floor() {
                        return Err(
                            "Slider range must contain a value at the selected decimal precision"
                                .into(),
                        );
                    }
                }
            }
            if let Some(part) = &n.part_id {
                if !matches!(n.kind.as_str(), "part_midi" | "part_player" | "monitor_output")
                    || part.is_empty()
                    || part.len() > 256
                {
                    return Err("A part association belongs only to a Part MIDI, Part player or Monitor output node and must be a valid part ID".into());
                }
            }
            if let Some(io) = &n.io {
                if !matches!(
                    n.kind.as_str(),
                    "midi_input"
                        | "midi_output"
                        | "osc_input"
                        | "osc_output"
                        | "midi_to_osc"
                        | "osc_to_midi"
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
                if !io.destination.is_empty() && !valid_osc_node_destination(&io.destination) {
                    return Err("OSC node destination must be hostname:port or IPv4:port with a port from 1–65535".into());
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
            if n.kind == "container"
                && !match &n.control_value {
                    None => true,
                    Some(ControlValue::Text(text)) => text.len() <= 2000,
                    Some(_) => false,
                }
            {
                return Err("Container explanations are text up to 2000 bytes".into());
            }
            if let Some(value) = &n.control_value {
                if !named_route(&n.kind)
                    && !matches!(
                        n.kind.as_str(),
                        "control_visualizer" | "control_input" | "toggle" | "container"
                    )
                {
                    return Err(
                        "Input literals belong to graphical controls or control visualizers".into(),
                    );
                }
                match value {
                    ControlValue::Number(v) if !v.is_finite() => {
                        return Err("Control numbers must be finite".into());
                    }
                    // Container explanations are documentation, not a control signal; they have their own 2,000-byte bound above.
                    ControlValue::Text(v) if n.kind != "container" && v.len() > MAX_CONTROL_TEXT_BYTES => {
                        return Err("Control strings must be at most 256 UTF-8 bytes".into());
                    }
                    _ => {}
                }
            }
            if named_route(&n.kind)
                && n.control_value
                    .as_ref()
                    .is_some_and(|v| !matches!(v, ControlValue::Text(_)))
            {
                return Err("Route target must be text".into());
            }
            if n.kind == "toggle"
                && n.control_value
                    .as_ref()
                    .is_some_and(|v| !matches!(v, ControlValue::Number(n) if *n == 0. || *n == 1.))
            {
                return Err("Toggle value must be 0 or 1".into());
            }
            if n.kind == "control_input" {
                let mode = n.parameters.get("mode").copied().unwrap_or(2.);
                let min = n.parameters.get("min").copied().unwrap_or(-100000.);
                let max = n.parameters.get("max").copied().unwrap_or(100000.);
                let step = n.parameters.get("step").copied().unwrap_or(0.);
                if mode.fract() != 0.
                    || ["changes_only", "hide_chrome"].iter().any(|key| n.parameters.get(*key).is_some_and(|v| *v != 0. && *v != 1.))
                    || min > max
                    || (mode == 1. && (min.fract() != 0. || max.fract() != 0.))
                    || !step.is_finite()
                    || step < 0.
                    || (mode == 1. && step.fract() != 0.)
                {
                    return Err("Choose a control type and valid minimum/maximum/step (whole numbers for Integer)".into());
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
            if d.category == "Spectral"
                || n.kind == "audio_visualizer"
                || (named_route(&n.kind) && n.kind.ends_with("spectral"))
            {
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
                if matches!(n.kind.as_str(), "input" | "output")
                    && DEVICE_ROUTE_KEYS.contains(&key.as_str())
                    && value.fract() != 0.
                {
                    return Err("Physical channel routes must be whole numbers".into());
                }
                if ((n.kind == "oscillator" && key == "waveform")
                    || (n.kind == "fm_synth"
                        && matches!(key.as_str(), "carrier_waveform" | "modulator_waveform")))
                    && value.fract() != 0.
                {
                    return Err("Waveform must be an integer from 0 to 4".into());
                }
                if n.kind == "looper" && value.fract() != 0. {
                    return Err("Looper mode and capacity must be whole numbers".into());
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
            .map(|e| &e.target)
            .collect::<BTreeSet<_>>()
            .len()
            > 1
        {
            return Err("Only one global clock tempo input may be connected".into());
        }
        let mut indegree = vec![0; self.nodes.len()];
        let mut outgoing: Vec<Vec<(usize, usize)>> = vec![vec![]; self.nodes.len()];
        let mut occupied = BTreeSet::new();
        let mut edge_ids = BTreeSet::new();
        for (edge_index, edge) in self.edges.iter().enumerate() {
            if !edge_ids.insert(&edge.id) {
                return Err("Duplicate edge ID".into());
            }
            let s = *ids.get(&edge.source).ok_or("Missing source node")?;
            let t = *ids.get(&edge.target).ok_or("Missing target node")?;
            let sd = descriptors
                .iter()
                .find(|d| d.kind == self.nodes[s].kind)
                .unwrap();
            let sd = script::descriptor(&self.nodes[s], sd);
            let td = descriptors
                .iter()
                .find(|d| d.kind == self.nodes[t].kind)
                .unwrap();
            let td = script::descriptor(&self.nodes[t], td);
            if self.nodes[s].kind == "pitch_tracker"
                && !sd
                    .outputs
                    .iter()
                    .take(self.nodes[s].parameters.get("slots").copied().unwrap_or(1.) as usize)
                    .any(|p| p.id == edge.source_port)
            {
                return Err("Pitch tracker output slot is not enabled".into());
            }
            if matches!(self.nodes[s].kind.as_str(), "knobs" | "sliders")
                && edge.source_port != "midi"
                && edge
                    .source_port
                    .rsplit('_')
                    .next()
                    .and_then(|v| v.parse::<usize>().ok())
                    .is_some_and(|slot| {
                        slot > self.nodes[s].parameters.get("count").copied().unwrap_or(4.) as usize
                    })
            {
                return Err("Controller output is not enabled".into());
            }
            if self.nodes[t].kind == "granular_field"
                && edge
                    .target_port
                    .strip_prefix("sample_")
                    .and_then(|v| v.parse::<usize>().ok())
                    .is_some_and(|slot| slot == 0 || slot > self.nodes[t].sample_choices.len())
            {
                return Err("Granular Field sample slot is not configured; add the sample to the node's list first".into());
            }
            if self.nodes[t].kind == "sliders"
                && edge.target_port != "midi"
                && edge
                    .target_port
                    .rsplit('_')
                    .next()
                    .and_then(|v| v.parse::<usize>().ok())
                    .is_some_and(|slot| {
                        slot > self.nodes[t].parameters.get("count").copied().unwrap_or(4.) as usize
                    })
            {
                return Err("Slider input is not enabled".into());
            }
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
                && !matches!(self.nodes[t].kind.as_str(), "record" | "pitch_tracker")
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
            // Control inputs arbitrate multiple sources; MIDI inputs concatenate
            // every source's messages in connection order within each sample.
            if !occupied.insert((t, edge.target_port.clone()))
                && !matches!(signal, Signal::Control | Signal::Midi)
            {
                return Err("Input already has a driver; use a mixer or math node".into());
            }
            outgoing[s].push((t, edge_index));
            indegree[t] += 1;
        }
        // Topological order with deterministic feedback breaking. When nothing
        // is ready, the loop that depends on no other unscheduled loop starts at
        // its node drawn furthest left (then highest, then by id); the wires
        // still feeding that node from inside the loop become feedback edges,
        // which the engine reads one sample late. Only audio and control wires
        // may close a loop.
        let mut ready: BTreeSet<usize> = indegree
            .iter()
            .enumerate()
            .filter_map(|(i, n)| (*n == 0).then_some(i))
            .collect();
        let mut order = Vec::new();
        let mut scheduled = vec![false; self.nodes.len()];
        let mut feedback_edge = vec![false; self.edges.len()];
        let mut feedback = Vec::new();
        loop {
            while let Some(i) = ready.pop_first() {
                order.push(i);
                scheduled[i] = true;
                for &(t, edge) in &outgoing[i] {
                    if feedback_edge[edge] {
                        continue;
                    }
                    indegree[t] -= 1;
                    if indegree[t] == 0 {
                        ready.insert(t);
                    }
                }
            }
            if order.len() == self.nodes.len() {
                break;
            }
            let start = loop_start(&self.nodes, &outgoing, &scheduled, &feedback_edge);
            for (edge_index, edge) in self.edges.iter().enumerate() {
                let s = ids[&edge.source];
                if ids[&edge.target] != start || feedback_edge[edge_index] || scheduled[s] {
                    continue;
                }
                let source_descriptor = descriptors
                    .iter()
                    .find(|d| d.kind == self.nodes[s].kind)
                    .unwrap();
                let signal = script::descriptor(&self.nodes[s], source_descriptor)
                    .outputs
                    .iter()
                    .find(|p| p.id == edge.source_port)
                    .unwrap()
                    .signal;
                if !matches!(signal, Signal::Audio | Signal::Control) {
                    return Err(
                        "Feedback loops may only close through audio or control connections".into(),
                    );
                }
                feedback_edge[edge_index] = true;
                feedback.push(edge_index);
            }
            indegree[start] = 0;
            ready.insert(start);
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
        fn route_target(n: &Node) -> &str {
            match &n.control_value {
                Some(ControlValue::Text(value)) => value.as_str(),
                _ => "",
            }
        }
        let dynamic = |n: &Node| {
            self.edges
                .iter()
                .any(|e| e.target == n.id && e.target_port == "target")
        };
        if check_routes {
            for receive in self
                .nodes
                .iter()
                .filter(|n| named_route(&n.kind) && n.kind.starts_with("receive_"))
            {
                for send in self.nodes.iter().filter(|n| {
                    named_route(&n.kind)
                        && n.kind.starts_with("send_")
                        && n.kind.strip_prefix("send_") == receive.kind.strip_prefix("receive_")
                }) {
                    if !dynamic(send)
                        && !dynamic(receive)
                        && !route_target(send).is_empty()
                        && route_target(send) == route_target(receive)
                    {
                        if send.kind != "send_control" && send.channels != receive.channels {
                            return Err(
                                "Named audio/spectral routes must have matching channels".into()
                            );
                        }
                        if send.kind == "send_spectral"
                            && ["size", "overlap"].iter().any(|key| {
                                send.parameters
                                    .get(*key)
                                    .copied()
                                    .unwrap_or(if *key == "size" { 1024. } else { 4. })
                                    != receive
                                        .parameters
                                        .get(*key)
                                        .copied()
                                        .unwrap_or(if *key == "size" { 1024. } else { 4. })
                            })
                        {
                            return Err(
                                "Named spectral routes must have matching FFT configurations"
                                    .into(),
                            );
                        }
                    }
                }
            }
        }
        let mut text = vec![false; self.nodes.len()];
        for _ in 0..=self.nodes.len() {
            let previous = text.clone();
            for &i in &order {
                let n = &self.nodes[i];
                if n.kind == "receive_control" {
                    text[i] |= self.nodes.iter().enumerate().any(|(j, s)| {
                        s.kind == "send_control"
                            && (dynamic(s)
                                || dynamic(n)
                                || (!route_target(s).is_empty()
                                    && route_target(s) == route_target(n)))
                            && text[j]
                    });
                } else if matches!(
                    n.kind.as_str(),
                    "control_visualizer" | "control_input" | "osc_input" | "send_control"
                ) || (n.kind.starts_with("subgraph_") && n.kind.ends_with("_control"))
                {
                    let incoming: Vec<_> = self
                        .edges
                        .iter()
                        .filter(|e| e.target == n.id && e.target_port == "in")
                        .collect();
                    text[i] |= if incoming.is_empty() {
                        (n.kind != "send_control"
                            && matches!(n.control_value, Some(ControlValue::Text(_))))
                            || (n.kind == "osc_input" && n.parameters.get("text") == Some(&1.))
                            || (n.kind == "control_input" && n.parameters.get("mode") == Some(&4.))
                    } else {
                        incoming.iter().any(|e| text[*ids.get(&e.source).unwrap()])
                    };
                }
            }
            if previous == text {
                break;
            }
        }
        for edge in &self.edges {
            let source = *ids.get(&edge.source).unwrap();
            let target = *ids.get(&edge.target).unwrap();
            if named_route(&self.nodes[target].kind)
                && edge.target_port == "target"
                && !text[source]
            {
                return Err("Named route target input requires string control data".into());
            }
            if text[source]
                && !(named_route(&self.nodes[target].kind)
                    && (edge.target_port == "target" || self.nodes[target].kind == "send_control"))
                && !matches!(
                    self.nodes[target].kind.as_str(),
                    "control_visualizer" | "control_input" | "osc_output" | "console_out" | "toggle"
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
        Ok(Schedule { order, feedback })
    }
}

/// Render order plus the indices of connections that close feedback loops.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Schedule {
    pub order: Vec<usize>,
    pub feedback: Vec<usize>,
}

/// Among unscheduled nodes, find the strongly connected components that no
/// other unscheduled component feeds, and return the leftmost node in them.
fn loop_start(
    nodes: &[Node],
    outgoing: &[Vec<(usize, usize)>],
    scheduled: &[bool],
    feedback_edge: &[bool],
) -> usize {
    struct Tarjan<'a> {
        outgoing: &'a [Vec<(usize, usize)>],
        scheduled: &'a [bool],
        feedback_edge: &'a [bool],
        index: Vec<Option<usize>>,
        low: Vec<usize>,
        on_stack: Vec<bool>,
        stack: Vec<usize>,
        component: Vec<usize>,
        next_index: usize,
        next_component: usize,
    }
    impl Tarjan<'_> {
        fn visit(&mut self, v: usize) {
            self.index[v] = Some(self.next_index);
            self.low[v] = self.next_index;
            self.next_index += 1;
            self.stack.push(v);
            self.on_stack[v] = true;
            for k in 0..self.outgoing[v].len() {
                let (w, edge) = self.outgoing[v][k];
                if self.scheduled[w] || self.feedback_edge[edge] {
                    continue;
                }
                match self.index[w] {
                    None => {
                        self.visit(w);
                        self.low[v] = self.low[v].min(self.low[w]);
                    }
                    Some(index) if self.on_stack[w] => self.low[v] = self.low[v].min(index),
                    _ => {}
                }
            }
            if self.low[v] == self.index[v].unwrap() {
                while let Some(w) = self.stack.pop() {
                    self.on_stack[w] = false;
                    self.component[w] = self.next_component;
                    if w == v {
                        break;
                    }
                }
                self.next_component += 1;
            }
        }
    }
    let mut tarjan = Tarjan {
        outgoing,
        scheduled,
        feedback_edge,
        index: vec![None; nodes.len()],
        low: vec![0; nodes.len()],
        on_stack: vec![false; nodes.len()],
        stack: Vec::new(),
        component: vec![usize::MAX; nodes.len()],
        next_index: 0,
        next_component: 0,
    };
    for v in 0..nodes.len() {
        if !scheduled[v] && tarjan.index[v].is_none() {
            tarjan.visit(v);
        }
    }
    let mut source_component = vec![true; tarjan.next_component];
    for v in 0..nodes.len() {
        if scheduled[v] {
            continue;
        }
        for &(w, edge) in &outgoing[v] {
            if !scheduled[w] && !feedback_edge[edge] && tarjan.component[v] != tarjan.component[w] {
                source_component[tarjan.component[w]] = false;
            }
        }
    }
    (0..nodes.len())
        .filter(|v| !scheduled[*v] && source_component[tarjan.component[*v]])
        .min_by(|a, b| {
            nodes[*a]
                .x
                .total_cmp(&nodes[*b].x)
                .then_with(|| nodes[*a].y.total_cmp(&nodes[*b].y))
                .then_with(|| nodes[*a].id.cmp(&nodes[*b].id))
        })
        .expect("an unscheduled node exists")
}

impl Project {
    /// Musical positions and BPM use quarter notes regardless of meter.
    pub fn initial_meter(&self) -> (u8, u8) {
        self.score
            .as_ref()
            .and_then(|s| s.meters.first())
            .filter(|m| m.beat == 0.)
            .map(|m| (m.beats, m.unit))
            .unwrap_or((self.beats_per_bar, self.beat_unit))
    }
    pub fn quarter_beats_per_bar(&self) -> f64 {
        let (beats, unit) = self.initial_meter();
        beats as f64 * 4. / unit as f64
    }

    /// Synth voices used to have only decay and release, where a positive decay
    /// made every voice a one-shot that fell to silence. With the full per-voice
    /// ADSR that contour is decay to a sustain of 0, so a saved decay without a
    /// saved sustain keeps its sound. Returns whether anything changed.
    pub fn upgrade_legacy_envelopes(&mut self) -> bool {
        let mut changed = false;
        for node in &mut self.graph.nodes {
            if matches!(node.kind.as_str(), "synth" | "fm_synth")
                && node.parameters.get("decay").is_some_and(|d| *d > 0.)
                && !node.parameters.contains_key("sustain")
            {
                node.parameters.insert("sustain".into(), 0.);
                changed = true;
            }
        }
        changed
    }

    pub fn validate(&self) -> Result<(), String> {
        for (node,user) in &self.local_audio_assignments {
            if user.is_empty() || !self.graph.nodes.iter().any(|n|n.id==*node && n.kind=="browser_input") {return Err("Local audio assignments must name a local audio input and user".into());}
        }
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
        if let Some(score) = &self.score {
            score.validate()?;
            if score
                .spans()
                .len()
                .saturating_mul(self.parts.iter().map(|p| p.notes.len()).sum::<usize>())
                > 320_000
            {
                return Err("Expanded score exceeds the 320,000-note preparation budget".into());
            }
        }
        if self.parts.iter().filter(|p| p.solo).count() > 1 {
            return Err("Only one part may be soloed".into());
        }
        if self.parts.len() > 32 {
            return Err("At most 32 parts".into());
        }
        if self
            .conductor
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 128)
            || self.conducted.count_in_pulses > 32
            || ![1, 2, 4, 8, 16, 32].contains(&self.conducted.pulse_unit)
            || self.conducted.sets.len() > 64
        {
            return Err("Invalid conducted performance settings".into());
        }
        if self.conducted.midi_bindings.len() > 128 { return Err("At most 128 conducted MIDI bindings".into()); }
        let mut midi_targets = BTreeSet::new();
        let mut midi_controls = BTreeSet::new();
        for binding in &self.conducted.midi_bindings {
            binding.validate(self)?;
            if !midi_targets.insert((&binding.action, &binding.target))
                || !midi_controls.insert((&binding.source, &binding.device, binding.status, binding.control))
            { return Err("Duplicate conducted MIDI binding".into()); }
        }
        let mut ids = BTreeSet::new();
        for p in &self.parts {
            score::validate(p)?;
            for staff in &p.staves {
                if staff.instrument_node.as_ref().is_some_and(|id| {
                    !self.graph.nodes.iter().any(|n| {
                        &n.id == id
                            && matches!(
                                n.kind.as_str(),
                                "synth" | "fm_synth" | "input" | "browser_input"
                            )
                    })
                }) {
                    return Err(
                        "Staff instrument must reference a local instrument/input node".into(),
                    );
                }
            }
            score::validate_automation(&p.automation)?;
            if let Some(d) = &p.dynamics {
                score::validate_automation(&[d.lane(p.midi_channel)])?;
            }
            for s in &p.staves {
                if let Some(d) = &s.dynamics {
                    score::validate_automation(
                        &[d.lane(s.midi_channel.unwrap_or(p.midi_channel))],
                    )?;
                }
            }
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
            if !["treble", "bass", "alto", "tenor", "percussion"].contains(&p.clef.as_str())
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
            let mut last_meter = -1.;
            for meter in &p.performance_meters {
                if !meter.beat.is_finite()
                    || meter.beat < 0.
                    || meter.beat >= p.loop_beats
                    || meter.beat <= last_meter
                    || !(1..=16).contains(&meter.beats)
                    || ![1, 2, 4, 8, 16, 32].contains(&meter.unit)
                {
                    return Err("Invalid or unordered part meter change".into());
                }
                last_meter = meter.beat;
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
        let part_ids: BTreeSet<_> = self.parts.iter().map(|p| p.id.as_str()).collect();
        let mut set_ids = BTreeSet::new();
        for set in &self.conducted.sets {
            let mut members = BTreeSet::new();
            if !set_ids.insert(set.id.as_str())
                || set.id.is_empty()
                || set.id.len() > 128
                || set.name.trim().is_empty()
                || set.name.len() > 120
                || set.parts.len() > 32
                || set
                    .parts
                    .iter()
                    .any(|id| !part_ids.contains(id.as_str()) || !members.insert(id.as_str()))
            {
                return Err("Invalid conducted set".into());
            }
        }
        for node in &self.graph.nodes {
            if node
                .part_id
                .as_ref()
                .is_some_and(|id| !self.parts.iter().any(|p| &p.id == id))
            {
                return Err("Node part association must reference a part in this project".into());
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
                script: None,
                sample_choices: vec![],
                part_id: None,
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
        local_audio_assignments: BTreeMap::new(),        score: None,
        schema_version: 1,
        id,
        name,
        mode: mode.clone(),
        revision: 0,
        bpm: 120.,
        beats_per_bar: 4,
        beat_unit: 4,
        conductor: None,
        conducted: ConductedLayout {
            sets: if matches!(mode, Mode::Conducted) {
                vec![ConductedSet {
                    id: "set-1".into(),
                    name: "Set 1".into(),
                    parts: vec!["part-1".into()],
                }]
            } else {
                vec![]
            },
            ..Default::default()
        },
        graph: Graph { nodes, edges },
        parts: vec![Part {
            muted: false,
            solo: false,
            staves: vec![],
            automation: vec![],
            dynamics: None,
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
                    notation: None,
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
            performance_meters: vec![],
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
    #[test]
    fn container_explanations_may_exceed_the_control_text_bound() {
        let mut graph = demo_project("t".into(), "t".into(), Mode::Freeform).graph;
        let mut frame = graph.nodes[0].clone();
        frame.id = "frame".into();
        frame.kind = "container".into();
        frame.parameters.clear();
        frame.channels = 1;
        frame.control_value = Some(ControlValue::Text("x".repeat(1500)));
        graph.nodes = vec![frame.clone()];
        graph.edges.clear();
        assert!(graph.validate().is_ok(), "1,500-byte explanation should validate");
        frame.control_value = Some(ControlValue::Text("x".repeat(2001)));
        graph.nodes = vec![frame.clone()];
        assert!(graph.validate().is_err(), "2,001 bytes exceeds the container bound");
        frame.control_value = Some(ControlValue::Number(1.));
        graph.nodes = vec![frame];
        assert!(graph.validate().is_err(), "numbers are not explanations");
    }
    use super::*;
    #[test]
    fn osc_node_destinations_accept_dns_and_reject_malformed_endpoints() {
        for address in [
            "localhost:9000",
            "synth.local:65535",
            "osc-1.example.com.:1",
            "127.0.0.1:9000",
        ] {
            assert!(valid_osc_node_destination(address), "{address}");
        }
        for address in [
            "host",
            "host:0",
            "host:65536",
            "host:-1",
            "host:abc",
            "host: 9",
            "bad host:9",
            "http://host:9",
            "-bad:9",
            "bad-:9",
            "a..b:9",
            "256.1.1.1:9",
            "[::1]:9",
        ] {
            assert!(!valid_osc_node_destination(address), "{address}");
        }
    }
    #[test]
    fn part_note_contract_requires_a_local_part_and_numeric_ports() {
        let mut p = demo_project("x".into(), "x".into(), Mode::Structured);
        let mut node = p.graph.nodes[0].clone();
        node.id = "notes".into();
        node.kind = "part_midi".into();
        node.parameters.clear();
        node.part_id = Some(p.parts[0].id.clone());
        p.graph.nodes.push(node);
        assert!(p.validate().is_ok());
        p.graph.nodes.last_mut().unwrap().part_id = Some("another-project-part".into());
        assert!(p.validate().is_err());
        p.graph.nodes.last_mut().unwrap().part_id = None;
        assert!(p.validate().is_ok());
        let mut monitor = p.graph.nodes[0].clone();
        monitor.id = "performer-monitor".into();
        monitor.kind = "monitor_output".into();
        monitor.part_id = Some(p.parts[0].id.clone());
        p.graph.nodes.push(monitor);
        assert!(p.validate().is_ok());
        p.graph.nodes[0].part_id = Some(p.parts[0].id.clone());
        assert!(p.validate().is_err());
        let catalog = catalog();
        for kind in ["part_midi", "midi_input", "osc_to_midi", "piano"] {
            let d = catalog.iter().find(|d| d.kind == kind).unwrap();
            for name in ["pitch", "velocity", "gate", "trigger", "note_off"] {
                assert!(
                    d.outputs
                        .iter()
                        .any(|p| p.id == name && p.signal == Signal::Control)
                );
            }
        }
        for kind in ["poly_sampler", "midi_output", "midi_to_osc", "piano"] {
            let d = catalog.iter().find(|d| d.kind == kind).unwrap();
            for name in ["pitch", "velocity", "gate", "trigger", "note_off"] {
                assert!(
                    d.inputs
                        .iter()
                        .any(|p| p.id == name && p.signal == Signal::Control)
                );
            }
        }
    }
    #[test]
    fn conducted_midi_bindings_validate_targets_devices_channels_and_uniqueness() {
        let mut p = demo_project("id".into(), "test".into(), Mode::Conducted);
        let b = ConductedMidiBinding {action:"part".into(),target:p.parts[0].id.clone(),source:"server".into(),device:"Keys".into(),status:0x92,control:60};
        p.conducted.midi_bindings.push(b.clone());
        assert!(p.validate().is_ok());
        let encoded=serde_json::to_value(&p).unwrap();
        assert_eq!(serde_json::from_value::<Project>(encoded).unwrap().conducted.midi_bindings[0], b);
        p.conducted.midi_bindings.push(b.clone());assert!(p.validate().is_err());p.conducted.midi_bindings.pop();
        for invalid in [
            ConductedMidiBinding {target:"missing".into(),..b.clone()},
            ConductedMidiBinding {source:"remote-user".into(),..b.clone()},
            ConductedMidiBinding {device:"".into(),..b.clone()},
            ConductedMidiBinding {status:0xf8,..b.clone()},
            ConductedMidiBinding {control:128,..b.clone()},
            ConductedMidiBinding {action:"select_set".into(),target:"".into(),..b.clone()},
        ] {p.conducted.midi_bindings[0]=invalid;assert!(p.validate().is_err());}
        p.conducted.midi_bindings[0]=ConductedMidiBinding {action:"select_set".into(),target:"".into(),status:0xb7,..b};
        assert!(p.validate().is_ok());
        let mut legacy=serde_json::to_value(&p).unwrap();legacy["conducted"].as_object_mut().unwrap().remove("midi_bindings");
        assert!(serde_json::from_value::<Project>(legacy).unwrap().conducted.midi_bindings.is_empty());
    }

    #[test]
    fn conducted_sets_and_part_meters_are_server_validated() {
        let mut p = demo_project("p".into(), "p".into(), Mode::Conducted);
        assert!(p.validate().is_ok());
        p.conducted.sets[0].parts.push("missing".into());
        assert!(p.validate().unwrap_err().contains("conducted set"));
        p.conducted.sets[0].parts.pop();
        p.parts[0].performance_meters = vec![score::MeterChange {
            beat: 0.,
            beats: 6,
            unit: 8,
        }];
        assert!(p.validate().is_ok());
        p.parts[0].performance_meters[0].unit = 3;
        assert!(p.validate().unwrap_err().contains("part meter"));
    }
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
    fn feedback_loops_start_at_the_leftmost_node_and_reject_midi_cycles() {
        let p = demo_project("x".into(), "x".into(), Mode::Freeform);
        let make = |id: &str, kind: &str, x: f64| {
            let mut n = p.graph.nodes[0].clone();
            n.id = id.into();
            n.kind = kind.into();
            n.channels = 2;
            n.x = x;
            n.y = 0.;
            n.parameters.clear();
            n
        };
        let edge = |source: &str, sp: &str, target: &str, tp: &str| Edge {
            id: format!("{source}->{target}.{tp}"),
            source: source.into(),
            source_port: sp.into(),
            target: target.into(),
            target_port: tp.into(),
        };
        let g = Graph {
            nodes: vec![
                make("osc", "oscillator", -300.),
                make("mix", "mixer", 0.),
                make("delay", "delay", 300.),
                make("gain", "gain", 600.),
            ],
            edges: vec![
                edge("osc", "out", "mix", "a"),
                edge("mix", "out", "delay", "in"),
                edge("delay", "out", "gain", "in"),
                edge("gain", "out", "mix", "b"),
            ],
        };
        let schedule = g.schedule().unwrap();
        assert_eq!(schedule.feedback, vec![3]);
        let names = |order: &[usize]| {
            order
                .iter()
                .map(|i| g.nodes[*i].id.as_str())
                .collect::<Vec<_>>()
        };
        assert_eq!(names(&schedule.order), ["osc", "mix", "delay", "gain"]);
        assert!(g.validate().is_ok());
        // Drawing the delay furthest left makes it the loop start instead.
        let mut left = g.clone();
        left.nodes[2].x = -600.;
        let schedule = left.schedule().unwrap();
        assert_eq!(schedule.feedback, vec![1]);
        assert_eq!(schedule.order[1], 2);
        // A self-loop is the smallest cycle.
        let looped = Graph {
            nodes: vec![make("sum", "add", 0.)],
            edges: vec![edge("sum", "out", "sum", "b")],
        };
        assert_eq!(looped.schedule().unwrap().feedback, vec![0]);
        let midi = Graph {
            nodes: vec![make("a", "piano", 0.), make("b", "piano", 300.)],
            edges: vec![
                edge("a", "midi", "b", "midi"),
                edge("b", "midi", "a", "midi"),
            ],
        };
        assert!(
            midi.validate()
                .unwrap_err()
                .contains("Feedback loops may only close")
        );
    }
    #[test]
    fn midi_inputs_accept_multiple_sources_audio_inputs_do_not() {
        let p = demo_project("x".into(), "x".into(), Mode::Freeform);
        let make = |id: &str, kind: &str| {
            let mut n = p.graph.nodes[0].clone();
            n.id = id.into();
            n.kind = kind.into();
            n.channels = 2;
            n.parameters.clear();
            n
        };
        let edge = |source: &str, sp: &str, target: &str, tp: &str| Edge {
            id: format!("{source}-{target}-{tp}"),
            source: source.into(),
            source_port: sp.into(),
            target: target.into(),
            target_port: tp.into(),
        };
        let midi = Graph {
            nodes: vec![make("a", "piano"), make("b", "piano"), make("s", "synth")],
            edges: vec![
                edge("a", "midi", "s", "midi"),
                edge("b", "midi", "s", "midi"),
            ],
        };
        assert!(midi.validate().is_ok());
        let audio = Graph {
            nodes: vec![
                make("x", "oscillator"),
                make("y", "oscillator"),
                make("g", "gain"),
            ],
            edges: vec![edge("x", "out", "g", "in"), edge("y", "out", "g", "in")],
        };
        assert!(
            audio
                .validate()
                .unwrap_err()
                .contains("already has a driver")
        );
    }
    #[test]
    fn control_cycles_schedule_as_feedback_edges() {
        let mut p = demo_project("x".into(), "x".into(), Mode::Freeform);
        p.graph.edges.push(Edge {
            id: "loop".into(),
            source: "mod".into(),
            source_port: "out".into(),
            target: "count".into(),
            target_port: "trigger".into(),
        });
        assert!(p.validate().is_ok());
        let flat = p.graph.flatten().unwrap();
        let schedule = flat.schedule().unwrap();
        assert_eq!(schedule.feedback.len(), 1);
        assert_eq!(schedule.order.len(), flat.nodes.len());
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
    fn field_node(template: &Node, choices: &[u32]) -> Node {
        let mut n = template.clone();
        n.id = "field".into();
        n.kind = "granular_field".into();
        n.channels = 2;
        n.parameters.clear();
        n.sample_choices = choices
            .iter()
            .map(|asset| SampleChoice { asset: *asset, name: format!("S{asset}"), nickname: String::new() })
            .collect();
        n
    }
    fn field_graph(template: &Node, choices: &[u32], edges: Vec<Edge>) -> Graph {
        let mut value = template.clone();
        value.id = "v".into();
        value.kind = "value".into();
        value.channels = 1;
        value.parameters.clear();
        let mut input = template.clone();
        input.id = "mic".into();
        input.kind = "input".into();
        input.channels = 2;
        input.parameters.clear();
        Graph { nodes: vec![field_node(template, choices), value, input], edges }
    }
    fn wire(source: &str, source_port: &str, target_port: &str) -> Edge {
        Edge { id: format!("{source}-{target_port}"), source: source.into(), source_port: source_port.into(), target: "field".into(), target_port: target_port.into() }
    }
    #[test]
    fn granular_field_descriptor_has_two_audio_inputs_and_only_live_parameters() {
        let catalog = catalog();
        let d = catalog.iter().find(|d| d.kind == "granular_field").expect("granular_field in catalog");
        assert_eq!(d.inputs.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(), ["live_1", "live_2"]);
        assert!(d.inputs.iter().all(|p| p.signal == Signal::Audio), "no control or MIDI inlets");
        assert_eq!(d.outputs.len(), 1);
        assert_eq!(d.parameters.len(), 10 + GRANULAR_FIELD_SAMPLES * 5 + GRANULAR_FIELD_LIVE * 5);
        assert!(d.parameters.iter().all(|p| !p.structural), "every setting changes live");
        let find = |id: &str| d.parameters.iter().find(|p| p.id == id).unwrap();
        assert_eq!((find("x").min, find("x").max, find("y").default), (-1., 1., 0.));
        assert_eq!((find("focus").min, find("focus").max, find("focus").default), (0.05, 2., 0.5));
        assert_eq!((find("randomize_pitch").min, find("randomize_pitch").max, find("randomize_pitch").default), (0., 1., 0.));
        assert_eq!((find("pitch").min, find("pitch").max), (-24., 24.));
        assert_eq!(find("sample_8").max, 1000000000.);
        assert_eq!((find("live_2_buffer_ms").min, find("live_2_buffer_ms").max, find("live_2_buffer_ms").default), (100., 10000., 500.));
        assert_eq!(find("source_1_x").default, 0.7, "fresh slots sit on a ring");
        assert!(d.documentation.is_some());
    }
    #[test]
    fn granular_field_validation_gates_sample_lists_slots_and_whole_numbers() {
        let template = demo_project("x".into(), "x".into(), Mode::Freeform).graph.nodes[0].clone();
        assert!(field_graph(&template, &[1, 2, 3, 4, 5, 6, 7, 8], vec![]).validate().is_ok());
        let err = field_graph(&template, &[1, 2, 3, 4, 5, 6, 7, 8, 9], vec![]).validate().unwrap_err();
        assert!(err.contains("at most 8"), "{err}");
        let mut other = field_graph(&template, &[], vec![]);
        other.nodes[1].sample_choices = vec![SampleChoice { asset: 1, name: "s".into(), nickname: String::new() }];
        assert!(other.validate().unwrap_err().contains("Sample lists belong"));
        assert!(field_graph(&template, &[1, 2], vec![wire("v", "out", "sample_2")]).validate().is_ok());
        let err = field_graph(&template, &[1, 2], vec![wire("v", "out", "sample_3")]).validate().unwrap_err();
        assert!(err.contains("not configured"), "{err}");
        assert!(field_graph(&template, &[], vec![wire("mic", "out", "live_1"), wire("v", "out", "x")]).validate().is_ok(), "live ports are always connectable");
        assert!(field_graph(&template, &[], vec![wire("mic", "out", "live_2")]).validate().is_ok());
        let mut fractional = field_graph(&template, &[1], vec![]);
        fractional.nodes[0].parameters.insert("sample_1".into(), 1.5);
        assert!(fractional.validate().unwrap_err().contains("whole numbers"));
        let mut tuned = field_graph(&template, &[1], vec![]);
        tuned.nodes[0].parameters.insert("source_1_tune".into(), 3.5);
        assert!(tuned.validate().is_ok(), "tune is continuous");
        let mut wide = field_graph(&template, &[], vec![wire("mic", "out", "live_1")]);
        wide.nodes[2].channels = 4;
        assert!(wide.validate().is_err(), "audio widths must match like any audio port");
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

#[cfg(test)]
mod synth_contract_tests {
    use super::*;
    #[test]
    fn midi_contract_mono_defaults_and_fm_validation() {
        let descriptors = catalog();
        for kind in ["synth", "fm_synth"] {
            let d = descriptors.iter().find(|d| d.kind == kind).unwrap();
            assert_eq!(d.default_channels, 1);
            assert_eq!(
                d.inputs.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
                ["pitch", "velocity", "gate", "trigger", "note_off", "midi"]
            );
            assert_eq!(d.inputs.last().unwrap().signal, Signal::Midi);
            assert!(d.inputs[..5].iter().all(|p| p.signal == Signal::Control));
        }
        let mut p = demo_project("p".into(), "p".into(), Mode::Freeform);
        p.parts.clear();
        p.graph.edges.clear();
        p.graph.nodes.truncate(1);
        let n = &mut p.graph.nodes[0];
        n.kind = "fm_synth".into();
        n.parameters.clear();
        n.channels = 8;
        assert!(p.validate().is_ok());
        for key in ["carrier_waveform", "modulator_waveform"] {
            for value in [-1., 0.5, 5.] {
                p.graph.nodes[0].parameters.insert(key.into(), value);
                assert!(p.validate().is_err());
            }
            p.graph.nodes[0].parameters.insert(key.into(), 4.);
            assert!(p.validate().is_ok());
        }
        p.graph.nodes[0].channels = 9;
        assert!(p.validate().is_err());
    }
}
