use super::*;
use serde_json::json;
fn graph() -> Graph {
    serde_json::from_value(json!({"nodes":[
 {"id":"root","kind":"subgraph","label":"Root","x":0,"y":0,"channels":1},
 {"id":"v","kind":"value","parent":"root","label":"Value","x":0,"y":0,"channels":1,"parameters":{"value":0}},
 {"id":"s","kind":"smooth_change","parent":"root","label":"Smooth","x":200,"y":0,"channels":1,"parameters":{"curve":0,"time":1000}}
 ],"edges":[{"id":"vs","source":"v","source_port":"out","target":"s","target_port":"in"}]})).unwrap()
}
#[test]
fn recall_preserves_smoothing_mid_move_with_changed_options_and_republishes() {
    let g = graph();
    let mut e = Engine::prepare(g.clone(), 1000.).unwrap();
    e.render(&[], &mut [[0.; 8]]);
    e.parameter("v", "value", 10.).unwrap();
    e.render(&[], &mut [[0.; 8]; 300]);
    let before = e.telemetry()["s"]["_out"];
    assert!(before > 2. && before < 4.);
    let mut recalled = g.clone();
    recalled.nodes[1].parameters.insert("value".into(), 20.);
    recalled.nodes[2].parameters.insert("curve".into(), 3.);
    recalled.nodes[2].parameters.insert("time".into(), 2000.);
    let mut next = Engine::prepare(recalled, 1000.).unwrap();
    next.carry_recall_state(&mut e);
    next.republish_restored(&["v".into(), "s".into()]);
    next.render(&[], &mut [[0.; 8]]);
    let after = next.telemetry()["s"]["_out"];
    assert!((after - before).abs() < 0.1);
    assert!(
        next.nodes
            .iter()
            .find(|n| n.id == "v")
            .unwrap()
            .control_event
    );
    next.render(&[], &mut [[0.; 8]; 2000]);
    assert_eq!(next.telemetry()["s"]["_out"], 20.);
    let mut same = Engine::prepare(next.graph.clone(), 1000.).unwrap();
    same.carry_recall_state(&mut next);
    same.republish_restored(&["v".into()]);
    same.render(&[], &mut [[0.; 8]]);
    assert!(
        same.nodes
            .iter()
            .find(|n| n.id == "v")
            .unwrap()
            .control_event
    );
}
#[test]
fn state_input_consumes_changes_and_repeated_events_but_not_held_values() {
    let mut g = graph();
    g.nodes[1].parent = None;
    g.nodes[1].parameters.insert("value".into(), 1.);
    g.edges=vec![serde_json::from_value(json!({"id":"state","source":"v","source_port":"out","target":"root","target_port":"state"})).unwrap()];
    let mut e = Engine::prepare(g.clone(), 1000.).unwrap();
    e.render(&[], &mut [[0.; 8]; 10]);
    assert_eq!(
        e.take_state_requests(),
        vec![("root".into(), pr0_core::ControlValue::Number(1.))]
    );
    e.render(&[], &mut [[0.; 8]; 100]);
    assert!(e.take_state_requests().is_empty());
    let mut next = Engine::prepare(g, 1000.).unwrap();
    next.carry_node_state(&mut e);
    next.render(&[], &mut [[0.; 8]]);
    assert!(next.take_state_requests().is_empty());
    next.republish_restored(&["v".into()]);
    next.render(&[], &mut [[0.; 8]]);
    assert_eq!(next.take_state_requests().len(), 1);
    next.republish_restored(&["v".into()]);
    next.render(&[], &mut [[0.; 8]]);
    assert_eq!(next.take_state_requests().len(), 1);
}
#[test]
fn text_selectors_are_exact_and_connected_literals_keep_cable_authority() {
    let mut g = graph();
    g.nodes[1].kind = "control_input".into();
    g.nodes[1].parent = None;
    g.nodes[1].parameters = BTreeMap::from([("mode".into(), 4.)]);
    g.nodes[1].control_value = Some(pr0_core::ControlValue::Text("1".into()));
    g.edges=vec![serde_json::from_value(json!({"id":"state","source":"v","source_port":"out","target":"root","target_port":"state"})).unwrap()];
    let mut e = Engine::prepare(g, 1000.).unwrap();
    e.render(&[], &mut [[0.; 8]]);
    assert_eq!(
        e.take_state_requests(),
        vec![("root".into(), pr0_core::ControlValue::Text("1".into()))]
    );
    let mut g = graph();
    g.nodes[2].kind = "control_input".into();
    g.nodes[2].parameters = BTreeMap::from([("mode".into(), 1.)]);
    g.nodes[2].control_value = Some(pr0_core::ControlValue::Number(77.));
    g.nodes[1].parameters.insert("value".into(), 3.);
    let mut old = Engine::prepare(g.clone(), 1000.).unwrap();
    old.render(&[], &mut [[0.; 8]]);
    old.control("s", &pr0_core::ControlValue::Number(50.));
    old.render(&[], &mut [[0.; 8]]);
    let mut next = Engine::prepare(g.clone(), 1000.).unwrap();
    next.carry_recall_state(&mut old);
    next.republish_restored(&["s".into()]);
    next.render(&[], &mut [[0.; 8]]);
    assert_eq!(next.telemetry()["s"]["_out"], 3.);
    next.snapshot_options(&mut g);
    assert_eq!(
        g.nodes[2].control_value,
        Some(pr0_core::ControlValue::Number(77.))
    );
}

#[test]
fn literal_controller_positions_snapshot_and_republish_without_midi_attacks() {
    let mut g = graph();
    g.nodes[1].kind = "knobs".into();
    g.nodes[1].parameters = BTreeMap::from([("count".into(), 1.)]);
    g.edges.clear();
    let mut old = Engine::prepare(g.clone(), 1000.).unwrap();
    old.nodes
        .iter_mut()
        .find(|n| n.id == "v")
        .unwrap()
        .controllers
        .as_mut()
        .unwrap()
        .values[0] = 0.75;
    old.snapshot_options(&mut g);
    assert_eq!(g.nodes[1].control_positions, vec![Some(0.75)]);
    let mut recalled = Engine::prepare(g, 1000.).unwrap();
    recalled.carry_recall_state(&mut old);
    recalled.republish_restored(&["v".into()]);
    recalled.render(&[], &mut [[0.; 8]]);
    let n = recalled.nodes.iter().find(|n| n.id == "v").unwrap();
    assert_eq!(n.control[0], 0.75);
    assert!(n.control_event);
    assert_eq!(n.midi_frame.len, 0);
    recalled.render(&[], &mut [[0.; 8]]);
    assert!(
        !recalled
            .nodes
            .iter()
            .find(|n| n.id == "v")
            .unwrap()
            .control_event
    );
}
