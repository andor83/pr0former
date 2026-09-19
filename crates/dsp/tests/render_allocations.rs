//! Exercise prepared DSP paths under a thread-local allocator guard.
use pr0_core::{ControlValue, Edge, Graph, Node};
use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::Cell,
};
thread_local! {
    static CHECK: Cell<bool> = const { Cell::new(false) };
    static CALLS: Cell<usize> = const { Cell::new(0) };
}
struct CheckedAllocator;
fn allocation() {
    let _ = CHECK.try_with(|check| {
        if check.get() {
            let _ = CALLS.try_with(|calls| calls.set(calls.get() + 1));
        }
    });
}
unsafe impl GlobalAlloc for CheckedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        allocation();
        unsafe { System.alloc(layout) }
    }
    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        allocation();
        unsafe { System.alloc_zeroed(layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, size: usize) -> *mut u8 {
        allocation();
        unsafe { System.realloc(ptr, layout, size) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        allocation();
        unsafe { System.dealloc(ptr, layout) }
    }
}
#[global_allocator]
static ALLOCATOR: CheckedAllocator = CheckedAllocator;
#[test]
fn script_bridge_commands_events_overflow_and_replacement_do_not_allocate() {
    use pr0_dsp::script::{Action, Bridge, Command};
    let mut n=node("js","js_control");
    let config=pr0_core::script::Script { outputs:vec![pr0_core::script::Input{name:"value".into(),initial:0.}], bindings:vec![pr0_core::script::Binding{kind:"publish".into(),name:"bus".into()}],..Default::default() };
    n.script=Some(config.clone());
    let mut r=node("receiver","receive_control");r.control_value=Some(ControlValue::Text("bus".into()));
    let graph=Graph{nodes:vec![n,r],edges:vec![]};
    let mut e=pr0_dsp::Engine::prepare(graph.clone(),48000.).unwrap();let(b,mut w)=Bridge::new(config.clone());e.attach_script("js",b);
    let mut replacement=pr0_dsp::Engine::prepare(graph,48000.).unwrap();let(b,_new_worker)=Bridge::new(config);replacement.attach_script("js",b);
    let named=pr0_dsp::script::prepare_value(&ControlValue::Number(0.5));
    CHECK.set(true);CALLS.set(0);
    for sample in 0..4096 {
        let action=if sample%2==0{Action::Output{port:0,value:sample as f64}}else{Action::Named{binding:0,value:named}};
        assert!(w.commands.push(Command{action,sample,beat:None,generation:0}).is_ok());
        e.render(&[],&mut[[0.;8]]);
        while w.events.pop().is_ok(){}
    }
    replacement.carry_node_state(&mut e);
    replacement.render(&[],&mut[[0.;8];128]);
    w.shared.fault.store(true,std::sync::atomic::Ordering::Release);
    replacement.render(&[],&mut[[0.;8];128]);
    CHECK.set(false);
    assert_eq!(CALLS.get(),0,"script bridge must never allocate or free while rendering or carrying state");
}
fn node(id: &str, kind: &str) -> Node {
    Node {
        states: None,
        control_positions: vec![],
        script: None,
        sample_choices: vec![],
        id: id.into(),
        kind: kind.into(),
        label: id.into(),
        x: 0.,
        y: 0.,
        channels: 8,
        parameters: Default::default(),
        part_id: None,
        io: None,
        library: None,
        parent: None,
        control_value: None,
    }
}
#[test]
fn connected_control_overrides_and_change_events_do_not_allocate() {
    let source=node("source","value");
    let mut control=node("control","control_input");
    control.parameters.insert("changes_only".into(),1.);
    let graph=Graph { nodes:vec![source,control],edges:vec![Edge { id:"wire".into(),source:"source".into(),source_port:"out".into(),target:"control".into(),target_port:"in".into() }] };
    let mut engine=pr0_dsp::Engine::prepare(graph,48000.).unwrap();
    engine.render(&[],&mut [[0.;8]]);
    CHECK.set(true);CALLS.set(0);
    for value in 0..1000 {
        engine.control("control",&ControlValue::Number(value as f64));
        engine.render(&[],&mut [[0.;8];8]);
        engine.parameter("source","value",value as f64).unwrap();
        engine.render(&[],&mut [[0.;8];8]);
    }
    CHECK.set(false);assert_eq!(CALLS.get(),0);
}
#[test]
fn prepared_polyphonic_spectral_recording_and_routing_render_without_heap_activity() {
    let mut graph = Graph {
        nodes: [
            ("keys", "piano"),
            ("local", "local_midi_input"),
            ("mic", "browser_input"),
            ("knobs", "knobs"),
            ("sliders", "sliders"),
            ("fm", "fm_synth"),
            ("sample", "poly_sampler"),
            ("grains", "granular_synth"),
            ("cloud", "granular_cloud"),
            ("field", "granular_field"),
            ("tone", "oscillator"),
            ("shift", "granular_pitch_shift"),
            ("convolve", "convolution"),
            ("fft", "fft"),
            ("send", "send_spectral"),
            ("receive", "receive_spectral"),
            ("ifft", "ifft"),
            ("meter", "audio_visualizer"),
            ("tracker", "pitch_tracker"),
            ("loop", "looper"),
            ("record", "record"),
            ("one", "value"),
            ("debug", "console_out"),
        ]
        .into_iter()
        .map(|(id, kind)| node(id, kind))
        .collect(),
        edges: vec![],
    };
    for n in &mut graph.nodes {
        if n.id == "loop" {
            n.parameters.insert("max_seconds".into(), 1.);
        }
        if n.id == "sample" {
            for (key, value) in [
                ("attack", 4.),
                ("decay", 8.),
                ("sustain", 0.4),
                ("release", 12.),
            ] {
                n.parameters.insert(key.into(), value);
            }
        }
        if n.id == "one" {
            n.parameters.insert("value".into(), 1.);
        }
        if n.id == "field" {
            n.sample_choices = (1..=2)
                .map(|asset| pr0_core::SampleChoice { asset, name: format!("Field {asset}"), nickname: String::new() })
                .collect();
            n.parameters.insert("randomize_pitch".into(), 1.);
            n.parameters.insert("pitch".into(), 7.);
        }
        if n.id == "send" || n.id == "receive" {
            n.control_value = Some(ControlValue::Text("spectrum".into()));
        }
    }
    let mut wire = |source: &str, source_port: &str, target: &str, target_port: &str| {
        graph.edges.push(Edge {
            id: format!("{source}-{source_port}-{target}-{target_port}"),
            source: source.into(),
            source_port: source_port.into(),
            target: target.into(),
            target_port: target_port.into(),
        })
    };
    for target in ["fm", "sample", "grains"] {
        wire("keys", "midi", target, "midi");
        for port in ["pitch", "velocity", "gate", "trigger", "note_off"] {
            wire("keys", port, target, port);
        }
    }
    for (s, sp, t, tp) in [
        ("one", "out", "debug", "in"),
        ("knobs", "midi", "sliders", "midi"),
        ("local", "midi", "sliders", "midi"),
        ("one", "out", "sliders", "slider_2"),
        ("grains", "out", "shift", "in"),
        ("fm", "out", "convolve", "a"),
        ("tone", "out", "convolve", "b"),
        ("convolve", "out", "fft", "in"),
        ("fft", "out", "send", "in"),
        ("receive", "out", "ifft", "in"),
        ("ifft", "out", "meter", "in"),
        ("tone", "out", "tracker", "in"),
        ("sample", "out", "loop", "in"),
        ("loop", "out", "record", "in"),
        ("one", "out", "loop", "start_loop"),
        ("one", "out", "record", "start"),
        ("mic", "out", "field", "live_1"),
        ("one", "out", "field", "x"),
    ] {
        wire(s, sp, t, tp);
    }
    let mut engine = pr0_dsp::Engine::prepare(graph, 48000.).unwrap();
    for id in ["sample", "grains", "cloud"] {
        engine.set_sample(id, vec![[0.1; 8]; 48000]);
    }
    for asset in 1..=2 {
        engine.add_sample_choice("field", asset, vec![[0.1 * asset as f32; 8]; 48000]);
    }
    for pitch in 40..104 {
        engine.piano_note("keys", pitch, 100);
    }
    let mut output = vec![[0.; 8]; 1024];
    engine.controller("sliders", 0, None);
    engine.controller("knobs", 0, Some(0.333));
    CALLS.set(0);
    CHECK.set(true);
    for index in 0..64 {
        if index == 32 {
            for pitch in 40..104 {
                engine.piano_note("keys", pitch, 0);
            }
        }
        if index == 48 {
            for pitch in 40..104 {
                engine.piano_note("keys", pitch, 100);
            }
        }
        engine.node_midi_message(
            "local",
            pr0_core::midi::Message {
                status: 0xb0,
                data1: 1,
                data2: index,
            },
        );
        engine.node_midi_message(
            "keys",
            pr0_core::midi::Message {
                status: 0xe0,
                data1: 0,
                data2: if index % 2 == 0 { 96 } else { 32 },
            },
        );
        engine.render(&[], &mut output);
    }
    CHECK.set(false);
    assert_eq!(
        CALLS.get(),
        0,
        "render must neither allocate nor free prepared storage"
    );
    assert!(output.iter().flatten().all(|v| v.is_finite()));
}

#[test]
fn sample_shortlist_switching_neither_allocates_nor_frees_audio_buffers() {
    let mut selector = node("selector", "sample_selector");
    selector.sample_choices = (1..=3)
        .map(|asset| pr0_core::SampleChoice {
            asset,
            name: format!("Sample {asset}"),
            nickname: String::new(),
        })
        .collect();
    let mut graph = Graph {
        nodes: vec![selector, node("keys", "piano")],
        edges: vec![],
    };
    for kind in ["sample", "poly_sampler", "granular_synth"] {
        graph.nodes.push(node(kind, kind));
        if kind != "sample" {
            graph.edges.push(Edge {
                id: format!("notes-{kind}"),
                source: "keys".into(),
                source_port: "midi".into(),
                target: kind.into(),
                target_port: "midi".into(),
            });
        }
        graph.edges.push(Edge {
            id: kind.into(),
            source: "selector".into(),
            source_port: "out".into(),
            target: kind.into(),
            target_port: "sample_id".into(),
        });
    }
    // The field's sample setter follows the same shortlist through a parameter.
    let mut field = node("field", "granular_field");
    field.sample_choices = graph.nodes[0].sample_choices.clone();
    graph.nodes.push(field);
    graph.edges.push(Edge {
        id: "field-sample_1".into(),
        source: "selector".into(),
        source_port: "out".into(),
        target: "field".into(),
        target_port: "sample_1".into(),
    });
    let mut engine = pr0_dsp::Engine::prepare(graph, 48000.).unwrap();
    for kind in ["sample", "poly_sampler", "granular_synth", "field"] {
        for asset in 1..=3 {
            engine.add_sample_choice(kind, asset, vec![[asset as f32 * 0.1; 8]; 4096]);
        }
    }
    engine.clock.running = true;
    let mut output = [[0.; 8]; 32];
    CALLS.set(0);
    CHECK.set(true);
    for index in 0..128 {
        engine
            .parameter("selector", "index", (index % 4) as f64)
            .unwrap();
        engine.render(&[], &mut output);
        engine.node_midi_message(
            "keys",
            pr0_core::midi::Message {
                status: 0x90,
                data1: 60,
                data2: 100,
            },
        );
        engine.render(&[], &mut output);
    }
    CHECK.set(false);
    assert_eq!(
        CALLS.get(),
        0,
        "sample switches must retain all prepared storage"
    );
}

#[test]
fn independent_part_players_repeat_retrigger_stop_and_retire_without_heap_activity() {
    use pr0_core::{
        midi::Message,
        score::{AutomationEvent, AutomationLane, Curve, MessageKind},
    };
    use pr0_dsp::part_player::{Clip, Event};
    let graph = Graph {
        nodes: vec![
            node("play", "trigger"),
            node("repeat", "trigger"),
            node("stop", "trigger"),
            node("clip", "part_player"),
            node("sink", "midi_output"),
        ],
        edges: [
            ("play", "out", "clip", "play"),
            ("repeat", "out", "clip", "repeat"),
            ("stop", "out", "clip", "stop"),
            ("clip", "midi", "sink", "midi"),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (source, source_port, target, target_port))| Edge {
            id: i.to_string(),
            source: source.into(),
            source_port: source_port.into(),
            target: target.into(),
            target_port: target_port.into(),
        })
        .collect(),
    };
    let mut e = pr0_dsp::Engine::prepare(graph.clone(), 1000.).unwrap();
    e.clock.bpm = 400.;
    e.set_part_player(
        "clip",
        Clip {
            length: 0.25,
            meters: vec![],
            spans: vec![],
            events: vec![Event {
                beat: 0.,
                message: Message {
                    status: 0x93,
                    data1: 64,
                    data2: 100,
                },
            }],
            automation: vec![AutomationLane {
                id: "pedal".into(),
                name: "Pedal".into(),
                channel: 4,
                message: MessageKind::Cc,
                number: 64,
                initial: Some(0.),
                events: vec![AutomationEvent {
                    id: "down".into(),
                    beat: 0.,
                    duration: 0.,
                    start: 127.,
                    end: 127.,
                    curve: Curve::Step,
                }],
            }],
        },
    );
    let mut replacement = pr0_dsp::Engine::prepare(graph, 1000.).unwrap();
    CALLS.with(|c| c.set(0));
    CHECK.with(|c| c.set(true));
    for sample in 0..10000 {
        if sample % 901 == 0 {
            e.bang("repeat");
        }
        if sample % 997 == 0 {
            e.bang("play");
        }
        if sample % 2003 == 0 {
            e.bang("stop");
        }
        e.render(&[], &mut [[0.; 8]]);
        while e.take_midi_message("sink").is_some() {}
    }
    replacement.carry_node_state(&mut e);
    replacement.render(&[], &mut [[0.; 8]]);
    CHECK.with(|c| c.set(false));
    assert_eq!(
        CALLS.with(Cell::get),
        0,
        "part playback must not allocate or free storage while rendering or carrying state"
    );
}

#[test]
fn state_selector_emission_and_recall_transfer_are_allocation_free() {
    let mut source=node("source","value");source.parameters.insert("value".into(),1.);
    let receiver=node("receiver","subgraph");
    let graph=Graph{nodes:vec![source,receiver],edges:vec![Edge{id:"state".into(),source:"source".into(),source_port:"out".into(),target:"receiver".into(),target_port:"state".into()}]};
    let mut engine=pr0_dsp::Engine::prepare(graph.clone(),48000.).unwrap();
    let mut replacement=pr0_dsp::Engine::prepare(graph,48000.).unwrap();
    let restored=vec!["source".into()];
    CHECK.set(true);CALLS.set(0);
    engine.render(&[],&mut [[0.;8];128]);replacement.carry_recall_state(&mut engine);
    for _ in 0..256 {replacement.republish_restored(&restored);replacement.render(&[],&mut [[0.;8];128]);}
    CHECK.set(false);assert_eq!(CALLS.get(),0);assert_eq!(replacement.take_state_requests().len(),1);
}
