//! Prepared DSP graph. `render` performs no allocation or locking.
mod channels;
mod clock_ratio;
mod convolution;
pub mod count_in;
mod effects;
mod envelope;
mod granular;
mod looper;
mod midi_controls;
mod midi_events;
mod named;
pub mod note_inputs;
mod oscillator;
mod pitch_tracker;
pub mod recorder;
mod sampler;
mod sequence;
mod spectral;
pub mod stretch;
mod visualizer;

use pr0_core::{
    DEVICE_ROUTE_KEYS, Graph, MAX_CHANNELS, MAX_DEVICE_CHANNELS, catalog, device_channel,
};
use std::collections::BTreeMap;
use std::f64::consts::{PI, TAU};

pub const BLOCK: usize = 128;

/// Minimal source-plugin contract. Construct and prepare outside the audio thread.
pub trait AudioPlugin: Send {
    fn prepare(&mut self, sample_rate: f64, channels: usize);
    fn reset(&mut self);
    fn process(&mut self, input: &[f32], output: &mut [f32]);
    fn latency_samples(&self) -> usize {
        0
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Clock {
    pub sample: u64,
    pub reset_generation: u64,
    pub beat: f64,
    pub bpm: f64,
    pub running: bool,
    pub sample_rate: f64,
    pub bar_beats: f64,
    pub beat_length: f64,
}
impl Clock {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample: 0,
            reset_generation: 0,
            beat: 0.,
            bpm: 120.,
            running: false,
            sample_rate,
            bar_beats: 4.,
            beat_length: 1.,
        }
    }
    pub fn advance(&mut self) {
        self.sample += 1;
        if self.running {
            self.beat += self.bpm / (60. * self.sample_rate);
        }
    }
    pub fn set_tempo(&mut self, bpm: f64) {
        self.bpm = bpm.clamp(1., 400.);
    }
    pub fn stop(&mut self) {
        self.reset_generation = self.reset_generation.wrapping_add(1);
        self.running = false;
        self.beat = 0.;
    }
}

#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f64,
    b1: f64,
    b2: f64,
    a1: f64,
    a2: f64,
    z1: f64,
    z2: f64,
}
impl Biquad {
    fn configure(&mut self, frequency: f64, q: f64, high: bool, sr: f64) {
        let w = TAU * frequency.clamp(1., sr * 0.45) / sr;
        let c = w.cos();
        let a = w.sin() / (2. * q);
        let a0 = 1. + a;
        let (b0, b1, b2) = if high {
            ((1. + c) / 2., -(1. + c), (1. + c) / 2.)
        } else {
            ((1. - c) / 2., 1. - c, (1. - c) / 2.)
        };
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = -2. * c / a0;
        self.a2 = (1. - a) / a0;
    }
    fn tick(&mut self, x: f64) -> f64 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

#[derive(Clone)]
struct Binding {
    source: usize,
    edge: usize,
    merge: Option<usize>,
    midi_lane: Option<usize>,
    source_port: usize,
    destination: usize,
    parameter: bool,
    signal: pr0_core::Signal,
}
struct ControlMerge {
    members: Vec<usize>,
    previous: Vec<visualizer::Datum>,
    value: visualizer::Datum,
    winner: usize,
    event: bool,
}
// A complete MIDI bundle is decoded before scalar arbitration so unrelated
// sources cannot replace its pitch or hide its release pulse. Prepared off-render.
struct MidiLane {
    source: usize,
    decoder: note_inputs::NoteInputs,
    values: [f64; 5],
}
const GRAPH_VOICE_OWNER: u64 = u64::MAX;
#[derive(Clone, Copy, Default)]
struct Voice {
    owner: u64,
    graph_lane: usize,
    pitch_ratio: f64,
    note_id: u32,
    order: u64,
    mod_phase: f64,
    pitch: u8,
    phase: f64,
    level: f64,
    releasing: bool,
}
struct RuntimeNode {
    midi_pending: Box<midi_events::Buffer>,
    midi_frame: Box<midi_events::Buffer>,
    clock_ratio: clock_ratio::ClockRatio,
    channel_map: channels::ChannelMap,
    adsr: envelope::Adsr,
    steps: sequence::Steps,
    sample: Vec<[f32; 8]>,
    sample_position: usize,
    spectral: Option<Box<spectral::Spectral>>,
    id: String,
    kind: String,
    channels: usize,
    names: Vec<String>,
    values: Vec<f64>,
    defaults: Vec<f64>,
    limits: Vec<(f64, f64)>,
    bindings: Vec<Binding>,
    merges: Vec<ControlMerge>,
    route: Option<Box<named::Route>>,
    target_text: Option<visualizer::Text>,
    input_events: [bool; 8],
    control_event: bool,
    control_event_only: bool,
    input_event_only: [bool; 8],
    compensations: Vec<Vec<[f64; 8]>>,
    compensation_cursors: Vec<usize>,
    latency: usize,
    input: [[f64; MAX_CHANNELS]; 8],
    output: [f64; MAX_CHANNELS],
    control: [f64; 8],
    part_id: Option<String>,
    io: Option<pr0_core::IoConfig>,
    note_inputs: note_inputs::NoteInputs,
    midi_lanes: Vec<MidiLane>,
    outgoing_notes: [Option<note_inputs::NoteEvent>; 2],
    sampler: Option<Box<sampler::Sampler>>,
    granular: Option<Box<granular::Granular>>,
    pitch_shift: Option<Box<granular::PitchShift>>,
    convolution: Option<Box<convolution::Convolution>>,
    pitch_tracker: Option<Box<pitch_tracker::PitchTracker>>,
    looper: Option<Box<looper::Looper>>,
    recorder: Option<Box<recorder::Recorder>>,
    midi_controls: Option<Box<midi_controls::MidiControls>>,
    control_text: Option<visualizer::Text>,
    input_text: Option<visualizer::Text>,
    toggle_text: Option<visualizer::Text>,
    fallback: visualizer::Datum,
    analyzer: Option<Box<visualizer::Analyzer>>,
    voices: [Voice; 64],
    voice_order: u64,
    external: [f32; MAX_DEVICE_CHANNELS],
    external_set: bool,
    bang: bool,
    phase: f64,
    previous: f64,
    count: f64,
    seed: u64,
    envelope: f64,
    smooth: f64,
    frequency_driven: bool,
    filters: [[Biquad; 3]; MAX_CHANNELS],
    first: [(f64, f64); MAX_CHANNELS],
    filter_cutoff: f64,
    delay: Vec<[f32; MAX_CHANNELS]>,
    cursor: usize,
    eq: [[effects::Section; 5]; 8],
    vocoder: Option<Box<effects::Vocoder>>,
    eq_previous: [f64; 15],
    reverb: Vec<[f64; 8]>,
}
impl RuntimeNode {
    fn synth_note(&mut self, owner: u64, note_id: u32, pitch: u8, velocity: u8) {
        if velocity == 0 {
            for voice in &mut self.voices {
                if voice.owner == owner && voice.note_id == note_id && voice.pitch == pitch {
                    voice.releasing = true;
                }
            }
            return;
        }
        let index = self
            .voices
            .iter()
            .position(|v| v.level < 1e-5)
            .unwrap_or_else(|| {
                self.voices
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| a.level.total_cmp(&b.level))
                    .map(|(i, _)| i)
                    .unwrap_or(0)
            });
        self.voice_order = self.voice_order.wrapping_add(1);
        self.voices[index] = Voice {
            owner,
            graph_lane: 0,
            pitch_ratio: 2_f64.powf((pitch as f64 - 69.) / 12.),
            note_id,
            order: self.voice_order,
            pitch,
            phase: 0.,
            mod_phase: 0.,
            level: velocity.min(127) as f64 / 127.,
            releasing: false,
        };
    }
    fn graph_synth_event(&mut self, event: note_inputs::NoteEvent, lane: usize) {
        if event.velocity == 0 {
            if let Some(voice) = self
                .voices
                .iter_mut()
                .filter(|v| {
                    v.owner == GRAPH_VOICE_OWNER
                        && v.graph_lane == lane
                        && v.pitch == event.pitch
                        && v.level > 0.
                        && !v.releasing
                })
                .min_by_key(|v| v.order)
            {
                voice.releasing = true;
            }
        } else {
            self.synth_note(
                GRAPH_VOICE_OWNER,
                self.voice_order.wrapping_add(1) as u32,
                event.pitch,
                event.velocity,
            );
            let order = self.voice_order;
            if let Some(voice) = self.voices.iter_mut().find(|v| v.order == order) {
                voice.graph_lane = lane;
            }
        }
    }
    fn graph_synth_notes(&mut self) {
        let values = std::array::from_fn(|i| self.input[i][0]);
        let connected = std::array::from_fn(|i| {
            self.bindings
                .iter()
                .any(|b| !b.parameter && b.destination == i && b.midi_lane.is_none())
        });
        for event in self
            .note_inputs
            .tick(values, connected)
            .into_iter()
            .flatten()
        {
            self.graph_synth_event(event, 0);
        }
        for index in 0..self.midi_lanes.len() {
            let lane = &mut self.midi_lanes[index];
            let notes = lane.decoder.tick(lane.values, [true; 5]);
            for event in notes.into_iter().flatten() {
                self.graph_synth_event(event, index + 1);
            }
        }
    }
    fn device_frame(&self) -> [f32; MAX_DEVICE_CHANNELS] {
        let mut frame = [0.; MAX_DEVICE_CHANNELS];
        for (ch, key) in DEVICE_ROUTE_KEYS.iter().enumerate().take(self.channels) {
            if let Some(destination) = device_channel(self.p(key), ch, 0) {
                frame[destination] += self.output[ch] as f32;
            }
        }
        frame
    }
    fn p(&self, name: &str) -> f64 {
        self.names
            .iter()
            .position(|p| p == name)
            .map(|i| self.values[i])
            .unwrap_or(0.)
    }
    fn process(&mut self, clock: &Clock, hardware: &[f32; MAX_CHANNELS]) {
        let sr = clock.sample_rate;
        let a = self.p("a");
        let b = self.p("b");
        let input = self.input[0];
        let mut scalar = 0.;
        self.output = [0.; MAX_CHANNELS];
        self.control_text = None;
        if self.kind != "receive_control" {
            self.control_event_only = false;
        }
        match self.kind.as_str() {
            "subgraph_input_audio" | "subgraph_output_audio" => self.output = input,
            "subgraph_input_control" | "subgraph_output_control" => {
                scalar = input[0];
                self.control_text = self.input_text;
                self.control_event = self.input_events[0];
                self.control_event_only = self.input_event_only[0];
            }
            "audio_to_control" => scalar = input[0] * self.p("scale") + self.p("offset"),
            "control_visualizer" | "control_input" => {
                if self.bindings.is_empty() {
                    if self.kind == "control_input" && self.p("mode") == 0. {
                        scalar = self.bang as u8 as f64;
                        self.bang = false;
                    } else {
                        match self.fallback {
                            visualizer::Datum::Number(value) => scalar = value,
                            visualizer::Datum::Text(value) => self.control_text = Some(value),
                        }
                    }
                } else {
                    scalar = input[0];
                    self.control_text = self.input_text;
                    self.control_event = self.input_events[0];
                    self.control_event_only = self.input_event_only[0];
                }
            }
            "audio_visualizer" => {
                self.output = input;
                self.analyzer.as_mut().unwrap().capture(&input);
            }
            "spectral_visualizer" => {}
            "add" => scalar = a + b,
            "subtract" => scalar = a - b,
            "multiply" => scalar = a * b,
            "divide" => scalar = if b == 0. { 0. } else { a / b },
            "modulo" => scalar = if b == 0. { 0. } else { a.rem_euclid(b) },
            "power" => scalar = a.powf(b),
            "min" => scalar = a.min(b),
            "max" => scalar = a.max(b),
            "greater" => scalar = (a > b) as u8 as f64,
            "less" => scalar = (a < b) as u8 as f64,
            "equal" => scalar = (a == b) as u8 as f64,
            "and" => scalar = (a != 0. && b != 0.) as u8 as f64,
            "or" => scalar = (a != 0. || b != 0.) as u8 as f64,
            "not" => scalar = (a == 0.) as u8 as f64,
            "abs" => scalar = a.abs(),
            "sqrt" => scalar = a.sqrt(),
            "sin" => scalar = a.sin(),
            "cos" => scalar = a.cos(),
            "log" => scalar = a.ln(),
            "exp" => scalar = a.exp(),
            "mtof" => scalar = 440. * 2_f64.powf((a - 69.) / 12.),
            "ftom" => scalar = 69. + 12. * (a / 440.).log2(),
            "dbtoa" => scalar = 10_f64.powf(a / 20.),
            "atodb" => scalar = 20. * a.abs().max(1e-9).log10(),
            "clamp" => scalar = a.max(self.p("min")).min(self.p("max")),
            "scale" => scalar = self.p("min") + a * (self.p("max") - self.p("min")),
            "piano" => {
                let values = std::array::from_fn(|i| self.input[i][0]);
                let connected = std::array::from_fn(|i| {
                    self.bindings
                        .iter()
                        .any(|b| !b.parameter && b.destination == i)
                });
                let notes = self.note_inputs.tick(values, connected);
                let midi = self.midi_controls.as_mut().unwrap();
                for note in notes.into_iter().flatten() {
                    midi.note(note.pitch, note.velocity);
                }
                self.control[..5].copy_from_slice(&midi.tick());
                scalar = self.control[0];
            }
            "midi_to_control" => {
                self.control[4] = 0.;
                for index in 0..self.midi_frame.len {
                    let event = self.midi_frame.events[index];
                    if (self.p("message_type") == 0.
                        || self.p("message_type") == f64::from(event.status >> 4))
                        && (self.p("number_filter") < 0.
                            || self.p("number_filter") == f64::from(event.data1))
                    {
                        self.control[..5].copy_from_slice(&[
                            f64::from(event.status >> 4),
                            f64::from(event.data1),
                            if event.status >> 4 == 14 {
                                f64::from(u16::from(event.data1) + (u16::from(event.data2) << 7))
                            } else if matches!(event.status >> 4, 12 | 13) {
                                f64::from(event.data1)
                            } else {
                                f64::from(event.data2)
                            },
                            f64::from((event.status & 15) + 1),
                            1.,
                        ]);
                    }
                }
            }
            "part_midi" | "midi_input" | "osc_to_midi" => {
                self.control[..5].copy_from_slice(&self.midi_controls.as_mut().unwrap().tick());
                self.control[5] = self.control[0];
                self.control[6] = self.control[1];
                scalar = self.control[0];
            }
            "midi_output" | "midi_to_osc" | "poly_sampler" | "granular_synth" => {
                let mut values = std::array::from_fn(|i| self.input[i][0]);
                let mut connected = std::array::from_fn(|i| {
                    self.bindings
                        .iter()
                        .any(|b| !b.parameter && b.destination == i)
                });
                // Keep older MIDI number/value connections valid as aliases.
                if self.kind == "midi_output" {
                    for (index, alias) in [(0, 5), (1, 6)] {
                        if !connected[index]
                            && self
                                .bindings
                                .iter()
                                .any(|b| !b.parameter && b.destination == alias)
                        {
                            values[index] = self.input[alias][0];
                            connected[index] = true;
                        }
                    }
                }
                let notes = self.note_inputs.tick(values, connected);
                if self.kind == "granular_synth" {
                    for note in notes.into_iter().flatten() {
                        if clock.running || note.velocity == 0 {
                            self.granular
                                .as_mut()
                                .unwrap()
                                .note(note.pitch, note.velocity);
                        }
                    }
                    let settings = granular::Settings {
                        root: self.p("root_note"),
                        position: self.p("position"),
                        spray_ms: self.p("spray"),
                        grain_ms: self.p("grain_ms"),
                        density: self.p("density"),
                        amplitude: self.p("amplitude"),
                        release_ms: self.p("release"),
                    };
                    self.output = self.granular.as_mut().unwrap().render(
                        &self.sample,
                        self.channels,
                        sr,
                        settings,
                    );
                } else if self.kind == "poly_sampler" {
                    for note in notes.into_iter().flatten() {
                        if clock.running || note.velocity == 0 {
                            self.sampler
                                .as_mut()
                                .unwrap()
                                .note(note.pitch, note.velocity);
                        }
                    }
                    let (root, amplitude, looping, release) = (
                        self.p("root_note"),
                        self.p("amplitude"),
                        self.p("loop") > 0.,
                        self.p("release"),
                    );
                    self.output = self.sampler.as_mut().unwrap().render(
                        &self.sample,
                        root,
                        amplitude,
                        looping,
                        release,
                        sr,
                        clock.running,
                    );
                } else {
                    self.outgoing_notes = notes;
                }
            }
            "clock" => {
                scalar =
                    if clock.running && (clock.beat.floor() != self.previous || clock.beat == 0.) {
                        1.
                    } else {
                        0.
                    };
                self.previous = clock.beat.floor();
                self.control[1] = clock.beat;
                self.control[2] = clock.bpm;
            }
            "clock_ratio" => {
                let values =
                    self.clock_ratio
                        .tick_clock(clock, self.p("multiply"), self.p("divide"));
                self.control[..3].copy_from_slice(&values);
                scalar = self.control[0];
            }
            "metro" => {
                self.phase += self.p("bpm") / (60. * sr);
                if self.phase >= 1. {
                    self.phase -= 1.;
                    scalar = if self.p("enabled") > 0. { 1. } else { 0. };
                }
            }
            "counter" => {
                let trig = input[0];
                if self.input[1][0] > 0. {
                    self.count = self.p("min");
                } else if trig > 0. && self.previous <= 0. {
                    self.count += self.p("step");
                    if self.count > self.p("max") {
                        self.count = self.p("min");
                    }
                    if self.count < self.p("min") {
                        self.count = self.p("max");
                    }
                }
                self.previous = trig;
                scalar = self.count;
            }
            "step_sequencer" => {
                let (index, pulse) = self.steps.tick(
                    input[0] > 0.,
                    self.input[1][0] > 0.,
                    self.p("enabled") > 0.,
                    self.p("length") as usize,
                );
                // Descriptor order is length, enabled, then the 16 step values.
                scalar = self.values[2 + index];
                self.control[1] = index as f64;
                self.control[2] = pulse as u8 as f64;
            }
            "adsr" => {
                let settings = envelope::Settings {
                    attack: self.p("attack"),
                    decay: self.p("decay"),
                    sustain: self.p("sustain"),
                    release: self.p("release"),
                };
                scalar = self
                    .adsr
                    .tick(self.p("gate") > 0., input[0] > 0., settings, sr);
            }
            "gate" => scalar = if self.p("open") > 0. { input[0] } else { 0. },
            "send_control" => {
                scalar = input[0];
                self.control_text = self.input_text;
                self.control_event = self.input_events[0];
                self.control_event_only = self.input_event_only[0];
            }
            "receive_control" => match self.route.as_ref().unwrap().value {
                visualizer::Datum::Number(v) => scalar = v,
                visualizer::Datum::Text(t) => self.control_text = Some(t),
            },
            "send_audio" => self.output = input,
            "receive_audio" => self.output = self.route.as_ref().unwrap().audio,
            "send_spectral" | "receive_spectral" => {}
            "convolution" => {
                let mix = self.p("mix");
                let normalize = self.p("normalize") > 0.;
                self.output =
                    self.convolution
                        .as_mut()
                        .unwrap()
                        .tick(input, self.input[1], mix, normalize);
            }
            "granular_pitch_shift" => {
                let shift = self.p("semitones");
                let mix = self.p("mix");
                self.output =
                    self.pitch_shift
                        .as_mut()
                        .unwrap()
                        .tick(input, self.channels, shift, mix);
            }
            "pitch_tracker" => {
                let slots = self.p("slots") as usize;
                let threshold = self.p("threshold");
                let tracker = self.pitch_tracker.as_mut().unwrap();
                tracker.tick(&input[..self.channels], sr, slots, threshold);
                scalar = tracker.notes[0];
                self.control[1..4].copy_from_slice(&tracker.notes[1..4]);
            }
            "value" => {
                let driven = self
                    .bindings
                    .iter()
                    .any(|b| !b.parameter && b.destination == 0);
                if !driven || input[0] != 0. {
                    self.count = self.p("value");
                }
                // A triggered zero is an explicit command, not absence of a signal.
                self.control_event = driven && input[0] != 0.;
                scalar = self.count;
            }
            "random" => {
                if input[0] > 0. && self.previous <= 0. {
                    self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    self.count =
                        ((self.seed >> 32) as f64 / u32::MAX as f64 * self.p("max")).floor();
                }
                self.previous = input[0];
                scalar = self.count;
            }
            "lfo" => {
                self.phase = (self.phase + self.p("rate") / sr).fract();
                scalar = self.p("min")
                    + (0.5 + 0.5 * (TAU * self.phase).sin()) * (self.p("max") - self.p("min"));
            }
            "channel_split" => {
                self.output[..self.channels].copy_from_slice(&input[..self.channels]);
            }
            "channel_merge" => {
                for ch in 0..self.channels {
                    self.output[ch] = self.input[ch][0];
                }
            }
            "channel_map" => {
                let routes = std::array::from_fn(|i| self.values[i] as usize);
                self.output = self.channel_map.tick(input, routes, self.channels, sr);
            }
            "browser_input" => {
                for ch in 0..self.channels {
                    self.output[ch] = self.external[ch] as f64;
                }
            }
            "input" => {
                let offset = self.p("offset") as usize;
                let hardware: &[f32] = if self.external_set || self.p("interface") != 0. {
                    &self.external
                } else {
                    hardware
                };
                for (ch, key) in DEVICE_ROUTE_KEYS.iter().enumerate().take(self.channels) {
                    self.output[ch] = device_channel(self.p(key), ch, offset)
                        .and_then(|index| hardware.get(index))
                        .copied()
                        .unwrap_or(0.) as f64;
                }
            }
            "oscillator" => {
                if self.frequency_driven {
                    self.smooth = self.p("frequency");
                } else {
                    self.smooth +=
                        (self.p("frequency") - self.smooth) * (1. - (-1. / (0.005 * sr)).exp());
                }
                self.phase = (self.phase + self.smooth / sr).fract();
                self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let noise = (self.seed >> 32) as f64 / u32::MAX as f64 * 2. - 1.;
                let v = oscillator::wave(
                    self.phase,
                    self.smooth / sr,
                    self.p("waveform").round() as u32,
                    noise,
                ) * self.p("amplitude");
                self.output[..self.channels].fill(v);
            }
            "record" => {
                self.recorder
                    .as_mut()
                    .unwrap()
                    .tick(input, self.input[1][0], self.input[2][0]);
            }
            "looper" => {
                let mode = self.p("loop_mode");
                let commands = std::array::from_fn(|i| self.input[i + 1][0]);
                self.output = self
                    .looper
                    .as_mut()
                    .unwrap()
                    .tick(input, commands, mode, clock);
            }
            "synth" | "fm_synth" => {
                self.graph_synth_notes();
                let mut sum = 0.;
                let release = (-1. / (self.p("release") * 0.001 * sr)).exp();
                let fm = self.kind == "fm_synth";
                let (carrier, modulator, depth, carrier_shape, modulator_shape) = if fm {
                    (
                        self.p("carrier_frequency"),
                        self.p("modulator_frequency"),
                        self.p("fm_depth"),
                        self.p("carrier_waveform").round() as u32,
                        self.p("modulator_waveform").round() as u32,
                    )
                } else {
                    (440., 0., 0., 0, 0)
                };
                for v in &mut self.voices {
                    if v.level < 1e-5 {
                        v.level = 0.;
                        continue;
                    }
                    let ratio = v.pitch_ratio;
                    let mod_step = (modulator * ratio / sr).min(0.49);
                    v.mod_phase = (v.mod_phase + mod_step).fract();
                    self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let mod_noise = (self.seed >> 32) as f64 / u32::MAX as f64 * 2. - 1.;
                    let modulation = if fm {
                        oscillator::wave(v.mod_phase, mod_step, modulator_shape, mod_noise) * depth
                    } else {
                        0.
                    };
                    let step = ((carrier + modulation) * ratio / sr).clamp(-0.49, 0.49);
                    v.phase = (v.phase + step).rem_euclid(1.);
                    self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                    let carrier_noise = (self.seed >> 32) as f64 / u32::MAX as f64 * 2. - 1.;
                    sum += oscillator::wave(v.phase, step.abs(), carrier_shape, carrier_noise)
                        * v.level;
                    if v.releasing {
                        v.level *= release;
                    }
                }
                let amplitude = self.p("amplitude");
                self.output[..self.channels].fill(sum * amplitude);
            }
            "noise" => {
                self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let v =
                    ((self.seed >> 32) as f64 / u32::MAX as f64 * 2. - 1.) * self.p("amplitude");
                self.output[..self.channels].fill(v);
            }
            "gain" | "mixer" | "output" | "monitor_output" => {
                let target = 10_f64.powf(self.p("gain") / 20.);
                self.smooth += (target - self.smooth) * (1. - (-1. / (0.005 * sr)).exp());
                for ch in 0..self.channels {
                    self.output[ch] = (input[ch]
                        + if self.kind == "mixer" {
                            self.input[1][ch]
                        } else {
                            0.
                        })
                        * self.smooth;
                }
                scalar = self.smooth;
            }
            "meter" => {
                self.output = input;
                scalar = input[..self.channels]
                    .iter()
                    .fold(0_f64, |m, x| m.max(x.abs()));
            }
            "envelope" => {
                let peak = input[..self.channels]
                    .iter()
                    .fold(0_f64, |m, x| m.max(x.abs()));
                self.envelope =
                    peak.max(self.envelope * (-1. / (self.p("release") * 0.001 * sr)).exp());
                scalar = self.envelope;
            }
            "lowpass" | "highpass" => {
                let order = self.p("poles") as usize;
                let cutoff = self.p("cutoff").clamp(1., sr * 0.45);
                let high = self.kind == "highpass";
                if (cutoff - self.filter_cutoff).abs() > 0.01 {
                    for k in 0..order / 2 {
                        let q = 1. / (2. * (((2 * k + 1) as f64) * PI / (2. * order as f64)).sin());
                        for ch in 0..self.channels {
                            self.filters[ch][k].configure(cutoff, q, high, sr);
                        }
                    }
                    self.filter_cutoff = cutoff;
                }
                let k = (PI * cutoff / sr).tan();
                let a1 = (k - 1.) / (k + 1.);
                let b0 = if high { 1. / (k + 1.) } else { k / (k + 1.) };
                let b1 = if high { -b0 } else { b0 };
                for ch in 0..self.channels {
                    let mut x = input[ch];
                    if order % 2 == 1 {
                        let (px, py) = self.first[ch];
                        let y = b0 * x + b1 * px - a1 * py;
                        self.first[ch] = (x, y);
                        x = y;
                    }
                    for j in 0..order / 2 {
                        x = self.filters[ch][j].tick(x);
                    }
                    self.output[ch] = x;
                }
            }
            "compressor" | "noise_gate" | "limiter" | "expander" => {
                let side = if self
                    .bindings
                    .iter()
                    .any(|b| !b.parameter && b.destination == 1)
                {
                    self.input[1]
                } else {
                    input
                };
                let peak = side[..self.channels]
                    .iter()
                    .fold(0_f64, |m, x| m.max(x.abs()));
                let time = if peak > self.envelope {
                    self.p("attack")
                } else {
                    self.p("release")
                };
                let coef = (-1. / (time * 0.001 * sr)).exp();
                self.envelope = peak + (self.envelope - peak) * coef;
                let db = 20. * self.envelope.max(1e-9).log10();
                let threshold = self.p("threshold");
                let ratio = self.p("ratio");
                let reduction = match self.kind.as_str() {
                    "noise_gate" => {
                        if db < threshold {
                            -90.
                        } else {
                            0.
                        }
                    }
                    "expander" => (db - threshold).min(0.) * (ratio - 1.),
                    "limiter" => (threshold - db).min(0.),
                    _ => (threshold - db).min(0.) * (1. - 1. / ratio),
                };
                let gain = 10_f64.powf((reduction + self.p("makeup")) / 20.);
                self.smooth = gain + (self.smooth - gain) * coef;
                for ch in 0..self.channels {
                    self.output[ch] = input[ch] * self.smooth;
                }
                scalar = reduction;
            }
            "sample" | "phase_vocoder" => {
                if clock.beat < self.previous {
                    self.sample_position = 0;
                }
                self.previous = clock.beat;
                if clock.running && !self.sample.is_empty() {
                    if self.sample_position >= self.sample.len() && self.p("loop") > 0. {
                        self.sample_position = 0;
                    }
                    if let Some(frame) = self.sample.get(self.sample_position) {
                        let gain = self.p("amplitude");
                        for ch in 0..self.channels {
                            self.output[ch] = frame[ch] as f64 * gain;
                        }
                    }
                    self.sample_position += 1;
                }
            }
            "fft" | "rfft" => {
                self.spectral.as_mut().unwrap().forward(input);
            }
            "ifft" | "rifft" => {
                self.output = self.spectral.as_mut().unwrap().inverse();
            }
            "spectral_math" => {
                let factors = [
                    self.p("magnitude_scale"),
                    self.p("magnitude_offset"),
                    self.p("phase_scale"),
                    self.p("phase_offset"),
                ];
                self.spectral.as_mut().unwrap().map_bins(factors, None);
            }
            "spectral_curve" => {
                // The catalog prepares two 33-point curves after size and overlap.
                self.spectral.as_mut().unwrap().map_bins(
                    [1., 0., 1., 0.],
                    Some((&self.values[2..35], &self.values[35..68])),
                );
            }
            "spectral_gain" | "to_polar" | "to_cartesian" => {
                let gain = self.p("gain");
                self.spectral.as_mut().unwrap().transform(&self.kind, gain);
            }
            "toggle" => {
                if self
                    .bindings
                    .iter()
                    .any(|b| !b.parameter && b.destination == 0)
                    && (!self.input_event_only[0] || self.input_events[0])
                    && (input[0] != self.previous
                        || self.input_text != self.toggle_text
                        || self.input_events[0])
                {
                    let next = if self.input_text.is_some() || input[0] > 0. {
                        1.
                    } else {
                        0.
                    };
                    self.bang |= next != self.count;
                    self.count = next;
                }
                if !self.input_event_only[0] || self.input_events[0] {
                    self.previous = input[0];
                    self.toggle_text = self.input_text;
                }
                self.control_event_only = true;
                self.control_event = self.bang;
                self.bang = false;
                scalar = self.count;
            }
            "trigger" => {
                self.control_event_only = self.input_event_only[0];
                self.control_event = self.bang || self.input_events[0];
                scalar = if self.bang { 1. } else { input[0] };
                if self.bang || (scalar != 0. && self.previous == 0.) {
                    self.count += 1.;
                }
                self.bang = false;
                self.previous = scalar;
            }
            "change" => {
                scalar = if input[0] != self.previous { 1. } else { 0. };
                self.previous = input[0];
            }
            "select" => {
                scalar = if input[0] == self.p("value") && input[0] != self.previous {
                    1.
                } else {
                    0.
                };
                self.previous = input[0];
            }
            "sample_hold" => {
                if self.input[1][0] > 0. && self.previous <= 0. {
                    self.count = input[0];
                }
                self.previous = self.input[1][0];
                scalar = self.count;
            }
            "ramp" => {
                self.smooth += (self.p("target") - self.smooth)
                    * (1. - (-1. / (self.p("time") * 0.001 * sr)).exp());
                scalar = self.smooth;
            }
            "pan" => {
                self.output = input;
                let angle = (self.p("pan") + 1.) * PI / 4.;
                if self.channels >= 2 {
                    let mono = (input[0] + input[1]) * 0.5;
                    self.output[0] = mono * angle.cos();
                    self.output[1] = mono * angle.sin();
                }
            }
            "crossfade" => {
                let angle = self.p("mix") * PI / 2.;
                for ch in 0..self.channels {
                    self.output[ch] = input[ch] * angle.cos() + self.input[1][ch] * angle.sin();
                }
            }
            "audio_gate" => {
                self.smooth += (self.p("open") - self.smooth) * (1. - (-1. / (0.005 * sr)).exp());
                for ch in 0..self.channels {
                    self.output[ch] = input[ch] * self.smooth;
                }
            }
            "eq3" | "eq5" => {
                let bands = if self.kind == "eq3" { 3 } else { 5 };
                for band in 0..bands {
                    let hz = self.values[band * 3];
                    let gain = self.values[band * 3 + 1];
                    let q = self.values[band * 3 + 2];
                    if hz != self.eq_previous[band * 3]
                        || gain != self.eq_previous[band * 3 + 1]
                        || q != self.eq_previous[band * 3 + 2]
                    {
                        for ch in 0..self.channels {
                            self.eq[ch][band].peak(hz, q, gain, sr);
                        }
                        self.eq_previous[band * 3] = hz;
                        self.eq_previous[band * 3 + 1] = gain;
                        self.eq_previous[band * 3 + 2] = q;
                    }
                }
                for ch in 0..self.channels {
                    let mut x = input[ch];
                    for band in 0..bands {
                        x = self.eq[ch][band].tick(x);
                    }
                    self.output[ch] = x;
                }
            }
            "bandpass" | "notch" => {
                let hz = self.p("cutoff");
                let q = self.p("q");
                if hz != self.eq_previous[0] || q != self.eq_previous[1] {
                    for ch in 0..self.channels {
                        if self.kind == "bandpass" {
                            self.eq[ch][0].bandpass(hz, q, sr);
                        } else {
                            self.eq[ch][0].notch(hz, q, sr);
                        }
                    }
                    self.eq_previous[0] = hz;
                    self.eq_previous[1] = q;
                }
                for ch in 0..self.channels {
                    self.output[ch] = self.eq[ch][0].tick(input[ch]);
                }
            }
            "vocoder" => {
                let attack = self.p("attack");
                let release = self.p("release");
                let mix = self.p("mix");
                let wet = self.vocoder.as_mut().unwrap().tick(
                    input,
                    self.input[1],
                    self.channels,
                    attack,
                    release,
                );
                for ch in 0..self.channels {
                    self.output[ch] = input[ch] * (1. - mix) + wet[ch] * mix;
                }
            }
            "reverb" => {
                let mix = self.p("mix");
                let decay = self.p("decay");
                for ch in 0..self.channels {
                    let mut sum = 0.;
                    for (j, length) in [1557, 1617, 1491, 1422].iter().enumerate() {
                        let index = j * 2048;
                        let pos = index + (self.cursor % length);
                        let v = self.reverb[pos][ch];
                        self.reverb[pos][ch] = input[ch] + v * decay;
                        sum += v * 0.25;
                    }
                    self.output[ch] = input[ch] * (1. - mix) + sum * mix;
                }
                self.cursor += 1;
            }
            "delay" => {
                let len = (self.p("time") * 0.001 * sr) as usize;
                let pos = (self.cursor + self.delay.len() - len.min(self.delay.len() - 1))
                    % self.delay.len();
                let wet = self.delay[pos];
                let feedback = self.p("feedback");
                let mix = self.p("mix");
                for ch in 0..self.channels {
                    self.delay[self.cursor][ch] = (input[ch] + wet[ch] as f64 * feedback) as f32;
                    self.output[ch] = input[ch] * (1. - mix) + wet[ch] as f64 * mix;
                }
                self.cursor = (self.cursor + 1) % self.delay.len();
            }
            _ => {}
        }
        self.control[0] = if scalar.is_finite() { scalar } else { 0. };
        for value in &mut self.output {
            if !value.is_finite() {
                *value = 0.;
            }
        }
    }
}

pub struct Engine {
    nodes: Vec<RuntimeNode>,
    order: Vec<usize>,
    pub clock: Clock,
    pub graph_clock: Clock,
    pub graph: Graph,
    priority: Vec<usize>,
}
impl Engine {
    pub fn prepare(graph: Graph, sample_rate: f64) -> Result<Self, String> {
        let authored = graph;
        let graph = authored.flatten()?;
        let order = graph.validate_flat()?;
        if !sample_rate.is_finite() || sample_rate <= 0. {
            return Err("Invalid engine sample rate".into());
        }
        let loop_bytes: f64 = graph
            .nodes
            .iter()
            .filter(|n| n.kind == "looper")
            .map(|n| {
                n.parameters.get("max_seconds").copied().unwrap_or(30.)
                    * sample_rate
                    * n.channels as f64
                    * 8.
                    * 4.
            })
            .sum();
        if loop_bytes > 512. * 1024. * 1024. {
            return Err("Loopers exceed 512 MiB of recording memory; reduce capacity, channels or looper count".into());
        }
        let descriptors = catalog();
        let mut nodes = Vec::new();
        for n in &graph.nodes {
            let d = descriptors.iter().find(|d| d.kind == n.kind).unwrap();
            let values: Vec<f64> = d
                .parameters
                .iter()
                .map(|p| n.parameters.get(&p.id).copied().unwrap_or(p.default))
                .collect();
            nodes.push(RuntimeNode {
                clock_ratio: clock_ratio::ClockRatio::default(),
                channel_map: channels::ChannelMap::default(),
                adsr: envelope::Adsr::default(),
                steps: sequence::Steps::default(),
                sample: vec![],
                sample_position: 0,
                spectral: if d.category == "Spectral"
                    || (pr0_core::named_route(&n.kind) && n.kind.ends_with("spectral"))
                {
                    Some(Box::new(spectral::Spectral::new(
                        n.parameters.get("size").copied().unwrap_or(1024.) as usize,
                        n.parameters.get("overlap").copied().unwrap_or(4.) as usize,
                        n.channels,
                    )))
                } else {
                    None
                },
                id: n.id.clone(),
                kind: n.kind.clone(),
                channels: if n.kind == "pitch_tracker" {
                    graph
                        .edges
                        .iter()
                        .find(|e| e.target == n.id && e.target_port == "in")
                        .map(|edge| {
                            let source = graph.nodes.iter().find(|s| s.id == edge.source).unwrap();
                            descriptors
                                .iter()
                                .find(|d| d.kind == source.kind)
                                .unwrap()
                                .outputs
                                .iter()
                                .find(|p| p.id == edge.source_port)
                                .unwrap()
                                .fixed_channels
                                .unwrap_or(source.channels)
                        })
                        .unwrap_or(n.channels)
                } else {
                    n.channels
                },
                names: d.parameters.iter().map(|p| p.id.clone()).collect(),
                defaults: values.clone(),
                values,
                limits: d.parameters.iter().map(|p| (p.min, p.max)).collect(),
                bindings: vec![],
                merges: vec![],
                route: pr0_core::named_route(&n.kind).then(|| {
                    Box::new(named::Route::new(
                        &n.kind,
                        n.control_value.as_ref(),
                        n.channels,
                        n.parameters.get("size").copied().unwrap_or(1024.) as usize,
                        n.parameters.get("overlap").copied().unwrap_or(4.) as usize,
                        graph.nodes.len(),
                    ))
                }),
                target_text: None,
                input_events: [false; 8],
                control_event: false,
                control_event_only: n.kind == "toggle",
                input_event_only: [false; 8],
                compensations: vec![],
                compensation_cursors: vec![],
                latency: 0,
                input: [[0.; MAX_CHANNELS]; 8],
                output: [0.; MAX_CHANNELS],
                control: [0.; 8],
                part_id: n.part_id.clone(),
                io: n.io.clone(),
                note_inputs: note_inputs::NoteInputs::default(),
                midi_lanes: Vec::new(),
                midi_pending: Box::new(midi_events::Buffer::new()),
                midi_frame: Box::new(midi_events::Buffer::new()),
                outgoing_notes: [None; 2],
                recorder: if n.kind == "record" {
                    let width = graph
                        .edges
                        .iter()
                        .find(|e| e.target == n.id && e.target_port == "in")
                        .map(|edge| {
                            let source = graph.nodes.iter().find(|s| s.id == edge.source).unwrap();
                            descriptors
                                .iter()
                                .find(|d| d.kind == source.kind)
                                .unwrap()
                                .outputs
                                .iter()
                                .find(|p| p.id == edge.source_port)
                                .unwrap()
                                .fixed_channels
                                .unwrap_or(source.channels)
                        })
                        .unwrap_or(0);
                    Some(Box::new(recorder::Recorder::new(width)))
                } else {
                    None
                },
                looper: if n.kind == "looper" {
                    Some(Box::new(looper::Looper::prepare(
                        sample_rate,
                        n.channels,
                        n.parameters.get("max_seconds").copied().unwrap_or(30.),
                    )?))
                } else {
                    None
                },
                pitch_tracker: (n.kind == "pitch_tracker").then(|| {
                    Box::new(pitch_tracker::PitchTracker::new(
                        n.parameters.get("fft_size").copied().unwrap_or(8192.) as usize,
                    ))
                }),
                granular: (n.kind == "granular_synth")
                    .then(|| Box::new(granular::Granular::default())),
                pitch_shift: (n.kind == "granular_pitch_shift").then(|| {
                    Box::new(granular::PitchShift::new(
                        sample_rate,
                        n.parameters.get("grain_ms").copied().unwrap_or(20.),
                    ))
                }),
                convolution: (n.kind == "convolution").then(|| {
                    Box::new(convolution::Convolution::new(
                        n.parameters.get("window").copied().unwrap_or(256.) as usize,
                        n.channels,
                    ))
                }),
                sampler: (n.kind == "poly_sampler").then(|| Box::new(sampler::Sampler::default())),
                midi_controls: (matches!(
                    n.kind.as_str(),
                    "part_midi" | "midi_input" | "osc_to_midi" | "piano"
                ))
                .then(|| Box::new(midi_controls::MidiControls::new())),
                control_text: None,
                input_text: None,
                toggle_text: None,
                fallback: if n.kind == "control_input"
                    && n.parameters.get("mode") == Some(&4.)
                    && n.control_value.is_none()
                {
                    visualizer::Datum::prepare(Some(&pr0_core::ControlValue::Text(String::new())))
                } else {
                    visualizer::Datum::prepare(n.control_value.as_ref())
                },
                analyzer: if matches!(n.kind.as_str(), "audio_visualizer" | "spectral_visualizer") {
                    Some(Box::new(visualizer::Analyzer::new(
                        n.parameters.get("size").copied().unwrap_or(1024.) as usize,
                        n.channels,
                    )))
                } else {
                    None
                },
                voices: [Voice::default(); 64],
                voice_order: 0,
                external: [0.; MAX_DEVICE_CHANNELS],
                external_set: false,
                bang: false,
                phase: 0.,
                previous: if n.kind == "trigger" {
                    0.
                } else if n.kind == "toggle" {
                    f64::NAN
                } else {
                    -1.
                },
                count: if n.kind == "toggle"
                    && matches!(n.control_value, Some(pr0_core::ControlValue::Number(1.)))
                {
                    1.
                } else {
                    0.
                },
                seed: n.parameters.get("seed").copied().unwrap_or(1.) as u64,
                envelope: 0.,
                smooth: 0.,
                frequency_driven: graph
                    .edges
                    .iter()
                    .any(|edge| edge.target == n.id && edge.target_port == "frequency"),
                filters: [[Biquad::default(); 3]; MAX_CHANNELS],
                first: [(0., 0.); MAX_CHANNELS],
                filter_cutoff: -1.,
                delay: if n.kind == "delay" {
                    vec![[0.; MAX_CHANNELS]; (sample_rate * 2.) as usize + 1]
                } else {
                    vec![]
                },
                cursor: 0,
                eq: [[effects::Section::default(); 5]; 8],
                vocoder: if n.kind == "vocoder" {
                    Some(Box::new(effects::Vocoder::new(sample_rate)))
                } else {
                    None
                },
                eq_previous: [f64::NAN; 15],
                reverb: if n.kind == "reverb" {
                    vec![[0.; 8]; 8192]
                } else {
                    vec![]
                },
            });
        }
        for (edge_index, edge) in graph.edges.iter().enumerate() {
            let s = graph
                .nodes
                .iter()
                .position(|n| n.id == edge.source)
                .unwrap();
            let t = graph
                .nodes
                .iter()
                .position(|n| n.id == edge.target)
                .unwrap();
            let sd = descriptors
                .iter()
                .find(|d| d.kind == nodes[s].kind)
                .unwrap();
            let td = descriptors
                .iter()
                .find(|d| d.kind == nodes[t].kind)
                .unwrap();
            let source_port = sd
                .outputs
                .iter()
                .position(|p| p.id == edge.source_port)
                .unwrap();
            let parameter = td.parameters.iter().position(|p| p.id == edge.target_port);
            let destination = parameter.unwrap_or_else(|| {
                td.inputs
                    .iter()
                    .position(|p| p.id == edge.target_port)
                    .unwrap()
            });
            nodes[t].bindings.push(Binding {
                source: s,
                edge: edge_index,
                merge: None,
                midi_lane: None,
                source_port,
                destination,
                parameter: parameter.is_some(),
                signal: sd.outputs[source_port].signal,
            });
        }
        // Only complete, directly wired standard MIDI sources form independent
        // note lanes. Arbitrary control patches retain their scalar merge rules.
        for target in 0..nodes.len() {
            if !matches!(nodes[target].kind.as_str(), "synth" | "fm_synth") {
                continue;
            }
            let mut sources: Vec<_> = (0..nodes.len()).collect();
            sources.sort_by(|a, b| nodes[*a].id.cmp(&nodes[*b].id));
            for source in sources {
                if nodes[source].midi_controls.is_none() {
                    continue;
                }
                let complete = (0..5).all(|port| {
                    nodes[target].bindings.iter().any(|b| {
                        b.source == source
                            && !b.parameter
                            && b.source_port == port
                            && b.destination == port
                    })
                });
                if !complete {
                    continue;
                }
                let lane = nodes[target].midi_lanes.len();
                for b in &mut nodes[target].bindings {
                    if b.source == source
                        && !b.parameter
                        && b.source_port == b.destination
                        && b.destination < 5
                    {
                        b.midi_lane = Some(lane);
                    }
                }
                nodes[target].midi_lanes.push(MidiLane {
                    source,
                    decoder: note_inputs::NoteInputs::default(),
                    values: [0.; 5],
                });
            }
        }
        let positions: Vec<_> = graph
            .nodes
            .iter()
            .map(|n| {
                let (mut x, mut y) = (n.x, n.y);
                let mut parent = authored
                    .nodes
                    .iter()
                    .find(|a| a.id == n.id)
                    .and_then(|a| a.parent.as_deref());
                while let Some(id) = parent {
                    let p = authored.nodes.iter().find(|p| p.id == id).unwrap();
                    x += p.x;
                    y += p.y;
                    parent = p.parent.as_deref();
                }
                (x, y)
            })
            .collect();
        let mut ranked: Vec<_> = (0..nodes.len()).collect();
        ranked.sort_by(|a, b| {
            positions[*a]
                .1
                .total_cmp(&positions[*b].1)
                .then_with(|| positions[*a].0.total_cmp(&positions[*b].0))
                .then_with(|| graph.nodes[*a].id.cmp(&graph.nodes[*b].id))
        });
        let mut priority = vec![0; nodes.len()];
        for (rank, index) in ranked.into_iter().enumerate() {
            priority[index] = rank;
        }
        for node in &mut nodes {
            for binding in 0..node.bindings.len() {
                let b = &node.bindings[binding];
                if b.signal != pr0_core::Signal::Control
                    || b.merge.is_some()
                    || b.midi_lane.is_some()
                {
                    continue;
                }
                let members: Vec<_> = node
                    .bindings
                    .iter()
                    .enumerate()
                    .filter(|(_, other)| {
                        other.midi_lane.is_none()
                            && other.signal == b.signal
                            && other.destination == b.destination
                            && other.parameter == b.parameter
                    })
                    .map(|(i, _)| i)
                    .collect();
                if members.len() < 2 {
                    continue;
                }
                let winner = *members
                    .iter()
                    .min_by_key(|i| priority[node.bindings[**i].source])
                    .unwrap();
                let group = node.merges.len();
                for &i in &members {
                    node.bindings[i].merge = Some(group);
                }
                node.merges.push(ControlMerge {
                    previous: vec![visualizer::Datum::Number(0.); members.len()],
                    members,
                    value: visualizer::Datum::Number(0.),
                    event: false,
                    winner,
                });
            }
        }
        for &idx in &order {
            let input_latency = nodes[idx]
                .bindings
                .iter()
                .filter(|b| b.signal != pr0_core::Signal::Control)
                .map(|b| nodes[b.source].latency)
                .max()
                .unwrap_or(0);
            for j in 0..nodes[idx].bindings.len() {
                let binding = &nodes[idx].bindings[j];
                let delay = if binding.signal == pr0_core::Signal::Audio {
                    input_latency.saturating_sub(nodes[binding.source].latency)
                } else {
                    0
                };
                nodes[idx].compensations.push(vec![[0.; 8]; delay]);
                nodes[idx].compensation_cursors.push(0);
            }
            nodes[idx].latency = input_latency
                + if pr0_core::named_route(&nodes[idx].kind)
                    && nodes[idx].kind.starts_with("receive_")
                {
                    1
                } else if nodes[idx].kind == "convolution" {
                    nodes[idx].p("window") as usize
                } else if let Some(shifter) = &nodes[idx].pitch_shift {
                    shifter.latency()
                } else if matches!(nodes[idx].kind.as_str(), "fft" | "rfft") {
                    nodes[idx].p("size") as usize
                } else {
                    0
                };
        }
        Ok(Self {
            nodes,
            priority,
            order,
            clock: Clock::new(sample_rate),
            graph_clock: Clock {
                running: true,
                ..Clock::new(sample_rate)
            },
            graph,
        })
    }
    pub fn take_record_events(
        &mut self,
    ) -> Vec<(String, String, usize, u32, Vec<recorder::Event>)> {
        let mut chunks = Vec::new();
        for node in &mut self.nodes {
            if let Some(recorder) = &mut node.recorder {
                if recorder.pending() {
                    let name = self
                        .graph
                        .nodes
                        .iter()
                        .find(|n| n.id == node.id)
                        .unwrap()
                        .label
                        .clone();
                    chunks.push((
                        node.id.clone(),
                        name,
                        recorder.channels,
                        self.clock.sample_rate as u32,
                        recorder.drain(),
                    ));
                }
            }
        }
        chunks
    }
    pub fn finish_recordings(&mut self) {
        for node in &mut self.nodes {
            if let Some(recorder) = &mut node.recorder {
                recorder.finish();
            }
        }
    }
    /// Off-render persistence API. A zero-length snapshot deletes the previous recording.
    pub fn take_loop_snapshot(&mut self) -> Option<(String, u8, usize, u32, Vec<f32>)> {
        for node in &mut self.nodes {
            if let Some(looper) = &mut node.looper {
                if let Some((track, channels, audio)) = looper.snapshot() {
                    return Some((
                        node.id.clone(),
                        track,
                        channels,
                        self.clock.sample_rate as u32,
                        audio,
                    ));
                }
            }
        }
        None
    }
    pub fn finish_loops(&mut self) {
        for node in &mut self.nodes {
            if let Some(looper) = &mut node.looper {
                looper.finish();
            }
        }
    }
    pub fn clear_loop(&mut self, node: &str, track: u8) -> Result<(), String> {
        if !(1..=8).contains(&track) {
            return Err("Loop track must be 1–8".into());
        }
        let looper = self
            .nodes
            .iter_mut()
            .find(|n| n.id == node)
            .and_then(|n| n.looper.as_mut())
            .ok_or("Looper node missing")?;
        looper.clear(track as usize - 1);
        Ok(())
    }
    pub fn restore_loop(
        &mut self,
        node: &str,
        track: u8,
        audio: &[f32],
        channels: usize,
        rate: u32,
    ) -> Result<(), String> {
        if !(1..=8).contains(&track) {
            return Err("Loop track must be 1–8".into());
        }
        self.nodes
            .iter_mut()
            .find(|n| n.id == node)
            .and_then(|n| n.looper.as_mut())
            .ok_or("Looper node missing")?
            .restore(
                track as usize - 1,
                audio,
                channels,
                rate,
                self.clock.sample_rate,
            )
    }
    pub fn set_meter(&mut self, beats_per_bar: u8, beat_unit: u8) {
        self.graph_clock.beat_length = 4. / beat_unit.max(1) as f64;
        self.graph_clock.bar_beats = beats_per_bar.max(1) as f64 * self.graph_clock.beat_length;
    }
    /// Move state for unchanged nodes into a prepared graph. Binding indices
    /// belong to each compiled graph and must never move with runtime history.
    /// Runs between render blocks; swapping prepared storage does not allocate.
    pub fn carry_node_state(&mut self, previous: &mut Self) {
        if self.clock.sample_rate != previous.clock.sample_rate {
            return;
        }
        let meter = (self.graph_clock.bar_beats, self.graph_clock.beat_length);
        self.graph_clock = previous.graph_clock;
        (self.graph_clock.bar_beats, self.graph_clock.beat_length) = meter;
        for index in 0..self.nodes.len() {
            let target = &self.nodes[index];
            let Some(source_index) = previous.nodes.iter().position(|n| n.id == target.id) else {
                continue;
            };
            let source = &previous.nodes[source_index];
            let compatible = target.kind == source.kind
                && (target.route.is_none()
                    || (self.nodes.len() == previous.nodes.len()
                        && self
                            .nodes
                            .iter()
                            .zip(&previous.nodes)
                            .all(|(a, b)| a.id == b.id)))
                && target.part_id == source.part_id
                && target.io == source.io
                && target.channels == source.channels
                && target.recorder.as_ref().map(|r| r.channels)
                    == source.recorder.as_ref().map(|r| r.channels)
                && (target.defaults == source.defaults || target.kind == "piano")
                && target.fallback == source.fallback
                && target.latency == source.latency
                && target.bindings.len() == source.bindings.len()
                && target
                    .bindings
                    .iter()
                    .zip(&source.bindings)
                    .enumerate()
                    .all(|(i, (a, b))| {
                        self.nodes[a.source].id == previous.nodes[b.source].id
                            && self.nodes[a.source].kind == previous.nodes[b.source].kind
                            && a.source_port == b.source_port
                            && a.destination == b.destination
                            && a.parameter == b.parameter
                            && a.signal == b.signal
                            && target.compensations[i].len() == source.compensations[i].len()
                    });
            if !compatible {
                let target = &mut self.nodes[index];
                let source = &mut previous.nodes[source_index];
                if let (Some(a), Some(b)) = (&target.looper, &source.looper) {
                    if a.compatible(b) {
                        std::mem::swap(&mut target.looper, &mut source.looper);
                    }
                }
                if target.kind == source.kind && target.midi_controls.is_some() {
                    std::mem::swap(&mut target.midi_controls, &mut source.midi_controls);
                    if target.part_id == source.part_id && target.defaults == source.defaults {
                        std::mem::swap(&mut target.midi_pending, &mut source.midi_pending);
                    }
                    if target.part_id != source.part_id
                        || target.io != source.io
                        || target.defaults != source.defaults
                    {
                        target.midi_controls.as_mut().unwrap().release();
                    }
                }
                continue;
            }
            let target = &mut self.nodes[index];
            let source = &mut previous.nodes[source_index];
            std::mem::swap(target, source);
            std::mem::swap(&mut target.bindings, &mut source.bindings);
            for (old, new) in target.midi_lanes.iter_mut().zip(&mut source.midi_lanes) {
                std::mem::swap(&mut old.source, &mut new.source);
            }
            if target.kind == "piano" {
                // Octave is display configuration; changing it must not release held notes.
                std::mem::swap(&mut target.defaults, &mut source.defaults);
                std::mem::swap(&mut target.values, &mut source.values);
            }

            // Sequencer migration already selected voices by surviving part.
            // Do not resurrect voices from deleted or musically changed parts.
            if matches!(target.kind.as_str(), "synth" | "fm_synth") {
                std::mem::swap(&mut target.voices, &mut source.voices);
                // Graph MIDI is independent of score ownership and survives compatible edits.
                for voice in source
                    .voices
                    .iter()
                    .filter(|v| v.owner == GRAPH_VOICE_OWNER && v.level > 0.)
                {
                    if let Some(slot) = target.voices.iter_mut().find(|v| v.level == 0.) {
                        *slot = *voice;
                    }
                }
            }
        }
    }
    pub fn set_sample(&mut self, id: &str, sample: Vec<[f32; 8]>) {
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
            n.sample = sample;
            n.sample_position = 0;
        }
    }
    /// Preserve a part's synth phase and release envelopes in a prepared graph.
    /// Both voice arrays have fixed capacity; no allocation occurs here.
    pub fn carry_note_voices(&mut self, previous: &Self, node: &str, owner: u64) {
        let Some(source) = previous
            .nodes
            .iter()
            .find(|n| n.id == node && matches!(n.kind.as_str(), "synth" | "fm_synth"))
        else {
            return;
        };
        let Some(target) = self
            .nodes
            .iter_mut()
            .find(|n| n.id == node && matches!(n.kind.as_str(), "synth" | "fm_synth"))
        else {
            return;
        };
        if source.kind != target.kind {
            return;
        }
        target.voice_order = target.voice_order.max(source.voice_order);
        for voice in source
            .voices
            .iter()
            .filter(|v| v.owner == owner && v.level > 0.)
        {
            if let Some(slot) = target.voices.iter_mut().find(|v| v.level == 0.) {
                *slot = *voice;
            }
        }
    }
    pub fn part_note(&mut self, part: &str, pitch: u8, velocity: u8) {
        self.part_staff_note(part, 1, pitch, velocity)
    }
    pub fn part_staff_note(&mut self, part: &str, staff: u8, pitch: u8, velocity: u8) {
        for node in &mut self.nodes {
            if node.kind == "part_midi"
                && node.part_id.as_deref() == Some(part)
                && (node.p("staff") == 0. || node.p("staff") == f64::from(staff))
            {
                node.midi_controls.as_mut().unwrap().note(pitch, velocity);
            }
        }
    }
    pub fn part_notes_off(&mut self, part: &str) {
        for node in &mut self.nodes {
            if node.kind == "part_midi" && node.part_id.as_deref() == Some(part) {
                node.midi_controls.as_mut().unwrap().release();
            }
        }
    }
    pub fn part_source(&self, id: &str) -> Option<&str> {
        self.nodes
            .iter()
            .find(|n| n.id == id && n.kind == "part_midi")
            .and_then(|n| n.part_id.as_deref())
    }
    pub fn part_source_staff(&self, id: &str) -> Option<u8> {
        self.nodes
            .iter()
            .find(|n| n.id == id && n.kind == "part_midi")
            .map(|n| n.p("staff") as u8)
    }
    pub fn node_midi_message(&mut self, id: &str, message: pr0_core::midi::Message) {
        if message.valid() {
            if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
                node.midi_pending.push(message);
            }
        }
    }
    /// Used only during prepared graph installation, never in render.
    pub fn part_sources(&self) -> Vec<(String, String)> {
        self.nodes
            .iter()
            .filter(|n| n.kind == "part_midi")
            .filter_map(|n| n.part_id.as_ref().map(|p| (n.id.clone(), p.clone())))
            .collect()
    }
    pub fn piano_note(&mut self, id: &str, pitch: u8, velocity: u8) {
        if (self.graph_clock.running || velocity == 0)
            && self.nodes.iter().any(|n| n.id == id && n.kind == "piano")
        {
            self.node_midi_note(id, pitch, velocity);
        }
    }
    pub fn node_midi_note(&mut self, id: &str, pitch: u8, velocity: u8) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
            if let Some(midi) = &mut node.midi_controls {
                midi.note(pitch, velocity);
            }
        }
    }
    pub fn node_midi_cc(&mut self, id: &str, controller: u8, value: u8) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
            if let Some(midi) = &mut node.midi_controls {
                midi.cc(controller, value);
            }
        }
    }
    pub fn reset_midi_sources(&mut self) {
        for node in &mut self.nodes {
            if let Some(midi) = &mut node.midi_controls {
                midi.release();
            }
        }
    }
    pub fn node_midi_reset(&mut self, id: &str) {
        if let Some(node) = self.nodes.iter_mut().find(|n| n.id == id) {
            if let Some(midi) = &mut node.midi_controls {
                midi.release();
            }
        }
    }
    pub fn part_message(&mut self, part: &str, message: pr0_core::midi::Message) {
        self.part_staff_message(part, 0, message)
    }
    pub fn part_staff_message(&mut self, part: &str, staff: u8, message: pr0_core::midi::Message) {
        if !message.valid() {
            return;
        }
        for node in &mut self.nodes {
            if node.kind == "part_midi"
                && node.part_id.as_deref() == Some(part)
                && (staff == 0 || node.p("staff") == 0. || node.p("staff") == f64::from(staff))
            {
                node.midi_pending.push(message)
            }
        }
    }
    pub fn take_midi_message(&mut self, id: &str) -> Option<pr0_core::midi::Message> {
        self.nodes
            .iter_mut()
            .find(|n| n.id == id && n.kind == "midi_output")
            .and_then(|n| n.midi_frame.pop())
    }
    pub fn take_midi_output(&mut self, id: &str) -> [Option<note_inputs::NoteEvent>; 2] {
        self.nodes
            .iter_mut()
            .find(|n| n.id == id)
            .map(|n| std::mem::replace(&mut n.outgoing_notes, [None; 2]))
            .unwrap_or([None; 2])
    }
    pub fn note(&mut self, node: &str, pitch: u8, velocity: u8) {
        self.note_scoped(node, 0, pitch as u32, pitch, velocity);
    }
    /// A score note belongs to a part and note instance, even when pitches match.
    /// Voice storage is prepared with the graph; dispatch does not allocate.
    pub fn note_scoped(&mut self, node: &str, owner: u64, note_id: u32, pitch: u8, velocity: u8) {
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == node) {
            if matches!(n.kind.as_str(), "synth" | "fm_synth") {
                n.synth_note(owner, note_id, pitch, velocity);
            }
        }
    }
    pub fn node_notes_off(&mut self, id: &str) {
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == id) {
            for v in &mut n.voices {
                v.releasing = true;
            }
        }
    }
    pub fn all_notes_off(&mut self) {
        for n in &mut self.nodes {
            for v in &mut n.voices {
                v.releasing = true;
            }
        }
    }
    /// Source-side snapshot for an authorized audio connection preview.
    pub fn audio_frame(&self, source: &str, port: &str) -> [f32; MAX_CHANNELS] {
        let Some(node) = self.nodes.iter().find(|n| n.id == source) else {
            return [0.; MAX_CHANNELS];
        };
        if node.kind == "channel_split" {
            let channel = port
                .strip_prefix("ch_")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            let mut frame = [0.; MAX_CHANNELS];
            if (1..=node.channels).contains(&channel) {
                frame[0] = node.output[channel - 1] as f32;
            }
            frame
        } else {
            std::array::from_fn(|ch| node.output[ch] as f32)
        }
    }
    pub fn output_frame(&self, id: &str) -> [f32; MAX_CHANNELS] {
        self.nodes
            .iter()
            .find(|n| n.id == id && n.kind == "output")
            .map(|n| std::array::from_fn(|ch| n.output[ch] as f32))
            .unwrap_or([0.; MAX_CHANNELS])
    }
    /// Read a dedicated monitor after rendering one frame. Never sums into the
    /// hardware output. Wider bundles explicitly select their first two channels.
    pub fn monitor_frame(&self, id: &str) -> [f32; 2] {
        self.nodes
            .iter()
            .find(|n| n.id == id && n.kind == "monitor_output")
            .map(|n| {
                [
                    n.output[0] as f32,
                    n.output[if n.channels == 1 { 0 } else { 1 }] as f32,
                ]
            })
            .unwrap_or([0.; 2])
    }
    /// Apply native output routing after gain; destinations sum without normalization.
    pub fn device_output_frame(&self, id: &str) -> [f32; MAX_DEVICE_CHANNELS] {
        self.nodes
            .iter()
            .find(|n| n.id == id && n.kind == "output")
            .map(|n| n.device_frame())
            .unwrap_or([0.; MAX_DEVICE_CHANNELS])
    }

    pub fn external(&mut self, node: &str, sample: [f32; MAX_CHANNELS]) {
        let mut physical = [0.; MAX_DEVICE_CHANNELS];
        physical[..MAX_CHANNELS].copy_from_slice(&sample);
        self.device_input(node, physical);
    }
    pub fn device_input(&mut self, node: &str, sample: [f32; MAX_DEVICE_CHANNELS]) {
        if let Some(n) = self
            .nodes
            .iter_mut()
            .find(|n| n.id == node && matches!(n.kind.as_str(), "browser_input" | "input"))
        {
            n.external = sample;
            n.external_set = true;
        }
    }
    pub fn control(&mut self, id: &str, value: &pr0_core::ControlValue) {
        if let Some(node) = self.nodes.iter_mut().find(|n| {
            n.id == id
                && (n.kind == "toggle" || (n.kind == "control_input" && n.bindings.is_empty()))
        }) {
            if node.kind == "toggle" {
                let pr0_core::ControlValue::Number(value) = value else {
                    return;
                };
                if *value != 0. && *value != 1. {
                    return;
                }
                node.bang |= node.count != *value;
                node.count = *value;
            }
            node.fallback = visualizer::Datum::prepare(Some(value));
        }
    }
    pub fn tempo_connected(&self) -> bool {
        self.nodes
            .iter()
            .any(|n| n.kind == "clock" && n.bindings.iter().any(|b| !b.parameter))
    }
    /// Validate transient external control events without changing the project literal.
    pub fn external_control(&mut self, id: &str, value: &pr0_core::ControlValue) -> bool {
        let Some(n) = self
            .nodes
            .iter()
            .find(|n| n.id == id && n.kind == "control_input" && n.bindings.is_empty())
        else {
            return false;
        };
        let mode = n.p("mode");
        let valid = match value {
            pr0_core::ControlValue::Number(v) => {
                mode > 0.
                    && mode < 4.
                    && v.is_finite()
                    && *v >= n.p("min")
                    && *v <= n.p("max")
                    && (mode != 1. || v.fract() == 0.)
            }
            pr0_core::ControlValue::Text(s) => mode == 4. && s.len() <= 256,
        };
        if valid {
            self.control(id, value);
        }
        valid
    }
    pub fn bang(&mut self, id: &str) -> bool {
        if let Some(node) = self.nodes.iter_mut().find(|n| {
            n.id == id
                && (n.kind == "trigger"
                    || (n.kind == "control_input" && n.bindings.is_empty() && n.p("mode") == 0.))
        }) {
            node.bang = true;
            return true;
        }
        false
    }
    pub fn parameter(&mut self, node: &str, key: &str, value: f64) -> Result<(), String> {
        let n = self
            .nodes
            .iter_mut()
            .find(|n| n.id == node)
            .ok_or("Node missing")?;
        let i = n
            .names
            .iter()
            .position(|p| p == key)
            .ok_or("Parameter missing")?;
        if n.bindings.iter().any(|b| b.parameter && b.destination == i) {
            return Err("Parameter is driven by a connection".into());
        }
        let (min, max) = n.limits[i];
        if !value.is_finite() || value < min || value > max {
            return Err("Parameter out of range".into());
        }
        n.defaults[i] = value;
        n.values[i] = value;
        Ok(())
    }
    fn receive_named(&mut self, idx: usize) {
        let node = &mut self.nodes[idx];
        let route = node.route.as_mut().unwrap();
        let target_port = usize::from(route.send);
        let driven = node
            .bindings
            .iter()
            .any(|b| !b.parameter && b.destination == target_port);
        let name = if driven {
            node.target_text
                .unwrap_or_else(|| visualizer::Text::new(""))
        } else {
            match node.fallback {
                visualizer::Datum::Text(t) => t,
                _ => visualizer::Text::new(""),
            }
        };
        route.error = driven && node.target_text.is_none();
        if route.name != name {
            route.name = name;
            route.seen.fill(0);
            route.value = visualizer::Datum::Number(0.);
        }
        if route.send {
            return;
        }
        let signal = route.signal;
        route.connected = 0;
        route.audio = [0.; 8];
        let mut best = usize::MAX;
        let mut spectrum_source = None;
        let mut fallback: Option<(usize, usize, visualizer::Datum)> = None;
        let mut current_present = false;
        for source in 0..self.nodes.len() {
            let Some(send) = self.nodes[source].route.as_ref() else {
                continue;
            };
            if !send.send
                || !send.ready
                || send.signal != signal
                || name.as_str().is_empty()
                || send.published_name != name
            {
                continue;
            }
            let compatible = signal == pr0_core::Signal::Control
                || (self.nodes[source].channels == self.nodes[idx].channels
                    && (signal != pr0_core::Signal::Spectral
                        || (self.nodes[source].p("size") == self.nodes[idx].p("size")
                            && self.nodes[source].p("overlap") == self.nodes[idx].p("overlap"))));
            if !compatible {
                self.nodes[idx].route.as_mut().unwrap().error = true;
                continue;
            }
            let serial = send.serial;
            let value = send.published_value;
            let audio = send.published_audio;
            let generation = send.snapshot.as_ref().map(|s| s.generation).unwrap_or(0);
            let rank = self.priority[source];
            let receive = self.nodes[idx].route.as_mut().unwrap();
            receive.connected += 1;
            current_present |= receive.control_source == Some(source);
            if fallback.as_ref().is_none_or(|f| rank < f.1) {
                fallback = Some((source, rank, value));
            }
            match signal {
                pr0_core::Signal::Control => {
                    if receive.seen[source] != serial && rank < best {
                        receive.value = value;
                        receive.control_source = Some(source);
                        best = rank;
                    }
                    receive.seen[source] = serial;
                }
                pr0_core::Signal::Audio => {
                    for ch in 0..8 {
                        receive.audio[ch] += audio[ch];
                    }
                }
                pr0_core::Signal::Midi => {}
                pr0_core::Signal::Spectral => {
                    if rank < best {
                        spectrum_source = Some((source, generation));
                        best = rank;
                    }
                }
            }
        }
        self.nodes[idx].control_event = signal == pr0_core::Signal::Control && best != usize::MAX;
        let receive = self.nodes[idx].route.as_mut().unwrap();
        if receive.connected == 0 {
            receive.value = visualizer::Datum::Number(0.);
            receive.control_source = None;
        } else if signal == pr0_core::Signal::Control && best == usize::MAX && !current_present {
            if let Some((source, _, value)) = fallback {
                receive.value = value;
                receive.control_source = Some(source);
            }
        }
        if signal == pr0_core::Signal::Spectral && receive.spectral_source != spectrum_source {
            receive.spectral_source = spectrum_source;
            if let Some((source, _)) = spectrum_source {
                if source < idx {
                    let (a, b) = self.nodes.split_at_mut(idx);
                    b[0].spectral
                        .as_mut()
                        .unwrap()
                        .route_from(a[source].route.as_ref().unwrap().snapshot.as_ref().unwrap());
                } else {
                    let (a, b) = self.nodes.split_at_mut(source);
                    a[idx]
                        .spectral
                        .as_mut()
                        .unwrap()
                        .route_from(b[0].route.as_ref().unwrap().snapshot.as_ref().unwrap());
                }
            } else {
                self.nodes[idx].spectral.as_mut().unwrap().route_silence();
            }
        }
    }
    pub fn render(&mut self, input: &[[f32; MAX_CHANNELS]], output: &mut [[f32; MAX_CHANNELS]]) {
        for (frame_idx, out) in output.iter_mut().enumerate() {
            let hardware = input.get(frame_idx).copied().unwrap_or([0.; MAX_CHANNELS]);
            *out = [0.; MAX_CHANNELS];
            for order_index in 0..self.order.len() {
                let idx = self.order[order_index];
                self.nodes[idx].midi_frame.clear();
                while let Some(event) = self.nodes[idx].midi_pending.pop() {
                    self.nodes[idx].midi_frame.push(event);
                }
                self.nodes[idx].midi_pending.clear();
                self.nodes[idx].input = [[0.; MAX_CHANNELS]; 8];
                self.nodes[idx].input_text = None;
                self.nodes[idx].target_text = None;
                self.nodes[idx].input_events = [false; 8];
                self.nodes[idx].input_event_only = [false; 8];
                self.nodes[idx].control_event = false;
                for lane in 0..self.nodes[idx].midi_lanes.len() {
                    let source = self.nodes[idx].midi_lanes[lane].source;
                    let values = std::array::from_fn(|i| self.nodes[source].control[i]);
                    self.nodes[idx].midi_lanes[lane].values = values;
                }
                for group in 0..self.nodes[idx].merges.len() {
                    let mut best = usize::MAX;
                    self.nodes[idx].merges[group].event = false;
                    for member in 0..self.nodes[idx].merges[group].members.len() {
                        let index = self.nodes[idx].merges[group].members[member];
                        let b = self.nodes[idx].bindings[index].clone();
                        let source = &self.nodes[b.source];
                        let datum = if b.source_port == 0 {
                            source.control_text.map(visualizer::Datum::Text)
                        } else {
                            None
                        }
                        .unwrap_or(visualizer::Datum::Number(source.control[b.source_port]));
                        let event = b.source_port == 0 && source.control_event;
                        if b.source_port == 0 && source.control_event_only && !event {
                            continue;
                        }
                        let rank = self.priority[b.source];
                        let merge = &mut self.nodes[idx].merges[group];
                        if (datum != merge.previous[member] || event) && rank < best {
                            best = rank;
                            merge.value = datum;
                            merge.winner = index;
                            merge.event = event;
                        }
                        merge.previous[member] = datum;
                    }
                }
                for j in 0..self.nodes[idx].bindings.len() {
                    let b = self.nodes[idx].bindings[j].clone();
                    if b.signal == pr0_core::Signal::Midi {
                        for index in 0..self.nodes[b.source].midi_frame.len {
                            let event = self.nodes[b.source].midi_frame.events[index];
                            self.nodes[idx].midi_frame.push(event);
                        }
                        continue;
                    }

                    if b.midi_lane.is_some() {
                        continue;
                    }
                    if b.merge
                        .is_some_and(|g| self.nodes[idx].merges[g].winner != j)
                    {
                        continue;
                    }
                    let mut control = self.nodes[b.source]
                        .control
                        .get(b.source_port)
                        .copied()
                        .unwrap_or(0.);
                    let mut text = if b.source_port == 0 {
                        self.nodes[b.source].control_text
                    } else {
                        None
                    };
                    if let Some(group) = b.merge {
                        match self.nodes[idx].merges[group].value {
                            visualizer::Datum::Number(value) => {
                                control = value;
                                text = None;
                            }
                            visualizer::Datum::Text(value) => {
                                control = 0.;
                                text = Some(value);
                            }
                        }
                    }
                    if b.signal == pr0_core::Signal::Control {
                        let event_only =
                            b.source_port == 0 && self.nodes[b.source].control_event_only;
                        let event = b.merge.map_or(
                            b.source_port == 0 && self.nodes[b.source].control_event,
                            |group| self.nodes[idx].merges[group].event,
                        );
                        if !b.parameter {
                            self.nodes[idx].input_events[b.destination] = event;
                            self.nodes[idx].input_event_only[b.destination] = event_only;
                        }
                        if event_only && !event {
                            continue;
                        }
                    }
                    let mut audio = self.nodes[b.source].output;
                    if self.nodes[b.source].kind == "channel_split" {
                        let channel = audio[b.source_port];
                        audio = [0.; MAX_CHANNELS];
                        audio[0] = channel;
                    }
                    if !self.nodes[idx].compensations[j].is_empty() {
                        let cursor = self.nodes[idx].compensation_cursors[j];
                        let previous = self.nodes[idx].compensations[j][cursor];
                        self.nodes[idx].compensations[j][cursor] = audio;
                        audio = previous;
                        self.nodes[idx].compensation_cursors[j] =
                            (cursor + 1) % self.nodes[idx].compensations[j].len();
                    }
                    if b.signal == pr0_core::Signal::Spectral {
                        if b.source < idx {
                            let (left, right) = self.nodes.split_at_mut(idx);
                            if let (Some(source), Some(target)) =
                                (&left[b.source].spectral, &mut right[0].spectral)
                            {
                                target.copy_from(source);
                            }
                        } else {
                            let (left, right) = self.nodes.split_at_mut(b.source);
                            if let (Some(source), Some(target)) =
                                (&right[0].spectral, &mut left[idx].spectral)
                            {
                                target.copy_from(source);
                            }
                        }
                    } else if b.parameter {
                        let (min, max) = self.nodes[idx].limits[b.destination];
                        self.nodes[idx].values[b.destination] = control.clamp(min, max);
                    } else {
                        // All audio ports are conventionally in/a/b/sidechain; signal type is compiled here.
                        let is_control = b.signal == pr0_core::Signal::Control;
                        if is_control {
                            self.nodes[idx].input[b.destination][0] = control;
                            if b.destination == 0 {
                                self.nodes[idx].input_text = text;
                            }
                            if self.nodes[idx]
                                .route
                                .as_ref()
                                .is_some_and(|route| b.destination == usize::from(route.send))
                            {
                                self.nodes[idx].target_text = text;
                            }
                        } else {
                            self.nodes[idx].input[b.destination] = audio;
                        }
                    }
                }
                if self.nodes[idx].kind == "clock" && !self.nodes[idx].bindings.is_empty() {
                    let bpm = self.nodes[idx].input[0][0];
                    if bpm.is_finite() {
                        self.clock.set_tempo(bpm);
                    }
                }
                self.graph_clock.set_tempo(self.clock.bpm);
                if self.nodes[idx].route.is_some() {
                    self.receive_named(idx);
                    if self.nodes[idx].kind == "receive_control" {
                        self.nodes[idx].control_event_only = self.nodes[idx]
                            .route
                            .as_ref()
                            .and_then(|r| r.control_source)
                            .is_some_and(|source| {
                                self.nodes[source].route.as_ref().unwrap().event_only
                            });
                    }
                }
                self.nodes[idx].process(&self.graph_clock, &hardware);
                if self.nodes[idx].kind == "output" {
                    for (ch, sample) in out.iter_mut().enumerate() {
                        *sample += self.nodes[idx].output[ch] as f32;
                    }
                }
            }
            for node in &mut self.nodes {
                if let Some(route) = &mut node.route {
                    if !route.send {
                        continue;
                    }
                    route.event_only = node.control_event_only;
                    if node.control_event_only && !node.control_event {
                        route.ready = true;
                        route.published_name = route.name;
                        continue;
                    }
                    let value = node
                        .control_text
                        .map(visualizer::Datum::Text)
                        .unwrap_or(visualizer::Datum::Number(node.control[0]));
                    if node.control_event
                        || !route.ready
                        || route.published_name != route.name
                        || route.published_value != value
                    {
                        route.serial = route.serial.wrapping_add(1).max(1);
                    }
                    route.ready = true;
                    route.published_name = route.name;
                    route.published_value = value;
                    route.published_audio = node.output;
                    if let (Some(snapshot), Some(spectrum)) = (&mut route.snapshot, &node.spectral)
                    {
                        snapshot.copy_from(spectrum);
                    }
                }
            }
            for x in out.iter_mut() {
                *x = x.clamp(-1., 1.);
            }
            self.clock.advance();
            self.graph_clock.advance();
        }
    }
    /// Called by the non-realtime orchestration worker between blocks.
    /// Server calls at telemetry cadence only while visualizations are subscribed.
    pub fn analyze_visualizers(&mut self) {
        for node in &mut self.nodes {
            if let Some(analyzer) = &mut node.analyzer {
                if let Some(frames) = &node.spectral {
                    analyzer.spectral_block(&frames.bins, frames.polar);
                } else {
                    analyzer.analyze();
                }
            }
        }
    }
    pub fn visualizations(&self) -> std::collections::BTreeMap<String, pr0_core::Visualization> {
        use pr0_core::{ControlValue, Visualization};
        self.nodes
            .iter()
            .filter_map(|node| {
                let value = match node.kind.as_str() {
                    "control_visualizer" | "control_input" => Visualization::Control {
                        value: node
                            .control_text
                            .map(|t| ControlValue::Text(t.as_str().to_owned()))
                            .unwrap_or(ControlValue::Number(node.control[0])),
                    },
                    "audio_visualizer" => node.analyzer.as_ref().unwrap().snapshot(),
                    "spectral_visualizer" => {
                        let frames = node.spectral.as_ref().unwrap();
                        let analyzer = node.analyzer.as_ref().unwrap();
                        Visualization::Spectral {
                            sequence: analyzer.sequence,
                            generation: frames.generation,
                            size: frames.bins[0].len(),
                            ready: frames.generation > 0,
                            polar: frames.polar,
                            channels: visualizer::spectrum(&frames.bins, frames.polar, 1.),
                            history: analyzer.history(),
                            columns: analyzer.columns(),
                        }
                    }
                    _ => return None,
                };
                Some((node.id.clone(), value))
            })
            .collect()
    }
    pub fn route_targets(&self) -> BTreeMap<String, String> {
        self.nodes
            .iter()
            .filter_map(|n| {
                n.route
                    .as_ref()
                    .map(|r| (n.id.clone(), r.name.as_str().to_owned()))
            })
            .collect()
    }
    pub fn telemetry(&self) -> BTreeMap<String, BTreeMap<String, f64>> {
        self.nodes
            .iter()
            .map(|n| {
                let mut values: BTreeMap<String, f64> = n
                    .names
                    .iter()
                    .cloned()
                    .zip(n.values.iter().copied())
                    .collect();
                values.insert(
                    "_out".into(),
                    if n.control_event_only && !n.control_event {
                        0.
                    } else {
                        n.control[0]
                    },
                );
                if let Some(tracker) = &n.pitch_tracker {
                    for slot in 0..n.p("slots") as usize {
                        values.insert(format!("pitch{}", slot + 1), tracker.notes[slot]);
                        values.insert(format!("_strength{}", slot + 1), tracker.strengths[slot]);
                    }
                }
                if n.kind == "toggle" {
                    values.insert("_checked".into(), n.count);
                }
                for (index, b) in n
                    .bindings
                    .iter()
                    .enumerate()
                    .filter(|(_, b)| b.signal == pr0_core::Signal::Control)
                {
                    if b.midi_lane.is_some() {
                        continue;
                    }
                    if b.merge.is_some_and(|group| n.merges[group].winner != index) {
                        continue;
                    }
                    values.insert(
                        format!("_driver_{}", self.graph.edges[b.edge].target_port),
                        b.edge as f64,
                    );
                }

                if let Some(route) = &n.route {
                    values.insert("_route_connected".into(), route.connected as f64);
                    values.insert("_route_error".into(), route.error as u8 as f64);
                }
                if n.kind == "trigger" {
                    values.insert("_trigger_sequence".into(), n.count);
                }
                if let Some(recorder) = &n.recorder {
                    values.insert("_recording".into(), recorder.active as u8 as f64);
                    values.insert(
                        "_record_seconds".into(),
                        recorder.frames as f64 / self.clock.sample_rate,
                    );
                    values.insert("_record_overflow".into(), recorder.overflow as u8 as f64);
                    values.insert("_record_channels".into(), recorder.channels as f64);
                }
                if let Some(looper) = &n.looper {
                    for (i, t) in looper.tracks.iter().enumerate() {
                        for (key, value) in [
                            ("seconds", t.length as f64 / self.clock.sample_rate),
                            ("recording", u8::from(t.recording) as f64),
                            ("playing", u8::from(t.playing) as f64),
                            ("pending_record", u8::from(t.record_at.is_some()) as f64),
                            ("pending_play", u8::from(t.play_at.is_some()) as f64),
                            ("full", u8::from(t.full) as f64),
                        ] {
                            values.insert(format!("_track_{}_{}", i + 1, key), value);
                        }
                    }
                }
                if n.kind == "piano" {
                    let start = ((n.p("octave") + 1.) * 12.) as usize;
                    for pitch in start..(start + 12 * n.p("octaves") as usize).min(128) {
                        values.insert(
                            format!("_key{pitch}"),
                            if n.midi_controls.as_ref().unwrap().held(pitch) {
                                1.
                            } else {
                                0.
                            },
                        );
                    }
                }

                values.insert(
                    "_midi_event_dropped".into(),
                    (n.midi_pending.dropped + n.midi_frame.dropped) as f64,
                );
                if matches!(
                    n.kind.as_str(),
                    "part_midi" | "midi_input" | "osc_to_midi" | "piano"
                ) {
                    values.insert(
                        "_dropped".into(),
                        n.midi_controls.as_ref().unwrap().dropped as f64,
                    );
                    for (name, value) in ["pitch", "velocity", "gate", "trigger", "note_off"]
                        .into_iter()
                        .zip(n.control)
                    {
                        values.insert(name.into(), value);
                    }
                }
                if n.kind == "clock" {
                    values.insert("tempo".into(), self.clock.bpm);
                }
                values.insert("_latency".into(), n.latency as f64);
                values.insert(
                    "_peak".into(),
                    n.output.iter().fold(0_f64, |m, x| m.max(x.abs())),
                );
                if matches!(
                    n.kind.as_str(),
                    "gain" | "output" | "mixer" | "monitor_output"
                ) {
                    values.insert("gain".into(), 20. * n.smooth.max(1e-9).log10());
                }
                (n.id.clone(), values)
            })
            .collect()
    }
    pub fn carry_parameters(&self, graph: &mut Graph) {
        for node in &mut graph.nodes {
            if let Some(old) = self.nodes.iter().find(|n| n.id == node.id) {
                for (i, name) in old.names.iter().enumerate() {
                    let was_driven = old
                        .bindings
                        .iter()
                        .any(|b| b.parameter && b.destination == i);
                    let is_driven = graph
                        .edges
                        .iter()
                        .any(|e| e.target == node.id && e.target_port == *name);
                    if was_driven && !is_driven {
                        node.parameters.insert(name.clone(), old.values[i]);
                    }
                }
            }
        }
    }
}

/// Allocation-free FFT transform after construction. Complex bins are explicit.
pub struct Fourier {
    forward: std::sync::Arc<dyn rustfft::Fft<f32>>,
    inverse: std::sync::Arc<dyn rustfft::Fft<f32>>,
    scratch: Vec<rustfft::num_complex::Complex<f32>>,
}
impl Fourier {
    pub fn new(size: usize) -> Result<Self, String> {
        if !size.is_power_of_two() || !(256..=8192).contains(&size) {
            return Err("FFT size must be a power of two from 256 to 8192".into());
        }
        let mut planner = rustfft::FftPlanner::new();
        let forward = planner.plan_fft_forward(size);
        let inverse = planner.plan_fft_inverse(size);
        let scratch = vec![
            Default::default();
            forward
                .get_inplace_scratch_len()
                .max(inverse.get_inplace_scratch_len())
        ];
        Ok(Self {
            forward,
            inverse,
            scratch,
        })
    }
    pub fn forward(&mut self, data: &mut [rustfft::num_complex::Complex<f32>]) {
        self.forward.process_with_scratch(data, &mut self.scratch);
    }
    pub fn inverse(&mut self, data: &mut [rustfft::num_complex::Complex<f32>]) {
        self.inverse.process_with_scratch(data, &mut self.scratch);
        let scale = 1. / data.len() as f32;
        for x in data {
            *x *= scale;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pr0_core::{Mode, demo_project};
    #[test]
    fn piano_passes_chords_and_releases_and_preserves_held_notes_on_octave_change() {
        let mut graph = Graph {
            nodes: vec![
                visual_node("keys", "piano", 1),
                visual_node("receive", "piano", 1),
                visual_node("out", "midi_output", 1),
            ],
            edges: vec![],
        };
        for (source, target) in [("keys", "receive"), ("receive", "out")] {
            for port in ["pitch", "velocity", "gate", "trigger", "note_off"] {
                graph.edges.push(pr0_core::Edge {
                    id: format!("{source}-{port}"),
                    source: source.into(),
                    source_port: port.into(),
                    target: target.into(),
                    target_port: port.into(),
                });
            }
        }
        let mut engine = Engine::prepare(graph.clone(), 48000.).unwrap();
        engine.clock.running = true;
        for (pitch, velocity) in [(60, 100), (64, 80), (40, 70), (60, 0), (64, 0), (40, 0)] {
            engine.piano_note("keys", pitch, velocity);
        }
        let mut events = vec![];
        for _ in 0..16 {
            engine.render(&[], &mut [[0.; 8]]);
            for note in engine.take_midi_output("out").into_iter().flatten() {
                events.push((note.pitch, note.velocity));
            }
        }
        assert_eq!(
            events,
            vec![(60, 100), (64, 80), (40, 70), (60, 0), (64, 0), (40, 0)]
        );
        assert_eq!(engine.telemetry()["receive"]["gate"], 0.);
        engine.piano_note("keys", 60, 100);
        engine.render(&[], &mut [[0.; 8]; 2]);
        assert_eq!(engine.telemetry()["receive"]["_key60"], 1.);
        graph.nodes[1].parameters.insert("octave".into(), 3.);
        let mut next = Engine::prepare(graph.clone(), 48000.).unwrap();
        next.clock = engine.clock;
        next.carry_node_state(&mut engine);
        next.render(&[], &mut [[0.; 8]; 2]);
        assert!(!next.telemetry()["receive"].contains_key("_key60"));
        assert_eq!(next.telemetry()["receive"]["gate"], 1.);
        graph.nodes[1].parameters.insert("octaves".into(), 2.);
        let mut expanded = Engine::prepare(graph.clone(), 48000.).unwrap();
        expanded.clock = next.clock;
        expanded.carry_node_state(&mut next);
        next = expanded;
        assert_eq!(next.telemetry()["receive"]["_key60"], 1.);
        assert_eq!(
            next.telemetry()["receive"]
                .keys()
                .filter(|k| k.starts_with("_key"))
                .count(),
            24
        );
        next.piano_note("keys", 60, 0);
        next.render(&[], &mut [[0.; 8]; 4]);
        assert_eq!(next.telemetry()["receive"]["gate"], 0.);
        next.piano_note("keys", 65, 100);
        next.render(&[], &mut [[0.; 8]; 4]);
        next.reset_midi_sources();
        next.clock.running = false;
        next.render(&[], &mut [[0.; 8]; 16]);
        assert_eq!(next.telemetry()["receive"]["gate"], 0.);
        graph.nodes[1].parameters.insert("octave".into(), 9.);
        graph.nodes[1].parameters.insert("octaves".into(), 8.);
        let upper = Engine::prepare(graph, 48000.).unwrap();
        assert_eq!(
            upper.telemetry()["receive"]
                .keys()
                .filter(|k| k.starts_with("_key"))
                .count(),
            8
        );
        assert!(!upper.telemetry()["receive"].contains_key("_key128"));
    }
    #[test]
    fn part_events_fan_out_to_sampler_and_matching_midi_osc_ports() {
        let mut source = visual_node("notes", "part_midi", 2);
        source.part_id = Some("part-a".into());
        let mut sampler = visual_node("sampler", "poly_sampler", 2);
        sampler.parameters = [
            ("root_note".into(), 60.),
            ("loop".into(), 1.),
            ("release".into(), 1.),
        ]
        .into();
        let graph = Graph {
            nodes: vec![
                source,
                sampler,
                visual_node("midi", "midi_output", 2),
                visual_node("osc", "midi_to_osc", 2),
            ],
            edges: ["sampler", "midi", "osc"]
                .into_iter()
                .flat_map(|target| {
                    ["pitch", "velocity", "gate", "trigger", "note_off"]
                        .into_iter()
                        .map(move |port| pr0_core::Edge {
                            id: format!("{target}-{port}"),
                            source: "notes".into(),
                            source_port: port.into(),
                            target: target.into(),
                            target_port: port.into(),
                        })
                })
                .collect(),
        };
        let mut engine = Engine::prepare(graph, 48000.).unwrap();
        engine.clock.running = true;
        engine.set_sample("sampler", vec![[1.; 8]; 1024]);
        engine.part_note("part-b", 72, 127);
        engine.part_note("part-a", 60, 100);
        engine.part_note("part-a", 64, 80);
        engine.part_note("part-a", 60, 0);
        engine.part_note("part-a", 64, 0);
        let mut events = vec![];
        let mut audible = false;
        for _ in 0..60 {
            engine.render(&[], &mut [[0.; 8]]);
            let midi = engine.take_midi_output("midi");
            let osc = engine.take_midi_output("osc");
            assert_eq!(midi, osc);
            events.extend(midi.into_iter().flatten().map(|n| (n.pitch, n.velocity)));
            audible |= engine.audio_frame("sampler", "out")[0] > 0.;
        }
        assert_eq!(events, vec![(60, 100), (64, 80), (60, 0), (64, 0)]);
        assert!(audible);
        assert_eq!(engine.audio_frame("sampler", "out"), [0.; 8]);
    }
    #[test]
    fn graphical_control_bang_is_one_sample_and_input_overrides_manual_value() {
        let mut node = visual_node("gui", "control_input", 1);
        node.parameters.insert("mode".into(), 0.);
        let mut e = Engine::prepare(
            Graph {
                nodes: vec![node.clone()],
                edges: vec![],
            },
            48000.,
        )
        .unwrap();
        e.bang("gui");
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.telemetry()["gui"]["_out"], 1.);
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.telemetry()["gui"]["_out"], 0.);
        node.parameters.insert("mode".into(), 2.);
        node.control_value = Some(pr0_core::ControlValue::Number(3.));
        let mut e = Engine::prepare(
            Graph {
                nodes: vec![node.clone()],
                edges: vec![],
            },
            48000.,
        )
        .unwrap();
        e.control("gui", &pr0_core::ControlValue::Number(8.5));
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.telemetry()["gui"]["_out"], 8.5);
        let mut source = visual_node("text", "control_input", 1);
        source.parameters.insert("mode".into(), 4.);
        source.control_value = Some(pr0_core::ControlValue::Text("test".into()));
        let graph = Graph {
            nodes: vec![node, source],
            edges: vec![pr0_core::Edge {
                id: "wire".into(),
                source: "text".into(),
                source_port: "out".into(),
                target: "gui".into(),
                target_port: "in".into(),
            }],
        };
        let mut e = Engine::prepare(graph, 48000.).unwrap();
        e.control("gui", &pr0_core::ControlValue::Number(99.));
        e.render(&[], &mut [[0.; 8]; 1]);
        assert!(
            matches!(&e.visualizations()["gui"],pr0_core::Visualization::Control{value:pr0_core::ControlValue::Text(s)} if s=="test")
        );
    }

    #[test]
    fn subgraph_boundaries_preserve_audio_control_and_spectral_signals() {
        for signal in ["audio", "control", "spectral"] {
            for width in [1, 2, 8] {
                let mut source = visual_node(
                    "source",
                    if signal == "control" {
                        "value"
                    } else {
                        "input"
                    },
                    width,
                );
                if signal == "control" {
                    source.parameters.insert("value".into(), 37.);
                }
                let group = visual_node("group", "subgraph", width);
                let mut inlet = visual_node("inlet", &format!("subgraph_input_{signal}"), width);
                inlet.parent = Some("group".into());
                let mut outlet = inlet.clone();
                outlet.id = "outlet".into();
                outlet.kind = format!("subgraph_output_{signal}");
                let sink = visual_node(
                    "sink",
                    if signal == "control" {
                        "value"
                    } else {
                        "monitor_output"
                    },
                    width,
                );
                let mut graph = Graph {
                    nodes: vec![source, group, inlet, outlet, sink],
                    edges: vec![],
                };
                let wire =
                    |id: &str, source: &str, source_port: &str, target: &str, target_port: &str| {
                        pr0_core::Edge {
                            id: id.into(),
                            source: source.into(),
                            source_port: source_port.into(),
                            target: target.into(),
                            target_port: target_port.into(),
                        }
                    };
                graph.edges = vec![
                    wire("enter", "source", "out", "group", "inlet"),
                    wire("inside", "inlet", "out", "outlet", "in"),
                    wire(
                        "leave",
                        "group",
                        "outlet",
                        "sink",
                        if signal == "control" { "value" } else { "in" },
                    ),
                ];
                if signal == "spectral" {
                    let mut fft = visual_node("fft", "fft", width);
                    fft.parameters.insert("size".into(), 1024.);
                    let mut ifft = visual_node("ifft", "ifft", width);
                    ifft.parameters.insert("size".into(), 1024.);
                    graph.nodes.extend([fft, ifft]);
                    graph.edges[0].source = "fft".into();
                    graph.edges[2].target = "ifft".into();
                    graph.edges.extend([
                        wire("analysis", "source", "out", "fft", "in"),
                        wire("synthesis", "ifft", "out", "sink", "in"),
                    ]);
                }
                let mut engine = Engine::prepare(graph.clone(), 48000.).unwrap();
                engine.render(&vec![[0.2; 8]; 4096], &mut vec![[0.; 8]; 4096]);
                if signal == "control" {
                    assert_eq!(engine.telemetry()["sink"]["value"], 37.);
                } else if signal == "audio" {
                    assert!((engine.audio_frame("outlet", "out")[0] - 0.2).abs() < 1e-6);
                } else {
                    let frame = |id: &str| {
                        engine
                            .nodes
                            .iter()
                            .find(|n| n.id == id)
                            .unwrap()
                            .spectral
                            .as_ref()
                            .unwrap()
                    };
                    assert_eq!(frame("fft").bins, frame("outlet").bins);
                    assert_eq!(frame("fft").generation, frame("outlet").generation);
                }
                // Authored edges cannot bypass a boundary even if their signal types match.
                graph.edges[0].target = "inlet".into();
                graph.edges[0].target_port = "in".into();
                assert!(graph.validate().is_err());
            }
        }
    }

    #[test]
    fn subgraphs_allow_deep_nesting_but_reject_containment_cycles() {
        let mut graph = Graph::default();
        for index in 0..200 {
            let mut node = visual_node(&format!("group-{index}"), "subgraph", 2);
            if index > 0 {
                node.parent = Some(format!("group-{}", index - 1));
            }
            graph.nodes.push(node);
        }
        assert!(graph.validate().is_ok());
        graph.nodes[0].parent = Some("group-199".into());
        assert!(graph.validate().unwrap_err().contains("containment"));
        graph.nodes[0].parent = Some("absent".into());
        assert!(graph.validate().unwrap_err().contains("Missing parent"));
    }

    #[test]
    fn connected_tempo_changes_global_clock_without_resetting_phase() {
        let mut p = demo_project("x".into(), "x".into(), Mode::Freeform);
        let source = p.graph.nodes.iter_mut().find(|n| n.id == "tone").unwrap();
        source.kind = "add".into();
        source.parameters = [("a".into(), 60.), ("b".into(), 0.)].into();
        p.graph.edges = vec![pr0_core::Edge {
            id: "tempo".into(),
            source: "tone".into(),
            source_port: "out".into(),
            target: "clock".into(),
            target_port: "tempo".into(),
        }];
        let mut e = Engine::prepare(p.graph.clone(), 1000.).unwrap();
        e.clock.beat = 3.25;
        e.clock.running = true;
        e.render(&[], &mut [[0.; 8]; 100]);
        assert!((e.clock.beat - 3.35).abs() < 1e-10);
        assert_eq!(e.clock.bpm, 60.);
        e.parameter("tone", "a", 240.).unwrap();
        e.render(&[], &mut [[0.; 8]; 100]);
        assert!((e.clock.beat - 3.75).abs() < 1e-10);
        assert_eq!(e.telemetry()["clock"]["tempo"], 240.);
        e.parameter("tone", "a", -10.).unwrap();
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.clock.bpm, 1.);
        let mut other = p
            .graph
            .nodes
            .iter()
            .find(|n| n.id == "clock")
            .unwrap()
            .clone();
        other.id = "second-clock".into();
        p.graph.nodes.push(other);
        let mut edge = p.graph.edges[0].clone();
        edge.id = "second-tempo".into();
        edge.target = "second-clock".into();
        p.graph.edges.push(edge);
        assert!(p.graph.validate().unwrap_err().contains("Only one"));
    }

    #[test]
    fn physical_routes_select_duplicate_mute_and_mix_channels() {
        let p = demo_project("x".into(), "x".into(), Mode::Freeform);
        let mut input = p.graph.nodes[0].clone();
        input.id = "physical-in".into();
        input.kind = "input".into();
        input.channels = 8;
        input.parameters = [
            ("route_1".into(), 64.),
            ("route_2".into(), 9.),
            ("route_3".into(), 9.),
            ("route_4".into(), 0.),
        ]
        .into();
        let mut output = input.clone();
        output.id = "physical-out".into();
        output.kind = "output".into();
        output.parameters = [
            ("gain".into(), 0.),
            ("route_1".into(), 1.),
            ("route_2".into(), 1.),
            ("route_3".into(), 64.),
            ("route_4".into(), 0.),
        ]
        .into();
        let mut graph = pr0_core::Graph {
            nodes: vec![input, output],
            edges: vec![pr0_core::Edge {
                id: "wire".into(),
                source: "physical-in".into(),
                source_port: "out".into(),
                target: "physical-out".into(),
                target_port: "in".into(),
            }],
        };
        for invalid in [-2., 0.5, 65., f64::NAN] {
            graph.nodes[0].parameters.insert("route_1".into(), invalid);
            assert!(graph.validate().is_err());
        }
        graph.nodes[0].parameters.insert("route_1".into(), 64.);
        let mut e = Engine::prepare(graph.clone(), 48000.).unwrap();
        e.clock.running = true;
        let mut physical = [0.; MAX_DEVICE_CHANNELS];
        physical[63] = 0.25;
        physical[8] = 0.5;
        physical[3] = 1.;
        physical[4] = 0.125;
        e.device_input("physical-in", physical);
        // Allow the existing 5 ms output-gain smoothing to settle.
        e.render(&[], &mut vec![[0.; 8]; 4800]);
        assert_eq!(
            e.audio_frame("physical-in", "out"),
            [0.25, 0.5, 0.5, 0., 0.125, 0., 0., 0.]
        );
        let routed = e.device_output_frame("physical-out");
        assert_eq!(routed[0], 0.75);
        assert_eq!(routed[63], 0.5);
        assert_eq!(routed[4], 0.125);
        assert_eq!(routed.iter().filter(|v| **v != 0.).count(), 3);
        // An eight-channel full-scale bundle downmixes into exactly two channels.
        graph.nodes[0].parameters.clear();
        for (i, key) in DEVICE_ROUTE_KEYS.iter().enumerate() {
            graph.nodes[1]
                .parameters
                .insert((*key).into(), (i % 2 + 1) as f64);
        }
        graph.nodes[1]
            .parameters
            .insert("gain".into(), -20. * 4_f64.log10());
        let mut stereo = Engine::prepare(graph.clone(), 48000.).unwrap();
        stereo.clock.running = true;
        stereo.external("physical-in", [1.; 8]);
        stereo.render(&[], &mut vec![[0.; 8]; 4800]);
        let frame = stereo.device_output_frame("physical-out");
        assert!((frame[0] - 1.).abs() < 1e-6 && (frame[1] - 1.).abs() < 1e-6);
        assert!(frame[2..].iter().all(|v| *v == 0.));
        // Live replacement preserves the new mapping rather than copying old destinations.
        for key in DEVICE_ROUTE_KEYS {
            graph.nodes[1].parameters.insert(key.into(), 0.);
        }
        let mut muted = Engine::prepare(graph, 48000.).unwrap();
        muted.carry_node_state(&mut stereo);
        muted.external("physical-in", [1.; 8]);
        muted.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(
            muted.device_output_frame("physical-out"),
            [0.; MAX_DEVICE_CHANNELS]
        );
        // Missing physical input channels remain silent even when explicitly selected.
        e.external("physical-in", [1.; 8]);
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(&e.audio_frame("physical-in", "out")[..4], &[0.; 4]);
    }

    #[test]
    fn native_inputs_receive_separate_external_frames_and_channel_offsets() {
        let mut p = demo_project("x".into(), "x".into(), Mode::Freeform);
        p.graph.edges.clear();
        p.graph.nodes.retain(|n| n.id == "tone" || n.id == "gain");
        for (index, node) in p.graph.nodes.iter_mut().enumerate() {
            node.kind = "input".into();
            node.parameters = [
                ("interface".into(), (index + 1) as f64),
                ("offset".into(), index as f64),
            ]
            .into();
        }
        let mut e = Engine::prepare(p.graph, 48000.).unwrap();
        e.external("tone", [0.25; 8]);
        e.external("gain", [0., 0.5, 0.75, 0., 0., 0., 0., 0.]);
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.audio_frame("tone", "out")[0], 0.25);
        assert_eq!(e.audio_frame("gain", "out")[0], 0.5);
        assert_eq!(e.audio_frame("gain", "out")[1], 0.75);
        e.external("gain", [0.; 8]);
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.audio_frame("gain", "out")[0], 0.);
    }

    #[test]
    fn clock_ratio_graph_preserves_phase_across_show_stop_and_tempo_changes() {
        let p = demo_project("x".into(), "x".into(), Mode::Freeform);
        let mut node = p.graph.nodes[0].clone();
        node.id = "ratio".into();
        node.kind = "clock_ratio".into();
        node.parameters = [("multiply".into(), 4.), ("divide".into(), 1.)].into();
        let mut e = Engine::prepare(
            Graph {
                nodes: vec![node],
                edges: vec![],
            },
            48000.,
        )
        .unwrap();
        e.clock.running = true;
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.nodes[0].control[0], 1.);
        e.clock.stop();
        e.clock.running = true;
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.nodes[0].control[0], 0.);
        e.graph_clock.beat = 0.125;
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.nodes[0].control[..3], [0., 0.5, 0.]);
        e.clock.set_tempo(60.);
        e.graph_clock.beat = 0.25;
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.nodes[0].control[..3], [1., 0., 1.]);
    }
    #[test]
    fn dedicated_monitor_is_isolated_from_speakers_and_duplicates_mono() {
        for width in [1, 2, 8] {
            let p = demo_project("x".into(), "x".into(), Mode::Freeform);
            let mut input = p.graph.nodes[0].clone();
            input.id = "in".into();
            input.kind = "input".into();
            input.channels = width;
            input.parameters.clear();
            let mut monitor = input.clone();
            monitor.id = "cue".into();
            monitor.kind = "monitor_output".into();
            monitor.parameters.insert("gain".into(), 0.);
            let graph = Graph {
                nodes: vec![input, monitor],
                edges: vec![pr0_core::Edge {
                    id: "send".into(),
                    source: "in".into(),
                    source_port: "out".into(),
                    target: "cue".into(),
                    target_port: "in".into(),
                }],
            };
            let mut e = Engine::prepare(graph, 48000.).unwrap();
            let mut speakers = [[0.; 8]; 128];
            e.render(&[[0.25, 0.5, 0.75, 1., 1., 1., 1., 1.]; 128], &mut speakers);
            assert_eq!(speakers, [[0.; 8]; 128]);
            let cue = e.monitor_frame("cue");
            assert!(cue[0] > 0.);
            assert_eq!(cue[1], cue[0] * if width == 1 { 1. } else { 2. });
            assert_eq!(e.monitor_frame("missing"), [0.; 2]);
            assert_eq!(e.monitor_frame("in"), [0.; 2]);
        }
    }
    #[test]
    fn split_mono_processing_merge_round_trip_at_all_widths() {
        for width in 1..=8 {
            let p = demo_project("x".into(), "x".into(), Mode::Freeform);
            let node = |id: &str, kind: &str, channels| {
                let mut n = p.graph.nodes[0].clone();
                n.id = id.into();
                n.kind = kind.into();
                n.channels = channels;
                n.parameters.clear();
                n
            };
            let edge =
                |id: &str, source: &str, port: &str, target: &str, inlet: &str| pr0_core::Edge {
                    id: id.into(),
                    source: source.into(),
                    source_port: port.into(),
                    target: target.into(),
                    target_port: inlet.into(),
                };
            let mut graph = Graph {
                nodes: vec![
                    node("in", "input", width),
                    node("split", "channel_split", width),
                    node("mono", "channel_map", 1),
                    node("merge", "channel_merge", width),
                ],
                edges: vec![
                    edge("in", "in", "out", "split", "in"),
                    edge("mono-in", "split", "ch_1", "mono", "in"),
                    edge("mono-out", "mono", "out", "merge", "ch_1"),
                ],
            };
            for ch in 2..=8 {
                graph.edges.push(edge(
                    &format!("branch-{ch}"),
                    "split",
                    &format!("ch_{ch}"),
                    "merge",
                    &format!("ch_{ch}"),
                ));
            }
            let mut e = Engine::prepare(graph.clone(), 48000.).unwrap();
            let input = [std::array::from_fn(|i| (i + 1) as f32 / 10.)];
            e.render(&input, &mut [[0.; 8]; 1]);
            for i in 0..8 {
                assert_eq!(
                    e.nodes[3].output[i],
                    if i < width { input[0][i] as f64 } else { 0. }
                );
            }
            if width > 1 {
                graph.nodes[2].channels = 2;
                assert!(
                    graph.validate().is_err(),
                    "mono ports reject a stereo processor"
                );
            }
        }
    }
    #[test]
    fn channel_mapping_preserves_typed_graph_widths() {
        for width in 1..=8 {
            let p = demo_project("x".into(), "x".into(), Mode::Freeform);
            let node = |id: &str, kind: &str| {
                let mut n = p.graph.nodes[0].clone();
                n.id = id.into();
                n.kind = kind.into();
                n.channels = width;
                n.parameters.clear();
                n
            };
            let mut map = node("map", "channel_map");
            for i in 0..width {
                map.parameters
                    .insert(format!("output_{}", i + 1), (width - i) as f64);
            }
            let graph = Graph {
                nodes: vec![node("in", "input"), map, node("out", "output")],
                edges: vec![
                    pr0_core::Edge {
                        id: "a".into(),
                        source: "in".into(),
                        source_port: "out".into(),
                        target: "map".into(),
                        target_port: "in".into(),
                    },
                    pr0_core::Edge {
                        id: "b".into(),
                        source: "map".into(),
                        source_port: "out".into(),
                        target: "out".into(),
                        target_port: "in".into(),
                    },
                ],
            };
            let mut mapped = Engine::prepare(graph.clone(), 48000.).unwrap();
            let mut identity_graph = graph;
            identity_graph.nodes[1].parameters.clear();
            let mut identity = Engine::prepare(identity_graph, 48000.).unwrap();
            let input = [std::array::from_fn(|i| (i + 1) as f32 / 10.); 128];
            let reversed = [std::array::from_fn(|i| {
                if i < width {
                    (width - i) as f32 / 10.
                } else {
                    0.
                }
            }); 128];
            let mut actual = [[0.; 8]; 128];
            let mut expected = actual;
            mapped.render(&input, &mut actual);
            identity.render(&reversed, &mut expected);
            assert_eq!(actual, expected);
            assert!(
                actual
                    .iter()
                    .all(|frame| frame[width..].iter().all(|v| *v == 0.))
            );
        }
    }
    #[test]
    fn step_node_outputs_value_index_and_single_sample_pulse() {
        let p = demo_project("x".into(), "x".into(), Mode::Freeform);
        let mut gate = p.graph.nodes[0].clone();
        gate.id = "trigger".into();
        gate.kind = "value".into();
        gate.parameters = [("value".into(), 1.)].into();
        let mut step = gate.clone();
        step.id = "steps".into();
        step.kind = "step_sequencer".into();
        step.parameters = [
            ("length".into(), 2.),
            ("step_1".into(), 60.),
            ("step_2".into(), 67.),
        ]
        .into();
        let graph = Graph {
            nodes: vec![gate, step],
            edges: vec![pr0_core::Edge {
                id: "pulse".into(),
                source: "trigger".into(),
                source_port: "out".into(),
                target: "steps".into(),
                target_port: "trigger".into(),
            }],
        };
        let mut e = Engine::prepare(graph, 48000.).unwrap();
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.nodes[1].control[..3], [60., 0., 1.]);
        e.render(&[], &mut [[0.; 8]; 64]);
        assert_eq!(e.nodes[1].control[..3], [60., 0., 0.]);
        e.parameter("trigger", "value", 0.).unwrap();
        e.render(&[], &mut [[0.; 8]; 1]);
        e.parameter("trigger", "value", 1.).unwrap();
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.nodes[1].control[..3], [67., 1., 1.]);
        e.parameter("steps", "step_2", 69.).unwrap();
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.nodes[1].control[..3], [69., 1., 0.]);
    }
    #[test]
    fn adsr_graph_drives_connected_parameter_through_attack_and_release() {
        let p = demo_project("x".into(), "x".into(), Mode::Freeform);
        let template = &p.graph.nodes[0];
        let node = |id: &str, kind: &str, parameters: BTreeMap<String, f64>| {
            let mut n = template.clone();
            n.id = id.into();
            n.kind = kind.into();
            n.parameters = parameters;
            n
        };
        let graph = Graph {
            nodes: vec![
                node("gate", "value", [("value".into(), 1.)].into()),
                node(
                    "envelope",
                    "adsr",
                    [
                        ("attack".into(), 4.),
                        ("decay".into(), 2.),
                        ("sustain".into(), 0.5),
                        ("release".into(), 2.),
                    ]
                    .into(),
                ),
                node("readout", "value", BTreeMap::new()),
            ],
            edges: vec![
                pr0_core::Edge {
                    id: "gate-edge".into(),
                    source: "gate".into(),
                    source_port: "out".into(),
                    target: "envelope".into(),
                    target_port: "gate".into(),
                },
                pr0_core::Edge {
                    id: "env-edge".into(),
                    source: "envelope".into(),
                    source_port: "out".into(),
                    target: "readout".into(),
                    target_port: "value".into(),
                },
            ],
        };
        let mut engine = Engine::prepare(graph, 1000.).unwrap();
        engine.render(&[], &mut [[0.; 8]; 4]);
        assert_eq!(engine.telemetry()["readout"]["value"], 1.);
        engine.render(&[], &mut [[0.; 8]; 2]);
        assert_eq!(engine.telemetry()["readout"]["value"], 0.5);
        assert!(engine.parameter("envelope", "gate", 0.).is_err());
        engine.parameter("gate", "value", 0.).unwrap();
        engine.render(&[], &mut [[0.; 8]; 2]);
        assert_eq!(engine.telemetry()["readout"]["value"], 0.);
    }
    #[test]
    fn unchanged_delay_and_control_state_survive_reordered_graph() {
        let mut p = demo_project("x".into(), "x".into(), Mode::Freeform);
        let delay = p.graph.nodes.iter_mut().find(|n| n.id == "gain").unwrap();
        delay.kind = "delay".into();
        delay.parameters = [
            ("time".into(), 20.),
            ("feedback".into(), 0.7),
            ("mix".into(), 1.),
        ]
        .into();
        let mut old = Engine::prepare(p.graph.clone(), 48000.).unwrap();
        let mut reference = Engine::prepare(p.graph.clone(), 48000.).unwrap();
        for engine in [&mut old, &mut reference] {
            engine.clock.running = true;
            engine.note_scoped("tone", 1, 1, 60, 100);
            engine.render(&[], &mut [[0.; 8]; 1500]);
            engine.note_scoped("tone", 1, 1, 60, 0);
        }
        p.graph.nodes.reverse();
        let mut next = Engine::prepare(p.graph, 48000.).unwrap();
        next.clock = old.clock;
        next.carry_note_voices(&old, "tone", 1);
        next.carry_node_state(&mut old);
        let mut actual = [[0.; 8]; 2048];
        let mut expected = [[0.; 8]; 2048];
        next.render(&[], &mut actual);
        reference.render(&[], &mut expected);
        assert!(
            actual.iter().any(|f| f[0].abs() > 0.001),
            "delay tail is audible"
        );
        assert_eq!(
            actual, expected,
            "state migration must match uninterrupted rendering"
        );
        assert_eq!(next.telemetry(), reference.telemetry());
    }

    #[test]
    fn changed_effect_configuration_does_not_inherit_old_state() {
        let p = demo_project("x".into(), "x".into(), Mode::Freeform);
        let mut old = Engine::prepare(p.graph.clone(), 48000.).unwrap();
        old.clock.running = true;
        old.render(&[], &mut [[0.; 8]; 128]);
        let mut graph = p.graph;
        graph
            .nodes
            .iter_mut()
            .find(|n| n.id == "mod")
            .unwrap()
            .parameters
            .insert("b".into(), 7.);
        let mut next = Engine::prepare(graph, 48000.).unwrap();
        next.carry_node_state(&mut old);
        assert_eq!(
            next.nodes.iter().find(|n| n.id == "mod").unwrap().p("b"),
            7.
        );
    }
    #[test]
    fn prepared_graph_keeps_scoped_voice_phase_and_envelope() {
        let p = demo_project("x".into(), "x".into(), Mode::Freeform);
        let mut old = Engine::prepare(p.graph.clone(), 48000.).unwrap();
        old.note_scoped("tone", 7, 1, 60, 100);
        old.note_scoped("tone", 8, 2, 64, 90);
        old.render(&[], &mut [[0.; 8]; 256]);
        old.note_scoped("tone", 7, 1, 60, 0);
        let mut new = Engine::prepare(p.graph, 48000.).unwrap();
        new.carry_note_voices(&old, "tone", 7);
        let source = old
            .nodes
            .iter()
            .find(|n| n.id == "tone")
            .unwrap()
            .voices
            .iter()
            .find(|v| v.owner == 7)
            .unwrap();
        let voices = &new.nodes.iter().find(|n| n.id == "tone").unwrap().voices;
        let target = voices.iter().find(|v| v.owner == 7).unwrap();
        assert_eq!(target.phase, source.phase);
        assert_eq!(target.level, source.level);
        assert!(target.releasing);
        assert_eq!(voices.iter().filter(|v| v.level > 0.).count(), 1);
    }
    #[test]
    fn tempo_edit_preserves_phase() {
        let mut c = Clock::new(48000.);
        c.running = true;
        for _ in 0..24000 {
            c.advance();
        }
        assert!((c.beat - 1.).abs() < 1e-9);
        c.set_tempo(60.);
        assert!((c.beat - 1.).abs() < 1e-9);
        for _ in 0..48000 {
            c.advance();
        }
        assert!((c.beat - 2.).abs() < 1e-8);
    }
    #[test]
    fn scoped_notes_release_only_the_matching_instance() {
        let p = demo_project("x".into(), "x".into(), Mode::Freeform);
        let mut e = Engine::prepare(p.graph, 48000.).unwrap();
        e.note_scoped("tone", 1, 0, 60, 100);
        e.note_scoped("tone", 1, 1, 60, 100);
        e.note_scoped("tone", 2, 0, 60, 100);
        e.note_scoped("tone", 1, 0, 60, 0);
        let voices = &e.nodes.iter().find(|n| n.id == "tone").unwrap().voices;
        assert_eq!(
            voices
                .iter()
                .filter(|v| v.level > 0. && !v.releasing)
                .count(),
            2
        );
        assert!(
            voices
                .iter()
                .any(|v| v.owner == 1 && v.note_id == 1 && !v.releasing)
        );
        assert!(voices.iter().any(|v| v.owner == 2 && !v.releasing));
        // Release tails expire while the two other voices keep sounding.
        e.render(&[], &mut [[0.; 8]; 4096]);
        let voices = &e.nodes.iter().find(|n| n.id == "tone").unwrap().voices;
        assert_eq!(
            voices
                .iter()
                .filter(|v| v.level > 0.01 && !v.releasing)
                .count(),
            2
        );
    }
    fn visual_node(id: &str, kind: &str, channels: usize) -> pr0_core::Node {
        let mut node = demo_project("x".into(), "x".into(), Mode::Freeform)
            .graph
            .nodes
            .remove(0);
        node.id = id.into();
        node.kind = kind.into();
        node.channels = channels;
        node.parameters.clear();
        node.control_value = None;
        if [
            "fft",
            "ifft",
            "to_polar",
            "to_cartesian",
            "spectral_visualizer",
            "audio_visualizer",
        ]
        .contains(&kind)
        {
            node.parameters.insert("size".into(), 256.);
            if kind != "audio_visualizer" {
                node.parameters.insert("overlap".into(), 4.);
            }
        }
        node
    }
    fn visual_edge(id: &str, source: &str, target: &str) -> pr0_core::Edge {
        pr0_core::Edge {
            id: id.into(),
            source: source.into(),
            source_port: "out".into(),
            target: target.into(),
            target_port: "in".into(),
        }
    }
    #[test]
    fn audio_visualizer_is_transparent_at_every_width_and_records_each_block() {
        for channels in 1..=8 {
            let input = visual_node("in", "input", channels);
            let vis = visual_node("vis", "audio_visualizer", channels);
            let out = visual_node("out", "output", channels);
            let mut reference = Engine::prepare(
                Graph {
                    nodes: vec![input.clone(), out.clone()],
                    edges: vec![visual_edge("direct", "in", "out")],
                },
                48000.,
            )
            .unwrap();
            let mut e = Engine::prepare(
                Graph {
                    nodes: vec![input, vis, out],
                    edges: vec![
                        visual_edge("a", "in", "vis"),
                        visual_edge("b", "vis", "out"),
                    ],
                },
                48000.,
            )
            .unwrap();
            for block in 0..4 {
                let input: Vec<[f32; 8]> = (0..128)
                    .map(|i| {
                        std::array::from_fn(|ch| {
                            if ch < channels {
                                ((i + block * 128) as f32 * std::f32::consts::TAU / 32.).sin()
                                    * (ch + 1) as f32
                                    * 0.05
                            } else {
                                0.
                            }
                        })
                    })
                    .collect();
                let mut expected = vec![[0.; 8]; 128];
                let mut actual = expected.clone();
                reference.render(&input, &mut expected);
                e.render(&input, &mut actual);
                e.analyze_visualizers();
                assert_eq!(actual, expected);
            }
            let pr0_core::Visualization::Audio {
                sequence,
                channels: spectra,
                history,
                columns,
                ready,
                ..
            } = e.visualizations().remove("vis").unwrap()
            else {
                panic!()
            };
            assert_eq!(sequence, 4);
            assert_eq!(columns, 4);
            assert!(ready);
            assert_eq!(spectra.len(), channels);
            assert_eq!(history[0].len(), 4 * 32 * 2);
            assert!(spectra[0].magnitude[8] > 0.049);
        }
    }
    #[test]
    fn control_visualizer_preserves_numbers_and_utf8_strings() {
        for value in [
            pr0_core::ControlValue::Number(-123.75),
            pr0_core::ControlValue::Text("ready ♫ 東京".into()),
        ] {
            let mut source = visual_node("source", "control_visualizer", 1);
            source.control_value = Some(value.clone());
            let target = visual_node("target", "control_visualizer", 1);
            let mut e = Engine::prepare(
                Graph {
                    nodes: vec![source, target],
                    edges: vec![visual_edge("line", "source", "target")],
                },
                48000.,
            )
            .unwrap();
            e.render(&[], &mut [[0.; 8]; 8]);
            let pr0_core::Visualization::Control { value: observed } =
                e.visualizations().remove("target").unwrap()
            else {
                panic!()
            };
            assert_eq!(value, observed);
        }
    }
    #[test]
    fn spectral_visualizer_preserves_cartesian_and_polar_frames() {
        for polar in [false, true] {
            for width in [1, 8] {
                let mut nodes = vec![
                    visual_node("in", "input", width),
                    visual_node("fft", "fft", width),
                ];
                let mut edges = vec![visual_edge("audio", "in", "fft")];
                let source = if polar {
                    nodes.push(visual_node("polar", "to_polar", width));
                    edges.push(visual_edge("polar-line", "fft", "polar"));
                    "polar"
                } else {
                    "fft"
                };
                nodes.push(visual_node("vis", "spectral_visualizer", width));
                edges.push(visual_edge("spectral-line", source, "vis"));
                let mut e = Engine::prepare(Graph { nodes, edges }, 48000.).unwrap();
                let input: Vec<_> = (0..512).map(|i| [(i as f32 * 0.1).sin(); 8]).collect();
                e.render(&input, &mut vec![[0.; 8]; 512]);
                e.analyze_visualizers();
                let upstream = e
                    .nodes
                    .iter()
                    .find(|n| n.id == source)
                    .unwrap()
                    .spectral
                    .as_ref()
                    .unwrap();
                let passed = e
                    .nodes
                    .iter()
                    .find(|n| n.id == "vis")
                    .unwrap()
                    .spectral
                    .as_ref()
                    .unwrap();
                assert_eq!(upstream.bins, passed.bins);
                assert_eq!(upstream.generation, passed.generation);
                assert_eq!(passed.polar, polar);
                assert_eq!(
                    e.nodes.iter().find(|n| n.id == source).unwrap().latency,
                    e.nodes.iter().find(|n| n.id == "vis").unwrap().latency
                );
                let pr0_core::Visualization::Spectral {
                    channels, columns, ..
                } = e.visualizations().remove("vis").unwrap()
                else {
                    panic!()
                };
                assert_eq!(channels.len(), width);
                assert_eq!(channels[0].phase.len(), 129);
                assert_eq!(columns, 1);
            }
        }
    }
    #[test]
    fn oscillator_graph_is_sine_and_independent_of_render_block_size() {
        let mut p = demo_project("x".into(), "x".into(), Mode::Freeform);
        let mut node = p.graph.nodes.remove(0);
        node.id = "osc".into();
        node.kind = "oscillator".into();
        node.parameters = [("frequency".into(), 1000.), ("amplitude".into(), 0.25)].into();
        let graph = Graph {
            nodes: vec![node],
            edges: vec![],
        };
        let mut reference = Engine::prepare(graph.clone(), 48000.).unwrap();
        reference.render(&[], &mut vec![[0.; 8]; 9600]);
        let mut samples = vec![];
        for _ in 0..4800 {
            reference.render(&[], &mut [[0.; 8]; 1]);
            samples.push(reference.audio_frame("osc", "out")[0] as f64);
        }
        let amplitude = |h: f64| {
            let sin = samples
                .iter()
                .enumerate()
                .map(|(i, v)| v * (TAU * i as f64 * h / 48.).sin())
                .sum::<f64>();
            let cos = samples
                .iter()
                .enumerate()
                .map(|(i, v)| v * (TAU * i as f64 * h / 48.).cos())
                .sum::<f64>();
            sin.hypot(cos) / 2400.
        };
        assert!((amplitude(1.) - 0.25).abs() < 1e-5);
        for harmonic in 2..10 {
            assert!(amplitude(harmonic as f64) < 1e-5);
        }
        for block in [32, 64, 128, 256, 512, 1024] {
            let mut e = Engine::prepare(graph.clone(), 48000.).unwrap();
            let mut left = 14400;
            while left > 0 {
                let count = left.min(block);
                e.render(&[], &mut vec![[0.; 8]; count]);
                left -= count;
            }
            assert_eq!(
                e.audio_frame("osc", "out"),
                reference.audio_frame("osc", "out")
            );
        }
    }
    #[test]
    fn audio_to_control_drives_unsmoothed_sample_rate_fm() {
        for width in 1..=8 {
            let template = demo_project("x".into(), "x".into(), Mode::Freeform)
                .graph
                .nodes[0]
                .clone();
            let make = |id: &str, kind: &str, parameters: BTreeMap<String, f64>| {
                let mut node = template.clone();
                node.id = id.into();
                node.kind = kind.into();
                node.channels = width;
                node.parameters = parameters;
                node
            };
            let graph = Graph {
                nodes: vec![
                    make("mod", "input", BTreeMap::new()),
                    make(
                        "convert",
                        "audio_to_control",
                        [("scale".into(), 800.), ("offset".into(), 1000.)].into(),
                    ),
                    make(
                        "carrier",
                        "oscillator",
                        [("frequency".into(), 440.), ("amplitude".into(), 0.2)].into(),
                    ),
                ],
                edges: vec![
                    pr0_core::Edge {
                        id: "signal".into(),
                        source: "mod".into(),
                        source_port: "out".into(),
                        target: "convert".into(),
                        target_port: "in".into(),
                    },
                    pr0_core::Edge {
                        id: "fm".into(),
                        source: "convert".into(),
                        source_port: "out".into(),
                        target: "carrier".into(),
                        target_port: "frequency".into(),
                    },
                ],
            };
            let mut engine = Engine::prepare(graph, 48000.).unwrap();
            let mut phase = 0.;
            for sample in 0..4800 {
                let modulation = (TAU * sample as f64 / 12.).sin() as f32;
                let mut input = [0.9; MAX_CHANNELS];
                input[0] = modulation;
                engine.render(&[input], &mut [[0.; MAX_CHANNELS]; 1]);
                let frequency = modulation as f64 * 800. + 1000.;
                phase = (phase + frequency / 48000.).fract();
                assert_eq!(engine.nodes[1].control[0], frequency);
                assert_eq!(engine.nodes[2].smooth, frequency);
                for value in &engine.audio_frame("carrier", "out")[..width] {
                    assert!((*value as f64 - (TAU * phase).sin() * 0.2).abs() < 1e-7);
                }
            }
        }
    }

    #[test]
    fn sine_stays_clean_through_gain_and_output() {
        for gain_db in [-12., 0., 6.] {
            let mut p = demo_project("x".into(), "x".into(), Mode::Freeform);
            p.graph
                .nodes
                .retain(|n| ["tone", "gain", "out"].contains(&n.id.as_str()));
            p.graph
                .edges
                .retain(|e| ["tone", "gain"].contains(&e.source.as_str()));
            for node in &mut p.graph.nodes {
                if node.id == "tone" {
                    node.kind = "oscillator".into();
                    node.parameters = [
                        ("frequency".into(), 873.),
                        ("amplitude".into(), 0.1759),
                        ("waveform".into(), 0.),
                    ]
                    .into();
                } else {
                    node.parameters.insert(
                        "gain".into(),
                        if node.id == "gain" { gain_db } else { -13.7 },
                    );
                }
            }
            let mut engine = Engine::prepare(p.graph, 48000.).unwrap();
            engine.render(&[], &mut vec![[0.; MAX_CHANNELS]; 9600]);
            let factor = 10_f64.powf((gain_db - 13.7) / 20.);
            for _ in 0..4800 {
                let mut rendered = [[0.; MAX_CHANNELS]; 1];
                engine.render(&[], &mut rendered);
                let source = engine.audio_frame("tone", "out");
                let output = engine.output_frame("out");
                for ch in 0..2 {
                    assert!((output[ch] as f64 - source[ch] as f64 * factor).abs() < 1e-7);
                    assert_eq!(rendered[0][ch], output[ch]);
                }
            }
        }
    }
    #[test]
    fn oscillator_produces_finite_audio() {
        let p = demo_project("x".into(), "x".into(), Mode::Freeform);
        let mut e = Engine::prepare(p.graph, 48000.).unwrap();
        e.note("tone", 60, 100);
        let mut out = [[0.; 8]; 128];
        e.render(&[], &mut out);
        assert!(out.iter().any(|f| f[0].abs() > 0.001));
        assert!(out.iter().flatten().all(|x| x.is_finite()));
        assert!(out.iter().all(|f| f[2] == 0.));
    }
    #[test]
    fn connected_parameter_is_read_only() {
        let p = demo_project("x".into(), "x".into(), Mode::Freeform);
        let mut e = Engine::prepare(p.graph, 48000.).unwrap();
        assert!(e.parameter("mod", "a", 3.).is_err());
        assert!(e.parameter("mod", "b", 3.).is_ok());
    }
    #[test]
    fn fft_round_trip() {
        for size in [256, 1024, 8192] {
            let mut fft = Fourier::new(size).unwrap();
            let mut data: Vec<_> = (0..size)
                .map(|i| rustfft::num_complex::Complex::new((i as f32 * 0.1).sin(), 0.))
                .collect();
            let original = data.clone();
            fft.forward(&mut data);
            fft.inverse(&mut data);
            assert!(
                data.iter()
                    .zip(original)
                    .all(|(a, b)| (*a - b).norm() < 1e-5)
            );
        }
    }
    #[test]
    fn disconnect_retains_driven_value() {
        let p = demo_project("x".into(), "x".into(), Mode::Freeform);
        let mut e = Engine::prepare(p.graph.clone(), 48000.).unwrap();
        e.clock.running = true;
        e.render(&[], &mut [[0.; 8]; 128]);
        let mut g = p.graph;
        g.edges.retain(|x| x.target != "mod");
        e.carry_parameters(&mut g);
        assert_eq!(
            g.nodes.iter().find(|n| n.id == "mod").unwrap().parameters["a"],
            1.
        );
    }
}

#[cfg(test)]
mod poly_synth_tests {
    use super::*;
    fn node(id: &str, kind: &str, channels: usize) -> pr0_core::Node {
        let mut n = pr0_core::demo_project("test".into(), "test".into(), pr0_core::Mode::Freeform)
            .graph
            .nodes
            .remove(0);
        n.id = id.into();
        n.kind = kind.into();
        n.channels = channels;
        n.parameters.clear();
        n
    }
    fn graph(kind: &str, channels: usize) -> pr0_core::Graph {
        pr0_core::Graph {
            nodes: vec![node("keys", "piano", 1), node("tone", kind, channels)],
            edges: ["pitch", "velocity", "gate", "trigger", "note_off"]
                .into_iter()
                .map(|p| pr0_core::Edge {
                    id: p.into(),
                    source: "keys".into(),
                    source_port: p.into(),
                    target: "tone".into(),
                    target_port: p.into(),
                })
                .collect(),
        }
    }
    fn tick(e: &mut Engine) {
        e.render(&[], &mut [[0.; 8]; 16]);
    }
    #[test]
    fn standard_inputs_preserve_polyphony_releases_and_score_ownership() {
        for kind in ["synth", "fm_synth"] {
            let mut e = Engine::prepare(graph(kind, 1), 48000.).unwrap();
            e.note_scoped("tone", 7, 1, 60, 127);
            for (pitch, velocity) in [(60, 100), (60, 80), (64, 90)] {
                e.piano_note("keys", pitch, velocity);
                tick(&mut e);
            }
            let held = |e: &Engine, pitch| {
                e.nodes
                    .iter()
                    .find(|n| n.id == "tone")
                    .unwrap()
                    .voices
                    .iter()
                    .filter(|v| {
                        v.owner == GRAPH_VOICE_OWNER
                            && v.pitch == pitch
                            && v.level > 0.
                            && !v.releasing
                    })
                    .count()
            };
            assert_eq!(held(&e, 60), 2);
            assert_eq!(held(&e, 64), 1);
            e.piano_note("keys", 60, 0);
            tick(&mut e);
            assert_eq!(held(&e, 60), 1);
            assert_eq!(held(&e, 64), 1);
            assert!(
                e.nodes
                    .iter()
                    .find(|n| n.id == "tone")
                    .unwrap()
                    .voices
                    .iter()
                    .any(|v| v.owner == 7 && !v.releasing)
            );
            e.note_scoped("tone", 7, 1, 60, 0);
            tick(&mut e);
            assert_eq!(held(&e, 60), 1);
            e.piano_note("keys", 60, 0);
            e.piano_note("keys", 64, 0);
            tick(&mut e);
            e.render(&[], &mut [[0.; 8]; 48000]);
            assert_eq!(e.audio_frame("tone", "out"), [0.; 8]);
        }
    }
    #[test]
    fn independent_midi_sources_release_their_own_fm_and_sine_voices() {
        for kind in ["synth", "fm_synth"] {
            for keyboard in ["piano", "midi_input"] {
                let mut g = graph(kind, 1);
                g.nodes[0].kind = keyboard.into();
                let mut part = node("part", "part_midi", 1);
                part.part_id = Some("score".into());
                part.y = -100.; // The score wins ordinary simultaneous scalar merges.
                g.nodes.push(part);
                for port in ["pitch", "velocity", "gate", "trigger", "note_off"] {
                    g.edges.push(pr0_core::Edge {
                        id: format!("part-{port}"),
                        source: "part".into(),
                        source_port: port.into(),
                        target: "tone".into(),
                        target_port: port.into(),
                    });
                }
                let mut e = Engine::prepare(g.clone(), 48000.).unwrap();
                let held = |e: &Engine, pitch: u8, velocity: u8| {
                    e.nodes
                        .iter()
                        .find(|n| n.id == "tone")
                        .unwrap()
                        .voices
                        .iter()
                        .filter(|v| {
                            v.pitch == pitch
                                && !v.releasing
                                && (v.level - velocity as f64 / 127.).abs() < 1e-9
                        })
                        .count()
                };
                // Releasing a pitch unchanged at the keyboard must not use the
                // score's more recently changed pitch (the reported stuck note).
                e.part_note("score", 60, 110);
                tick(&mut e);
                e.node_midi_note("keys", 64, 90);
                tick(&mut e);
                e.part_note("score", 67, 110);
                tick(&mut e);
                e.node_midi_note("keys", 64, 0);
                tick(&mut e);
                assert_eq!(held(&e, 64, 90), 0);
                assert_eq!(held(&e, 67, 110), 1);
                e.part_notes_off("score");
                e.render(&[], &mut vec![[0.; 8]; 48000]);
                // Different source pitches, same-pitch repeated keyboard attacks,
                // simultaneous attacks, and simultaneous release/attack all survive.
                e.part_note("score", 60, 110);
                e.node_midi_note("keys", 64, 90);
                tick(&mut e);
                assert_eq!(held(&e, 60, 110), 1);
                assert_eq!(held(&e, 64, 90), 1);
                e.node_midi_note("keys", 64, 0);
                e.part_note("score", 67, 110);
                tick(&mut e);
                assert_eq!(held(&e, 64, 90), 0);
                assert_eq!(held(&e, 67, 110), 1);
                e.node_midi_note("keys", 60, 90);
                tick(&mut e);
                e.node_midi_note("keys", 60, 80);
                tick(&mut e);
                // Compatible graph edits remap source indices without losing
                // decoder history or changing a held voice's source ownership.
                g.nodes.reverse();
                let mut next = Engine::prepare(g, 48000.).unwrap();
                next.carry_node_state(&mut e);
                e = next;
                e.node_midi_note("keys", 60, 0);
                tick(&mut e);
                assert_eq!(held(&e, 60, 90), 0);
                assert_eq!(held(&e, 60, 80), 1);
                assert_eq!(held(&e, 60, 110), 1);
                e.part_notes_off("score");
                tick(&mut e);
                assert_eq!(held(&e, 60, 110), 0);
                assert_eq!(held(&e, 67, 110), 0);
                assert_eq!(held(&e, 60, 80), 1);
                e.node_midi_reset("keys");
                tick(&mut e);
                assert_eq!(held(&e, 60, 80), 0);
                e.render(&[], &mut vec![[0.; 8]; 48000]);
                assert_eq!(e.audio_frame("tone", "out"), [0.; 8]);
            }
        }
    }
    #[test]
    fn fm_depth_zero_matches_sine_and_frequency_inputs_follow_every_sample() {
        let mut plain = Engine::prepare(graph("synth", 1), 48000.).unwrap();
        let mut fm = Engine::prepare(graph("fm_synth", 1), 48000.).unwrap();
        fm.parameter("tone", "fm_depth", 0.).unwrap();
        for e in [&mut plain, &mut fm] {
            e.note("tone", 69, 127);
        }
        for _ in 0..256 {
            tick(&mut plain);
            tick(&mut fm);
            assert!(
                (plain.audio_frame("tone", "out")[0] - fm.audio_frame("tone", "out")[0]).abs()
                    < 1e-6
            );
        }
        let mut g = graph("fm_synth", 1);
        for (id, key, value) in [
            ("carrier", "carrier_frequency", 880.),
            ("modulator", "modulator_frequency", 660.),
        ] {
            let mut n = node(id, "value", 1);
            n.parameters.insert("value".into(), value);
            g.nodes.push(n);
            g.edges.push(pr0_core::Edge {
                id: id.into(),
                source: id.into(),
                source_port: "out".into(),
                target: "tone".into(),
                target_port: key.into(),
            });
        }
        let mut e = Engine::prepare(g, 48000.).unwrap();
        e.parameter("tone", "fm_depth", 0.).unwrap();
        e.note("tone", 69, 127);
        e.render(&[], &mut [[0.; 8]; 12]);
        let tone = e.nodes.iter().find(|n| n.id == "tone").unwrap();
        assert!((tone.voices[0].phase - 880. * 12. / 48000.).abs() < 1e-10);
        assert!((tone.voices[0].mod_phase - 660. * 12. / 48000.).abs() < 1e-10);
        let phase = tone.voices[0].phase;
        assert!(e.parameter("tone", "carrier_frequency", 440.).is_err());
        e.parameter("carrier", "value", 440.).unwrap();
        e.render(&[], &mut [[0.; 8]; 1]);
        assert!(
            (e.nodes.iter().find(|n| n.id == "tone").unwrap().voices[0].phase
                - phase
                - 440. / 48000.)
                .abs()
                < 1e-10
        );
    }
    #[test]
    fn fm_waveforms_modulate_and_multichannel_output_is_bounded() {
        let mut reference = Engine::prepare(graph("fm_synth", 8), 48000.).unwrap();
        reference.parameter("tone", "fm_depth", 0.).unwrap();
        reference.note("tone", 60, 127);
        let mut modulated = Engine::prepare(graph("fm_synth", 8), 48000.).unwrap();
        modulated.note("tone", 60, 127);
        let mut changed = false;
        for _ in 0..300 {
            tick(&mut reference);
            tick(&mut modulated);
            changed |= (reference.audio_frame("tone", "out")[0]
                - modulated.audio_frame("tone", "out")[0])
                .abs()
                > 0.01;
        }
        assert!(changed);
        for shape in 0..5 {
            let mut e = Engine::prepare(graph("fm_synth", 8), 48000.).unwrap();
            e.parameter("tone", "carrier_waveform", shape as f64)
                .unwrap();
            e.parameter("tone", "modulator_waveform", shape as f64)
                .unwrap();
            e.note("tone", 60, 127);
            e.note("tone", 64, 80);
            e.note("tone", 67, 90);
            for _ in 0..300 {
                tick(&mut e);
                let frame = e.audio_frame("tone", "out");
                assert!(frame.iter().all(|v| v.is_finite() && v.abs() <= 0.61));
                assert!(frame.iter().all(|v| *v == frame[0]));
            }
        }
    }
    #[test]
    fn graph_voices_survive_compatible_replacement_without_resurrecting_score_notes() {
        let g = graph("fm_synth", 1);
        let mut old = Engine::prepare(g.clone(), 48000.).unwrap();
        old.piano_note("keys", 69, 100);
        old.note_scoped("tone", 3, 1, 72, 100);
        tick(&mut old);
        let phase = old
            .nodes
            .iter()
            .find(|n| n.id == "tone")
            .unwrap()
            .voices
            .iter()
            .find(|v| v.owner == GRAPH_VOICE_OWNER)
            .unwrap()
            .phase;
        let mut new = Engine::prepare(g, 48000.).unwrap();
        new.carry_node_state(&mut old);
        let voices = &new.nodes.iter().find(|n| n.id == "tone").unwrap().voices;
        assert_eq!(voices.iter().filter(|v| v.level > 0.).count(), 1);
        assert_eq!(voices.iter().find(|v| v.level > 0.).unwrap().phase, phase);
        new.piano_note("keys", 69, 0);
        tick(&mut new);
        assert!(
            new.nodes
                .iter()
                .find(|n| n.id == "tone")
                .unwrap()
                .voices
                .iter()
                .filter(|v| v.level > 0.)
                .all(|v| v.releasing)
        );
    }
    #[test]
    fn gate_fallback_and_voice_capacity() {
        for kind in ["synth", "fm_synth"] {
            let mut g = graph(kind, 1);
            g.edges
                .retain(|e| e.target_port == "pitch" || e.target_port == "gate");
            let mut e = Engine::prepare(g, 48000.).unwrap();
            e.piano_note("keys", 60, 100);
            tick(&mut e);
            assert!(e.audio_frame("tone", "out")[0].abs() > 0.001);
            e.piano_note("keys", 60, 0);
            tick(&mut e);
            assert!(
                e.nodes
                    .iter()
                    .find(|n| n.id == "tone")
                    .unwrap()
                    .voices
                    .iter()
                    .filter(|v| v.level > 0.)
                    .all(|v| v.releasing)
            );
            for pitch in 0..127 {
                e.note_scoped("tone", 2, pitch as u32, pitch, 100);
            }
            assert_eq!(
                e.nodes
                    .iter()
                    .find(|n| n.id == "tone")
                    .unwrap()
                    .voices
                    .iter()
                    .filter(|v| v.level > 0. && !v.releasing)
                    .count(),
                64
            );
        }
    }
}

#[cfg(test)]
mod looper_engine_tests {
    use super::*;
    fn graph() -> Graph {
        let mut node = pr0_core::demo_project("p".into(), "p".into(), pr0_core::Mode::Freeform)
            .graph
            .nodes
            .remove(0);
        node.kind = "looper".into();
        node.id = "loop".into();
        node.channels = 1;
        node.parameters = [("max_seconds".into(), 1.)].into();
        Graph {
            nodes: vec![node],
            edges: vec![],
        }
    }
    #[test]
    fn preparation_rejects_excessive_recording_memory() {
        let mut g = graph();
        g.nodes[0].channels = 8;
        g.nodes[0].parameters.insert("max_seconds".into(), 300.);
        assert!(
            Engine::prepare(g, 96000.)
                .err()
                .unwrap()
                .contains("512 MiB")
        );
    }
    #[test]
    fn edits_preserve_recordings_and_apply_new_meter_without_resetting_phase() {
        let mut old = Engine::prepare(graph(), 100.).unwrap();
        old.set_meter(3, 8);
        old.graph_clock.beat = 0.75;
        let clock = old.graph_clock;
        old.nodes[0]
            .looper
            .as_mut()
            .unwrap()
            .tick([0.3; 8], [1., 0., 0., 0., 0.], 0., &clock);
        old.nodes[0]
            .looper
            .as_mut()
            .unwrap()
            .tick([0.; 8], [0., 1., 0., 0., 0.], 0., &clock);
        let mut changed = graph();
        changed.nodes[0].parameters.insert("loop_mode".into(), 0.);
        let mut next = Engine::prepare(changed, 100.).unwrap();
        next.set_meter(5, 8);
        next.carry_node_state(&mut old);
        assert_eq!(next.graph_clock.beat, 0.75);
        assert_eq!(next.graph_clock.bar_beats, 2.5);
        assert_eq!(next.graph_clock.beat_length, 0.5);
        let clock = next.graph_clock;
        assert!(
            (next.nodes[0].looper.as_mut().unwrap().tick(
                [0.; 8],
                [0., 0., 1., 0., 0.],
                0.,
                &clock
            )[0] - 0.3)
                .abs()
                < 1e-6
        );
        let mut changed = graph();
        changed.nodes[0].parameters.insert("max_seconds".into(), 2.);
        let mut reset = Engine::prepare(changed, 100.).unwrap();
        reset.carry_node_state(&mut next);
        assert_eq!(reset.nodes[0].looper.as_ref().unwrap().tracks[0].length, 0);
    }
}

#[cfg(test)]
mod recording_graph_tests {
    use super::*;
    #[test]
    fn record_infers_audio_width_and_retains_capture_through_unrelated_edits() {
        let p = pr0_core::demo_project("p".into(), "Archive".into(), pr0_core::Mode::Freeform);
        let mut tone = p.graph.nodes[0].clone();
        tone.id = "tone".into();
        tone.kind = "oscillator".into();
        tone.channels = 8;
        tone.parameters = [("frequency".into(), 440.), ("amplitude".into(), 0.25)].into();
        let mut record = tone.clone();
        record.id = "record".into();
        record.kind = "record".into();
        record.channels = 1;
        record.parameters.clear();
        let mut start = record.clone();
        start.id = "start".into();
        start.kind = "value".into();
        start.parameters = [("value".into(), 1.)].into();
        let graph = pr0_core::Graph {
            nodes: vec![tone, record, start],
            edges: vec![
                pr0_core::Edge {
                    id: "in".into(),
                    source: "tone".into(),
                    source_port: "out".into(),
                    target: "record".into(),
                    target_port: "in".into(),
                },
                pr0_core::Edge {
                    id: "start".into(),
                    source: "start".into(),
                    source_port: "out".into(),
                    target: "record".into(),
                    target_port: "start".into(),
                },
            ],
        };
        assert!(graph.validate().is_ok());
        let mut engine = Engine::prepare(graph.clone(), 48000.).unwrap();
        engine.render(&[], &mut [[0.; 8]; 100]);
        assert_eq!(engine.telemetry()["record"]["_record_channels"], 8.);
        let mut next = Engine::prepare(graph, 48000.).unwrap();
        next.carry_node_state(&mut engine);
        next.render(&[], &mut [[0.; 8]; 100]);
        next.finish_recordings();
        let chunks = next.take_record_events();
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].2, 8);
        assert_eq!(chunks[0].4.len(), 202);
        assert!(matches!(chunks[0].4[100], recorder::Event::Audio(a) if a[7].abs() > 0.01));
        assert!(engine.take_record_events().is_empty());
        let mut changed = next.graph.clone();
        changed
            .nodes
            .iter_mut()
            .find(|n| n.id == "tone")
            .unwrap()
            .channels = 2;
        let mut changed = Engine::prepare(changed, 48000.).unwrap();
        changed.carry_node_state(&mut next);
        assert_eq!(changed.telemetry()["record"]["_record_channels"], 2.);
    }
}

#[cfg(test)]
mod trigger_tests {
    use super::*;
    #[test]
    fn toggle_latches_manual_state_until_input_changes_and_validates_literals() {
        let p = pr0_core::demo_project("p".into(), "Toggle".into(), pr0_core::Mode::Freeform);
        let mut source = p.graph.nodes[0].clone();
        source.id = "source".into();
        source.kind = "value".into();
        source.parameters = [("value".into(), 0.)].into();
        let mut toggle = source.clone();
        toggle.id = "toggle".into();
        toggle.kind = "toggle".into();
        toggle.parameters.clear();
        toggle.control_value = Some(pr0_core::ControlValue::Number(1.));
        let mut graph = pr0_core::Graph {
            nodes: vec![source, toggle],
            edges: vec![],
        };
        let mut e = Engine::prepare(graph.clone(), 48000.).unwrap();
        let frame = &mut [[0.; 8]; 1];
        e.render(&[], frame);
        assert_eq!(e.telemetry()["toggle"]["_checked"], 1.);
        e.control("toggle", &pr0_core::ControlValue::Number(0.));
        e.render(&[], frame);
        e.render(&[], frame);
        assert_eq!(e.telemetry()["toggle"]["_checked"], 0.);
        graph.edges.push(pr0_core::Edge {
            id: "in".into(),
            source: "source".into(),
            source_port: "out".into(),
            target: "toggle".into(),
            target_port: "in".into(),
        });
        let mut e = Engine::prepare(graph.clone(), 48000.).unwrap();
        e.render(&[], frame);
        assert_eq!(e.telemetry()["toggle"]["_checked"], 0.);
        e.control("toggle", &pr0_core::ControlValue::Number(1.));
        e.render(&[], &mut [[0.; 8]; 100]);
        assert_eq!(e.telemetry()["toggle"]["_checked"], 1.);
        e.parameter("source", "value", 2.).unwrap();
        e.render(&[], frame);
        assert_eq!(e.telemetry()["toggle"]["_checked"], 1.);
        e.control("toggle", &pr0_core::ControlValue::Number(0.));
        e.render(&[], &mut [[0.; 8]; 100]);
        assert_eq!(e.telemetry()["toggle"]["_checked"], 0.);
        e.parameter("source", "value", 3.).unwrap();
        e.render(&[], frame);
        assert_eq!(e.telemetry()["toggle"]["_checked"], 1.);
        e.parameter("source", "value", 0.).unwrap();
        e.render(&[], frame);
        assert_eq!(e.telemetry()["toggle"]["_checked"], 0.);
        for value in [
            pr0_core::ControlValue::Number(0.5),
            pr0_core::ControlValue::Text("on".into()),
        ] {
            graph.nodes[1].control_value = Some(value);
            assert!(graph.validate().unwrap_err().contains("Toggle value"));
        }
    }
    #[test]
    fn trigger_passes_signed_values_and_manual_pulse_overrides_for_one_sample() {
        let p = pr0_core::demo_project("p".into(), "Triggers".into(), pr0_core::Mode::Freeform);
        let mut source = p.graph.nodes[0].clone();
        source.id = "source".into();
        source.kind = "value".into();
        source.parameters = [("value".into(), -2.)].into();
        let mut trigger = source.clone();
        trigger.id = "trigger".into();
        trigger.kind = "trigger".into();
        trigger.parameters.clear();
        let graph = pr0_core::Graph {
            nodes: vec![source, trigger],
            edges: vec![pr0_core::Edge {
                id: "input".into(),
                source: "source".into(),
                source_port: "out".into(),
                target: "trigger".into(),
                target_port: "in".into(),
            }],
        };
        let mut e = Engine::prepare(graph.clone(), 48000.).unwrap();
        let frame = &mut [[0.; 8]; 1];
        e.render(&[], frame);
        assert_eq!(e.telemetry()["trigger"]["_out"], -2.);
        assert_eq!(e.telemetry()["trigger"]["_trigger_sequence"], 1.);
        assert!(e.bang("trigger"));
        e.render(&[], frame);
        assert_eq!(e.telemetry()["trigger"]["_out"], 1.);
        e.render(&[], frame);
        assert_eq!(e.telemetry()["trigger"]["_out"], -2.);
        for value in [0., 1., 1., 0., 3.5, 0.] {
            e.parameter("source", "value", value).unwrap();
            e.render(&[], frame);
            assert_eq!(e.telemetry()["trigger"]["_out"], value);
        }
        let sequence = e.telemetry()["trigger"]["_trigger_sequence"];
        assert!(sequence >= 4.);
        // A one-sample input survives as presentation telemetry after output returns to zero.
        e.parameter("source", "value", -1.).unwrap();
        e.render(&[], frame);
        e.parameter("source", "value", 0.).unwrap();
        e.render(&[], frame);
        assert_eq!(e.telemetry()["trigger"]["_trigger_sequence"], sequence + 1.);
        let mut unplugged = graph;
        unplugged.edges.clear();
        let mut e = Engine::prepare(unplugged, 48000.).unwrap();
        assert!(e.bang("trigger"));
        e.render(&[], frame);
        assert_eq!(e.telemetry()["trigger"]["_out"], 1.);
        e.render(&[], frame);
        assert_eq!(e.telemetry()["trigger"]["_out"], 0.);
        assert!(!e.bang("source"));
    }
}

#[cfg(test)]
mod routing_tests;

#[cfg(test)]
mod typed_midi_tests {
    use super::*;
    fn graph(nested: bool) -> Graph {
        let template = pr0_core::demo_project("x".into(), "x".into(), pr0_core::Mode::Structured)
            .graph
            .nodes[0]
            .clone();
        let node = |id: &str, kind: &str, parent: Option<&str>| {
            let mut n = template.clone();
            n.id = id.into();
            n.kind = kind.into();
            n.label = id.into();
            n.parent = parent.map(str::to_string);
            n.parameters.clear();
            n.part_id = if kind == "part_midi" {
                Some("p".into())
            } else {
                None
            };
            n
        };
        let edge = |id: &str, source: &str, sp: &str, target: &str, tp: &str| pr0_core::Edge {
            id: id.into(),
            source: source.into(),
            source_port: sp.into(),
            target: target.into(),
            target_port: tp.into(),
        };
        if nested {
            Graph {
                nodes: vec![
                    node("source", "part_midi", None),
                    node("group", "subgraph", None),
                    node("in", "subgraph_input_midi", Some("group")),
                    node("out", "subgraph_output_midi", Some("group")),
                    node("sink", "midi_output", None),
                ],
                edges: vec![
                    edge("a", "source", "events", "group", "in"),
                    edge("b", "in", "out", "out", "in"),
                    edge("c", "group", "out", "sink", "events"),
                ],
            }
        } else {
            Graph {
                nodes: vec![
                    node("source", "part_midi", None),
                    node("sink", "midi_output", None),
                ],
                edges: vec![edge("a", "source", "events", "sink", "events")],
            }
        }
    }
    #[test]
    fn channel_messages_preserve_order_bytes_and_nested_boundaries() {
        let messages = [
            (0x91, 60, 100),
            (0x81, 60, 0),
            (0xb2, 11, 64),
            (0xe3, 127, 127),
            (0xc4, 17, 0),
            (0xd5, 90, 0),
            (0xa6, 60, 77),
        ]
        .map(|(status, data1, data2)| pr0_core::midi::Message {
            status,
            data1,
            data2,
        });
        for nested in [false, true] {
            let mut engine = Engine::prepare(graph(nested), 48000.).unwrap();
            engine.part_message("other", messages[0]);
            for message in messages {
                engine.part_message("p", message)
            }
            engine.render(&[], &mut [[0.; 8]]);
            let mut received = Vec::new();
            while let Some(message) = engine.take_midi_message("sink") {
                received.push(message)
            }
            assert_eq!(received, messages);
            engine.render(&[], &mut [[0.; 8]]);
            assert!(engine.take_midi_message("sink").is_none());
        }
    }
    #[test]
    fn overflow_is_bounded_and_sends_all_notes_off() {
        let mut engine = Engine::prepare(graph(false), 48000.).unwrap();
        for _ in 0..257 {
            engine.part_message(
                "p",
                pr0_core::midi::Message {
                    status: 0x90,
                    data1: 60,
                    data2: 90,
                },
            )
        }
        engine.render(&[], &mut [[0.; 8]]);
        for channel in 0..16 {
            assert_eq!(
                engine.take_midi_message("sink"),
                Some(pr0_core::midi::Message {
                    status: 0xb0 | channel,
                    data1: 123,
                    data2: 0
                })
            )
        }
        assert!(engine.take_midi_message("sink").is_none());
        assert_eq!(engine.telemetry()["source"]["_midi_event_dropped"], 257.);
    }
    #[test]
    fn staff_filter_does_not_leak_another_staff() {
        let mut g = graph(false);
        g.nodes[0].parameters.insert("staff".into(), 2.);
        let mut engine = Engine::prepare(g, 48000.).unwrap();
        let message = pr0_core::midi::Message {
            status: 0x90,
            data1: 60,
            data2: 90,
        };
        engine.part_staff_message("p", 1, message);
        engine.render(&[], &mut [[0.; 8]]);
        assert!(engine.take_midi_message("sink").is_none());
        engine.part_staff_message("p", 2, message);
        engine.render(&[], &mut [[0.; 8]]);
        assert_eq!(engine.take_midi_message("sink"), Some(message));
    }
}
