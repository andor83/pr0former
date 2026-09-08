use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use pr0_core::{MAX_CHANNELS, MAX_DEVICE_CHANNELS, Project};
use pr0_dsp::Engine;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, OnceLock,
        atomic::{AtomicU64, Ordering},
        mpsc::{SyncSender, sync_channel},
    },
    time::{Duration, Instant},
};
use tokio::sync::{broadcast, oneshot};

static START: OnceLock<Instant> = OnceLock::new();
pub fn monotonic_ms() -> f64 {
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.
}
pub enum Command {
    Osc {
        project: String,
        message: rosc::OscMessage,
    },
    Visualizers {
        session: String,
        project: String,
        enabled: bool,
    },
    Preview {
        project: String,
        source: String,
        port: String,
        channels: usize,
        reply: oneshot::Sender<Result<Value, String>>,
    },
    Enable(
        String,
        bool,
        crate::settings::Settings,
        oneshot::Sender<Result<(), String>>,
    ),
    ClearLoop {
        project: String,
        node: String,
        track: u8,
        reply: oneshot::Sender<Result<(), String>>,
    },
    Test(bool),
    Load(Project, Box<Engine>),
    Replace {
        project: Project,
        engine: Box<Engine>,
    },
    Unload,
    Show(bool),
    Parameter {
        node: String,
        key: String,
        value: f64,
        revision: u64,
    },
    Transport {
        action: String,
        count_in_beats: u8,
    },
    Tempo(f64),
    Control {
        node: String,
        value: pr0_core::ControlValue,
        revision: u64,
    },
    Bang(String),
    Piano {
        project: String,
        node: String,
        pitch: u8,
        velocity: u8,
    },
    Devices(oneshot::Sender<Value>),
    Hardware(bool),
    Clip {
        part: String,
        playing: bool,
    },
    BrowserInput {
        node: String,
        pcm: Vec<[f32; 2]>,
    },
}
pub fn start(
    events: broadcast::Sender<Value>,
    media: broadcast::Sender<crate::media::AudioBlock>,
    logs: Arc<crate::settings::Logs>,
    osc: Arc<crate::osc::Runtime>,
) -> SyncSender<Command> {
    let (tx, rx) = sync_channel::<Command>(256);
    std::thread::Builder::new()
        .name("pr0-orchestrator".into())
        .spawn(move || run(events, media, logs, rx, osc))
        .expect("Start audio worker");
    tx
}
fn run(
    events: broadcast::Sender<Value>,
    media: broadcast::Sender<crate::media::AudioBlock>,
    logs: Arc<crate::settings::Logs>,
    rx: std::sync::mpsc::Receiver<Command>,
    osc: Arc<crate::osc::Runtime>,
) {
    let io = crate::performance::external_worker(osc.clone());
    let node_outputs = crate::node_io::Outputs::new(osc.clone());
    let mut midi_inputs = crate::node_io::Inputs::default();
    let mut node_routes: Vec<(String, crate::node_io::Route, bool)> = Vec::new();
    let mut sequencer: Option<crate::performance::Sequencer> = None;
    let mut engine: Option<Engine> = None;
    let mut loop_store = crate::loops::Store::new();
    let mut record_store = crate::recordings::Store::new();
    let mut project: Option<Project> = None;
    let mut epoch = String::new();
    let mut log_project = String::new();
    let mut outputs: Vec<Output> = vec![];
    let mut enabled = false;
    let mut show_active = false;
    let mut settings = crate::settings::read();
    let mut testing = false;
    let mut test_sample = 0_u64;
    let mut media_rates: std::collections::BTreeMap<String, crate::samples::RateAdapter> =
        Default::default();
    let mut input_rates: std::collections::BTreeMap<String, crate::samples::RateAdapter> =
        Default::default();
    let mut logged_underruns = 0;
    let mut error_logged = String::new();
    let mut inputs: Vec<Input> = vec![];
    let mut hardware = false;
    let underruns = Arc::new(AtomicU64::new(0));
    let mut last = Instant::now();
    let mut meter_time = Instant::now();
    let mut deadline = Instant::now();
    let mut seq = 0_u64;
    let mut next_tempo: Option<(f64, f64)> = None;
    let mut count_in: Option<pr0_dsp::count_in::CountIn> = None;
    let mut device_error = String::new();
    let mut browser: std::collections::BTreeMap<String, std::collections::VecDeque<[f32; 2]>> =
        std::collections::BTreeMap::new();

    let mut previews: Vec<Preview> = Vec::new();
    let mut visualization_subscribers: std::collections::BTreeMap<String, (String, Instant)> =
        Default::default();
    loop {
        while let Ok(command) = rx.try_recv() {
            let context = match &command {
                Command::Load(p, _) => p.id.as_str(),
                _ => project
                    .as_ref()
                    .map(|p| p.id.as_str())
                    .unwrap_or(&log_project),
            };

            let description = match &command {
                Command::Load(..) => Some("Project engine prepared"),
                Command::Unload => Some("Show deactivated"),
                Command::Replace { .. } => Some("Graph replacement queued"),
                Command::Parameter { .. } => Some("Parameter updated"),
                Command::Transport { action, .. } => Some(action.as_str()),
                Command::Tempo(_) => Some("Tempo change queued"),
                Command::Clip { .. } => Some("Part cue queued"),
                Command::Hardware(_) => Some("Output state requested"),
                Command::Test(_) => Some("Latency metronome state changed"),
                _ => None,
            };

            if let Some(message) = description {
                logs.push(context, "info", message);
            }

            match command {
                Command::Visualizers {
                    session,
                    project,
                    enabled,
                } => {
                    if !enabled {
                        visualization_subscribers.remove(&session);
                    } else if visualization_subscribers.len() < 32
                        || visualization_subscribers.contains_key(&session)
                    {
                        visualization_subscribers.insert(session, (project, Instant::now()));
                    }
                }
                Command::Preview {
                    project: id,
                    source,
                    port,
                    channels,
                    reply,
                } => {
                    if project.as_ref().is_none_or(|p| p.id != id) {
                        let _ = reply.send(Err("Activate this show to preview audio".into()));
                    } else if previews.len() >= 32 {
                        let _ = reply.send(Err("Waveform preview capacity reached".into()));
                    } else {
                        previews.push(Preview {
                            source,
                            port,
                            channels,
                            reply: Some(reply),
                            frames: Vec::with_capacity(256),
                        });
                    }
                }
                Command::Enable(id, value, new_settings, reply) => {
                    count_in = None;
                    midi_inputs = crate::node_io::Inputs::default();
                    node_outputs.reset(vec![]);
                    log_project = id;
                    outputs.clear();
                    hardware = false;
                    enabled = false;
                    testing = false;
                    inputs.clear();
                    settings = new_settings;
                    media_rates.clear();
                    input_rates.clear();

                    let saved = if !value {
                        if let (Some(p), Some(e)) = (&project, &mut engine) {
                            e.finish_loops();
                            e.finish_recordings();
                            let recorded = record_store.flush(&p.id, e);
                            loop_store.flush(&p.id, e).and(recorded)
                        } else {
                            Ok(())
                        }
                    } else {
                        Ok(())
                    };
                    let result = if value {
                        open_outputs(&settings, underruns.clone()).and_then(|devices| {
                            let captured = open_inputs(&settings)?;
                            outputs = devices;
                            inputs = captured;
                            hardware = !outputs.is_empty();
                            enabled = true;
                            device_error.clear();
                            Ok(())
                        })
                    } else {
                        saved
                    };

                    if let Err(err) = &result {
                        device_error = err.clone();
                    }

                    let _=events.send(json!({"type":"audio_engine_status","enabled":enabled,"sample_rate":settings.sample_rate,"block_size":settings.block_size}));
                    let _ = reply.send(result);
                }
                Command::ClearLoop {
                    project: id,
                    node,
                    track,
                    reply,
                } => {
                    let result = if project.as_ref().is_some_and(|p| p.id == id) {
                        engine
                            .as_mut()
                            .ok_or("Engine unavailable".to_string())
                            .and_then(|e| {
                                e.clear_loop(&node, track)?;
                                loop_store.flush(&id, e)
                            })
                    } else {
                        Err("Project engine unavailable".into())
                    };
                    let _ = reply.send(result);
                }
                Command::Test(value) => {
                    testing = value;
                    test_sample = 0;
                }
                Command::Clip { part, playing } => {
                    if let (Some(seq), Some(e)) = (&mut sequencer, &mut engine) {
                        seq.launch(&part, playing, e, &io);
                    }
                }
                Command::BrowserInput { node, pcm } => {
                    let flat: Vec<f32> = pcm.iter().flatten().copied().collect();
                    let pcm: Vec<[f32; 2]> = input_rates
                        .entry(node.clone())
                        .or_insert_with(|| {
                            crate::samples::RateAdapter::new(48000, settings.sample_rate)
                        })
                        .process(&flat)
                        .chunks_exact(2)
                        .map(|f| [f[0], f[1]])
                        .collect();
                    if project.as_ref().is_some_and(|p| {
                        p.graph
                            .nodes
                            .iter()
                            .any(|n| n.id == node && n.kind == "browser_input")
                    }) {
                        let q = browser.entry(node).or_default();
                        if q.len() + pcm.len() > settings.sample_rate as usize / 10 {
                            q.clear();
                        }
                        q.extend(pcm);
                    }
                }
                Command::Replace {
                    project: p,
                    engine: prepared,
                } => {
                    // Retire changed/deleted external routes before installing their replacements.
                    let next_routes = prepare_node_routes(&p);
                    let retired: Vec<_> = node_routes
                        .iter()
                        .filter(|old| {
                            !next_routes.contains(old)
                                || project.as_ref().is_some_and(|previous| {
                                    note_input_wiring(previous, &old.0)
                                        != note_input_wiring(&p, &old.0)
                                })
                        })
                        .map(|old| old.0.clone())
                        .collect();
                    if !retired.is_empty() {
                        node_outputs.reset(retired);
                    }
                    node_routes = next_routes;
                    midi_inputs.configure(&p.graph);
                    // Commands are handled between DSP blocks, in order. Install
                    // now so a following parameter edit targets the new graph.
                    let mut prepared = prepared;
                    if let Some(previous) = &mut engine {
                        prepared.clock = previous.clock;
                        sequencer = Some(match sequencer.take() {
                            Some(seq) => seq.replace(&p, previous, &mut prepared, &io),
                            None => crate::performance::Sequencer::new(&p),
                        });
                        prepared.carry_node_state(previous);
                        // Only retired buffers remain in the old engine after state transfer.
                        previous.finish_loops();
                        previous.finish_recordings();
                        if let Some(p) = &project {
                            if let Err(e) = record_store.flush(&p.id, previous) {
                                device_error = e;
                            }
                            if let Err(e) = loop_store.flush(&p.id, previous) {
                                device_error = e;
                            }
                        }
                        if let Some(seq) = &sequencer {
                            seq.seed_part_nodes(previous, &mut prepared);
                        }
                    }
                    previews.clear();
                    for out in &mut outputs {
                        out.meter.clear();
                    }
                    project = Some(p);
                    engine = Some(*prepared);
                }

                Command::Load(p, mut e) => {
                    if let (Some(old), Some(engine)) = (&project, &mut engine) {
                        engine.finish_loops();
                        engine.finish_recordings();
                        if let Err(e) = record_store.flush(&old.id, engine) {
                            device_error = e;
                        }
                        if let Err(e) = loop_store.flush(&old.id, engine) {
                            device_error = e;
                        }
                    }
                    record_store.reset_error();
                    count_in = None;
                    node_outputs.reset(vec![]);
                    node_routes = prepare_node_routes(&p);
                    midi_inputs.configure(&p.graph);
                    for out in &mut outputs {
                        out.meter.clear();
                    }
                    sequencer = Some(crate::performance::Sequencer::new(&p));
                    e.clock.bpm = p.bpm;
                    engine = Some(*e);
                    project = Some(p);
                    epoch = uuid::Uuid::new_v4().to_string();
                    next_tempo = None;
                }
                Command::Show(value) => {
                    show_active = value;
                    if !value {
                        if let Some(e) = engine.as_mut() {
                            apply_transport(
                                "stop",
                                0,
                                4,
                                e,
                                &mut sequencer,
                                &mut count_in,
                                &mut next_tempo,
                                &io,
                            );
                        }
                    }
                }
                Command::Unload => {
                    if let (Some(p), Some(e)) = (&project, &mut engine) {
                        e.finish_loops();
                        e.finish_recordings();
                        if let Err(err) = record_store.flush(&p.id, e) {
                            device_error = err;
                        }
                        if let Err(err) = loop_store.flush(&p.id, e) {
                            device_error = err;
                        }
                    }
                    show_active = false;
                    count_in = None;
                    midi_inputs = crate::node_io::Inputs::default();
                    node_routes.clear();
                    node_outputs.reset(vec![]);
                    for out in &mut outputs {
                        out.meter.clear();
                    }
                    inputs.clear();
                    previews.clear();
                    let _ = io.try_send(crate::performance::External::Panic);
                    sequencer = None;
                    engine = None;
                    project = None;
                    browser.clear();
                    media_rates.clear();
                    input_rates.clear();
                    testing = false;
                }
                Command::Parameter {
                    node,
                    key,
                    value,
                    revision,
                } => {
                    if let Some(e) = engine.as_mut() {
                        if let Err(err) = e.parameter(&node, &key, value) {
                            device_error = err;
                        }
                        if let Some(p) = project.as_mut() {
                            p.revision = revision;
                            if let Some(n) = p.graph.nodes.iter_mut().find(|n| n.id == node) {
                                n.parameters.insert(key, value);
                            }
                        }
                    }
                }
                Command::Osc {
                    project: id,
                    message,
                } => {
                    let accepted = if enabled && project.as_ref().is_some_and(|p| p.id == id) {
                        if let Some(e) = engine.as_mut() {
                            match crate::osc::action(&message) {
                                Some(crate::osc::Action::Control(node, value)) => {
                                    e.external_control(&node, &value)
                                }
                                Some(crate::osc::Action::Bang(node)) => e.bang(&node),
                                Some(crate::osc::Action::Tempo(bpm)) if !e.tempo_connected() => {
                                    if e.clock.running {
                                        next_tempo = Some((e.clock.beat.floor() + 1., bpm));
                                    } else {
                                        e.clock.set_tempo(bpm);
                                    }
                                    true
                                }
                                Some(crate::osc::Action::Transport(action)) if show_active => {
                                    apply_transport(
                                        action,
                                        0,
                                        project.as_ref().map_or(4, |p| p.beat_unit),
                                        e,
                                        &mut sequencer,
                                        &mut count_in,
                                        &mut next_tempo,
                                        &io,
                                    );
                                    true
                                }
                                _ => {
                                    let mut matched = false;
                                    if let Some(note) = crate::node_io::osc_note(&message) {
                                        if let Some(p) = &project {
                                            for node in &p.graph.nodes {
                                                if node.kind == "osc_to_midi"
                                                    && node.io.as_ref().is_some_and(|io| {
                                                        io.address == message.addr
                                                    })
                                                {
                                                    e.node_midi_note(
                                                        &node.id,
                                                        note.pitch,
                                                        note.velocity,
                                                    );
                                                    matched = true;
                                                }
                                            }
                                        }
                                    }
                                    matched
                                }
                            }
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    if !accepted {
                        osc.reject();
                    }
                }
                Command::Control {
                    node,
                    value,
                    revision,
                } => {
                    if let Some(e) = engine.as_mut() {
                        e.control(&node, &value);
                    }
                    if let Some(p) = project.as_mut() {
                        p.revision = revision;
                        if let Some(n) = p.graph.nodes.iter_mut().find(|n| n.id == node) {
                            n.control_value = Some(value);
                        }
                    }
                }
                Command::Piano {
                    project: id,
                    node,
                    pitch,
                    velocity,
                } => {
                    if project.as_ref().is_some_and(|p| p.id == id) {
                        if let Some(e) = engine.as_mut() {
                            e.piano_note(&node, pitch, velocity);
                        }
                    }
                }
                Command::Bang(node) => {
                    if let Some(e) = engine.as_mut() {
                        e.bang(&node);
                    }
                }
                Command::Transport {
                    action,
                    count_in_beats,
                } => {
                    if let Some(e) = engine.as_mut() {
                        apply_transport(
                            &action,
                            count_in_beats,
                            project.as_ref().map_or(4, |p| p.beat_unit),
                            e,
                            &mut sequencer,
                            &mut count_in,
                            &mut next_tempo,
                            &io,
                        );
                    }
                }
                Command::Tempo(bpm) => {
                    if let Some(e) = engine.as_mut() {
                        if e.clock.running {
                            next_tempo = Some((e.clock.beat.floor() + 1., bpm));
                        } else {
                            e.clock.set_tempo(bpm);
                        }
                    }
                }
                Command::Devices(reply) => {
                    let devices = output_devices();
                    let midi_out = midir::MidiOutput::new("pr0former device list");
                    let midi_in = midir::MidiInput::new("pr0former device list");
                    let midi_error = midi_out
                        .as_ref()
                        .err()
                        .map(ToString::to_string)
                        .or_else(|| midi_in.as_ref().err().map(ToString::to_string));
                    let midi = midi_out
                        .ok()
                        .map(|m| {
                            m.ports()
                                .iter()
                                .filter_map(|p| m.port_name(p).ok())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let midi_inputs = midi_in
                        .ok()
                        .map(|m| {
                            m.ports()
                                .iter()
                                .filter_map(|p| m.port_name(p).ok())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let _=reply.send(json!({
"input_interfaces":device_details(settings.sample_rate, true),"outputs":devices.iter().map(|d|&d.1).collect::<Vec<_>>(),"interfaces":device_details(settings.sample_rate, false),"midi_outputs":midi,"midi_inputs":midi_inputs,"midi_error":midi_error,"block_size":settings.block_size,"sample_rate":settings.sample_rate,"engine_enabled":enabled,"hardware_enabled":hardware,"input_enabled":!inputs.is_empty(),"active_inputs":inputs.iter().map(|i|i.id).collect::<Vec<_>>(),"underruns":underruns.load(Ordering::Relaxed),"error":device_error}
));
                }
                Command::Hardware(value) => {
                    outputs.clear();
                    hardware = false;
                    if value {
                        match open_outputs(&settings, underruns.clone()) {
                            Ok(o) => {
                                outputs = o;
                                hardware = !outputs.is_empty();
                                device_error.clear();
                            }
                            Err(e) => device_error = e,
                        }
                    }
                }
            }
        }

        if let Some(failed) = outputs
            .iter()
            .find(|out| out.errors.load(Ordering::Relaxed) > 0)
        {
            device_error = format!(
                "Audio interface {} stream failed; hardware output stopped",
                failed.id
            );
            logs.push(
                project
                    .as_ref()
                    .map(|p| p.id.as_str())
                    .unwrap_or(&log_project),
                "error",
                &device_error,
            );
            outputs.clear();
            hardware = false;
            testing = false;
        }
        if outputs.first().is_some_and(|o| !o.buffer.needs_frames()) {
            std::thread::sleep(Duration::from_micros(500));
            continue;
        }

        if let Some(e) = engine.as_mut() {
            let mut storage = [[0.; MAX_CHANNELS]; 1024];
            let output = &mut storage[..settings.block_size];

            let mut monitors: std::collections::BTreeMap<String, Vec<f32>> = if media
                .receiver_count()
                > 0
            {
                project
                    .as_ref()
                    .map(|p| {
                        p.graph
                            .nodes
                            .iter()
                            .filter(|n| n.kind == "monitor_output")
                            .map(|n| (n.id.clone(), Vec::with_capacity(settings.block_size * 2)))
                            .collect()
                    })
                    .unwrap_or_default()
            } else {
                Default::default()
            };

            // Split rendering at the exact tempo boundary;
            // No browser timer drives DSP.
            for frame in output.iter_mut() {
                if let Some(p) = &project {
                    midi_inputs.drain(&p.graph, e);
                }
                for (node, q) in &mut browser {
                    let sample = q.pop_front().unwrap_or([0.; 2]);
                    let mut audio = [0.; MAX_CHANNELS];
                    audio[..2].copy_from_slice(&sample);
                    e.external(node, audio);
                }
                if let Some((beat, bpm)) = next_tempo {
                    if e.clock.beat >= beat {
                        e.clock.set_tempo(bpm);
                        next_tempo = None;
                    }
                }
                let click = advance_count_in(&mut count_in, e);
                if let Some(seq) = &mut sequencer {
                    seq.tick(e, &io);
                }
                for input in &mut inputs {
                    input.sample();
                }
                if let Some(p) = &project {
                    for node in p.graph.nodes.iter().filter(|n| n.kind == "input") {
                        let route = node.parameters.get("interface").copied().unwrap_or(0.) as u32;
                        let sample = inputs
                            .iter()
                            .find(|i| route == 0 || i.id == route)
                            .map(|i| i.frame)
                            .unwrap_or([0.; MAX_DEVICE_CHANNELS]);
                        e.device_input(&node.id, sample);
                    }
                }
                e.render(&[], std::slice::from_mut(frame));
                for (node, route, cc) in &node_routes {
                    for event in e.take_midi_output(node).into_iter().flatten() {
                        if enabled {
                            node_outputs.note(node, route, event, *cc);
                        }
                    }
                }
                for preview in &mut previews {
                    if preview.frames.len() < 256 {
                        preview
                            .frames
                            .push(e.audio_frame(&preview.source, &preview.port));
                    }
                }

                for out in &mut outputs {
                    let mut routed = [0.; MAX_DEVICE_CHANNELS];
                    if enabled {
                        if let Some(p) = &project {
                            for n in &p.graph.nodes {
                                if n.kind == "output" {
                                    let route =
                                        n.parameters.get("interface").copied().unwrap_or(0.) as u32;
                                    if route == out.id || route == 0 {
                                        let v = e.device_output_frame(&n.id);
                                        for ch in 0..MAX_DEVICE_CHANNELS {
                                            routed[ch] += v[ch];
                                        }
                                    }
                                }
                            }
                        }
                    }
                    out.meter.observe(&routed);
                    out.push(routed);
                }

                for (id, pcm) in &mut monitors {
                    let mut sample = e.monitor_frame(id);
                    if let Some(click) = click {
                        for ch in &mut sample {
                            *ch += click;
                        }
                    }
                    pcm.extend_from_slice(&sample);
                }
                // Count-in clicks join browser monitors; the development graph keeps running.
                if let Some(click) = click {
                    for ch in frame {
                        *ch += click;
                    }
                }
            }

            if let Some(p) = &project {
                loop_store.poll(&p.id, e);
                record_store.poll(&p.id, e);
            }
            e.analyze_visualizers();
            previews.retain_mut(|preview| {
                if preview.reply.as_ref().is_none_or(|r|r.is_closed()){return false;}
                if preview.frames.len()<256{return true;}
                let channels:Vec<Vec<f32>>=(0..preview.channels).map(|ch|preview.frames.iter().map(|f|f[ch]).collect()).collect();
                let _=preview.reply.take().unwrap().send(Ok(json!({"channels":channels,"sample_rate":settings.sample_rate,"sample":e.clock.sample})));false
            });
            if media.receiver_count() > 0 {
                if let Some(p) = &project {
                    let raw: Vec<f32> = output.iter().flat_map(|f| [f[0], f[1]]).collect();
                    let pcm = media_rates
                        .entry(String::new())
                        .or_insert_with(|| {
                            crate::samples::RateAdapter::new(settings.sample_rate, 48000)
                        })
                        .process(&raw);
                    for (id, pcm) in &mut monitors {
                        *pcm = media_rates
                            .entry(id.clone())
                            .or_insert_with(|| {
                                crate::samples::RateAdapter::new(settings.sample_rate, 48000)
                            })
                            .process(pcm);
                    }
                    let _ = media.send(crate::media::AudioBlock {
                        project: p.id.clone(),
                        pcm: Arc::new(pcm),
                        monitors: monitors
                            .into_iter()
                            .map(|(id, pcm)| (id, Arc::new(pcm)))
                            .collect(),
                    });
                }
            }

            if last.elapsed() >= Duration::from_millis(50) {
                last = Instant::now();
                seq += 1;
                visualization_subscribers
                    .retain(|_, (_, seen)| seen.elapsed() < Duration::from_secs(10));
                if let Some(p) = &project {
                    let count = underruns.load(Ordering::Relaxed);
                    if count > logged_underruns {
                        logs.push(
                            &p.id,
                            "warn",
                            &format!("Audio underrun: {} frames", count - logged_underruns),
                        );
                        logged_underruns = count;
                    }

                    if device_error != error_logged {
                        if !device_error.is_empty() {
                            logs.push(&p.id, "error", &device_error);
                        }
                        error_logged = device_error.clone();
                    }
                    let _=events.send(json!({
"type":"telemetry","project_id":p.id,"revision":p.revision,"epoch":epoch,"sequence":seq,"server_time":monotonic_ms(),"sample":e.clock.sample,"beat":e.clock.beat,"graph_beat":e.graph_clock.beat,"bpm":e.clock.bpm,"running":e.clock.running,"count_in_remaining":count_in.as_ref().map(|c|c.remaining()),"midi_input_error":midi_inputs.error,"node_io":node_outputs.status(),"block_size":settings.block_size,"sample_rate":settings.sample_rate,"engine_enabled":enabled,"hardware_enabled":hardware,"input_enabled":!inputs.is_empty(),"active_inputs":inputs.iter().map(|i|i.id).collect::<Vec<_>>(),"underruns":underruns.load(Ordering::Relaxed),"error":record_store.error().or_else(|| loop_store.error()).unwrap_or_else(|| device_error.clone()),"parts":sequencer.as_ref().map(|s|s.playback(e.clock.beat)).unwrap_or_default(),"values":e.telemetry(),"visualizations":if visualization_subscribers.values().any(|(id,seen)|id==&p.id && seen.elapsed()<Duration::from_secs(10)){e.visualizations()}else{Default::default()}}
));
                }
            }
        }

        if engine.is_none() && enabled {
            for _ in 0..settings.block_size {
                for input in &mut inputs {
                    input.sample();
                }
                let phase = test_sample % (settings.sample_rate as u64 / 2);
                let click = if testing && phase < settings.sample_rate as u64 / 100 {
                    (phase as f32 * std::f32::consts::TAU * 1000. / settings.sample_rate as f32)
                        .sin()
                        * 0.15
                        * (1. - phase as f32 / (settings.sample_rate as f32 / 100.))
                } else {
                    0.
                };
                for out in &mut outputs {
                    out.push([click; MAX_DEVICE_CHANNELS]);
                }
                test_sample += 1;
                if test_sample >= settings.sample_rate as u64 * 60 {
                    testing = false;
                }
            }

            if testing && last.elapsed() >= Duration::from_millis(50) {
                last = Instant::now();
                let _=events.send(json!({
"type":"latency_test","project_id":log_project,"sample":test_sample,"sample_rate":settings.sample_rate,"server_time":monotonic_ms()}
));
            }
        }

        if inputs.iter().any(|i| i.errors.load(Ordering::Relaxed) > 0) {
            inputs.clear();
            device_error =
                "Native input stream failed. Refresh devices and enable capture again.".into();
        }
        if meter_time.elapsed() >= Duration::from_millis(50) {
            meter_time = Instant::now();
            let input_levels: Vec<_> = inputs
                .iter_mut()
                .map(|i| i.meter.take(&[true; MAX_DEVICE_CHANNELS]))
                .collect();
            let output_levels: Vec<_> = outputs
                .iter_mut()
                .filter_map(|out| {
                    let selected = crate::hardware_meter::output_channels(project.as_ref(), out.id);
                    let levels = out.meter.take(&selected);
                    (!levels.levels.is_empty()).then_some(levels)
                })
                .collect();
            let _ = events.send(json!({"type":"hardware_levels","project_id":project.as_ref().map(|p|&p.id),"server_time":monotonic_ms(),"inputs":input_levels,"outputs":output_levels}));
        }
        if hardware {
            deadline = Instant::now();
            continue;
        }

        deadline +=
            Duration::from_secs_f64(settings.block_size as f64 / settings.sample_rate as f64);
        let now = Instant::now();
        if deadline > now {
            std::thread::sleep(deadline - now);
        } else if now.duration_since(deadline) > Duration::from_millis(100) {
            deadline = now;
        }
    }
}

// Discovery and stream startup use the same widest supported f32 configuration.
fn device_config(
    device: &cpal::Device,
    rate: u32,
    input: bool,
) -> Result<cpal::StreamConfig, String> {
    let configs: Vec<_> = if input {
        device
            .supported_input_configs()
            .map_err(|e| e.to_string())?
            .collect()
    } else {
        device
            .supported_output_configs()
            .map_err(|e| e.to_string())?
            .collect()
    };
    configs
        .into_iter()
        .filter(|c| {
            c.sample_format() == cpal::SampleFormat::F32
                && (1..=MAX_DEVICE_CHANNELS).contains(&(c.channels() as usize))
                && c.min_sample_rate().0 <= rate
                && c.max_sample_rate().0 >= rate
        })
        .max_by_key(|c| c.channels())
        .map(|c| c.with_sample_rate(cpal::SampleRate(rate)).config())
        .ok_or_else(|| format!("No supported 1–64-channel f32 configuration at {rate} Hz"))
}
fn device_details(rate: u32, input: bool) -> Vec<Value> {
    if native_disabled() {
        return vec![];
    }
    let host = cpal::default_host();
    let devices = if input {
        host.input_devices()
    } else {
        host.output_devices()
    };
    devices
        .map(|devices| {
            devices
                .filter_map(|device| {
                    let name = device.name().ok()?;
                    let config = device_config(&device, rate, input);
                    Some(json!({"id":device_id(&name), "name":name,
            "channels":config.as_ref().map(|c|c.channels).ok(), "error":config.err()}))
                })
                .collect()
        })
        .unwrap_or_default()
}

fn open_input(
    device: cpal::Device,
    rate: u32,
    errors: Arc<AtomicU64>,
) -> Result<
    (
        cpal::Stream,
        rtrb::Consumer<[f32; MAX_DEVICE_CHANNELS]>,
        usize,
    ),
    String,
> {
    let config = device_config(&device, rate, true)?;
    let channels = config.channels as usize;
    let (mut producer, consumer) = rtrb::RingBuffer::new(4096);
    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _| {
                for frame in data.chunks(channels) {
                    let mut samples = [0.; MAX_DEVICE_CHANNELS];
                    for (ch, sample) in frame.iter().take(MAX_DEVICE_CHANNELS).enumerate() {
                        samples[ch] = *sample;
                    }
                    let _ = producer.push(samples);
                }
            },
            move |_| {
                errors.fetch_add(1, Ordering::Relaxed);
            },
            None,
        )
        .map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, consumer, channels))
}

struct Input {
    id: u32,
    meter: crate::hardware_meter::Meter,
    _stream: cpal::Stream,
    queue: rtrb::Consumer<[f32; MAX_DEVICE_CHANNELS]>,
    frame: [f32; MAX_DEVICE_CHANNELS],
    errors: Arc<AtomicU64>,
}
impl Input {
    fn sample(&mut self) {
        self.frame = self.queue.pop().ok().unwrap_or([0.; MAX_DEVICE_CHANNELS]);
        self.meter.observe(&self.frame);
    }
}
fn device_id(name: &str) -> u32 {
    name.bytes()
        .fold(2166136261_u32, |h, b| (h ^ b as u32).wrapping_mul(16777619))
        % 999999999
        + 1
}
fn native_disabled() -> bool {
    std::env::var("PR0_DISABLE_NATIVE_DEVICES").as_deref() == Ok("1")
}
pub fn input_devices() -> Vec<(u32, String)> {
    if native_disabled() {
        return vec![];
    }
    cpal::default_host()
        .input_devices()
        .map(|ds| {
            ds.filter_map(|d| d.name().ok())
                .map(|name| (device_id(&name), name))
                .collect()
        })
        .unwrap_or_default()
}
fn open_inputs(settings: &crate::settings::Settings) -> Result<Vec<Input>, String> {
    let mut inputs = vec![];
    for selected in settings.input_interfaces.iter().filter(|i| i.enabled) {
        if native_disabled() {
            return Err("Native devices are disabled by PR0_DISABLE_NATIVE_DEVICES".into());
        }
        let device = cpal::default_host()
            .input_devices()
            .map_err(|e| e.to_string())?
            .find(|d| {
                d.name()
                    .is_ok_and(|name| name == selected.name && device_id(&name) == selected.id)
            })
            .ok_or_else(|| format!("Input {} is unavailable", selected.name))?;
        let errors = Arc::new(AtomicU64::new(0));
        let (stream, queue, channels) = open_input(device, settings.sample_rate, errors.clone())
            .map_err(|e| format!("Input {}: {e}", selected.name))?;
        inputs.push(Input {
            id: selected.id,
            meter: crate::hardware_meter::Meter::new(selected.id, selected.name.clone(), channels),
            _stream: stream,
            queue,
            frame: [0.; MAX_DEVICE_CHANNELS],
            errors,
        });
    }
    Ok(inputs)
}

#[cfg(test)]
mod input_startup_tests {
    #[test]
    fn no_selected_inputs_allows_engine_startup_without_opening_devices() {
        let mut settings = crate::settings::Settings::default();
        assert!(super::open_inputs(&settings).unwrap().is_empty());
        settings
            .input_interfaces
            .push(crate::settings::InputInterface {
                id: 1,
                name: "Unavailable unchecked input".into(),
                enabled: false,
            });
        assert!(super::open_inputs(&settings).unwrap().is_empty());
    }
}

pub fn output_devices() -> Vec<(u32, String)> {
    if native_disabled() {
        return vec![];
    }
    cpal::default_host()
        .output_devices()
        .map(|ds| {
            ds.filter_map(|d| d.name().ok())
                .map(|name| {
                    // Stable, exactly representable numeric route ID; zero is the all-enabled route.
                    let id = name
                        .bytes()
                        .fold(2166136261_u32, |h, b| (h ^ b as u32).wrapping_mul(16777619))
                        % 999999999
                        + 1;
                    (id, name)
                })
                .collect()
        })
        .unwrap_or_default()
}
struct Output {
    id: u32,
    meter: crate::hardware_meter::Meter,
    _stream: cpal::Stream,
    buffer: crate::output_buffer::OutputBuffer,
    delay: Vec<[f32; MAX_DEVICE_CHANNELS]>,
    cursor: usize,
    errors: Arc<AtomicU64>,
}
impl Output {
    fn push(&mut self, mut frame: [f32; MAX_DEVICE_CHANNELS]) {
        if !self.delay.is_empty() {
            std::mem::swap(&mut frame, &mut self.delay[self.cursor]);
            self.cursor = (self.cursor + 1) % self.delay.len();
        }
        self.buffer.push(frame);
    }
}
fn open_outputs(
    settings: &crate::settings::Settings,
    underruns: Arc<AtomicU64>,
) -> Result<Vec<Output>, String> {
    let mut outputs = vec![];
    let max_latency = settings
        .interfaces
        .iter()
        .filter(|i| i.enabled && i.correct_latency)
        .map(|i| i.latency_ms)
        .fold(0_f64, f64::max);
    for selected in settings.interfaces.iter().filter(|i| i.enabled) {
        if native_disabled() {
            return Err("Native devices are disabled by PR0_DISABLE_NATIVE_DEVICES".into());
        }
        let device = cpal::default_host()
            .output_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.name().ok().as_deref() == Some(&selected.name))
            .ok_or_else(|| format!("Interface unavailable: {}", selected.name))?;
        let config = device_config(&device, settings.sample_rate, false)
            .map_err(|e| format!("{}: {e}", selected.name))?;
        let channels = config.channels as usize;
        let (buffer, mut consumer) = crate::output_buffer::OutputBuffer::new(
            settings.block_size,
            settings.sample_rate,
            underruns.clone(),
        );
        let errors = Arc::new(AtomicU64::new(0));
        let err = errors.clone();
        let stream = device
            .build_output_stream(
                &config,
                move |data: &mut [f32], _| {
                    consumer.write(data, channels);
                },
                move |_| {
                    err.fetch_add(1, Ordering::Relaxed);
                },
                None,
            )
            .map_err(|e| e.to_string())?;
        stream.play().map_err(|e| e.to_string())?;
        let delay = if selected.correct_latency {
            ((max_latency - selected.latency_ms) * settings.sample_rate as f64 / 1000.).round()
                as usize
        } else {
            0
        };
        outputs.push(Output {
            id: selected.id,
            meter: crate::hardware_meter::Meter::new(selected.id, selected.name.clone(), channels),
            _stream: stream,
            buffer,
            delay: vec![[0.; MAX_DEVICE_CHANNELS]; delay],
            cursor: 0,
            errors,
        });
    }
    Ok(outputs)
}

struct Preview {
    source: String,
    port: String,
    channels: usize,
    reply: Option<oneshot::Sender<Result<Value, String>>>,
    frames: Vec<[f32; MAX_CHANNELS]>,
}

// Shared by HTTP and OSC so pause/stop cannot leave a pending auto-start.
#[allow(clippy::too_many_arguments)]
fn apply_transport(
    action: &str,
    beats: u8,
    beat_unit: u8,
    engine: &mut Engine,
    sequencer: &mut Option<crate::performance::Sequencer>,
    count_in: &mut Option<pr0_dsp::count_in::CountIn>,
    next_tempo: &mut Option<(f64, f64)>,
    io: &SyncSender<crate::performance::External>,
) {
    match action {
        "play" if !engine.clock.running && count_in.is_none() => {
            // Resume directly; count only when starting from rewind.
            *count_in = if engine.clock.beat == 0. {
                pr0_dsp::count_in::CountIn::new(beats, beat_unit)
            } else {
                None
            };
            engine.clock.running = count_in.is_none();
        }
        "pause" => {
            *count_in = None;
            if let Some(seq) = sequencer {
                seq.pause(engine, io);
            }
            engine.clock.running = false;
        }
        "stop" => {
            *count_in = None;
            if let Some(seq) = sequencer {
                seq.reset(engine, io);
            }
            engine.clock.stop();
            *next_tempo = None;
        }
        _ => {}
    }
}

fn advance_count_in(
    count_in: &mut Option<pr0_dsp::count_in::CountIn>,
    engine: &mut Engine,
) -> Option<f32> {
    let click = count_in
        .as_mut()
        .and_then(|count| count.next(&engine.clock));
    if count_in.is_some() && click.is_none() {
        *count_in = None;
        engine.clock.running = true;
    }
    click
}

#[cfg(test)]
mod transport_tests {
    use super::*;

    #[test]
    fn changed_note_input_wiring_retires_notes_but_edge_ids_do_not() {
        let p =
            pr0_core::demo_project("wiring".into(), "Wiring".into(), pr0_core::Mode::Structured);
        let target = p.graph.edges[0].target.clone();
        let original = note_input_wiring(&p, &target);
        let mut edited = p.clone();
        edited.graph.edges.reverse();
        for edge in &mut edited.graph.edges {
            edge.id.push_str("-new");
        }
        assert_eq!(original, note_input_wiring(&edited, &target));
        edited.graph.edges.retain(|edge| edge.target != target);
        assert_ne!(original, note_input_wiring(&edited, &target));
    }

    #[test]
    fn start_boundary_cancellation_duplicate_play_and_resume() {
        let p = pr0_core::demo_project("count".into(), "Count".into(), pr0_core::Mode::Structured);
        let mut e = Engine::prepare(p.graph.clone(), 48000.).unwrap();
        let mut seq = Some(crate::performance::Sequencer::new(&p));
        let (io, _) = sync_channel(256);
        let mut count = None;
        let mut tempo = None;
        apply_transport("play", 6, 8, &mut e, &mut seq, &mut count, &mut tempo, &io);
        // Six eighth-note beats at 120 quarter BPM: 72,000 silent transport samples.
        for i in 0..72000 {
            if i == 12000 {
                apply_transport("play", 6, 8, &mut e, &mut seq, &mut count, &mut tempo, &io);
            }
            assert!(advance_count_in(&mut count, &mut e).is_some());
            seq.as_mut().unwrap().tick(&mut e, &io);
            assert_eq!(seq.as_ref().unwrap().lanes[0].next, 0);
            assert!(!e.clock.running);
            e.render(&[], &mut [[0.; MAX_CHANNELS]]);
            assert_eq!(e.clock.beat, 0.);
        }
        assert!(advance_count_in(&mut count, &mut e).is_none());
        assert!(e.clock.running);
        assert_eq!(e.clock.beat, 0.);
        seq.as_mut().unwrap().tick(&mut e, &io);
        assert!(seq.as_ref().unwrap().lanes[0].next > 0);
        e.render(&[], &mut [[0.; MAX_CHANNELS]]);
        apply_transport("pause", 0, 8, &mut e, &mut seq, &mut count, &mut tempo, &io);
        let paused = e.clock.beat;
        apply_transport("play", 6, 8, &mut e, &mut seq, &mut count, &mut tempo, &io);
        assert!(count.is_none() && e.clock.running);
        assert_eq!(e.clock.beat, paused);
        for action in ["stop", "pause"] {
            apply_transport("stop", 0, 8, &mut e, &mut seq, &mut count, &mut tempo, &io);
            apply_transport("play", 6, 8, &mut e, &mut seq, &mut count, &mut tempo, &io);
            assert!(count.is_some());
            apply_transport(action, 0, 8, &mut e, &mut seq, &mut count, &mut tempo, &io);
            assert!(advance_count_in(&mut count, &mut e).is_none());
            assert!(!e.clock.running);
        }
        apply_transport("play", 0, 8, &mut e, &mut seq, &mut count, &mut tempo, &io);
        assert!(count.is_none() && e.clock.running);
    }
}

fn prepare_node_routes(project: &Project) -> Vec<(String, crate::node_io::Route, bool)> {
    project
        .graph
        .nodes
        .iter()
        .filter_map(|n| {
            crate::node_io::Route::from_node(n).map(|route| {
                (
                    n.id.clone(),
                    route,
                    n.kind == "midi_output" && n.parameters.get("mode") == Some(&1.),
                )
            })
        })
        .collect()
}

// Ignore edge IDs/order: only changed inlet contracts retire held external notes.
fn note_input_wiring(project: &Project, node: &str) -> Vec<(String, String, String)> {
    let mut inputs: Vec<_> = project
        .graph
        .edges
        .iter()
        .filter(|edge| edge.target == node)
        .map(|edge| {
            (
                edge.target_port.clone(),
                edge.source.clone(),
                edge.source_port.clone(),
            )
        })
        .collect();
    inputs.sort();
    inputs
}
