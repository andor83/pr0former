//! Isolated QuickJS workers, prepared off the audio-producing thread.
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use pr0_core::script::Script;
use pr0_dsp::script::{Action, Bridge, Command, Event, Kind, Worker};
use rquickjs::{CatchResultExt, Context, Function, Object, Persistent, Runtime};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

const API: &str = include_str!("script_api.js");
static LOG_SEQUENCE: AtomicU64 = AtomicU64::new(1);
// Bounds simultaneous compilation requests independently of project node limits.
static COMPILES: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);

struct Vm {
    api: Persistent<Object<'static>>,
    context: Context,
    runtime: Runtime,
    deadline: Arc<AtomicU64>,
    start: Instant,
}
impl Vm {
    fn new(source: &str) -> Result<(Self, Script), String> {
        if source.len() > pr0_core::script::MAX_SOURCE {
            return Err("Script source exceeds 64 KiB".into());
        }
        let runtime = Runtime::new().map_err(|e| e.to_string())?;
        runtime.set_memory_limit(16 * 1024 * 1024);
        runtime.set_max_stack_size(256 * 1024);
        let deadline = Arc::new(AtomicU64::new(250_000));
        let end = deadline.clone();
        let start = Instant::now();
        let started = start;
        runtime.set_interrupt_handler(Some(Box::new(move || {
            started.elapsed().as_micros() as u64 > end.load(Ordering::Relaxed)
        })));
        let context = Context::full(&runtime).map_err(|e| e.to_string())?;
        let (api, manifest) = context.with(|ctx| -> Result<_, String> {
            let object: Object = ctx.eval(API).catch(&ctx).map_err(|e| e.to_string())?;
            let mut options = rquickjs::context::EvalOptions::default();
            options.strict = true;
            options.filename = Some("script.js".into());
            ctx.eval_with_options::<(), _>(source, options)
                .catch(&ctx)
                .map_err(|e| e.to_string())?;
            let seal: Function = object.get("seal").map_err(|e| e.to_string())?;
            let manifest: String = seal.call(()).catch(&ctx).map_err(|e| e.to_string())?;
            Ok((Persistent::save(&ctx, object), manifest))
        })?;
        if runtime.is_job_pending() {
            return Err("Promises and asynchronous setup are unsupported".into());
        }
        let mut manifest: Script = serde_json::from_value({
            let mut v: Value = serde_json::from_str(&manifest).map_err(|e| e.to_string())?;
            v["source"] = source.into();
            v
        })
        .map_err(|e| e.to_string())?;
        manifest.source = source.into();
        manifest.validate()?;
        Ok((
            Self {
                api,
                context,
                runtime,
                deadline,
                start,
            },
            manifest,
        ))
    }
    fn dispatch(&self, event: &Value) -> Result<Batch, String> {
        self.deadline.store(
            self.start.elapsed().as_micros() as u64 + 5_000,
            Ordering::Relaxed,
        );
        let serialized = serde_json::to_string(event).map_err(|e| e.to_string())?;
        let result = self.context.with(|ctx| -> Result<String, String> {
            let object = self.api.clone().restore(&ctx).map_err(|e| e.to_string())?;
            let function: Function = object.get("dispatch").map_err(|e| e.to_string())?;
            function
                .call((serialized,))
                .catch(&ctx)
                .map_err(|e| e.to_string())
        })?;
        // Promises/async jobs have no scheduler or host IO and are not driven here.
        if self.runtime.is_job_pending() {
            return Err(
                "Promises and async functions are unsupported; use engine-time helpers".into(),
            );
        }
        if result.len() > 512 * 1024 {
            return Err("Script response exceeds 512 KiB".into());
        }
        serde_json::from_str(&result).map_err(|e| e.to_string())
    }
}

#[derive(Deserialize)]
struct Batch {
    commands: Vec<Outgoing>,
    logs: Vec<Log>,
}
#[derive(Deserialize)]
struct Log {
    level: String,
    sample: u64,
    message: String,
}
#[derive(Deserialize)]
struct Outgoing {
    #[serde(flatten)]
    action: OutAction,
    sample: u64,
    beat: Option<f64>,
    generation: u64,
}
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum OutAction {
    Output {
        port: usize,
        value: f64,
    },
    Midi {
        status: u8,
        data1: u8,
        data2: u8,
    },
    Named {
        binding: usize,
        value: pr0_core::ControlValue,
    },
}
fn action(action: OutAction, config: &Script) -> Result<Action, String> {
    match action {
        OutAction::Output { port, value } if port < config.outputs.len() && value.is_finite() => {
            Ok(Action::Output { port, value })
        }
        OutAction::Midi {
            status,
            data1,
            data2,
        } => {
            let message = pr0_core::midi::Message {
                status,
                data1,
                data2,
            };
            if message.valid() {
                Ok(Action::Midi(message))
            } else {
                Err("Invalid MIDI message".into())
            }
        }
        OutAction::Named { binding, value } if binding < config.bindings.len() => {
            match &value {
                pr0_core::ControlValue::Text(_) => {
                    return Err("Script named publications must be numeric".into());
                }
                pr0_core::ControlValue::Number(n) if !n.is_finite() => {
                    return Err("Named number must be finite".into());
                }
                _ => {}
            }
            Ok(Action::Named {
                binding,
                value: pr0_dsp::script::prepare_value(&value),
            })
        }
        _ => Err("Invalid script output".into()),
    }
}
fn event(event: Event) -> Value {
    let c = event.clock;
    let mut value = json!({"sample":c.sample,"beat":c.beat,"bpm":c.bpm,"sample_rate":c.sample_rate,"generation":c.reset_generation});
    let detail = match event.kind {
        Kind::Clock {
            running,
            click,
            tick,
            position,
            unit,
        } => {
            json!({"type":"clock","running":running,"click":click,"tick":tick,"position":position,"unit":unit})
        }
        Kind::Input { port, value } => json!({"type":"input","port":port,"value":value}),
        Kind::Midi(m) => {
            let (bytes, len) = m.bytes();
            json!({"type":"midi","bytes":&bytes[..len]})
        }
        Kind::Named { binding, value } => {
            json!({"type":"named","binding":binding,"value":pr0_dsp::script::serialize_value(value)})
        }
    };
    value
        .as_object_mut()
        .unwrap()
        .extend(detail.as_object().unwrap().clone());
    value
}
fn record(worker: &Worker, log: Log) {
    let mut d = worker.shared.diagnostics.lock().unwrap();
    if d.logs.len() >= 64 {
        d.logs.remove(0);
    }
    d.logs.push((
        LOG_SEQUENCE.fetch_add(1, Ordering::Relaxed),
        log.sample,
        log.level,
        log.message,
    ));
}
fn deliver(vm: &Vm, worker: &mut Worker, config: &Script, event: &Value) -> Result<(), String> {
    let batch = vm.dispatch(event)?;
    if batch.commands.len() > 256 || batch.logs.len() > 64 {
        return Err("Script response budget exceeded".into());
    }
    for out in batch.commands {
        if out.beat.is_some_and(|b| !b.is_finite()) {
            return Err("Scheduled beat must be finite".into());
        }
        let cmd = Command {
            action: action(out.action, config)?,
            sample: out.sample,
            beat: out.beat,
            generation: out.generation,
        };
        worker
            .commands
            .push(cmd)
            .map_err(|_| "Script command queue overflow".to_owned())?;
    }
    for mut log in batch.logs {
        log.message.truncate(log.message.floor_char_boundary(2048));
        record(worker, log);
    }
    worker.shared.diagnostics.lock().unwrap().events += 1;
    Ok(())
}
fn run(vm: Vm, mut worker: Worker, config: Script) {
    let mut last = json!({"sample":0,"beat":0,"bpm":120,"sample_rate":48000,"generation":0});
    while worker.shared.alive.load(Ordering::Acquire) {
        if worker.shared.fault.load(Ordering::Acquire) {
            if worker.shared.diagnostics.lock().unwrap().error.is_none() {
                fail(
                    &worker,
                    "Script event queue overflow; outputs reset. Apply the script to restart.",
                    last["sample"].as_u64().unwrap_or(0),
                );
            }
            std::thread::sleep(Duration::from_millis(2));
            continue;
        }
        let mut worked = false;
        for _ in 0..64 {
            let Ok(e) = worker.events.pop() else { break };
            worked = true;
            last = event(e);
            if let Err(error) = deliver(&vm, &mut worker, &config, &last) {
                fail(&worker, &error, e.clock.sample);
                break;
            }
        }
        for _ in 0..8 {
            if worker.shared.fault.load(Ordering::Acquire) {
                break;
            }
            let Ok(text) = worker.osc.try_recv() else {
                break;
            };
            worked = true;
            let Ok(mut osc) = serde_json::from_str::<Value>(&text) else {
                continue;
            };
            for key in ["sample", "beat", "bpm", "sample_rate", "generation"] {
                if osc.get(key).is_none() {
                    osc[key] = last[key].clone();
                }
            }
            if let Err(error) = deliver(&vm, &mut worker, &config, &osc) {
                fail(&worker, &error, osc["sample"].as_u64().unwrap_or(0));
                break;
            }
        }
        if !worked {
            std::thread::sleep(Duration::from_micros(500));
        }
    }
}
fn fail(worker: &Worker, error: &str, sample: u64) {
    worker.shared.fault.store(true, Ordering::Release);
    let error = error.chars().take(2048).collect::<String>();
    worker.shared.diagnostics.lock().unwrap().error = Some(error.clone());
    record(
        worker,
        Log {
            level: "error".into(),
            sample,
            message: error,
        },
    );
}

/// Each worker initializes in its own thread. The preparation caller waits; rendering never does.
pub fn prepare(
    engine: &mut pr0_dsp::Engine,
    graph: &pr0_core::Graph,
    block_size: usize,
) -> Result<(), String> {
    for node in graph.nodes.iter().filter(|n| n.kind == "js_control") {
        let config = node.script.clone().unwrap_or_default();
        config.validate()?;
        let (mut bridge, worker) = Bridge::new(config.clone());
        bridge.block_size = block_size as u64;
        let (ready, rx) = std::sync::mpsc::sync_channel(1);
        let expected = config.clone();
        std::thread::Builder::new()
            .name("pr0-js-control".into())
            .spawn(move || match Vm::new(&config.source) {
                Ok((vm, mut actual)) => {
                    actual.revision = expected.revision;
                    if actual == expected {
                        if ready.send(Ok(())).is_ok() {
                            run(vm, worker, config);
                        }
                    } else {
                        let _ = ready.send(Err(
                            "Script manifest does not match server compilation".into(),
                        ));
                    }
                }
                Err(error) => {
                    let _ = ready.send(Err(error));
                }
            })
            .map_err(|e| e.to_string())?;
        rx.recv()
            .map_err(|e| e.to_string())?
            .map_err(|e: String| format!("{}: {e}", node.label))?;
        engine.attach_script(&node.id, bridge);
    }
    Ok(())
}
pub fn compile_source(source: &str) -> Result<Script, String> {
    Vm::new(source).map(|(_, manifest)| manifest)
}
/// Re-execute declarations for changed scripts; never trust a browser-supplied manifest.
pub async fn validate_graph(
    graph: &pr0_core::Graph,
    previous: Option<&pr0_core::Graph>,
) -> Result<(), String> {
    let scripts: Vec<_> = graph
        .nodes
        .iter()
        .filter(|n| n.kind == "js_control")
        .filter(|n| {
            !previous.is_some_and(|p| {
                p.nodes
                    .iter()
                    .any(|old| old.id == n.id && old.kind == n.kind && old.script == n.script)
            })
        })
        .map(|n| (n.label.clone(), n.script.clone().unwrap_or_default()))
        .collect();
    if scripts.is_empty() {
        return Ok(());
    }
    let _permit = COMPILES.acquire().await.map_err(|e| e.to_string())?;
    tokio::task::spawn_blocking(move||{for(label,script)in scripts{
        let mut actual=compile_source(&script.source)?;actual.revision=script.revision;
        if actual!=script{return Err(format!("{label}: script ports differ from the compiled source. Compile and Apply the script again."));}
    }Ok(())}).await.map_err(|e|e.to_string())?
}
#[derive(Deserialize)]
pub struct CompileRequest {
    source: String,
}
pub async fn compile(
    State(app): State<crate::App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<CompileRequest>,
) -> crate::Api<Json<Script>> {
    crate::csrf(&headers)?;
    let u = crate::user(&app, &headers)?;
    crate::can_edit(&crate::role(&app, &id, &u)?)?;
    let _permit = COMPILES.acquire().await.map_err(crate::internal)?;
    let script = tokio::task::spawn_blocking(move || compile_source(&request.source))
        .await
        .map_err(crate::internal)?
        .map_err(crate::bad)?;
    Ok(Json(script))
}
pub fn telemetry(
    engine: &pr0_dsp::Engine,
    logs: &crate::settings::Logs,
    project: &pr0_core::Project,
    seen: &mut std::collections::BTreeMap<String, u64>,
) -> Value {
    seen.retain(|id, _| {
        project
            .graph
            .nodes
            .iter()
            .any(|n| n.id == *id && n.kind == "js_control")
    });
    let mut result = serde_json::Map::new();
    for (id, d, dropped, late, faulted) in engine.script_status() {
        let previous = *seen.get(&id).unwrap_or(&0);
        let label = project
            .graph
            .nodes
            .iter()
            .find(|n| n.id == id)
            .map(|n| n.label.as_str())
            .unwrap_or(&id);
        for (seq, sample, level, message) in &d.logs {
            if *seq > previous {
                logs.push(
                    &project.id,
                    level,
                    &format!("JavaScript [{label} · {id}] sample {sample}: {message}"),
                );
                seen.insert(id.clone(), *seq);
            }
        }
        result.insert(id,json!({"error":d.error,"logs":d.logs.iter().map(|(_,sample,level,message)|json!({"sample":sample,"level":level,"message":message})).collect::<Vec<_>>(),"events":d.events,"dropped":dropped,"late":late,"faulted":faulted}));
    }
    Value::Object(result)
}
pub fn osc_event(message: &rosc::OscMessage, engine: &pr0_dsp::Engine) -> bool {
    let payload = json!({"type":"osc","address":message.addr,"args":message.args.iter().map(osc_arg).collect::<Vec<_>>(),"sample":engine.graph_clock.sample,"beat":engine.graph_clock.beat,"bpm":engine.clock.bpm,"sample_rate":engine.clock.sample_rate,"generation":engine.graph_clock.reset_generation});
    if let Ok(text) = serde_json::to_string(&payload) {
        if text.len() <= 16384 {
            return engine.script_osc(&text);
        }
    }
    false
}
fn osc_arg(arg: &rosc::OscType) -> Value {
    use rosc::OscType::*;
    match arg {
        Int(v) => json!({"type":"int","value":v}),
        Float(v) => json!({"type":"float","value":v}),
        Double(v) => json!({"type":"double","value":v}),
        Long(v) => json!({"type":"int64","value":v.to_string()}),
        String(v) => json!({"type":"string","value":v}),
        Bool(v) => json!({"type":"bool","value":v}),
        Blob(v) => json!({"type":"blob","value":v}),
        Char(v) => json!({"type":"char","value":v.to_string()}),
        Time(v) => json!({"type":"time","seconds":v.seconds,"fractional":v.fractional}),
        Color(v) => json!({"type":"color","value":[v.red,v.green,v.blue,v.alpha]}),
        Midi(v) => json!({"type":"midi","value":[v.port,v.status,v.data1,v.data2]}),
        Array(v) => {
            json!({"type":"array","value":v.content.iter().map(osc_arg).collect::<Vec<_>>()})
        }
        Nil => json!({"type":"nil","value":null}),
        Inf => json!({"type":"inf","value":null}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn clock(sample: u64, beat: f64) -> Value {
        json!({"type":"clock","sample":sample,"beat":beat,"bpm":120,"sample_rate":48000,"generation":0,"running":false,"tick":true,"click":false,"position":beat,"unit":4})
    }
    #[test]
    fn named_ports_read_write_and_handlers_are_real_js() {
        let (vm,manifest)=Vm::new(r#"
            const input=define_input("velocity", 100);
            const output=define_output("gain");
            input.on("change", e=>{output.write(clamp(input.read()/127));console.log("velocity",e.value);});
            define_input("trigger", e=>{if(e.value>0)output.pulse();});
        "#).unwrap();
        assert_eq!(manifest.inputs.len(), 2);
        assert_eq!(manifest.outputs[0].name, "gain");
        let mut e = clock(10, 0.);
        e["type"] = "input".into();
        e["port"] = 0.into();
        e["value"] = 127.into();
        let batch = vm.dispatch(&e).unwrap();
        assert!(matches!(
            batch.commands[0].action,
            OutAction::Output { port: 0, value: 1. }
        ));
        assert_eq!(batch.logs[0].message, "velocity 127");
        assert_eq!(batch.logs[0].sample, 10);
    }
    #[test]
    fn midi_raw_bytes_decoding_filters_and_scheduled_releases() {
        let (vm, _) = Vm::new(
            r#"
            midi.on("cc", e=>{if(e.controller===1)console.log(e.channel,e.normalized,e.bytes);});
            midi.on("note_on", e=>midi.note(e.note+12,e.velocity,0.5,e.channel,e.beat+1));
            midi.on("note_off",e=>console.log(e.type,e.status));
        "#,
        )
        .unwrap();
        let mut e = clock(48000, 2.);
        e["type"] = "midi".into();
        e["bytes"] = json!([0xb2, 1, 127]);
        assert_eq!(vm.dispatch(&e).unwrap().logs[0].message, "3 1 [178,1,127]");
        e["bytes"] = json!([0x92, 60, 100]);
        let batch = vm.dispatch(&e).unwrap();
        assert_eq!(batch.commands.len(), 2);
        assert_eq!(batch.commands[0].beat, Some(3.));
        assert_eq!(batch.commands[1].beat, Some(3.5));
        assert!(matches!(
            batch.commands[1].action,
            OutAction::Midi {
                status: 0x82,
                data1: 72,
                data2: 0
            }
        ));
        e["bytes"] = json!([0x92, 60, 0]);
        assert_eq!(vm.dispatch(&e).unwrap().logs[0].message, "note_off 146");
    }
    #[test]
    fn ticks_metronome_timers_and_reset_follow_engine_time() {
        let (vm, _) = Vm::new(
            r#"
            engine.on("ready",()=>every_beats(1,e=>console.log("timer",e.beat)));
            on_tick(e=>console.log("tick",e.frames));
            metronome.on("click",e=>console.log("click",e.beat_unit));
            engine.on("reset",()=>console.log("reset"));
        "#,
        )
        .unwrap();
        assert_eq!(
            vm.dispatch(&clock(0, 0.)).unwrap().logs[0].message,
            "tick 0"
        );
        let mut e = clock(128, 1.);
        e["click"] = true.into();
        e["unit"] = 8.into();
        let batch = vm.dispatch(&e).unwrap();
        assert_eq!(
            batch
                .logs
                .iter()
                .map(|l| l.message.as_str())
                .collect::<Vec<_>>(),
            vec!["tick 128", "timer 1", "click 8"]
        );
        e["generation"] = 1.into();
        e["beat"] = 0.into();
        e["click"] = false.into();
        assert_eq!(vm.dispatch(&e).unwrap().logs[0].message, "reset");
        e["beat"] = 10.into();
        assert!(
            !vm.dispatch(&e)
                .unwrap()
                .logs
                .iter()
                .any(|l| l.message.starts_with("timer"))
        );
    }
    #[test]
    fn setup_timers_and_console_rebase_to_live_engine_time_and_rate() {
        let (vm, _) = Vm::new(
            r#"
            console.log("setup");
            after_ms(10,e=>console.log("timer",e.sample));
        "#,
        )
        .unwrap();
        let mut e = clock(96000, 2.);
        e["sample_rate"] = 96000.into();
        let batch = vm.dispatch(&e).unwrap();
        assert_eq!(batch.logs.len(), 1);
        assert_eq!(batch.logs[0].sample, 96000);
        e["sample"] = 96959.into();
        assert!(vm.dispatch(&e).unwrap().logs.is_empty());
        e["sample"] = 96960.into();
        assert_eq!(vm.dispatch(&e).unwrap().logs[0].message, "timer 96960");
    }
    #[test]
    fn async_once_and_timer_handlers_are_rejected() {
        let (vm, _) = Vm::new("engine.once('tick',async()=>{});").unwrap();
        assert!(vm.dispatch(&clock(0, 0.)).is_err());
        let (vm, _) = Vm::new("after_ms(1,async()=>{});").unwrap();
        vm.dispatch(&clock(0, 0.)).unwrap();
        assert!(vm.dispatch(&clock(48, 0.)).is_err());
    }
    #[test]
    fn osc_preserves_all_arguments_and_named_bindings() {
        let (vm, config) = Vm::new(
            r#"
            const bus=control.send("level");
            osc.bind("/fader").on("message",e=>{bus.write(e.args[0].value);console.log(e.args);});
            bind_send("gesture").on("change",e=>console.log(e.name,e.value));
            bind_receive("level").on("change",e=>console.log("received",e.value));
        "#,
        )
        .unwrap();
        assert_eq!(config.bindings.len(), 3);
        let mut e = clock(128, 0.);
        e["type"] = "osc".into();
        e["address"] = "/fader".into();
        e["args"] = json!([{"type":"float","value":0.75},{"type":"string","value":"hi"}]);
        let b = vm.dispatch(&e).unwrap();
        assert!(matches!(
            &b.commands[0].action,
            OutAction::Named {
                binding: 0,
                value: pr0_core::ControlValue::Number(0.75)
            }
        ));
        e["address"] = "/unmatched".into();
        assert!(vm.dispatch(&e).unwrap().commands.is_empty());
        e["type"] = "named".into();
        e["binding"] = 1.into();
        e["value"] = "text".into();
        assert_eq!(vm.dispatch(&e).unwrap().logs[0].message, "gesture text");
        assert_eq!(
            osc_arg(&rosc::OscType::Long(i64::MAX))["value"],
            i64::MAX.to_string()
        );
    }
    #[test]
    fn math_helpers_and_seeded_random_are_usable() {
        let(vm,_)=Vm::new(r#"engine.on("ready",()=>{
            seed_random(99);const a=random();seed_random(99);if(a!==random())throw Error("random");
            if(wrap(-1,0,8)!==7||midi_to_hz(69)!==440||hz_to_midi(440)!==69||quantize(1.8,0.5)!==2||map_range(64,0,128,0,1)!==0.5)throw Error("math");
            if(!euclidean(0,3,8)||chance(0)||!chance(1)||choose([5])!==5||db_to_gain(0)!==1||gain_to_db(1)!==0)throw Error("music");
            console.info("helpers ok");
        });"#).unwrap();
        assert_eq!(
            vm.dispatch(&clock(0, 0.)).unwrap().logs[0].message,
            "helpers ok"
        );
    }
    #[test]
    fn sandbox_limits_syntax_loops_heap_and_manifest() {
        assert!(compile_source("const = ;").is_err());
        assert!(compile_source("while(true) {}").is_err());
        assert!(
            compile_source("const a=[];while(true)a.push(new Array(100000).fill(1));").is_err()
        );
        assert!(compile_source("define_input('a');define_input('a');").is_err());
        assert!(compile_source("define_input('midi');").is_err());
        assert!(compile_source("for(let i=0;i<9;i++)define_output('o'+i);").is_err());
        assert!(compile_source("fetch('https://example.com');").is_err());
        let (vm, _) = Vm::new("on_tick(()=>{while(true){}});").unwrap();
        assert!(vm.dispatch(&clock(0, 0.)).is_err());
        let (vm, _) = Vm::new("on_tick(()=>define_output('late'));").unwrap();
        assert!(vm.dispatch(&clock(0, 0.)).is_err());
        let (vm, _) = Vm::new("on_tick(async()=>{});").unwrap();
        assert!(vm.dispatch(&clock(0, 0.)).is_err());
    }
    #[tokio::test]
    async fn server_rejects_forged_ports_even_with_engine_disabled() {
        let mut graph:pr0_core::Graph=serde_json::from_value(json!({"nodes":[{"id":"s","kind":"js_control","label":"Script","channels":1,"x":0,"y":0,"parameters":{},"script":{"source":"define_output('real');","inputs":[],"outputs":[{"name":"forged","initial":0}],"bindings":[]}}],"edges":[]})).unwrap();
        graph.validate().unwrap();
        assert!(validate_graph(&graph, None).await.is_err());
        graph.nodes[0].script = Some(compile_source("define_output('real');").unwrap());
        assert!(validate_graph(&graph, None).await.is_ok());
    }
    #[test]
    fn dedicated_worker_drives_real_graph_and_logs() {
        let config=compile_source("const out=define_output('value');on_tick(e=>{out.write(42);console.log('worker',e.frames);});").unwrap();
        let graph:pr0_core::Graph=serde_json::from_value(json!({"nodes":[{"id":"s","kind":"js_control","label":"Script","channels":1,"x":0,"y":0,"parameters":{},"script":config},{"id":"v","kind":"multiply","label":"Value","channels":1,"x":300,"y":0,"parameters":{"b":2}}],"edges":[{"id":"e","source":"s","source_port":"value","target":"v","target_port":"a"}]})).unwrap();
        let mut e = pr0_dsp::Engine::prepare(graph.clone(), 48000.).unwrap();
        prepare(&mut e, &graph, 128).unwrap();
        let start = Instant::now();
        while start.elapsed() < Duration::from_secs(1) {
            e.render(&[], &mut [[0.; 8]; 128]);
            if e.telemetry()["v"]["_out"] == 84. {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert_eq!(e.telemetry()["v"]["_out"], 84.);
        assert!(!e.script_status()[0].1.logs.is_empty());
    }
}
