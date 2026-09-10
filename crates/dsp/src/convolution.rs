//! Continuous short-frame convolution with 50% overlapped input grains.
use crate::Fourier;
use rustfft::num_complex::Complex;
pub struct Convolution {
    size: usize,
    channels: usize,
    history_a: Vec<[f32; 8]>,
    history_b: Vec<[f32; 8]>,
    window: Vec<f32>,
    a: Vec<Complex<f32>>,
    b: Vec<Complex<f32>>,
    fft: Fourier,
    overlap: Vec<[f64; 8]>,
    cursor: usize,
    read: usize,
    samples: usize,
}
impl Convolution {
    pub fn new(size: usize, channels: usize) -> Self {
        Self {
            size,
            channels,
            history_a: vec![[0.; 8]; size],
            history_b: vec![[0.; 8]; size],
            window: (0..size)
                .map(|i| {
                    (0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / size as f64).cos()) as f32
                })
                .collect(),
            a: vec![Complex::default(); size * 2],
            b: vec![Complex::default(); size * 2],
            fft: Fourier::new(size * 2).expect("validated convolution size"),
            overlap: vec![[0.; 8]; size * 2 + 1],
            cursor: 0,
            read: 0,
            samples: 0,
        }
    }
    pub fn tick(&mut self, a: [f64; 8], b: [f64; 8], mix: f64, normalize: bool) -> [f64; 8] {
        let dry = self.history_a[self.cursor];
        self.history_a[self.cursor] = a.map(|x| x as f32);
        self.history_b[self.cursor] = b.map(|x| x as f32);
        let wet = self.overlap[self.read];
        self.overlap[self.read] = [0.; 8];
        self.cursor = (self.cursor + 1) % self.size;
        self.samples = self.samples.saturating_add(1);
        if self.samples >= self.size && self.samples % (self.size / 2) == 0 {
            for ch in 0..self.channels {
                self.a.fill(Complex::default());
                self.b.fill(Complex::default());
                let mut norm = 0.;
                for i in 0..self.size {
                    let source = (self.cursor + i) % self.size;
                    self.a[i].re = self.history_a[source][ch] * self.window[i];
                    self.b[i].re = self.history_b[source][ch];
                    norm += self.b[i].re.abs() as f64;
                }
                self.fft.forward(&mut self.a);
                self.fft.forward(&mut self.b);
                let scale = if normalize {
                    if norm > 1e-12 { 1. / norm } else { 0. }
                } else {
                    1.
                };
                for i in 0..self.a.len() {
                    self.a[i] *= self.b[i] * scale as f32;
                }
                self.fft.inverse(&mut self.a);
                for i in 0..self.size * 2 {
                    let target = (self.read + 1 + i) % self.overlap.len();
                    self.overlap[target][ch] += self.a[i].re as f64;
                }
            }
        }
        self.read = (self.read + 1) % self.overlap.len();
        std::array::from_fn(|ch| {
            if ch < self.channels {
                dry[ch] as f64 * (1. - mix) + wet[ch] * mix
            } else {
                0.
            }
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn streaming_fft_matches_direct_overlapped_convolution_and_dry_latency() {
        let size = 128;
        let frames = 768;
        let a: Vec<[f64; 8]> = (0..frames)
            .map(|i| {
                std::array::from_fn(|ch| {
                    if ch < 2 && i < 400 {
                        ((i * (ch + 1)) as f64 * 0.13).sin() * 0.2
                    } else {
                        0.
                    }
                })
            })
            .collect();
        let b: Vec<[f64; 8]> = (0..frames)
            .map(|i| {
                std::array::from_fn(|ch| {
                    if ch < 2 && i < 350 {
                        ((i + ch * 7) as f64 * 0.21).cos() * 0.1
                    } else {
                        0.
                    }
                })
            })
            .collect();
        for normalize in [false, true] {
            let mut reference = vec![[0.; 8]; frames + size * 2];
            for start in (0..frames - size + 1).step_by(size / 2) {
                for ch in 0..2 {
                    let norm: f64 = b[start..start + size].iter().map(|x| x[ch].abs()).sum();
                    let scale = if normalize {
                        if norm > 1e-12 { 1. / norm } else { 0. }
                    } else {
                        1.
                    };
                    for i in 0..size {
                        let window =
                            0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / size as f64).cos();
                        for j in 0..size {
                            reference[start + size + i + j][ch] +=
                                a[start + i][ch] * window * b[start + j][ch] * scale;
                        }
                    }
                }
            }
            let mut effect = Convolution::new(size, 2);
            for i in 0..frames {
                let out = effect.tick(a[i], b[i], 1., normalize);
                for ch in 0..8 {
                    assert!(
                        (out[ch] - reference[i][ch]).abs() < 1e-5,
                        "frame {i} channel {ch}"
                    );
                }
            }
        }
        let mut dry = Convolution::new(size, 2);
        for i in 0..frames {
            let out = dry.tick(a[i], b[i], 0., true);
            for ch in 0..2 {
                let expected = if i >= size { a[i - size][ch] } else { 0. };
                assert!((out[ch] - expected).abs() < 1e-7);
            }
        }
    }
}
