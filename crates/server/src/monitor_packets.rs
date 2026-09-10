//! Assemble fixed 10 ms / 48 kHz monitor packets before the bounded broadcast.
//! Runs between DSP blocks. Queue duration is independent of DSP block size.
use crate::media::AudioBlock;
use std::{collections::BTreeMap, sync::Arc};
const SAMPLES: usize = 480 * 2;
// Covers even a full 16,384-frame native-ring refill at 44.1 kHz. This is
// capacity for bursts, not a prebuffer or a delay before sending live packets.
pub const QUEUED_PACKETS: usize = 64;
#[derive(Default)]
pub struct Packets {
    project: String,
    rate: u32,
    pcm: Vec<f32>,
    monitors: BTreeMap<String, Vec<f32>>,
}
impl Packets {
    /// Reset partial PCM and resampler history together on project/rate/feed changes.
    pub fn configure(
        &mut self,
        project: &str,
        rate: u32,
        feeds: &BTreeMap<String, Vec<f32>>,
    ) -> bool {
        if self.project == project && self.rate == rate && self.monitors.keys().eq(feeds.keys()) {
            return false;
        }
        self.project = project.into();
        self.rate = rate;
        self.pcm = Vec::with_capacity(SAMPLES);
        self.monitors = feeds
            .keys()
            .map(|id| (id.clone(), Vec::with_capacity(SAMPLES)))
            .collect();
        true
    }
    pub fn push(
        &mut self,
        pcm: &[f32],
        feeds: &BTreeMap<String, Vec<f32>>,
        mut send: impl FnMut(AudioBlock),
    ) {
        debug_assert!(self.monitors.keys().eq(feeds.keys()));
        debug_assert!(feeds.values().all(|feed| feed.len() == pcm.len()));
        let mut offset = 0;
        while offset < pcm.len() {
            let end = (offset + SAMPLES - self.pcm.len()).min(pcm.len());
            self.pcm.extend_from_slice(&pcm[offset..end]);
            for (id, pending) in &mut self.monitors {
                pending.extend_from_slice(&feeds[id][offset..end]);
            }
            offset = end;
            if self.pcm.len() == SAMPLES {
                send(AudioBlock {
                    project: self.project.clone(),
                    pcm: Arc::new(std::mem::replace(
                        &mut self.pcm,
                        Vec::with_capacity(SAMPLES),
                    )),
                    monitors: self
                        .monitors
                        .iter_mut()
                        .map(|(id, pending)| {
                            (
                                id.clone(),
                                Arc::new(std::mem::replace(pending, Vec::with_capacity(SAMPLES))),
                            )
                        })
                        .collect(),
                });
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn small_dsp_blocks_do_not_overrun_the_monitor_queue_during_a_callback_burst() {
        // A full native ring may be refilled in one burst of small DSP blocks.
        // The former eight-block queue lost almost all of that refill.
        for rate in [44100, 96000] {
            for block in [32, 64, 128, 256, 512, 1024] {
                let (tx, mut rx) = tokio::sync::broadcast::channel(QUEUED_PACKETS);
                let mut packets = Packets::default();
                let mut feed = BTreeMap::from([("cue".into(), vec![])]);
                assert!(packets.configure("project", rate, &feed));
                let mut master = crate::samples::RateAdapter::new(rate, 48000);
                let mut cue = crate::samples::RateAdapter::new(rate, 48000);
                let mut converted = 0;
                for start in (0..16384).step_by(block) {
                    let raw: Vec<_> = (start..start + block)
                        .flat_map(|i| [i as f32 / 4096.; 2])
                        .collect();
                    let pcm = master.process(&raw);
                    converted += pcm.len();
                    feed.insert(
                        "cue".into(),
                        cue.process(&raw.iter().map(|v| -v).collect::<Vec<_>>()),
                    );
                    packets.push(&pcm, &feed, |packet| {
                        assert!(tx.send(packet).is_ok());
                    });
                }
                let mut count = 0;
                while !rx.is_empty() {
                    let packet = rx.try_recv().expect("no lag during callback refill");
                    assert_eq!(packet.pcm.len(), SAMPLES);
                    assert!(
                        packet
                            .pcm
                            .iter()
                            .zip(packet.monitors["cue"].iter())
                            .all(|(a, b)| (a + b).abs() < 1e-6)
                    );
                    count += 1;
                }
                assert_eq!(count, converted / SAMPLES);
            }
        }
    }
    #[test]
    fn partial_packets_preserve_samples_and_reset_on_project_rate_or_feed_changes() {
        let mut packets = Packets::default();
        let mut feeds = BTreeMap::from([("cue".into(), vec![])]);
        assert!(packets.configure("a", 44100, &feeds));
        let mut result = vec![];
        for start in (0..2880).step_by(64) {
            let pcm: Vec<_> = (start..(start + 64).min(2880)).map(|i| i as f32).collect();
            feeds.insert("cue".into(), pcm.clone());
            assert!(!packets.configure("a", 44100, &feeds));
            packets.push(&pcm, &feeds, |p| result.extend_from_slice(&p.pcm));
        }
        assert_eq!(result, (0..2880).map(|i| i as f32).collect::<Vec<_>>());
        for (project, rate) in [("b", 44100), ("b", 96000)] {
            feeds.insert("cue".into(), vec![9.; 128]);
            packets.push(&[9.; 128], &feeds, |_| panic!("partial packet"));
            assert!(packets.configure(project, rate, &feeds));
            assert!(packets.pcm.is_empty());
        }
        feeds.clear();
        assert!(packets.configure("b", 96000, &feeds));
        packets.push(&vec![0.; SAMPLES], &feeds, |p| {
            assert!(p.monitors.is_empty());
            assert!(p.pcm.iter().all(|v| *v == 0.));
        });
    }
}
