//! Bounded control-step selection. No clock, allocation, or I/O of its own.
#[derive(Default)]
pub struct Steps {
    index: usize,
    started: bool,
    trigger: bool,
    reset: bool,
}
impl Steps {
    /// Returns the current zero-based index and a one-sample advance/reset pulse.
    pub fn tick(
        &mut self,
        trigger: bool,
        reset: bool,
        enabled: bool,
        length: usize,
    ) -> (usize, bool) {
        let length = length.clamp(1, 16);
        self.index %= length;
        let reset_edge = reset && !self.reset;
        let advance = enabled && trigger && !self.trigger;
        let pulse = if reset_edge {
            self.index = 0;
            self.started = true;
            true
        } else if advance {
            if self.started {
                self.index = (self.index + 1) % length;
            }
            self.started = true;
            true
        } else {
            false
        };
        self.trigger = trigger;
        self.reset = reset;
        (self.index, pulse)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn first_pulse_and_wrap_follow_edges() {
        let mut s = Steps::default();
        for expected in [0, 1, 2, 0] {
            assert_eq!(s.tick(true, false, true, 3), (expected, true));
            assert_eq!(s.tick(true, false, true, 3), (expected, false));
            assert_eq!(s.tick(false, false, true, 3), (expected, false));
        }
    }
    #[test]
    fn reset_wins_and_disabled_edges_are_not_replayed() {
        let mut s = Steps::default();
        s.tick(true, false, true, 16);
        s.tick(false, false, true, 16);
        assert_eq!(s.tick(true, false, true, 16), (1, true));
        s.tick(false, false, true, 16);
        assert_eq!(s.tick(true, true, true, 16), (0, true));
        assert_eq!(s.tick(true, true, true, 16), (0, false));
        s.tick(false, false, false, 16);
        assert_eq!(s.tick(true, false, false, 16), (0, false));
        assert_eq!(s.tick(true, false, true, 16), (0, false));
    }
    #[test]
    fn shrinking_length_keeps_index_in_range() {
        let mut s = Steps::default();
        for _ in 0..16 {
            s.tick(true, false, true, 16);
            s.tick(false, false, true, 16);
        }
        assert_eq!(s.tick(false, false, true, 3), (0, false));
        assert_eq!(s.tick(true, false, true, 1), (0, true));
    }
}
