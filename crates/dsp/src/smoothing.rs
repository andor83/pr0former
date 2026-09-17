//! Shaped control smoothing: a move from the value the node is holding to the
//! value now at its input. Easing curves are defined over the progress of one
//! move, so they land exactly on the target; the two chase curves only approach
//! it, which is what the snap percentage is for.

#[derive(Clone, Copy, PartialEq)]
pub enum Curve {
    Linear,
    EaseIn,
    EaseOut,
    EaseInOut,
    Exponential,
    Spring,
}
impl Curve {
    pub fn from_index(index: f64) -> Self {
        match index.round() as i64 {
            0 => Self::Linear,
            1 => Self::EaseIn,
            2 => Self::EaseOut,
            4 => Self::Exponential,
            5 => Self::Spring,
            _ => Self::EaseInOut,
        }
    }
    /// Fraction of the move completed at `progress`. Every curve runs 0 to 1.
    fn shaped(self, progress: f64) -> f64 {
        match self {
            Self::EaseIn => progress * progress,
            Self::EaseOut => 1. - (1. - progress) * (1. - progress),
            Self::EaseInOut => progress * progress * (3. - 2. * progress),
            _ => progress,
        }
    }
}
#[derive(Clone, Copy)]
pub struct Smoothing {
    value: f64,
    progress: f64,
    /// Distance latched when this move began; zero means settled.
    distance: f64,
    velocity: f64,
}
impl Default for Smoothing {
    fn default() -> Self {
        Self { value: f64::NAN, progress: 1., distance: 0., velocity: 0. }
    }
}
impl Smoothing {
    /// `time` is milliseconds, `snap` a percentage of the distance the current
    /// move started with. A move begins when a settled value meets a new target;
    /// a target that moves mid-flight only changes where the move is heading, so
    /// the output never jumps and a shaped move still lands on schedule.
    pub fn tick(&mut self, target: f64, curve: Curve, time: f64, snap: f64, sample_rate: f64) -> f64 {
        // Opening a patch at a value must not sweep up to it from zero.
        if !self.value.is_finite() {
            self.value = target;
            return self.value;
        }
        if self.distance == 0. {
            if self.value == target {
                return self.value;
            }
            self.distance = (target - self.value).abs();
            self.progress = 0.;
        }
        let samples = time * 0.001 * sample_rate;
        if samples <= 1. {
            self.settle(target);
            return self.value;
        }
        match curve {
            // The classic one-pole: `time` is its time constant, 63% of the way.
            Curve::Exponential => self.value += (target - self.value) * (1. - (-1. / samples).exp()),
            Curve::Spring => {
                // Critically damped, so there is no overshoot and velocity stays
                // continuous when the target moves in the middle of a move.
                let dt = 1. / sample_rate;
                let omega = 2. / (time * 0.001);
                let x = omega * dt;
                let decay = 1. / (1. + x + 0.48 * x * x + 0.235 * x * x * x);
                let change = self.value - target;
                let step = (self.velocity + omega * change) * dt;
                self.velocity = (self.velocity - omega * step) * decay;
                self.value = target + (change + step) * decay;
            }
            shaped => {
                // Each sample covers the shaped share of what is left, which is
                // the same path as start + distance * curve(progress) but needs
                // no memory of where the move started.
                let previous = self.progress;
                self.progress = (previous + 1. / samples).min(1.);
                let from = shaped.shaped(previous);
                if self.progress >= 1. || 1. - from <= 1e-12 {
                    self.value = target;
                } else {
                    self.value += (target - self.value) * (shaped.shaped(self.progress) - from) / (1. - from);
                }
            }
        }
        if self.value == target || (snap > 0. && (target - self.value).abs() <= snap * 0.01 * self.distance) {
            self.settle(target);
        }
        self.value
    }
    fn settle(&mut self, target: f64) {
        self.value = target;
        self.velocity = 0.;
        self.distance = 0.;
        self.progress = 1.;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Move to 1 from rest and report the output of every sample.
    fn move_to(curve: Curve, snap: f64, samples: usize) -> Vec<f64> {
        let mut s = Smoothing::default();
        s.tick(0., curve, 100., snap, 1000.);
        (0..samples).map(|_| s.tick(1., curve, 100., snap, 1000.)).collect()
    }
    fn arrival(values: &[f64]) -> Option<usize> {
        values.iter().position(|v| *v == 1.)
    }
    #[test]
    fn each_shaped_curve_follows_its_easing_and_lands_exactly_on_time() {
        // 100 ms at 1 kHz is a 100-sample move; half way through it the curves
        // are at their defining values, and all four arrive on the last sample.
        for (curve, half) in [
            (Curve::Linear, 0.5),
            (Curve::EaseIn, 0.25),
            (Curve::EaseOut, 0.75),
            (Curve::EaseInOut, 0.5),
        ] {
            let values = move_to(curve, 0., 200);
            assert!((values[49] - half).abs() < 1e-9, "half way: {} vs {half}", values[49]);
            assert_eq!(arrival(&values), Some(99), "arrives as the move ends");
            assert!(values.windows(2).all(|w| w[1] >= w[0]), "monotone approach");
        }
    }
    #[test]
    fn the_chase_curves_only_approach_the_target_until_snap_ends_the_move() {
        let exponential = move_to(Curve::Exponential, 0., 600);
        // One time constant is 63.2% of the way, and nothing ever arrives.
        assert!((exponential[99] - 0.632_120_6).abs() < 1e-6, "{}", exponential[99]);
        assert_eq!(arrival(&exponential), None);
        let spring = move_to(Curve::Spring, 0., 600);
        assert_eq!(arrival(&spring), None);
        assert!(spring.iter().all(|v| *v < 1.), "critical damping does not overshoot");
        assert!(spring[400] > 0.99, "but it does get close: {}", spring[400]);
        // Snap is what makes a chase curve finish: 5% of the way out is 3 time
        // constants for the one-pole.
        assert_eq!(arrival(&move_to(Curve::Exponential, 5., 600)), Some(299));
        assert_eq!(arrival(&move_to(Curve::Spring, 5., 600)), Some(237));
        // A shaped curve is cut slightly short by the same rule.
        assert_eq!(arrival(&move_to(Curve::Linear, 5., 200)), Some(94));
    }
    #[test]
    fn the_first_sample_adopts_its_input_instead_of_sweeping_up_from_zero() {
        let mut s = Smoothing::default();
        assert_eq!(s.tick(0.8, Curve::EaseInOut, 100., 5., 1000.), 0.8);
        assert_eq!(s.tick(0.8, Curve::EaseInOut, 100., 5., 1000.), 0.8);
    }
    #[test]
    fn a_target_that_moves_mid_flight_redirects_the_move_without_a_jump() {
        let mut s = Smoothing::default();
        s.tick(0., Curve::Linear, 100., 0., 1000.);
        for _ in 0..50 {
            s.tick(1., Curve::Linear, 100., 0., 1000.);
        }
        let before = s.value;
        assert!((before - 0.5).abs() < 1e-9);
        // The target triples, and the output still moves by one ordinary step.
        let after = s.tick(3., Curve::Linear, 100., 0., 1000.);
        assert!(after > before && after - before < 0.06, "{before} -> {after}");
        // The move keeps its schedule: it still ends 50 samples from its start.
        let rest: Vec<f64> = (0..49).map(|_| s.tick(3., Curve::Linear, 100., 0., 1000.)).collect();
        assert_eq!(rest.last(), Some(&3.));
    }
    #[test]
    fn zero_time_is_an_immediate_move() {
        let mut s = Smoothing::default();
        s.tick(0., Curve::EaseInOut, 0., 5., 1000.);
        assert_eq!(s.tick(9., Curve::EaseInOut, 0., 5., 1000.), 9.);
    }
}
