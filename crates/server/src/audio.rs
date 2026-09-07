use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use pr0_core::{MAX_CHANNELS, Project};
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
    Test(bool),
    Load(Project, Box<Engine>),
    Replace {
        project: Project,
        engine: Box<Engine>,
    },
    Unload,
    Parameter {
        node: String,
        key: String,
        value: f64,
        revision: u64,
    },
    Transport(String),
    Tempo(f64),
    Control {
        node: String,
        value: pr0_core::ControlValue,
        revision: u64,
    },
    Bang(String),
    Devices(oneshot::Sender<Value>),
    Hardware(bool),
    Capture(bool),
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
) -> SyncSender<Command> {
    let (tx, rx) = sync_channel::<Command>(256);
    std::thread::Builder::new()
        .name("pr0-orchestrator".into())
        .spawn(move || run(events, media, logs, rx))
        .expect("Start audio worker");
    tx
}
fn run(
    events: broadcast::Sender<Value>,
    media: broadcast::Sender<crate::media::AudioBlock>,
    logs: Arc<crate::settings::Logs>,
    rx: std::sync::mpsc::Receiver<Command>,
) {
    let io = crate::performance::external_worker();
    let mut sequencer: Option<crate::performance::Sequencer> = None;
    let mut engine: Option<Engine> = None;
    let mut project: Option<Project> = None;
    let mut epoch = String::new();
    let mut log_project = String::new();
    let mut outputs: Vec<Output> = vec![];
    let mut enabled = false;
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
    let mut deadline = Instant::now();
    let mut seq = 0_u64;
    let mut next_tempo: Option<(f64, f64)> = None;
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
                Command::Transport(a) => Some(a.as_str()),
                Command::Tempo(_) => Some("Tempo change queued"),
                Command::Clip { .. } => Some("Part cue queued"),
                Command::Capture(_) => Some("Input state requested"),
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
                    log_project = id;
                    outputs.clear();
                    hardware = false;
                    enabled = false;
                    testing = false;
                    inputs.clear();
                    settings = new_settings;
                    media_rates.clear();
                    input_rates.clear();

                    let result = if value {
                        open_outputs(&settings, underruns.clone()).map(|devices| {
                            outputs = devices;
                            hardware = !outputs.is_empty();
                            enabled = true;
                            device_error.clear();
                        })
                    } else {
                        Ok(())
                    };

                    if let Err(err) = &result {
                        device_error = err.clone();
                    }

                    let _=events.send(json!({"type":"audio_engine_status","enabled":enabled,"sample_rate":settings.sample_rate,"block_size":settings.block_size}));
                    let _ = reply.send(result);
                }
                Command::Test(value) => {
                    testing = value;
                    test_sample = 0;
                }
                Command::Capture(enabled) => {
                    inputs.clear();
                    if enabled {
                        match open_inputs(&settings) {
                            Ok(opened) => {
                                inputs = opened;
                                device_error.clear();
                            }
                            Err(e) => device_error = e,
                        }
                    }
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
                    }
                    previews.clear();
                    project = Some(p);
                    engine = Some(*prepared);
                }

                Command::Load(p, mut e) => {
                    sequencer = Some(crate::performance::Sequencer::new(&p));
                    e.clock.bpm = p.bpm;
                    engine = Some(*e);
                    project = Some(p);
                    epoch = uuid::Uuid::new_v4().to_string();
                    next_tempo = None;
                }
                Command::Unload => {
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
                Command::Bang(node) => {
                    if let Some(e) = engine.as_mut() {
                        e.bang(&node);
                    }
                }
                Command::Transport(action) => {
                    if let Some(e) = engine.as_mut() {
                        match action.as_str() {
                            "play" => e.clock.running = true,
                            "pause" => {
                                if let Some(seq) = &mut sequencer {
                                    seq.pause(e, &io);
                                }
                                e.clock.running = false;
                            }
                            "stop" => {
                                if let Some(seq) = &mut sequencer {
                                    seq.reset(e, &io);
                                }
                                e.clock.stop();
                                next_tempo = None;
                            }
                            _ => {}
                        }
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
                    let midi = midir::MidiOutput::new("pr0former device list")
                        .ok()
                        .map(|m| {
                            m.ports()
                                .iter()
                                .filter_map(|p| m.port_name(p).ok())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    let _=reply.send(json!({
"input_interfaces":input_devices().iter().map(|(id,name)|json!({"id":id,"name":name})).collect::<Vec<_>>(),"outputs":devices.iter().map(|d|&d.1).collect::<Vec<_>>(),"interfaces":devices.iter().map(|(id,name)|json!({
"id":id,"name":name}
)).collect::<Vec<_>>(),"midi_outputs":midi,"block_size":settings.block_size,"sample_rate":settings.sample_rate,"engine_enabled":enabled,"hardware_enabled":hardware,"input_enabled":!inputs.is_empty(),"active_inputs":inputs.iter().map(|i|i.id).collect::<Vec<_>>(),"underruns":underruns.load(Ordering::Relaxed),"error":device_error}
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
                if let Some(seq) = &mut sequencer {
                    seq.tick(e, &io);
                }
                for input in &mut inputs {
                    input.frame = input.queue.pop().ok().unwrap_or([0.; MAX_CHANNELS]);
                }
                if let Some(p) = &project {
                    for node in p.graph.nodes.iter().filter(|n| n.kind == "input") {
                        let route = node.parameters.get("interface").copied().unwrap_or(0.) as u32;
                        let sample = inputs
                            .iter()
                            .find(|i| route == 0 || i.id == route)
                            .map(|i| i.frame)
                            .unwrap_or([0.; MAX_CHANNELS]);
                        e.external(&node.id, sample);
                    }
                }
                e.render(&[], std::slice::from_mut(frame));
                for preview in &mut previews {
                    if preview.frames.len() < 256 {
                        preview
                            .frames
                            .push(e.audio_frame(&preview.source, &preview.port));
                    }
                }

                for out in &mut outputs {
                    let mut routed = [0.; MAX_CHANNELS];
                    if e.clock.running {
                        if let Some(p) = &project {
                            for n in &p.graph.nodes {
                                if n.kind == "output" {
                                    let route =
                                        n.parameters.get("interface").copied().unwrap_or(0.) as u32;
                                    if route == out.id || route == 0 {
                                        let v = e.output_frame(&n.id);
                                        for ch in 0..MAX_CHANNELS {
                                            routed[ch] += v[ch];
                                        }
                                    }
                                }
                            }
                        }
                    }
                    out.push(routed);
                }

                for (id, pcm) in &mut monitors {
                    pcm.extend_from_slice(&if e.clock.running {
                        e.monitor_frame(id)
                    } else {
                        [0.; 2]
                    });
                }
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
                    let raw: Vec<f32> = output
                        .iter()
                        .flat_map(|f| {
                            if e.clock.running {
                                [f[0], f[1]]
                            } else {
                                [0.; 2]
                            }
                        })
                        .collect();
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
"type":"telemetry","project_id":p.id,"revision":p.revision,"epoch":epoch,"sequence":seq,"server_time":monotonic_ms(),"sample":e.clock.sample,"beat":e.clock.beat,"bpm":e.clock.bpm,"running":e.clock.running,"block_size":settings.block_size,"sample_rate":settings.sample_rate,"engine_enabled":enabled,"hardware_enabled":hardware,"input_enabled":!inputs.is_empty(),"active_inputs":inputs.iter().map(|i|i.id).collect::<Vec<_>>(),"underruns":underruns.load(Ordering::Relaxed),"error":device_error,"parts":sequencer.as_ref().map(|s|s.playback(e.clock.beat)).unwrap_or_default(),"values":e.telemetry(),"visualizations":if visualization_subscribers.values().any(|(id,seen)|id==&p.id && seen.elapsed()<Duration::from_secs(10)){e.visualizations()}else{Default::default()}}
));
                }
            }
        }

        if engine.is_none() && enabled {
            for _ in 0..settings.block_size {
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
                    out.push([click; MAX_CHANNELS]);
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

fn open_input(
    device: cpal::Device,
    rate: u32,
    errors: Arc<AtomicU64>,
) -> Result<(cpal::Stream, rtrb::Consumer<[f32; MAX_CHANNELS]>), String> {
    let supported = device
        .supported_input_configs()
        .map_err(|e| e.to_string())?
        .find(|c| {
            c.sample_format() == cpal::SampleFormat::F32
                && c.min_sample_rate().0 <= rate
                && c.max_sample_rate().0 >= rate
        })
        .ok_or("Input does not support the selected sample rate with f32 samples")?;
    let config = supported.with_sample_rate(cpal::SampleRate(rate)).config();
    let channels = config.channels as usize;
    let (mut producer, consumer) = rtrb::RingBuffer::new(4096);
    let stream = device
        .build_input_stream(
            &config,
            move |data: &[f32], _| {
                for frame in data.chunks(channels) {
                    let mut samples = [0.; MAX_CHANNELS];
                    for (ch, sample) in frame.iter().take(MAX_CHANNELS).enumerate() {
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
    Ok((stream, consumer))
}

struct Input {
    id: u32,
    _stream: cpal::Stream,
    queue: rtrb::Consumer<[f32; MAX_CHANNELS]>,
    frame: [f32; MAX_CHANNELS],
    errors: Arc<AtomicU64>,
}
fn device_id(name: &str) -> u32 {
    name.bytes()
        .fold(2166136261_u32, |h, b| (h ^ b as u32).wrapping_mul(16777619))
        % 999999999
        + 1
}
pub fn input_devices() -> Vec<(u32, String)> {
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
        let device = cpal::default_host()
            .input_devices()
            .map_err(|e| e.to_string())?
            .find(|d| {
                d.name()
                    .is_ok_and(|name| name == selected.name && device_id(&name) == selected.id)
            })
            .ok_or_else(|| format!("Input {} is unavailable", selected.name))?;
        let errors = Arc::new(AtomicU64::new(0));
        let (stream, queue) = open_input(device, settings.sample_rate, errors.clone())
            .map_err(|e| format!("Input {}: {e}", selected.name))?;
        inputs.push(Input {
            id: selected.id,
            _stream: stream,
            queue,
            frame: [0.; MAX_CHANNELS],
            errors,
        });
    }
    if inputs.is_empty() {
        return Err(
            "No native inputs enabled. Select inputs in System settings and save first.".into(),
        );
    }
    Ok(inputs)
}

pub fn output_devices() -> Vec<(u32, String)> {
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
    _stream: cpal::Stream,
    buffer: crate::output_buffer::OutputBuffer,
    delay: Vec<[f32; MAX_CHANNELS]>,
    cursor: usize,
    errors: Arc<AtomicU64>,
}
impl Output {
    fn push(&mut self, mut frame: [f32; MAX_CHANNELS]) {
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
        let device = cpal::default_host()
            .output_devices()
            .map_err(|e| e.to_string())?
            .find(|d| d.name().ok().as_deref() == Some(&selected.name))
            .ok_or_else(|| format!("Interface unavailable: {}", selected.name))?;
        let supported = device
            .supported_output_configs()
            .map_err(|e| e.to_string())?
            .find(|c| {
                c.sample_format() == cpal::SampleFormat::F32
                    && c.min_sample_rate().0 <= settings.sample_rate
                    && c.max_sample_rate().0 >= settings.sample_rate
            })
            .ok_or_else(|| {
                format!(
                    "{} does not support {} Hz / f32",
                    selected.name, settings.sample_rate
                )
            })?;
        let config = supported
            .with_sample_rate(cpal::SampleRate(settings.sample_rate))
            .config();
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
            _stream: stream,
            buffer,
            delay: vec![[0.; MAX_CHANNELS]; delay],
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
