use super::*;
fn node(id: &str, kind: &str, channels: usize) -> pr0_core::Node {
    let mut n = pr0_core::demo_project("x".into(), "x".into(), pr0_core::Mode::Freeform)
        .graph
        .nodes[0]
        .clone();
    n.id = id.into();
    n.kind = kind.into();
    n.label = id.into();
    n.channels = channels;
    n.parameters.clear();
    n
}
fn field(channels: usize, choices: &[u32], params: &[(&str, f64)]) -> pr0_core::Node {
    let mut n = node("field", "granular_field", channels);
    n.sample_choices = choices
        .iter()
        .map(|asset| pr0_core::SampleChoice { asset: *asset, name: format!("S{asset}"), nickname: String::new() })
        .collect();
    for (key, value) in params {
        n.parameters.insert((*key).into(), *value);
    }
    n
}
fn edge(source: &str, target: &str, target_port: &str) -> pr0_core::Edge {
    pr0_core::Edge {
        id: format!("{source}-{target}-{target_port}"),
        source: source.into(),
        source_port: "out".into(),
        target: target.into(),
        target_port: target_port.into(),
    }
}
fn energy(e: &mut Engine, samples: usize) -> f64 {
    let mut sum = 0.;
    for _ in 0..samples {
        e.render(&[], &mut [[0.; 8]]);
        sum += e.audio_frame("field", "out")[0] as f64;
    }
    sum
}
fn two_samples(channels: usize, extra: &[(&str, f64)]) -> Engine {
    let mut params = vec![
        ("source_1_x", -0.8), ("source_1_y", 0.), ("source_2_x", 0.8), ("source_2_y", 0.),
        ("grain_ms", 50.), ("density", 20.), ("spray", 0.), ("focus", 0.3), ("x", -0.8), ("y", 0.),
    ];
    params.extend_from_slice(extra);
    let g = Graph { nodes: vec![field(channels, &[1, 2], &params)], edges: vec![] };
    let mut e = Engine::prepare(g, 1000.).unwrap();
    e.add_sample_choice("field", 1, vec![[1.; 8]; 10000]);
    e.add_sample_choice("field", 2, vec![[-1.; 8]; 10000]);
    e
}
#[test]
fn granular_field_plays_the_nearest_sample_at_every_channel_width() {
    for channels in 1..=8 {
        let mut e = two_samples(channels, &[]);
        assert!(!e.clock.running, "the field runs without transport");
        assert!(energy(&mut e, 300) > 1., "{channels}: the positive source at x=-0.8 sounds");
        let frame = e.audio_frame("field", "out");
        assert!(frame[..channels].iter().all(|v| v.is_finite()) && frame[channels..].iter().all(|v| *v == 0.), "{channels}: {frame:?}");
        e.parameter("field", "x", 0.8).unwrap();
        energy(&mut e, 100); // in-flight grains from the old source finish
        assert!(energy(&mut e, 300) < -1., "{channels}: the negative source at x=0.8 sounds");
        let t = e.telemetry();
        let f = &t["field"];
        assert!(f["_source_2_weight"] > 0.99 && f["_source_1_weight"] < 0.01, "{f:?}");
        assert_eq!((f["_x"], f["_sample_1_missing"], f["_sample_2_missing"], f["_source_9_weight"]), (0.8, 0., 0., 0.));
        assert!(f["_grains"] >= 1.);
        // A sample setter pointing at an unknown asset silences that slot and reports it.
        e.parameter("field", "sample_2", 999.).unwrap();
        energy(&mut e, 100);
        assert_eq!(energy(&mut e, 300), 0., "{channels}: missing sample is silent");
        assert_eq!(e.telemetry()["field"]["_sample_2_missing"], 1.);
        e.parameter("field", "sample_2", 1.).unwrap();
        energy(&mut e, 100);
        assert!(energy(&mut e, 300) > 1., "{channels}: the setter re-points slot 2 at sample 1");
    }
}
#[test]
fn pitch_transposes_grains_and_randomize_scatters_them() {
    // A 10 Hz sine at 1 kHz: the Hann window scales it but never moves its zero crossings.
    let sine: Vec<[f32; 8]> = (0..20000).map(|i| [(std::f64::consts::TAU * 10. * i as f64 / 1000.).sin() as f32; 8]).collect();
    let crossings = |pitch: f64, randomize: f64| {
        let params = [("grain_ms", 400.), ("density", 2.), ("spray", 0.), ("position", 0.1), ("pitch", pitch), ("randomize_pitch", randomize), ("amplitude", 1.), ("x", 0.7), ("y", 0.)];
        let g = Graph { nodes: vec![field(1, &[1], &params)], edges: vec![] };
        let mut e = Engine::prepare(g, 1000.).unwrap();
        e.add_sample_choice("field", 1, sine.clone());
        let mut previous = 0f64;
        let mut count = 0;
        for _ in 0..2000 {
            e.render(&[], &mut [[0.; 8]]);
            let v = e.audio_frame("field", "out")[0] as f64;
            if v.abs() > 1e-3 {
                if previous != 0. && (v > 0.) != (previous > 0.) {
                    count += 1;
                }
                previous = v;
            }
        }
        count as f64
    };
    let (base, octave) = (crossings(0., 0.), crossings(12., 0.));
    assert!(base >= 24., "grains sound: {base}");
    assert!(octave > base * 1.7 && octave < base * 2.3, "an octave doubles the read speed: {base} -> {octave}");
    let random = crossings(12., 1.);
    assert!(random > base * 0.4 && random < base * 2.3, "random offsets within an octave: {base} vs {random}");
}
#[test]
fn live_input_joins_the_field_only_while_connected() {
    let params = [("position", 1.), ("spray", 0.), ("grain_ms", 20.), ("density", 50.), ("x", 0.), ("y", 0.), ("amplitude", 1.)];
    let g = Graph {
        nodes: vec![node("mic", "browser_input", 1), field(1, &[], &params)],
        edges: vec![edge("mic", "field", "live_1")],
    };
    let mut e = Engine::prepare(g, 1000.).unwrap();
    let mut positive = 0.;
    let mut negative = 0.;
    let mut fills = Vec::new();
    for t in 0..1200 {
        let value = if t < 600 { 1. } else { -1. };
        e.external("mic", [value; 8]);
        e.render(&[], &mut [[0.; 8]]);
        let out = e.audio_frame("field", "out")[0] as f64;
        if (100..600).contains(&t) {
            assert!(out >= -1e-6, "t {t}: only captured positive audio plays: {out}");
            positive += out;
        }
        if t >= 700 {
            assert!(out <= 1e-6, "t {t}: after the flip only negative audio plays: {out}");
            negative += out;
        }
        if t % 100 == 0 {
            fills.push(e.telemetry()["field"]["_live_1_fill"]);
        }
    }
    assert!(positive > 1. && negative < -1., "{positive} {negative}");
    assert!(fills.windows(2).all(|w| w[1] >= w[0]) && *fills.last().unwrap() == 1., "{fills:?}");
    let f = e.telemetry();
    assert!(f["field"]["_source_9_weight"] > 0.99, "{:?}", f["field"]);
    // Without the cable the same node has no live source and stays silent.
    let g = Graph { nodes: vec![node("mic", "browser_input", 1), field(1, &[], &params)], edges: vec![] };
    let mut e = Engine::prepare(g, 1000.).unwrap();
    for _ in 0..300 {
        e.external("mic", [1.; 8]);
        e.render(&[], &mut [[0.; 8]]);
        assert_eq!(e.audio_frame("field", "out")[0], 0.);
    }
    let f = e.telemetry();
    assert_eq!((f["field"]["_source_9_weight"], f["field"]["_live_1_fill"], f["field"]["_grains"]), (0., 0., 0.));
}
#[test]
fn live_buffer_memory_is_capped_across_nodes() {
    let build = |fields: usize| {
        let mut nodes = vec![node("mic", "browser_input", 2)];
        let mut edges = vec![];
        for i in 0..fields {
            let mut f = field(2, &[], &[]);
            f.id = format!("field{i}");
            nodes.push(f);
            edges.push(edge("mic", &format!("field{i}"), "live_1"));
            edges.push(edge("mic", &format!("field{i}"), "live_2"));
        }
        Graph { nodes, edges }
    };
    assert!(Engine::prepare(build(4), 96000.).is_ok(), "eight rings fit under the cap");
    let err = Engine::prepare(build(5), 96000.).err().expect("the ninth and tenth rings exceed the cap");
    assert!(err.contains("exceed 256 MiB"), "{err}");
}
#[test]
fn compatible_replacements_carry_grains_and_rings() {
    let mut a = two_samples(2, &[]);
    energy(&mut a, 120);
    let mut b = two_samples(2, &[]);
    b.carry_node_state(&mut a);
    assert!(energy(&mut b, 10).abs() > 0., "in-flight grains keep sounding after the swap");
    // A different sample list is an incompatible node; the field state still moves across.
    let params = [("source_1_x", -0.8), ("source_1_y", 0.), ("grain_ms", 50.), ("density", 20.), ("spray", 0.), ("focus", 0.3), ("x", -0.8), ("y", 0.)];
    let g = Graph { nodes: vec![field(2, &[1, 2, 3], &params)], edges: vec![] };
    let mut c = Engine::prepare(g, 1000.).unwrap();
    c.carry_node_state(&mut b);
    c.add_sample_choice("field", 1, vec![[1.; 8]; 10000]);
    c.add_sample_choice("field", 2, vec![[-1.; 8]; 10000]);
    energy(&mut c, 100);
    let f = c.telemetry();
    assert_eq!(f["field"]["_sample_3_missing"], 1., "slot 3 has no audio yet");
    assert!(energy(&mut c, 300) > 1.);
}
