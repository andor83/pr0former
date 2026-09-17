//! Two-dimensional multi-source granular cloud with live-input capture rings.
//!
//! Up to eight project samples and two live audio inputs sit on a field with
//! coordinates in -1..1. A control point roams the field; every grain picks its
//! source at birth with probability proportional to `exp(-(distance/focus)^2)`.
//! Live inputs are written into rings prepared at their ten-second maximum and
//! read as if they were samples running from the oldest frame to the newest.
//! All storage is prepared up front; rendering never allocates.
use std::f64::consts::TAU;

pub const SAMPLES: usize = 8;
pub const LIVE: usize = 2;
/// Slots `0..SAMPLES` are samples, `SAMPLES..SOURCES` the live inputs.
pub const SOURCES: usize = SAMPLES + LIVE;
pub const GRAINS: usize = 128;
/// Every connected live input reserves this much audio so its usable window can
/// change while the engine runs.
pub const MAX_BUFFER_SECONDS: f64 = 10.;
pub const RING_BYTES_PER_FRAME: f64 = 8. * 4.;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Kind {
    Missing,
    /// Index into the node's sample bank.
    Sample(usize),
    /// Index of the capture ring.
    Live(usize),
}

#[derive(Clone, Copy, Debug)]
pub struct Source {
    pub x: f64,
    pub y: f64,
    /// Semitone offset applied to every grain from this source.
    pub tune: f64,
    pub gain: f64,
    pub kind: Kind,
}
impl Source {
    pub const MISSING: Source = Source { x: 0., y: 0., tune: 0., gain: 0., kind: Kind::Missing };
}

pub struct Settings {
    pub x: f64,
    pub y: f64,
    pub focus: f64,
    /// Semitones: a fixed transposition, or the random range when `randomize_pitch`.
    pub pitch: f64,
    pub randomize_pitch: bool,
    pub position: f64,
    pub spray_ms: f64,
    pub grain_ms: f64,
    pub density: f64,
    pub amplitude: f64,
    /// Usable window of each live ring in milliseconds.
    pub buffer_ms: [f64; LIVE],
}

#[derive(Clone, Copy, Default)]
struct Grain {
    slot: usize,
    position: f64,
    increment: f64,
    age: usize,
    length: usize,
    gain: f64,
}

/// Capture ring: `cursor` is the next frame to overwrite; the newest valid frame
/// is one behind it and the oldest is `filled` frames behind it.
#[derive(Default)]
struct Ring {
    frames: Vec<[f32; 8]>,
    cursor: usize,
    filled: usize,
}

/// Positions of every field parameter in the node's value table, resolved once at
/// prepare so rendering never searches parameter names.
#[derive(Clone, Copy, Debug)]
pub struct Indices {
    pub x: usize,
    pub y: usize,
    pub focus: usize,
    pub pitch: usize,
    pub randomize_pitch: usize,
    pub position: usize,
    pub spray: usize,
    pub grain_ms: usize,
    pub density: usize,
    pub amplitude: usize,
    pub sample: [usize; SAMPLES],
    pub source_x: [usize; SAMPLES],
    pub source_y: [usize; SAMPLES],
    pub source_tune: [usize; SAMPLES],
    pub source_gain: [usize; SAMPLES],
    pub live_x: [usize; LIVE],
    pub live_y: [usize; LIVE],
    pub live_tune: [usize; LIVE],
    pub live_gain: [usize; LIVE],
    pub live_buffer_ms: [usize; LIVE],
}
impl Indices {
    pub fn resolve(names: &[String]) -> Result<Self, String> {
        let find = |key: String| {
            names
                .iter()
                .position(|n| *n == key)
                .ok_or_else(|| format!("Granular Field parameter {key} missing from descriptor"))
        };
        let per_sample = |prefix: &str, suffix: &str| -> Result<[usize; SAMPLES], String> {
            let mut out = [0; SAMPLES];
            for (i, slot) in out.iter_mut().enumerate() {
                *slot = find(format!("{prefix}{}{suffix}", i + 1))?;
            }
            Ok(out)
        };
        let per_live = |suffix: &str| -> Result<[usize; LIVE], String> {
            let mut out = [0; LIVE];
            for (i, slot) in out.iter_mut().enumerate() {
                *slot = find(format!("live_{}{suffix}", i + 1))?;
            }
            Ok(out)
        };
        Ok(Self {
            x: find("x".into())?,
            y: find("y".into())?,
            focus: find("focus".into())?,
            pitch: find("pitch".into())?,
            randomize_pitch: find("randomize_pitch".into())?,
            position: find("position".into())?,
            spray: find("spray".into())?,
            grain_ms: find("grain_ms".into())?,
            density: find("density".into())?,
            amplitude: find("amplitude".into())?,
            sample: per_sample("sample_", "")?,
            source_x: per_sample("source_", "_x")?,
            source_y: per_sample("source_", "_y")?,
            source_tune: per_sample("source_", "_tune")?,
            source_gain: per_sample("source_", "_gain")?,
            live_x: per_live("_x")?,
            live_y: per_live("_y")?,
            live_tune: per_live("_tune")?,
            live_gain: per_live("_gain")?,
            live_buffer_ms: per_live("_buffer_ms")?,
        })
    }
}

pub struct GranularField {
    pub indices: Indices,
    grains: [Grain; GRAINS],
    next: usize,
    countdown: f64,
    seed: u64,
    rings: [Ring; LIVE],
    /// (requested asset, bank index) per sample slot; re-validated against the bank on use.
    slot_cache: [(u32, Option<usize>); SAMPLES],
    /// Normalised selection weights from the most recent spawn attempt.
    pub weights: [f64; SOURCES],
    pub slot_missing: [bool; SAMPLES],
    pub active_grains: usize,
}

impl GranularField {
    /// Reserve a ten-second ring for every connected live input. Unconnected
    /// inputs get an empty ring and never take part in the field.
    pub fn prepare(rate: f64, connected: [bool; LIVE], names: &[String]) -> Result<Self, String> {
        let indices = Indices::resolve(names)?;
        let mut rings: [Ring; LIVE] = Default::default();
        for (ring, connected) in rings.iter_mut().zip(connected) {
            if connected {
                let len = (rate * MAX_BUFFER_SECONDS).ceil().max(2.) as usize;
                ring.frames
                    .try_reserve_exact(len)
                    .map_err(|_| "Unable to reserve Granular Field live buffer memory")?;
                ring.frames.resize(len, [0.; 8]);
            }
        }
        Ok(Self {
            indices,
            grains: [Grain::default(); GRAINS],
            next: 0,
            countdown: 0.,
            seed: 1,
            rings,
            slot_cache: [(0, None); SAMPLES],
            weights: [0.; SOURCES],
            slot_missing: [false; SAMPLES],
            active_grains: 0,
        })
    }
    /// Same rings allocated: a live replacement can carry captured audio and grains.
    pub fn compatible(&self, other: &Self) -> bool {
        self.rings.iter().zip(&other.rings).all(|(a, b)| a.frames.len() == b.frames.len())
    }
    pub fn live_connected(&self, ring: usize) -> bool {
        self.rings.get(ring).is_some_and(|r| !r.frames.is_empty())
    }
    pub fn write_live(&mut self, ring: usize, frame: [f64; 8]) {
        let Some(ring) = self.rings.get_mut(ring) else { return };
        let len = ring.frames.len();
        if len == 0 {
            return;
        }
        ring.frames[ring.cursor] = frame.map(|v| v as f32);
        ring.cursor = (ring.cursor + 1) % len;
        ring.filled = (ring.filled + 1).min(len);
    }
    fn usable(&self, ring: usize, buffer_ms: f64, rate: f64) -> usize {
        let len = self.rings[ring].frames.len();
        ((buffer_ms * rate / 1000.).round() as usize).clamp(2.min(len), len)
    }
    /// Share of the usable window already captured, 0 for an unconnected input.
    pub fn ring_fill(&self, ring: usize, buffer_ms: f64, rate: f64) -> f64 {
        if !self.live_connected(ring) {
            return 0.;
        }
        let usable = self.usable(ring, buffer_ms, rate);
        self.rings[ring].filled.min(usable) as f64 / usable as f64
    }
    /// Bank index for a slot's asset, cached and re-checked so bank swaps heal themselves.
    pub fn resolve_slot(&mut self, slot: usize, asset: u32, bank: &[(u32, Vec<[f32; 8]>)]) -> Option<usize> {
        let (cached_asset, cached_index) = self.slot_cache[slot];
        if cached_asset == asset {
            if let Some(index) = cached_index {
                if bank.get(index).is_some_and(|b| b.0 == asset && b.1.len() >= 2) {
                    return Some(index);
                }
            }
        }
        let found = bank.iter().position(|(id, frames)| *id == asset && frames.len() >= 2);
        self.slot_cache[slot] = (asset, found);
        self.slot_missing[slot] = found.is_none();
        found
    }
    fn random(&mut self) -> f64 {
        self.seed = self.seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.seed >> 32) as f64 / u32::MAX as f64
    }
    /// Choose a source by field distance and start one grain from it.
    fn spawn(&mut self, bank: &[(u32, Vec<[f32; 8]>)], sources: &[Source; SOURCES], s: &Settings, length: usize, rate: f64) {
        let focus = s.focus.max(0.05);
        let mut total = 0.;
        for (i, source) in sources.iter().enumerate() {
            self.weights[i] = if source.kind == Kind::Missing {
                0.
            } else {
                let d2 = (s.x - source.x).powi(2) + (s.y - source.y).powi(2);
                (-d2 / (focus * focus)).exp()
            };
            total += self.weights[i];
        }
        if total < 1e-12 {
            self.weights = [0.; SOURCES];
            return;
        }
        for w in &mut self.weights {
            *w /= total;
        }
        let draw = self.random();
        let mut cumulative = 0.;
        let mut slot = SOURCES - 1;
        for (i, w) in self.weights.iter().enumerate() {
            cumulative += w;
            if draw < cumulative {
                slot = i;
                break;
            }
        }
        let source = sources[slot];
        if source.kind == Kind::Missing {
            return;
        }
        let offset = if s.randomize_pitch { (self.random() * 2. - 1.) * s.pitch.abs() } else { s.pitch };
        let increment = 2f64.powf((source.tune + offset) / 12.);
        let spray = (self.random() * 2. - 1.) * s.spray_ms * rate / 1000.;
        let position = match source.kind {
            Kind::Sample(b) => {
                let len = bank[b].1.len();
                if len < 2 {
                    return;
                }
                (s.position * (len - 1) as f64 + spray).rem_euclid(len as f64)
            }
            Kind::Live(r) => {
                let len = self.rings[r].frames.len();
                if len < 2 {
                    return;
                }
                let usable = self.usable(r, s.buffer_ms[r], rate);
                let filled = self.rings[r].filled.min(usable) as f64;
                // The grain reads `length` frames at `increment` while the head advances one
                // frame per sample. Keep every read between the oldest valid frame and the head.
                let span = (length - 1) as f64;
                let min_distance = 1. + (span * (increment - 1.)).max(0.);
                let max_distance = filled - (span * (1. - increment)).max(0.);
                if filled < 2. || min_distance > max_distance {
                    return;
                }
                let desired = filled - (s.position * (filled - 1.) + spray).clamp(0., filled - 1.);
                let distance = desired.clamp(min_distance, max_distance);
                (self.rings[r].cursor as f64 - distance).rem_euclid(len as f64)
            }
            Kind::Missing => return,
        };
        let index = self
            .grains
            .iter()
            .position(|g| g.age >= g.length)
            .unwrap_or(self.next);
        self.next = (index + 1) % GRAINS;
        self.grains[index] = Grain {
            slot,
            position,
            increment,
            age: 0,
            length,
            gain: source.gain / (s.density * length as f64 / rate * 0.5).max(1.),
        };
    }
    pub fn render(
        &mut self,
        bank: &[(u32, Vec<[f32; 8]>)],
        sources: &[Source; SOURCES],
        s: &Settings,
        channels: usize,
        rate: f64,
    ) -> [f64; 8] {
        let mut out = [0.; 8];
        let length = (s.grain_ms * rate / 1000.).round().max(2.) as usize;
        self.countdown -= 1.;
        if self.countdown <= 0. {
            self.countdown += rate / s.density.max(1.);
            self.spawn(bank, sources, s, length, rate);
        }
        let mut active = 0;
        let rings = &self.rings;
        for grain in &mut self.grains {
            if grain.age >= grain.length {
                continue;
            }
            let frames: &[[f32; 8]] = match sources[grain.slot].kind {
                Kind::Sample(b) => &bank[b].1,
                Kind::Live(r) => &rings[r].frames,
                Kind::Missing => &[],
            };
            if frames.len() < 2 {
                grain.age = grain.length;
                continue;
            }
            active += 1;
            let window = 0.5 - 0.5 * (TAU * grain.age as f64 / grain.length as f64).cos();
            let pos = grain.position.rem_euclid(frames.len() as f64);
            let i = pos as usize;
            let t = pos - i as f64;
            let next = (i + 1) % frames.len();
            let gain = window * grain.gain * s.amplitude;
            for ch in 0..channels.min(8) {
                out[ch] += (frames[i][ch] as f64 * (1. - t) + frames[next][ch] as f64 * t) * gain;
            }
            grain.position += grain.increment;
            grain.age += 1;
        }
        self.active_grains = active;
        out
    }
    #[cfg(test)]
    fn live_grains(&self) -> impl Iterator<Item = &Grain> {
        self.grains.iter().filter(|g| g.age < g.length && g.slot >= SAMPLES)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn names() -> Vec<String> {
        pr0_core::catalog()
            .into_iter()
            .find(|d| d.kind == "granular_field")
            .unwrap()
            .parameters
            .into_iter()
            .map(|p| p.id)
            .collect()
    }
    fn settings() -> Settings {
        Settings {
            x: 0.,
            y: 0.,
            focus: 0.5,
            pitch: 0.,
            randomize_pitch: false,
            position: 1.,
            spray_ms: 0.,
            grain_ms: 50.,
            density: 20.,
            amplitude: 1.,
            buffer_ms: [100., 100.],
        }
    }
    fn live_source() -> [Source; SOURCES] {
        let mut sources = [Source::MISSING; SOURCES];
        sources[SAMPLES] = Source { x: 0., y: 0., tune: 0., gain: 1., kind: Kind::Live(0) };
        sources
    }
    #[test]
    fn ring_grains_never_read_across_the_write_head() {
        for (pitch, randomize) in [(-12., false), (0., false), (12., false), (24., false), (24., true)] {
            let mut field = GranularField::prepare(1000., [true, false], &names()).unwrap();
            let mut s = settings();
            s.pitch = pitch;
            s.randomize_pitch = randomize;
            s.position = 0.5;
            s.spray_ms = 40.;
            // Four-times-speed grains need a lookback longer than 100 ms; give the window room.
            s.buffer_ms = [400., 400.];
            let sources = live_source();
            let mut spawned = 0;
            for t in 0..600 {
                field.write_live(0, [t as f64; 8]);
                field.render(&[], &sources, &s, 1, 1000.);
                let ring = &field.rings[0];
                let usable = field.usable(0, s.buffer_ms[0], 1000.) as f64;
                let valid = ring.filled.min(usable as usize) as f64;
                for grain in field.live_grains() {
                    spawned += 1;
                    // The grain already advanced past its last read; its next read happens after
                    // the head moves one frame, so that read sits one frame further back.
                    let next_read = (ring.cursor as f64 - grain.position).rem_euclid(ring.frames.len() as f64) + 1.;
                    assert!(
                        next_read >= 1. - 1e-9 && next_read <= valid + 1. + 1e-9,
                        "pitch {pitch} randomize {randomize} t {t}: next read {next_read} outside 1..{}", valid + 1.
                    );
                }
            }
            assert!(spawned > 0, "pitch {pitch}: grains were born");
        }
    }
    #[test]
    fn weights_follow_gaussian_distance_and_a_tiny_focus_picks_the_nearest() {
        let bank = vec![(1, vec![[0.5; 8]; 100]), (2, vec![[-0.5; 8]; 100])];
        let mut sources = [Source::MISSING; SOURCES];
        sources[0] = Source { x: -0.5, y: 0., tune: 0., gain: 1., kind: Kind::Sample(0) };
        sources[1] = Source { x: 0.5, y: 0., tune: 0., gain: 1., kind: Kind::Sample(1) };
        let mut field = GranularField::prepare(1000., [false, false], &names()).unwrap();
        let mut s = settings();
        field.render(&bank, &sources, &s, 1, 1000.);
        assert!((field.weights[0] - 0.5).abs() < 1e-9 && (field.weights[1] - 0.5).abs() < 1e-9);
        s.x = -0.5;
        s.focus = 0.05;
        let mut field = GranularField::prepare(1000., [false, false], &names()).unwrap();
        let mut energy = 0.;
        for _ in 0..2000 {
            energy += field.render(&bank, &sources, &s, 1, 1000.)[0];
        }
        assert!(field.weights[0] > 0.999 && field.weights[1] < 1e-6, "{:?}", field.weights);
        assert!(energy > 1., "only the positive source sounds: {energy}");
    }
    #[test]
    fn no_sources_or_far_control_point_stays_silent() {
        let mut field = GranularField::prepare(1000., [false, false], &names()).unwrap();
        let s = settings();
        for _ in 0..500 {
            assert_eq!(field.render(&[], &[Source::MISSING; SOURCES], &s, 2, 1000.), [0.; 8]);
        }
        assert_eq!(field.active_grains, 0);
    }
    #[test]
    fn fixed_pitch_is_uniform_and_random_pitch_scatters_within_range() {
        let mut field = GranularField::prepare(1000., [true, false], &names()).unwrap();
        let mut s = settings();
        s.pitch = 12.;
        s.density = 100.;
        s.grain_ms = 10.;
        let sources = live_source();
        for t in 0..400 {
            field.write_live(0, [(t as f64 * 0.1).sin(); 8]);
            field.render(&[], &sources, &s, 1, 1000.);
        }
        let increments: Vec<f64> = field.live_grains().map(|g| g.increment).collect();
        assert!(!increments.is_empty() && increments.iter().all(|i| (i - 2.).abs() < 1e-9), "{increments:?}");
        s.randomize_pitch = true;
        let mut field = GranularField::prepare(1000., [true, false], &names()).unwrap();
        let mut seen = Vec::new();
        for t in 0..400 {
            field.write_live(0, [(t as f64 * 0.1).sin(); 8]);
            field.render(&[], &sources, &s, 1, 1000.);
            seen.extend(field.live_grains().map(|g| g.increment));
        }
        assert!(seen.iter().all(|i| *i >= 0.5 - 1e-9 && *i <= 2. + 1e-9), "within an octave either way");
        let spread = seen.iter().cloned().fold(0., f64::max) - seen.iter().cloned().fold(f64::MAX, f64::min);
        assert!(spread > 0.5, "increments vary: {spread}");
    }
}
