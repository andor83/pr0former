//! Prepared independent score clips. Tick uses engine samples, never show transport.
use crate::{Clock, midi_events::Buffer, score_automation::Automation};
use pr0_core::{
    midi::Message,
    score::{AutomationLane, MeterChange, Span},
};

#[derive(Clone, PartialEq)]
pub struct Event {
    pub beat: f64,
    pub message: Message,
}
#[derive(Clone, PartialEq)]
pub struct Clip {
    pub events: Vec<Event>,
    pub length: f64,
    pub meters: Vec<MeterChange>,
    pub spans: Vec<Span>,
    pub automation: Vec<AutomationLane>,
}
pub struct Player {
    clip: Clip,
    automation: Vec<Automation>,
    held: [[u16; 128]; 16],
    previous: [bool; 3],
    next: usize,
    pending: bool,
    last_pulse: Option<(i64, u64, u64)>,
    last_position: f64,
    pending_repeat: bool,
    pub playing: bool,
    pub repeating: bool,
    pub position: f64,
}
pub fn compatible(a: Option<&Player>, b: Option<&Player>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.clip == b.clip,
        (None, None) => true,
        _ => false,
    }
}
impl Player {
    pub fn new(clip: Clip) -> Self {
        let automation = clip
            .automation
            .iter()
            .cloned()
            .map(Automation::new)
            .collect();
        Self {
            clip,
            automation,
            held: [[0; 128]; 16],
            previous: [false; 3],
            next: 0,
            pending: false,
            last_pulse: None,
            last_position: 0.,
            pending_repeat: false,
            playing: false,
            repeating: false,
            position: 0.,
        }
    }
    pub fn pending(&self) -> bool {
        self.pending
    }
    fn emit(&mut self, message: Message, out: &mut Buffer) {
        let count = &mut self.held[(message.status & 15) as usize][message.data1 as usize];
        match message.status >> 4 {
            9 if message.data2 > 0 => *count = count.saturating_add(1),
            8 | 9 => *count = count.saturating_sub(1),
            _ => {}
        }
        out.push(message);
    }
    fn release(&mut self, out: &mut Buffer) {
        for (channel, pitches) in self.held.iter_mut().enumerate() {
            for (pitch, count) in pitches.iter_mut().enumerate() {
                for _ in 0..*count {
                    out.push(Message {
                        status: 0x80 | channel as u8,
                        data1: pitch as u8,
                        data2: 0,
                    });
                }
                *count = 0;
            }
        }
        for lane in &mut self.automation {
            if let Some(message) = lane.reset() {
                out.push(message);
            }
        }
    }
    pub(crate) fn stop(&mut self, out: &mut Buffer) {
        self.release(out);
        self.playing = false;
        self.repeating = false;
        self.pending = false;
    }
    pub fn written_position(&self) -> f64 {
        self.clip
            .spans
            .iter()
            .find(|s| self.position < s.elapsed + s.end - s.start)
            .map(|s| s.start + self.position - s.elapsed)
            .unwrap_or_else(|| self.clip.spans.last().map_or(self.position, |s| s.end))
    }
    pub fn bar_beat(&self) -> (f64, f64) {
        let position = self.written_position();
        let mut bar = 1.;
        let mut start = 0.;
        let mut beats = 4.;
        let mut unit = 1.;
        for meter in &self.clip.meters {
            if meter.beat > position + 1e-9 {
                break;
            }
            bar += ((meter.beat - start) / (beats * unit) - 1e-9)
                .ceil()
                .max(0.);
            start = meter.beat;
            beats = f64::from(meter.beats);
            unit = 4. / f64::from(meter.unit);
        }
        let elapsed = (position - start).max(0.) / unit;
        (
            bar + ((elapsed + 1e-8) / beats).floor(),
            1. + ((elapsed + 1e-8).rem_euclid(beats)).floor(),
        )
    }
    pub(crate) fn tick(
        &mut self,
        clock: &Clock,
        inputs: [f64; 3],
        out: &mut Buffer,
        metronome: Option<(f64, f64, u8)>,
    ) {
        let high = inputs.map(|v| v.is_finite() && v > 0.);
        let edge: [bool; 3] = std::array::from_fn(|i| high[i] && !self.previous[i]);
        self.previous = high;
        let (position, origin, unit) = metronome
            .map(|(p, o, u)| (p, o, 4. / f64::from(u.max(1))))
            .unwrap_or((clock.beat, 0., clock.beat_length.max(0.001)));
        let pulse = (
            ((position - origin + 1e-9) / unit).floor() as i64,
            origin.to_bits(),
            unit.to_bits(),
        );
        let boundary = self.last_pulse.is_some_and(|last| last != pulse)
            || position + 1e-9 < self.last_position;
        self.last_pulse = Some(pulse);
        self.last_position = position;
        if edge[2] {
            self.stop(out);
            return;
        }
        // Fulfil a previously queued launch before accepting another trigger on
        // this same beat. A clock wired to Play must not postpone playback forever.
        if self.pending && boundary {
            self.release(out);
            self.pending = false;
            self.next = 0;
            self.position = 0.;
            self.repeating = self.pending_repeat;
            self.playing = self.clip.length > 0.;
        }
        if edge[0] || edge[1] {
            self.pending = true;
            self.pending_repeat = edge[1];
        }
        if !self.playing {
            return;
        }
        if self.position + 1e-9 >= self.clip.length {
            self.release(out);
            if self.repeating {
                self.position = (self.position - self.clip.length).max(0.);
                self.next = 0;
            } else {
                self.position = self.clip.length;
                self.playing = false;
                return;
            }
        }
        // Automation follows written positions through authored score repeats.
        let written = self.written_position();
        for index in 0..self.automation.len() {
            for event in self.automation[index]
                .tick(written, clock.sample, clock.sample_rate)
                .into_iter()
                .flatten()
            {
                out.push(event);
            }
        }
        while let Some(event) = self.clip.events.get(self.next) {
            if event.beat > self.position + 1e-9 {
                break;
            }
            self.emit(event.message, out);
            self.next += 1;
        }
        self.position += clock.bpm / (60. * clock.sample_rate);
    }
}
