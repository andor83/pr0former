use std::f64::consts::TAU;
fn poly_blep(t: f64, dt: f64) -> f64 {
    if dt <= 0. {
        return 0.;
    }
    if t < dt {
        let x = t / dt;
        2. * x - x * x - 1.
    } else if t > 1. - dt {
        let x = (t - 1.) / dt;
        x * x + 2. * x + 1.
    } else {
        0.
    }
}
pub fn wave(phase: f64, step: f64, shape: u32, noise: f64) -> f64 {
    let dt = step.clamp(0., 0.5);
    match shape {
        1 => 1. - 4. * (phase - 0.5).abs(),
        2 => 2. * phase - 1. - poly_blep(phase, dt),
        3 => {
            (if phase < 0.5 { 1. } else { -1. }) + poly_blep(phase, dt)
                - poly_blep((phase + 0.5).fract(), dt)
        }
        4 => noise,
        _ => (TAU * phase).sin(),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sine_has_no_extra_harmonics() {
        let samples: Vec<f64> = (0..4800)
            .map(|i| wave((i % 48) as f64 / 48., 1. / 48., 0, 0.))
            .collect();
        for harmonic in 2..20 {
            let power = (0..2)
                .map(|part| {
                    samples
                        .iter()
                        .enumerate()
                        .map(|(i, v)| {
                            let angle = TAU * i as f64 * harmonic as f64 / 48.;
                            v * if part == 0 { angle.sin() } else { angle.cos() }
                        })
                        .sum::<f64>()
                        .powi(2)
                })
                .sum::<f64>()
                .sqrt()
                / 2400.;
            assert!(power < 1e-10);
        }
    }
    #[test]
    fn waveforms_are_distinct_and_bounded() {
        assert_eq!(wave(0.25, 0.01, 0, 0.3), 1.);
        assert_eq!(wave(0.25, 0.01, 1, 0.3), 0.);
        assert_eq!(wave(0.25, 0.01, 2, 0.3), -0.5);
        assert_eq!(wave(0.25, 0.01, 3, 0.3), 1.);
        assert_eq!(wave(0.25, 0.01, 4, 0.3), 0.3);
        for shape in 0..5 {
            for i in 0..1000 {
                assert!(wave(i as f64 / 1000., 0.1, shape, 0.7).abs() <= 1.00001);
            }
        }
    }
}
