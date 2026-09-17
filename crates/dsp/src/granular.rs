//! Fixed-capacity sample grains and a streaming two-head granular pitch shifter.
use crate::envelope::{Adsr, Settings as EnvelopeSettings};
#[derive(Clone, Copy, Default)]
struct Voice {
    pitch: u8,
    velocity: f64,
    /// 1 while the voice is allocated; 0 marks a free slot.
    level: f64,
    released: bool,
    serial: u64,
    countdown: f64,
    envelope: Adsr,
}
#[derive(Clone, Copy, Default)]
struct Grain {
    voice: usize,
    serial: u64,
    position: f64,
    increment: f64,
    age: usize,
    length: usize,
    gain: f64,
}
pub struct Settings {
    pub root: f64,
    pub position: f64,
    pub spray_ms: f64,
    pub grain_ms: f64,
    pub density: f64,
    pub amplitude: f64,
    pub envelope: EnvelopeSettings,
}
pub struct Granular {
    voices: [Voice; 16],
    grains: [Grain; 128],
    serial: u64,
    next: usize,
    seed: u64,
}
impl Default for Granular {
    fn default() -> Self {
        Self {
            voices: [Voice::default(); 16],
            grains: [Grain::default(); 128],
            serial: 0,
            next: 0,
            seed: 1,
        }
    }
}
impl Granular {
    /// Envelope peak across active voices, plus sounding/held voice counts.
    pub fn envelope_state(&self) -> (f64, usize, usize) {
        self.voices
            .iter()
            .filter(|v| v.level > 0.)
            .fold((0_f64, 0, 0), |(level, active, held), v| {
                (
                    level.max(v.envelope.level()),
                    active + 1,
                    held + usize::from(!v.released),
                )
            })
    }
    /// A continuously held voice reuses the same bounded grain scheduler and windows.
    pub fn cloud(&mut self, pitch: u8, velocity: u8) {
        if self.voices[0].level <= 0. || self.voices[0].released {
            self.note(pitch, velocity.max(1));
        }
        self.voices[0].pitch = pitch;
        self.voices[0].velocity = f64::from(velocity) / 127.;
    }

    pub fn note(&mut self, pitch: u8, velocity: u8) {
        if velocity == 0 {
            if let Some(v) = self
                .voices
                .iter_mut()
                .filter(|v| v.level > 0. && !v.released && v.pitch == pitch)
                .min_by_key(|v| v.serial)
            {
                v.released = true
            }
        } else {
            self.serial = self.serial.wrapping_add(1).max(1);
            let index = self
                .voices
                .iter()
                .position(|v| v.level <= 0.)
                .unwrap_or_else(|| {
                    self.voices
                        .iter()
                        .enumerate()
                        .min_by_key(|(_, v)| v.serial)
                        .unwrap()
                        .0
                });
            self.voices[index] = Voice {
                pitch,
                velocity: velocity as f64 / 127.,
                level: 1.,
                released: false,
                serial: self.serial,
                countdown: 0.,
                envelope: Adsr::default(),
            };
        }
    }
    pub fn render(
        &mut self,
        sample: &[[f32; 8]],
        channels: usize,
        rate: f64,
        s: Settings,
    ) -> [f64; 8] {
        let mut out = [0.; 8];
        if sample.is_empty() {
            return out;
        }
        let length = (s.grain_ms * rate / 1000.).round().max(2.) as usize;
        for index in 0..16 {
            let voice = &mut self.voices[index];
            if voice.level <= 0. {
                continue;
            }
            // Each voice runs its own ADSR; grains stop being scheduled at
            // release and the slot frees once the release has ended.
            voice
                .envelope
                .tick(!voice.released, false, false, s.envelope, rate);
            if voice.envelope.idle() {
                voice.level = 0.;
                continue;
            }
            if voice.released {
                continue;
            }
            voice.countdown -= 1.;
            if voice.countdown <= 0. {
                voice.countdown += rate / s.density.max(1.);
                self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
                let random = (self.seed >> 32) as f64 / u32::MAX as f64 * 2. - 1.;
                let position = (s.position * (sample.len() - 1) as f64
                    + random * s.spray_ms * rate / 1000.)
                    .rem_euclid(sample.len() as f64);
                let slot = self
                    .grains
                    .iter()
                    .position(|g| g.age >= g.length)
                    .unwrap_or(self.next);
                self.next = (slot + 1) % 128;
                self.grains[slot] = Grain {
                    voice: index,
                    serial: voice.serial,
                    position,
                    increment: 2f64.powf((voice.pitch as f64 - s.root) / 12.),
                    age: 0,
                    length,
                    gain: voice.velocity / (s.density * length as f64 / rate * 0.5).max(1.),
                };
            }
        }
        for grain in &mut self.grains {
            if grain.age >= grain.length {
                continue;
            }
            let voice = &self.voices[grain.voice];
            if grain.serial != voice.serial || voice.level <= 0. {
                grain.age = grain.length;
                continue;
            }
            let window =
                0.5 - 0.5 * (std::f64::consts::TAU * grain.age as f64 / grain.length as f64).cos();
            let pos = grain.position.rem_euclid(sample.len() as f64);
            let i = pos as usize;
            let t = pos - i as f64;
            for ch in 0..channels {
                out[ch] += (sample[i][ch] as f64 * (1. - t)
                    + sample[(i + 1) % sample.len()][ch] as f64 * t)
                    * window
                    * grain.gain
                    * voice.envelope.level()
                    * s.amplitude;
            }
            grain.position += grain.increment;
            grain.age += 1;
        }
        out
    }
}
#[derive(Clone, Copy, Default)]
struct Head {
    position: f64,
    increment: f64,
    age: usize,
}
pub struct PitchShift {
    buffer: Vec<[f32; 8]>,
    cursor: usize,
    length: usize,
    hop: usize,
    ticks: usize,
    heads: [Head; 2],
    next: usize,
}
impl PitchShift {
    pub fn new(rate: f64, grain_ms: f64) -> Self {
        let length = (rate * grain_ms / 1000.).round().max(4.) as usize;
        Self {
            buffer: vec![[0.; 8]; length * 5 + 8],
            cursor: 0,
            length,
            hop: length / 2,
            ticks: 0,
            heads: [Head {
                age: length,
                ..Head::default()
            }; 2],
            next: 0,
        }
    }
    pub fn latency(&self) -> usize {
        3 * self.length + 2
    }
    pub fn tick(&mut self, input: [f64; 8], channels: usize, semitones: f64, mix: f64) -> [f64; 8] {
        let size = self.buffer.len();
        let latency = self.latency();
        self.buffer[self.cursor] = input.map(|x| x as f32);
        if self.ticks % self.hop == 0 {
            self.heads[self.next] = Head {
                position: ((self.cursor + size - latency) % size) as f64,
                increment: 2f64.powf(semitones / 12.),
                age: 0,
            };
            self.next = (self.next + 1) % 2;
        }
        let mut wet = [0.; 8];
        let mut weight = 0.;
        for head in &mut self.heads {
            if head.age >= self.length {
                continue;
            }
            let window =
                0.5 - 0.5 * (std::f64::consts::TAU * head.age as f64 / self.length as f64).cos();
            let pos = head.position.rem_euclid(size as f64);
            let index = pos as usize;
            let t = pos - index as f64;
            for ch in 0..channels {
                wet[ch] += (self.buffer[index][ch] as f64 * (1. - t)
                    + self.buffer[(index + 1) % size][ch] as f64 * t)
                    * window;
            }
            weight += window;
            head.position += head.increment;
            head.age += 1;
        }
        let dry = self.buffer[(self.cursor + size - latency) % size];
        let output = std::array::from_fn(|ch| {
            if ch < channels {
                dry[ch] as f64 * (1. - mix)
                    + if weight > 1e-9 {
                        wet[ch] / weight * mix
                    } else {
                        0.
                    }
            } else {
                0.
            }
        });
        self.cursor = (self.cursor + 1) % size;
        self.ticks = self.ticks.wrapping_add(1);
        output
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn settings() -> Settings {
        Settings {
            root: 60.,
            position: 0.25,
            spray_ms: 0.,
            grain_ms: 40.,
            density: 50.,
            amplitude: 0.5,
            envelope: EnvelopeSettings { attack: 0., decay: 0., sustain: 1., release: 20., reset: false },
        }
    }
    #[test]
    fn cloud_grains_scatter_around_center_and_use_smooth_windows() {
        let sample = vec![[1.; 8]; 10000];
        let mut cloud = Granular::default();
        cloud.cloud(60, 127);
        let mut starts = Vec::new();
        let mut previous = 0.;
        for i in 0..1000 {
            let out = cloud.render(
                &sample,
                8,
                1000.,
                Settings {
                    position: 0.5,
                    spray_ms: 30.,
                    grain_ms: 20.,
                    density: 10.,
                    amplitude: 1.,
                    ..settings()
                },
            );
            if let Some(grain) = cloud.grains.iter().find(|g| g.age == 1) {
                assert_eq!(out, [0.;8]);
                starts.push(grain.position);
            }
            if cloud.grains.iter().any(|g| g.age == 11) { assert!(out[0] > 0.99); }
            if cloud.grains.iter().all(|g| g.age >= g.length) && i > 0 {
                // The last rendered grain sample approaches zero before the gap.
                assert!(out[0] < 0.025);
            }
            assert!((out[0] - previous).abs() < 0.2);
            previous = out[0];
        }
        assert!(starts.iter().all(|p| *p >= 4970. && *p <= 5031.));
        assert!(starts.windows(2).any(|w| (w[0] - w[1]).abs() > 1.));
    }
    #[test]
    fn sample_grains_preserve_channels_release_repeated_notes_and_stay_bounded() {
        let sample: Vec<[f32; 8]> = (0..4800)
            .map(|i| {
                std::array::from_fn(|ch| ((i as f64 * 0.05).sin() * (ch + 1) as f64 * 0.1) as f32)
            })
            .collect();
        let mut synth = Granular::default();
        synth.note(60, 100);
        synth.note(60, 80);
        let mut energy = 0.;
        for _ in 0..4800 {
            let out = synth.render(&sample, 8, 48000., settings());
            energy += out[0] * out[0];
            assert!((out[7] - out[0] * 8.).abs() < 1e-6);
        }
        assert!(energy > 1.);
        synth.note(60, 0);
        assert_eq!(
            synth
                .voices
                .iter()
                .filter(|v| !v.released && v.level > 0.)
                .count(),
            1
        );
        synth.note(60, 0);
        for _ in 0..2000 {
            synth.render(&sample, 8, 48000., settings());
        }
        assert_eq!(synth.render(&sample, 8, 48000., settings()), [0.; 8]);
        for pitch in 0..127 {
            synth.note(pitch, 127);
        }
        for _ in 0..4800 {
            assert!(
                synth
                    .render(
                        &sample,
                        8,
                        48000.,
                        Settings {
                            grain_ms: 500.,
                            density: 100.,
                            ..settings()
                        }
                    )
                    .iter()
                    .all(|x| x.is_finite())
            );
        }
        assert_eq!(synth.voices.len(), 16);
        assert_eq!(synth.grains.len(), 128);
    }
    #[test]
    fn live_shift_unity_matches_delayed_input_and_octave_changes_frequency() {
        let rate = 48000.;
        let mut shift = PitchShift::new(rate, 50.);
        let latency = shift.latency();
        for i in 0..12000 {
            let value = (std::f64::consts::TAU * 440. * i as f64 / rate).sin();
            let out = shift.tick([value; 8], 8, 0., 1.);
            let expected = if i >= latency {
                (std::f64::consts::TAU * 440. * (i - latency) as f64 / rate).sin()
            } else {
                0.
            };
            for x in out {
                assert!((x - expected).abs() < 1e-6);
            }
        }
        let mut shift = PitchShift::new(rate, 50.);
        let mut bins = vec![rustfft::num_complex::Complex::default(); 8192];
        for i in 0..24000 {
            let x = (std::f64::consts::TAU * 440. * i as f64 / rate).sin();
            let out = shift.tick([x; 8], 8, 12., 1.);
            if i >= 24000 - 8192 {
                bins[i - (24000 - 8192)].re = out[0] as f32;
            }
        }
        crate::Fourier::new(8192).unwrap().forward(&mut bins);
        let peak = (1..4096)
            .max_by(|a, b| bins[*a].norm_sqr().total_cmp(&bins[*b].norm_sqr()))
            .unwrap();
        assert!((peak as f64 * rate / 8192. - 880.).abs() < 6.);
    }
}
