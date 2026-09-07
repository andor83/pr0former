//! Tempo-linked clock subdivision using engine quarter-note position.
#[derive(Default)]
pub struct ClockRatio {
    reset_generation: u64,
    previous: Option<f64>,
    phase: f64,
    cycles: u64,
}
impl ClockRatio {
    pub fn tick_clock(
        &mut self,
        clock: &crate::Clock,
        numerator: f64,
        denominator: f64,
    ) -> [f64; 3] {
        if self.reset_generation != clock.reset_generation {
            *self = Self {
                reset_generation: clock.reset_generation,
                ..Self::default()
            };
        }
        self.tick(clock.beat, clock.running, numerator, denominator)
    }

    /// Pulse, fractional phase, completed cycles. Transport pause holds phase.
    pub fn tick(&mut self, beat: f64, running: bool, numerator: f64, denominator: f64) -> [f64; 3] {
        let Some(previous) = self.previous else {
            if running {
                self.previous = Some(beat);
                return [1., self.phase, 0.];
            }
            return [0., self.phase, self.cycles as f64];
        };
        if beat < previous {
            self.previous = None;
            self.phase = 0.;
            self.cycles = 0;
            return self.tick(beat, running, numerator, denominator);
        }
        if !running {
            return [0., self.phase, self.cycles as f64];
        }
        self.previous = Some(beat);
        self.phase += (beat - previous) * numerator / denominator;
        // Account for numerical round-off at exact musical boundaries.
        let completed = (self.phase + 1e-9).floor() as u64;
        if completed > 0 {
            self.phase = (self.phase - completed as f64).max(0.);
            self.cycles = self.cycles.saturating_add(completed);
        }
        [(completed > 0) as u8 as f64, self.phase, self.cycles as f64]
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn subdivisions_and_divisions_follow_musical_position() {
        let mut multiply = ClockRatio::default();
        assert_eq!(multiply.tick(0., true, 4., 1.)[0], 1.);
        assert_eq!(multiply.tick(0.125, true, 4., 1.), [0., 0.5, 0.]);
        assert_eq!(multiply.tick(0.25, true, 4., 1.), [1., 0., 1.]);
        let mut divide = ClockRatio::default();
        divide.tick(0., true, 1., 2.);
        assert_eq!(divide.tick(1., true, 1., 2.), [0., 0.5, 0.]);
        assert_eq!(divide.tick(2., true, 1., 2.), [1., 0., 1.]);
    }
    #[test]
    fn edits_and_pause_preserve_phase_and_stop_resets() {
        let mut c = ClockRatio::default();
        c.tick(0., true, 1., 1.);
        assert_eq!(c.tick(0.5, true, 1., 1.)[1], 0.5);
        assert_eq!(c.tick(0.5, true, 2., 1.), [0., 0.5, 0.]);
        assert_eq!(c.tick(0.5, false, 2., 1.), [0., 0.5, 0.]);
        assert_eq!(c.tick(0.5, true, 2., 1.), [0., 0.5, 0.]);
        assert_eq!(c.tick(0.75, true, 2., 1.), [1., 0., 1.]);
        assert_eq!(c.tick(0., false, 2., 1.), [0., 0., 0.]);
        assert_eq!(c.tick(0., true, 2., 1.), [1., 0., 0.]);
    }
}
