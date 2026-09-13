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
fn node(id: &str, kind: &str) -> Node {
    Node {
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
        if n.id == "one" {
            n.parameters.insert("value".into(), 1.);
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
    ] {
        wire(s, sp, t, tp);
    }
    let mut engine = pr0_dsp::Engine::prepare(graph, 48000.).unwrap();
    for id in ["sample", "grains"] {
        engine.set_sample(id, vec![[0.1; 8]; 48000]);
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
