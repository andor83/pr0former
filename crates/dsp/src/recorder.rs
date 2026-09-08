//! Fixed-capacity event buffer. File work and draining happen outside render.
#[derive(Clone, Copy, Debug)]
pub enum Event {
    Start,
    Audio([f32; 8]),
    Stop,
    Overflow,
}
pub struct Recorder {
    events: Vec<Event>,
    read: usize,
    length: usize,
    previous: [bool; 2],
    pub active: bool,
    pub frames: u64,
    pub overflow: bool,
    pub channels: usize,
}
impl Recorder {
    pub fn new(channels: usize) -> Self {
        Self {
            events: vec![Event::Stop; 65536],
            read: 0,
            length: 0,
            previous: [false; 2],
            active: false,
            frames: 0,
            overflow: false,
            channels,
        }
    }
    fn push(&mut self, event: Event) -> bool {
        if self.length == self.events.len() {
            return false;
        }
        let index = (self.read + self.length) % self.events.len();
        self.events[index] = event;
        self.length += 1;
        true
    }
    pub fn tick(&mut self, input: [f64; 8], start: f64, stop: f64) {
        let signals = [
            start.is_finite() && start > 0.,
            stop.is_finite() && stop > 0.,
        ];
        if signals[1] && !self.previous[1] {
            self.finish();
        } else if signals[0]
            && !self.previous[0]
            && !self.active
            && !self.overflow
            && self.channels > 0
        {
            if self.push(Event::Start) {
                self.active = true;
                self.frames = 0;
            } else {
                self.overflow = true;
            }
        }
        self.previous = signals;
        if self.active {
            // Reserve one event for an explicit failure marker; never silently drop audio.
            if self.length + 1 >= self.events.len() {
                self.active = false;
                self.overflow = true;
                self.push(Event::Overflow);
            } else {
                self.push(Event::Audio(input.map(|v| {
                    if v.is_finite() {
                        v.clamp(-(f32::MAX as f64), f32::MAX as f64) as f32
                    } else {
                        0.
                    }
                })));
                self.frames += 1;
            }
        }
    }
    pub fn finish(&mut self) {
        if self.active {
            self.active = false;
            if !self.push(Event::Stop) {
                self.overflow = true;
            }
        }
    }
    /// Allocates only in the orchestration worker between render blocks.
    pub fn drain(&mut self) -> Vec<Event> {
        let mut result = Vec::with_capacity(self.length);
        while self.length > 0 {
            result.push(self.events[self.read]);
            self.read = (self.read + 1) % self.events.len();
            self.length -= 1;
        }
        result
    }
    pub fn pending(&self) -> bool {
        self.length > 0
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn start_stop_are_sample_exact_and_stop_wins() {
        let mut r = Recorder::new(8);
        r.tick([0.25; 8], 1., 0.);
        r.tick([0.5; 8], 1., 0.);
        r.tick([1.; 8], 0., 1.);
        assert_eq!(r.frames, 2);
        let events = r.drain();
        assert!(
            matches!(events.as_slice(), [Event::Start, Event::Audio(a), Event::Audio(b), Event::Stop] if a[7] == 0.25 && b[7] == 0.5)
        );
        r.tick([0.; 8], 0., 0.);
        r.tick([0.; 8], 1., 1.);
        assert!(!r.active && !r.pending());
    }
    #[test]
    fn disk_backpressure_stops_capture_with_explicit_overflow() {
        let mut r = Recorder::new(2);
        for _ in 0..70000 {
            r.tick([0.; 8], 1., 0.);
        }
        assert!(!r.active && r.overflow);
        assert!(matches!(r.drain().last(), Some(Event::Overflow)));
        r.tick([0.; 8], 0., 0.);
        r.tick([0.; 8], 1., 0.);
        assert!(!r.active); // reset engine after fixing disk throughput
    }
}
