use crate::envelope::{Adsr, Settings};
use pr0_core::MAX_CHANNELS;
#[derive(Clone, Copy, Default)]
struct Voice {
    pitch: u8,
    root: f64,
    increment: f64,
    velocity: f64,
    position: f64,
    envelope: Adsr,
    active: bool,
    released: bool,
    serial: u64,
}
pub struct Sampler {
    voices: [Voice; 64],
    serial: u64,
    running: bool,
}
impl Default for Sampler {
    fn default() -> Self {
        Self {
            voices: [Voice::default(); 64],
            serial: 0,
            running: false,
        }
    }
}
impl Sampler {
    /// Envelope peak across active voices, plus sounding/held voice counts.
    pub fn envelope_state(&self) -> (f64, usize, usize) {
        self.voices
            .iter()
            .filter(|v| v.active)
            .fold((0_f64, 0, 0), |(level, active, held), v| {
                (
                    level.max(v.envelope.level()),
                    active + 1,
                    held + usize::from(!v.released),
                )
            })
    }
    pub fn note(&mut self, pitch: u8, velocity: u8) {
        if velocity == 0 {
            if let Some(voice) = self
                .voices
                .iter_mut()
                .filter(|v| v.active && !v.released && v.pitch == pitch)
                .min_by_key(|v| v.serial)
            {
                voice.released = true;
            }
        } else {
            self.serial = self.serial.saturating_add(1);
            let index = self
                .voices
                .iter()
                .position(|v| !v.active)
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
                root: f64::NAN,
                increment: 0.,
                velocity: f64::from(velocity) / 127.,
                position: 0.,
                envelope: Adsr::default(),
                active: true,
                released: false,
                serial: self.serial,
            };
        }
    }
    pub fn render(
        &mut self,
        sample: &[[f32; MAX_CHANNELS]],
        root: f64,
        amplitude: f64,
        looping: bool,
        envelope: Settings,
        rate: f64,
        running: bool,
    ) -> [f64; MAX_CHANNELS] {
        if self.running && !running {
            for voice in &mut self.voices {
                voice.released = true;
            }
        }
        self.running = running;
        let mut output = [0.; MAX_CHANNELS];
        if sample.is_empty() {
            return output;
        }
        for voice in &mut self.voices {
            if !voice.active {
                continue;
            }
            if voice.position >= sample.len() as f64 {
                if looping {
                    voice.position %= sample.len() as f64;
                } else {
                    voice.active = false;
                    continue;
                }
            }
            let level = voice
                .envelope
                .tick(!voice.released, false, false, envelope, rate);
            if voice.envelope.idle() {
                voice.active = false;
                continue;
            }
            let i = voice.position as usize;
            let next = if i + 1 < sample.len() {
                i + 1
            } else if looping {
                0
            } else {
                i
            };
            let fraction = voice.position - i as f64;
            for ch in 0..MAX_CHANNELS {
                let value = f64::from(sample[i][ch]) * (1. - fraction)
                    + f64::from(sample[next][ch]) * fraction;
                output[ch] += value * voice.velocity * level * amplitude;
            }
            if voice.root != root {
                voice.root = root;
                voice.increment = 2_f64.powf((f64::from(voice.pitch) - root) / 12.);
            }
            voice.position += voice.increment;
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn flat(release: f64) -> Settings {
        Settings {
            attack: 0.,
            decay: 0.,
            sustain: 1.,
            release,
            reset: false,
        }
    }
    #[test]
    fn independent_envelopes_overlap_and_release_only_the_matching_voice() {
        let mut sampler = Sampler::default();
        let sample = [[1.; MAX_CHANNELS]; 32];
        let shape = Settings {
            attack: 4.,
            decay: 2.,
            sustain: 0.5,
            release: 2.,
            reset: false,
        };
        let tick = |s: &mut Sampler| s.render(&sample, 60., 1., true, shape, 1000., true)[0];
        sampler.note(60, 127);
        assert_eq!(tick(&mut sampler), 0.25);
        assert_eq!(tick(&mut sampler), 0.5);
        sampler.note(64, 127);
        assert_eq!(tick(&mut sampler), 1.);
        sampler.note(60, 0);
        assert_eq!(tick(&mut sampler), 0.875);
        assert_eq!(tick(&mut sampler), 0.75);
        assert_eq!(tick(&mut sampler), 1.);
        assert_eq!(sampler.envelope_state(), (1., 1, 1));
        assert_eq!(tick(&mut sampler), 0.75);
        assert_eq!(tick(&mut sampler), 0.5);
        sampler.note(64, 0);
        assert_eq!(tick(&mut sampler), 0.25);
        assert_eq!(tick(&mut sampler), 0.);
        tick(&mut sampler);
        assert_eq!(sampler.envelope_state(), (0., 0, 0));
    }
    #[test]
    fn silent_sustain_and_slow_attacks_keep_voices_and_stage_times_latch() {
        let mut sampler = Sampler::default();
        let sample = [[1.; MAX_CHANNELS]; 8];
        let mut shape = Settings {
            attack: 4.,
            decay: 0.,
            sustain: 0.,
            release: 0.,
            reset: false,
        };
        sampler.note(60, 127);
        sampler.render(&sample, 60., 1., true, shape, 1000., true);
        shape.attack = 100.;
        sampler.note(60, 127);
        let frame = sampler.render(&sample, 60., 1., true, shape, 1000., true);
        assert!(
            (frame[0] - 0.51).abs() < 1e-9,
            "old attack keeps its timing, new note gets new timing"
        );
        for _ in 0..110 {
            sampler.render(&sample, 60., 1., true, shape, 1000., true);
        }
        assert_eq!(sampler.envelope_state(), (0., 2, 2));
        shape.sustain = 1.;
        let frame = sampler.render(&sample, 60., 1., true, shape, 1000., true);
        assert!(
            frame[0] > 0. && frame[0] < 2.,
            "sustain edits are smoothed for both silent held voices"
        );
        sampler.note(60, 0);
        sampler.render(&sample, 60., 1., true, shape, 1000., true);
        assert_eq!(
            sampler.envelope_state().1,
            1,
            "same pitch releases oldest held voice only"
        );
        sampler.note(60, 0);
        sampler.render(&sample, 60., 1., true, shape, 1000., true);
        assert_eq!(sampler.envelope_state(), (0., 0, 0));
    }
    #[test]
    fn zero_attack_and_sustain_do_not_reuse_held_slots_and_voice_stealing_starts_fresh() {
        let sample = [[1.; MAX_CHANNELS]; 8];
        let mut sampler = Sampler::default();
        let silent = Settings {
            attack: 0.,
            decay: 0.,
            sustain: 0.,
            release: 0.,
            reset: false,
        };
        for pitch in 0..64 {
            sampler.note(pitch, 127);
            sampler.render(&sample, 60., 1., true, silent, 1000., true);
        }
        assert_eq!(sampler.envelope_state(), (0., 64, 64));
        sampler.note(100, 127);
        let shape = Settings {
            attack: 4.,
            decay: 0.,
            sustain: 0.,
            release: 0.,
            reset: false,
        };
        assert_eq!(
            sampler.render(&sample, 60., 1., true, shape, 1000., true)[0],
            0.25
        );
        assert_eq!(sampler.voices[0].pitch, 100);
    }
    #[test]
    fn held_voice_retunes_immediately_when_root_changes() {
        let sample = vec![[1.; MAX_CHANNELS]; 1024];
        let mut sampler = Sampler::default();
        sampler.note(60, 100);
        for (root, position) in [
            (60., 1.),
            (60., 2.),
            (48., 4.),
            (72., 4.5),
            (60.5, 4.5 + 2_f64.powf(-0.5 / 12.)),
        ] {
            sampler.render(&sample, root, 1., true, flat(10.), 48000., true);
            assert!((sampler.voices[0].position - position).abs() < 1e-12);
        }
    }
    #[test]
    fn overlapping_pitches_velocity_release_and_pitch_ratio() {
        let sample = vec![[1.; MAX_CHANNELS]; 1024];
        let mut sampler = Sampler::default();
        sampler.note(60, 127);
        sampler.note(67, 64);
        let frame = sampler.render(&sample, 60., 1., true, flat(1.), 48000., true);
        assert!((frame[0] - (1. + 64. / 127.)).abs() < 1e-9);
        sampler.note(60, 0);
        for _ in 0..49 {
            sampler.render(&sample, 60., 1., true, flat(1.), 48000., true);
        }
        assert!(
            (sampler.render(&sample, 60., 1., true, flat(1.), 48000., true)[0] - 64. / 127.).abs()
                < 1e-9
        );
        sampler.note(67, 0);
        for _ in 0..49 {
            sampler.render(&sample, 60., 1., true, flat(1.), 48000., true);
        }
        assert_eq!(
            sampler.render(&sample, 60., 1., true, flat(1.), 48000., true),
            [0.; 8]
        );
        let ramp: Vec<_> = (0..20).map(|i| [i as f32; 8]).collect();
        let mut octave = Sampler::default();
        octave.note(72, 127);
        assert_eq!(
            octave.render(&ramp, 60., 1., false, flat(1.), 48000., true)[0],
            0.
        );
        assert_eq!(
            octave.render(&ramp, 60., 1., false, flat(1.), 48000., true)[0],
            2.
        );
    }
    #[test]
    fn same_pitch_releases_one_voice_and_pause_releases_rest() {
        let sample = vec![[1.; 8]; 8];
        let mut sampler = Sampler::default();
        sampler.note(60, 127);
        sampler.note(60, 127);
        sampler.render(&sample, 60., 1., true, flat(1.), 48000., true);
        sampler.note(60, 0);
        for _ in 0..50 {
            sampler.render(&sample, 60., 1., true, flat(1.), 48000., true);
        }
        assert_eq!(
            sampler.render(&sample, 60., 1., true, flat(1.), 48000., true)[0],
            1.
        );
        for _ in 0..50 {
            sampler.render(&sample, 60., 1., true, flat(1.), 48000., false);
        }
        assert_eq!(
            sampler.render(&sample, 60., 1., true, flat(1.), 48000., true),
            [0.; 8]
        );
    }
}
