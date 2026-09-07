//! Streaming windowed FFT/IFFT with bounded storage and explicit frame generations.
use crate::Fourier;
use rustfft::num_complex::Complex;
use std::f32::consts::TAU;

pub struct Spectral {
    pub bins: Vec<Vec<Complex<f32>>>,
    pub generation: u64,
    pub polar: bool,
    size: usize,
    hop: usize,
    channels: usize,
    cursor: usize,
    samples: u64,
    fresh: bool,
    fft: Fourier,
    history: Vec<Vec<f32>>,
    overlap: Vec<Vec<f32>>,
    window: Vec<f32>,
    normalization: Vec<f32>,
}
impl Spectral {
    pub fn new(size: usize, overlap: usize, channels: usize) -> Self {
        let hop = size / overlap;
        let window: Vec<f32> = (0..size)
            .map(|i| 0.5 - 0.5 * (TAU * i as f32 / size as f32).cos())
            .collect();
        let normalization = (0..size)
            .map(|i| {
                (0..overlap)
                    .map(|j| window[(i + j * hop) % size].powi(2))
                    .sum::<f32>()
                    .max(1e-6)
            })
            .collect();
        Self {
            bins: vec![vec![Complex::default(); size]; channels],
            generation: 0,
            polar: false,
            size,
            hop,
            channels,
            cursor: 0,
            samples: 0,
            fresh: false,
            fft: Fourier::new(size).expect("validated FFT size"),
            history: vec![vec![0.; size]; channels],
            overlap: vec![vec![0.; size * 2]; channels],
            window,
            normalization,
        }
    }
    pub fn copy_from(&mut self, source: &Self) {
        if source.generation != self.generation
            && source.size == self.size
            && source.channels == self.channels
        {
            for (ch, input) in source.bins.iter().enumerate() {
                self.bins[ch].copy_from_slice(input);
            }
            self.generation = source.generation;
            self.polar = source.polar;
            self.fresh = true;
        }
    }
    pub fn forward(&mut self, input: [f64; 8]) {
        for (ch, x) in input.iter().enumerate().take(self.channels) {
            self.history[ch][self.cursor] = *x as f32;
        }
        self.cursor = (self.cursor + 1) % self.size;
        self.samples += 1;
        if self.samples >= self.size as u64 && self.samples % self.hop as u64 == 0 {
            for ch in 0..self.channels {
                for i in 0..self.size {
                    self.bins[ch][i] = Complex::new(
                        self.history[ch][(self.cursor + i) % self.size] * self.window[i],
                        0.,
                    );
                }
                self.fft.forward(&mut self.bins[ch]);
            }
            self.generation += 1;
            self.polar = false;
        }
    }
    pub fn inverse(&mut self) -> [f64; 8] {
        if self.fresh {
            for ch in 0..self.channels {
                if self.polar {
                    for bin in &mut self.bins[ch] {
                        *bin = Complex::from_polar(bin.re, bin.im);
                    }
                }
                self.fft.inverse(&mut self.bins[ch]);
                for i in 0..self.size {
                    let pos = (self.cursor + 1 + i) % (self.size * 2);
                    self.overlap[ch][pos] +=
                        self.bins[ch][i].re * self.window[i] / self.normalization[i];
                }
            }
            self.fresh = false;
        }
        let mut output = [0.; 8];
        for (ch, out) in output.iter_mut().enumerate().take(self.channels) {
            *out = self.overlap[ch][self.cursor] as f64;
            self.overlap[ch][self.cursor] = 0.;
        }
        self.cursor = (self.cursor + 1) % (self.size * 2);
        output
    }
    /// Apply once to each fresh frame. Work in magnitude/phase and mirror the
    /// negative half so inverse transforms still represent real audio.
    pub fn map_bins(&mut self, factors: [f64; 4], curves: Option<(&[f64], &[f64])>) {
        if !self.fresh {
            return;
        }
        let half = self.size / 2;
        for channel in &mut self.bins {
            for i in 0..=half {
                let bin = channel[i];
                let (magnitude, phase) = if self.polar {
                    (bin.re, bin.im)
                } else {
                    (bin.norm(), bin.arg())
                };
                let (gain, shift) = curves
                    .map(|(m, p)| {
                        let x = i as f64 * 32. / half as f64;
                        let left = (x.floor() as usize).min(31);
                        let fraction = x - left as f64;
                        (
                            m[left] + (m[left + 1] - m[left]) * fraction,
                            p[left] + (p[left + 1] - p[left]) * fraction,
                        )
                    })
                    .unwrap_or((1., 0.));
                let magnitude =
                    ((magnitude as f64 * factors[0] + factors[1]).max(0.) * gain) as f32;
                let phase = if i == 0 || i == half {
                    phase
                } else {
                    ((phase as f64 * factors[2] + factors[3] + shift + std::f64::consts::PI)
                        .rem_euclid(std::f64::consts::TAU)
                        - std::f64::consts::PI) as f32
                };
                channel[i] = if self.polar {
                    Complex::new(magnitude, phase)
                } else {
                    Complex::from_polar(magnitude, phase)
                };
                if i > 0 && i < half {
                    channel[self.size - i] = if self.polar {
                        Complex::new(magnitude, -phase)
                    } else {
                        channel[i].conj()
                    };
                } else if !self.polar {
                    channel[i].im = 0.;
                }
            }
        }
        self.fresh = false;
    }

    pub fn transform(&mut self, kind: &str, gain: f64) {
        if !self.fresh {
            return;
        }
        for channel in &mut self.bins {
            for bin in channel {
                match kind {
                    "spectral_gain" => {
                        if self.polar {
                            bin.re *= gain as f32
                        } else {
                            *bin *= gain as f32
                        }
                    }
                    "to_polar" => {
                        if !self.polar {
                            *bin = Complex::new(bin.norm(), bin.arg())
                        }
                    }
                    "to_cartesian" => {
                        if self.polar {
                            *bin = Complex::from_polar(bin.re, bin.im)
                        }
                    }
                    _ => {}
                }
            }
        }
        if kind == "to_polar" {
            self.polar = true;
        }
        if kind == "to_cartesian" {
            self.polar = false;
        }
        self.fresh = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spectral_math_and_curves_preserve_frame_contracts() {
        for polar in [false, true] {
            for channels in 1..=8 {
                for size in [256, 1024, 8192] {
                    let mut source = Spectral::new(size, 4, channels);
                    source.generation = 7;
                    source.polar = polar;
                    for channel in &mut source.bins {
                        for i in 0..=size / 2 {
                            let value = if polar {
                                Complex::new(2., 0.3)
                            } else {
                                Complex::from_polar(2., 0.3)
                            };
                            channel[i] = value;
                            if i > 0 && i < size / 2 {
                                channel[size - i] = if polar {
                                    Complex::new(2., -0.3)
                                } else {
                                    value.conj()
                                };
                            }
                        }
                        channel[0] = Complex::new(2., 0.);
                        channel[size / 2] = Complex::new(2., 0.);
                    }
                    let mut mapped = Spectral::new(size, 4, channels);
                    mapped.copy_from(&source);
                    mapped.map_bins([2., 1., 2., 0.1], None);
                    let bin = mapped.bins[0][size / 4];
                    let expected = if polar {
                        Complex::new(5., 0.7)
                    } else {
                        Complex::from_polar(5., 0.7)
                    };
                    assert!((bin - expected).norm() < 1e-5);
                    let saved = mapped.bins.clone();
                    mapped.map_bins([10., 0., 1., 0.], None);
                    assert_eq!(mapped.bins, saved, "must process each generation only once");
                    assert_eq!(mapped.generation, 7);
                    assert_eq!(mapped.polar, polar);
                    let mut curved = Spectral::new(size, 4, channels);
                    curved.copy_from(&source);
                    let magnitude: Vec<f64> = (0..33).map(|i| i as f64 / 32.).collect();
                    curved.map_bins([1., 0., 1., 0.], Some((&magnitude, &[0.2; 33])));
                    for channel in &curved.bins {
                        for i in 1..size / 2 {
                            let value = channel[i];
                            let mag = if polar { value.re } else { value.norm() };
                            assert!((mag - 2. * i as f32 / (size / 2) as f32).abs() < 1e-5);
                            let mirror = if polar {
                                Complex::new(value.re, -value.im)
                            } else {
                                value.conj()
                            };
                            assert_eq!(channel[size - i], mirror);
                        }
                        assert_eq!(channel[0].im, 0.);
                        assert_eq!(channel[size / 2].im, 0.);
                    }
                    assert_eq!(curved.generation, source.generation);
                    assert_eq!(curved.polar, polar);
                }
            }
        }
    }
    #[test]
    fn streaming_fft_reconstructs_with_reported_delay() {
        for size in [256, 1024, 8192] {
            let mut forward = Spectral::new(size, 4, 2);
            let mut inverse = Spectral::new(size, 4, 2);
            for sample in 0..size * 5 {
                let x = (sample as f64 * 0.071).sin();
                forward.forward([x; 8]);
                inverse.copy_from(&forward);
                let y = inverse.inverse();
                if sample > size * 3 {
                    let expected = ((sample - size) as f64 * 0.071).sin();
                    assert!(
                        (y[0] - expected).abs() < 1e-5,
                        "size={size} sample={sample}: {} vs {expected}",
                        y[0]
                    );
                    assert_eq!(y[0], y[1]);
                }
            }
        }
    }
}
