//! Fixed-size MIDI 1.0 channel messages; never audio or scalar control values.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Message {
    pub status: u8,
    pub data1: u8,
    pub data2: u8,
}
impl Message {
    pub fn valid(self) -> bool {
        (0x80..=0xef).contains(&self.status) && self.data1 < 128 && self.data2 < 128
    }
    pub fn bytes(self) -> ([u8; 3], usize) {
        (
            [self.status, self.data1, self.data2],
            if matches!(self.status >> 4, 12 | 13) {
                2
            } else {
                3
            },
        )
    }
}
