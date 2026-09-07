//! Reusable source-plugin examples used by the built-in effects.
use std::f64::consts::TAU;

#[derive(Clone, Copy, Default)]
pub struct Section {
    b: [f64; 3],
    a: [f64; 2],
    z: [f64; 2],
}
impl Section {
    pub fn bandpass(&mut self, hz: f64, q: f64, sr: f64) {
        let w = TAU * hz.clamp(10., sr * 0.45) / sr;
        let alpha = w.sin() / (2. * q);
        let a0 = 1. + alpha;
        self.b = [alpha / a0, 0., -alpha / a0];
        self.a = [-2. * w.cos() / a0, (1. - alpha) / a0];
    }
    pub fn peak(&mut self, hz: f64, q: f64, db: f64, sr: f64) {
        let w = TAU * hz.clamp(10., sr * 0.45) / sr;
        let alpha = w.sin() / (2. * q);
        let gain = 10_f64.powf(db / 40.);
        let a0 = 1. + alpha / gain;
        self.b = [
            (1. + alpha * gain) / a0,
            -2. * w.cos() / a0,
            (1. - alpha * gain) / a0,
        ];
        self.a = [-2. * w.cos() / a0, (1. - alpha / gain) / a0];
    }
    pub fn notch(&mut self, hz: f64, q: f64, sr: f64) {
        let w = TAU * hz.clamp(10., sr * 0.45) / sr;
        let alpha = w.sin() / (2. * q);
        let a0 = 1. + alpha;
        self.b = [1. / a0, -2. * w.cos() / a0, 1. / a0];
        self.a = [-2. * w.cos() / a0, (1. - alpha) / a0];
    }
    pub fn tick(&mut self, x: f64) -> f64 {
        let y = self.b[0] * x + self.z[0];
        self.z[0] = self.b[1] * x - self.a[0] * y + self.z[1];
        self.z[1] = self.b[2] * x - self.a[1] * y;
        y
    }
}
pub struct Vocoder {
    carrier: [[Section; 16]; 8],
    modulator: [[Section; 16]; 8],
    envelope: [[f64; 16]; 8],
    sr: f64,
}
impl Vocoder {
    pub fn new(sr: f64) -> Self {
        let mut result = Self {
            carrier: [[Section::default(); 16]; 8],
            modulator: [[Section::default(); 16]; 8],
            envelope: [[0.; 16]; 8],
            sr,
        };
        for band in 0..16 {
            let hz = 100. * 80_f64.powf(band as f64 / 15.);
            for ch in 0..8 {
                result.carrier[ch][band].bandpass(hz, 3., sr);
                result.modulator[ch][band].bandpass(hz, 3., sr);
            }
        }
        result
    }
    pub fn tick(
        &mut self,
        carrier: [f64; 8],
        modulator: [f64; 8],
        channels: usize,
        attack: f64,
        release: f64,
    ) -> [f64; 8] {
        let mut out = [0.; 8];
        for ch in 0..channels {
            for band in 0..16 {
                let detector = self.modulator[ch][band].tick(modulator[ch]).abs();
                let time = if detector > self.envelope[ch][band] {
                    attack
                } else {
                    release
                };
                let c = (-1. / (0.001 * time.max(0.1) * self.sr)).exp();
                self.envelope[ch][band] = detector + (self.envelope[ch][band] - detector) * c;
                out[ch] += self.carrier[ch][band].tick(carrier[ch]) * self.envelope[ch][band] * 6.;
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn flat_eq_is_unity() {
        let mut s = Section::default();
        s.peak(1000., 1., 0., 48000.);
        for i in 0..1000 {
            let x = (i as f64 * 0.1).sin();
            assert!((s.tick(x) - x).abs() < 1e-10);
        }
    }
    #[test]
    fn silent_modulator_silences_vocoder() {
        let mut v = Vocoder::new(48000.);
        for _ in 0..1000 {
            assert_eq!(v.tick([0.5; 8], [0.; 8], 8, 10., 100.), [0.; 8]);
        }
    }
}
