use super::*;
fn graph(curve: f64, snap: f64) -> Graph {
    let mut source = pr0_core::demo_project("x".into(), "x".into(), pr0_core::Mode::Freeform)
        .graph
        .nodes[0]
        .clone();
    source.id = "v".into();
    source.kind = "value".into();
    source.label = "v".into();
    source.channels = 1;
    source.parameters.clear();
    source.parameters.insert("value".into(), 0.);
    let mut smooth = source.clone();
    smooth.id = "s".into();
    smooth.kind = "smooth_change".into();
    smooth.label = "s".into();
    smooth.parameters.clear();
    for (key, value) in [("curve", curve), ("time", 100.), ("snap", snap)] {
        smooth.parameters.insert(key.into(), value);
    }
    Graph {
        nodes: vec![source, smooth],
        edges: vec![pr0_core::Edge { id: "e".into(), source: "v".into(), source_port: "out".into(), target: "s".into(), target_port: "in".into() }],
    }
}
fn out(e: &Engine) -> f64 {
    e.telemetry()["s"]["_out"]
}
fn step(e: &mut Engine, to: f64) {
    e.parameter("v", "value", to).unwrap();
}
#[test]
fn a_stepped_input_glides_over_the_move_time_and_arrives() {
    let mut e = Engine::prepare(graph(0., 0.), 1000.).unwrap();
    e.render(&[], &mut [[0.; 8]; 10]);
    assert_eq!(out(&e), 0., "a settled input is passed through untouched");
    step(&mut e, 1.);
    // 100 ms at 1 kHz: the linear curve is half way after half the move.
    e.render(&[], &mut [[0.; 8]; 50]);
    assert!((out(&e) - 0.5).abs() < 0.02, "{}", out(&e));
    e.render(&[], &mut [[0.; 8]; 50]);
    assert_eq!(out(&e), 1., "a shaped move lands exactly on the target");
    // Settled again, so the next step starts its own move.
    step(&mut e, 3.);
    e.render(&[], &mut [[0.; 8]; 50]);
    assert!(out(&e) > 1.9 && out(&e) < 2.1, "{}", out(&e));
}
#[test]
fn snap_is_what_makes_a_chasing_curve_arrive() {
    // Exponential with no snap approaches the target and never reaches it.
    let mut e = Engine::prepare(graph(4., 0.), 1000.).unwrap();
    e.render(&[], &mut [[0.; 8]]);
    step(&mut e, 1.);
    e.render(&[], &mut [[0.; 8]; 100]);
    assert!((out(&e) - 0.632).abs() < 0.01, "one time constant: {}", out(&e));
    e.render(&[], &mut [[0.; 8]; 2000]);
    assert!(out(&e) < 1. && out(&e) > 0.999, "close but never there: {}", out(&e));
    // The same curve with the default snap finishes the move.
    let mut e = Engine::prepare(graph(4., 5.), 1000.).unwrap();
    e.render(&[], &mut [[0.; 8]]);
    step(&mut e, 1.);
    // Three time constants is the last 5%, and the move begins on the sample
    // the new value arrives.
    e.render(&[], &mut [[0.; 8]; 299]);
    assert!(out(&e) < 1.);
    e.render(&[], &mut [[0.; 8]]);
    assert_eq!(out(&e), 1., "three time constants is within 5%");
}
#[test]
fn the_curve_and_time_can_be_changed_while_the_engine_runs() {
    let mut e = Engine::prepare(graph(1., 0.), 1000.).unwrap();
    e.render(&[], &mut [[0.; 8]]);
    step(&mut e, 1.);
    e.render(&[], &mut [[0.; 8]; 50]);
    // Ease in is a quarter of the way at half time, where ease out is three
    // quarters; selecting a curve live re-shapes the rest of the move.
    assert!((out(&e) - 0.25).abs() < 0.02, "{}", out(&e));
    e.parameter("s", "curve", 2.).unwrap();
    e.render(&[], &mut [[0.; 8]; 50]);
    assert_eq!(out(&e), 1.);
}
