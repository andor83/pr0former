//! Offline sample preparation: phase-vocoder time stretch followed by pitch resampling.
//! All allocations occur before the sample enters the live graph.
use crate::Fourier;
use rustfft::num_complex::Complex;
use std::f64::consts::{PI, TAU};

pub fn prepare(input: &[[f32; 8]], channels: usize, speed: f64, semitones: f64) -> Vec<[f32; 8]> {
    if input.is_empty() {
        return vec![];
    }
    let size = 1024;
    let hop = 256;
    let pitch = 2_f64.powf(semitones / 12.);
    let advance = hop as f64 * speed / pitch;
    let frames = (input.len() as f64 / advance).ceil() as usize;
    let stretched_len = frames * hop + size;
    let mut output = vec![[0_f32; 8]; stretched_len];
    let mut weight = vec![0_f32; stretched_len];
    let window: Vec<f32> = (0..size)
        .map(|i| (0.5 - 0.5 * (TAU * i as f64 / size as f64).cos()) as f32)
        .collect();
    let mut fft = Fourier::new(size).unwrap();
    let mut bins = vec![Complex::default(); size];
    let mut previous = vec![vec![0_f64; size / 2 + 1]; channels];
    let mut phase = previous.clone();
    for frame in 0..frames {
        let start = frame as f64 * advance;
        for ch in 0..channels {
            for i in 0..size {
                let position = start + i as f64;
                let index = position as usize;
                let fraction = (position - index as f64) as f32;
                let a = input.get(index).map(|f| f[ch]).unwrap_or(0.);
                let b = input.get(index + 1).map(|f| f[ch]).unwrap_or(0.);
                bins[i] = Complex::new((a + (b - a) * fraction) * window[i], 0.);
            }
            fft.forward(&mut bins);
            for k in 0..=size / 2 {
                let angle = bins[k].arg() as f64;
                let magnitude = bins[k].norm();
                let omega = TAU * k as f64 / size as f64;
                if frame == 0 {
                    phase[ch][k] = angle;
                } else {
                    let delta =
                        (angle - previous[ch][k] - omega * advance + PI).rem_euclid(TAU) - PI;
                    phase[ch][k] += (omega + delta / advance) * hop as f64;
                }
                previous[ch][k] = angle;
                bins[k] = Complex::from_polar(magnitude, phase[ch][k] as f32);
                if k > 0 && k < size / 2 {
                    bins[size - k] = bins[k].conj();
                }
            }
            fft.inverse(&mut bins);
            for i in 0..size {
                output[frame * hop + i][ch] += bins[i].re * window[i];
            }
        }
        for i in 0..size {
            weight[frame * hop + i] += window[i] * window[i];
        }
    }
    for (i, frame) in output.iter_mut().enumerate() {
        if weight[i] > 0.01 {
            for x in frame.iter_mut().take(channels) {
                *x /= weight[i];
            }
        }
    }
    let length = (input.len() as f64 / speed).round() as usize;
    (0..length)
        .map(|i| {
            let position = i as f64 * pitch;
            let index = position as usize;
            let fraction = (position - index as f64) as f32;
            let mut result = [0.; 8];
            for (ch, out) in result.iter_mut().enumerate().take(channels) {
                let a = output.get(index).map(|f| f[ch]).unwrap_or(0.);
                let b = output.get(index + 1).map(|f| f[ch]).unwrap_or(0.);
                *out = a + (b - a) * fraction;
            }
            result
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn time_stretch_preserves_tone() {
        let input: Vec<_> = (0..24000)
            .map(|i| {
                let mut f = [0.; 8];
                f[0] = (TAU * 440. * i as f64 / 48000.).sin() as f32;
                f
            })
            .collect();
        let output = prepare(&input, 1, 2., 0.);
        assert_eq!(output.len(), 12000);
        let crossings = output[3000..10000]
            .windows(2)
            .filter(|w| w[0][0] <= 0. && w[1][0] > 0.)
            .count();
        let hz = crossings as f64 * 48000. / 7000.;
        assert!((hz - 440.).abs() < 10., "{hz}");
        assert!(output.iter().flatten().all(|v| v.is_finite()));
    }
}
