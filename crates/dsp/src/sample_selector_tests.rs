use super::*;
fn node(id: &str, kind: &str, channels: usize) -> pr0_core::Node {
    let mut n = pr0_core::demo_project("p".into(), "p".into(), pr0_core::Mode::Structured)
        .graph
        .nodes
        .remove(0);
    n.id = id.into();
    n.kind = kind.into();
    n.label = id.into();
    n.channels = channels;
    n.parameters.clear();
    n
}
fn graph(kind: &str, channels: usize) -> Graph {
    let mut selector = node("selector", "sample_selector", channels);
    selector.sample_choices = vec![
        pr0_core::SampleChoice {
            asset: 11,
            name: "First".into(),
            nickname: "A".into(),
        },
        pr0_core::SampleChoice {
            asset: 22,
            name: "Second".into(),
            nickname: String::new(),
        },
    ];
    let mut graph = Graph {
        nodes: vec![selector, node("sampler", kind, channels)],
        edges: vec![pr0_core::Edge {
            id: "select".into(),
            source: "selector".into(),
            source_port: "out".into(),
            target: "sampler".into(),
            target_port: "sample_id".into(),
        }],
    };
    if kind != "sample" {
        graph.nodes.push(node("keys", "piano", channels));
        graph.edges.push(pr0_core::Edge {
            id: "notes".into(),
            source: "keys".into(),
            source_port: "midi".into(),
            target: "sampler".into(),
            target_port: "midi".into(),
        });
    }
    graph
}
#[test]
fn shortlist_switches_prepared_audio_and_invalid_indices_select_silence() {
    for channels in 1..=8 {
        for kind in ["sample", "poly_sampler", "granular_synth"] {
            let mut engine = Engine::prepare(graph(kind, channels), 48000.).unwrap();
            engine.add_sample_choice("sampler", 11, vec![[0.2; 8]; 4096]);
            engine.add_sample_choice("sampler", 22, vec![[-0.4; 8]; 4096]);
            engine.clock.running = true;
            for (index, asset, sign) in [
                (0., 11., 1.),
                (1., 22., -1.),
                (1.9, 22., -1.),
                (-1., 0., 0.),
                (99., 0., 0.),
                (0., 11., 1.),
            ] {
                engine.parameter("selector", "index", index).unwrap();
                engine.render(&[], &mut [[0.; 8]; 4]);
                if kind != "sample" {
                    engine.node_midi_message(
                        "keys",
                        pr0_core::midi::Message {
                            status: 0x90,
                            data1: 60,
                            data2: 100,
                        },
                    );
                }
                engine.render(&[], &mut [[0.; 8]; 32]);
                let telemetry = engine.telemetry();
                assert_eq!(telemetry["selector"]["_out"], asset);
                assert_eq!(telemetry["sampler"]["_sample_id"], asset);
                let sampler = engine.nodes.iter().find(|n| n.id == "sampler").unwrap();
                for value in &sampler.output[..channels] {
                    if sign == 0. {
                        assert_eq!(*value, 0.);
                    } else {
                        assert!(value * sign > 0., "{kind}: {value}");
                    }
                }
            }
        }
    }
}
#[test]
fn replacing_shortlist_refreshes_bank_while_unchanged_graph_retains_selection() {
    let original = graph("poly_sampler", 2);
    let mut old = Engine::prepare(original.clone(), 48000.).unwrap();
    old.add_sample_choice("sampler", 11, vec![[0.1; 8]; 128]);
    old.add_sample_choice("sampler", 22, vec![[0.2; 8]; 128]);
    old.render(&[], &mut [[0.; 8]; 4]);
    let mut next = Engine::prepare(original.clone(), 48000.).unwrap();
    next.add_sample_choice("sampler", 11, vec![[0.1; 8]; 128]);
    next.add_sample_choice("sampler", 22, vec![[0.2; 8]; 128]);
    next.carry_node_state(&mut old);
    next.render(&[], &mut [[0.; 8]; 4]);
    assert_eq!(next.telemetry()["sampler"]["_sample_id"], 11.);
    let mut changed = original;
    changed.nodes[0].sample_choices[0].asset = 33;
    let mut replacement = Engine::prepare(changed, 48000.).unwrap();
    replacement.add_sample_choice("sampler", 22, vec![[0.2; 8]; 128]);
    replacement.add_sample_choice("sampler", 33, vec![[0.3; 8]; 128]);
    replacement.carry_node_state(&mut next);
    replacement.render(&[], &mut [[0.; 8]; 4]);
    assert_eq!(replacement.telemetry()["selector"]["_out"], 33.);
    assert_eq!(replacement.telemetry()["sampler"]["_sample_id"], 33.);
}

#[test]
fn unprepared_sample_id_reports_silence_even_with_an_empty_bank() {
    let mut engine = Engine::prepare(graph("poly_sampler", 2), 48000.).unwrap();
    engine.render(&[], &mut [[0.; 8]; 4]);
    let telemetry = engine.telemetry();
    assert_eq!(telemetry["selector"]["_out"], 11.);
    assert_eq!(telemetry["sampler"]["_sample_id"], 0.);
    assert_eq!(telemetry["sampler"]["_sample_missing"], 1.);
}
