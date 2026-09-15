//! Bounded, allocation-free bridge. JavaScript lives exclusively in pr0-server.
use crate::{
    Clock, midi_events,
    visualizer::{Datum, Text},
};
use pr0_core::{midi::Message, script::Script};
use rtrb::{Consumer, Producer, RingBuffer};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

pub const CAPACITY: usize = 1024;
pub const PENDING: usize = 256;
pub fn prepare_value(value: &pr0_core::ControlValue) -> Datum {
    Datum::prepare(Some(value))
}
pub fn serialize_value(value: Datum) -> pr0_core::ControlValue {
    match value {
        Datum::Number(v) => pr0_core::ControlValue::Number(v),
        Datum::Text(v) => pr0_core::ControlValue::Text(v.as_str().into()),
    }
}
#[derive(Clone, Copy)]
pub enum Kind {
    Clock {
        running: bool,
        click: bool,
        tick: bool,
        position: f64,
        unit: u8,
    },
    Input {
        port: usize,
        value: f64,
    },
    Midi(Message),
    Named {
        binding: usize,
        value: Datum,
    },
}
#[derive(Clone, Copy)]
pub struct Event {
    pub clock: Clock,
    pub kind: Kind,
}
#[derive(Clone, Copy)]
pub enum Action {
    Output { port: usize, value: f64 },
    Midi(Message),
    Named { binding: usize, value: Datum },
}
#[derive(Clone, Copy)]
pub struct Command {
    pub action: Action,
    pub sample: u64,
    pub beat: Option<f64>,
    pub generation: u64,
}
#[derive(Clone, Default)]
pub struct Diagnostics {
    pub error: Option<String>,
    pub logs: Vec<(u64, u64, String, String)>,
    pub events: u64,
}
pub struct Shared {
    pub alive: AtomicBool,
    pub fault: AtomicBool,
    pub dropped: AtomicU64,
    pub late: AtomicU64,
    pub diagnostics: Mutex<Diagnostics>,
}
pub struct Worker {
    pub events: Consumer<Event>,
    pub commands: Producer<Command>,
    /// OSC JSON is delivered only outside render, over a separate bounded queue.
    pub osc: std::sync::mpsc::Receiver<String>,
    pub shared: Arc<Shared>,
}
pub struct Binding {
    pub kind: String,
    pub name: Text,
    pub observed: Vec<Option<Datum>>,
    pub published: Option<Datum>,
    pub serial: u64,
}
pub struct Bridge {
    pub block_size: u64,
    pub config: Script,
    pub shared: Arc<Shared>,
    pub bindings: Vec<Binding>,
    pub osc: std::sync::mpsc::SyncSender<String>,
    events: Producer<Event>,
    commands: Consumer<Command>,
    pending: Box<[Option<Command>; PENDING]>,
    pending_count: usize,
    pending_order: [u64; PENDING],
    next_order: u64,
    pub inputs: [f64; 8],
    pub outputs: [f64; 9],
    previous: [Option<f64>; 8],
    last_click: Option<(u64, i64, u8)>,
    last_running: Option<bool>,
    generation: Option<u64>,
    fault_released: bool,
    held: [[bool; 128]; 16],
    sustain: [bool; 16],
}
impl Bridge {
    pub fn new(config: Script) -> (Self, Worker) {
        let (events, receive) = RingBuffer::new(CAPACITY);
        let (send, commands) = RingBuffer::new(CAPACITY);
        let (osc, osc_receive) = std::sync::mpsc::sync_channel(32);
        let shared = Arc::new(Shared {
            alive: AtomicBool::new(true),
            fault: AtomicBool::new(false),
            dropped: AtomicU64::new(0),
            late: AtomicU64::new(0),
            diagnostics: Mutex::new(Diagnostics::default()),
        });
        let bindings = config
            .bindings
            .iter()
            .map(|b| Binding {
                kind: b.kind.clone(),
                name: Text::new(&b.name),
                observed: vec![],
                published: None,
                serial: 0,
            })
            .collect();
        let mut inputs = [0.; 8];
        let mut outputs = [0.; 9];
        for (i, p) in config.inputs.iter().enumerate() {
            inputs[i] = p.initial;
        }
        for (i, p) in config.outputs.iter().enumerate() {
            outputs[i] = p.initial;
        }
        (
            Self {
                block_size: 128,
                config,
                shared: shared.clone(),
                bindings,
                osc,
                events,
                commands,
                pending: Box::new([None; PENDING]),
                pending_count: 0,
                pending_order: [0; PENDING],
                next_order: 0,
                inputs,
                outputs,
                previous: [None; 8],
                last_click: None,
                last_running: None,
                generation: None,
                fault_released: false,
                held: [[false; 128]; 16],
                sustain: [false; 16],
            },
            Worker {
                events: receive,
                commands: send,
                osc: osc_receive,
                shared,
            },
        )
    }
    fn overflow(&self) {
        self.shared.dropped.fetch_add(1, Ordering::Relaxed);
        self.shared.fault.store(true, Ordering::Release);
    }
    pub fn push(&mut self, clock: Clock, kind: Kind) {
        if !self.shared.fault.load(Ordering::Acquire)
            && self.events.push(Event { clock, kind }).is_err()
        {
            self.overflow();
        }
    }
    pub fn observe(
        &mut self,
        clock: Clock,
        binding: usize,
        source: usize,
        value: Datum,
        event: bool,
    ) {
        if self.bindings[binding].observed[source] != Some(value) || event {
            self.bindings[binding].observed[source] = Some(value);
            self.push(clock, Kind::Named { binding, value });
        }
    }
    pub fn release(&mut self, midi: &mut midi_events::Buffer) {
        let count = self.held.iter().flatten().filter(|held| **held).count()
            + self.sustain.iter().filter(|held| **held).count();
        // Do not let Buffer's overflow panic discard earlier sustain releases.
        // Large releases use channel panic messages instead of thousands of offs.
        let panic = count + midi.len > midi_events::CAPACITY;
        for ch in 0..16 {
            if panic && (self.sustain[ch] || self.held[ch].iter().any(|held| *held)) {
                midi.push(Message {
                    status: 0xb0 | ch as u8,
                    data1: 64,
                    data2: 0,
                });
                midi.push(Message {
                    status: 0xb0 | ch as u8,
                    data1: 123,
                    data2: 0,
                });
                self.sustain[ch] = false;
                self.held[ch].fill(false);
                continue;
            }
            if self.sustain[ch] {
                midi.push(Message {
                    status: 0xb0 | ch as u8,
                    data1: 64,
                    data2: 0,
                });
                self.sustain[ch] = false;
            }
            for pitch in 0..128 {
                if self.held[ch][pitch] {
                    midi.push(Message {
                        status: 0x80 | ch as u8,
                        data1: pitch as u8,
                        data2: 0,
                    });
                    self.held[ch][pitch] = false;
                }
            }
        }
    }
    pub fn tick(
        &mut self,
        clock: Clock,
        running: bool,
        meter: Option<(f64, f64, u8)>,
        values: &[[f64; 8]; 8],
        connected: [bool; 8],
        input_events: [bool; 8],
        midi: &mut midi_events::Buffer,
        passthrough: bool,
    ) {
        // `midi` arrives holding this block's incoming messages and leaves holding
        // the node's output. With passthrough the incoming messages stay in place
        // and the script's own output is appended after them; otherwise the script
        // consumes them and only what it sends goes out.
        if self.shared.fault.load(Ordering::Acquire) {
            if !passthrough {
                midi.clear();
            }
            if !self.fault_released {
                self.release(midi);
                self.fault_released = true;
            }
            self.outputs.fill(0.);
            self.pending.fill(None);
            self.pending_count = 0;
            for b in &mut self.bindings {
                b.published = None;
            }
            return;
        }
        for i in 0..midi.len {
            self.push(clock, Kind::Midi(midi.events[i]));
        }
        if !passthrough {
            midi.clear();
        }
        let reset = self.generation != Some(clock.reset_generation);
        if reset {
            self.pending.fill(None);
            self.pending_count = 0;
            self.release(midi);
            self.generation = Some(clock.reset_generation);
        }
        for i in 0..self.config.inputs.len() {
            let value = if connected[i] {
                values[i][0]
            } else {
                self.config.inputs[i].initial
            };
            let value = if value.is_finite() { value } else { 0. };
            self.inputs[i] = value;
            if self.previous[i] != Some(value) || input_events[i] {
                self.previous[i] = Some(value);
                self.push(clock, Kind::Input { port: i, value });
            }
        }
        let (position, origin, unit) =
            meter.unwrap_or((clock.beat, 0., (4. / clock.beat_length).round() as u8));
        let grid = (
            clock.reset_generation,
            ((position - origin) / (4. / unit.max(1) as f64)).floor() as i64,
            unit,
        );
        let click = self.last_click != Some(grid);
        let tick = clock.sample % self.block_size.max(1) == 0;
        if click || reset || self.last_running != Some(running) || tick {
            self.push(
                clock,
                Kind::Clock {
                    running,
                    click,
                    tick,
                    position,
                    unit,
                },
            );
            self.last_click = Some(grid);
            self.last_running = Some(running);
        }
        // Bound admission and work even if a worker produces a full queue.
        for _ in 0..32 {
            let Ok(cmd) = self.commands.pop() else { break };
            if cmd.generation != clock.reset_generation {
                continue;
            }
            if let Some(i) = self.pending.iter().position(|p| p.is_none()) {
                self.pending[i] = Some(cmd);
                self.pending_order[i] = self.next_order;
                self.next_order = self.next_order.wrapping_add(1);
                self.pending_count += 1;
            } else {
                self.overflow();
                break;
            }
        }
        for _ in 0..PENDING {
            if self.pending_count == 0 {
                break;
            }
            // Apply overdue events in timestamp order, then admission order. A
            // reused free slot must never reverse a note-on and its note-off.
            let next = self
                .pending
                .iter()
                .enumerate()
                .filter_map(|(i, p)| {
                    let cmd = (*p)?;
                    if cmd
                        .beat
                        .map_or(clock.sample < cmd.sample, |beat| clock.beat < beat)
                    {
                        return None;
                    }
                    let due = cmd.beat.map_or(cmd.sample as f64, |beat| {
                        clock.sample as f64
                            + (beat - clock.beat) * clock.sample_rate * 60. / clock.bpm
                    });
                    Some((i, due, self.pending_order[i]))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1).then_with(|| a.2.cmp(&b.2)));
            let Some((i, _, _)) = next else {
                break;
            };
            let cmd = self.pending[i].unwrap();
            self.pending[i] = None;
            self.pending_count -= 1;
            if cmd.beat.is_none() && clock.sample > cmd.sample {
                self.shared.late.fetch_add(1, Ordering::Relaxed);
            }
            match cmd.action {
                Action::Output { port, value }
                    if port < self.config.outputs.len() && value.is_finite() =>
                {
                    self.outputs[port] = value
                }
                Action::Midi(message) if message.valid() => {
                    if midi.len == midi_events::CAPACITY {
                        self.overflow();
                        break;
                    }
                    let ch = (message.status & 15) as usize;
                    match message.status >> 4 {
                        9 => self.held[ch][message.data1 as usize] = message.data2 > 0,
                        8 => self.held[ch][message.data1 as usize] = false,
                        11 if matches!(message.data1, 120 | 123) => self.held[ch].fill(false),
                        11 if message.data1 == 64 => self.sustain[ch] = message.data2 >= 64,
                        _ => {}
                    }
                    midi.push(message);
                }
                Action::Named { binding, value } if binding < self.bindings.len() => {
                    let b = &mut self.bindings[binding];
                    b.published = Some(value);
                    b.serial = b.serial.wrapping_add(1).max(1);
                }
                _ => {}
            }
        }
    }
}
impl Drop for Bridge {
    fn drop(&mut self) {
        self.shared.alive.store(false, Ordering::Release);
    }
}

impl crate::Engine {
    pub fn attach_script(&mut self, id: &str, mut bridge: Bridge) {
        for b in &mut bridge.bindings {
            b.observed.resize(self.nodes.len(), None);
        }
        if let Some(node) = self
            .nodes
            .iter_mut()
            .find(|n| n.id == id && n.kind == "js_control")
        {
            node.script = Some(Box::new(bridge));
        }
    }
    /// Off-render only: string payloads never enter the rendering queues.
    pub fn script_osc(&self, message: &str) -> bool {
        let mut accepted = false;
        for n in &self.nodes {
            if let Some(s) = &n.script {
                if s.osc.try_send(message.to_owned()).is_err() {
                    s.shared.dropped.fetch_add(1, Ordering::Relaxed);
                } else {
                    accepted = true;
                }
            }
        }
        accepted
    }
    pub fn script_status(&self) -> Vec<(String, Diagnostics, u64, u64, bool)> {
        self.nodes
            .iter()
            .filter_map(|n| {
                n.script.as_ref().map(|s| {
                    (
                        n.id.clone(),
                        s.shared.diagnostics.lock().unwrap().clone(),
                        s.shared.dropped.load(Ordering::Relaxed),
                        s.shared.late.load(Ordering::Relaxed),
                        s.shared.fault.load(Ordering::Acquire),
                    )
                })
            })
            .collect()
    }
    pub(crate) fn observe_scripts(&mut self) {
        for si in 0..self.script_nodes.len() {
            let idx = self.script_nodes[si];
            let Some(script) = &self.nodes[idx].script else {
                continue;
            };
            let count = script.bindings.len();
            for bi in 0..count {
                for ni in 0..self.nodes.len() {
                    let Some(route) = &self.nodes[ni].route else {
                        continue;
                    };
                    let binding = &self.nodes[idx].script.as_ref().unwrap().bindings[bi];
                    if route.signal != pr0_core::Signal::Control
                        || route.name != binding.name
                        || (binding.kind == "send") != route.send
                        || binding.kind == "publish"
                    {
                        continue;
                    }
                    let node = &self.nodes[ni];
                    if node.control_event_only && !node.control_event {
                        continue;
                    }
                    let value = node
                        .control_text
                        .map(Datum::Text)
                        .unwrap_or(Datum::Number(node.control[0]));
                    let event = node.control_event;
                    self.nodes[idx].script.as_mut().unwrap().observe(
                        self.graph_clock,
                        bi,
                        ni,
                        value,
                        event,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pr0_core::{
        Graph,
        script::{Binding as Spec, Input},
    };
    fn graph(config: &Script) -> Graph {
        let mut graph =
            pr0_core::demo_project("test".into(), "test".into(), pr0_core::Mode::Freeform).graph;
        let mut script = graph.nodes[0].clone();
        script.id = "s".into();
        script.kind = "js_control".into();
        script.parameters.clear();
        script.channels = 1;
        script.script = Some(config.clone());
        let mut receive = script.clone();
        receive.id = "r".into();
        receive.kind = "receive_control".into();
        receive.script = None;
        receive.control_value = Some(pr0_core::ControlValue::Text("bus".into()));
        receive.x = 500.;
        graph.nodes = vec![script, receive];
        graph.edges.clear();
        graph
    }
    fn config() -> Script {
        Script {
            outputs: vec![Input {
                name: "value".into(),
                initial: 0.,
            }],
            bindings: vec![Spec {
                kind: "publish".into(),
                name: "bus".into(),
            }],
            ..Script::default()
        }
    }
    fn engine(config: &Script) -> (crate::Engine, Worker) {
        let mut e = crate::Engine::prepare(graph(config), 48000.).unwrap();
        let (b, w) = Bridge::new(config.clone());
        e.attach_script("s", b);
        (e, w)
    }
    fn command(action: Action, sample: u64) -> Command {
        Command {
            action,
            sample,
            beat: None,
            generation: 0,
        }
    }
    #[test]
    fn scheduled_values_and_note_releases_apply_on_exact_samples() {
        let (mut e, mut w) = engine(&config());
        w.commands
            .push(command(Action::Output { port: 0, value: 9. }, 17))
            .unwrap_or_else(|_| panic!("queue"));
        w.commands
            .push(command(
                Action::Midi(Message {
                    status: 144,
                    data1: 60,
                    data2: 100,
                }),
                18,
            ))
            .unwrap_or_else(|_| panic!("queue"));
        for sample in 0..20 {
            e.render(&[], &mut [[0.; 8]]);
            assert_eq!(e.nodes[0].control[0], if sample >= 17 { 9. } else { 0. });
        }
        w.shared.fault.store(true, Ordering::Release);
        e.render(&[], &mut [[0.; 8]]);
        assert_eq!(e.nodes[0].control[0], 0.);
        assert_eq!(
            e.nodes[0].midi_frame.events[0],
            Message {
                status: 128,
                data1: 60,
                data2: 0
            }
        );
        e.render(&[], &mut [[0.; 8]]);
        assert_eq!(e.nodes[0].midi_frame.len, 0);
    }
    fn saw_midi(w: &mut Worker, expected: Message) -> bool {
        let mut seen = false;
        while let Ok(event) = w.events.pop() {
            if matches!(event.kind, Kind::Midi(m) if m == expected) {
                seen = true;
            }
        }
        seen
    }
    #[test]
    fn midi_passthrough_relays_input_and_appends_script_output() {
        let incoming = Message { status: 0x91, data1: 64, data2: 90 };
        let scripted = Message { status: 0x90, data1: 60, data2: 100 };
        // Default: the incoming message is relayed and the script's own output follows it.
        let (mut e, mut w) = engine(&config());
        w.commands
            .push(command(Action::Midi(scripted), 0))
            .unwrap_or_else(|_| panic!("queue"));
        e.node_midi_message("s", incoming);
        e.render(&[], &mut [[0.; 8]]);
        let frame = &e.nodes[0].midi_frame;
        assert_eq!(&frame.events[..frame.len], &[incoming, scripted]);
        // The script still observed the incoming message.
        assert!(saw_midi(&mut w, incoming));
        // Relaying continues while the worker has faulted.
        w.shared.fault.store(true, Ordering::Release);
        e.render(&[], &mut [[0.; 8]]);
        e.node_midi_message("s", incoming);
        e.render(&[], &mut [[0.; 8]]);
        let frame = &e.nodes[0].midi_frame;
        assert_eq!(&frame.events[..frame.len], &[incoming]);
        // Passthrough off: the script consumes incoming MIDI and only its output goes out.
        let mut graph = graph(&config());
        graph.nodes[0].parameters.insert("midi_passthru".into(), 0.);
        let mut e = crate::Engine::prepare(graph, 48000.).unwrap();
        let (b, mut w) = Bridge::new(config());
        e.attach_script("s", b);
        w.commands
            .push(command(Action::Midi(scripted), 0))
            .unwrap_or_else(|_| panic!("queue"));
        e.node_midi_message("s", incoming);
        e.render(&[], &mut [[0.; 8]]);
        let frame = &e.nodes[0].midi_frame;
        assert_eq!(&frame.events[..frame.len], &[scripted]);
        assert!(saw_midi(&mut w, incoming));
    }
    #[test]
    fn named_publication_reloads_without_stale_serials() {
        let c = config();
        let (mut e, mut w) = engine(&c);
        w.commands
            .push(command(
                Action::Named {
                    binding: 0,
                    value: Datum::Number(3.),
                },
                0,
            ))
            .unwrap_or_else(|_| panic!("queue"));
        e.render(&[], &mut [[0.; 8]; 2]);
        assert_eq!(e.nodes[1].control[0], 3.);
        let mut next = c.clone();
        next.revision = 1;
        let (mut replacement, mut worker) = engine(&next);
        replacement.carry_node_state(&mut e);
        worker
            .commands
            .push(command(
                Action::Named {
                    binding: 0,
                    value: Datum::Number(7.),
                },
                0,
            ))
            .unwrap_or_else(|_| panic!("queue"));
        replacement.render(&[], &mut [[0.; 8]; 2]);
        assert_eq!(replacement.nodes[1].control[0], 7.);
    }
    #[test]
    fn large_midi_release_preserves_sustain_off_without_buffer_overflow() {
        let (mut b, _w) = Bridge::new(config());
        b.held.fill([true; 128]);
        b.sustain.fill(true);
        let mut midi = midi_events::Buffer::new();
        b.release(&mut midi);
        assert_eq!(midi.len, 32);
        assert_eq!(midi.dropped, 0);
        for ch in 0..16 {
            assert_eq!(midi.events[ch * 2].data1, 64);
            assert_eq!(midi.events[ch * 2 + 1].data1, 123);
            assert!(!b.sustain[ch]);
            assert!(!b.held[ch].iter().any(|held| *held));
        }
    }
    #[test]
    fn overdue_commands_follow_time_order_not_reused_queue_slots() {
        let (mut e, mut w) = engine(&config());
        w.commands
            .push(command(Action::Output { port: 0, value: 1. }, 0))
            .unwrap_or_else(|_| panic!("queue"));
        w.commands
            .push(command(Action::Output { port: 0, value: 2. }, 10))
            .unwrap_or_else(|_| panic!("queue"));
        e.render(&[], &mut [[0.; 8]]);
        // This newer command reuses the first free slot but follows the older
        // command at the same timestamp.
        w.commands
            .push(command(Action::Output { port: 0, value: 3. }, 10))
            .unwrap_or_else(|_| panic!("queue"));
        e.render(&[], &mut [[0.; 8]; 10]);
        assert_eq!(e.nodes[0].control[0], 3.);
    }
    #[test]
    fn named_receiver_falls_back_when_active_script_faults() {
        let c = config();
        let mut g = graph(&c);
        let mut second = g.nodes[0].clone();
        second.id = "s2".into();
        second.x = 250.;
        g.nodes.push(second);
        let mut e = crate::Engine::prepare(g, 48000.).unwrap();
        let (first, mut w1) = Bridge::new(c.clone());
        let (second, mut w2) = Bridge::new(c);
        e.attach_script("s", first);
        e.attach_script("s2", second);
        w2.commands
            .push(command(
                Action::Named {
                    binding: 0,
                    value: Datum::Number(7.),
                },
                0,
            ))
            .unwrap_or_else(|_| panic!("queue"));
        e.render(&[], &mut [[0.; 8]; 2]);
        assert_eq!(e.nodes[1].control[0], 7.);
        w1.commands
            .push(command(
                Action::Named {
                    binding: 0,
                    value: Datum::Number(3.),
                },
                0,
            ))
            .unwrap_or_else(|_| panic!("queue"));
        e.render(&[], &mut [[0.; 8]; 2]);
        assert_eq!(e.nodes[1].control[0], 3.);
        w1.shared.fault.store(true, Ordering::Release);
        e.render(&[], &mut [[0.; 8]; 2]);
        assert_eq!(e.nodes[1].control[0], 7.);
    }
    #[test]
    fn unchanged_worker_survives_node_insertion_and_reordering() {
        let c = config();
        let (mut e, mut w) = engine(&c);
        let shared = w.shared.clone();
        w.commands
            .push(command(
                Action::Output {
                    port: 0,
                    value: 12.,
                },
                0,
            ))
            .unwrap_or_else(|_| panic!("queue"));
        e.render(&[], &mut [[0.; 8]; 2]);
        let mut g = graph(&c);
        let mut extra = g.nodes[1].clone();
        extra.id = "extra".into();
        g.nodes.push(extra);
        g.nodes.reverse();
        let mut replacement = crate::Engine::prepare(g, 48000.).unwrap();
        let (b, _new_worker) = Bridge::new(c);
        replacement.attach_script("s", b);
        replacement.carry_node_state(&mut e);
        let s = replacement
            .nodes
            .iter()
            .find(|n| n.id == "s")
            .unwrap()
            .script
            .as_ref()
            .unwrap();
        assert!(Arc::ptr_eq(&s.shared, &shared));
        assert_eq!(s.bindings[0].observed.len(), 3);
        replacement.render(&[], &mut [[0.; 8]; 2]);
        assert_eq!(
            replacement
                .nodes
                .iter()
                .find(|n| n.id == "s")
                .unwrap()
                .control[0],
            12.
        );
    }
    #[test]
    fn clock_reset_cancels_previous_generation_commands() {
        let (mut e, mut w) = engine(&config());
        w.commands
            .push(command(
                Action::Output {
                    port: 0,
                    value: 99.,
                },
                30,
            ))
            .unwrap_or_else(|_| panic!("queue"));
        e.render(&[], &mut [[0.; 8]; 4]);
        e.graph_clock.reset_generation = 1;
        e.render(&[], &mut [[0.; 8]; 40]);
        assert_eq!(e.nodes[0].control[0], 0.);
    }
}
