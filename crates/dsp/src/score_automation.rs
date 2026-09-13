//! Prepared automation state; event evaluation uses engine sample/beat time.
use pr0_core::{
    midi::Message,
    score::{AutomationLane, MessageKind},
};
pub struct Automation {
    pub generated: bool,
    pub lane: AutomationLane,
    last: Option<Message>,
    active: Option<usize>,
    position: f64,
    last_sample: u64,
}
impl Automation {
    pub fn new(lane: AutomationLane) -> Self {
        Self {
            generated: false,
            lane,
            last: None,
            active: None,
            position: -1.,
            last_sample: 0,
        }
    }
    pub fn generated(lane: AutomationLane) -> Self {
        let mut result = Self::new(lane);
        result.generated = true;
        result
    }
    pub fn reset(&mut self) -> Option<Message> {
        let off = self
            .active
            .take()
            .map(|_| self.lane.message_at(0.))
            .or_else(|| {
                // Pedal controllers can hold device voices after their note-offs.
                self.last
                    .filter(|m| {
                        m.status >> 4 == 0xb && matches!(m.data1, 64 | 66 | 69) && m.data2 >= 64
                    })
                    .map(|m| Message { data2: 0, ..m })
            });
        self.last = None;
        self.position = -1.;
        off
    }
    pub fn tick(&mut self, position: f64, sample: u64, rate: f64) -> [Option<Message>; 2] {
        let jumped = position + 1e-8 < self.position;
        let index = self
            .lane
            .events
            .partition_point(|e| e.beat <= position)
            .checked_sub(1);
        let note = self.lane.message == MessageKind::Note;
        if note {
            let active = index
                .filter(|i| position < self.lane.events[*i].beat + self.lane.events[*i].duration);
            let mut result = [None; 2];
            if self.active != active || jumped {
                if self.active.is_some() {
                    result[0] = Some(self.lane.message_at(0.))
                }
                if let Some(i) = active {
                    result[1] = Some(self.lane.message_at(self.lane.events[i].start))
                }
                self.active = active;
            }
            self.position = position;
            return result;
        }
        let Some(i) = index else {
            self.position = position;
            let message = self.lane.message_at(self.lane.initial.unwrap_or(
                if self.lane.message == MessageKind::Bend {
                    8192.
                } else {
                    0.
                },
            ));
            if jumped || self.last != Some(message) {
                self.last = Some(message);
                return [Some(message), None];
            }
            return [None; 2];
        };
        let e = &self.lane.events[i];
        let boundary = self.position < e.beat
            || self.position < e.beat + e.duration && position >= e.beat + e.duration;
        if !jumped
            && !boundary
            && self.last.is_some()
            && sample.saturating_sub(self.last_sample) < (rate / 100.) as u64
        {
            self.position = position;
            return [None; 2];
        }
        let t = if e.duration <= 0. {
            1.
        } else {
            (position - e.beat) / e.duration
        };
        let message = self
            .lane
            .message_at(e.start + (e.end - e.start) * e.curve.at(t));
        self.position = position;
        self.last_sample = sample;
        if self.last != Some(message) || jumped {
            self.last = Some(message);
            [Some(message), None]
        } else {
            [None; 2]
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use pr0_core::score::{AutomationEvent, Curve};
    #[test]
    fn pedal_release_and_resume_restore_without_stuck_device_notes() {
        let mut a = Automation::new(AutomationLane {
            initial: None,
            id: "pedal".into(),
            name: "Sustain".into(),
            channel: 1,
            message: MessageKind::Cc,
            number: 64,
            events: vec![AutomationEvent {
                id: "down".into(),
                beat: 0.,
                duration: 0.,
                start: 127.,
                end: 127.,
                curve: Curve::Step,
            }],
        });
        assert_eq!(a.tick(0., 0, 48000.)[0].unwrap().data2, 127);
        assert_eq!(a.reset().unwrap().data2, 0);
        assert_eq!(a.reset(), None);
        assert_eq!(a.tick(1., 1, 48000.)[0].unwrap().data2, 127);
    }
    #[test]
    fn bend_ramp_endpoints_and_backwards_state_are_exact() {
        let mut a = Automation::new(AutomationLane {
            initial: None,
            id: "a".into(),
            name: "Bend".into(),
            channel: 3,
            message: MessageKind::Bend,
            number: 0,
            events: vec![AutomationEvent {
                id: "e".into(),
                beat: 0.,
                duration: 4.,
                start: 0.,
                end: 16383.,
                curve: Curve::Linear,
            }],
        });
        assert_eq!(
            a.tick(0., 0, 48000.)[0],
            Some(Message {
                status: 0xe2,
                data1: 0,
                data2: 0
            })
        );
        assert_eq!(
            a.tick(4., 1, 48000.)[0],
            Some(Message {
                status: 0xe2,
                data1: 127,
                data2: 127
            })
        );
        assert_eq!(
            a.tick(0., 2, 48000.)[0],
            Some(Message {
                status: 0xe2,
                data1: 0,
                data2: 0
            })
        );
    }
}
