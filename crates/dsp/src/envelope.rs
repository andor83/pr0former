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
    /// The envelope is between an attack and its release.
    on: bool,
    gate: bool,
    trigger: bool,
    note_off: bool,
}
pub struct Settings {
    pub attack: f64,
    pub decay: f64,
    pub sustain: f64,
    pub release: f64,
    /// Every attack restarts from 0 instead of ramping from the current level.
    pub reset: bool,
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
    /// Advance one sample. Every input is edge triggered: a rising `gate` or
    /// `trigger` starts (or restarts) the attack from the current level, and a
    /// falling `gate` or rising `note_off` starts the release from the current
    /// level. A note off therefore releases even while the gate parameter is
    /// left high; the gate must fall and rise again to attack once more. With
    /// `settings.reset` every attack drops to 0 before ramping.
    pub fn tick(
        &mut self,
        gate: bool,
        trigger: bool,
        note_off: bool,
        settings: Settings,
        sample_rate: f64,
    ) -> f64 {
        let attack = (gate && !self.gate) || (trigger && !self.trigger);
        let release = (!gate && self.gate) || (note_off && !self.note_off);
        if attack {
            self.on = true;
            if settings.reset {
                self.level = 0.;
            }
            self.begin(Stage::Attack, 1., settings.attack, sample_rate);
        } else if release && self.on {
            self.on = false;
            self.begin(Stage::Release, 0., settings.release, sample_rate);
        }
        self.gate = gate;
        self.trigger = trigger;
        self.note_off = note_off;
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
            reset: false,
        }
    }
    #[test]
    fn stages_follow_sample_counts_and_release_current_level() {
        let mut env = Adsr::default();
        for expected in [0.25, 0.5, 0.75, 1., 0.75, 0.5, 0.5] {
            assert_eq!(env.tick(true, false, false, settings(), 1000.), expected);
        }
        assert_eq!(env.tick(false, false, false, settings(), 1000.), 0.25);
        assert_eq!(env.tick(false, false, false, settings(), 1000.), 0.);
        assert_eq!(env.tick(false, false, false, settings(), 1000.), 0.);
    }
    #[test]
    fn retrigger_and_early_release_start_from_current_level() {
        let mut env = Adsr::default();
        assert_eq!(env.tick(true, false, false, settings(), 1000.), 0.25);
        assert_eq!(env.tick(false, false, false, settings(), 1000.), 0.125);
        assert_eq!(env.tick(true, false, false, settings(), 1000.), 0.34375);
        let current = env.level;
        assert_eq!(
            env.tick(true, true, false, settings(), 1000.),
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
            reset: false,
        };
        assert_eq!(env.tick(true, false, false, s(), 48000.), 0.7);
        assert_eq!(env.tick(false, false, false, s(), 48000.), 0.);
    }
    #[test]
    fn trigger_pulse_holds_until_note_off_pulse_without_a_gate() {
        let mut env = Adsr::default();
        assert_eq!(env.tick(false, false, false, settings(), 1000.), 0.);
        // One-sample trigger pulse attacks and latches the envelope on.
        assert_eq!(env.tick(false, true, false, settings(), 1000.), 0.25);
        for expected in [0.5, 0.75, 1., 0.75, 0.5, 0.5, 0.5] {
            assert_eq!(env.tick(false, false, false, settings(), 1000.), expected);
        }
        // A one-sample note-off pulse releases; holding it high does nothing more.
        assert_eq!(env.tick(false, false, true, settings(), 1000.), 0.25);
        assert_eq!(env.tick(false, false, true, settings(), 1000.), 0.);
        assert_eq!(env.tick(false, false, false, settings(), 1000.), 0.);
        // A second trigger while note-off is still high re-attacks.
        assert_eq!(env.tick(false, true, true, settings(), 1000.), 0.25);
    }
    #[test]
    fn note_off_pulse_releases_a_high_gate_until_the_gate_rises_again() {
        let mut env = Adsr::default();
        for _ in 0..7 {
            env.tick(true, false, false, settings(), 1000.);
        }
        // Gate parameter left at 1 with a note-off pulse wired in: it still releases.
        assert_eq!(env.tick(true, false, true, settings(), 1000.), 0.25);
        assert_eq!(env.tick(true, false, false, settings(), 1000.), 0.);
        assert_eq!(env.tick(true, false, false, settings(), 1000.), 0.);
        // A trigger pulse re-attacks while the gate stays high...
        assert_eq!(env.tick(true, true, false, settings(), 1000.), 0.25);
        assert_eq!(env.tick(true, false, true, settings(), 1000.), 0.125);
        assert_eq!(env.tick(true, false, false, settings(), 1000.), 0.);
        // ...and so does the gate falling and rising again.
        assert_eq!(env.tick(false, false, false, settings(), 1000.), 0.);
        assert_eq!(env.tick(true, false, false, settings(), 1000.), 0.25);
    }
    #[test]
    fn retrigger_from_zero_restarts_the_full_attack_ramp() {
        let reset = || Settings {
            reset: true,
            ..settings()
        };
        let mut env = Adsr::default();
        for _ in 0..7 {
            env.tick(true, false, false, reset(), 1000.);
        }
        assert_eq!(env.level, 0.5);
        // A trigger while sustaining drops to 0 and ramps 0.25 per ms again.
        assert_eq!(env.tick(true, true, false, reset(), 1000.), 0.25);
        assert_eq!(env.tick(true, false, false, reset(), 1000.), 0.5);
        // Without the option the same trigger ramps from the current level.
        let mut env = Adsr::default();
        for _ in 0..7 {
            env.tick(true, false, false, settings(), 1000.);
        }
        assert_eq!(env.tick(true, true, false, settings(), 1000.), 0.625);
    }
}
