//! Per-device/per-channel sample peaks. Audio frames are observed on the worker,
//! before input routing and after summing the active graph's hardware outputs.
use pr0_core::{DEVICE_ROUTE_KEYS, MAX_DEVICE_CHANNELS, Project, device_channel};
use serde::Serialize;

#[derive(Clone, Debug, Serialize)]
pub struct ChannelLevel {
    pub channel: usize,
    pub peak: f32,
}
#[derive(Clone, Debug, Serialize)]
pub struct DeviceLevels {
    pub id: u32,
    pub name: String,
    pub channels: usize,
    pub levels: Vec<ChannelLevel>,
}
pub struct Meter {
    id: u32,
    name: String,
    channels: usize,
    peaks: [f32; MAX_DEVICE_CHANNELS],
}
impl Meter {
    pub fn new(id: u32, name: String, channels: usize) -> Self {
        Self {
            id,
            name,
            channels: channels.min(MAX_DEVICE_CHANNELS),
            peaks: [0.; MAX_DEVICE_CHANNELS],
        }
    }
    pub fn observe(&mut self, frame: &[f32; MAX_DEVICE_CHANNELS]) {
        for (peak, sample) in self.peaks[..self.channels].iter_mut().zip(frame) {
            *peak = peak.max(sample.abs());
        }
    }
    pub fn clear(&mut self) {
        self.peaks.fill(0.);
    }
    pub fn take(&mut self, selected: &[bool; MAX_DEVICE_CHANNELS]) -> DeviceLevels {
        let levels = (0..self.channels)
            .filter(|ch| selected[*ch])
            .map(|ch| ChannelLevel {
                channel: ch + 1,
                peak: self.peaks[ch],
            })
            .collect();
        self.clear();
        DeviceLevels {
            id: self.id,
            name: self.name.clone(),
            channels: self.channels,
            levels,
        }
    }
}
/// Preserve physical numbering, including sparse routes and 0/None. This mask
/// controls presentation only; audio is still summed through the engine routes.
pub fn output_channels(project: Option<&Project>, device: u32) -> [bool; MAX_DEVICE_CHANNELS] {
    let mut selected = [false; MAX_DEVICE_CHANNELS];
    if let Some(project) = project {
        for node in project.graph.nodes.iter().filter(|n| n.kind == "output") {
            let interface = node.parameters.get("interface").copied().unwrap_or(0.) as u32;
            if interface != 0 && interface != device {
                continue;
            }
            if !project
                .graph
                .edges
                .iter()
                .any(|e| e.target == node.id && e.target_port == "in")
            {
                continue;
            }
            for (ch, key) in DEVICE_ROUTE_KEYS.iter().enumerate().take(node.channels) {
                if let Some(ch) =
                    device_channel(node.parameters.get(*key).copied().unwrap_or(-1.), ch, 0)
                {
                    selected[ch] = true;
                }
            }
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn all_physical_inputs_are_metered_without_graph_nodes_including_high_channels() {
        let mut meter = Meter::new(1, "64-channel interface".into(), 64);
        let mut frame = [0.; 64];
        frame[0] = 0.2;
        frame[8] = -0.8;
        frame[63] = 1.25;
        meter.observe(&frame);
        meter.observe(&[0.; 64]);
        let levels = meter.take(&[true; 64]);
        assert_eq!(levels.levels.len(), 64);
        assert_eq!(levels.levels[8].peak, 0.8);
        assert_eq!(levels.levels[63].peak, 1.25);
        assert!(meter.take(&[true; 64]).levels.iter().all(|l| l.peak == 0.));
    }
    #[test]
    fn output_groups_select_active_graph_routes_and_meter_the_combined_signal() {
        let mut project = pr0_core::demo_project("x".into(), "x".into(), pr0_core::Mode::Freeform);
        let out = project
            .graph
            .nodes
            .iter_mut()
            .find(|n| n.kind == "output")
            .unwrap();
        out.parameters.insert("interface".into(), 7.);
        out.parameters.insert("route_1".into(), 9.);
        out.parameters.insert("route_2".into(), 0.);
        let mut other = out.clone();
        other.id = "second".into();
        other.parameters.insert("interface".into(), 0.);
        project.graph.nodes.push(other);
        project.graph.edges.push(pr0_core::Edge {
            id: "second-wire".into(),
            source: "gain".into(),
            source_port: "out".into(),
            target: "second".into(),
            target_port: "in".into(),
        });
        let mask = output_channels(Some(&project), 7);
        assert_eq!(mask.iter().filter(|b| **b).count(), 1);
        assert!(mask[8]);
        assert!(output_channels(Some(&project), 8)[8]); // All-enabled interface route.
        assert!(output_channels(None, 7).iter().all(|b| !*b));
        let mut meter = Meter::new(7, "Output".into(), 16);
        let mut mixed = [0.; 64];
        mixed[8] = 0.75 + 0.75;
        meter.observe(&mixed);
        meter.observe(&[0.; 64]);
        let snapshot = meter.take(&mask);
        assert_eq!(snapshot.levels.len(), 1);
        assert_eq!(snapshot.levels[0].channel, 9);
        assert_eq!(snapshot.levels[0].peak, 1.5);
        // Channel sums must be metered after cancellation too, not as summed peaks.
        mixed[8] = 0.75 - 0.75;
        meter.observe(&mixed);
        assert_eq!(meter.take(&mask).levels[0].peak, 0.);
        project.graph.edges.clear();
        assert!(output_channels(Some(&project), 7).iter().all(|b| !*b));
    }
}
