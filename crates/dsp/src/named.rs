//! Prepared graph-local routing snapshots. Publication is one engine sample delayed.
use crate::{
    spectral::Spectral,
    visualizer::{Datum, Text},
};
pub struct Route {
    pub name: Text,
    pub published_name: Text,
    pub ready: bool,
    pub event_only: bool,
    pub send: bool,
    pub signal: pr0_core::Signal,
    pub audio: [f64; 8],
    pub value: Datum,
    pub published_audio: [f64; 8],
    pub published_value: Datum,
    pub serial: u64,
    pub seen: Vec<u64>,
    pub snapshot: Option<Box<Spectral>>,
    pub spectral_source: Option<(usize, u64)>,
    pub connected: usize,
    pub control_source: Option<usize>,
    pub error: bool,
}
impl Route {
    pub fn new(
        kind: &str,
        target: Option<&pr0_core::ControlValue>,
        channels: usize,
        size: usize,
        overlap: usize,
        nodes: usize,
    ) -> Self {
        let name = match target {
            Some(pr0_core::ControlValue::Text(value)) => Text::new(value),
            _ => Text::new(""),
        };
        let signal = if kind.ends_with("audio") {
            pr0_core::Signal::Audio
        } else if kind.ends_with("spectral") {
            pr0_core::Signal::Spectral
        } else {
            pr0_core::Signal::Control
        };
        Self {
            name,
            published_name: name,
            ready: false,
            event_only: false,
            send: kind.starts_with("send_"),
            signal,
            audio: [0.; 8],
            value: Datum::Number(0.),
            published_audio: [0.; 8],
            published_value: Datum::Number(0.),
            serial: 0,
            seen: vec![0; nodes],
            snapshot: (signal == pr0_core::Signal::Spectral)
                .then(|| Box::new(Spectral::new(size, overlap, channels))),
            spectral_source: None,
            connected: 0,
            control_source: None,
            error: false,
        }
    }
}
