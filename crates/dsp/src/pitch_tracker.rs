//! Bounded, windowed FFT peak tracker. All storage and FFT scratch are prepared.
use crate::Fourier;
use rustfft::num_complex::Complex;
#[derive(Clone, Copy, Default)]
struct Peak {
    hz: f64,
    amplitude: f64,
    score: f64,
    suppressed: bool,
}
pub struct PitchTracker {
    fft: Fourier,
    history: Vec<f32>,
    window: Vec<f32>,
    bins: Vec<Complex<f32>>,
    magnitudes: Vec<f64>,
    cursor: usize,
    samples: usize,
    pub notes: [f64; 4],
    pub strengths: [f64; 4],
}
impl PitchTracker {
    pub fn new(size: usize) -> Self {
        Self {
            fft: Fourier::new(size).expect("validated tracker FFT size"),
            history: vec![0.; size],
            window: (0..size)
                .map(|i| {
                    (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / size as f64).cos()) as f32
                })
                .collect(),
            bins: vec![Complex::default(); size],
            magnitudes: vec![0.; size / 2 + 1],
            cursor: 0,
            samples: 0,
            notes: [-1.; 4],
            strengths: [0.; 4],
        }
    }
    pub fn tick(&mut self, audio: &[f64], sample_rate: f64, slots: usize, threshold_db: f64) {
        let size = self.history.len();
        self.history[self.cursor] = (audio.iter().copied().filter(|v| v.is_finite()).sum::<f64>()
            / audio.len().max(1) as f64) as f32;
        self.cursor = (self.cursor + 1) % size;
        self.samples = self.samples.saturating_add(1);
        if self.samples < size || self.samples % (size / 4) != 0 {
            return;
        }
        self.notes = [-1.; 4];
        self.strengths = [0.; 4];
        for i in 0..size {
            self.bins[i] =
                Complex::new(self.history[(self.cursor + i) % size] * self.window[i], 0.);
        }
        self.fft.forward(&mut self.bins);
        for (i, magnitude) in self.magnitudes.iter_mut().enumerate() {
            *magnitude = self.bins[i].norm() as f64 * 4. / size as f64;
        }
        let threshold = 10f64.powf(threshold_db / 20.);
        let strongest = self.magnitudes.iter().copied().fold(0f64, f64::max);
        let floor = threshold.max(strongest * 0.06);
        let mut peaks = [Peak::default(); 64];
        let mut count = 0;
        for bin in 2..size / 2 {
            let a = self.magnitudes[bin];
            if a < floor || a <= self.magnitudes[bin - 1] || a < self.magnitudes[bin + 1] {
                continue;
            }
            let left = self.magnitudes[bin - 1].max(1e-20).ln();
            let middle = a.max(1e-20).ln();
            let right = self.magnitudes[bin + 1].max(1e-20).ln();
            let denominator = left - 2. * middle + right;
            let offset = if denominator.abs() > 1e-12 {
                (0.5 * (left - right) / denominator).clamp(-0.5, 0.5)
            } else {
                0.
            };
            let hz = (bin as f64 + offset) * sample_rate / size as f64;
            let midi = 69. + 12. * (hz / 440.).log2();
            if !(0. ..=127.).contains(&midi) {
                continue;
            }
            let peak = Peak {
                hz,
                amplitude: a,
                score: a,
                suppressed: false,
            };
            let mut index = count.min(63);
            if count == 64 && a <= peaks[63].amplitude {
                continue;
            }
            while index > 0 && peaks[index - 1].amplitude < a {
                peaks[index] = peaks[index - 1];
                index -= 1
            }
            peaks[index] = peak;
            count = (count + 1).min(64);
        }
        // Group strong integer harmonics under an observed lower fundamental.
        // Octave-doubled instruments and missing fundamentals remain ambiguous.
        for high in 0..count {
            let mut fundamental = None;
            for low in 0..count {
                let ratio = peaks[high].hz / peaks[low].hz;
                let harmonic = ratio.round();
                if (2. ..=6.).contains(&harmonic)
                    && (ratio / harmonic - 1.).abs() < 0.015
                    && peaks[low].amplitude >= peaks[high].amplitude * 0.15
                    && fundamental.is_none_or(|old: usize| peaks[low].hz < peaks[old].hz)
                {
                    fundamental = Some(low)
                }
            }
            if let Some(low) = fundamental {
                peaks[high].suppressed = true;
                peaks[low].score += peaks[high].amplitude * 0.5;
            }
        }
        for slot in 0..slots.min(4) {
            let mut best = None;
            for i in 0..count {
                let note = (69. + 12. * (peaks[i].hz / 440.).log2()).round();
                if !peaks[i].suppressed
                    && !self.notes.contains(&note)
                    && best.is_none_or(|old: usize| peaks[i].score > peaks[old].score)
                {
                    best = Some(i)
                }
            }
            if let Some(i) = best {
                self.notes[slot] = (69. + 12. * (peaks[i].hz / 440.).log2()).round();
                self.strengths[slot] = peaks[i].score;
                peaks[i].suppressed = true;
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn tones(
        tracker: &mut PitchTracker,
        rate: f64,
        tones: &[(f64, f64)],
        channels: usize,
        cancel: bool,
    ) {
        for i in 0..16384 {
            let x = tones
                .iter()
                .map(|(hz, a)| a * (std::f64::consts::TAU * hz * i as f64 / rate).sin())
                .sum::<f64>();
            let mut audio = [x; 8];
            if cancel {
                audio[1] = -x;
            }
            tracker.tick(&audio[..channels], rate, 4, -55.);
        }
    }
    #[test]
    fn ranks_four_tones_and_clears_silence_across_sample_rates() {
        for rate in [44100., 48000., 96000.] {
            let mut tracker = PitchTracker::new(8192);
            tones(
                &mut tracker,
                rate,
                &[
                    (440., 0.8),
                    (329.6276, 0.6),
                    (261.6256, 0.4),
                    (391.9954, 0.2),
                ],
                8,
                false,
            );
            assert_eq!(tracker.notes, [69., 64., 60., 67.]);
            assert!(tracker.strengths.windows(2).all(|w| w[0] >= w[1]));
            tones(&mut tracker, rate, &[], 1, false);
            assert_eq!(tracker.notes, [-1.; 4]);
        }
    }
    #[test]
    fn mono_mix_cancels_opposite_channels_and_groups_harmonics() {
        let mut tracker = PitchTracker::new(8192);
        tones(
            &mut tracker,
            48000.,
            &[(220., 0.3), (440., 0.6), (660., 0.2)],
            1,
            false,
        );
        assert_eq!(tracker.notes, [57., -1., -1., -1.]);
        tones(&mut tracker, 48000., &[(440., 0.8)], 2, true);
        assert_eq!(tracker.notes, [-1.; 4]);
        tones(&mut tracker, 48000., &[(440., 0.00001)], 1, false);
        assert_eq!(tracker.notes, [-1.; 4]);
    }
}
