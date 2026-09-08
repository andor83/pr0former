use super::*;
fn node(id: &str, kind: &str, y: f64) -> pr0_core::Node {
    let mut n = pr0_core::demo_project("p".into(), "Routing".into(), pr0_core::Mode::Freeform)
        .graph
        .nodes[0]
        .clone();
    n.id = id.into();
    n.label = id.into();
    n.kind = kind.into();
    n.parameters.clear();
    n.control_value =
        pr0_core::named_route(kind).then(|| pr0_core::ControlValue::Text("main".into()));
    n.channels = 2;
    n.x = 0.;
    n.y = y;
    n
}
fn edge(id: &str, source: &str, out: &str, target: &str, input: &str) -> pr0_core::Edge {
    pr0_core::Edge {
        id: id.into(),
        source: source.into(),
        source_port: out.into(),
        target: target.into(),
        target_port: input.into(),
    }
}
fn render(e: &mut Engine) {
    e.render(&[], &mut [[0.; 8]; 1]);
}
#[test]
fn controls_merge_in_sample_order_and_height_breaks_ties_after_live_moves() {
    let mut a = node("a", "value", 0.);
    a.parameters.insert("value".into(), 10.);
    let mut b = node("b", "value", 200.);
    b.parameters.insert("value".into(), 20.);
    let graph = pr0_core::Graph {
        nodes: vec![b, a, node("result", "value", 400.)],
        edges: vec![
            edge("b", "b", "out", "result", "value"),
            edge("a", "a", "out", "result", "value"),
        ],
    };
    let mut e = Engine::prepare(graph.clone(), 48000.).unwrap();
    render(&mut e);
    assert_eq!(e.telemetry()["result"]["_out"], 10.);
    assert_eq!(e.telemetry()["result"]["_driver_value"], 1.);
    e.parameter("b", "value", 21.).unwrap();
    render(&mut e);
    assert_eq!(e.telemetry()["result"]["_out"], 21.);
    e.parameter("a", "value", 11.).unwrap();
    render(&mut e);
    assert_eq!(e.telemetry()["result"]["_out"], 11.);
    e.parameter("a", "value", 12.).unwrap();
    e.parameter("b", "value", 22.).unwrap();
    render(&mut e);
    assert_eq!(e.telemetry()["result"]["_out"], 12.);
    e.parameter("b", "value", 0.).unwrap();
    render(&mut e);
    assert_eq!(e.telemetry()["result"]["_out"], 0.);
    // Move only: preserve seen values; apply new visual priority to the next tie.
    let mut graph = graph;
    graph.nodes[0].y = -100.;
    graph.nodes[0].parameters.insert("value".into(), 0.);
    graph.nodes[1].parameters.insert("value".into(), 12.);
    let mut next = Engine::prepare(graph, 48000.).unwrap();
    next.carry_node_state(&mut e);
    render(&mut next);
    assert_eq!(next.telemetry()["result"]["_out"], 0.);
    next.parameter("a", "value", 13.).unwrap();
    next.parameter("b", "value", 23.).unwrap();
    render(&mut next);
    assert_eq!(next.telemetry()["result"]["_out"], 23.);
}
#[test]
fn value_set_and_trigger_are_independent_and_output_holds_between_triggers() {
    let mut set = node("set", "value", 0.);
    set.parameters.insert("value".into(), 42.);
    let mut trigger = node("trigger", "value", 10.);
    trigger.parameters.insert("value".into(), 0.);
    let g = pr0_core::Graph {
        nodes: vec![set, trigger, node("stored", "value", 100.)],
        edges: vec![
            edge("set", "set", "out", "stored", "value"),
            edge("trigger", "trigger", "out", "stored", "trigger"),
        ],
    };
    let mut e = Engine::prepare(g, 48000.).unwrap();
    render(&mut e);
    assert_eq!(e.telemetry()["stored"]["value"], 42.);
    assert_eq!(e.telemetry()["stored"]["_out"], 0.);
    e.parameter("trigger", "value", -1.).unwrap();
    render(&mut e);
    assert_eq!(e.telemetry()["stored"]["_out"], 42.);
    e.parameter("set", "value", 24.).unwrap();
    e.parameter("trigger", "value", 0.).unwrap();
    render(&mut e);
    assert_eq!(e.telemetry()["stored"]["value"], 24.);
    assert_eq!(e.telemetry()["stored"]["_out"], 42.);
    e.parameter("trigger", "value", 2.).unwrap();
    render(&mut e);
    assert_eq!(e.telemetry()["stored"]["_out"], 24.);
}
#[test]
fn named_control_fans_out_with_one_sample_delay_and_string_retargeting() {
    let mut value = node("value", "value", 0.);
    value.parameters.insert("value".into(), 7.);
    let mut target = node("target", "control_input", 0.);
    target.parameters.insert("mode".into(), 4.);
    target.control_value = Some(pr0_core::ControlValue::Text("main".into()));
    let g = pr0_core::Graph {
        nodes: vec![
            node("receive", "receive_control", 10.),
            node("second", "receive_control", 20.),
            node("send", "send_control", 0.),
            value,
            target,
        ],
        edges: vec![
            edge("value", "value", "out", "send", "in"),
            edge("name", "target", "out", "receive", "target"),
        ],
    };
    let mut e = Engine::prepare(g, 48000.).unwrap();
    render(&mut e);
    assert_eq!(e.telemetry()["receive"]["_out"], 0.);
    render(&mut e);
    assert_eq!(e.telemetry()["receive"]["_out"], 7.);
    assert_eq!(e.telemetry()["second"]["_out"], 7.);
    e.control("target", &pr0_core::ControlValue::Text("elsewhere".into()));
    render(&mut e);
    assert_eq!(e.telemetry()["receive"]["_out"], 0.);
    assert_eq!(e.telemetry()["second"]["_out"], 7.);
    assert_eq!(e.route_targets()["receive"], "elsewhere");
    e.control("target", &pr0_core::ControlValue::Text("main".into()));
    render(&mut e);
    assert_eq!(e.telemetry()["receive"]["_out"], 7.);
}
#[test]
fn named_audio_mixes_channels_and_dynamic_width_mismatch_is_reported() {
    let mut tone = node("tone", "oscillator", 0.);
    tone.parameters = [("frequency".into(), 440.), ("amplitude".into(), 0.2)].into();
    let mut target = node("target", "control_input", 0.);
    target.parameters.insert("mode".into(), 4.);
    target.control_value = Some(pr0_core::ControlValue::Text("main".into()));
    let mut wide = node("wide", "receive_audio", 0.);
    wide.channels = 4;
    let g = pr0_core::Graph {
        nodes: vec![
            tone,
            node("a", "send_audio", 0.),
            node("b", "send_audio", 20.),
            node("receive", "receive_audio", 0.),
            wide,
            target,
        ],
        edges: vec![
            edge("a", "tone", "out", "a", "in"),
            edge("b", "tone", "out", "b", "in"),
            edge("target", "target", "out", "wide", "target"),
        ],
    };
    let mut e = Engine::prepare(g, 48000.).unwrap();
    render(&mut e);
    render(&mut e);
    let tone = e.nodes[0].output;
    render(&mut e);
    assert_eq!(e.nodes[3].output[0], tone[0] * 2.);
    assert_eq!(e.nodes[3].output[1], tone[1] * 2.);
    assert_eq!(e.telemetry()["wide"]["_route_error"], 1.);
    assert_eq!(e.nodes[4].output, [0.; 8]);
}
#[test]
fn named_spectral_preserves_frames_and_feedback_does_not_create_a_graph_cycle() {
    let mut fft = node("fft", "fft", 0.);
    fft.parameters = [("size".into(), 256.), ("overlap".into(), 4.)].into();
    let mut send = node("send", "send_spectral", 0.);
    send.parameters = fft.parameters.clone();
    let mut receive = node("receive", "receive_spectral", 0.);
    receive.parameters = fft.parameters.clone();
    let mut tone = node("tone", "oscillator", 0.);
    tone.parameters = [("frequency".into(), 440.), ("amplitude".into(), 0.2)].into();
    let g = pr0_core::Graph {
        nodes: vec![tone, fft, send, receive],
        edges: vec![
            edge("tone", "tone", "out", "fft", "in"),
            edge("fft", "fft", "out", "send", "in"),
        ],
    };
    let mut e = Engine::prepare(g, 48000.).unwrap();
    e.render(&[], &mut [[0.; 8]; 256]);
    let bins = e.nodes[1].spectral.as_ref().unwrap().bins.clone();
    render(&mut e);
    assert_eq!(e.nodes[3].spectral.as_ref().unwrap().bins, bins);
    let g = pr0_core::Graph {
        nodes: vec![
            node("s", "send_control", 0.),
            node("r", "receive_control", 0.),
        ],
        edges: vec![edge("feedback", "r", "out", "s", "in")],
    };
    let mut e = Engine::prepare(g, 48000.).unwrap();
    e.render(&[], &mut [[0.; 8]; 100]);
    assert_eq!(e.telemetry()["r"]["_out"], 0.);
}
#[test]
fn text_contract_checks_all_drivers_and_named_routes() {
    let mut text = node("text", "control_input", 0.);
    text.parameters.insert("mode".into(), 4.);
    text.control_value = Some(pr0_core::ControlValue::Text("hi".into()));
    let g = pr0_core::Graph {
        nodes: vec![
            node("number", "value", 0.),
            text,
            node("merge", "control_visualizer", 0.),
            node("numeric", "value", 0.),
        ],
        edges: vec![
            edge("first", "number", "out", "merge", "in"),
            edge("text", "text", "out", "merge", "in"),
            edge("numeric", "merge", "out", "numeric", "value"),
        ],
    };
    assert!(g.validate().unwrap_err().contains("String control"));
    let mut g = g;
    g.nodes.truncate(2);
    g.nodes.push(node("send", "send_control", 0.));
    g.nodes.push(node("receive", "receive_control", 0.));
    g.nodes.push(node("numeric", "value", 0.));
    g.edges = vec![
        edge("text", "text", "out", "send", "in"),
        edge("numeric", "receive", "out", "numeric", "value"),
    ];
    assert!(g.validate().unwrap_err().contains("String control"));
}

#[test]
fn bang_values_latch_toggle_including_repeated_zero_and_spatial_ties() {
    for (merged, routed) in [(false, false), (true, false), (true, true)] {
        let mut one = node("one", "value", 0.);
        one.parameters.insert("value".into(), 1.);
        let mut nodes = vec![
            node("bang", "trigger", 0.),
            one,
            node("toggle", "toggle", 100.),
        ];
        let mut edges = vec![
            edge("bang", "bang", "out", "one", "trigger"),
            edge("one", "one", "out", "toggle", "in"),
        ];
        if merged {
            nodes.extend([
                node("offbang", "trigger", 200.),
                node("zero", "value", 200.),
            ]);
            edges.extend([
                edge("offbang", "offbang", "out", "zero", "trigger"),
                edge("zero", "zero", "out", "toggle", "in"),
            ]);
        }
        if routed {
            for edge in &mut edges {
                if edge.target == "toggle" {
                    edge.target = "send".into();
                }
            }
            nodes.extend([
                node("send", "send_control", 0.),
                node("receive", "receive_control", 100.),
                node("view", "control_visualizer", 100.),
            ]);
            edges.extend([
                edge("route", "receive", "out", "view", "in"),
                edge("view", "view", "out", "toggle", "in"),
            ]);
        }
        let mut e = Engine::prepare(pr0_core::Graph { nodes, edges }, 48000.).unwrap();
        e.render(&[], &mut [[0.; 8]; 4]);
        for _ in 0..3 {
            assert!(e.bang("bang"));
            e.render(&[], &mut [[0.; 8]; 512]);
            assert_eq!(e.telemetry()["toggle"]["_checked"], 1.);
            if merged {
                assert!(e.bang("offbang"));
            } else {
                e.parameter("one", "value", 0.).unwrap();
                render(&mut e);
                assert_eq!(e.telemetry()["toggle"]["_checked"], 1.);
                assert!(e.bang("bang"));
            }
            e.render(&[], &mut [[0.; 8]; 512]);
            assert_eq!(e.telemetry()["toggle"]["_checked"], 0.);
            if !merged {
                e.parameter("one", "value", 1.).unwrap();
            }
        }
        // Manual overrides must be replaced by a fresh command, even with identical payload.
        e.control("toggle", &pr0_core::ControlValue::Number(1.));
        render(&mut e);
        if merged {
            assert!(e.bang("offbang"));
        } else {
            e.parameter("one", "value", 0.).unwrap();
            assert!(e.bang("bang"));
        }
        e.render(&[], &mut [[0.; 8]; 512]);
        assert_eq!(e.telemetry()["toggle"]["_checked"], 0.);
        if merged {
            assert!(e.bang("bang"));
            assert!(e.bang("offbang"));
            e.render(&[], &mut [[0.; 8]; 512]);
            assert_eq!(e.telemetry()["toggle"]["_checked"], 1.);
        }
    }
}

#[test]
fn toggle_accepts_text_and_positive_numbers_and_clears_nonpositive_numbers() {
    let mut source = node("source", "control_input", 0.);
    source.parameters.insert("mode".into(), 4.);
    source.control_value = Some(pr0_core::ControlValue::Text("hello".into()));
    let graph = pr0_core::Graph {
        nodes: vec![source, node("toggle", "toggle", 100.)],
        edges: vec![edge("in", "source", "out", "toggle", "in")],
    };
    let mut e = Engine::prepare(graph, 48000.).unwrap();
    render(&mut e);
    assert_eq!(e.telemetry()["toggle"]["_checked"], 1.);
    e.control("toggle", &pr0_core::ControlValue::Number(0.));
    render(&mut e);
    assert_eq!(e.telemetry()["toggle"]["_checked"], 0.);
    for (value, expected) in [
        (pr0_core::ControlValue::Text("".into()), 1.),
        (pr0_core::ControlValue::Number(-1.), 0.),
        (pr0_core::ControlValue::Number(0.5), 1.),
        (pr0_core::ControlValue::Number(0.), 0.),
        (pr0_core::ControlValue::Text("0".into()), 1.),
        (pr0_core::ControlValue::Number(-0.5), 0.),
    ] {
        e.control("source", &value);
        render(&mut e);
        assert_eq!(e.telemetry()["toggle"]["_checked"], expected);
    }
}

#[test]
fn toggle_emits_once_per_change_without_idle_zero_events_through_routes_and_merges() {
    let mut graph = pr0_core::Graph {
        nodes: vec![
            node("toggle", "toggle", 0.),
            node("other", "toggle", 200.),
            node("flash", "trigger", 0.),
            node("counter", "counter", 0.),
            node("mirror", "toggle", 0.),
            node("value", "value", 0.),
            node("send", "send_control", 0.),
            node("receive", "receive_control", 0.),
            node("view", "control_visualizer", 0.),
            node("remote", "toggle", 0.),
            node("remote_flash", "trigger", 0.),
        ],
        edges: vec![
            edge("flash", "toggle", "out", "flash", "in"),
            edge("count", "toggle", "out", "counter", "trigger"),
            edge("mirror", "toggle", "out", "mirror", "in"),
            edge("value", "toggle", "out", "value", "value"),
            edge("other", "other", "out", "value", "value"),
            edge("send", "toggle", "out", "send", "in"),
            edge("view", "receive", "out", "view", "in"),
            edge("remote", "view", "out", "remote", "in"),
            edge("remote_flash", "view", "out", "remote_flash", "in"),
        ],
    };
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == "value")
        .unwrap()
        .parameters
        .insert("value".into(), 42.);
    let mut e = Engine::prepare(graph, 48000.).unwrap();
    e.render(&[], &mut [[0.; 8]; 8]);
    assert_eq!(e.telemetry()["value"]["value"], 42.);
    for (value, count) in [(1., 1.), (0., 1.), (1., 2.), (0., 2.)] {
        e.control("toggle", &pr0_core::ControlValue::Number(value));
        render(&mut e);
        let n = e.nodes.iter().find(|n| n.id == "toggle").unwrap();
        assert!(n.control_event);
        assert_eq!(n.control[0], value);
        assert_eq!(e.telemetry()["flash"]["_out"], value);
        render(&mut e);
        assert_eq!(e.telemetry()["remote_flash"]["_out"], value);
        e.render(&[], &mut [[0.; 8]; 512]);
        assert_eq!(e.telemetry()["toggle"]["_checked"], value);
        assert_eq!(e.telemetry()["toggle"]["_out"], 0.);
        assert_eq!(e.telemetry()["mirror"]["_checked"], value);
        assert_eq!(e.telemetry()["remote"]["_checked"], value);
        assert_eq!(e.telemetry()["flash"]["_out"], 0.);
        assert_eq!(e.telemetry()["remote_flash"]["_out"], 0.);
        assert_eq!(e.telemetry()["counter"]["_out"], count);
        assert_eq!(e.telemetry()["value"]["value"], value);
        assert!(
            !e.nodes
                .iter()
                .find(|n| n.id == "toggle")
                .unwrap()
                .control_event
        );
        e.control("toggle", &pr0_core::ControlValue::Number(value));
        render(&mut e);
        assert!(
            !e.nodes
                .iter()
                .find(|n| n.id == "toggle")
                .unwrap()
                .control_event
        );
    }
}

#[test]
fn pitch_tracker_graph_infers_audio_width_and_validates_enabled_numeric_outputs() {
    let mut tone = node("tone", "oscillator", 0.);
    tone.channels = 8;
    tone.parameters = [("frequency".into(), 440.), ("amplitude".into(), 0.5)].into();
    let mut tracker = node("tracker", "pitch_tracker", 100.);
    tracker.channels = 1;
    let mut graph = pr0_core::Graph {
        nodes: vec![tone, tracker, node("note", "value", 200.)],
        edges: vec![
            edge("audio", "tone", "out", "tracker", "in"),
            edge("note", "tracker", "pitch1", "note", "value"),
        ],
    };
    let mut engine = Engine::prepare(graph.clone(), 48000.).unwrap();
    engine.render(&[], &mut [[0.; 8]; 8192]);
    assert_eq!(engine.telemetry()["tracker"]["pitch1"], 69.);
    assert_eq!(engine.telemetry()["note"]["value"], 69.);
    graph.edges[1].source_port = "pitch2".into();
    assert!(graph.validate().unwrap_err().contains("slot"));
    graph.nodes[1].parameters.insert("slots".into(), 4.);
    assert!(graph.validate().is_ok());
    let mut engine = Engine::prepare(graph.clone(), 48000.).unwrap();
    engine.render(&[], &mut [[0.; 8]; 8192]);
    assert_eq!(engine.telemetry()["note"]["value"], -1.);
    for (key, value) in [("slots", 1.5), ("slots", 5.), ("fft_size", 3000.)] {
        let mut invalid = graph.clone();
        invalid.nodes[1].parameters.insert(key.into(), value);
        assert!(invalid.validate().is_err());
    }
}
