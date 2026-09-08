//! Monitor-only count-in, advanced once per engine sample while transport holds.
use crate::Clock;

pub struct CountIn {
    beats: u8,
    beat_unit: u8,
    position: f64,
    click_beat: u8,
    click_sample: u64,
}

impl CountIn {
    pub fn new(beats: u8, beat_unit: u8) -> Option<Self> {
        (beats > 0).then_some(Self {
            beats,
            beat_unit,
            position: 0.,
            click_beat: u8::MAX,
            click_sample: 0,
        })
    }

    pub fn remaining(&self) -> u8 {
        self.beats
            .saturating_sub((self.position + 1e-9).floor() as u8)
    }

    /// Some(click) owns this sample; None starts performance on this sample.
    /// Tempo edits change the rate of progress without resetting beat phase.
    pub fn next(&mut self, clock: &Clock) -> Option<f32> {
        if self.position + 1e-9 >= f64::from(self.beats) {
            return None;
        }
        let beat = (self.position + 1e-9).floor() as u8;
        if beat != self.click_beat {
            self.click_beat = beat;
            self.click_sample = clock.sample;
        }
        let elapsed = (clock.sample - self.click_sample) as f64 / clock.sample_rate;
        let frequency = if beat == 0 { 1500. } else { 1000. };
        let envelope = (1. - elapsed / 0.025).max(0.);
        let click = (std::f64::consts::TAU * frequency * elapsed).sin() * envelope * 0.2;
        self.position += clock.bpm / (60. * clock.sample_rate) * f64::from(self.beat_unit) / 4.;
        Some(click as f32)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_meter_duration_and_clicks_across_sample_rates() {
        for rate in [44100., 48000., 96000.] {
            for (beats, unit) in [(4, 4), (6, 8), (3, 2), (16, 32)] {
                let mut clock = Clock::new(rate);
                let mut count = CountIn::new(beats, unit).unwrap();
                let expected = (rate * 0.5 * 4. / f64::from(unit) * f64::from(beats)) as u64;
                let mut clicks = 0;
                let mut last_audible: Option<u64> = None;
                while let Some(sample) = count.next(&clock) {
                    assert!(sample.is_finite() && sample.abs() <= 0.2);
                    if sample.abs() > 1e-6 {
                        if last_audible
                            .is_none_or(|last| clock.sample - last > (rate * 0.03) as u64)
                        {
                            clicks += 1;
                        }
                        last_audible = Some(clock.sample);
                    }
                    clock.advance();
                    assert!(clock.sample <= expected);
                }
                assert_eq!(clock.sample, expected);
                assert_eq!(clicks, beats);
                assert_eq!(clock.beat, 0.);
                assert_eq!(count.remaining(), 0);
            }
        }
    }

    #[test]
    fn tempo_change_preserves_partial_count_beat() {
        let mut clock = Clock::new(48000.);
        let mut count = CountIn::new(2, 4).unwrap();
        for _ in 0..12000 {
            count.next(&clock).unwrap();
            clock.advance();
        }
        assert_eq!(count.remaining(), 2);
        clock.set_tempo(60.);
        for _ in 0..72000 {
            count.next(&clock).unwrap();
            clock.advance();
        }
        assert!(count.next(&clock).is_none());
        assert_eq!(clock.beat, 0.);
        assert!(CountIn::new(0, 4).is_none());
    }
}
