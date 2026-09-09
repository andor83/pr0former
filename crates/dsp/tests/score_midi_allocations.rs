//! Exercise prepared DSP paths under a thread-local allocator guard.
use pr0_core::{Edge, Graph, Node};
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
fn typed_midi_dispatch_and_render_do_not_allocate_or_free() {
    let mut source = node("source", "part_midi");
    source.part_id = Some("p".into());
    let graph = Graph {
        nodes: vec![source, node("out", "midi_output")],
        edges: vec![Edge {
            id: "events".into(),
            source: "source".into(),
            source_port: "events".into(),
            target: "out".into(),
            target_port: "events".into(),
        }],
    };
    let mut engine = pr0_dsp::Engine::prepare(graph, 48000.).unwrap();
    let mut output = [[0.; 8]];
    CALLS.set(0);
    CHECK.set(true);
    for value in 0..1024 {
        engine.part_message(
            "p",
            pr0_core::midi::Message {
                status: 0xb0,
                data1: 11,
                data2: (value % 128) as u8,
            },
        );
        engine.render(&[], &mut output);
        while engine.take_midi_message("out").is_some() {}
    }
    CHECK.set(false);
    assert_eq!(CALLS.get(), 0);
}
