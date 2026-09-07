//! Prepared DSP graph. `render` performs no allocation or locking.
mod channels;
mod clock_ratio;
mod effects;
mod envelope;
mod sequence;
mod spectral;
pub mod stretch;
use pr0_core::{Graph, MAX_CHANNELS, catalog};
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
    source_port: usize,
    destination: usize,
    parameter: bool,
    signal: pr0_core::Signal,
}
#[derive(Clone, Copy, Default)]
struct Voice {
    owner: u64,
    note_id: u32,
    pitch: u8,
    phase: f64,
    level: f64,
    releasing: bool,
}
struct RuntimeNode {
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
    compensations: Vec<Vec<[f64; 8]>>,
    compensation_cursors: Vec<usize>,
    latency: usize,
    input: [[f64; MAX_CHANNELS]; 8],
    output: [f64; MAX_CHANNELS],
    control: [f64; 3],
    voices: [Voice; 64],
    external: [f32; MAX_CHANNELS],
    phase: f64,
    previous: f64,
    count: f64,
    seed: u64,
    envelope: f64,
    smooth: f64,
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
        match self.kind.as_str() {
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
                self.control =
                    self.clock_ratio
                        .tick_clock(clock, self.p("multiply"), self.p("divide"));
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
            "value" => scalar = self.p("value"),
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
                for (ch, out) in self.output[..self.channels].iter_mut().enumerate() {
                    *out = hardware.get(ch + offset).copied().unwrap_or(0.) as f64;
                }
            }
            "oscillator" => {
                self.smooth +=
                    (self.p("frequency") - self.smooth) * (1. - (-1. / (0.005 * sr)).exp());
                self.phase = (self.phase + self.smooth / sr).fract();
                let v = (TAU * self.phase).sin() * self.p("amplitude");
                self.output[..self.channels].fill(v);
            }
            "synth" => {
                let mut sum = 0.;
                let release = (-1. / (self.p("release") * 0.001 * sr)).exp();
                for v in &mut self.voices {
                    if v.level < 1e-5 {
                        v.level = 0.;
                        continue;
                    }
                    v.phase =
                        (v.phase + 440. * 2_f64.powf((v.pitch as f64 - 69.) / 12.) / sr).fract();
                    sum += (v.phase * TAU).sin() * v.level;
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
            "spectral_gain" | "to_polar" | "to_cartesian" => {
                let gain = self.p("gain");
                self.spectral.as_mut().unwrap().transform(&self.kind, gain);
            }
            "trigger" => {
                scalar = if input[0] > 0. && self.previous <= 0. {
                    1.
                } else {
                    0.
                };
                self.previous = input[0];
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
    pub graph: Graph,
}
impl Engine {
    pub fn prepare(graph: Graph, sample_rate: f64) -> Result<Self, String> {
        let order = graph.validate()?;
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
                spectral: if d.category == "Spectral" {
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
                channels: n.channels,
                names: d.parameters.iter().map(|p| p.id.clone()).collect(),
                defaults: values.clone(),
                values,
                limits: d.parameters.iter().map(|p| (p.min, p.max)).collect(),
                bindings: vec![],
                compensations: vec![],
                compensation_cursors: vec![],
                latency: 0,
                input: [[0.; MAX_CHANNELS]; 8],
                output: [0.; MAX_CHANNELS],
                control: [0.; 3],
                voices: [Voice::default(); 64],
                external: [0.; MAX_CHANNELS],
                phase: 0.,
                previous: -1.,
                count: 0.,
                seed: n.parameters.get("seed").copied().unwrap_or(1.) as u64,
                envelope: 0.,
                smooth: 0.,
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
        for edge in &graph.edges {
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
                source_port,
                destination,
                parameter: parameter.is_some(),
                signal: sd.outputs[source_port].signal,
            });
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
                + if matches!(nodes[idx].kind.as_str(), "fft" | "rfft") {
                    nodes[idx].p("size") as usize
                } else {
                    0
                };
        }
        Ok(Self {
            nodes,
            order,
            clock: Clock::new(sample_rate),
            graph,
        })
    }
    /// Move state for unchanged nodes into a prepared graph. Binding indices
    /// belong to each compiled graph and must never move with runtime history.
    /// Runs between render blocks; swapping prepared storage does not allocate.
    pub fn carry_node_state(&mut self, previous: &mut Self) {
        if self.clock.sample_rate != previous.clock.sample_rate {
            return;
        }
        for index in 0..self.nodes.len() {
            let target = &self.nodes[index];
            let Some(source_index) = previous.nodes.iter().position(|n| n.id == target.id) else {
                continue;
            };
            let source = &previous.nodes[source_index];
            let compatible = target.kind == source.kind
                && target.channels == source.channels
                && target.defaults == source.defaults
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
                continue;
            }
            let target = &mut self.nodes[index];
            let source = &mut previous.nodes[source_index];
            std::mem::swap(target, source);
            std::mem::swap(&mut target.bindings, &mut source.bindings);
            // Sequencer migration already selected voices by surviving part.
            // Do not resurrect voices from deleted or musically changed parts.
            if target.kind == "synth" {
                std::mem::swap(&mut target.voices, &mut source.voices);
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
            .find(|n| n.id == node && n.kind == "synth")
        else {
            return;
        };
        let Some(target) = self
            .nodes
            .iter_mut()
            .find(|n| n.id == node && n.kind == "synth")
        else {
            return;
        };
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
    pub fn note(&mut self, node: &str, pitch: u8, velocity: u8) {
        self.note_scoped(node, 0, pitch as u32, pitch, velocity);
    }
    /// A score note belongs to a part and note instance, even when pitches match.
    /// Voice storage is prepared with the graph; dispatch does not allocate.
    pub fn note_scoped(&mut self, node: &str, owner: u64, note_id: u32, pitch: u8, velocity: u8) {
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id == node) {
            if n.kind == "synth" {
                if velocity == 0 {
                    for v in &mut n.voices {
                        if v.owner == owner && v.note_id == note_id && v.pitch == pitch {
                            v.releasing = true;
                        }
                    }
                } else {
                    let index = n
                        .voices
                        .iter()
                        .position(|v| v.level < 1e-5)
                        .unwrap_or_else(|| {
                            n.voices
                                .iter()
                                .enumerate()
                                .min_by(|(_, a), (_, b)| a.level.total_cmp(&b.level))
                                .map(|(i, _)| i)
                                .unwrap_or(0)
                        });
                    n.voices[index] = Voice {
                        owner,
                        note_id,
                        pitch,
                        phase: 0.,
                        level: velocity as f64 / 127.,
                        releasing: false,
                    };
                }
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
    pub fn external(&mut self, node: &str, sample: [f32; MAX_CHANNELS]) {
        if let Some(n) = self
            .nodes
            .iter_mut()
            .find(|n| n.id == node && n.kind == "browser_input")
        {
            n.external = sample;
        }
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
    pub fn render(&mut self, input: &[[f32; MAX_CHANNELS]], output: &mut [[f32; MAX_CHANNELS]]) {
        for (frame_idx, out) in output.iter_mut().enumerate() {
            let hardware = input.get(frame_idx).copied().unwrap_or([0.; MAX_CHANNELS]);
            *out = [0.; MAX_CHANNELS];
            for &idx in &self.order {
                self.nodes[idx].input = [[0.; MAX_CHANNELS]; 8];
                for j in 0..self.nodes[idx].bindings.len() {
                    let b = self.nodes[idx].bindings[j].clone();
                    let control = self.nodes[b.source]
                        .control
                        .get(b.source_port)
                        .copied()
                        .unwrap_or(0.);
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
                        } else {
                            self.nodes[idx].input[b.destination] = audio;
                        }
                    }
                }
                self.nodes[idx].process(&self.clock, &hardware);
                if self.nodes[idx].kind == "output" {
                    for (ch, sample) in out.iter_mut().enumerate() {
                        *sample += self.nodes[idx].output[ch] as f32;
                    }
                }
            }
            for x in out.iter_mut() {
                *x = x.clamp(-1., 1.);
            }
            self.clock.advance();
        }
    }
    /// Called by the non-realtime orchestration worker between blocks.
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
                values.insert("_out".into(), n.control[0]);
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
    fn clock_ratio_graph_preserves_tempo_phase_and_restarts_at_zero() {
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
        assert_eq!(e.nodes[0].control[0], 1.);
        e.clock.beat = 0.125;
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.nodes[0].control, [0., 0.5, 0.]);
        e.clock.set_tempo(60.);
        e.clock.beat = 0.25;
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.nodes[0].control, [1., 0., 1.]);
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
        assert_eq!(e.nodes[1].control, [60., 0., 1.]);
        e.render(&[], &mut [[0.; 8]; 64]);
        assert_eq!(e.nodes[1].control, [60., 0., 0.]);
        e.parameter("trigger", "value", 0.).unwrap();
        e.render(&[], &mut [[0.; 8]; 1]);
        e.parameter("trigger", "value", 1.).unwrap();
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.nodes[1].control, [67., 1., 1.]);
        e.parameter("steps", "step_2", 69.).unwrap();
        e.render(&[], &mut [[0.; 8]; 1]);
        assert_eq!(e.nodes[1].control, [69., 1., 0.]);
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
