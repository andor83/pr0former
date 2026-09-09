//! All event storage is allocated during graph preparation.
use pr0_core::midi::Message;
pub const CAPACITY: usize = 256;
pub struct Buffer {
    pub events: [Message; CAPACITY],
    pub len: usize,
    pub cursor: usize,
    pub dropped: u64,
}
impl Buffer {
    pub fn new() -> Self {
        Self {
            events: [Message::default(); CAPACITY],
            len: 0,
            cursor: 0,
            dropped: 0,
        }
    }
    pub fn clear(&mut self) {
        self.len = 0;
        self.cursor = 0
    }
    pub fn push(&mut self, event: Message) {
        if self.len < CAPACITY {
            self.events[self.len] = event;
            self.len += 1
        } else {
            self.dropped += self.len as u64 + 1;
            self.clear();
            for channel in 0..16 {
                self.events[self.len] = Message {
                    status: 0xb0 | channel,
                    data1: 123,
                    data2: 0,
                };
                self.len += 1
            }
        }
    }
    pub fn pop(&mut self) -> Option<Message> {
        if self.cursor >= self.len {
            return None;
        }
        let e = self.events[self.cursor];
        self.cursor += 1;
        Some(e)
    }
}
