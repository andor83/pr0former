//! Bounded debug capture. No formatting, allocation, locks or I/O during rendering.
use crate::visualizer::Datum;
#[derive(Clone, Copy)]
pub struct Entry { pub node: usize, pub sample: u64, pub value: Datum }
pub struct Buffer { entries: Vec<Option<Entry>>, head: usize, len: usize, dropped: u64 }
impl Buffer {
    pub fn new() -> Self { Self { entries: vec![None;128], head:0, len:0, dropped:0 } }
    pub fn push(&mut self, entry: Entry) {
        if self.len == self.entries.len() { self.dropped = self.dropped.saturating_add(1); return; }
        let index=(self.head+self.len)%self.entries.len(); self.entries[index]=Some(entry); self.len+=1;
    }
    pub fn pop(&mut self) -> Option<Entry> {
        if self.len==0 { return None; }
        let value=self.entries[self.head].take();self.head=(self.head+1)%self.entries.len();self.len-=1;value
    }
    pub fn take_dropped(&mut self) -> u64 { std::mem::take(&mut self.dropped) }
}
