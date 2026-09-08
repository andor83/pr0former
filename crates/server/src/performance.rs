//! Musical scheduling is driven by engine beats; external I/O runs on a separate worker.
use pr0_core::Project;
use std::{
    collections::BTreeMap,
    sync::mpsc::{SyncSender, sync_channel},
};

#[derive(Clone, PartialEq)]
pub struct Event {
    pub beat: f64,
    pub note_id: u32,
    pub pitch: u8,
    pub velocity: u8,
}
pub struct Lane {
    pub id: String,
    owner: u64,
    active: Vec<Option<(u8, u8)>>,
    suspended: Vec<Option<(u8, u8)>>,
    ends: Vec<f64>,
    pub node: Option<String>,
    pub events: Vec<Event>,
    pub next: usize,
    pub cycle: u64,
    pub start: f64,
    pub length: f64,
    pub playing: bool,
    pending: Option<(f64, bool)>,
    pub midi: Option<String>,
    pub midi_channel: u8,
    pub osc: Option<String>,
    pub address: String,
}
#[derive(serde::Serialize)]
pub struct PartPlayback {
    id: String,
    playing: bool,
    start: f64,
    position: f64,
    pending: Option<(f64, bool)>,
}
pub struct Sequencer {
    autoplay: bool,
    pub lanes: Vec<Lane>,
    pub last_running: bool,
    pub last_beat: f64,
}
pub enum External {
    Note {
        midi: Option<String>,
        midi_channel: u8,
        osc: Option<String>,
        address: String,
        pitch: u8,
        velocity: u8,
    },
    Panic,
}
impl Sequencer {
    pub fn new(p: &Project) -> Self {
        let autoplay = matches!(p.mode, pr0_core::Mode::Structured);
        Self {
            autoplay,
            lanes: p
                .parts
                .iter()
                .enumerate()
                .map(|(owner, p)| {
                    let mut events = Vec::new();
                    for (note_id, n) in p.notes.iter().enumerate() {
                        if n.rest || n.velocity == 0 || n.beat >= p.loop_beats {
                            continue;
                        }
                        events.push(Event {
                            note_id: note_id as u32,
                            beat: n.beat,
                            pitch: n.pitch,
                            velocity: n.velocity,
                        });
                        events.push(Event {
                            note_id: note_id as u32,
                            beat: (n.beat + n.duration).min(p.loop_beats),
                            pitch: n.pitch,
                            velocity: 0,
                        });
                    }
                    events.sort_by(|a, b| {
                        a.beat.total_cmp(&b.beat).then(a.velocity.cmp(&b.velocity))
                    });
                    Lane {
                        id: p.id.clone(),
                        owner: owner as u64 + 1,
                        active: vec![None; p.notes.len()],
                        suspended: vec![None; p.notes.len()],
                        ends: p
                            .notes
                            .iter()
                            .map(|n| (n.beat + n.duration).min(p.loop_beats))
                            .collect(),
                        node: p.instrument_node.clone(),
                        events,
                        next: 0,
                        cycle: 0,
                        start: 0.,
                        length: p.loop_beats,
                        playing: autoplay,
                        pending: None,
                        midi: p.midi_port.clone(),
                        midi_channel: p.midi_channel,
                        osc: p.osc_destination.clone(),
                        address: p.osc_address.clone(),
                    }
                })
                .collect(),
            last_running: false,
            last_beat: 0.,
        }
    }
    /// Transfer compatible lanes without replaying their event history.
    /// Preparation and this transfer happen on the orchestration worker.
    pub fn replace(
        mut self,
        p: &Project,
        previous: &mut pr0_dsp::Engine,
        prepared: &mut pr0_dsp::Engine,
        io: &SyncSender<External>,
    ) -> Self {
        let mut next = Self::new(p);
        let mut owner = self.lanes.iter().map(|l| l.owner).max().unwrap_or(0) + 1;
        for lane in &mut next.lanes {
            lane.owner = owner;
            owner += 1;
            if let Some(index) = self.lanes.iter().position(|old| {
                old.id == lane.id
                    && old.node == lane.node
                    && old.events == lane.events
                    && old.ends == lane.ends
                    && old.length == lane.length
                    && old.midi == lane.midi
                    && old.midi_channel == lane.midi_channel
                    && old.osc == lane.osc
                    && old.address == lane.address
            }) {
                let old = self.lanes.remove(index);
                if let Some(node) = &old.node {
                    prepared.carry_note_voices(previous, node, old.owner);
                }
                *lane = old;
            }
        }
        // Changed/deleted parts release only their own external notes. A global
        // panic here would also cut compatible parts that are still playing.
        for lane in &mut self.lanes {
            lane.release(previous, io);
        }
        next.last_running = self.last_running;
        next.last_beat = previous.clock.beat;
        next
    }
    /// Seed new/reassigned control nodes with the part's currently held notes.
    pub fn seed_part_nodes(&self, previous: &pr0_dsp::Engine, prepared: &mut pr0_dsp::Engine) {
        for (node, part) in prepared.part_sources() {
            if previous.part_source(&node) == Some(part.as_str()) {
                continue;
            }
            if let Some(lane) = self.lanes.iter().find(|lane| lane.id == part) {
                for (pitch, velocity) in lane.active.iter().flatten() {
                    prepared.node_midi_note(&node, *pitch, *velocity);
                }
            }
        }
    }
    pub fn launch(
        &mut self,
        id: &str,
        playing: bool,
        engine: &mut pr0_dsp::Engine,
        io: &SyncSender<External>,
    ) {
        if let Some(l) = self.lanes.iter_mut().find(|l| l.id == id) {
            if engine.clock.running {
                l.pending = Some((engine.clock.beat.floor() + 1., playing));
            } else {
                l.apply(playing, engine.clock.beat, engine, io);
            }
        }
    }
    pub fn reset(&mut self, engine: &mut pr0_dsp::Engine, io: &SyncSender<External>) {
        for lane in &mut self.lanes {
            lane.release(engine, io);
            lane.suspended.fill(None);
            lane.next = 0;
            lane.cycle = 0;
            lane.start = 0.;
            lane.playing = self.autoplay;
            lane.pending = None;
        }
        self.last_beat = 0.;
        self.last_running = false;
    }
    pub fn pause(&mut self, engine: &mut pr0_dsp::Engine, io: &SyncSender<External>) {
        if self.last_running {
            for lane in &mut self.lanes {
                lane.suspended.copy_from_slice(&lane.active);
                lane.release(engine, io);
            }
        }
        self.last_running = false;
    }
    pub fn playback(&self, beat: f64) -> Vec<PartPlayback> {
        self.lanes
            .iter()
            .map(|l| PartPlayback {
                id: l.id.clone(),
                playing: l.playing,
                start: l.start,
                position: if l.playing {
                    (beat - l.start).max(0.).rem_euclid(l.length)
                } else {
                    0.
                },
                pending: l.pending,
            })
            .collect()
    }
    pub fn tick(&mut self, engine: &mut pr0_dsp::Engine, io: &SyncSender<External>) {
        let beat = engine.clock.beat;
        let running = engine.clock.running;
        if self.last_running && !running {
            self.pause(engine, io);
        }
        if beat < self.last_beat {
            for l in &mut self.lanes {
                l.release(engine, io);
                l.suspended.fill(None);
                l.next = 0;
                l.cycle = 0;
                l.start = 0.;
            }
        }
        let resuming = running && !self.last_running;
        self.last_beat = beat;
        self.last_running = running;
        if !running {
            return;
        }
        for lane in &mut self.lanes {
            if let Some((boundary, playing)) = lane.pending {
                if beat + 1e-9 >= boundary {
                    lane.apply(playing, boundary, engine, io);
                }
            }
            if !lane.playing || beat < lane.start || lane.events.is_empty() {
                continue;
            }
            if resuming {
                lane.resume(beat, engine, io);
            }
            // A bounded number of simultaneous events per sample prevents runaway patches.
            for _ in 0..256 {
                if lane.next >= lane.events.len() {
                    lane.next = 0;
                    lane.cycle += 1;
                }
                let e = &lane.events[lane.next];
                let due = lane.start + lane.cycle as f64 * lane.length + e.beat;
                if due > beat + 1e-9 {
                    break;
                }
                // Pause/stop may already have released this note. Do not emit a
                // second off, which could release another overlapping MIDI note.
                if e.velocity == 0 && lane.active[e.note_id as usize].is_none() {
                    lane.next += 1;
                    continue;
                }
                if let Some(node) = &lane.node {
                    engine.note_scoped(node, lane.owner, e.note_id, e.pitch, e.velocity);
                }
                engine.part_note(&lane.id, e.pitch, e.velocity);
                lane.active[e.note_id as usize] = if e.velocity > 0 {
                    Some((e.pitch, e.velocity))
                } else {
                    None
                };
                if lane.midi.is_some() || lane.osc.is_some() {
                    let _ = io.try_send(External::Note {
                        midi: lane.midi.clone(),
                        midi_channel: lane.midi_channel,
                        osc: lane.osc.clone(),
                        address: lane.address.clone(),
                        pitch: e.pitch,
                        velocity: e.velocity,
                    });
                }
                lane.next += 1;
            }
        }
    }
}
impl Lane {
    fn apply(
        &mut self,
        playing: bool,
        start: f64,
        engine: &mut pr0_dsp::Engine,
        io: &SyncSender<External>,
    ) {
        self.release(engine, io);
        self.suspended.fill(None);
        self.playing = playing;
        self.start = start;
        self.next = 0;
        self.cycle = 0;
        self.pending = None;
    }

    fn resume(&mut self, beat: f64, engine: &mut pr0_dsp::Engine, io: &SyncSender<External>) {
        for (note_id, held) in self.suspended.iter_mut().enumerate() {
            if let Some((pitch, velocity)) = held.take() {
                let end = self.start + self.cycle as f64 * self.length + self.ends[note_id];
                if end <= beat + 1e-9 {
                    continue;
                }
                if let Some(node) = &self.node {
                    engine.note_scoped(node, self.owner, note_id as u32, pitch, velocity);
                }
                engine.part_note(&self.id, pitch, velocity);
                self.active[note_id] = Some((pitch, velocity));
                if self.midi.is_some() || self.osc.is_some() {
                    let _ = io.try_send(External::Note {
                        midi: self.midi.clone(),
                        midi_channel: self.midi_channel,
                        osc: self.osc.clone(),
                        address: self.address.clone(),
                        pitch,
                        velocity,
                    });
                }
            }
        }
    }

    fn release(&mut self, engine: &mut pr0_dsp::Engine, io: &SyncSender<External>) {
        engine.part_notes_off(&self.id);
        for (note_id, active) in self.active.iter_mut().enumerate() {
            if let Some((pitch, _)) = active.take() {
                if let Some(node) = &self.node {
                    engine.note_scoped(node, self.owner, note_id as u32, pitch, 0);
                }
                if self.midi.is_some() || self.osc.is_some() {
                    let _ = io.try_send(External::Note {
                        midi: self.midi.clone(),
                        midi_channel: self.midi_channel,
                        osc: self.osc.clone(),
                        address: self.address.clone(),
                        pitch,
                        velocity: 0,
                    });
                }
            }
        }
    }
}
// MIDI 1.0 and the pitch/velocity OSC contract cannot identify independent
// voices. Hold a shared pitch until its last scheduled owner releases it.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Route {
    Midi(String, u8),
    Osc(std::net::SocketAddr, String),
}
#[derive(Default)]
struct HeldNotes(BTreeMap<(Route, u8), u32>);
impl HeldNotes {
    fn update(&mut self, route: Route, pitch: u8, velocity: u8) -> bool {
        let key = (route, pitch);
        if velocity > 0 {
            let count = self.0.entry(key).or_default();
            *count += 1;
            *count == 1
        } else if let Some(count) = self.0.get_mut(&key) {
            *count -= 1;
            if *count == 0 {
                self.0.remove(&key);
                true
            } else {
                false
            }
        } else {
            false
        }
    }
}
fn send_osc(
    socket: &crate::osc::Runtime,
    destination: std::net::SocketAddr,
    address: &str,
    pitch: u8,
    velocity: u8,
) {
    {
        let packet = rosc::OscPacket::Message(rosc::OscMessage {
            addr: address.into(),
            args: vec![
                rosc::OscType::Int(pitch as i32),
                rosc::OscType::Int(velocity as i32),
            ],
        });
        if let Ok(bytes) = rosc::encoder::encode(&packet) {
            socket.send(&bytes, destination);
        }
    }
}
pub fn external_worker(socket: std::sync::Arc<crate::osc::Runtime>) -> SyncSender<External> {
    let (tx, rx) = sync_channel(4096);
    std::thread::Builder::new()
        .name("pr0-midi-osc".into())
        .spawn(move || {
            let mut midi: BTreeMap<String, midir::MidiOutputConnection> = BTreeMap::new();
            let mut held = HeldNotes::default();
            while let Ok(event) = rx.recv() {
                match event {
                    External::Note {
                        midi: port,
                        midi_channel,
                        osc,
                        address,
                        pitch,
                        velocity,
                    } => {
                        if let Some(port) = port.filter(|port| {
                            held.update(Route::Midi(port.clone(), midi_channel), pitch, velocity)
                        }) {
                            if !midi.contains_key(&port) {
                                if let Ok(output) = midir::MidiOutput::new("pr0former") {
                                    if let Some(p) = output.ports().iter().find(|p| {
                                        output.port_name(p).ok().as_deref() == Some(&port)
                                    }) {
                                        if let Ok(c) = output.connect(p, "pr0former performance") {
                                            midi.insert(port.clone(), c);
                                        }
                                    }
                                }
                            }
                            if let Some(c) = midi.get_mut(&port) {
                                let _ = c.send(&[
                                    (if velocity == 0 { 0x80 } else { 0x90 }) | (midi_channel - 1),
                                    pitch,
                                    velocity,
                                ]);
                            }
                        }
                        if let Some(destination) =
                            osc.and_then(|d| d.parse::<std::net::SocketAddr>().ok())
                        {
                            if held.update(
                                Route::Osc(destination, address.clone()),
                                pitch,
                                velocity,
                            ) {
                                send_osc(&socket, destination, &address, pitch, velocity);
                            }
                        }
                    }
                    External::Panic => {
                        for ((route, pitch), _) in &held.0 {
                            if let Route::Osc(destination, address) = route {
                                send_osc(&socket, *destination, address, *pitch, 0);
                            }
                        }
                        held.0.clear();
                        for c in midi.values_mut() {
                            for channel in 0..16 {
                                let _ = c.send(&[0xb0 | channel, 123, 0]);
                            }
                        }
                    }
                }
            }
        })
        .expect("Start MIDI/OSC worker");
    tx
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn part_control_streams_isolate_chords_and_reassignment_releases_old_pitches() {
        let mut p = pr0_core::demo_project("x".into(), "x".into(), pr0_core::Mode::Structured);
        p.parts[0].notes.truncate(2);
        p.parts[0].notes[0].pitch = 60;
        p.parts[0].notes[0].duration = 2.;
        p.parts[0].notes[1].pitch = 64;
        p.parts[0].notes[1].beat = 0.;
        p.parts[0].notes[1].duration = 2.;
        let mut other = p.parts[0].clone();
        other.id = "other".into();
        other.notes.truncate(1);
        other.notes[0].pitch = 72;
        p.parts.push(other);
        let mut node = p.graph.nodes[0].clone();
        node.id = "notes".into();
        node.kind = "part_midi".into();
        node.parameters.clear();
        node.part_id = Some(p.parts[0].id.clone());
        p.graph.nodes.push(node);
        let mut e = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        e.clock.running = true;
        let mut seq = Sequencer::new(&p);
        let (tx, _) = sync_channel(256);
        let mut events = vec![];
        for _ in 0..4 {
            seq.tick(&mut e, &tx);
            e.render(&[], &mut [[0.; 8]]);
            let v = e.telemetry().remove("notes").unwrap();
            if v["trigger"] > 0. {
                events.push(v["pitch"]);
            }
        }
        assert_eq!(events, vec![60., 64.]);
        p.graph.nodes.last_mut().unwrap().part_id = Some("other".into());
        let mut prepared = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        prepared.clock = e.clock;
        let mut next = seq.replace(&p, &mut e, &mut prepared, &tx);
        prepared.carry_node_state(&mut e);
        next.seed_part_nodes(&e, &mut prepared);
        let mut changes = vec![];
        for _ in 0..8 {
            next.tick(&mut prepared, &tx);
            prepared.render(&[], &mut [[0.; 8]]);
            let v = prepared.telemetry().remove("notes").unwrap();
            if v["trigger"] + v["note_off"] > 0. {
                changes.push((v["pitch"], v["velocity"]));
            }
        }
        assert_eq!(changes, vec![(60., 0.), (64., 0.), (72., 90.)]);
        next.pause(&mut prepared, &tx);
        prepared.clock.running = false;
        for _ in 0..4 {
            prepared.render(&[], &mut [[0.; 8]]);
        }
        assert_eq!(prepared.telemetry()["notes"]["gate"], 0.);
    }
    #[test]
    fn external_midi_channels_hold_the_same_pitch_independently() {
        let mut held = HeldNotes::default();
        let one = Route::Midi("port".into(), 1);
        let two = Route::Midi("port".into(), 2);
        assert!(held.update(one.clone(), 60, 90));
        assert!(held.update(two.clone(), 60, 90));
        assert!(held.update(one, 60, 0));
        assert_eq!(held.0.len(), 1);
        assert!(held.update(two, 60, 0));
        assert!(held.0.is_empty());
    }
    #[test]
    fn graph_replacement_preserves_lanes_cues_and_note_offs() {
        let mut p = pr0_core::demo_project("test".into(), "Test".into(), pr0_core::Mode::Freeform);
        p.parts[0].notes.truncate(1);
        p.parts[0].midi_port = Some("test sink".into());
        let mut removed = p.parts[0].clone();
        removed.id = "removed".into();
        removed.notes[0].pitch = 67;
        p.parts.push(removed);
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        engine.clock.beat = 40.;
        let mut seq = Sequencer::new(&p);
        let (tx, rx) = sync_channel(64);
        for part in &p.parts {
            seq.launch(&part.id, true, &mut engine, &tx);
        }
        engine.clock.running = true;
        seq.tick(&mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 2);
        engine.clock.beat = 40.25;
        seq.launch(&p.parts[0].id, false, &mut engine, &tx);
        p.parts.pop();
        let mut added = p.parts[0].clone();
        added.id = "new".into();
        p.parts.insert(0, added);
        let mut prepared = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        prepared.clock = engine.clock;
        let mut next = seq.replace(&p, &mut engine, &mut prepared, &tx);
        assert!(matches!(
            rx.try_recv().unwrap(),
            External::Note {
                pitch: 67,
                velocity: 0,
                ..
            }
        ));
        assert!(!next.lanes[0].playing);
        assert_ne!(next.lanes[0].owner, next.lanes[1].owner);
        assert_eq!(next.playback(40.25)[1].position, 0.25);
        assert_eq!(next.playback(40.25)[1].pending, Some((41., false)));
        next.tick(&mut prepared, &tx);
        assert_eq!(
            rx.try_iter().count(),
            0,
            "no old events replayed on replacement"
        );
        prepared.clock.beat = 40.75;
        next.tick(&mut prepared, &tx);
        assert!(matches!(
            rx.try_recv().unwrap(),
            External::Note {
                pitch: 60,
                velocity: 0,
                ..
            }
        ));
        prepared.clock.beat = 41.;
        next.tick(&mut prepared, &tx);
        assert!(!next.lanes[1].playing);
        assert_eq!(rx.try_iter().count(), 0);
    }

    #[test]
    fn resume_restores_held_notes_once_and_preserves_their_release_time() {
        let mut p =
            pr0_core::demo_project("test".into(), "Test".into(), pr0_core::Mode::Structured);
        p.parts[0].midi_port = Some("test sink".into());
        p.parts[0].notes.truncate(1);
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        let mut seq = Sequencer::new(&p);
        let (tx, rx) = sync_channel(64);
        engine.clock.running = true;
        seq.tick(&mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 1);
        engine.clock.beat = 0.25;
        // The command path must work even if pause/play arrive in one block.
        seq.pause(&mut engine, &tx);
        assert!(matches!(
            rx.try_recv().unwrap(),
            External::Note { velocity: 0, .. }
        ));
        seq.pause(&mut engine, &tx);
        seq.tick(&mut engine, &tx);
        assert!(matches!(
            rx.try_recv().unwrap(),
            External::Note { velocity: 90, .. }
        ));
        seq.tick(&mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 0);
        engine.clock.beat = 0.75;
        seq.tick(&mut engine, &tx);
        assert!(matches!(
            rx.try_recv().unwrap(),
            External::Note { velocity: 0, .. }
        ));
        seq.pause(&mut engine, &tx);
        seq.tick(&mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 0, "expired notes must not restart");
    }

    #[test]
    fn paused_part_stop_cancels_suspended_notes_and_pending_cues() {
        let mut p = pr0_core::demo_project("test".into(), "Test".into(), pr0_core::Mode::Freeform);
        p.parts[0].midi_port = Some("test sink".into());
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        let mut seq = Sequencer::new(&p);
        let (tx, rx) = sync_channel(64);
        seq.launch(&p.parts[0].id, true, &mut engine, &tx);
        engine.clock.running = true;
        seq.tick(&mut engine, &tx);
        engine.clock.beat = 0.25;
        seq.launch(&p.parts[0].id, true, &mut engine, &tx);
        seq.pause(&mut engine, &tx);
        engine.clock.running = false;
        seq.tick(&mut engine, &tx);
        assert_eq!(seq.playback(0.25)[0].pending, Some((1., true)));
        seq.launch(&p.parts[0].id, false, &mut engine, &tx);
        rx.try_iter().for_each(drop);
        engine.clock.running = true;
        engine.clock.beat = 1.;
        seq.tick(&mut engine, &tx);
        assert!(!seq.playback(1.)[0].playing);
        assert!(seq.playback(1.)[0].pending.is_none());
        assert_eq!(rx.try_iter().count(), 0);
    }

    #[test]
    fn independent_modes_wait_for_launch_and_reset_to_idle() {
        for mode in [pr0_core::Mode::Conducted, pr0_core::Mode::Freeform] {
            let mut p = pr0_core::demo_project("test".into(), "Test".into(), mode);
            p.parts[0].midi_port = Some("test sink".into());
            let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
            engine.clock.running = true;
            let mut seq = Sequencer::new(&p);
            let (tx, rx) = sync_channel(64);
            engine.clock.beat = 5.25;
            seq.tick(&mut engine, &tx);
            assert_eq!(rx.try_iter().count(), 0);
            seq.launch(&p.parts[0].id, true, &mut engine, &tx);
            assert_eq!(seq.playback(5.25)[0].pending, Some((6., true)));
            engine.clock.beat = 5.99;
            seq.tick(&mut engine, &tx);
            assert_eq!(rx.try_iter().count(), 0);
            engine.clock.beat = 6.;
            seq.tick(&mut engine, &tx);
            assert!(matches!(
                rx.try_recv().unwrap(),
                External::Note { velocity: 90, .. }
            ));
            assert_eq!(seq.playback(6.25)[0].position, 0.25);
            assert!(seq.playback(6.25)[0].pending.is_none());
            seq.reset(&mut engine, &tx);
            assert!(!seq.playback(0.)[0].playing);
        }
    }

    #[test]
    fn osc_panic_releases_the_shared_pitch_on_the_wire() {
        let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let tx = external_worker(std::sync::Arc::new(crate::osc::Runtime::default()));
        for velocity in [90, 70, 0] {
            tx.send(External::Note {
                midi: None,
                midi_channel: 1,
                osc: Some(receiver.local_addr().unwrap().to_string()),
                address: "/note".into(),
                pitch: 60,
                velocity,
            })
            .unwrap();
        }
        tx.send(External::Panic).unwrap();
        let mut buffer = [0; 1024];
        for velocity in [90, 0] {
            let n = receiver.recv(&mut buffer).unwrap();
            let (_, packet) = rosc::decoder::decode_udp(&buffer[..n]).unwrap();
            let rosc::OscPacket::Message(message) = packet else {
                panic!("expected note");
            };
            assert_eq!(message.addr, "/note");
            assert_eq!(
                message.args,
                vec![rosc::OscType::Int(60), rosc::OscType::Int(velocity)]
            );
        }
    }
    #[test]
    fn external_shared_pitch_stays_held_until_last_release() {
        let routes = [
            Route::Midi("test".into(), 1),
            Route::Midi("test".into(), 2),
            Route::Osc("127.0.0.1:9000".parse().unwrap(), "/note".into()),
        ];
        let mut held = HeldNotes::default();
        for route in &routes {
            assert!(held.update(route.clone(), 60, 90));
            assert!(!held.update(route.clone(), 60, 70));
            assert!(held.update(route.clone(), 64, 90));
            assert!(!held.update(route.clone(), 60, 0));
            assert!(held.update(route.clone(), 60, 0));
            assert!(!held.update(route.clone(), 60, 0));
            assert!(held.update(route.clone(), 64, 0));
        }
        assert!(held.0.is_empty());
    }

    #[test]
    fn stop_and_relaunch_release_only_held_notes() {
        let mut p =
            pr0_core::demo_project("test".into(), "Test".into(), pr0_core::Mode::Structured);
        p.parts[0].midi_port = Some("test sink".into());
        p.parts[0].notes.truncate(1);
        p.parts[0].notes[0].duration = 4.;
        let mut second = p.parts[0].clone();
        second.id = "second".into();
        p.parts.push(second);
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        engine.clock.running = true;
        let mut seq = Sequencer::new(&p);
        let (tx, rx) = sync_channel(64);
        seq.tick(&mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 2);
        seq.launch(&p.parts[0].id, false, &mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 0, "stop waits for the boundary");
        assert!(seq.lanes[0].playing);
        engine.clock.beat = 1.;
        seq.tick(&mut engine, &tx);
        let offs: Vec<_> = rx.try_iter().collect();
        assert_eq!(offs.len(), 1, "only the currently held note gets an off");
        assert!(matches!(
            offs[0],
            External::Note {
                pitch: 60,
                velocity: 0,
                ..
            }
        ));
        assert!(seq.lanes[0].active.iter().all(Option::is_none));
        assert!(seq.lanes[1].active.iter().any(Option::is_some));
        seq.launch("second", true, &mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 0);
        engine.clock.beat = 2.;
        seq.tick(&mut engine, &tx);
        assert_eq!(
            rx.try_iter().count(),
            2,
            "relaunch releases and starts an instance"
        );
    }

    #[test]
    fn reset_at_zero_replays_the_initial_event_without_a_stuck_note() {
        let mut p =
            pr0_core::demo_project("test".into(), "Test".into(), pr0_core::Mode::Structured);
        p.parts[0].midi_port = Some("test sink".into());
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        engine.clock.running = true;
        let mut seq = Sequencer::new(&p);
        let (tx, rx) = sync_channel(64);
        seq.tick(&mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 1);
        seq.reset(&mut engine, &tx);
        engine.clock.stop();
        assert_eq!(rx.try_iter().count(), 1);
        engine.clock.running = true;
        seq.tick(&mut engine, &tx);
        assert!(matches!(
            rx.try_recv().unwrap(),
            External::Note { velocity: 90, .. }
        ));
        engine.clock.running = false;
        seq.tick(&mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 1);
        engine.clock.running = true;
        engine.clock.beat = 0.8;
        seq.tick(&mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 0, "pause already released the note");
    }

    #[test]
    fn scheduled_note_on_and_off_follow_engine_time() {
        let mut p =
            pr0_core::demo_project("test".into(), "Test".into(), pr0_core::Mode::Structured);
        p.parts[0].midi_port = Some("test sink".into());
        p.parts[0].notes.truncate(1);
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        engine.clock.running = true;
        let mut sequencer = Sequencer::new(&p);
        let (tx, rx) = sync_channel(16);
        sequencer.tick(&mut engine, &tx);
        assert!(matches!(
            rx.try_recv().unwrap(),
            External::Note {
                pitch: 60,
                velocity: 90,
                ..
            }
        ));
        for _ in 0..18002 {
            engine.render(&[], &mut [[0.; 8]; 1]);
            sequencer.tick(&mut engine, &tx);
        }
        assert!(matches!(
            rx.try_recv().unwrap(),
            External::Note {
                pitch: 60,
                velocity: 0,
                ..
            }
        ));
        assert!(rx.try_recv().is_err());
    }
}
