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
fn phasor(id: &str, params: &[(&str, f64)]) -> pr0_core::Node {
    let mut all = vec![("frequency", 2.), ("glide", 0.)];
    all.extend_from_slice(params);
    node(id, "phasor", &all)
}
fn out(e: &Engine, id: &str) -> f64 {
    e.telemetry()[id]["_out"]
}
fn trace(e: &mut Engine, id: &str, samples: usize) -> Vec<f64> {
    (0..samples)
        .map(|_| {
            e.render(&[], &mut [[0.; 8]]);
            out(e, id)
        })
        .collect()
}
#[test]
fn the_ramp_runs_zero_to_one_once_per_cycle() {
    let g = Graph { nodes: vec![phasor("a", &[])], edges: vec![] };
    let mut e = Engine::prepare(g, 1000.).unwrap();
    let values = trace(&mut e, "a", 1000);
    // 2 Hz at 1 kHz is a 500-sample cycle, and the first sample is the start of one.
    for (i, v) in values.iter().enumerate() {
        let expected = (i as f64 * 0.002).fract();
        assert!((v - expected).abs() < 1e-9, "sample {i}: {v} vs {expected}");
    }
    // One thousand samples cover two cycles, so the ramp falls once, at the wrap.
    let falls = values.windows(2).filter(|w| w[1] < w[0]).count();
    assert_eq!(falls, 1, "{values:?}");
    assert!(values.iter().all(|v| (0. ..1.).contains(v)));
}
#[test]
fn a_frequency_change_bends_the_slope_without_moving_the_value() {
    let g = Graph { nodes: vec![phasor("a", &[("frequency", 1.)])], edges: vec![] };
    let mut e = Engine::prepare(g, 1000.).unwrap();
    trace(&mut e, "a", 300);
    let before = out(&e, "a");
    e.parameter("a", "frequency", 40.).unwrap();
    e.render(&[], &mut [[0.; 8]]);
    // Without glide the step is the new one immediately, but the ramp still
    // continues from where it stood: no jump, only a steeper climb.
    let after = out(&e, "a");
    assert!((after - before - 0.001).abs() < 1e-9, "{before} -> {after}");
    let values = trace(&mut e, "a", 200);
    assert!(values.windows(2).filter(|w| w[1] < w[0]).count() == 8, "40 Hz now: {values:?}");
}
#[test]
fn glide_eases_the_slope_in_and_zero_frequency_holds_the_ramp() {
    let g = Graph { nodes: vec![phasor("a", &[("frequency", 1.), ("glide", 100.)])], edges: vec![] };
    let mut e = Engine::prepare(g, 1000.).unwrap();
    // The first sample adopts the set frequency instead of sliding up from zero.
    let start = trace(&mut e, "a", 2);
    assert!((start[1] - start[0] - 0.001).abs() < 1e-9, "{start:?}");
    trace(&mut e, "a", 300);
    e.parameter("a", "frequency", 40.).unwrap();
    let eased = trace(&mut e, "a", 2);
    // One sample into a 100 ms glide the slope has barely left 1 Hz.
    let step = eased[1] - eased[0];
    assert!(step > 0.001 && step < 0.0015, "{step}");
    // Given the glide time the slope arrives at the new frequency.
    let settled = trace(&mut e, "a", 1000);
    let step = settled[999] - settled[998];
    assert!((step - 0.04).abs() < 1e-4, "{step}");
    // Zero frequency is a hold, not a reset.
    e.parameter("a", "glide", 0.).unwrap();
    e.parameter("a", "frequency", 0.).unwrap();
    let held = trace(&mut e, "a", 50);
    assert!(held[0] > 0. && held.iter().all(|v| *v == held[0]), "{held:?}");
}
#[test]
fn a_sync_edge_restarts_the_ramp_at_zero() {
    let mut trigger = node("t", "value", &[("value", 0.)]);
    trigger.channels = 1;
    let g = Graph {
        nodes: vec![trigger, phasor("a", &[])],
        edges: vec![pr0_core::Edge { id: "s".into(), source: "t".into(), source_port: "out".into(), target: "a".into(), target_port: "sync".into() }],
    };
    let mut e = Engine::prepare(g, 1000.).unwrap();
    trace(&mut e, "a", 137);
    assert!(out(&e, "a") > 0.);
    e.parameter("t", "value", 1.).unwrap();
    let restarted = trace(&mut e, "a", 3);
    assert_eq!(restarted[0], 0.);
    assert!((restarted[1] - 0.002).abs() < 1e-9, "{restarted:?}");
    // A held-high sync does not freeze the ramp; only the rising edge restarts it.
    trace(&mut e, "a", 100);
    assert!(out(&e, "a") > 0.2);
}
#[test]
fn a_cabled_frequency_drives_the_ramp() {
    let mut rate = node("r", "value", &[("value", 5.)]);
    rate.channels = 1;
    let g = Graph {
        nodes: vec![rate, phasor("f", &[])],
        edges: vec![pr0_core::Edge { id: "r".into(), source: "r".into(), source_port: "out".into(), target: "f".into(), target_port: "frequency".into() }],
    };
    let mut e = Engine::prepare(g, 1000.).unwrap();
    let values = trace(&mut e, "f", 1000);
    assert_eq!(values.windows(2).filter(|w| w[1] < w[0]).count(), 4, "one second at the cabled 5 Hz: {values:?}");
}
