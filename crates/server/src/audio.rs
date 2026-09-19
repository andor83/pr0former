use cpal::traits::{DeviceTrait, StreamTrait};
// Enumerating a host's devices is a platform that *has* a device list. Logical
// route platforms resolve through `crate::native_audio` and never call it.
#[cfg(not(target_os = "ios"))]
use cpal::traits::HostTrait;
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
    Snapshot { project:String, reply:oneshot::Sender<Result<(Project,u64),String>> },
    LiveControl { project: String, node: String, value: f64, revision: u64 },
    LocalMidi {
        project: String,
        node: String,
        message: Option<pr0_core::midi::Message>,
    },
    Controller {
        project: String,
        node: String,
        index: usize,
        value: Option<f64>,
        cancel: bool,
    },
    Shutdown(oneshot::Sender<Result<(), String>>),
    /// Closes (`true`) or reopens (`false`) the native audio hardware streams
    /// without disturbing the engine, the prepared graph, transport position,
    /// loops or open recordings. Idempotent: requesting the state the worker is
    /// already in replies `Ok` and touches no device.
    ///
    /// The reply is a plain synchronous channel so a host can complete a
    /// transition from an operating-system callback thread — an iOS audio
    /// interruption or background notification — without an async runtime.
    Suspend {
        suspended: bool,
        reply: std::sync::mpsc::SyncSender<Result<(), String>>,
    },
    Osc {
        project: String,
        message: rosc::OscMessage,
    },
    Visualizers {
        session: String,
        project: String,
        enabled: bool,
    },
    Monitor {
        session: String,
        project: String,
        node: Option<String>,
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
        recall: Option<crate::states::Install>,
        project: Project,
        engine: Box<Engine>,
    },
    Unload,
    Show(bool),
    /// Click track mixed into browser monitors while transport runs.
    Metronome(bool),
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
    Seek(f64),
    Tempo(f64),
    Control {
        node: String,
        value: pr0_core::ControlValue,
        revision: u64,
    },
    Bang(String),
    Audition {
        project: String,
        part: String,
        staff: u8,
        node: Option<String>,
        channel: u8,
        pitch: u8,
        velocity: u8,
    },
    Piano {
        project: String,
        node: String,
        message: pr0_core::midi::Message,
    },
    Devices(oneshot::Sender<Value>),
    Hardware(bool),
    Clip {
        part: String,
        playing: bool,
        repeat: bool,
    },
    Arm {
        parts: Vec<String>,
        armed: bool,
    },
    Cue {
        request_id: Option<String>,
        parts: Vec<String>,
        playing: bool,
        repeat: bool,
        count_in_pulses: u8,
    },
    ConductedConfig { project: String, layout: pr0_core::ConductedLayout, revision: u64 },
    ConductedToggle { project: String, part: String, arm: bool },
    Dynamics {
        parts: Vec<String>,
        value: Option<u8>,
    },
    BrowserMidi {
        part: String,
        message: pr0_core::midi::Message,
    },
    BrowserMidiPanic {
        parts: Vec<String>,
    },
    BrowserInput {
        project: String,
        user: String,
        node: String,
        pcm: Vec<[f32; 2]>,
    },
}
pub fn start(
    events: broadcast::Sender<crate::Event>,
    media: broadcast::Sender<crate::media::AudioBlock>,
    logs: Arc<crate::settings::Logs>,
    osc: Arc<crate::osc::Runtime>,
    config: Arc<crate::config::RuntimeConfig>,
) -> SyncSender<Command> {
    let (tx, rx) = sync_channel::<Command>(256);
    std::thread::Builder::new()
        .name("pr0-orchestrator".into())
        .spawn(move || run(events, media, logs, rx, osc, config))
        .expect("Start audio worker");
    tx
}
fn run(
    events: broadcast::Sender<crate::Event>,
    media: broadcast::Sender<crate::media::AudioBlock>,
    logs: Arc<crate::settings::Logs>,
    rx: std::sync::mpsc::Receiver<Command>,
    osc: Arc<crate::osc::Runtime>,
    config: Arc<crate::config::RuntimeConfig>,
) {
    let io = crate::performance::external_worker(osc.clone());
    let node_outputs = crate::node_io::Outputs::new(osc.clone(), config.native_devices);
    let mut midi_inputs = crate::node_io::Inputs::new(config.native_devices);
    let mut node_routes: Vec<(String, crate::node_io::Route, bool)> = Vec::new();
    let mut sequencer: Option<crate::performance::Sequencer> = None;
    let mut audition: Option<AuditionState> = None;
    let mut engine: Option<Engine> = None;
    let mut persistence =
        crate::persistence::Persistence::new(config.loops_dir(), config.recordings_dir.clone());
    let mut shutting_down = false;
    let mut project: Option<Project> = None;
    let mut epoch = String::new();
    let mut runtime_generation=0_u64;
    let mut log_project = String::new();
    let mut outputs: Vec<Output> = vec![];
    let mut enabled = false;
    let mut show_active = false;
    let mut settings = crate::settings::read(&config);
    let mut testing = false;
    let mut test_sample = 0_u64;
    let mut media_rates: std::collections::BTreeMap<String, crate::samples::RateAdapter> =
        Default::default();
    let mut media_packets = crate::monitor_packets::Packets::default();
    let mut input_rates: std::collections::BTreeMap<String, crate::samples::RateAdapter> =
        Default::default();
    let mut logged_underruns = 0;
    let mut error_logged = String::new();
    let mut inputs: Vec<Input> = vec![];
    let mut hardware = false;
    // Set by `Command::Suspend`: the host has closed the native hardware for a
    // platform lifecycle event (an iOS interruption, or the application leaving
    // the foreground). Nothing renders while it is set, so the engine sample
    // clock, transport position, prepared graph, loop buffers and open
    // recordings are all exactly where resume finds them.
    let mut suspended = false;
    // The owner's standing hardware intent, which outlives both suspension and
    // a failed open. See [`Wanted`].
    let mut wanted = Wanted::default();
    let underruns = Arc::new(AtomicU64::new(0));
    let mut last = Instant::now();
    let mut meter_time = Instant::now();
    let mut deadline = Instant::now();
    let mut seq = 0_u64;
    let mut script_logs = std::collections::BTreeMap::new();
    let mut next_tempo: Option<(f64, f64)> = None;
    let mut count_in: Option<pr0_dsp::count_in::CountIn> = None;
    let mut metronome = false;
    let mut metro = pr0_dsp::count_in::Metronome::new();
    let mut cue_metro = pr0_dsp::count_in::Metronome::new();
    let mut device_error = String::new();
    let mut browser: std::collections::BTreeMap<String, std::collections::VecDeque<[f32; 2]>> =
        std::collections::BTreeMap::new();
    // Block-scoped caches. Node ids are resolved to engine indices once per
    // block so the per-sample loop performs no string comparisons or map
    // lookups, and buffers are reused across blocks instead of reallocated.
    let mut storage = vec![[0.; MAX_CHANNELS]; 1024];
    let mut raw: Vec<f32> = Vec::new();
    let mut browser_index: Vec<Option<usize>> = Vec::new();
    let mut input_index: Vec<(usize, Option<usize>)> = Vec::new();
    let mut output_index: Vec<(usize, u32)> = Vec::new();
    let mut route_index: Vec<Option<usize>> = Vec::new();
    let mut monitors: std::collections::BTreeMap<String, Vec<f32>> = Default::default();
    // Newest pending OSC value and last send time per `osc_output` node.
    let mut osc_values: std::collections::BTreeMap<
        String,
        (Option<pr0_core::ControlValue>, Option<Instant>),
    > = Default::default();

    let mut monitor_subscriptions: std::collections::BTreeMap<
        String,
        (String, Option<String>, Instant),
    > = Default::default();
    let mut max_work_us = 0u64;
    let mut max_block_gap_us = 0u64;
    let mut previous_block = Instant::now();
    let mut previews: Vec<Preview> = Vec::new();
    let mut visualization_subscribers: std::collections::BTreeMap<String, (String, Instant)> =
        Default::default();
    loop {
        let cycle_started = Instant::now();
        if shutting_down && persistence.ready() {
            return;
        }
        for _ in 0..16 {
            if shutting_down || !persistence.ready() {
                break;
            }
            let Ok(command) = rx.try_recv() else { break };
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
                Command::Clip { .. } | Command::Cue { .. } => Some("Part cue queued"),
                Command::Arm { .. } => Some("Part arm state changed"),
                Command::Dynamics { .. } => Some("Live dynamics changed"),
                Command::BrowserMidi { .. } => None,
                Command::BrowserMidiPanic { .. } => Some("Browser MIDI notes released"),
                Command::Hardware(_) => Some("Output state requested"),
                Command::Test(_) => Some("Latency metronome state changed"),
                _ => None,
            };

            if let Some(message) = description {
                logs.push(context, "info", message);
            }

            match command {
                Command::Shutdown(reply) => {
                    runtime_generation=runtime_generation.wrapping_add(1);
                    outputs.clear();
                    inputs.clear();
                    // Nothing may reopen a device after this point, including a
                    // lifecycle resume that raced application exit into the
                    // queue behind it.
                    wanted = Wanted::default();
                    midi_inputs = crate::node_io::Inputs::new(config.native_devices);
                    node_outputs.reset(vec![]);
                    if let (Some(p), Some(mut e)) = (&project, engine.take()) {
                        if let Some(seq) = &mut sequencer {
                            seq.reset(&mut e, &io);
                        }
                        persistence.retire(p.id.clone(), e);
                    }
                    persistence.barrier(reply);
                    shutting_down = true;
                }

                Command::Suspend {
                    suspended: requested,
                    reply,
                } => {
                    let context = project
                        .as_ref()
                        .map(|p| p.id.as_str())
                        .unwrap_or(&log_project)
                        .to_owned();
                    let result = if requested {
                        // Dropping the streams closes the hardware. Engine,
                        // sequencer, transport, persistence, node routing and
                        // the owner's `wanted` intent are all deliberately
                        // untouched, so resume restores what the owner asked
                        // for rather than a snapshot of what the system
                        // happened to interrupt.
                        //
                        // Idempotent: a repeated interruption or background
                        // event finds nothing open and closes nothing twice.
                        outputs.clear();
                        inputs.clear();
                        testing = false;
                        suspended = true;
                        Ok(())
                    } else {
                        // The worker's state follows the request whether or not
                        // the devices reopen, exactly as enabling does: a
                        // failure is a device error on a resumed runtime, not a
                        // runtime stuck in a suspended state no host asked for.
                        suspended = false;
                        // Reconciling rather than replaying: resume opens
                        // whichever wanted direction is not open, which makes a
                        // repeat resume both harmless when there is nothing to
                        // do and a retry when the previous one could not open a
                        // device. An owner who stopped hardware output, or who
                        // never enabled the engine, is never started by a
                        // platform lifecycle event, because neither `wanted`
                        // nor `enabled` says so.
                        let missing = wanted.missing(enabled, &outputs, &inputs);
                        if missing.any() {
                            prepare_platform_audio(&config, &settings, &logs, &context);
                            match open_wanted(
                                &config,
                                &settings,
                                &underruns,
                                missing,
                                &mut outputs,
                                &mut inputs,
                            ) {
                                Ok(()) => {
                                    device_error.clear();
                                    Ok(())
                                }
                                Err(error) => {
                                    device_error = error.clone();
                                    Err(error)
                                }
                            }
                        } else {
                            Ok(())
                        }
                    };
                    hardware = !outputs.is_empty();
                    if let Err(error) = &result {
                        logs.push(&context, "error", error);
                    }
                    // A host that stopped waiting still gets the transition.
                    let _ = reply.try_send(result);
                }

                Command::Monitor {
                    session,
                    project,
                    node,
                    enabled,
                } => {
                    if enabled {
                        monitor_subscriptions.insert(session, (project, node, Instant::now()));
                    } else {
                        monitor_subscriptions.remove(&session);
                    }
                }
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
                    midi_inputs = crate::node_io::Inputs::new(config.native_devices);
                    node_outputs.reset(vec![]);
                    log_project = id;
                    outputs.clear();
                    hardware = false;
                    enabled = false;
                    testing = false;
                    inputs.clear();
                    settings = new_settings;
                    media_rates.clear();
                    media_packets = crate::monitor_packets::Packets::default();
                    input_rates.clear();

                    if !value {
                        if let (Some(p), Some(e)) = (&project, engine.take()) {
                            persistence.retire(p.id.clone(), e);
                        }
                        sequencer = None;
                    }
                    // Enabling the engine asks for both directions; disabling
                    // it withdraws both. Recorded before anything is opened, so
                    // a request that arrives while the hardware is closed for a
                    // platform lifecycle event is still the intent resume acts
                    // on — without this, enabling the engine in a backgrounded
                    // application produced a runtime that returned to the
                    // foreground enabled and permanently silent.
                    wanted = Wanted {
                        outputs: value,
                        inputs: value,
                    };
                    let result = if value && suspended {
                        // Hardware is closed for a platform lifecycle event.
                        // The engine still enables; resume opens the devices
                        // the same way this branch would have.
                        enabled = true;
                        Ok(())
                    } else if value {
                        prepare_platform_audio(&config, &settings, &logs, &log_project);
                        open_devices(&config, &settings, underruns.clone(), true, true).map(
                            |(devices, captured)| {
                                outputs = devices;
                                inputs = captured;
                                hardware = !outputs.is_empty();
                                enabled = true;
                                device_error.clear();
                            },
                        )
                    } else {
                        Ok(())
                    };

                    if let Err(err) = &result {
                        device_error = err.clone();
                    }

                    let _=events.send(json!({"type":"audio_engine_status","enabled":enabled,"sample_rate":settings.sample_rate,"block_size":settings.block_size}).into());
                    if value || result.is_err() {
                        let _ = reply.send(result);
                    } else {
                        persistence.barrier(reply);
                    }
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
                            .and_then(|e| e.clear_loop(&node, track))
                    } else {
                        Err("Project engine unavailable".into())
                    };
                    if let Err(e) = result {
                        let _ = reply.send(Err(e));
                    } else {
                        persistence.clear(id, node, track);
                        persistence.barrier(reply);
                    }
                }
                Command::Test(value) => {
                    testing = value;
                    test_sample = 0;
                }
                Command::Clip {
                    part,
                    playing,
                    repeat,
                } => {
                    if let (Some(seq), Some(e)) = (&mut sequencer, &mut engine) {
                        if repeat {
                            seq.launch_repeat(&part, playing, true, e, &io);
                        } else {
                            seq.launch(&part, playing, e, &io);
                        }
                    }
                }
                Command::Arm { parts, armed } => {
                    if let Some(seq) = &mut sequencer {
                        seq.arm(&parts, armed);
                    }
                }
                Command::Cue {
                    request_id,
                    parts,
                    playing,
                    repeat,
                    count_in_pulses,
                } => {
                    if let (Some(seq), Some(e)) = (&mut sequencer, &mut engine) {
                        seq.cue_request(
                            request_id,
                            &parts,
                            playing,
                            repeat,
                            count_in_pulses,
                            e,
                            &io,
                        );
                    }
                }
                Command::ConductedConfig { project: id, layout, revision } => {
                    if let Some(p) = &mut project {
                        if p.id == id && p.revision <= revision { p.conducted = layout; p.revision = revision; }
                    }
                }
                Command::ConductedToggle { project: id, part, arm } => {
                    if project.as_ref().is_some_and(|p| p.id == id) {
                        if let (Some(seq), Some(e)) = (&mut sequencer, &mut engine) {
                            if let Some(state) = seq.playback(e.clock.beat).into_iter().find(|s| s.id == part) {
                                if arm {
                                    if !state.armed {
                                        let p=project.as_ref().unwrap();
                                        if let Some(performer)=p.parts.iter().find(|p|p.id==part).and_then(|p|p.performer.as_ref()) {
                                            let conflicts=p.parts.iter().filter(|p|p.id!=part&&p.performer.as_ref()==Some(performer)).map(|p|p.id.clone()).collect::<Vec<_>>();
                                            seq.arm(&conflicts,false);
                                        }
                                    }
                                    seq.arm(&[part], !state.armed);
                                }
                                else {
                                    let playing = !state.pending.map(|(_, on)| on).unwrap_or(state.playing || state.queue_position.is_some());
                                    let count = project.as_ref().unwrap().conducted.count_in_pulses;
                                    seq.cue_request(None, &[part], playing, false, if playing {count} else {0}, e, &io);
                                }
                            }
                        }
                    }
                }
                Command::Dynamics { parts, value } => {
                    if let (Some(seq), Some(e)) = (&mut sequencer, &mut engine) {
                        seq.dynamics(&parts, value, e, &io);
                    }
                }
                Command::BrowserMidi { part, message } => {
                    if let (Some(seq), Some(e)) = (&mut sequencer, &mut engine) {
                        seq.browser_midi(&part, message, e, &io);
                    }
                }
                Command::BrowserMidiPanic { parts } => {
                    if let (Some(seq), Some(e)) = (&mut sequencer, &mut engine) {
                        seq.browser_midi_panic(&parts, e, &io);
                    }
                }
                Command::BrowserInput { project: source_project, user: source_user, node, pcm } => {
                    if !project.as_ref().is_some_and(|p|p.id==source_project&&p.local_audio_assignments.get(&node)==Some(&source_user)) {continue;}
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
                Command::Snapshot {project:id,reply} => {
                    let result=match (&project,&engine) {
                        (Some(p),Some(e)) if p.id==id => {let mut p=p.clone(); e.snapshot_options(&mut p.graph); Ok((p,runtime_generation))},
                        _=>Err("Engine is not enabled for this project".into()),
                    };
                    let _=reply.send(result);
                }
                Command::Replace {
                    recall,
                    project: p,
                    engine: prepared,
                } => {
                    if let Some(r)=&recall {
                        if runtime_generation!=r.runtime_generation || r.current.load(std::sync::atomic::Ordering::SeqCst)!=r.generation || !project.as_ref().is_some_and(|old|old.id==p.id && old.revision==p.revision) {
                            let _=recall.unwrap().reply.send(Err("Recall superseded by a runtime change".into()));
                            persistence.discard(prepared);
                            continue;
                        }
                    }
                    if let (Some(e), Some((part, staff, node, channel, pitch, _))) =
                        (engine.as_mut(), audition.take())
                    {
                        audition_note(e, &part, staff, node.as_deref(), channel, pitch, 0);
                    }
                    runtime_generation=runtime_generation.wrapping_add(1);
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
                        if recall.is_some() { prepared.carry_recall_state(previous); } else { prepared.carry_node_state(previous); }
                        if let Some(seq) = &sequencer {
                            seq.seed_part_nodes(previous, &mut prepared);
                        }
                    }
                    if let Some(old)=&project {
                        for node in &p.graph.nodes {
                            if node.kind=="browser_input" && old.local_audio_assignments.get(&node.id)!=p.local_audio_assignments.get(&node.id) {
                                browser.remove(&node.id);input_rates.remove(&node.id);prepared.external(&node.id,[0.;MAX_CHANNELS]);
                            }
                        }
                    }
                    previews.clear();
                    for out in &mut outputs {
                        out.meter.clear();
                    }
                    if let (Some(old), Some(previous)) = (&project, engine.take()) {
                        persistence.retire(old.id.clone(), previous);
                    }
                    let success = if let Some(r)=recall {
                        prepared.republish_restored(&r.restored);
                        let event=json!({"type":"state_recalled","project_id":p.id,"event_id":uuid::Uuid::new_v4().to_string(),"node":r.node,"restored":r.restored,"skipped":r.skipped,"nodes":p.graph.nodes.iter().map(pr0_core::states::Options::capture).collect::<Vec<_>>()});
                        Some((r.reply,event))
                    } else { None };
                    let _=events.send(json!({"type":"effective_options","project_id":p.id,"nodes":p.graph.nodes.iter().map(pr0_core::states::Options::capture).collect::<Vec<_>>()}).into());
                    project = Some(p);
                    engine = Some(*prepared);
                    if let Some((reply,event))=success {let _=events.send(event.clone().into());let _=reply.send(Ok(event));}
                }

                Command::Load(p, mut e) => {
                    runtime_generation=runtime_generation.wrapping_add(1);
                    max_work_us = 0;
                    max_block_gap_us = 0;
                    previous_block = Instant::now();
                    persistence.reset();
                    if let (Some(old), Some(previous)) = (&project, engine.take()) {
                        persistence.retire(old.id.clone(), previous);
                    }
                    count_in = None;
                    node_outputs.reset(vec![]);
                    node_routes = prepare_node_routes(&p);
                    midi_inputs.configure(&p.graph);
                    for out in &mut outputs {
                        out.meter.clear();
                    }
                    sequencer = Some(crate::performance::Sequencer::new(&p));
                    e.clock.bpm = p
                        .score
                        .as_ref()
                        .and_then(|s| s.tempo_at(0.))
                        .unwrap_or(p.bpm);
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
                    runtime_generation=runtime_generation.wrapping_add(1);
                    audition = None;
                    if let (Some(p), Some(e)) = (&project, engine.take()) {
                        persistence.retire(p.id.clone(), e);
                    }
                    show_active = false;
                    count_in = None;
                    midi_inputs = crate::node_io::Inputs::new(config.native_devices);
                    node_routes.clear();
                    node_outputs.reset(vec![]);
                    for out in &mut outputs {
                        out.meter.clear();
                    }
                    inputs.clear();
                    // Deactivating the show closes capture, so the intent goes
                    // with it: a later platform resume must not reopen a
                    // microphone for a runtime with no project loaded. Output
                    // is left exactly as it is, because this does not close it.
                    wanted.inputs = false;
                    previews.clear();
                    let _ = io.try_send(crate::performance::External::Panic);
                    sequencer = None;
                    engine = None;
                    project = None;
                    browser.clear();
                    media_rates.clear();
                    media_packets = crate::monitor_packets::Packets::default();
                    input_rates.clear();
                    testing = false;
                }
                Command::Parameter {
                    node,
                    key,
                    value,
                    revision,
                } => {
                    runtime_generation=runtime_generation.wrapping_add(1);
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
                            let script_accepted = crate::scripts::osc_event(&message, e);
                            let accepted = match crate::osc::action(&message) {
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
                                        project.as_ref().map_or(4, |p| p.initial_meter().1),
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
                                                    // OSC notes carry no channel; channel 1.
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
                                    if let (Some(value), Some(p)) =
                                        (crate::node_io::osc_value(&message), &project)
                                    {
                                        for node in &p.graph.nodes {
                                            if node.kind == "osc_input"
                                                && node
                                                    .io
                                                    .as_ref()
                                                    .is_some_and(|io| io.address == message.addr)
                                            {
                                                matched |= e.osc_input(&node.id, &value);
                                            }
                                        }
                                    }
                                    matched || script_accepted
                                }
                            };
                            accepted || script_accepted
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
                Command::LiveControl { project: id, node, value, revision } => {
                    runtime_generation=runtime_generation.wrapping_add(1);
                    if project.as_ref().is_some_and(|p| p.id == id && p.revision == revision) {
                        if let Some(e) = engine.as_mut() { e.external_control(&node, &pr0_core::ControlValue::Number(value)); }
                    }
                }
                Command::Control {
                    node,
                    value,
                    revision,
                } => {
                    runtime_generation=runtime_generation.wrapping_add(1);
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
                Command::Audition {
                    project: id,
                    part,
                    staff,
                    node,
                    channel,
                    pitch,
                    velocity,
                } => {
                    if project.as_ref().is_some_and(|p| p.id == id) {
                        if let Some(e) = engine.as_mut() {
                            if let Some((part, staff, node, channel, pitch, _)) = audition.take() {
                                audition_note(e, &part, staff, node.as_deref(), channel, pitch, 0);
                            }
                            audition_note(
                                e,
                                &part,
                                staff,
                                node.as_deref(),
                                channel,
                                pitch,
                                velocity,
                            );
                            audition = Some((
                                part,
                                staff,
                                node,
                                channel,
                                pitch,
                                (settings.sample_rate as usize / 3).max(1),
                            ));
                        }
                    }
                }
                Command::LocalMidi {
                    project: id,
                    node,
                    message,
                } => {
                    if project.as_ref().is_some_and(|p| {
                        p.id == id
                            && p.graph
                                .nodes
                                .iter()
                                .any(|n| n.id == node && n.kind == "local_midi_input")
                    }) {
                        if let Some(e) = engine.as_mut() {
                            if let Some(message) = message {
                                e.node_midi_message(&node, message);
                            } else {
                                e.node_midi_reset(&node);
                                for channel in 1..16 {
                                    e.node_midi_message(
                                        &node,
                                        pr0_core::midi::Message {
                                            status: 0xb0 | channel,
                                            data1: 123,
                                            data2: 0,
                                        },
                                    );
                                }
                            }
                        }
                    }
                }
                Command::Controller {
                    project: id,
                    node,
                    index,
                    value,
                    cancel,
                } => {
                    runtime_generation=runtime_generation.wrapping_add(1);
                    if project.as_ref().is_some_and(|p| p.id == id) {
                        if let Some(e) = engine.as_mut() {
                            if cancel {
                                e.cancel_controller_learn(&node, index);
                            } else {
                                e.controller(&node, index, value);
                            }
                        }
                    }
                }
                Command::Piano {
                    project: id,
                    node,
                    message,
                } => {
                    if project.as_ref().is_some_and(|p| p.id == id) {
                        if let Some(e) = engine.as_mut() {
                            if message.status == 0xe0 {
                                e.node_midi_message(&node, message);
                            } else {
                                e.piano_note(&node, message.data1, message.data2);
                            }
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
                            project.as_ref().map_or(4, |p| p.initial_meter().1),
                            e,
                            &mut sequencer,
                            &mut count_in,
                            &mut next_tempo,
                            &io,
                        );
                    }
                }
                Command::Seek(beat) => {
                    if let Some(e) = engine.as_mut() {
                        let beat = beat.max(0.0);
                        e.clock.beat = beat;
                        e.graph_clock.beat = beat;
                        e.clock.reset_generation = e.clock.reset_generation.wrapping_add(1);
                        e.graph_clock.reset_generation =
                            e.graph_clock.reset_generation.wrapping_add(1);
                    }
                }
                Command::Metronome(value) => metronome = value,
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
                    let _=reply.send(json!({"worker_max_work_us":max_work_us,"worker_max_block_gap_us":max_block_gap_us,"block_size":settings.block_size,"sample_rate":settings.sample_rate,"engine_enabled":enabled,"hardware_enabled":hardware,"audio_suspended":suspended,"input_enabled":!inputs.is_empty(),"active_inputs":inputs.iter().map(|i|i.id).collect::<Vec<_>>(),"underruns":underruns.load(Ordering::Relaxed),"error":device_error}));
                }
                Command::Hardware(value) => {
                    // The owner's hardware-output intent, recorded whether or
                    // not a device can be touched right now. While the hardware
                    // is closed for a platform lifecycle event this is all the
                    // command does, and resume then honours the latest request
                    // — starting output that was asked for while suspended, and
                    // leaving output that was stopped while suspended closed.
                    wanted.outputs = value;
                    outputs.clear();
                    hardware = false;
                    if value && !suspended {
                        prepare_platform_audio(&config, &settings, &logs, &log_project);
                        match open_outputs(&config, &settings, underruns.clone()) {
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

        // Suspended: the hardware is closed and nothing is rendered. The engine
        // is not advanced on the software schedule either, so a backgrounded or
        // interrupted application resumes from the engine sample it left rather
        // than racing the wall clock forward in silence. Commands are still
        // drained above, so shutdown and resume both arrive normally.
        if suspended {
            std::thread::sleep(Duration::from_millis(10));
            // Resume renders its next block immediately instead of catching up
            // on the whole suspended interval.
            deadline = Instant::now();
            previous_block = Instant::now();
            continue;
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
            // `wanted.outputs` deliberately stays set. The owner still asked for
            // hardware output; a route that disappeared under the stream is the
            // case the host answers with a suspend/resume pair, and resume has
            // to be able to reopen on the replacement route.
        }
        if outputs.first().is_some_and(|o| !o.buffer.needs_frames()) {
            std::thread::sleep(Duration::from_micros(500));
            continue;
        }

        if let Some(e) = engine.as_mut() {
            max_block_gap_us = max_block_gap_us.max(previous_block.elapsed().as_micros() as u64);
            previous_block = Instant::now();
            monitor_subscriptions.retain(|_, (_, _, seen)| seen.elapsed() < Duration::from_secs(8));
            let output = &mut storage[..settings.block_size];

            // Rebuild the dedicated-monitor set only when subscriptions change;
            // otherwise clear and reuse each feed's buffer.
            let subscribed = |p: &Project, n: &pr0_core::Node| {
                n.kind == "monitor_output"
                    && monitor_subscriptions
                        .values()
                        .any(|(id, node, _)| id == &p.id && node.as_ref() == Some(&n.id))
            };
            match (&project, media.receiver_count() > 0) {
                (Some(p), true) => {
                    let wanted = p.graph.nodes.iter().filter(|n| subscribed(p, n));
                    let unchanged = wanted.clone().count() == monitors.len()
                        && wanted.clone().all(|n| monitors.contains_key(&n.id));
                    if unchanged {
                        for pcm in monitors.values_mut() {
                            pcm.clear();
                        }
                    } else {
                        monitors = wanted
                            .map(|n| (n.id.clone(), Vec::with_capacity(settings.block_size * 2)))
                            .collect();
                    }
                }
                _ => monitors.clear(),
            }
            browser_index.clear();
            browser_index.extend(browser.keys().map(|node| e.node_index(node)));
            input_index.clear();
            output_index.clear();
            if let Some(p) = &project {
                for n in &p.graph.nodes {
                    let interface = n.parameters.get("interface").copied().unwrap_or(0.) as u32;
                    match (n.kind.as_str(), e.node_index(&n.id)) {
                        ("input", Some(index)) => input_index.push((
                            index,
                            inputs
                                .iter()
                                .position(|i| interface == 0 || i.id == interface),
                        )),
                        ("output", Some(index)) => output_index.push((index, interface)),
                        _ => {}
                    }
                }
            }
            route_index.clear();
            route_index.extend(node_routes.iter().map(|(node, _, _)| e.node_index(node)));
            let monitor_index: Vec<(usize, Option<&str>)> = monitors
                .keys()
                .map(|id| {
                    (
                        e.node_index(id).unwrap_or(usize::MAX),
                        project
                            .as_ref()
                            .and_then(|p| p.graph.nodes.iter().find(|n| n.id == *id))
                            .and_then(|n| n.part_id.as_deref()),
                    )
                })
                .collect();

            if let Some(p) = &project {
                midi_inputs.drain(&p.graph, e);
            }
            if media.receiver_count() == 0 {
                media_rates.clear();
                media_packets = crate::monitor_packets::Packets::default();
            }
            // Split rendering at the exact tempo boundary;
            // No browser timer drives DSP.
            for frame in output.iter_mut() {
                for ((_, q), index) in browser.iter_mut().zip(&browser_index) {
                    let sample = q.pop_front().unwrap_or([0.; 2]);
                    if let Some(index) = *index {
                        let mut audio = [0.; MAX_CHANNELS];
                        audio[..2].copy_from_slice(&sample);
                        e.external_at(index, audio);
                    }
                }
                if let Some((beat, bpm)) = next_tempo {
                    if e.clock.beat >= beat {
                        e.clock.set_tempo(bpm);
                        next_tempo = None;
                    }
                }
                let count_click = advance_count_in(&mut count_in, e);
                if let Some(seq) = &mut sequencer {
                    seq.tick(e, &io);
                }
                let cue_count_in = sequencer
                    .as_ref()
                    .is_some_and(|s| s.cue_count_in(e.clock.beat));
                let (position, origin, beats, unit) = sequencer
                    .as_ref()
                    .map(|s| s.metronome_position(e.clock.beat))
                    .unwrap_or((e.clock.beat, 0., 4, 4));
                e.set_part_player_metronome(e.clock.running.then_some((position, origin, unit)));
                let metro_click = if metronome {
                    metro.next_at(&e.clock, position, origin, beats, unit)
                } else {
                    metro = pr0_dsp::count_in::Metronome::new();
                    None
                };
                let cue_click = if cue_count_in {
                    let unit = project
                        .as_ref()
                        .map(|p| p.conducted.pulse_unit)
                        .unwrap_or(4);
                    cue_metro.next_at(&e.clock, e.clock.beat, 0., 1, unit)
                } else {
                    cue_metro = pr0_dsp::count_in::Metronome::new();
                    None
                };
                let click = match (count_click, metro_click) {
                    (None, None) => None,
                    (a, b) => Some(a.unwrap_or(0.) + b.unwrap_or(0.)),
                };
                for input in &mut inputs {
                    input.sample();
                }
                for &(index, device) in &input_index {
                    let sample = device
                        .map(|d| inputs[d].frame)
                        .unwrap_or([0.; MAX_DEVICE_CHANNELS]);
                    e.device_input_at(index, sample);
                }
                advance_audition(e, &mut audition);
                e.render(&[], std::slice::from_mut(frame));
                // Typed cables reach a MIDI output raw; scalar cables are decoded
                // into note events. The engine keeps the two disjoint per node.
                for ((node, route, cc), index) in node_routes.iter().zip(&route_index) {
                    let Some(index) = *index else { continue };
                    if let crate::node_io::Route::OscValue(..) = route {
                        if let Some(value) = e.take_osc_output_at(index) {
                            osc_values.entry(node.clone()).or_insert((None, None)).0 = Some(value);
                        }
                        continue;
                    }
                    while let Some(message) = e.take_midi_message_at(index) {
                        if enabled {
                            node_outputs.message(node, route, message)
                        }
                    }
                    for event in e.take_midi_output_at(index).into_iter().flatten() {
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
                        for &(index, route) in &output_index {
                            if route == out.id || route == 0 {
                                let v = e.device_output_frame_at(index);
                                for (sum, value) in routed.iter_mut().zip(v) {
                                    *sum += value;
                                }
                            }
                        }
                    }
                    out.meter.observe(&routed);
                    out.push(routed);
                }

                for ((_, pcm), (index, part)) in monitors.iter_mut().zip(&monitor_index) {
                    let mut sample = e.monitor_frame_at(*index);
                    if let Some(click) = click {
                        for ch in &mut sample {
                            *ch += click;
                        }
                    }
                    if let Some(cue_click) = cue_click {
                        let receives_cue = part.is_some_and(|part| {
                            sequencer
                                .as_ref()
                                .is_some_and(|seq| seq.cue_count_in_for_part(e.clock.beat, part))
                        });
                        if receives_cue {
                            for ch in &mut sample {
                                *ch += cue_click;
                            }
                        }
                    }
                    pcm.extend_from_slice(&sample);
                }
                // Shared transport/metronome clicks join every browser monitor. Conducted
                // cue clicks are added only to a part-associated dedicated feed above.
                if let Some(click) = click {
                    for ch in frame {
                        *ch += click;
                    }
                }
            }

            for (node, selector) in e.take_state_requests() {
                if project.as_ref().and_then(|p|p.graph.nodes.iter().find(|n|n.id==node)).and_then(|n|n.states.as_ref()).and_then(|b|b.select(&selector)).is_some() {
                    let _=events.send(json!({"type":"state_request","runtime_generation":runtime_generation,"project_id":project.as_ref().unwrap().id,"node":node,"selector":selector}).into());
                }
            }
            // Bounded OSC value output: the newest pending value goes out once
            // per node interval; intermediate values are coalesced, never queued.
            osc_values.retain(|node, _| node_routes.iter().any(|(id, _, _)| id == node));
            for (node, (pending, last_sent)) in &mut osc_values {
                let Some((_, route, _)) = node_routes.iter().find(|(id, _, _)| id == node) else {
                    continue;
                };
                let crate::node_io::Route::OscValue(_, _, rate) = route else {
                    continue;
                };
                let interval = Duration::from_secs_f64(1. / f64::from(*rate).max(1.));
                if !last_sent.is_none_or(|sent: Instant| sent.elapsed() >= interval) {
                    continue;
                }
                if let Some(value) = pending.take() {
                    if enabled {
                        node_outputs.value(route, value);
                    }
                    *last_sent = Some(Instant::now());
                }
            }
            if let Some(p) = &project {
                persistence.poll(&p.id, e);
            }
            previews.retain_mut(|preview| {
                if preview.reply.as_ref().is_none_or(|r|r.is_closed()){return false;}
                if preview.frames.len()<256{return true;}
                let channels:Vec<Vec<f32>>=(0..preview.channels).map(|ch|preview.frames.iter().map(|f|f[ch]).collect()).collect();
                let _=preview.reply.take().unwrap().send(Ok(json!({"channels":channels,"sample_rate":settings.sample_rate,"sample":e.clock.sample})));false
            });
            if media.receiver_count() > 0 {
                if let Some(p) = &project {
                    if media_packets.configure(&p.id, settings.sample_rate, &monitors) {
                        media_rates.clear();
                    }
                    raw.clear();
                    raw.extend(output.iter().flat_map(|f| [f[0], f[1]]));
                    let pcm = media_rates
                        .entry(String::new())
                        .or_insert_with(|| {
                            crate::samples::RateAdapter::new(settings.sample_rate, 48000)
                        })
                        .process(&raw);
                    for (id, pcm) in &mut monitors {
                        if !media_rates.contains_key(id) {
                            media_rates.insert(
                                id.clone(),
                                crate::samples::RateAdapter::new(settings.sample_rate, 48000),
                            );
                        }
                        *pcm = media_rates.get_mut(id).unwrap().process(pcm);
                    }
                    media_packets.push(&pcm, &monitors, |packet| {
                        let _ = media.send(packet);
                    });
                }
            }

            max_work_us = max_work_us.max(cycle_started.elapsed().as_micros() as u64);
            if last.elapsed() >= Duration::from_millis(50) {
                last = Instant::now();
                seq += 1;
                visualization_subscribers
                    .retain(|_, (_, seen)| seen.elapsed() < Duration::from_secs(10));
                if let Some(p) = &project {
                    let visualize = visualization_subscribers
                        .values()
                        .any(|(id, seen)| id == &p.id && seen.elapsed() < Duration::from_secs(10));
                    if visualize {
                        e.analyze_visualizers();
                    }
                    let (debug, dropped)=e.take_console_entries();
                    let scripts = crate::scripts::telemetry(e, &logs, p, &mut script_logs);
                    for entry in debug {
                        let value=match entry.value {pr0_core::ControlValue::Number(value)=>value.to_string(),pr0_core::ControlValue::Text(value)=>serde_json::to_string(&value).unwrap_or_default()};
                        logs.push(&p.id,"info",&format!("Console Out [{} · {}] sample {}: {}",entry.label,entry.node,entry.sample,value));
                    }
                    if dropped>0 { logs.push(&p.id,"warn",&format!("Console Out: dropped {dropped} values because debug capture filled (128 entries per telemetry interval)")); }
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
                    let _=events.send(json!({"scripts":scripts,
"type":"telemetry","project_id":p.id,"revision":p.revision,"epoch":epoch,"sequence":seq,"server_time":monotonic_ms(),"sample":e.clock.sample,"beat":e.clock.beat,"graph_beat":e.graph_clock.beat,"bpm":e.clock.bpm,"running":e.clock.running,"count_in_remaining":count_in.as_ref().map(|c|c.remaining()),"metronome":metronome,"midi_input_error":midi_inputs.error,"node_io":node_outputs.status(),"worker_max_work_us":max_work_us,"worker_max_block_gap_us":max_block_gap_us,"block_size":settings.block_size,"sample_rate":settings.sample_rate,"engine_enabled":enabled,"hardware_enabled":hardware,"audio_suspended":suspended,"input_enabled":!inputs.is_empty(),"active_inputs":inputs.iter().map(|i|i.id).collect::<Vec<_>>(),"underruns":underruns.load(Ordering::Relaxed),"error":persistence.error().unwrap_or_else(|| device_error.clone()),"parts":sequencer.as_ref().map(|s|s.playback(e.clock.beat)).unwrap_or_default(),"values":e.telemetry(),"osc_messages":e.osc_messages(),"route_targets":e.route_targets(),"feedback_edges":e.feedback_edges(),"visualizations":if visualize{e.visualizations()}else{Default::default()}}
).into());
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
).into());
            }
        }

        if inputs.iter().any(|i| i.errors.load(Ordering::Relaxed) > 0) {
            inputs.clear();
            device_error =
                "Native input stream failed. Refresh devices and enable capture again.".into();
            // As with output above, the intent survives the failure so a later
            // platform resume retries capture on whatever route replaced it.
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
            let _ = events.send(json!({"type":"hardware_levels","project_id":project.as_ref().map(|p|&p.id),"server_time":monotonic_ms(),"inputs":input_levels,"outputs":output_levels}).into());
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

// Potentially slow OS enumeration. Call from spawn_blocking, never the audio worker.
pub fn device_inventory(config: &crate::config::RuntimeConfig, sample_rate: u32) -> Value {
    if !config.native_devices {
        return json!({"input_interfaces":[],"outputs":[],"interfaces":[],"midi_outputs":[],"midi_inputs":[],"midi_error":null});
    }
    let devices = output_devices(config);
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
    json!({"input_interfaces":device_details(config,sample_rate,true),"outputs":devices.iter().map(|d|&d.1).collect::<Vec<_>>(),"interfaces":device_details(config,sample_rate,false),"midi_outputs":midi,"midi_inputs":midi_inputs,"midi_error":midi_error})
}

// Discovery and stream startup use the same widest supported f32 configuration.
//
// Only asked of platforms that enumerate devices. Platforms that present
// logical routes answer from [`crate::native_audio`] instead — see
// [`stream_config`] — because `supported_input_configs` is CPAL's aborting path
// on iOS.
#[cfg(not(target_os = "ios"))]
fn device_config(
    device: &cpal::Device,
    rate: u32,
    input: bool,
) -> Result<cpal::StreamConfig, String> {
    let route_channels = device.name().ok().filter(|name| name.starts_with("pulse:DEVICE="))
        .and_then(|name|crate::linux_audio::route(&name,input))
        .and_then(|v|v["channels"].as_u64()).map(|v|v as u16);
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
                && route_channels.is_none_or(|channels|c.channels()==channels)
                && (1..=MAX_DEVICE_CHANNELS).contains(&(c.channels() as usize))
                && c.min_sample_rate().0 <= rate
                && c.max_sample_rate().0 >= rate
        })
        .max_by_key(|c| c.channels())
        .map(|c| c.with_sample_rate(cpal::SampleRate(rate)).config())
        .ok_or_else(|| format!("No supported 1–64-channel f32 configuration at {rate} Hz"))
}
/// The configuration a stream is opened with, from whichever device model this
/// platform uses.
///
/// The one place the two models meet. Enumerating platforms negotiate against
/// the endpoint's reported formats exactly as before; logical-route platforms
/// synthesize the configuration from the host's route capability, because
/// asking CPAL what an iOS device supports constructs and initializes an audio
/// unit and can abort the process.
#[cfg(target_os = "ios")]
fn stream_config(
    config: &crate::config::RuntimeConfig,
    _device: &cpal::Device,
    rate: u32,
    input: bool,
) -> Result<cpal::StreamConfig, String> {
    crate::native_audio::logical_stream_config(config, rate, input)
}

#[cfg(not(target_os = "ios"))]
fn stream_config(
    _config: &crate::config::RuntimeConfig,
    device: &cpal::Device,
    rate: u32,
    input: bool,
) -> Result<cpal::StreamConfig, String> {
    device_config(device, rate, input)
}

fn device_details(
    config: &crate::config::RuntimeConfig,
    rate: u32,
    input: bool,
) -> Vec<Value> {
    if !config.native_devices {
        return vec![];
    }
    // Keep metadata, not device handles: retaining ALSA handles can reserve
    // physical PCMs. Single-flight caching prevents every browser refresh from
    // repeatedly opening every device in both directions. Enumeration is a
    // machine-wide fact, but the route identities below are derived from a
    // runtime's saved settings, so the cache is keyed by settings file too and
    // two runtimes in one process never read each other's route IDs.
    type Inventory = (Instant, u32, std::path::PathBuf, [Vec<Value>; 2]);
    static CACHE: OnceLock<std::sync::Mutex<Option<Inventory>>> = OnceLock::new();
    let settings_path = config.audio_settings_path();
    let mut cache = CACHE.get_or_init(Default::default).lock().unwrap();
    if cache.as_ref().is_none_or(|(at, r, path, _)| {
        *r != rate || *path != settings_path || at.elapsed() >= Duration::from_secs(30)
    }) {
        *cache = Some((Instant::now(), rate, settings_path, inventory(config, rate)));
    }
    cache.as_ref().unwrap().3[usize::from(input)].clone()
}

/// The routes this platform publishes, as `[outputs, inputs]`.
///
/// Logical-route platforms have their own implementation and *do not compile*
/// the enumeration below. That is the point of splitting it here rather than
/// branching at run time: on iOS, `cpal::Host::devices` yields one device whose
/// `supports_input` initializes a RemoteIO audio unit, and AudioToolbox aborts
/// the process when that RPC times out. A compile-time boundary means no later
/// edit can reintroduce the call by relaxing a condition.
#[cfg(target_os = "ios")]
fn inventory(config: &crate::config::RuntimeConfig, rate: u32) -> [Vec<Value>; 2] {
    crate::native_audio::logical_rows(config, rate)
}

#[cfg(not(target_os = "ios"))]
fn inventory(config: &crate::config::RuntimeConfig, rate: u32) -> [Vec<Value>; 2] {
    let mut rows: [Vec<Value>; 2] = Default::default();
    if let Ok(devices) = cpal::default_host().devices() {
        let devices:Vec<_>=devices.collect();
        let labels:Vec<_>=devices.iter().filter_map(|d|d.name().ok()).collect();
        let saved=crate::settings::read(config);
        for device in devices {
            let Ok(label) = device.name() else { continue };
            let Ok(name) = device_key(&device) else { continue };
            for (index, direction) in [false, true].into_iter().enumerate() {
                // Unsupported directions are not selectable routes.
                let supported = if direction {device.supports_input()} else {device.supports_output()};
                if !supported { continue; }
                let previous:Vec<_>=if direction {saved.input_interfaces.iter().map(|i|(i.id,i.name.as_str())).collect()}
                    else {saved.interfaces.iter().map(|i|(i.id,i.name.as_str())).collect()};
                let unique=labels.iter().filter(|n|**n==label).count()==1;
                let id=route_id(&name,&label,unique,&previous);
                let display=if !unique && name!=label {format!("{label} · {name}")}else{label.clone()};
                let config = device_config(&device, rate, direction);
                rows[index].push(json!({"id":id,"name":name,"label":display,"backend":if cfg!(target_os="windows"){"WASAPI (shared)"}else if cfg!(target_os="linux"){"ALSA"}else{"CoreAudio"},
                    "channels":config.as_ref().map(|c|c.channels).ok(),"error":config.err()}));
            }
        }
    }
    #[cfg(target_os="linux")]
    for route in crate::linux_audio::discover_routes() {
        let name=route["name"].as_str().unwrap();
        let input=route["input"].as_bool().unwrap();
        let device:cpal::Device=cpal::platform::AlsaDevice::from_pcm_name(name).into();
        let config=device_config(&device,rate,input);
        rows[usize::from(input)].push(json!({"id":device_id(name),"name":name,"label":route["label"],
            "backend":route["backend"],"channels":config.as_ref().map(|c|c.channels).ok(),"error":config.err()}));
    }
    rows
}

#[cfg(not(target_os = "ios"))]
fn device_key(device:&cpal::Device)->Result<String,String> {
    #[cfg(target_os="windows")]
    if let cpal::platform::DeviceInner::Wasapi(device)=device.as_inner() {
        return device.endpoint_id().map(|id|format!("wasapi:{id}")).map_err(|e|e.to_string());
    }
    device.name().map_err(|e|e.to_string())
}
#[cfg(not(target_os = "ios"))]
fn route_id(key:&str,label:&str,unique:bool,previous:&[(u32,&str)])->u32 {
    previous.iter().find(|(_,name)|*name==key || (key.starts_with("wasapi:") && unique && *name==label))
        .map(|(id,_)|*id).unwrap_or_else(||device_id(key))
}
/// Resolves a saved route to a CPAL device on a platform that enumerates them.
///
/// Logical-route platforms have their own resolution in
/// [`crate::native_audio::resolve_logical`], which is not merely a shortcut:
/// enumerating to find a match calls `supports_input`, and therefore
/// `AudioUnitInitialize`, on the single device CPAL's iOS backend reports.
#[cfg(not(target_os = "ios"))]
fn selected_device(name:&str,input:bool)->Result<cpal::Device,String> {
    #[cfg(target_os="linux")]
    if name.starts_with("pulse:DEVICE=") || name.starts_with("hw:CARD=") {
        if crate::linux_audio::route(name,input).is_none() {
            return Err(format!("Selected route is unavailable; refresh devices: {name}"));
        }
        return Ok(cpal::platform::AlsaDevice::from_pcm_name(name).into());
    }
    let mut matching=cpal::default_host().devices().map_err(|e|e.to_string())?
        .filter(|d|if input {d.supports_input()} else {d.supports_output()})
        .filter(|d|device_key(d).ok().as_deref()==Some(name) || (!name.starts_with("wasapi:") && d.name().ok().as_deref()==Some(name)));
    let device=matching.next().ok_or_else(||format!("Interface unavailable: {name}"))?;
    if matching.next().is_some(){return Err(format!("Ambiguous interface name: {name}; select a specific endpoint in System settings"));}
    Ok(device)
}

/// Resolves a saved logical route on a platform that presents them.
#[cfg(target_os = "ios")]
fn selected_device(name: &str, input: bool) -> Result<cpal::Device, String> {
    crate::native_audio::resolve_logical(name, input)
}

fn open_input(
    runtime: &crate::config::RuntimeConfig,
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
    let config = stream_config(runtime, &device, rate, true)?;
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
pub(crate) fn device_id(name: &str) -> u32 {
    name.bytes()
        .fold(2166136261_u32, |h, b| (h ^ b as u32).wrapping_mul(16777619))
        % 999999999
        + 1
}
pub fn input_devices(config: &crate::config::RuntimeConfig) -> Vec<(u32, String)> {
    if !config.native_devices {
        return vec![];
    }
    device_details(config, crate::settings::read(config).sample_rate, true).iter()
        .filter_map(|d| Some((d["id"].as_u64()? as u32, d["name"].as_str()?.to_owned()))).collect()
}
fn open_inputs(
    config: &crate::config::RuntimeConfig,
    settings: &crate::settings::Settings,
) -> Result<Vec<Input>, String> {
    let mut inputs = vec![];
    // Authorization is checked once, before any capture device is touched, and
    // only when the settings actually select one: a runtime with no enabled
    // input must never make the system ask for a microphone. On the
    // orchestration worker, never in a callback, and never blocking on a person
    // — see [`crate::native_audio::authorize_capture`].
    if settings.input_interfaces.iter().any(|i| i.enabled) && config.native_devices {
        crate::native_audio::authorize_capture(config)?;
    }
    for selected in settings.input_interfaces.iter().filter(|i| i.enabled) {
        if !config.native_devices {
            return Err(NATIVE_DEVICES_DISABLED.into());
        }
        let device = selected_device(&selected.name,true)?;
        let errors = Arc::new(AtomicU64::new(0));
        let (stream, queue, channels) =
            open_input(config, device, settings.sample_rate, errors.clone())
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
    fn endpoint_ids_survive_renames_and_legacy_upgrade_without_merging_duplicate_names() {
        assert_eq!(super::route_id("wasapi:endpoint-a","Speakers",true,&[(42,"Speakers")]),42);
        assert_eq!(super::route_id("wasapi:endpoint-a","Renamed",true,&[(42,"wasapi:endpoint-a")]),42);
        assert_ne!(super::route_id("wasapi:endpoint-a","Speakers",false,&[(42,"Speakers")]),42);
        assert_ne!(super::route_id("wasapi:endpoint-a","Speakers",false,&[]),super::route_id("wasapi:endpoint-b","Speakers",false,&[]));
        assert_eq!(super::route_id("CoreAudio name","CoreAudio name",true,&[]),super::device_id("CoreAudio name"));
    }
    /// Enabling the engine and resuming suspended hardware share one opening
    /// path, so both directions come back together or not at all: a failure
    /// leaves no half-opened device set behind, and a configuration that selects
    /// nothing opens nothing.
    #[test]
    fn opening_devices_covers_both_directions_and_refuses_rather_than_half_opening() {
        let mut config = crate::config::RuntimeConfig::new(std::env::temp_dir());
        let mut settings = crate::settings::Settings::default();
        let underruns = || std::sync::Arc::new(super::AtomicU64::new(0));
        let (outputs, inputs) =
            super::open_devices(&config, &settings, underruns(), true, true).unwrap();
        assert!(outputs.is_empty() && inputs.is_empty());

        config.native_devices = false;
        settings.input_interfaces.push(crate::settings::InputInterface {
            id: 1,
            name: "Selected input".into(),
            enabled: true,
        });
        match super::open_devices(&config, &settings, underruns(), true, true) {
            Err(error) => assert!(error.contains("Native devices are disabled"), "{error}"),
            Ok(_) => panic!("a disabled device layer must refuse to open a selected interface"),
        }
        // A direction nobody asked for is not opened, so resuming hardware the
        // owner had stopped cannot restart it.
        let (outputs, inputs) =
            super::open_devices(&config, &settings, underruns(), false, false).unwrap();
        assert!(outputs.is_empty() && inputs.is_empty());
    }

    /// Capture authorization is enforced in the device layer, before any
    /// capture device is touched, and it is enforced by *refusing* rather than
    /// by whatever the platform would do to a process that opened a microphone
    /// it may not use. On iOS that is the difference between an error a
    /// performer can act on and `SIGABRT` inside AudioToolbox.
    #[test]
    fn capture_is_refused_before_a_device_is_touched_when_the_host_withholds_permission() {
        use crate::config::{AudioRoutes, CaptureAuthorization, CaptureSupport, RouteDescription};

        #[derive(Debug)]
        struct Host(CaptureAuthorization);
        impl AudioRoutes for Host {
            fn describe(&self) -> RouteDescription {
                RouteDescription {
                    output_channels: 2,
                    input_channels: 1,
                    input_available: true,
                    sample_rate: Some(48000),
                }
            }
            fn capture_support(&self) -> CaptureSupport {
                CaptureSupport::Supported
            }
            fn capture_authorization(&self) -> CaptureAuthorization {
                self.0
            }
            fn request_capture_authorization(&self) {}
        }

        let mut settings = crate::settings::Settings::default();
        settings
            .input_interfaces
            .push(crate::settings::InputInterface {
                id: 1,
                // Deliberately a name no machine running this test has, so a
                // refusal cannot be mistaken for "the device was not found":
                // the authorization check has to come first.
                name: "a route this test never resolves".into(),
                enabled: true,
            });

        for (authorization, expected) in [
            (CaptureAuthorization::Denied, "denied"),
            (CaptureAuthorization::Restricted, "restricted"),
            (CaptureAuthorization::Undetermined, "not been granted yet"),
        ] {
            let mut config = crate::config::RuntimeConfig::new(std::env::temp_dir());
            config.audio_routes = Some(std::sync::Arc::new(Host(authorization)));
            let error = match super::open_inputs(&config, &settings) {
                Err(error) => error.to_lowercase(),
                Ok(_) => panic!("capture must be refused for {authorization:?}"),
            };
            assert!(error.contains(expected), "{authorization:?}: {error}");
        }

        // With nothing selected, no authorization is consulted and no prompt is
        // possible: an application that can record must not ask until a
        // performer configures an input.
        let mut config = crate::config::RuntimeConfig::new(std::env::temp_dir());
        config.audio_routes = Some(std::sync::Arc::new(Host(CaptureAuthorization::Denied)));
        assert!(
            super::open_inputs(&config, &crate::settings::Settings::default())
                .is_ok_and(|inputs| inputs.is_empty())
        );

        // A host that withheld the whole device layer still reports that, not a
        // permission problem it has no opinion about.
        config.native_devices = false;
        match super::open_inputs(&config, &settings) {
            Err(error) => assert!(error.contains("Native devices are disabled"), "{error}"),
            Ok(_) => panic!("a disabled device layer must refuse a selected input"),
        }
    }

    #[test]
    fn no_selected_inputs_allows_engine_startup_without_opening_devices() {
        let config = crate::config::RuntimeConfig::new(std::env::temp_dir());
        let mut settings = crate::settings::Settings::default();
        assert!(super::open_inputs(&config, &settings).unwrap().is_empty());
        settings
            .input_interfaces
            .push(crate::settings::InputInterface {
                id: 1,
                name: "Unavailable unchecked input".into(),
                enabled: false,
            });
        assert!(super::open_inputs(&config, &settings).unwrap().is_empty());
    }
}

pub fn output_devices(config: &crate::config::RuntimeConfig) -> Vec<(u32, String)> {
    if !config.native_devices {
        return vec![];
    }
    device_details(config, crate::settings::read(config).sample_rate, false).iter()
        .filter_map(|d| Some((d["id"].as_u64()? as u32, d["name"].as_str()?.to_owned()))).collect()
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
/// Refusing to open a device on a runtime whose host withheld the capability.
///
/// `PR0_DISABLE_NATIVE_DEVICES=1` is still how the standalone process host
/// clears it, but `RuntimeConfig::native_devices` is now a host decision that an
/// embedded runtime sets directly, so the message names the capability rather
/// than one host's environment variable.
const NATIVE_DEVICES_DISABLED: &str =
    "Native devices are disabled for this runtime (PR0_DISABLE_NATIVE_DEVICES for the standalone server)";

/// The orchestration worker's standing native-hardware intent: what the owner
/// has asked to have open, as opposed to what is open right now.
///
/// The two are different whenever a platform lifecycle event has the hardware
/// closed — an iOS interruption, or the application in the background — and
/// they are also different after an open fails. Keeping the intent separate is
/// what lets a resume act on requests that arrived while the devices were shut
/// (enabling the engine, or toggling hardware output) and retry an open that
/// did not succeed, instead of restoring a snapshot of whatever happened to be
/// open at the instant the system interrupted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Wanted {
    outputs: bool,
    inputs: bool,
}

impl Wanted {
    /// Which wanted directions are not open. `enabled` is the engine gate: a
    /// platform lifecycle event never starts hardware for a runtime whose
    /// engine is off, however the intent got set.
    fn missing(self, enabled: bool, outputs: &[Output], inputs: &[Input]) -> Self {
        Self {
            outputs: enabled && self.outputs && outputs.is_empty(),
            inputs: enabled && self.inputs && inputs.is_empty(),
        }
    }

    fn any(self) -> bool {
        self.outputs || self.inputs
    }
}

/// Installs the host's platform audio policy for the settings a device is about
/// to be opened with.
///
/// On iOS this is the `AVAudioSession` category/mode/preference install, and it
/// has to happen here rather than only at launch: the recording category
/// follows an actually enabled input, and a performer may enable one while the
/// application is running. Every other host supplies no policy and this is a
/// pointer comparison. Never reached from a render or device callback — only
/// from the orchestration worker's command handlers, which already open
/// devices.
fn prepare_platform_audio(
    config: &crate::config::RuntimeConfig,
    settings: &crate::settings::Settings,
    logs: &crate::settings::Logs,
    context: &str,
) {
    let Some(policy) = config.audio_policy() else {
        return;
    };
    let preferences = crate::settings::AudioPreferences::from_settings(settings);
    if let Err(error) = policy.prepare(&preferences) {
        // Reported, not fatal: whatever the device layer says next about the
        // open that follows is the more actionable error.
        logs.push(context, "error", &format!("Platform audio session: {error}"));
    }
}

/// Opens the `missing` directions into `outputs`/`inputs`, independently.
///
/// Independent, unlike [`open_devices`], and deliberately so: a resume is a
/// platform event, not an owner action, and a microphone the system will not
/// hand back — permission revoked while the application was in the background,
/// a capture device removed — must not also keep the loudspeaker closed. Each
/// direction is still all-or-nothing within itself, because `open_outputs` and
/// `open_inputs` each return the whole set or drop what they had opened.
///
/// Whatever did open stays open; the error names every direction that did not,
/// and the caller leaves the intent set so the next resume retries.
fn open_wanted(
    config: &crate::config::RuntimeConfig,
    settings: &crate::settings::Settings,
    underruns: &Arc<AtomicU64>,
    missing: Wanted,
    outputs: &mut Vec<Output>,
    inputs: &mut Vec<Input>,
) -> Result<(), String> {
    let mut failures: Vec<String> = Vec::new();
    if missing.outputs {
        match open_outputs(config, settings, underruns.clone()) {
            Ok(devices) => *outputs = devices,
            Err(error) => failures.push(error),
        }
    }
    if missing.inputs {
        match open_inputs(config, settings) {
            Ok(captured) => *inputs = captured,
            Err(error) => failures.push(error),
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures.join("; "))
    }
}

/// Opens the selected native devices for `settings` in the requested
/// directions.
///
/// Outputs are opened first and the pair is returned together, so a failing
/// input never leaves the worker holding half a device set: the already opened
/// outputs are dropped — and therefore closed — with the error. Shared by engine
/// enablement, which wants both directions, and by resuming suspended hardware,
/// which wants back exactly what suspension closed.
fn open_devices(
    config: &crate::config::RuntimeConfig,
    settings: &crate::settings::Settings,
    underruns: Arc<AtomicU64>,
    wanted_outputs: bool,
    wanted_inputs: bool,
) -> Result<(Vec<Output>, Vec<Input>), String> {
    let outputs = if wanted_outputs {
        open_outputs(config, settings, underruns)?
    } else {
        vec![]
    };
    let inputs = if wanted_inputs {
        open_inputs(config, settings)?
    } else {
        vec![]
    };
    Ok((outputs, inputs))
}

fn open_outputs(
    config: &crate::config::RuntimeConfig,
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
        if !config.native_devices {
            return Err(NATIVE_DEVICES_DISABLED.into());
        }
        let device = selected_device(&selected.name,false)?;
        let stream_config = stream_config(config, &device, settings.sample_rate, false)
            .map_err(|e| format!("{}: {e}", selected.name))?;
        let channels = stream_config.channels as usize;
        let (buffer, mut consumer) = crate::output_buffer::OutputBuffer::new(
            settings.block_size,
            settings.sample_rate,
            underruns.clone(),
        );
        let errors = Arc::new(AtomicU64::new(0));
        let err = errors.clone();
        let stream = device
            .build_output_stream(
                &stream_config,
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
        "repeat" => {
            if let Some(seq) = sequencer {
                seq.repeat_autoplay();
            }
            if !engine.clock.running && count_in.is_none() {
                // Resume directly; count only when starting from rewind.
                *count_in = if engine.clock.beat == 0. {
                    pr0_dsp::count_in::CountIn::new(beats, beat_unit)
                } else {
                    None
                };
                engine.clock.running = count_in.is_none();
                if engine.clock.running {
                    align_graph_clock(engine);
                }
            }
        }
        "play" if !engine.clock.running && count_in.is_none() => {
            // Resume directly; count only when starting from rewind.
            *count_in = if engine.clock.beat == 0. {
                pr0_dsp::count_in::CountIn::new(beats, beat_unit)
            } else {
                None
            };
            engine.clock.running = count_in.is_none();
            if engine.clock.running {
                align_graph_clock(engine);
            }
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
            // Graph timing nodes rewind with the show but keep rendering.
            engine.graph_clock.stop();
            engine.graph_clock.running = true;
            *next_tempo = None;
        }
        _ => {}
    }
}

/// Graph timing nodes (clock, clock ratio, looper bars, sample restarts) read
/// the free-running graph clock. Starting the show snaps that clock to the show
/// beat so bar boundaries and phase agree with the score while it plays.
fn align_graph_clock(engine: &mut Engine) {
    engine.graph_clock.beat = engine.clock.beat;
    engine.graph_clock.reset_generation = engine.graph_clock.reset_generation.wrapping_add(1);
    engine.graph_clock.running = true;
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
        align_graph_clock(engine);
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
    fn transport_aligns_the_graph_clock_to_the_show() {
        let p = pr0_core::demo_project("align".into(), "Align".into(), pr0_core::Mode::Structured);
        let mut e = Engine::prepare(p.graph.clone(), 48000.).unwrap();
        let mut seq = Some(crate::performance::Sequencer::new(&p));
        let (io, _) = sync_channel(256);
        let mut count = None;
        let mut tempo = None;
        // The graph clock free-runs while the show is idle.
        e.render(&[], &mut [[0.; MAX_CHANNELS]; 4800]);
        assert!(e.graph_clock.beat > 0.);
        assert_eq!(e.clock.beat, 0.);
        let generation = e.graph_clock.reset_generation;
        // Play without a count-in snaps the graph clock to the show beat.
        apply_transport("play", 0, 4, &mut e, &mut seq, &mut count, &mut tempo, &io);
        assert!(e.clock.running && e.graph_clock.running);
        assert_eq!(e.graph_clock.beat, e.clock.beat);
        assert_ne!(e.graph_clock.reset_generation, generation);
        e.render(&[], &mut [[0.; MAX_CHANNELS]; 4800]);
        assert!((e.graph_clock.beat - e.clock.beat).abs() < 1e-9);
        // Pause keeps the graph rendering; resume realigns.
        apply_transport("pause", 0, 4, &mut e, &mut seq, &mut count, &mut tempo, &io);
        e.render(&[], &mut [[0.; MAX_CHANNELS]; 4800]);
        assert!(e.graph_clock.beat > e.clock.beat);
        apply_transport("play", 0, 4, &mut e, &mut seq, &mut count, &mut tempo, &io);
        assert_eq!(e.graph_clock.beat, e.clock.beat);
        // Stop rewinds both while the graph clock keeps running.
        apply_transport("stop", 0, 4, &mut e, &mut seq, &mut count, &mut tempo, &io);
        assert_eq!(e.graph_clock.beat, 0.);
        assert!(e.graph_clock.running && !e.clock.running);
        // Count-in completion aligns as well.
        apply_transport("play", 2, 4, &mut e, &mut seq, &mut count, &mut tempo, &io);
        assert!(count.is_some());
        e.render(&[], &mut [[0.; MAX_CHANNELS]; 100]);
        while advance_count_in(&mut count, &mut e).is_some() {
            e.render(&[], &mut [[0.; MAX_CHANNELS]]);
        }
        assert!(e.clock.running);
        assert_eq!(e.graph_clock.beat, e.clock.beat);
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

// Called on the orchestration worker; note-off timing counts rendered samples.
fn audition_note(
    e: &mut Engine,
    part: &str,
    staff: u8,
    node: Option<&str>,
    channel: u8,
    pitch: u8,
    velocity: u8,
) {
    if let Some(node) = node {
        e.note_scoped(node, u64::MAX, 0, pitch, velocity);
    }
    e.part_staff_message(
        part,
        staff,
        pr0_core::midi::Message {
            status: (if velocity == 0 { 0x80 } else { 0x90 }) | (channel - 1),
            data1: pitch,
            data2: velocity,
        },
    );
}

type AuditionState = (String, u8, Option<String>, u8, u8, usize);
fn advance_audition(e: &mut Engine, audition: &mut Option<AuditionState>) {
    if let Some((part, staff, node, channel, pitch, remaining)) = audition {
        if *remaining == 0 {
            audition_note(e, part, *staff, node.as_deref(), *channel, *pitch, 0);
            *audition = None;
        } else {
            *remaining -= 1;
        }
    }
}
#[cfg(test)]
mod audition_tests {
    use super::*;
    #[test]
    fn preview_releases_on_rendered_samples_with_transport_stopped() {
        let mut p = pr0_core::demo_project(
            "preview".into(),
            "preview".into(),
            pr0_core::Mode::Structured,
        );
        let mut n = p.graph.nodes[0].clone();
        n.id = "preview-midi".into();
        n.kind = "part_midi".into();
        n.parameters.clear();
        n.part_id = Some(p.parts[0].id.clone());
        p.graph.nodes.push(n);
        let mut e = Engine::prepare(p.graph, 48000.).unwrap();
        let part = &p.parts[0].id;
        audition_note(&mut e, part, 1, None, 1, 64, 90);
        let mut preview = Some((part.clone(), 1, None, 1, 64, 16));
        for _ in 0..16 {
            advance_audition(&mut e, &mut preview);
            e.render(&[], &mut [[0.; MAX_CHANNELS]]);
        }
        assert!(!e.clock.running);
        assert_eq!(e.telemetry()["preview-midi"]["gate"], 1.);
        advance_audition(&mut e, &mut preview);
        assert!(preview.is_none());
        for _ in 0..4 {
            e.render(&[], &mut [[0.; MAX_CHANNELS]]);
        }
        assert_eq!(e.telemetry()["preview-midi"]["gate"], 0.);
    }
}

/// The platform audio lifecycle state machine, driven through the real
/// orchestration worker.
///
/// **No native device is opened by any of this.** Every configuration here sets
/// `RuntimeConfig::native_devices = false`, which makes opening a *selected*
/// interface fail deterministically before CPAL is reached, and opening an
/// unselected one succeed trivially. That is the whole test lever: whether a
/// transition returns `Ok` or the disabled-devices error says exactly which
/// directions the worker decided to open, with no hardware, no permissions
/// prompt and no audio on the build machine. Nothing here is a claim about how
/// any platform's devices behave.
#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    use crate::settings::{InputInterface, Interface, Settings};

    struct Worker {
        commands: SyncSender<Command>,
        directory: std::path::PathBuf,
        stopped: bool,
    }

    impl Worker {
        fn start(label: &str) -> Self {
            let directory = std::env::temp_dir()
                .join(format!("pr0-lifecycle-{label}-{}", uuid::Uuid::new_v4()));
            std::fs::create_dir_all(&directory).unwrap();
            let mut config = crate::config::RuntimeConfig::embedded(directory.join("data"));
            config.recordings_dir = directory.join("recordings");
            // The lever: selected interfaces refuse to open instead of reaching
            // any audio or MIDI API.
            config.native_devices = false;
            std::fs::create_dir_all(&config.data_dir).unwrap();
            let (events, _) = broadcast::channel(64);
            let (media, _) = broadcast::channel(64);
            let config = Arc::new(config);
            let osc = crate::osc::Runtime::load(&config);
            Self {
                commands: super::start(
                    events,
                    media,
                    Arc::new(crate::settings::Logs::default()),
                    osc,
                    config,
                ),
                directory,
                stopped: false,
            }
        }

        fn enable(&self, value: bool, settings: &Settings) -> Result<(), String> {
            let (reply, answer) = oneshot::channel();
            self.commands
                .send(Command::Enable(
                    "project".into(),
                    value,
                    settings.clone(),
                    reply,
                ))
                .unwrap();
            answer.blocking_recv().unwrap()
        }

        /// Exactly the call `RuntimeHandle::suspend_audio`/`resume_audio` make.
        fn suspend(&self, suspended: bool) -> Result<(), String> {
            let (reply, answers) = std::sync::mpsc::sync_channel(1);
            self.commands
                .send(Command::Suspend { suspended, reply })
                .unwrap();
            answers
                .recv_timeout(Duration::from_secs(10))
                .expect("the worker answers a lifecycle transition")
        }

        fn hardware(&self, value: bool) {
            self.commands.send(Command::Hardware(value)).unwrap();
        }

        fn unload(&self) {
            self.commands.send(Command::Unload).unwrap();
        }

        /// Reads back the state `/api/devices` and telemetry publish. Ordered
        /// behind every command already sent, because one worker drains one
        /// queue.
        fn reported(&self) -> Value {
            let (reply, answer) = oneshot::channel();
            self.commands.send(Command::Devices(reply)).unwrap();
            answer.blocking_recv().unwrap()
        }

        fn stop(&mut self) {
            if std::mem::replace(&mut self.stopped, true) {
                return;
            }
            let (reply, answer) = oneshot::channel();
            let _ = self.commands.send(Command::Shutdown(reply));
            let _ = answer.blocking_recv();
        }
    }

    impl Drop for Worker {
        fn drop(&mut self) {
            self.stop();
            let _ = std::fs::remove_dir_all(&self.directory);
        }
    }

    /// Settings that select nothing, so every direction opens trivially.
    fn nothing_selected() -> Settings {
        Settings::default()
    }

    /// Settings that select one capture interface, so opening inputs fails and
    /// opening outputs still succeeds.
    fn capture_selected() -> Settings {
        let mut settings = Settings::default();
        settings.input_interfaces.push(InputInterface {
            id: 1,
            name: "Selected input".into(),
            enabled: true,
        });
        settings
    }

    /// Settings that select one playback interface, so opening outputs fails
    /// and opening inputs still succeeds.
    fn playback_selected() -> Settings {
        let mut settings = Settings::default();
        settings.interfaces.push(Interface {
            id: 2,
            name: "Selected output".into(),
            enabled: true,
            correct_latency: false,
            latency_ms: 0.,
        });
        settings
    }

    fn refused(result: Result<(), String>) -> String {
        let error = result.expect_err("a selected interface must refuse to open");
        assert!(error.contains("Native devices are disabled"), "{error}");
        error
    }

    /// The contract a host relies on: suspending is idempotent, closes the
    /// hardware, changes nothing above it, and is visible to the API.
    #[test]
    fn suspension_is_idempotent_and_visible_and_resume_restores_the_engine() {
        let mut worker = Worker::start("idempotent");
        worker.enable(true, &nothing_selected()).unwrap();
        assert_eq!(worker.reported()["engine_enabled"], json!(true));
        assert_eq!(worker.reported()["audio_suspended"], json!(false));

        worker.suspend(true).unwrap();
        worker.suspend(true).expect("a repeated suspend closes nothing twice");
        let reported = worker.reported();
        assert_eq!(reported["audio_suspended"], json!(true));
        assert_eq!(reported["hardware_enabled"], json!(false));
        assert_eq!(reported["input_enabled"], json!(false));
        // The engine is untouched by a platform event: only the hardware closed.
        assert_eq!(reported["engine_enabled"], json!(true));

        worker.suspend(false).unwrap();
        worker.suspend(false).expect("a repeated resume opens nothing twice");
        assert_eq!(worker.reported()["audio_suspended"], json!(false));
        worker.stop();
    }

    /// Enabling the engine while the hardware is closed for a platform
    /// lifecycle event must still be what resume acts on. Before the worker
    /// kept a standing intent it remembered only what was open at the moment of
    /// suspension, so this sequence returned to the foreground with the engine
    /// enabled and every device permanently closed — silently.
    #[test]
    fn enabling_while_suspended_is_what_resume_opens() {
        let mut worker = Worker::start("enable-while-suspended");
        worker.enable(true, &nothing_selected()).unwrap();
        worker.suspend(true).unwrap();
        // Nothing was open when the hardware closed.
        assert_eq!(worker.reported()["hardware_enabled"], json!(false));

        // A settings change that selects capture, applied while suspended. The
        // engine enables; no device is touched yet.
        worker
            .enable(true, &capture_selected())
            .expect("the engine enables while the hardware is closed");
        assert_eq!(worker.reported()["engine_enabled"], json!(true));

        // Resume must try to open the input this asked for.
        refused(worker.suspend(false));
        worker.stop();
    }

    /// The direction intent survives an open that fails, so a later resume
    /// retries rather than quietly giving up. The old snapshot behavior lost it
    /// after exactly one failed cycle: the second background/foreground pair
    /// reported success with nothing open.
    #[test]
    fn a_failed_open_keeps_the_intent_for_the_next_resume() {
        let mut worker = Worker::start("failed-open");
        worker.enable(true, &nothing_selected()).unwrap();
        worker.suspend(true).unwrap();
        worker.enable(true, &capture_selected()).unwrap();
        refused(worker.suspend(false));

        // Repeating the resume retries the open rather than reporting the
        // already-resumed state as success.
        refused(worker.suspend(false));

        // And a whole further background/foreground cycle still retries it.
        worker.suspend(true).unwrap();
        refused(worker.suspend(false));

        // The failure is reported, and the runtime is resumed rather than stuck
        // in a suspended state no host asked for.
        let reported = worker.reported();
        assert_eq!(reported["audio_suspended"], json!(false));
        assert!(
            reported["error"]
                .as_str()
                .unwrap()
                .contains("Native devices are disabled"),
            "{reported}"
        );
        worker.stop();
    }

    /// Hardware output requested while suspended is opened by resume; hardware
    /// output stopped while suspended stays closed. Both directions of the
    /// toggle used to be dropped entirely.
    #[test]
    fn the_hardware_toggle_is_honoured_across_a_suspension() {
        let mut worker = Worker::start("hardware-toggle");
        worker.enable(true, &nothing_selected()).unwrap();
        worker.suspend(true).unwrap();
        // Select a playback interface, then stop output while suspended.
        worker.enable(true, &playback_selected()).unwrap();
        worker.hardware(false);
        worker
            .suspend(false)
            .expect("output the owner stopped is not restarted by a lifecycle event");
        assert_eq!(worker.reported()["hardware_enabled"], json!(false));

        // Ask for it again while suspended: resume now opens it.
        worker.suspend(true).unwrap();
        worker.hardware(true);
        refused(worker.suspend(false));
        worker.stop();
    }

    /// A resume opens the directions independently, so a capture device the
    /// system will not hand back cannot also keep playback closed.
    #[test]
    fn one_refused_direction_does_not_close_the_other() {
        let mut worker = Worker::start("independent-directions");
        // Playback selects nothing (opens trivially); capture selects an
        // interface that cannot open.
        worker.enable(true, &capture_selected()).ok();
        worker.suspend(true).unwrap();
        worker.enable(true, &capture_selected()).unwrap();
        let error = refused(worker.suspend(false));
        // Exactly one direction failed, and it named itself once.
        assert_eq!(error.matches("Native devices are disabled").count(), 1);
        // Playback is open (empty selection, no error), capture is not.
        assert_eq!(worker.reported()["input_enabled"], json!(false));
        worker.stop();
    }

    /// A disabled engine is never started by a platform lifecycle event, and
    /// deactivating a show withdraws the capture intent with the capture it
    /// closes: returning to the foreground must not reopen a microphone for a
    /// runtime with no project loaded.
    #[test]
    fn a_lifecycle_event_never_starts_hardware_nobody_asked_for() {
        let mut worker = Worker::start("no-unrequested-start");
        // Never enabled: suspend and resume both do nothing at all.
        worker.suspend(true).unwrap();
        worker.suspend(false).unwrap();
        assert_eq!(worker.reported()["engine_enabled"], json!(false));

        worker.enable(true, &capture_selected()).ok();
        worker.unload();
        worker.suspend(true).unwrap();
        worker
            .suspend(false)
            .expect("a deactivated show does not reopen capture");
        assert_eq!(worker.reported()["input_enabled"], json!(false));
        worker.stop();
    }

    /// Shutdown wins any race with a lifecycle event: the worker stops, and the
    /// host's bounded wait ends with a disconnected channel rather than a hang.
    #[test]
    fn a_lifecycle_event_racing_shutdown_is_bounded_and_opens_nothing() {
        let mut worker = Worker::start("shutdown-race");
        worker.enable(true, &nothing_selected()).unwrap();
        worker.suspend(true).unwrap();
        worker.enable(true, &capture_selected()).unwrap();
        worker.stop();

        let (reply, answers) = std::sync::mpsc::sync_channel(1);
        let _ = worker.commands.send(Command::Suspend {
            suspended: false,
            reply,
        });
        match answers.recv_timeout(Duration::from_secs(10)) {
            // The worker had already returned: the queue is disconnected.
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => (),
            // Or it drained the command while stopping, in which case the
            // withdrawn intent means it opened nothing.
            Ok(outcome) => assert_eq!(outcome, Ok(())),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                panic!("a lifecycle transition must never outlive shutdown unanswered")
            }
        }
    }
}
