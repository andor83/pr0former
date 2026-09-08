use pr0_core::MAX_CHANNELS;
#[derive(Clone, Copy, Default)]
struct Voice {
    pitch: u8,
    velocity: f64,
    position: f64,
    level: f64,
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
    pub fn note(&mut self, pitch: u8, velocity: u8) {
        if velocity == 0 {
            if let Some(voice) = self
                .voices
                .iter_mut()
                .filter(|v| v.level > 0. && !v.released && v.pitch == pitch)
                .min_by_key(|v| v.serial)
            {
                voice.released = true;
            }
        } else {
            self.serial = self.serial.saturating_add(1);
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
                velocity: f64::from(velocity) / 127.,
                position: 0.,
                level: 1.,
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
        release_ms: f64,
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
            if voice.level <= 0. {
                continue;
            }
            if voice.position >= sample.len() as f64 {
                if looping {
                    voice.position %= sample.len() as f64;
                } else {
                    voice.level = 0.;
                    continue;
                }
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
                output[ch] += value * voice.velocity * voice.level * amplitude;
            }
            voice.position += 2_f64.powf((f64::from(voice.pitch) - root) / 12.);
            if voice.released {
                voice.level = (voice.level - 1. / (release_ms.max(1.) * rate / 1000.)).max(0.);
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn overlapping_pitches_velocity_release_and_pitch_ratio() {
        let sample = vec![[1.; MAX_CHANNELS]; 1024];
        let mut sampler = Sampler::default();
        sampler.note(60, 127);
        sampler.note(67, 64);
        let frame = sampler.render(&sample, 60., 1., true, 1., 48000., true);
        assert!((frame[0] - (1. + 64. / 127.)).abs() < 1e-9);
        sampler.note(60, 0);
        for _ in 0..49 {
            sampler.render(&sample, 60., 1., true, 1., 48000., true);
        }
        assert!(
            (sampler.render(&sample, 60., 1., true, 1., 48000., true)[0] - 64. / 127.).abs() < 1e-9
        );
        sampler.note(67, 0);
        for _ in 0..49 {
            sampler.render(&sample, 60., 1., true, 1., 48000., true);
        }
        assert_eq!(
            sampler.render(&sample, 60., 1., true, 1., 48000., true),
            [0.; 8]
        );
        let ramp: Vec<_> = (0..20).map(|i| [i as f32; 8]).collect();
        let mut octave = Sampler::default();
        octave.note(72, 127);
        assert_eq!(
            octave.render(&ramp, 60., 1., false, 1., 48000., true)[0],
            0.
        );
        assert_eq!(
            octave.render(&ramp, 60., 1., false, 1., 48000., true)[0],
            2.
        );
    }
    #[test]
    fn same_pitch_releases_one_voice_and_pause_releases_rest() {
        let sample = vec![[1.; 8]; 8];
        let mut sampler = Sampler::default();
        sampler.note(60, 127);
        sampler.note(60, 127);
        sampler.render(&sample, 60., 1., true, 1., 48000., true);
        sampler.note(60, 0);
        for _ in 0..50 {
            sampler.render(&sample, 60., 1., true, 1., 48000., true);
        }
        assert_eq!(
            sampler.render(&sample, 60., 1., true, 1., 48000., true)[0],
            1.
        );
        for _ in 0..50 {
            sampler.render(&sample, 60., 1., true, 1., 48000., false);
        }
        assert_eq!(
            sampler.render(&sample, 60., 1., true, 1., 48000., true),
            [0.; 8]
        );
    }
}
