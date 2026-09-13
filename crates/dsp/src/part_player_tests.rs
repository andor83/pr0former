use super::*;
use part_player::{Clip, Event};
use pr0_core::midi::Message;
fn graph() -> Graph {
    let template = pr0_core::demo_project("test".into(), "test".into(), pr0_core::Mode::Freeform)
        .graph
        .nodes[0]
        .clone();
    Graph {
        nodes: [
            ("play", "trigger"),
            ("repeat", "trigger"),
            ("stop", "trigger"),
            ("clip", "part_player"),
            ("sink", "midi_output"),
        ]
        .into_iter()
        .map(|(id, kind)| {
            let mut n = template.clone();
            n.id = id.into();
            n.label = id.into();
            n.kind = kind.into();
            n.parameters.clear();
            n.part_id = (id == "clip").then(|| "part".into());
            n
        })
        .collect(),
        edges: [
            ("play", "out", "clip", "play"),
            ("repeat", "out", "clip", "repeat"),
            ("stop", "out", "clip", "stop"),
            ("clip", "midi", "sink", "midi"),
        ]
        .into_iter()
        .enumerate()
        .map(
            |(i, (source, source_port, target, target_port))| pr0_core::Edge {
                id: i.to_string(),
                source: source.into(),
                source_port: source_port.into(),
                target: target.into(),
                target_port: target_port.into(),
            },
        )
        .collect(),
    }
}
fn message(velocity: u8) -> Message {
    Message {
        status: if velocity > 0 { 0x92 } else { 0x82 },
        data1: 60,
        data2: velocity,
    }
}
fn clip() -> Clip {
    Clip {
        length: 2.,
        events: vec![
            Event {
                beat: 0.,
                message: message(90),
            },
            Event {
                beat: 1.5,
                message: message(0),
            },
        ],
        meters: vec![pr0_core::score::MeterChange {
            beat: 0.,
            beats: 3,
            unit: 8,
        }],
        spans: vec![],
        automation: vec![],
    }
}
fn engine() -> Engine {
    let mut e = Engine::prepare(graph(), 1000.).unwrap();
    e.clock.bpm = 60.;
    e.set_part_player("clip", clip());
    e
}
fn tick(e: &mut Engine) -> Vec<Message> {
    e.render(&[], &mut [[0.; 8]]);
    let mut events = vec![];
    while let Some(m) = e.take_midi_message("sink") {
        events.push(m)
    }
    events
}
fn advance(e: &mut Engine, samples: usize) -> Vec<(usize, Message)> {
    let mut result = vec![];
    for sample in 0..samples {
        for message in tick(e) {
            result.push((sample, message));
        }
    }
    result
}
#[test]
fn part_player_quantizes_once_and_uses_standard_channel_midi_while_show_stopped() {
    let mut e = engine();
    e.graph_clock.beat = 0.2;
    e.set_meter(6, 8);
    e.bang("play");
    let events = advance(&mut e, 2400);
    assert_eq!(events, vec![(300, message(90)), (1800, message(0))]);
    assert!(!e.clock.running);
    assert_eq!(e.telemetry()["clip"]["_playing"], 0.);
    assert_eq!(e.telemetry()["clip"]["_pending"], 0.);
}
#[test]
fn part_player_retrigger_stop_priority_and_cancel_pending_release_notes() {
    let mut e = engine();
    e.bang("repeat");
    advance(&mut e, 1001);
    e.bang("play");
    assert!(tick(&mut e).is_empty());
    let restart = advance(&mut e, 999);
    assert_eq!(restart, vec![(998, message(0)), (998, message(90))]);
    e.bang("stop");
    assert_eq!(tick(&mut e), vec![message(0)]);
    e.bang("play");
    tick(&mut e);
    e.bang("stop");
    tick(&mut e);
    assert!(advance(&mut e, 2000).is_empty());
    e.bang("play");
    e.bang("stop");
    tick(&mut e);
    assert_eq!(e.telemetry()["clip"]["_pending"], 0.);
}
#[test]
fn part_player_loops_tempo_changes_and_show_rewinds_preserve_local_phase() {
    let mut e = engine();
    e.bang("repeat");
    advance(&mut e, 1500);
    let before = e.telemetry()["clip"]["_position"];
    assert!((before - 0.5).abs() < 1e-8);
    e.clock.set_tempo(120.);
    e.clock.stop();
    e.graph_clock.stop();
    e.graph_clock.running = true;
    let events = advance(&mut e, 751);
    assert_eq!(events, vec![(500, message(0)), (750, message(90))]);
    assert_eq!(e.telemetry()["clip"]["_repeating"], 1.);
    assert_eq!(e.telemetry()["clip"]["_bar"], 1.);
    assert_eq!(e.telemetry()["clip"]["_beat"], 1.);
}
#[test]
fn part_player_replacement_preserves_compatible_clips_and_releases_changed_or_deleted_sources() {
    let mut old = engine();
    old.bang("repeat");
    advance(&mut old, 1100);
    let mut same = engine();
    same.carry_node_state(&mut old);
    assert!((same.telemetry()["clip"]["_position"] - 0.1).abs() < 1e-8);
    assert!(tick(&mut same).is_empty());
    let mut changed = engine();
    let mut next = clip();
    next.events[0].message.data1 = 65;
    changed.set_part_player("clip", next);
    changed.carry_node_state(&mut same);
    assert_eq!(tick(&mut changed), vec![message(0)]);
    assert_eq!(changed.telemetry()["clip"]["_playing"], 0.);
    advance(&mut changed, 4);
    assert_eq!(changed.telemetry()["clip"]["gate"], 0.);
    changed.bang("repeat");
    advance(&mut changed, 1000);
    let mut g = graph();
    g.nodes.retain(|n| n.id != "clip");
    g.edges.clear();
    let mut removed = Engine::prepare(g, 1000.).unwrap();
    removed.carry_node_state(&mut changed);
    let off = tick(&mut removed);
    assert_eq!(off.len(), 1);
    assert_eq!(off[0].data1, 65);
    assert_eq!(off[0].data2, 0);
}

#[test]
fn part_player_periodic_beat_triggers_launch_instead_of_starving_and_held_values_do_not_retrigger()
{
    let mut e = engine();
    let mut ons = vec![];
    for sample in 0..3100 {
        if sample % 1000 == 0 {
            e.bang("play");
        }
        for event in tick(&mut e) {
            if event.data2 > 0 {
                ons.push(sample);
            }
        }
    }
    assert_eq!(ons, vec![1000, 2000, 3000]);
    let mut clip = part_player::Player::new(clip());
    let mut clock = Clock::new(1000.);
    clock.running = true;
    clock.bpm = 60.;
    let mut frame = midi_events::Buffer::new();
    let mut attacks = 0;
    for _ in 0..5000 {
        frame.clear();
        clip.tick(&clock, [1., 0., 0.], &mut frame, None);
        attacks += frame.events[..frame.len]
            .iter()
            .filter(|m| m.data2 > 0)
            .count();
        clock.advance();
    }
    assert_eq!(attacks, 1);
}
#[test]
fn part_player_launches_on_written_meter_boundaries_and_repeat_jumps() {
    let mut e = engine();
    e.graph_clock.beat = 20.25;
    e.set_part_player_metronome(Some((1.25, 1.25, 8)));
    e.bang("play");
    assert!(tick(&mut e).is_empty());
    e.set_part_player_metronome(Some((1.749, 1.25, 8)));
    assert!(tick(&mut e).is_empty());
    e.set_part_player_metronome(Some((1.75, 1.25, 8)));
    assert_eq!(tick(&mut e), vec![message(90)]);
    e.bang("play");
    tick(&mut e);
    e.set_part_player_metronome(Some((0., 0., 4)));
    assert_eq!(tick(&mut e), vec![message(0), message(90)]);
}
