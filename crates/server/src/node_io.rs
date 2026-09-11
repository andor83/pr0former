//! Physical MIDI input callbacks only push fixed messages into bounded SPSC rings.
//! Output connections and MIDI/OSC serialization run on a separate worker.
use pr0_core::{Graph, Node};
use pr0_dsp::{Engine, note_inputs::NoteEvent};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::{SyncSender, sync_channel},
    },
};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Route {
    Midi(String, u8),
    Osc(std::net::SocketAddr, String),
    /// Single-argument OSC value route with its maximum send rate in Hz.
    OscValue(std::net::SocketAddr, String, u32),
}
impl Route {
    pub fn from_node(node: &Node) -> Option<Self> {
        let io = node.io.as_ref()?;
        match node.kind.as_str() {
            "midi_output" if !io.port.is_empty() => Some(Self::Midi(
                io.port.clone(),
                node.parameters.get("channel").copied().unwrap_or(1.) as u8,
            )),
            "midi_to_osc" if !io.address.is_empty() => io
                .destination
                .parse()
                .ok()
                .map(|destination| Self::Osc(destination, io.address.clone())),
            "osc_output" if !io.address.is_empty() => {
                io.destination.parse().ok().map(|destination| {
                    Self::OscValue(
                        destination,
                        io.address.clone(),
                        node.parameters
                            .get("rate")
                            .copied()
                            .unwrap_or(60.)
                            .clamp(1., 200.) as u32,
                    )
                })
            }
            _ => None,
        }
    }
}
struct OutputEvent {
    node: String,
    route: Route,
    note: NoteEvent,
    cc: bool,
}
enum Command {
    Midi {
        node: String,
        route: Route,
        message: pr0_core::midi::Message,
    },
    Event(OutputEvent),
    Value {
        route: Route,
        value: pr0_core::ControlValue,
    },
    Reset(Vec<String>),
}
type Held = BTreeMap<(String, Route, u8), u32>;
pub struct Outputs {
    tx: SyncSender<Command>,
    panic: Arc<AtomicBool>,
    dropped: Arc<AtomicU64>,
    error: Arc<Mutex<Option<String>>>,
}
impl Outputs {
    pub fn new(osc: Arc<crate::osc::Runtime>) -> Self {
        let (tx, rx) = sync_channel(4096);
        let panic = Arc::new(AtomicBool::new(false));
        let reset = panic.clone();
        let dropped = Arc::new(AtomicU64::new(0));
        let worker_dropped = dropped.clone();
        let error = Arc::new(Mutex::new(None));
        let worker_error = error.clone();
        std::thread::spawn(move || {
            let mut midi = BTreeMap::<String, midir::MidiOutputConnection>::new();
            let mut held = Held::new();
            while let Ok(command) = rx.recv() {
                if reset.swap(false, Ordering::AcqRel) {
                    while rx.try_recv().is_ok() {}
                    release(&mut held, &[], &mut midi, &osc, &worker_error);
                    continue;
                }
                let command = match command {
                    Command::Midi {
                        node,
                        route,
                        message,
                    } => {
                        let kind = message.status >> 4;
                        if kind == 11 && matches!(message.data1, 120 | 123) {
                            release(&mut held, &[node.clone()], &mut midi, &osc, &worker_error);
                        }
                        let route = match route {
                            Route::Midi(port, _) => Route::Midi(port, (message.status & 15) + 1),
                            other => other,
                        };
                        if matches!(kind, 8 | 9) {
                            Command::Event(OutputEvent {
                                node,
                                route,
                                note: NoteEvent {
                                    pitch: message.data1,
                                    velocity: if kind == 8 { 0 } else { message.data2 },
                                },
                                cc: false,
                            })
                        } else {
                            if let Err(e) = send_message(&route, message, &mut midi) {
                                *worker_error.lock().unwrap() = Some(e)
                            };
                            continue;
                        }
                    }
                    other => other,
                };
                match command {
                    Command::Midi { .. } => unreachable!(),
                    Command::Reset(nodes) => {
                        release(&mut held, &nodes, &mut midi, &osc, &worker_error)
                    }
                    Command::Value { route, value } => {
                        if let Err(e) = send_value(&route, &value, &osc) {
                            *worker_error.lock().unwrap() = Some(e);
                        }
                    }
                    Command::Event(event) => {
                        if !event.cc {
                            let key = (event.node, event.route.clone(), event.note.pitch);
                            if event.note.velocity > 0 {
                                if held.get(&key).copied().unwrap_or(0) >= 64 {
                                    worker_dropped.fetch_add(1, Ordering::Relaxed);
                                    release(
                                        &mut held,
                                        &[key.0.clone()],
                                        &mut midi,
                                        &osc,
                                        &worker_error,
                                    );
                                    *worker_error.lock().unwrap()=Some("Too many overlapping identical MIDI notes; node notes released".into());
                                    continue;
                                }
                                *held.entry(key).or_default() += 1;
                            } else if let Some(count) = held.get_mut(&key) {
                                *count -= 1;
                                if *count == 0 {
                                    held.remove(&key);
                                }
                            } else {
                                continue;
                            }
                        }
                        if let Err(e) = send(&event.route, event.note, event.cc, &mut midi, &osc) {
                            *worker_error.lock().unwrap() = Some(e);
                        }
                    }
                }
            }
            release(&mut held, &[], &mut midi, &osc, &worker_error);
        });
        Self {
            tx,
            panic,
            dropped,
            error,
        }
    }
    pub fn note(&self, node: &str, route: &Route, note: NoteEvent, cc: bool) {
        if self
            .tx
            .try_send(Command::Event(OutputEvent {
                node: node.into(),
                route: route.clone(),
                note,
                cc,
            }))
            .is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            self.panic.store(true, Ordering::Release);
        }
    }
    pub fn message(&self, node: &str, route: &Route, message: pr0_core::midi::Message) {
        if self
            .tx
            .try_send(Command::Midi {
                node: node.into(),
                route: route.clone(),
                message,
            })
            .is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            self.panic.store(true, Ordering::Release);
        }
    }
    /// Send one OSC value; the orchestration worker applies the node's rate limit.
    pub fn value(&self, route: &Route, value: pr0_core::ControlValue) {
        if self
            .tx
            .try_send(Command::Value {
                route: route.clone(),
                value,
            })
            .is_err()
        {
            self.dropped.fetch_add(1, Ordering::Relaxed);
        }
    }
    pub fn reset(&self, nodes: Vec<String>) {
        if self.tx.try_send(Command::Reset(nodes)).is_err() {
            self.panic.store(true, Ordering::Release);
        }
    }
    pub fn status(&self) -> serde_json::Value {
        serde_json::json!({"dropped":self.dropped.load(Ordering::Relaxed),"error":*self.error.lock().unwrap()})
    }
}
fn release(
    held: &mut Held,
    nodes: &[String],
    midi: &mut BTreeMap<String, midir::MidiOutputConnection>,
    osc: &crate::osc::Runtime,
    error: &Mutex<Option<String>>,
) {
    let keys: Vec<_> = held
        .keys()
        .filter(|(node, _, _)| nodes.is_empty() || nodes.contains(node))
        .cloned()
        .collect();
    for key in keys {
        let (_, route, pitch) = &key;
        let count = held.get(&key).copied().unwrap_or(0);
        for _ in 0..count {
            if let Err(e) = send(
                route,
                NoteEvent {
                    pitch: *pitch,
                    velocity: 0,
                },
                false,
                midi,
                osc,
            ) {
                *error.lock().unwrap() = Some(e);
            }
        }
        held.remove(&key);
    }
}
fn send_message(
    route: &Route,
    message: pr0_core::midi::Message,
    midi: &mut BTreeMap<String, midir::MidiOutputConnection>,
) -> Result<(), String> {
    let Route::Midi(port, _) = route else {
        return Err("Typed MIDI requires a MIDI output route".into());
    };
    if !midi.contains_key(port) {
        if std::env::var_os("PR0_DISABLE_NATIVE_DEVICES").is_some() {
            return Err("Native MIDI disabled for this server".into());
        }
        let output = midir::MidiOutput::new("pr0former score").map_err(|e| e.to_string())?;
        let target = output
            .ports()
            .into_iter()
            .find(|p| output.port_name(p).ok().as_ref() == Some(port))
            .ok_or_else(|| format!("MIDI output unavailable: {port}"))?;
        midi.insert(
            port.clone(),
            output
                .connect(&target, "pr0former score output")
                .map_err(|e| e.to_string())?,
        );
    }
    let (bytes, len) = message.bytes();
    midi.get_mut(port)
        .unwrap()
        .send(&bytes[..len])
        .map_err(|e| e.to_string())
}
fn send_value(
    route: &Route,
    value: &pr0_core::ControlValue,
    osc: &crate::osc::Runtime,
) -> Result<(), String> {
    let Route::OscValue(destination, address, _) = route else {
        return Err("OSC values require an OSC output route".into());
    };
    let packet = rosc::OscPacket::Message(rosc::OscMessage {
        addr: address.clone(),
        args: vec![match value {
            pr0_core::ControlValue::Number(number) => rosc::OscType::Float(*number as f32),
            pr0_core::ControlValue::Text(text) => rosc::OscType::String(text.clone()),
        }],
    });
    let bytes = rosc::encoder::encode(&packet).map_err(|e| e.to_string())?;
    osc.send(&bytes, *destination);
    Ok(())
}
fn send(
    route: &Route,
    note: NoteEvent,
    cc: bool,
    midi: &mut BTreeMap<String, midir::MidiOutputConnection>,
    osc: &crate::osc::Runtime,
) -> Result<(), String> {
    match route {
        Route::OscValue(..) => Err("OSC value routes carry no notes".into()),
        Route::Midi(port, channel) => {
            if !midi.contains_key(port) {
                if std::env::var_os("PR0_DISABLE_NATIVE_DEVICES").is_some() {
                    return Err("Native MIDI disabled for this server".into());
                }
                let output =
                    midir::MidiOutput::new("pr0former graph").map_err(|e| e.to_string())?;
                let target = output
                    .ports()
                    .into_iter()
                    .find(|p| output.port_name(p).ok().as_ref() == Some(port))
                    .ok_or_else(|| format!("MIDI output unavailable: {port}"))?;
                let connection = output
                    .connect(&target, "pr0former graph output")
                    .map_err(|e| e.to_string())?;
                midi.insert(port.clone(), connection);
            }
            midi.get_mut(port)
                .unwrap()
                .send(&[
                    (if cc {
                        0xb0
                    } else if note.velocity == 0 {
                        0x80
                    } else {
                        0x90
                    }) | (channel - 1),
                    note.pitch,
                    note.velocity,
                ])
                .map_err(|e| e.to_string())
        }
        Route::Osc(destination, address) => {
            let packet = rosc::OscPacket::Message(rosc::OscMessage {
                addr: address.clone(),
                args: vec![
                    rosc::OscType::Int(note.pitch.into()),
                    rosc::OscType::Int(note.velocity.into()),
                ],
            });
            let bytes = rosc::encoder::encode(&packet).map_err(|e| e.to_string())?;
            osc.send(&bytes, *destination);
            Ok(())
        }
    }
}

struct Input {
    port: String,
    _connection: midir::MidiInputConnection<()>,
    queue: rtrb::Consumer<[u8; 3]>,
    dropped: Arc<AtomicU64>,
    last_dropped: u64,
}
#[derive(Default)]
pub struct Inputs {
    inputs: Vec<Input>,
    pub error: Option<String>,
}
impl Inputs {
    pub fn configure(&mut self, graph: &Graph) {
        let mut ports: Vec<_> = graph
            .nodes
            .iter()
            .filter(|n| n.kind == "midi_input")
            .filter_map(|n| n.io.as_ref())
            .map(|io| io.port.clone())
            .filter(|p| !p.is_empty())
            .collect();
        ports.sort();
        ports.dedup();
        self.inputs.retain(|i| ports.contains(&i.port));
        self.error = None;
        for port in ports {
            if self.inputs.iter().any(|i| i.port == port) {
                continue;
            }
            match open_input(&port) {
                Ok(input) => self.inputs.push(input),
                Err(e) => self.error = Some(e),
            }
        }
    }
    pub fn drain(&mut self, graph: &Graph, engine: &mut Engine) {
        for input in &mut self.inputs {
            let dropped = input.dropped.load(Ordering::Relaxed);
            if dropped != input.last_dropped {
                input.last_dropped = dropped;
                while input.queue.pop().is_ok() {}
                for node in &graph.nodes {
                    if node.kind == "midi_input"
                        && node.io.as_ref().is_some_and(|io| io.port == input.port)
                    {
                        engine.node_midi_reset(&node.id);
                    }
                }
                self.error = Some(format!(
                    "MIDI input queue overflow on {}; notes released",
                    input.port
                ));
            }
            for _ in 0..256 {
                let Ok(message) = input.queue.pop() else {
                    break;
                };
                for node in &graph.nodes {
                    if node.kind == "midi_input"
                        && node.io.as_ref().is_some_and(|io| io.port == input.port)
                    {
                        route_midi(node, message, engine);
                    }
                }
            }
        }
    }
}
fn open_input(port: &str) -> Result<Input, String> {
    if std::env::var_os("PR0_DISABLE_NATIVE_DEVICES").is_some() {
        return Err("Native MIDI disabled for this server".into());
    }
    let mut input = midir::MidiInput::new("pr0former graph").map_err(|e| e.to_string())?;
    input.ignore(midir::Ignore::None);
    let target = input
        .ports()
        .into_iter()
        .find(|p| input.port_name(p).ok().as_deref() == Some(port))
        .ok_or_else(|| format!("MIDI input unavailable: {port}"))?;
    let (mut producer, queue) = rtrb::RingBuffer::new(4096);
    let dropped = Arc::new(AtomicU64::new(0));
    let callback_dropped = dropped.clone();
    let connection = input
        .connect(
            &target,
            "pr0former graph input",
            move |_, bytes, _| {
                if bytes.len() == 3
                    && matches!(bytes[0] & 0xf0, 0x80 | 0x90 | 0xb0)
                    && bytes[1] < 128
                    && bytes[2] < 128
                    && producer.push([bytes[0], bytes[1], bytes[2]]).is_err()
                {
                    callback_dropped.fetch_add(1, Ordering::Relaxed);
                }
            },
            (),
        )
        .map_err(|e| e.to_string())?;
    Ok(Input {
        port: port.into(),
        _connection: connection,
        queue,
        dropped,
        last_dropped: 0,
    })
}
pub fn route_midi(node: &Node, message: [u8; 3], engine: &mut Engine) {
    if message[1] > 127 || message[2] > 127 {
        return;
    }
    let channel = node.parameters.get("channel").copied().unwrap_or(1.) as u8;
    if channel != 0 && channel != (message[0] & 0x0f) + 1 {
        return;
    }
    let cc = node.parameters.get("mode") == Some(&1.);
    // Forward the raw message with its channel nibble; the node decodes its own
    // scalar outlets from the same frame that its typed output carries.
    let forward = match message[0] & 0xf0 {
        0x80 | 0x90 => !cc,
        0xb0 => cc || matches!(message[1], 120 | 123),
        _ => false,
    };
    if forward {
        engine.node_midi_message(
            &node.id,
            pr0_core::midi::Message {
                status: message[0],
                data1: message[1],
                data2: message[2],
            },
        );
    }
}
/// A single numeric or short text argument for `osc_input` nodes.
pub fn osc_value(message: &rosc::OscMessage) -> Option<pr0_core::ControlValue> {
    match message.args.as_slice() {
        [rosc::OscType::String(text)] if text.len() <= pr0_core::MAX_CONTROL_TEXT_BYTES => {
            Some(pr0_core::ControlValue::Text(text.clone()))
        }
        [value] => crate::osc::number(value).map(pr0_core::ControlValue::Number),
        _ => None,
    }
}
pub fn osc_note(message: &rosc::OscMessage) -> Option<NoteEvent> {
    fn value(v: &rosc::OscType) -> Option<u8> {
        match v {
            rosc::OscType::Int(v) if (0..=127).contains(v) => Some(*v as u8),
            _ => None,
        }
    }
    if let [pitch, velocity] = message.args.as_slice() {
        Some(NoteEvent {
            pitch: value(pitch)?,
            velocity: value(velocity)?,
        })
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn osc_output_route_carries_destination_address_and_rate() {
        let p = pr0_core::demo_project("x".into(), "x".into(), pr0_core::Mode::Freeform);
        let mut node = p.graph.nodes[0].clone();
        node.kind = "osc_output".into();
        node.parameters = [("rate".into(), 30.)].into();
        node.io = Some(pr0_core::IoConfig {
            port: String::new(),
            address: "/level".into(),
            destination: "127.0.0.1:9100".into(),
        });
        assert_eq!(
            Route::from_node(&node),
            Some(Route::OscValue(
                "127.0.0.1:9100".parse().unwrap(),
                "/level".into(),
                30
            ))
        );
        node.io.as_mut().unwrap().address.clear();
        assert_eq!(Route::from_node(&node), None);
        let message = rosc::OscMessage {
            addr: "/level".into(),
            args: vec![rosc::OscType::Float(0.25)],
        };
        assert_eq!(
            osc_value(&message),
            Some(pr0_core::ControlValue::Number(0.25))
        );
        let text = rosc::OscMessage {
            addr: "/level".into(),
            args: vec![rosc::OscType::String("go".into())],
        };
        assert_eq!(
            osc_value(&text),
            Some(pr0_core::ControlValue::Text("go".into()))
        );
        let pair = rosc::OscMessage {
            addr: "/level".into(),
            args: vec![rosc::OscType::Int(60), rosc::OscType::Int(100)],
        };
        assert_eq!(osc_value(&pair), None);
    }
    #[test]
    fn route_midi_forwards_filtered_messages_with_channel_byte() {
        let p = pr0_core::demo_project("x".into(), "x".into(), pr0_core::Mode::Freeform);
        let mut node = p.graph.nodes[0].clone();
        node.id = "keys".into();
        node.kind = "midi_input".into();
        node.parameters = [("channel".into(), 0.)].into();
        let mut sink = node.clone();
        sink.id = "sink".into();
        sink.kind = "midi_output".into();
        sink.parameters.clear();
        let mut engine = Engine::prepare(
            Graph {
                nodes: vec![node.clone(), sink],
                edges: vec![pr0_core::Edge {
                    id: "cable".into(),
                    source: "keys".into(),
                    source_port: "midi".into(),
                    target: "sink".into(),
                    target_port: "midi".into(),
                }],
            },
            48000.,
        )
        .unwrap();
        let message = |status, data1, data2| pr0_core::midi::Message {
            status,
            data1,
            data2,
        };
        route_midi(&node, [0x93, 60, 100], &mut engine);
        engine.render(&[], &mut [[0.; 8]]);
        assert_eq!(
            engine.take_midi_message("sink"),
            Some(message(0x93, 60, 100))
        );
        assert_eq!(engine.telemetry()["keys"]["pitch"], 60.);
        assert_eq!(engine.telemetry()["keys"]["gate"], 1.);
        engine.render(&[], &mut [[0.; 8]]);
        // Note mode drops ordinary controllers before they reach the graph.
        route_midi(&node, [0xb3, 7, 99], &mut engine);
        engine.render(&[], &mut [[0.; 8]]);
        assert_eq!(engine.take_midi_message("sink"), None);
        // All-notes-off is forwarded and releases the decoded note.
        route_midi(&node, [0xb3, 123, 0], &mut engine);
        engine.render(&[], &mut [[0.; 8]]);
        assert_eq!(
            engine.take_midi_message("sink"),
            Some(message(0xb3, 123, 0))
        );
        assert_eq!(engine.telemetry()["keys"]["note_off"], 1.);
        assert_eq!(engine.telemetry()["keys"]["gate"], 0.);
    }
    #[test]
    fn keyboard_channel_note_off_and_cc_are_routed_to_only_the_selected_node() {
        let p = pr0_core::demo_project("x".into(), "x".into(), pr0_core::Mode::Freeform);
        let mut node = p.graph.nodes[0].clone();
        node.id = "keyboard".into();
        node.kind = "midi_input".into();
        node.parameters = [("channel".into(), 2.)].into();
        let mut engine = Engine::prepare(
            Graph {
                nodes: vec![node.clone()],
                edges: vec![],
            },
            48000.,
        )
        .unwrap();
        route_midi(&node, [0x90, 60, 100], &mut engine);
        engine.render(&[], &mut [[0.; 8]]);
        assert_eq!(engine.telemetry()["keyboard"]["gate"], 0.);
        route_midi(&node, [0x91, 64, 80], &mut engine);
        engine.render(&[], &mut [[0.; 8]]);
        assert_eq!(engine.telemetry()["keyboard"]["pitch"], 64.);
        assert_eq!(engine.telemetry()["keyboard"]["trigger"], 1.);
        engine.render(&[], &mut [[0.; 8]]);
        route_midi(&node, [0x91, 64, 0], &mut engine);
        engine.render(&[], &mut [[0.; 8]]);
        assert_eq!(engine.telemetry()["keyboard"]["note_off"], 1.);
        assert_eq!(engine.telemetry()["keyboard"]["gate"], 0.);
        engine.render(&[], &mut [[0.; 8]]);
        // In Note mode ordinary controllers never reach the graph.
        route_midi(&node, [0xb1, 7, 99], &mut engine);
        engine.render(&[], &mut [[0.; 8]]);
        assert_eq!(engine.telemetry()["keyboard"]["pitch"], 64.);
        // Message type is structural, so a CC-mode node is a freshly prepared engine.
        node.parameters.insert("mode".into(), 1.);
        let mut engine = Engine::prepare(
            Graph {
                nodes: vec![node.clone()],
                edges: vec![],
            },
            48000.,
        )
        .unwrap();
        route_midi(&node, [0xb1, 7, 99], &mut engine);
        engine.render(&[], &mut [[0.; 8]]);
        assert_eq!(engine.telemetry()["keyboard"]["pitch"], 7.);
        assert_eq!(engine.telemetry()["keyboard"]["velocity"], 99.);
        assert_eq!(engine.telemetry()["keyboard"]["trigger"], 1.);
    }
    #[test]
    fn osc_note_validation() {
        let message = |args| rosc::OscMessage {
            addr: "/notes".into(),
            args,
        };
        assert_eq!(
            osc_note(&message(vec![
                rosc::OscType::Int(60),
                rosc::OscType::Int(0)
            ])),
            Some(NoteEvent {
                pitch: 60,
                velocity: 0
            })
        );
        for args in [
            vec![],
            vec![rosc::OscType::Int(60)],
            vec![rosc::OscType::Int(128), rosc::OscType::Int(1)],
            vec![rosc::OscType::Int(60), rosc::OscType::Float(1.)],
        ] {
            assert!(osc_note(&message(args)).is_none());
        }
    }
    #[test]
    fn osc_converter_sends_each_polyphonic_event_and_node_reset_releases_held_pitches() {
        let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let outputs = Outputs::new(Arc::new(crate::osc::Runtime::default()));
        let route = Route::Osc(receiver.local_addr().unwrap(), "/notes".into());
        for (pitch, velocity) in [(60, 90), (64, 80), (60, 70), (60, 0)] {
            outputs.note("node", &route, NoteEvent { pitch, velocity }, false);
        }
        let receive = || {
            let mut buffer = [0; 128];
            let (n, _) = receiver.recv_from(&mut buffer).unwrap();
            let (_, rosc::OscPacket::Message(message)) =
                rosc::decoder::decode_udp(&buffer[..n]).unwrap()
            else {
                panic!("not message")
            };
            assert_eq!(message.addr, "/notes");
            osc_note(&message).unwrap()
        };
        for (pitch, velocity) in [(60, 90), (64, 80), (60, 70), (60, 0)] {
            assert_eq!(receive(), NoteEvent { pitch, velocity });
        }
        outputs.reset(vec!["node".into()]);
        assert_eq!(
            receive(),
            NoteEvent {
                pitch: 60,
                velocity: 0
            }
        );
        assert_eq!(
            receive(),
            NoteEvent {
                pitch: 64,
                velocity: 0
            }
        );
        assert_eq!(outputs.status()["dropped"], 0);
    }
}
