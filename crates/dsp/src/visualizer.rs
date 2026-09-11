//! Prepared analysis state. Capture is allocation-free; FFT analysis and JSON happen outside render.
use crate::Fourier;
use pr0_core::{ControlValue, MAX_CONTROL_TEXT_BYTES};
use pr0_core::{SpectrumChannel, Visualization};
use rustfft::num_complex::Complex;
#[derive(Clone, Copy, PartialEq)]
pub struct Text {
    bytes: [u8; MAX_CONTROL_TEXT_BYTES],
    len: usize,
}
impl Text {
    /// Callers validate length upstream; anything longer is truncated at a
    /// character boundary rather than panicking on the audio worker.
    pub(crate) fn new(text: &str) -> Self {
        debug_assert!(text.len() <= MAX_CONTROL_TEXT_BYTES);
        let mut len = text.len().min(MAX_CONTROL_TEXT_BYTES);
        while !text.is_char_boundary(len) {
            len -= 1;
        }
        let mut value = Self {
            bytes: [0; MAX_CONTROL_TEXT_BYTES],
            len,
        };
        value.bytes[..len].copy_from_slice(&text.as_bytes()[..len]);
        value
    }
    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..self.len]).expect("prepared UTF-8")
    }
}
#[derive(Clone, Copy, PartialEq)]
pub enum Datum {
    Number(f64),
    Text(Text),
}
impl Datum {
    pub fn prepare(value: Option<&ControlValue>) -> Self {
        match value {
            Some(ControlValue::Text(text)) => Self::Text(Text::new(text)),
            Some(ControlValue::Number(v)) => Self::Number(*v),
            None => Self::Number(0.),
        }
    }
}
pub struct Analyzer {
    size: usize,
    history: Vec<Vec<f32>>,
    cursor: usize,
    samples: u64,
    pub sequence: u64,
    window: Vec<f32>,
    bins: Vec<Vec<Complex<f32>>>,
    fft: Fourier,
    colors: Vec<Vec<u8>>,
    color_cursor: usize,
    columns: usize,
}
impl Analyzer {
    pub fn new(size: usize, channels: usize) -> Self {
        Self {
            size,
            history: vec![vec![0.; size]; channels],
            cursor: 0,
            samples: 0,
            sequence: 0,
            window: (0..size)
                .map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / size as f32).cos())
                .collect(),
            bins: vec![vec![Complex::default(); size]; channels],
            fft: Fourier::new(size).expect("validated analysis size"),
            colors: vec![vec![0; 128 * 32]; channels],
            color_cursor: 0,
            columns: 0,
        }
    }
    pub fn capture(&mut self, frame: &[f64; 8]) {
        for (ch, row) in self.history.iter_mut().enumerate() {
            row[self.cursor] = frame[ch] as f32;
        }
        self.cursor = (self.cursor + 1) % self.size;
        self.samples += 1;
    }
    pub fn analyze(&mut self) {
        for ch in 0..self.history.len() {
            for i in 0..self.size {
                self.bins[ch][i] = Complex::new(
                    self.history[ch][(self.cursor + i) % self.size] * self.window[i],
                    0.,
                );
            }
            self.fft.forward(&mut self.bins[ch]);
        }
        self.record(false);
        self.sequence += 1;
    }
    fn record(&mut self, polar: bool) {
        for ch in 0..self.bins.len() {
            let positive = self.size / 2 + 1;
            for band in 0..32 {
                let start = band * positive / 32;
                let end = ((band + 1) * positive / 32).max(start + 1);
                let peak = self.bins[ch][start..end]
                    .iter()
                    .map(|b| if polar { b.re.abs() } else { b.norm() })
                    .fold(0_f32, f32::max)
                    * 4.
                    / self.size as f32;
                let db = 20. * peak.max(1e-6).log10();
                self.colors[ch][self.color_cursor * 32 + band] =
                    ((db + 100.).clamp(0., 100.) * 2.55).round() as u8;
            }
        }
        self.color_cursor = (self.color_cursor + 1) % 128;
        self.columns = (self.columns + 1).min(128);
    }
    pub fn spectral_block(&mut self, bins: &[Vec<Complex<f32>>], polar: bool) {
        for (to, from) in self.bins.iter_mut().zip(bins) {
            to.copy_from_slice(from);
        }
        self.record(polar);
        self.sequence += 1;
    }
    pub fn history(&self) -> Vec<String> {
        const HEX: &[u8] = b"0123456789abcdef";
        self.colors
            .iter()
            .map(|colors| {
                let mut text = String::with_capacity(self.columns * 64);
                let start = if self.columns == 128 {
                    self.color_cursor
                } else {
                    0
                };
                for col in 0..self.columns {
                    for value in &colors[((start + col) % 128) * 32..((start + col) % 128 + 1) * 32]
                    {
                        text.push(HEX[(value >> 4) as usize] as char);
                        text.push(HEX[(value & 15) as usize] as char);
                    }
                }
                text
            })
            .collect()
    }
    pub fn columns(&self) -> usize {
        self.columns
    }
    pub fn snapshot(&self) -> Visualization {
        Visualization::Audio {
            sequence: self.sequence,
            size: self.size,
            ready: self.samples >= self.size as u64,
            channels: spectrum(&self.bins, false, 4. / self.size as f32),
            history: self.history(),
            columns: self.columns,
        }
    }
}
pub fn spectrum(channels: &[Vec<Complex<f32>>], polar: bool, scale: f32) -> Vec<SpectrumChannel> {
    channels
        .iter()
        .map(|bins| {
            let positive = &bins[..bins.len() / 2 + 1];
            let magnitude = positive
                .iter()
                .map(|b| {
                    if polar {
                        b.re.abs() * scale
                    } else {
                        b.norm() * scale
                    }
                })
                .map(|v| if v.is_finite() { v } else { 0. })
                .collect();
            let phase = positive
                .iter()
                .map(|b| if polar { b.im } else { b.arg() })
                .map(|v| if v.is_finite() { v } else { 0. })
                .collect();
            SpectrumChannel { magnitude, phase }
        })
        .collect()
}
