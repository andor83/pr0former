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
fn edge(source: &str, target_port: &str) -> pr0_core::Edge {
    pr0_core::Edge {
        id: target_port.into(),
        source: source.into(),
        source_port: "out".into(),
        target: "cloud".into(),
        target_port: target_port.into(),
    }
}
fn energy(e: &mut Engine, samples: usize) -> f64 {
    let mut sum = 0.;
    for _ in 0..samples {
        e.render(&[], &mut [[0.; 8]]);
        sum += e.audio_frame("cloud", "out")[0] as f64;
    }
    sum
}
#[test]
fn granular_cloud_autoplays_scans_and_switches_samples_at_every_channel_width() {
    for channels in 1..=8 {
        let mut cloud = node("cloud", "granular_cloud", channels);
        cloud.parameters.extend([
            ("position".into(), 0.1),
            ("spray".into(), 0.),
            ("grain_ms".into(), 5.),
            ("density".into(), 100.),
        ]);
        let mut e = Engine::prepare(
            Graph {
                nodes: vec![cloud],
                edges: vec![],
            },
            1000.,
        )
        .unwrap();
        e.add_sample_choice(
            "cloud",
            1,
            (0..10000)
                .map(|i| [if i < 5000 { 1. } else { -1. }; 8])
                .collect(),
        );
        e.add_sample_choice("cloud", 2, vec![[0.25; 8]; 10000]);
        e.parameter("cloud", "asset", 1.).unwrap();
        assert!(!e.clock.running);
        assert!(energy(&mut e, 100) > 1.);
        e.parameter("cloud", "position", 0.75).unwrap();
        energy(&mut e, 10);
        assert!(energy(&mut e, 100) < -1.);
        for center in [0., 1.] {
            e.parameter("cloud", "position", center).unwrap();
            assert!(energy(&mut e, 100).is_finite());
        }
        e.parameter("cloud", "asset", 2.).unwrap();
        assert!(energy(&mut e, 100) > 0.);
        e.parameter("cloud", "asset", 999.).unwrap();
        assert_eq!(energy(&mut e, 100), 0.);
        assert_eq!(e.telemetry()["cloud"]["_sample_missing"], 1.);
    }
}
#[test]
fn granular_cloud_keeps_numeric_gates_and_rejects_midi_wires() {
    let graph = Graph {
        nodes: vec![node("cloud", "granular_cloud", 2), node("gate", "value", 2)],
        edges: vec![edge("gate", "gate")],
    };
    let mut e = Engine::prepare(graph, 1000.).unwrap();
    e.set_sample("cloud", vec![[1.; 8]; 1000]);
    assert_eq!(energy(&mut e, 200), 0.);
    e.parameter("gate", "value", 1.).unwrap();
    assert!(energy(&mut e, 200) > 0.);
    e.parameter("gate", "value", 0.).unwrap();
    energy(&mut e, 250);
    assert_eq!(energy(&mut e, 100), 0.);
    let graph = Graph {
        nodes: vec![node("cloud", "granular_cloud", 2), node("keys", "piano", 2)],
        edges: vec![pr0_core::Edge {
            id: "midi".into(),
            source: "keys".into(),
            source_port: "midi".into(),
            target: "cloud".into(),
            target_port: "midi".into(),
        }],
    };
    assert!(graph.validate().is_err());
    let catalog = catalog();
    let synth = catalog.iter().find(|n| n.kind == "granular_synth").unwrap();
    let cloud = catalog.iter().find(|n| n.kind == "granular_cloud").unwrap();
    assert_eq!(
        synth
            .inputs
            .iter()
            .filter(|p| p.signal != pr0_core::Signal::Midi)
            .map(|p| &p.id)
            .collect::<Vec<_>>(),
        cloud.inputs.iter().map(|p| &p.id).collect::<Vec<_>>()
    );
    assert_eq!(
        synth
            .parameters
            .iter()
            .map(|p| (&p.id, p.min, p.max, p.default))
            .collect::<Vec<_>>(),
        cloud
            .parameters
            .iter()
            .map(|p| (&p.id, p.min, p.max, p.default))
            .collect::<Vec<_>>()
    );
}
