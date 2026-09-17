use super::*;
fn node(id: &str, kind: &str, params: &[(&str, f64)]) -> pr0_core::Node {
    let mut n = pr0_core::demo_project("x".into(), "x".into(), pr0_core::Mode::Freeform)
        .graph
        .nodes[0]
        .clone();
    n.id = id.into();
    n.kind = kind.into();
    n.label = id.into();
    n.channels = 1;
    n.parameters.clear();
    for (key, value) in params {
        n.parameters.insert((*key).into(), *value);
    }
    n
}
fn lfo(id: &str, extra: &[(&str, f64)]) -> pr0_core::Node {
    let mut params = vec![("rate", 2.), ("min", -1.), ("max", 1.)];
    params.extend_from_slice(extra);
    node(id, "lfo", &params)
}
fn out(e: &Engine, id: &str) -> f64 {
    e.telemetry()[id]["_out"]
}
#[test]
fn lfos_at_one_rate_are_identical_regardless_of_when_they_were_added() {
    let mut first = Engine::prepare(Graph { nodes: vec![lfo("a", &[])], edges: vec![] }, 1000.).unwrap();
    // Run partway into a cycle, then add a second LFO by live replacement.
    first.render(&[], &mut [[0.; 8]; 137]);
    let mut second = Engine::prepare(Graph { nodes: vec![lfo("a", &[]), lfo("b", &[])], edges: vec![] }, 1000.).unwrap();
    second.carry_node_state(&mut first);
    for _ in 0..600 {
        second.render(&[], &mut [[0.; 8]]);
        let (a, b) = (out(&second, "a"), out(&second, "b"));
        assert!((a - b).abs() < 1e-9, "{a} vs {b}");
    }
    // The shared clock keeps counting across the replacement: phase is not restarted.
    // The clock advances after the nodes run, so the last output belongs to the previous sample.
    let t = (second.graph_clock.sample - 1) as f64 / 1000.;
    let expected = (std::f64::consts::TAU * (t * 2.).fract()).sin();
    assert!((out(&second, "a") - expected).abs() < 1e-6, "{} vs {expected}", out(&second, "a"));
}
#[test]
fn phase_offset_shifts_the_cycle_and_a_half_turn_inverts_it() {
    let g = Graph { nodes: vec![lfo("a", &[]), lfo("b", &[("phase", 180.)]), lfo("c", &[("phase", 90.)])], edges: vec![] };
    let mut e = Engine::prepare(g, 1000.).unwrap();
    for _ in 0..800 {
        e.render(&[], &mut [[0.; 8]]);
        assert!((out(&e, "a") + out(&e, "b")).abs() < 1e-9, "180° inverts around the midpoint");
    }
    // 90° ahead: c leads a by a quarter cycle (125 samples at 2 Hz and 1 kHz).
    let mut a_trace = Vec::new();
    let mut c_trace = Vec::new();
    for _ in 0..500 {
        e.render(&[], &mut [[0.; 8]]);
        a_trace.push(out(&e, "a"));
        c_trace.push(out(&e, "c"));
    }
    for i in 125..500 {
        assert!((c_trace[i - 125] - a_trace[i]).abs() < 1e-6, "quarter-cycle lead at {i}");
    }
}
#[test]
fn sync_pulse_restarts_the_cycle_and_driven_rate_runs_free() {
    let mut trigger = node("t", "value", &[("value", 0.)]);
    trigger.channels = 1;
    let g = Graph {
        nodes: vec![trigger, lfo("a", &[("phase", 90.)])],
        edges: vec![pr0_core::Edge { id: "s".into(), source: "t".into(), source_port: "out".into(), target: "a".into(), target_port: "sync".into() }],
    };
    let mut e = Engine::prepare(g, 1000.).unwrap();
    e.render(&[], &mut [[0.; 8]; 333]);
    e.parameter("t", "value", 1.).unwrap();
    e.render(&[], &mut [[0.; 8]]);
    // Right after the edge the cycle sits at its phase offset: 90° is the peak.
    assert!((out(&e, "a") - 1.).abs() < 1e-6, "{}", out(&e, "a"));
    e.render(&[], &mut [[0.; 8]; 250]);
    assert!((out(&e, "a") + 1.).abs() < 1e-6, "half a cycle later it is the trough: {}", out(&e, "a"));
    // A cabled rate accumulates instead of following the clock, but still oscillates.
    let mut rate = node("r", "value", &[("value", 5.)]);
    rate.channels = 1;
    let g = Graph {
        nodes: vec![rate, lfo("f", &[])],
        edges: vec![pr0_core::Edge { id: "r".into(), source: "r".into(), source_port: "out".into(), target: "f".into(), target_port: "rate".into() }],
    };
    let mut e = Engine::prepare(g, 1000.).unwrap();
    let mut values = Vec::new();
    for _ in 0..400 {
        e.render(&[], &mut [[0.; 8]]);
        values.push(out(&e, "f"));
    }
    assert!(values.iter().cloned().fold(-2., f64::max) > 0.9 && values.iter().cloned().fold(2., f64::min) < -0.9, "{values:?}");
}
#[test]
fn unlocked_lfo_glides_through_rate_changes_while_locked_lfo_jumps() {
    let g = Graph { nodes: vec![lfo("locked", &[]), lfo("free", &[("clock_lock", 0.)])], edges: vec![] };
    let mut e = Engine::prepare(g, 1000.).unwrap();
    // Both start in step: the free LFO accumulates from the same origin.
    for _ in 0..300 {
        e.render(&[], &mut [[0.; 8]]);
        assert!((out(&e, "locked") - out(&e, "free")).abs() < 1e-6);
    }
    let before = (out(&e, "locked"), out(&e, "free"));
    e.parameter("locked", "rate", 7.).unwrap();
    e.parameter("free", "rate", 7.).unwrap();
    e.render(&[], &mut [[0.; 8]]);
    let after = (out(&e, "locked"), out(&e, "free"));
    // Free: one sample at 7 Hz moves the sine by at most 2π·0.007 ≈ 0.044.
    assert!((after.1 - before.1).abs() < 0.05, "free glides: {} -> {}", before.1, after.1);
    // Locked: phase = t·rate jumps when the rate changes mid-run.
    assert!((after.0 - before.0).abs() > 0.2, "locked jumps: {} -> {}", before.0, after.0);
}
