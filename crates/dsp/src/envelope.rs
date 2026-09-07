//! Linear sample-counted ADSR. Durations latch when their stage starts.
#[derive(Clone, Copy, Default, PartialEq)]
enum Stage {
    #[default]
    Idle,
    Attack,
    Decay,
    Sustain,
    Release,
}
#[derive(Default)]
pub struct Adsr {
    stage: Stage,
    level: f64,
    target: f64,
    step: f64,
    remaining: u64,
    gate: bool,
    trigger: bool,
}
pub struct Settings {
    pub attack: f64,
    pub decay: f64,
    pub sustain: f64,
    pub release: f64,
}
impl Adsr {
    fn begin(&mut self, stage: Stage, target: f64, ms: f64, sample_rate: f64) {
        self.stage = stage;
        self.target = target;
        self.remaining = (ms * sample_rate / 1000.).round() as u64;
        self.step = if self.remaining == 0 {
            0.
        } else {
            (target - self.level) / self.remaining as f64
        };
    }
    pub fn tick(&mut self, gate: bool, trigger: bool, settings: Settings, sample_rate: f64) -> f64 {
        if gate && (!self.gate || (trigger && !self.trigger)) {
            self.begin(Stage::Attack, 1., settings.attack, sample_rate);
        } else if !gate && self.gate {
            self.begin(Stage::Release, 0., settings.release, sample_rate);
        }
        self.gate = gate;
        self.trigger = trigger;
        // At most attack, decay and sustain can be traversed in a zero-time tick.
        for _ in 0..3 {
            match self.stage {
                Stage::Idle => {
                    self.level = 0.;
                    break;
                }
                Stage::Sustain => {
                    // Live sustain edits are smoothed over 5 ms.
                    self.level += (settings.sustain - self.level)
                        * (1. - (-1. / (0.005 * sample_rate)).exp());
                    break;
                }
                _ if self.remaining > 0 => {
                    self.level += self.step;
                    self.remaining -= 1;
                    if self.remaining == 0 {
                        self.level = self.target;
                    }
                    break;
                }
                Stage::Attack => {
                    self.level = self.target;
                    self.begin(Stage::Decay, settings.sustain, settings.decay, sample_rate);
                }
                Stage::Decay => {
                    self.level = self.target;
                    self.stage = Stage::Sustain;
                }
                Stage::Release => {
                    self.level = 0.;
                    self.stage = Stage::Idle;
                }
            }
        }
        self.level.clamp(0., 1.)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn settings() -> Settings {
        Settings {
            attack: 4.,
            decay: 2.,
            sustain: 0.5,
            release: 2.,
        }
    }
    #[test]
    fn stages_follow_sample_counts_and_release_current_level() {
        let mut env = Adsr::default();
        for expected in [0.25, 0.5, 0.75, 1., 0.75, 0.5, 0.5] {
            assert_eq!(env.tick(true, false, settings(), 1000.), expected);
        }
        assert_eq!(env.tick(false, false, settings(), 1000.), 0.25);
        assert_eq!(env.tick(false, false, settings(), 1000.), 0.);
        assert_eq!(env.tick(false, false, settings(), 1000.), 0.);
    }
    #[test]
    fn retrigger_and_early_release_start_from_current_level() {
        let mut env = Adsr::default();
        assert_eq!(env.tick(true, false, settings(), 1000.), 0.25);
        assert_eq!(env.tick(false, false, settings(), 1000.), 0.125);
        assert_eq!(env.tick(true, false, settings(), 1000.), 0.34375);
        let current = env.level;
        assert_eq!(
            env.tick(true, true, settings(), 1000.),
            current + (1. - current) / 4.
        );
    }
    #[test]
    fn zero_durations_are_finite_and_immediate() {
        let mut env = Adsr::default();
        let s = || Settings {
            attack: 0.,
            decay: 0.,
            sustain: 0.7,
            release: 0.,
        };
        assert_eq!(env.tick(true, false, s(), 48000.), 0.7);
        assert_eq!(env.tick(false, false, s(), 48000.), 0.);
    }
}
