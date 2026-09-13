//! Bounded note-event to scalar-control conversion. All storage is prepared.
use std::collections::VecDeque;
const CAPACITY: usize = 1024;

pub struct MidiControls {
    queue: VecDeque<(u8, u8, bool)>,
    held: [u16; 128],
    values: [f64; 5],
    gap: bool,
    releasing: bool,
    pub dropped: u64,
    pub bend: f64,
    pub bend_ratio: f64,
}
impl MidiControls {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::with_capacity(CAPACITY),
            held: [0; 128],
            values: [0.; 5],
            gap: false,
            releasing: false,
            dropped: 0,
            bend: 0.,
            bend_ratio: 1.,
        }
    }
    pub fn note(&mut self, pitch: u8, velocity: u8) {
        if pitch > 127 || velocity > 127 {
            return;
        }
        if self.queue.len() == CAPACITY {
            self.dropped += self.queue.len() as u64 + 1;
            self.release();
            return;
        }
        self.queue.push_back((pitch, velocity, true));
    }
    pub fn cc(&mut self, controller: u8, value: u8) {
        if self.queue.len() == CAPACITY {
            self.dropped += 1;
            return;
        }
        self.queue.push_back((controller, value, false));
    }
    /// Decode fixed channel messages into the serializer. Note on/off become
    /// `note`; CC 120/123 on any channel releases; other CC values reach `cc`
    /// only when `cc_controls`. Pitch bend uses a two-semitone range; like the
    /// note decoder, it merges channels. Everything else is ignored. No allocation.
    pub fn feed(&mut self, events: &[pr0_core::midi::Message], cc_controls: bool) {
        for event in events {
            match event.status >> 4 {
                14 => {
                    let value = u16::from(event.data1) | (u16::from(event.data2) << 7);
                    self.bend =
                        (f64::from(value) - 8192.) / if value < 8192 { 8192. } else { 8191. } * 2.;
                    self.bend_ratio = 2_f64.powf(self.bend / 12.);
                }
                9 => self.note(event.data1, event.data2),
                8 => self.note(event.data1, 0),
                11 if cc_controls => self.cc(event.data1, event.data2),
                11 if matches!(event.data1, 120 | 123) => self.release(),
                _ => {}
            }
        }
    }
    /// Cancel unsent attacks and release every note already emitted, by pitch.
    pub fn release(&mut self) {
        self.queue.clear();
        self.bend = 0.;
        self.bend_ratio = 1.;
        self.releasing = true;
    }
    pub fn held(&self, pitch: usize) -> bool {
        self.held[pitch] > 0
    }
    pub fn tick(&mut self) -> [f64; 5] {
        self.values[3] = 0.;
        self.values[4] = 0.;
        if self.gap {
            self.gap = false;
            return self.values;
        }
        let event = if self.releasing {
            if let Some(pitch) = self.held.iter().position(|count| *count > 0) {
                Some((pitch as u8, 0, true))
            } else {
                self.releasing = false;
                self.queue.pop_front()
            }
        } else {
            self.queue.pop_front()
        };
        if let Some((pitch, velocity, note)) = event {
            if !note {
                self.values = [f64::from(pitch), f64::from(velocity), 0., 1., 0.];
                self.gap = true;
                return self.values;
            }
            self.values[0] = f64::from(pitch);
            self.values[1] = f64::from(velocity);
            let held = &mut self.held[pitch as usize];
            if velocity > 0 {
                *held = held.saturating_add(1);
                self.values[3] = 1.;
            } else {
                *held = held.saturating_sub(1);
                self.values[4] = 1.;
            }
            self.values[2] = if self.held.iter().any(|count| *count > 0) {
                1.
            } else {
                0.
            };
            self.gap = true;
        }
        self.values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn chords_releases_and_same_pitch_instances_remain_distinct() {
        let mut midi = MidiControls::new();
        for event in [(60, 90), (64, 80), (60, 70), (64, 0), (60, 0), (60, 0)] {
            midi.note(event.0, event.1);
        }
        for (index, (pitch, velocity)) in [(60, 90), (64, 80), (60, 70), (64, 0), (60, 0), (60, 0)]
            .into_iter()
            .enumerate()
        {
            let values = midi.tick();
            assert_eq!(
                values,
                [
                    pitch as f64,
                    velocity as f64,
                    if index < 5 { 1. } else { 0. },
                    if velocity > 0 { 1. } else { 0. },
                    if velocity == 0 { 1. } else { 0. }
                ]
            );
            assert_eq!(&midi.tick()[3..], &[0., 0.]);
        }
    }
    #[test]
    fn cancellation_and_overflow_release_emitted_notes_without_queued_attacks() {
        let mut midi = MidiControls::new();
        midi.note(60, 100);
        midi.tick();
        midi.tick();
        midi.note(64, 90);
        midi.release();
        assert_eq!(midi.tick(), [60., 0., 0., 0., 1.]);
        midi.tick();
        assert_eq!(midi.tick()[3], 0.);
        midi.note(67, 80);
        midi.tick();
        midi.tick();
        for _ in 0..=CAPACITY {
            midi.note(70, 90);
        }
        assert_eq!(midi.dropped, CAPACITY as u64 + 1);
        assert_eq!(midi.tick(), [67., 0., 0., 0., 1.]);
        assert_eq!(midi.queue.capacity(), CAPACITY);
    }
    #[test]
    fn feed_maps_notes_all_notes_off_and_optional_cc() {
        use pr0_core::midi::Message;
        let m = |status, data1, data2| Message {
            status,
            data1,
            data2,
        };
        let mut midi = MidiControls::new();
        midi.feed(&[m(0x93, 60, 100), m(0xb3, 7, 99), m(0xe0, 0, 64)], false);
        assert_eq!(midi.tick(), [60., 100., 1., 1., 0.]);
        midi.tick();
        assert_eq!(midi.tick(), [60., 100., 1., 0., 0.]);
        midi.feed(&[m(0x83, 60, 0)], false);
        assert_eq!(midi.tick(), [60., 0., 0., 0., 1.]);
        midi.tick();
        midi.feed(&[m(0x90, 64, 90), m(0x90, 64, 0)], false);
        assert_eq!(midi.tick(), [64., 90., 1., 1., 0.]);
        midi.tick();
        assert_eq!(midi.tick(), [64., 0., 0., 0., 1.]);
        midi.tick();
        midi.feed(&[m(0xb5, 7, 99)], true);
        assert_eq!(midi.tick(), [7., 99., 0., 1., 0.]);
        midi.tick();
        midi.feed(&[m(0x90, 62, 80)], false);
        midi.tick();
        midi.tick();
        midi.feed(&[m(0x90, 65, 80), m(0xb0, 123, 0)], false);
        assert_eq!(midi.tick(), [62., 0., 0., 0., 1.]);
        midi.tick();
        assert_eq!(midi.tick()[3..], [0., 0.]);
    }
}
