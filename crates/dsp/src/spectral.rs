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
