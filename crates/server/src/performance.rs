//! Musical scheduling is driven by engine beats; external I/O runs on a separate worker.
use pr0_core::Project;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::mpsc::{SyncSender, sync_channel},
};

#[derive(Clone, PartialEq)]
pub struct Event {
    pub beat: f64,
    pub note_id: u32,
    pub pitch: u8,
    pub velocity: u8,
}
#[derive(Clone, PartialEq)]
struct NoteRoute {
    node: Option<String>,
    midi: Option<String>,
    channel: u8,
    staff: u8,
}
pub struct Lane {
    routes: Vec<NoteRoute>,
    automation: Vec<crate::score_automation::Automation>,
    pub score_spans: Vec<pr0_core::score::Span>,
    pub looping: bool,
    independent: bool,
    pub armed: bool,
    pub repeat_override: bool,
    count_in_start: Option<f64>,
    pending_repeat: bool,
    meter_map: Vec<MeterScale>,
    dynamic_override: Option<u8>,
    browser_notes: [bool; 128],
    performer: Option<String>,
    pub id: String,
    owner: u64,
    active: Vec<Option<(u8, u8)>>,
    suspended: Vec<Option<(u8, u8)>>,
    ends: Vec<f64>,
    pub node: Option<String>,
    pub events: Vec<Event>,
    pub next: usize,
    pub cycle: u64,
    pub start: f64,
    pub length: f64,
    pub playing: bool,
    pending: Option<(f64, bool)>,
    pub midi: Option<String>,
    pub midi_channel: u8,
    pub osc: Option<String>,
    pub address: String,
}
#[derive(serde::Serialize)]
pub struct PartPlayback {
    position_end: Option<f64>,
    id: String,
    playing: bool,
    start: f64,
    position: f64,
    pending: Option<(f64, bool)>,
    armed: bool,
    repeating: bool,
    count_in_remaining: Option<u8>,
    dynamic_override: Option<u8>,
    queue_position: Option<u8>,
    scheduled_start: Option<f64>,
}
#[derive(Clone, Copy, PartialEq)]
struct MeterScale {
    local_start: f64,
    engine_start: f64,
    scale: f64,
}
fn local_to_engine(map: &[MeterScale], beat: f64) -> f64 {
    let segment = map
        .iter()
        .rev()
        .find(|segment| segment.local_start <= beat + 1e-9)
        .unwrap_or(&map[0]);
    segment.engine_start + (beat - segment.local_start) * segment.scale
}
#[derive(Clone, Copy)]
struct QueuedPart {
    lane: usize,
    repeat: bool,
    at: f64,
}
struct PerformerQueue {
    performer: String,
    items: [Option<QueuedPart>; 32],
    len: usize,
}
impl PerformerQueue {
    fn push(&mut self, item: QueuedPart) {
        if self.items[..self.len]
            .iter()
            .flatten()
            .any(|queued| queued.lane == item.lane)
        {
            return;
        }
        if self.len < self.items.len() {
            self.items[self.len] = Some(item);
            self.len += 1;
        }
    }
    fn pop(&mut self) -> Option<QueuedPart> {
        let item = self.items[0].take();
        self.items.copy_within(1..self.len, 0);
        if self.len > 0 {
            self.len -= 1;
            self.items[self.len] = None;
        }
        item
    }
    fn peek(&self) -> Option<QueuedPart> {
        self.items.first().copied().flatten()
    }
    fn remove(&mut self, lane: usize) {
        let Some(index) = self.items[..self.len]
            .iter()
            .position(|item| item.is_some_and(|item| item.lane == lane))
        else {
            return;
        };
        self.items.copy_within(index + 1..self.len, index);
        self.len -= 1;
        self.items[self.len] = None;
    }
}
pub struct Sequencer {
    autoplay: bool,
    pub lanes: Vec<Lane>,
    pub last_running: bool,
    pub last_beat: f64,
    /// Written-position tempo map (beat, bpm), applied edge-triggered so manual
    /// tempo edits between entries are respected.
    tempos: Vec<(f64, f64)>,
    meters: Vec<(f64, u8, u8)>,
    last_written: Option<f64>,
    pulse_quarters: f64,
    queues: Vec<PerformerQueue>,
    recent_cues: VecDeque<String>,
}
pub enum External {
    Note {
        midi: Option<String>,
        midi_channel: u8,
        osc: Option<String>,
        address: String,
        pitch: u8,
        velocity: u8,
    },
    Message {
        midi: String,
        message: pr0_core::midi::Message,
    },
    Panic,
}
impl Sequencer {
    pub fn new(p: &Project) -> Self {
        let autoplay = matches!(p.mode, pr0_core::Mode::Structured);
        let mut prepared = p.clone();
        let solo = prepared.parts.iter().any(|p| p.solo);
        for part in &mut prepared.parts {
            if part.muted || (solo && !part.solo) {
                part.notes.clear();
                part.automation.clear();
                part.dynamics = None;
                for s in &mut part.staves {
                    s.dynamics = None;
                }
            }
            let original = part.notes.clone();
            // Per-staff dynamics take precedence over the legacy part-level dynamics.
            let staves = part.staves.clone();
            let part_dynamics = part.dynamics.clone();
            let dynamics_for = |n: &pr0_core::Note| -> Option<pr0_core::score::Dynamics> {
                n.notation
                    .as_ref()
                    .and_then(|v| staves.iter().find(|s| s.id == v.staff))
                    .and_then(|s| s.dynamics.clone())
                    .or_else(|| part_dynamics.clone())
            };
            let by_id: std::collections::BTreeMap<_, _> =
                original.iter().map(|n| (n.id.as_str(), n)).collect();
            let mut grace_totals = std::collections::BTreeMap::<&str, f64>::new();
            let mut grace_offsets = std::collections::BTreeMap::<&str, f64>::new();
            for g in &original {
                if let Some(target) = g.notation.as_ref().and_then(|v| v.grace_to.as_ref()) {
                    let total = grace_totals.entry(target.as_str()).or_default();
                    grace_offsets.insert(g.id.as_str(), *total);
                    *total += g.duration.min(0.25);
                }
            }

            let tied: std::collections::BTreeSet<_> = original
                .iter()
                .filter_map(|n| n.notation.as_ref().and_then(|v| v.tie_to.as_ref()))
                .collect();
            part.notes = original
                .iter()
                .filter(|n| !tied.contains(&n.id))
                .map(|n| {
                    let mut note = n.clone();
                    let mut current = n;
                    for _ in 0..original.len() {
                        let Some(target) = current
                            .notation
                            .as_ref()
                            .and_then(|v| v.tie_to.as_ref())
                            .and_then(|id| by_id.get(id.as_str()).copied())
                        else {
                            break;
                        };
                        note.duration += target.duration;
                        current = target;
                    }
                    if let Some(d) = dynamics_for(n) {
                        note.velocity = d.velocity(note.beat, note.velocity);
                    }
                    if let Some(v) = &n.notation {
                        match v.articulation.as_deref() {
                            Some("staccato") => note.duration *= 0.5,
                            Some("marcato") => {
                                note.duration *= 0.75;
                                note.velocity = (note.velocity as f64 * 1.3).min(127.) as u8
                            }
                            Some("accent") => {
                                note.velocity = (note.velocity as f64 * 1.2).min(127.) as u8
                            }
                            _ => {}
                        }
                        if let Some(target) = &v.grace_to {
                            let total = grace_totals.get(target.as_str()).copied().unwrap_or(0.25);
                            let principal =
                                by_id.get(target.as_str()).map(|n| n.duration).unwrap_or(1.);
                            let scale = (principal * 0.5 / total).min(1.);
                            note.beat +=
                                grace_offsets.get(n.id.as_str()).copied().unwrap_or(0.) * scale;
                            note.duration = note.duration.min(0.25) * scale;
                        }
                        if v.slur_to.is_some() {
                            note.duration *= 1.05;
                        }
                    }
                    let grace = grace_totals
                        .get(n.id.as_str())
                        .copied()
                        .unwrap_or(0.)
                        .min(note.duration * 0.5);
                    note.beat += grace;
                    note.duration -= grace;
                    note
                })
                .collect();
        }
        let timeline = p.score.as_ref();
        let spans = timeline.map(|s| s.spans()).unwrap_or_default();
        let mut part_spans = BTreeMap::new();
        if timeline.is_some() {
            for part in &mut prepared.parts {
                let mut elapsed = 0.;
                let local_spans: Vec<_> = spans
                    .iter()
                    .filter(|s| autoplay || s.start < part.loop_beats)
                    .map(|s| {
                        let end = if autoplay {
                            s.end
                        } else {
                            s.end.min(part.loop_beats)
                        };
                        let result = pr0_core::score::Span {
                            start: s.start,
                            end,
                            elapsed,
                        };
                        elapsed += end - s.start;
                        result
                    })
                    .collect();
                part.notes = local_spans
                    .iter()
                    .flat_map(|span| {
                        part.notes
                            .iter()
                            .filter(move |n| n.beat >= span.start && n.beat < span.end)
                            .map(move |n| {
                                let mut next = n.clone();
                                next.beat = span.elapsed + n.beat - span.start;
                                next.duration = n.duration.min(span.end - n.beat);
                                next
                            })
                    })
                    .collect();
                if !autoplay && !part.performance_meters.is_empty() {
                    let authored = part.performance_meters.clone();
                    let fallback = pr0_core::score::MeterChange {
                        beat: 0.,
                        beats: p.beats_per_bar,
                        unit: p.beat_unit,
                    };
                    let mut flattened = Vec::new();
                    for span in &local_spans {
                        let active = authored
                            .iter()
                            .filter(|meter| meter.beat <= span.start + 1e-9)
                            .last()
                            .unwrap_or(&fallback);
                        flattened.push(pr0_core::score::MeterChange {
                            beat: span.elapsed,
                            beats: active.beats,
                            unit: active.unit,
                        });
                        flattened.extend(
                            authored
                                .iter()
                                .filter(|meter| {
                                    meter.beat > span.start + 1e-9 && meter.beat < span.end - 1e-9
                                })
                                .map(|meter| pr0_core::score::MeterChange {
                                    beat: span.elapsed + meter.beat - span.start,
                                    beats: meter.beats,
                                    unit: meter.unit,
                                }),
                        );
                    }
                    part.performance_meters = flattened;
                }
                part.loop_beats = elapsed.max(0.25);
                part_spans.insert(part.id.clone(), local_spans);
            }
        }
        let tempos = p
            .score
            .as_ref()
            .map(|s| s.tempos.iter().map(|t| (t.beat, t.bpm)).collect())
            .unwrap_or_default();
        let mut meters = vec![(0., p.beats_per_bar as u8, p.beat_unit as u8)];
        if let Some(score) = &p.score {
            for m in &score.meters {
                if m.beat == 0. {
                    meters.clear();
                }
                meters.push((m.beat, m.beats, m.unit));
            }
        }
        Self {
            meters,
            autoplay,
            tempos,
            last_written: None,
            pulse_quarters: 4. / p.conducted.pulse_unit as f64,
            queues: p
                .parts
                .iter()
                .filter_map(|part| part.performer.as_ref())
                .fold(Vec::<PerformerQueue>::new(), |mut queues, performer| {
                    if !queues.iter().any(|queue| &queue.performer == performer) {
                        queues.push(PerformerQueue {
                            performer: performer.clone(),
                            items: [None; 32],
                            len: 0,
                        });
                    }
                    queues
                }),
            recent_cues: VecDeque::with_capacity(64),
            lanes: prepared
                .parts
                .iter()
                .enumerate()
                .map(|(owner, p)| {
                    let meter_map = if autoplay {
                        vec![MeterScale {
                            local_start: 0.,
                            engine_start: 0.,
                            scale: 1.,
                        }]
                    } else {
                        let mut changes = p.performance_meters.clone();
                        if changes.first().is_none_or(|meter| meter.beat != 0.) {
                            changes.insert(
                                0,
                                pr0_core::score::MeterChange {
                                    beat: 0.,
                                    beats: prepared.initial_meter().0,
                                    unit: prepared.initial_meter().1,
                                },
                            );
                        }
                        let mut engine_start = 0.;
                        changes
                            .iter()
                            .enumerate()
                            .map(|(index, meter)| {
                                let scale = (4. / prepared.conducted.pulse_unit as f64)
                                    / (4. / meter.unit as f64);
                                let segment = MeterScale {
                                    local_start: meter.beat,
                                    engine_start,
                                    scale,
                                };
                                if let Some(next) = changes.get(index + 1) {
                                    engine_start += (next.beat - meter.beat) * scale;
                                }
                                segment
                            })
                            .collect()
                    };
                    let mut events = Vec::new();
                    for (note_id, n) in p.notes.iter().enumerate() {
                        if n.rest || n.velocity == 0 || n.beat >= p.loop_beats {
                            continue;
                        }
                        events.push(Event {
                            note_id: note_id as u32,
                            beat: local_to_engine(&meter_map, n.beat),
                            pitch: n.pitch,
                            velocity: n.velocity,
                        });
                        events.push(Event {
                            note_id: note_id as u32,
                            beat: local_to_engine(
                                &meter_map,
                                (n.beat + n.duration).min(p.loop_beats),
                            ),
                            pitch: n.pitch,
                            velocity: 0,
                        });
                    }
                    events.sort_by(|a, b| {
                        a.beat.total_cmp(&b.beat).then(a.velocity.cmp(&b.velocity))
                    });
                    let ends = p
                        .notes
                        .iter()
                        .map(|n| {
                            local_to_engine(&meter_map, (n.beat + n.duration).min(p.loop_beats))
                        })
                        .collect();
                    let lane_length = local_to_engine(&meter_map, p.loop_beats);
                    Lane {
                        routes: p
                            .notes
                            .iter()
                            .map(|n| {
                                let staff = n
                                    .notation
                                    .as_ref()
                                    .and_then(|v| p.staves.iter().position(|s| s.id == v.staff));
                                let s = staff.and_then(|i| p.staves.get(i));
                                NoteRoute {
                                    node: s
                                        .and_then(|s| s.instrument_node.clone())
                                        .or_else(|| p.instrument_node.clone()),
                                    midi: s
                                        .and_then(|s| s.midi_port.clone())
                                        .or_else(|| p.midi_port.clone()),
                                    channel: s
                                        .and_then(|s| s.midi_channel)
                                        .unwrap_or(p.midi_channel),
                                    staff: staff.map(|i| i as u8 + 1).unwrap_or(1),
                                }
                            })
                            .collect(),
                        automation: p
                            .automation
                            .iter()
                            .cloned()
                            .map(crate::score_automation::Automation::new)
                            .chain(
                                p.dynamics
                                    .as_ref()
                                    .filter(|d| d.mode != pr0_core::score::DynamicsMode::Velocity)
                                    .map(|d| {
                                        crate::score_automation::Automation::generated(
                                            d.lane(p.midi_channel),
                                        )
                                    }),
                            )
                            .chain(p.staves.iter().filter_map(|s| {
                                s.dynamics
                                    .as_ref()
                                    .filter(|d| d.mode != pr0_core::score::DynamicsMode::Velocity)
                                    .map(|d| {
                                        let mut lane =
                                            d.lane(s.midi_channel.unwrap_or(p.midi_channel));
                                        lane.id = format!("score-dynamics-{}", s.id);
                                        crate::score_automation::Automation::generated(lane)
                                    })
                            }))
                            .collect(),
                        score_spans: part_spans.get(&p.id).cloned().unwrap_or_default(),
                        looping: autoplay && timeline.is_none_or(|s| s.loop_score),
                        independent: !autoplay,
                        armed: false,
                        repeat_override: false,
                        count_in_start: None,
                        pending_repeat: false,
                        meter_map,
                        dynamic_override: None,
                        browser_notes: [false; 128],
                        performer: p.performer.clone(),
                        id: p.id.clone(),
                        owner: owner as u64 + 1,
                        active: vec![None; p.notes.len()],
                        suspended: vec![None; p.notes.len()],
                        ends,
                        node: p.instrument_node.clone(),
                        events,
                        next: 0,
                        cycle: 0,
                        start: 0.,
                        length: lane_length,
                        playing: autoplay,
                        pending: None,
                        midi: p.midi_port.clone(),
                        midi_channel: p.midi_channel,
                        osc: p.osc_destination.clone(),
                        address: p.osc_address.clone(),
                    }
                })
                .collect(),
            last_running: false,
            last_beat: 0.,
        }
    }
    /// Transfer compatible lanes without replaying their event history.
    /// Preparation and this transfer happen on the orchestration worker.
    pub fn replace(
        mut self,
        p: &Project,
        previous: &mut pr0_dsp::Engine,
        prepared: &mut pr0_dsp::Engine,
        io: &SyncSender<External>,
    ) -> Self {
        let mut next = Self::new(p);
        let mut owner = self.lanes.iter().map(|l| l.owner).max().unwrap_or(0) + 1;
        for lane in &mut next.lanes {
            lane.owner = owner;
            owner += 1;
            if let Some(index) = self.lanes.iter().position(|old| {
                old.id == lane.id
                    && old.node == lane.node
                    && old.events == lane.events
                    && old.ends == lane.ends
                    && old.length == lane.length
                    && old.score_spans == lane.score_spans
                    && old.looping == lane.looping
                    && old.routes == lane.routes
                    && old
                        .automation
                        .iter()
                        .map(|a| &a.lane)
                        .eq(lane.automation.iter().map(|a| &a.lane))
                    && old.midi == lane.midi
                    && old.midi_channel == lane.midi_channel
                    && old.osc == lane.osc
                    && old.address == lane.address
            }) {
                let old = self.lanes.remove(index);
                let nodes: std::collections::BTreeSet<_> =
                    old.routes.iter().filter_map(|r| r.node.as_ref()).collect();
                for node in nodes {
                    prepared.carry_note_voices(previous, node, old.owner);
                }
                *lane = old;
            }
        }
        // Changed/deleted parts release only their own external notes. A global
        // panic here would also cut compatible parts that are still playing.
        for lane in &mut self.lanes {
            lane.release(previous, io);
        }
        next.last_running = self.last_running;
        next.last_beat = previous.clock.beat;
        next
    }
    /// Seed new/reassigned control nodes with the part's currently held notes.
    pub fn seed_part_nodes(&self, previous: &pr0_dsp::Engine, prepared: &mut pr0_dsp::Engine) {
        for (node, part) in prepared.part_sources() {
            let staff = prepared.part_source_staff(&node).unwrap_or(0);
            if previous.part_source(&node) == Some(part.as_str())
                && previous.part_source_staff(&node) == Some(staff)
            {
                continue;
            }
            if let Some(lane) = self.lanes.iter().find(|lane| lane.id == part) {
                for (index, active) in lane.active.iter().enumerate() {
                    if let Some((pitch, velocity)) = active {
                        let route = &lane.routes[index];
                        if staff == 0 || staff == route.staff {
                            prepared.node_midi_message(
                                &node,
                                pr0_core::midi::Message {
                                    status: 0x90 | (route.channel - 1),
                                    data1: *pitch,
                                    data2: *velocity,
                                },
                            );
                        }
                    }
                }
            }
        }
    }
    pub fn launch(
        &mut self,
        id: &str,
        playing: bool,
        engine: &mut pr0_dsp::Engine,
        io: &SyncSender<External>,
    ) {
        self.cue(&[id.to_string()], playing, false, 0, engine, io);
    }
    pub fn arm(&mut self, ids: &[String], armed: bool) {
        for lane in &mut self.lanes {
            if ids.contains(&lane.id) {
                lane.armed = armed;
            }
        }
    }
    pub fn dynamics(
        &mut self,
        ids: &[String],
        value: Option<u8>,
        engine: &mut pr0_dsp::Engine,
        io: &SyncSender<External>,
    ) {
        let use_group = ids.is_empty();
        let has_armed = self.lanes.iter().any(|lane| lane.armed);
        for lane in &mut self.lanes {
            if !(ids.contains(&lane.id)
                || (use_group && if has_armed { lane.armed } else { lane.playing }))
            {
                continue;
            }
            lane.dynamic_override = value;
            let cc = value.unwrap_or(127);
            let mut sent = std::collections::BTreeSet::new();
            for route in &lane.routes {
                if !sent.insert((route.staff, route.channel, route.midi.as_deref())) {
                    continue;
                }
                let message = pr0_core::midi::Message {
                    status: 0xb0 | (route.channel - 1),
                    data1: 11,
                    data2: cc,
                };
                engine.part_staff_message(&lane.id, route.staff, message);
                if let Some(midi) = &route.midi {
                    let _ = io.try_send(External::Message {
                        midi: midi.clone(),
                        message,
                    });
                }
            }
        }
    }
    pub fn browser_midi(
        &mut self,
        id: &str,
        mut message: pr0_core::midi::Message,
        engine: &mut pr0_dsp::Engine,
        io: &SyncSender<External>,
    ) {
        let Some(lane) = self.lanes.iter_mut().find(|lane| lane.id == id) else {
            return;
        };
        let kind = message.status & 0xf0;
        if kind == 0x90 && message.data2 == 0 {
            message.status = 0x80 | (message.status & 0x0f);
        }
        if message.status & 0xf0 == 0x90 {
            let velocity = lane.dynamic_override.unwrap_or(message.data2);
            message.data2 = velocity;
            lane.browser_notes[message.data1 as usize] = true;
            if let Some(node) = &lane.node {
                engine.note_scoped(
                    node,
                    lane.owner,
                    1_000_000 + message.data1 as u32,
                    message.data1,
                    velocity,
                );
            }
        } else if message.status & 0xf0 == 0x80 {
            lane.browser_notes[message.data1 as usize] = false;
            if let Some(node) = &lane.node {
                engine.note_scoped(
                    node,
                    lane.owner,
                    1_000_000 + message.data1 as u32,
                    message.data1,
                    0,
                );
            }
        }
        engine.part_message(&lane.id, message);
        if let Some(midi) = &lane.midi {
            let _ = io.try_send(External::Message {
                midi: midi.clone(),
                message,
            });
        }
    }
    pub fn browser_midi_panic(
        &mut self,
        ids: &[String],
        engine: &mut pr0_dsp::Engine,
        io: &SyncSender<External>,
    ) {
        for lane in &mut self.lanes {
            if !ids.contains(&lane.id) {
                continue;
            }
            for pitch in 0..128 {
                if !lane.browser_notes[pitch] {
                    continue;
                }
                lane.browser_notes[pitch] = false;
                if let Some(node) = &lane.node {
                    engine.note_scoped(node, lane.owner, 1_000_000 + pitch as u32, pitch as u8, 0);
                }
                engine.part_message(
                    &lane.id,
                    pr0_core::midi::Message {
                        status: 0x80 | (lane.midi_channel.saturating_sub(1)),
                        data1: pitch as u8,
                        data2: 0,
                    },
                );
            }
            if let Some(midi) = &lane.midi {
                let _ = io.try_send(External::Message {
                    midi: midi.clone(),
                    message: pr0_core::midi::Message {
                        status: 0xb0 | (lane.midi_channel.saturating_sub(1)),
                        data1: 123,
                        data2: 0,
                    },
                });
            }
        }
    }
    pub fn cue(
        &mut self,
        ids: &[String],
        playing: bool,
        repeat: bool,
        count_in_pulses: u8,
        engine: &mut pr0_dsp::Engine,
        io: &SyncSender<External>,
    ) {
        self.cue_request(None, ids, playing, repeat, count_in_pulses, engine, io)
    }
    pub fn cue_request(
        &mut self,
        request_id: Option<String>,
        ids: &[String],
        playing: bool,
        repeat: bool,
        count_in_pulses: u8,
        engine: &mut pr0_dsp::Engine,
        io: &SyncSender<External>,
    ) {
        if let Some(request_id) = request_id {
            if self.recent_cues.contains(&request_id) {
                return;
            }
            if self.recent_cues.len() == 64 {
                self.recent_cues.pop_front();
            }
            self.recent_cues.push_back(request_id);
        }
        let pulse = self.pulse_quarters;
        let boundary = if engine.clock.running {
            ((engine.clock.beat / pulse).floor() + 1.) * pulse
        } else {
            engine.clock.beat
        };
        let use_armed = ids.is_empty();
        for index in 0..self.lanes.len() {
            if !(ids.contains(&self.lanes[index].id) || (use_armed && self.lanes[index].armed)) {
                continue;
            }
            self.lanes[index].armed = false;
            if playing {
                if let Some(performer) = self.lanes[index].performer.clone() {
                    if let Some(active) = self.lanes.iter().position(|lane| {
                        lane.id != self.lanes[index].id
                            && lane.performer.as_deref() == Some(performer.as_str())
                            && (lane.playing || lane.pending.is_some_and(|pending| pending.1))
                    }) {
                        let active_lane = &self.lanes[active];
                        let active_end = if let Some((at, true)) = active_lane.pending {
                            at + active_lane.length
                        } else if active_lane.looping {
                            let cycles = ((boundary - active_lane.start) / active_lane.length)
                                .ceil()
                                .max(1.);
                            active_lane.start + cycles * active_lane.length
                        } else {
                            active_lane.start + active_lane.length
                        };
                        let queue_end = self
                            .queues
                            .iter()
                            .find(|queue| queue.performer == performer)
                            .and_then(|queue| queue.items[..queue.len].iter().flatten().last())
                            .map(|queued| queued.at + self.lanes[queued.lane].length)
                            .unwrap_or(active_end);
                        let scheduled = boundary.max(active_end).max(queue_end);
                        if self.lanes[active].repeat_override {
                            self.lanes[active].repeat_override = false;
                            self.lanes[active].pending = Some((scheduled, false));
                        }
                        if let Some(queue) =
                            self.queues.iter_mut().find(|q| q.performer == performer)
                        {
                            queue.push(QueuedPart {
                                lane: index,
                                repeat,
                                at: scheduled,
                            });
                        }
                        continue;
                    }
                }
            }
            if !playing {
                for queue in &mut self.queues {
                    queue.remove(index);
                }
            }
            let lane = &mut self.lanes[index];
            if engine.clock.running {
                let start = boundary
                    + if playing {
                        count_in_pulses as f64 * pulse
                    } else {
                        0.
                    };
                lane.pending = Some((start, playing));
                lane.pending_repeat = repeat;
                lane.count_in_start = (playing && count_in_pulses > 0).then_some(boundary);
            } else {
                lane.pending_repeat = repeat;
                lane.apply(playing, engine.clock.beat, engine, io);
            }
        }
    }
    /// Written score position of the shared traversal for the current clock beat.
    pub fn written_position(&self, beat: f64) -> f64 {
        let Some(lane) = self.lanes.first() else {
            return beat;
        };
        if lane.score_spans.is_empty() {
            return beat;
        }
        let elapsed = lane.engine_to_local(if lane.looping {
            (beat - lane.start).rem_euclid(lane.length)
        } else {
            (beat - lane.start).min(lane.length)
        });
        lane.score_spans
            .iter()
            .find(|s| elapsed < s.elapsed + s.end - s.start)
            .map(|s| s.start + elapsed - s.elapsed)
            .unwrap_or(elapsed)
    }
    /// Written position plus the active meter's bar origin, prepared off-render.
    pub fn metronome_position(&self, beat: f64) -> (f64, f64, u8, u8) {
        let written = self.written_position(beat);
        let i = self
            .meters
            .partition_point(|m| m.0 <= written + 1e-9)
            .saturating_sub(1);
        let (origin, beats, unit) = self.meters[i];
        (written, origin, beats, unit)
    }
    /// Edge-triggered tempo map: on start/resume/rewind apply the tempo in force,
    /// then apply each entry as the written position crosses it.
    fn apply_tempo_map(&mut self, beat: f64, restart: bool, engine: &mut pr0_dsp::Engine) {
        if self.tempos.is_empty() {
            return;
        }
        let written = self.written_position(beat);
        let previous = if restart { None } else { self.last_written };
        self.last_written = Some(written);
        let target = match previous {
            None => self
                .tempos
                .iter()
                .filter(|t| t.0 <= written + 1e-9)
                .last()
                .map(|t| t.1),
            Some(last) if written < last => self
                .tempos
                .iter()
                .filter(|t| t.0 <= written + 1e-9)
                .last()
                .map(|t| t.1),
            Some(last) => self
                .tempos
                .iter()
                .filter(|t| t.0 > last + 1e-9 && t.0 <= written + 1e-9)
                .last()
                .map(|t| t.1),
        };
        if let Some(bpm) = target {
            if (engine.clock.bpm - bpm).abs() > 1e-9 {
                engine.clock.set_tempo(bpm);
            }
        }
    }
    pub fn reset(&mut self, engine: &mut pr0_dsp::Engine, io: &SyncSender<External>) {
        let browser_parts: Vec<_> = self
            .lanes
            .iter()
            .filter(|lane| lane.browser_notes.iter().any(|held| *held))
            .map(|lane| lane.id.clone())
            .collect();
        self.browser_midi_panic(&browser_parts, engine, io);
        for queue in &mut self.queues {
            queue.items.fill(None);
            queue.len = 0;
        }
        for lane in &mut self.lanes {
            lane.release(engine, io);
            lane.suspended.fill(None);
            lane.next = 0;
            lane.cycle = 0;
            lane.start = 0.;
            lane.playing = self.autoplay;
            lane.pending = None;
            lane.armed = false;
            lane.repeat_override = false;
            lane.count_in_start = None;
            lane.pending_repeat = false;
        }
        self.last_beat = 0.;
        self.last_running = false;
    }
    pub fn pause(&mut self, engine: &mut pr0_dsp::Engine, io: &SyncSender<External>) {
        if self.last_running {
            for lane in &mut self.lanes {
                lane.suspended.copy_from_slice(&lane.active);
                lane.release(engine, io);
            }
        }
        self.last_running = false;
    }
    pub fn playback(&self, beat: f64) -> Vec<PartPlayback> {
        self.lanes
            .iter()
            .enumerate()
            .map(|(lane_index, l)| PartPlayback {
                position_end: {
                    let elapsed = l.engine_to_local(if l.looping {
                        (beat - l.start).max(0.).rem_euclid(l.length)
                    } else {
                        (beat - l.start).max(0.).min(l.length)
                    });
                    l.score_spans
                        .iter()
                        .find(|s| elapsed < s.elapsed + s.end - s.start)
                        .map(|s| s.end)
                        .or_else(|| l.score_spans.last().map(|s| s.end))
                },
                id: l.id.clone(),
                playing: l.playing,
                start: l.start,
                position: if l.playing {
                    let elapsed = l.engine_to_local(if l.looping {
                        (beat - l.start).max(0.).rem_euclid(l.length)
                    } else {
                        (beat - l.start).max(0.).min(l.length)
                    });
                    l.score_spans
                        .iter()
                        .find(|s| elapsed < s.elapsed + s.end - s.start)
                        .map(|s| s.start + elapsed - s.elapsed)
                        .unwrap_or_else(|| l.score_spans.last().map(|s| s.end).unwrap_or(elapsed))
                } else {
                    0.
                },
                pending: l.pending,
                armed: l.armed,
                repeating: l.repeat_override,
                count_in_remaining: l.pending.and_then(|(at, playing)| {
                    let start = l.count_in_start?;
                    (playing && beat >= start && beat < at)
                        .then(|| ((at - beat) / self.pulse_quarters).ceil().max(1.) as u8)
                }),
                dynamic_override: l.dynamic_override,
                queue_position: self.queues.iter().find_map(|queue| {
                    queue.items[..queue.len]
                        .iter()
                        .position(|item| item.is_some_and(|item| item.lane == lane_index))
                        .map(|index| index as u8 + 1)
                }),
                scheduled_start: self.queues.iter().find_map(|queue| {
                    queue.items[..queue.len]
                        .iter()
                        .flatten()
                        .find(|item| item.lane == lane_index)
                        .map(|item| item.at)
                }),
            })
            .collect()
    }
    pub fn cue_count_in(&self, beat: f64) -> bool {
        self.lanes.iter().any(|lane| {
            lane.count_in_start.is_some_and(|start| {
                lane.pending
                    .is_some_and(|(at, playing)| playing && beat >= start && beat < at)
            })
        })
    }
    pub fn cue_count_in_for_part(&self, beat: f64, part: &str) -> bool {
        let performer = self
            .lanes
            .iter()
            .find(|lane| lane.id == part)
            .and_then(|lane| lane.performer.as_deref());
        self.lanes.iter().any(|lane| {
            (lane.id == part || performer.is_some() && lane.performer.as_deref() == performer)
                && lane.count_in_start.is_some_and(|start| {
                    lane.pending
                        .is_some_and(|(at, playing)| playing && beat >= start && beat < at)
                })
        })
    }
    pub fn tick(&mut self, engine: &mut pr0_dsp::Engine, io: &SyncSender<External>) {
        let mut beat = engine.clock.beat;
        let running = engine.clock.running;
        if running
            && !self.last_running
            && self.autoplay
            && self
                .lanes
                .first()
                .is_some_and(|l| !l.looping && beat >= l.length)
        {
            engine.clock.beat = 0.;
            self.reset(engine, io);
            beat = 0.;
        }
        if self.last_running && !running {
            self.pause(engine, io);
        }
        if beat < self.last_beat {
            for l in &mut self.lanes {
                l.release(engine, io);
                l.suspended.fill(None);
                l.next = 0;
                l.cycle = 0;
                l.start = 0.;
            }
        }
        let resuming = running && !self.last_running;
        let rewound = beat < self.last_beat;
        self.last_beat = beat;
        self.last_running = running;
        if !running {
            self.last_written = None;
            return;
        }
        self.apply_tempo_map(beat, resuming || rewound, engine);
        if self.autoplay
            && self
                .lanes
                .first()
                .is_some_and(|l| !l.looping && beat >= l.length)
        {
            for lane in &mut self.lanes {
                let position = lane
                    .score_spans
                    .last()
                    .map(|s| s.end)
                    .unwrap_or(lane.length);
                lane.automation_tick(position, engine);
                lane.release(engine, io);
                lane.suspended.fill(None);
            }
            engine.clock.running = false;
            self.last_running = false;
            return;
        }
        for lane in &mut self.lanes {
            if let Some((boundary, playing)) = lane.pending {
                if beat + 1e-9 >= boundary {
                    lane.apply(playing, boundary, engine, io);
                }
            }
            if lane.playing && !lane.looping && beat + 1e-9 >= lane.start + lane.length {
                lane.release(engine, io);
                lane.playing = false;
                lane.repeat_override = false;
                lane.next = 0;
                lane.cycle = 0;
            }
        }
        for queue in &mut self.queues {
            let busy = self.lanes.iter().any(|lane| {
                lane.performer.as_deref() == Some(queue.performer.as_str())
                    && (lane.playing || lane.pending.is_some_and(|pending| pending.1))
            });
            if !busy && queue.peek().is_some_and(|next| beat + 1e-9 >= next.at) {
                if let Some(next) = queue.pop() {
                    let lane = &mut self.lanes[next.lane];
                    lane.pending_repeat = next.repeat;
                    lane.apply(true, next.at, engine, io);
                }
            }
        }
        for lane in &mut self.lanes {
            if lane.playing && beat >= lane.start {
                let elapsed = lane.engine_to_local(if lane.looping {
                    (beat - lane.start).rem_euclid(lane.length)
                } else {
                    (beat - lane.start).min(lane.length)
                });
                let position = lane
                    .score_spans
                    .iter()
                    .find(|s| elapsed < s.elapsed + s.end - s.start)
                    .map(|s| s.start + elapsed - s.elapsed)
                    .unwrap_or(elapsed);
                lane.automation_tick(position, engine);
            }
            if !lane.playing || beat < lane.start || lane.events.is_empty() {
                continue;
            }
            if resuming {
                lane.resume(beat, engine, io);
            }
            // A bounded number of simultaneous events per sample prevents runaway patches.
            for _ in 0..256 {
                if lane.next >= lane.events.len() {
                    if !lane.looping {
                        break;
                    }
                    lane.next = 0;
                    lane.cycle += 1;
                }
                let e = &lane.events[lane.next];
                let due = lane.start + lane.cycle as f64 * lane.length + e.beat;
                if due > beat + 1e-9 {
                    break;
                }
                // Pause/stop may already have released this note. Do not emit a
                // second off, which could release another overlapping MIDI note.
                if e.velocity == 0 && lane.active[e.note_id as usize].is_none() {
                    lane.next += 1;
                    continue;
                }
                let route = &lane.routes[e.note_id as usize];
                let velocity = if e.velocity > 0 {
                    lane.dynamic_override.unwrap_or(e.velocity)
                } else {
                    0
                };
                if let Some(node) = &route.node {
                    engine.note_scoped(node, lane.owner, e.note_id, e.pitch, velocity);
                }
                engine.part_staff_message(
                    &lane.id,
                    route.staff,
                    pr0_core::midi::Message {
                        status: (if velocity == 0 { 0x80 } else { 0x90 }) | (route.channel - 1),
                        data1: e.pitch,
                        data2: velocity,
                    },
                );
                lane.active[e.note_id as usize] = if velocity > 0 {
                    Some((e.pitch, velocity))
                } else {
                    None
                };
                if route.midi.is_some() || lane.osc.is_some() {
                    let _ = io.try_send(External::Note {
                        midi: route.midi.clone(),
                        midi_channel: route.channel,
                        osc: lane.osc.clone(),
                        address: lane.address.clone(),
                        pitch: e.pitch,
                        velocity,
                    });
                }
                lane.next += 1;
            }
        }
    }
}
impl Lane {
    fn engine_to_local(&self, elapsed: f64) -> f64 {
        let segment = self
            .meter_map
            .iter()
            .rev()
            .find(|segment| segment.engine_start <= elapsed + 1e-9)
            .unwrap_or(&self.meter_map[0]);
        segment.local_start + (elapsed - segment.engine_start) / segment.scale
    }

    fn automation_tick(&mut self, position: f64, engine: &mut pr0_dsp::Engine) {
        for index in 0..self.automation.len() {
            let a = &self.automation[index];
            let overridden = self.automation.iter().any(|other| {
                other.generated != a.generated
                    && other.lane.message == a.lane.message
                    && other.lane.channel == a.lane.channel
                    && other.lane.number == a.lane.number
                    && if a.generated {
                        other
                            .lane
                            .events
                            .iter()
                            .any(|e| position >= e.beat && position <= e.beat + e.duration)
                    } else {
                        !a.lane
                            .events
                            .iter()
                            .any(|e| position >= e.beat && position <= e.beat + e.duration)
                    }
            });
            let automation = &mut self.automation[index];
            if overridden {
                automation.reset();
                continue;
            }
            for message in automation
                .tick(position, engine.clock.sample, engine.clock.sample_rate)
                .into_iter()
                .flatten()
            {
                engine.part_message(&self.id, message);
            }
        }
    }
    fn apply(
        &mut self,
        playing: bool,
        start: f64,
        engine: &mut pr0_dsp::Engine,
        io: &SyncSender<External>,
    ) {
        self.release(engine, io);
        self.suspended.fill(None);
        self.playing = playing;
        if self.independent {
            self.looping = playing && self.pending_repeat;
            self.repeat_override = self.looping;
        }
        self.start = start;
        self.next = 0;
        self.cycle = 0;
        self.pending = None;
        self.pending_repeat = false;
        self.count_in_start = None;
    }

    fn resume(&mut self, beat: f64, engine: &mut pr0_dsp::Engine, io: &SyncSender<External>) {
        for (note_id, held) in self.suspended.iter_mut().enumerate() {
            if let Some((pitch, velocity)) = held.take() {
                let end = self.start + self.cycle as f64 * self.length + self.ends[note_id];
                if end <= beat + 1e-9 {
                    continue;
                }
                let route = &self.routes[note_id];
                if let Some(node) = &route.node {
                    engine.note_scoped(node, self.owner, note_id as u32, pitch, velocity);
                }
                engine.part_staff_message(
                    &self.id,
                    route.staff,
                    pr0_core::midi::Message {
                        status: 0x90 | (route.channel - 1),
                        data1: pitch,
                        data2: velocity,
                    },
                );
                self.active[note_id] = Some((pitch, velocity));
                if route.midi.is_some() || self.osc.is_some() {
                    let _ = io.try_send(External::Note {
                        midi: route.midi.clone(),
                        midi_channel: route.channel,
                        osc: self.osc.clone(),
                        address: self.address.clone(),
                        pitch,
                        velocity,
                    });
                }
            }
        }
    }

    fn release(&mut self, engine: &mut pr0_dsp::Engine, io: &SyncSender<External>) {
        engine.part_notes_off(&self.id);
        for automation in &mut self.automation {
            if let Some(off) = automation.reset() {
                engine.part_message(&self.id, off);
            }
        }
        for (note_id, active) in self.active.iter_mut().enumerate() {
            if let Some((pitch, _)) = active.take() {
                let route = &self.routes[note_id];
                engine.part_staff_message(
                    &self.id,
                    route.staff,
                    pr0_core::midi::Message {
                        status: 0x80 | (route.channel - 1),
                        data1: pitch,
                        data2: 0,
                    },
                );
                if let Some(node) = &route.node {
                    engine.note_scoped(node, self.owner, note_id as u32, pitch, 0);
                }
                if route.midi.is_some() || self.osc.is_some() {
                    let _ = io.try_send(External::Note {
                        midi: route.midi.clone(),
                        midi_channel: route.channel,
                        osc: self.osc.clone(),
                        address: self.address.clone(),
                        pitch,
                        velocity: 0,
                    });
                }
            }
        }
    }
}
// MIDI 1.0 and the pitch/velocity OSC contract cannot identify independent
// voices. Hold a shared pitch until its last scheduled owner releases it.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Route {
    Midi(String, u8),
    Osc(std::net::SocketAddr, String),
}
#[derive(Default)]
struct HeldNotes(BTreeMap<(Route, u8), u32>);
impl HeldNotes {
    fn update(&mut self, route: Route, pitch: u8, velocity: u8) -> bool {
        let key = (route, pitch);
        if velocity > 0 {
            let count = self.0.entry(key).or_default();
            *count += 1;
            *count == 1
        } else if let Some(count) = self.0.get_mut(&key) {
            *count -= 1;
            if *count == 0 {
                self.0.remove(&key);
                true
            } else {
                false
            }
        } else {
            false
        }
    }
}
fn send_osc(
    socket: &crate::osc::Runtime,
    destination: std::net::SocketAddr,
    address: &str,
    pitch: u8,
    velocity: u8,
) {
    {
        let packet = rosc::OscPacket::Message(rosc::OscMessage {
            addr: address.into(),
            args: vec![
                rosc::OscType::Int(pitch as i32),
                rosc::OscType::Int(velocity as i32),
            ],
        });
        if let Ok(bytes) = rosc::encoder::encode(&packet) {
            socket.send(&bytes, destination);
        }
    }
}
pub fn external_worker(socket: std::sync::Arc<crate::osc::Runtime>) -> SyncSender<External> {
    let (tx, rx) = sync_channel(4096);
    std::thread::Builder::new()
        .name("pr0-midi-osc".into())
        .spawn(move || {
            let mut midi: BTreeMap<String, midir::MidiOutputConnection> = BTreeMap::new();
            let mut held = HeldNotes::default();
            while let Ok(event) = rx.recv() {
                match event {
                    External::Note {
                        midi: port,
                        midi_channel,
                        osc,
                        address,
                        pitch,
                        velocity,
                    } => {
                        if let Some(port) = port.filter(|port| {
                            held.update(Route::Midi(port.clone(), midi_channel), pitch, velocity)
                        }) {
                            if !midi.contains_key(&port) {
                                if let Ok(output) = midir::MidiOutput::new("pr0former") {
                                    if let Some(p) = output.ports().iter().find(|p| {
                                        output.port_name(p).ok().as_deref() == Some(&port)
                                    }) {
                                        if let Ok(c) = output.connect(p, "pr0former performance") {
                                            midi.insert(port.clone(), c);
                                        }
                                    }
                                }
                            }
                            if let Some(c) = midi.get_mut(&port) {
                                let _ = c.send(&[
                                    (if velocity == 0 { 0x80 } else { 0x90 }) | (midi_channel - 1),
                                    pitch,
                                    velocity,
                                ]);
                            }
                        }
                        if let Some(destination) =
                            osc.and_then(|d| d.parse::<std::net::SocketAddr>().ok())
                        {
                            if held.update(
                                Route::Osc(destination, address.clone()),
                                pitch,
                                velocity,
                            ) {
                                send_osc(&socket, destination, &address, pitch, velocity);
                            }
                        }
                    }
                    External::Message {
                        midi: port,
                        message,
                    } => {
                        if !midi.contains_key(&port) {
                            if let Ok(output) = midir::MidiOutput::new("pr0former") {
                                if let Some(p) = output
                                    .ports()
                                    .iter()
                                    .find(|p| output.port_name(p).ok().as_deref() == Some(&port))
                                {
                                    if let Ok(c) = output.connect(p, "pr0former performance") {
                                        midi.insert(port.clone(), c);
                                    }
                                }
                            }
                        }
                        if let Some(c) = midi.get_mut(&port) {
                            let _ = c.send(&[message.status, message.data1, message.data2]);
                        }
                    }
                    External::Panic => {
                        for ((route, pitch), _) in &held.0 {
                            if let Route::Osc(destination, address) = route {
                                send_osc(&socket, *destination, address, *pitch, 0);
                            }
                        }
                        held.0.clear();
                        for c in midi.values_mut() {
                            for channel in 0..16 {
                                let _ = c.send(&[0xb0 | channel, 123, 0]);
                            }
                        }
                    }
                }
            }
        })
        .expect("Start MIDI/OSC worker");
    tx
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn part_control_streams_isolate_chords_and_reassignment_releases_old_pitches() {
        let mut p = pr0_core::demo_project("x".into(), "x".into(), pr0_core::Mode::Structured);
        p.parts[0].notes.truncate(2);
        p.parts[0].notes[0].pitch = 60;
        p.parts[0].notes[0].duration = 2.;
        p.parts[0].notes[1].pitch = 64;
        p.parts[0].notes[1].beat = 0.;
        p.parts[0].notes[1].duration = 2.;
        let mut other = p.parts[0].clone();
        other.id = "other".into();
        other.notes.truncate(1);
        other.notes[0].pitch = 72;
        p.parts.push(other);
        let mut node = p.graph.nodes[0].clone();
        node.id = "notes".into();
        node.kind = "part_midi".into();
        node.parameters.clear();
        node.part_id = Some(p.parts[0].id.clone());
        p.graph.nodes.push(node);
        let mut e = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        e.clock.running = true;
        let mut seq = Sequencer::new(&p);
        let (tx, _) = sync_channel(256);
        let mut events = vec![];
        for _ in 0..4 {
            seq.tick(&mut e, &tx);
            e.render(&[], &mut [[0.; 8]]);
            let v = e.telemetry().remove("notes").unwrap();
            if v["trigger"] > 0. {
                events.push(v["pitch"]);
            }
        }
        assert_eq!(events, vec![60., 64.]);
        p.graph.nodes.last_mut().unwrap().part_id = Some("other".into());
        let mut prepared = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        prepared.clock = e.clock;
        let mut next = seq.replace(&p, &mut e, &mut prepared, &tx);
        prepared.carry_node_state(&mut e);
        next.seed_part_nodes(&e, &mut prepared);
        let mut changes = vec![];
        for _ in 0..8 {
            next.tick(&mut prepared, &tx);
            prepared.render(&[], &mut [[0.; 8]]);
            let v = prepared.telemetry().remove("notes").unwrap();
            if v["trigger"] + v["note_off"] > 0. {
                changes.push((v["pitch"], v["velocity"]));
            }
        }
        assert_eq!(changes, vec![(60., 0.), (64., 0.), (72., 90.)]);
        next.pause(&mut prepared, &tx);
        prepared.clock.running = false;
        for _ in 0..4 {
            prepared.render(&[], &mut [[0.; 8]]);
        }
        assert_eq!(prepared.telemetry()["notes"]["gate"], 0.);
    }
    #[test]
    fn external_midi_channels_hold_the_same_pitch_independently() {
        let mut held = HeldNotes::default();
        let one = Route::Midi("port".into(), 1);
        let two = Route::Midi("port".into(), 2);
        assert!(held.update(one.clone(), 60, 90));
        assert!(held.update(two.clone(), 60, 90));
        assert!(held.update(one, 60, 0));
        assert_eq!(held.0.len(), 1);
        assert!(held.update(two, 60, 0));
        assert!(held.0.is_empty());
    }
    #[test]
    fn graph_replacement_preserves_lanes_cues_and_note_offs() {
        let mut p = pr0_core::demo_project("test".into(), "Test".into(), pr0_core::Mode::Freeform);
        p.parts[0].notes.truncate(1);
        p.parts[0].midi_port = Some("test sink".into());
        let mut removed = p.parts[0].clone();
        removed.id = "removed".into();
        removed.notes[0].pitch = 67;
        p.parts.push(removed);
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        engine.clock.beat = 40.;
        let mut seq = Sequencer::new(&p);
        let (tx, rx) = sync_channel(64);
        for part in &p.parts {
            seq.launch(&part.id, true, &mut engine, &tx);
        }
        engine.clock.running = true;
        seq.tick(&mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 2);
        engine.clock.beat = 40.25;
        seq.launch(&p.parts[0].id, false, &mut engine, &tx);
        p.parts.pop();
        let mut added = p.parts[0].clone();
        added.id = "new".into();
        p.parts.insert(0, added);
        let mut prepared = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        prepared.clock = engine.clock;
        let mut next = seq.replace(&p, &mut engine, &mut prepared, &tx);
        assert!(matches!(
            rx.try_recv().unwrap(),
            External::Note {
                pitch: 67,
                velocity: 0,
                ..
            }
        ));
        assert!(!next.lanes[0].playing);
        assert_ne!(next.lanes[0].owner, next.lanes[1].owner);
        assert_eq!(next.playback(40.25)[1].position, 0.25);
        assert_eq!(next.playback(40.25)[1].pending, Some((41., false)));
        next.tick(&mut prepared, &tx);
        assert_eq!(
            rx.try_iter().count(),
            0,
            "no old events replayed on replacement"
        );
        prepared.clock.beat = 40.75;
        next.tick(&mut prepared, &tx);
        assert!(matches!(
            rx.try_recv().unwrap(),
            External::Note {
                pitch: 60,
                velocity: 0,
                ..
            }
        ));
        prepared.clock.beat = 41.;
        next.tick(&mut prepared, &tx);
        assert!(!next.lanes[1].playing);
        assert_eq!(rx.try_iter().count(), 0);
    }

    #[test]
    fn resume_restores_held_notes_once_and_preserves_their_release_time() {
        let mut p =
            pr0_core::demo_project("test".into(), "Test".into(), pr0_core::Mode::Structured);
        p.parts[0].midi_port = Some("test sink".into());
        p.parts[0].notes.truncate(1);
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        let mut seq = Sequencer::new(&p);
        let (tx, rx) = sync_channel(64);
        engine.clock.running = true;
        seq.tick(&mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 1);
        engine.clock.beat = 0.25;
        // The command path must work even if pause/play arrive in one block.
        seq.pause(&mut engine, &tx);
        assert!(matches!(
            rx.try_recv().unwrap(),
            External::Note { velocity: 0, .. }
        ));
        seq.pause(&mut engine, &tx);
        seq.tick(&mut engine, &tx);
        assert!(matches!(
            rx.try_recv().unwrap(),
            External::Note { velocity: 90, .. }
        ));
        seq.tick(&mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 0);
        engine.clock.beat = 0.75;
        seq.tick(&mut engine, &tx);
        assert!(matches!(
            rx.try_recv().unwrap(),
            External::Note { velocity: 0, .. }
        ));
        seq.pause(&mut engine, &tx);
        seq.tick(&mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 0, "expired notes must not restart");
    }

    #[test]
    fn paused_part_stop_cancels_suspended_notes_and_pending_cues() {
        let mut p = pr0_core::demo_project("test".into(), "Test".into(), pr0_core::Mode::Freeform);
        p.parts[0].midi_port = Some("test sink".into());
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        let mut seq = Sequencer::new(&p);
        let (tx, rx) = sync_channel(64);
        seq.launch(&p.parts[0].id, true, &mut engine, &tx);
        engine.clock.running = true;
        seq.tick(&mut engine, &tx);
        engine.clock.beat = 0.25;
        seq.launch(&p.parts[0].id, true, &mut engine, &tx);
        seq.pause(&mut engine, &tx);
        engine.clock.running = false;
        seq.tick(&mut engine, &tx);
        assert_eq!(seq.playback(0.25)[0].pending, Some((1., true)));
        seq.launch(&p.parts[0].id, false, &mut engine, &tx);
        rx.try_iter().for_each(drop);
        engine.clock.running = true;
        engine.clock.beat = 1.;
        seq.tick(&mut engine, &tx);
        assert!(!seq.playback(1.)[0].playing);
        assert!(seq.playback(1.)[0].pending.is_none());
        assert_eq!(rx.try_iter().count(), 0);
    }

    #[test]
    fn independent_modes_wait_for_launch_and_reset_to_idle() {
        for mode in [pr0_core::Mode::Conducted, pr0_core::Mode::Freeform] {
            let mut p = pr0_core::demo_project("test".into(), "Test".into(), mode);
            p.parts[0].midi_port = Some("test sink".into());
            let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
            engine.clock.running = true;
            let mut seq = Sequencer::new(&p);
            let (tx, rx) = sync_channel(64);
            engine.clock.beat = 5.25;
            seq.tick(&mut engine, &tx);
            assert_eq!(rx.try_iter().count(), 0);
            seq.launch(&p.parts[0].id, true, &mut engine, &tx);
            assert_eq!(seq.playback(5.25)[0].pending, Some((6., true)));
            engine.clock.beat = 5.99;
            seq.tick(&mut engine, &tx);
            assert_eq!(rx.try_iter().count(), 0);
            engine.clock.beat = 6.;
            seq.tick(&mut engine, &tx);
            assert!(matches!(
                rx.try_recv().unwrap(),
                External::Note { velocity: 90, .. }
            ));
            assert_eq!(seq.playback(6.25)[0].position, 0.25);
            assert!(seq.playback(6.25)[0].pending.is_none());
            seq.reset(&mut engine, &tx);
            assert!(!seq.playback(0.)[0].playing);
        }
    }

    #[test]
    fn osc_panic_releases_the_shared_pitch_on_the_wire() {
        let receiver = std::net::UdpSocket::bind("127.0.0.1:0").unwrap();
        receiver
            .set_read_timeout(Some(std::time::Duration::from_secs(2)))
            .unwrap();
        let tx = external_worker(std::sync::Arc::new(crate::osc::Runtime::default()));
        for velocity in [90, 70, 0] {
            tx.send(External::Note {
                midi: None,
                midi_channel: 1,
                osc: Some(receiver.local_addr().unwrap().to_string()),
                address: "/note".into(),
                pitch: 60,
                velocity,
            })
            .unwrap();
        }
        tx.send(External::Panic).unwrap();
        let mut buffer = [0; 1024];
        for velocity in [90, 0] {
            let n = receiver.recv(&mut buffer).unwrap();
            let (_, packet) = rosc::decoder::decode_udp(&buffer[..n]).unwrap();
            let rosc::OscPacket::Message(message) = packet else {
                panic!("expected note");
            };
            assert_eq!(message.addr, "/note");
            assert_eq!(
                message.args,
                vec![rosc::OscType::Int(60), rosc::OscType::Int(velocity)]
            );
        }
    }
    #[test]
    fn external_shared_pitch_stays_held_until_last_release() {
        let routes = [
            Route::Midi("test".into(), 1),
            Route::Midi("test".into(), 2),
            Route::Osc("127.0.0.1:9000".parse().unwrap(), "/note".into()),
        ];
        let mut held = HeldNotes::default();
        for route in &routes {
            assert!(held.update(route.clone(), 60, 90));
            assert!(!held.update(route.clone(), 60, 70));
            assert!(held.update(route.clone(), 64, 90));
            assert!(!held.update(route.clone(), 60, 0));
            assert!(held.update(route.clone(), 60, 0));
            assert!(!held.update(route.clone(), 60, 0));
            assert!(held.update(route.clone(), 64, 0));
        }
        assert!(held.0.is_empty());
    }

    #[test]
    fn stop_and_relaunch_release_only_held_notes() {
        let mut p =
            pr0_core::demo_project("test".into(), "Test".into(), pr0_core::Mode::Structured);
        p.parts[0].midi_port = Some("test sink".into());
        p.parts[0].notes.truncate(1);
        p.parts[0].notes[0].duration = 4.;
        let mut second = p.parts[0].clone();
        second.id = "second".into();
        p.parts.push(second);
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        engine.clock.running = true;
        let mut seq = Sequencer::new(&p);
        let (tx, rx) = sync_channel(64);
        seq.tick(&mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 2);
        seq.launch(&p.parts[0].id, false, &mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 0, "stop waits for the boundary");
        assert!(seq.lanes[0].playing);
        engine.clock.beat = 1.;
        seq.tick(&mut engine, &tx);
        let offs: Vec<_> = rx.try_iter().collect();
        assert_eq!(offs.len(), 1, "only the currently held note gets an off");
        assert!(matches!(
            offs[0],
            External::Note {
                pitch: 60,
                velocity: 0,
                ..
            }
        ));
        assert!(seq.lanes[0].active.iter().all(Option::is_none));
        assert!(seq.lanes[1].active.iter().any(Option::is_some));
        seq.launch("second", true, &mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 0);
        engine.clock.beat = 2.;
        seq.tick(&mut engine, &tx);
        assert_eq!(
            rx.try_iter().count(),
            2,
            "relaunch releases and starts an instance"
        );
    }

    #[test]
    fn reset_at_zero_replays_the_initial_event_without_a_stuck_note() {
        let mut p =
            pr0_core::demo_project("test".into(), "Test".into(), pr0_core::Mode::Structured);
        p.parts[0].midi_port = Some("test sink".into());
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        engine.clock.running = true;
        let mut seq = Sequencer::new(&p);
        let (tx, rx) = sync_channel(64);
        seq.tick(&mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 1);
        seq.reset(&mut engine, &tx);
        engine.clock.stop();
        assert_eq!(rx.try_iter().count(), 1);
        engine.clock.running = true;
        seq.tick(&mut engine, &tx);
        assert!(matches!(
            rx.try_recv().unwrap(),
            External::Note { velocity: 90, .. }
        ));
        engine.clock.running = false;
        seq.tick(&mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 1);
        engine.clock.running = true;
        engine.clock.beat = 0.8;
        seq.tick(&mut engine, &tx);
        assert_eq!(rx.try_iter().count(), 0, "pause already released the note");
    }

    #[test]
    fn scheduled_note_on_and_off_follow_engine_time() {
        let mut p =
            pr0_core::demo_project("test".into(), "Test".into(), pr0_core::Mode::Structured);
        p.parts[0].midi_port = Some("test sink".into());
        p.parts[0].notes.truncate(1);
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        engine.clock.running = true;
        let mut sequencer = Sequencer::new(&p);
        let (tx, rx) = sync_channel(16);
        sequencer.tick(&mut engine, &tx);
        assert!(matches!(
            rx.try_recv().unwrap(),
            External::Note {
                pitch: 60,
                velocity: 90,
                ..
            }
        ));
        for _ in 0..18002 {
            engine.render(&[], &mut [[0.; 8]; 1]);
            sequencer.tick(&mut engine, &tx);
        }
        assert!(matches!(
            rx.try_recv().unwrap(),
            External::Note {
                pitch: 60,
                velocity: 0,
                ..
            }
        ));
        assert!(rx.try_recv().is_err());
    }
}

#[cfg(test)]
mod score_tests {
    use super::*;
    use pr0_core::score::*;
    #[test]
    fn tempo_map_changes_clock_tempo_at_written_positions_and_respects_manual_edits() {
        let mut p =
            pr0_core::demo_project("tempo".into(), "tempo".into(), pr0_core::Mode::Structured);
        p.score = Some(
            serde_json::from_value(serde_json::json!({
                "version": 1, "length": 8.0, "loop_score": false,
                "meters": [], "keys": [], "repeats": [],
                "tempos": [{"beat": 0.0, "bpm": 60.0}, {"beat": 2.0, "bpm": 120.0}]
            }))
            .unwrap(),
        );
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        engine.clock.running = true;
        engine.clock.bpm = 100.;
        let mut seq = Sequencer::new(&p);
        let (tx, rx) = sync_channel(256);
        seq.tick(&mut engine, &tx);
        assert_eq!(engine.clock.bpm, 60.);
        let mut guard = 0;
        while engine.clock.beat < 2. && guard < 200_000 {
            engine.render(&[], &mut [[0.; 8]; 1]);
            seq.tick(&mut engine, &tx);
            while rx.try_recv().is_ok() {}
            guard += 1;
        }
        assert_eq!(engine.clock.bpm, 120.);
        // A manual tempo edit after the last map entry stays in force.
        engine.clock.set_tempo(90.);
        for _ in 0..10 {
            engine.render(&[], &mut [[0.; 8]; 1]);
            seq.tick(&mut engine, &tx);
        }
        assert_eq!(engine.clock.bpm, 90.);
        // Rewinding re-applies the tempo in force at the new position.
        engine.clock.beat = 0.;
        seq.tick(&mut engine, &tx);
        assert_eq!(engine.clock.bpm, 60.);
    }
    #[test]
    fn mute_and_solo_suppress_events_and_release_held_notes() {
        let mut p = pr0_core::demo_project("mix".into(), "mix".into(), pr0_core::Mode::Structured);
        p.parts[0].midi_port = Some("test-port".into());
        let mut second = p.parts[0].clone();
        second.id = "second".into();
        p.parts.push(second);
        let mut seq = Sequencer::new(&p);
        let mut e = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        e.clock.running = true;
        let (tx, rx) = sync_channel(256);
        seq.tick(&mut e, &tx);
        while rx.try_recv().is_ok() {}
        p.parts[0].muted = true;
        let mut prepared = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        prepared.clock = e.clock;
        let next = seq.replace(&p, &mut e, &mut prepared, &tx);
        assert!(next.lanes[0].events.is_empty());
        assert!(!next.lanes[1].events.is_empty());
        assert!(
            rx.try_iter()
                .any(|event| matches!(event, External::Note { velocity: 0, .. }))
        );
        p.parts[1].solo = true;
        p.parts[0].muted = false;
        let next = Sequencer::new(&p);
        assert!(next.lanes[0].events.is_empty());
        assert!(!next.lanes[1].events.is_empty());
        p.parts[1].solo = false;
        assert!(
            Sequencer::new(&p)
                .lanes
                .iter()
                .all(|l| !l.events.is_empty())
        );
    }
    #[test]
    fn shared_repeat_positions_stop_and_restart_follow_engine_beats() {
        let mut p = pr0_core::demo_project("x".into(), "x".into(), pr0_core::Mode::Structured);
        p.score = Some(Timeline {
            barlines: vec![],
            version: 1,
            length: 8.,
            loop_score: false,
            meters: vec![MeterChange {
                beat: 4.,
                beats: 3,
                unit: 4,
            }],
            keys: vec![],
            repeats: vec![Repeat {
                start: 0.,
                end: 4.,
                times: 2,
                first_ending: None,
            }],
            navigation: None,
            tempos: vec![],
        });
        let mut seq = Sequencer::new(&p);
        assert_eq!(seq.lanes[0].length, 12.);
        assert_eq!(seq.playback(4.5)[0].position, 0.5);
        assert_eq!(seq.playback(8.5)[0].position, 4.5);
        let mut e = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        let (tx, _) = sync_channel(256);
        e.clock.running = true;
        seq.tick(&mut e, &tx);
        e.clock.beat = 12.;
        seq.tick(&mut e, &tx);
        assert!(!e.clock.running);
        e.clock.running = true;
        seq.tick(&mut e, &tx);
        assert_eq!(e.clock.beat, 0.);
        assert!(seq.lanes[0].active[0].is_some());
    }
    #[test]
    fn independent_parts_keep_launch_state_and_expand_their_local_ranges() {
        let mut p = pr0_core::demo_project("x".into(), "x".into(), pr0_core::Mode::Freeform);
        p.parts[0].loop_beats = 4.;
        p.score = Some(Timeline {
            barlines: vec![],
            version: 1,
            length: 8.,
            loop_score: false,
            meters: vec![],
            keys: vec![],
            repeats: vec![Repeat {
                start: 0.,
                end: 4.,
                times: 2,
                first_ending: None,
            }],
            navigation: None,
            tempos: vec![],
        });
        let seq = Sequencer::new(&p);
        assert!(!seq.lanes[0].playing);
        assert!(!seq.lanes[0].looping);
        assert_eq!(seq.lanes[0].length, 8.);
    }
    #[test]
    fn conducted_cues_use_the_global_pulse_count_in_and_finish_one_shot() {
        let mut p = pr0_core::demo_project("cue".into(), "Cue".into(), pr0_core::Mode::Conducted);
        p.conducted.pulse_unit = 8;
        p.parts[0].loop_beats = 2.;
        p.parts[0].performer = Some("player".into());
        let mut sibling = p.parts[0].clone();
        sibling.id = "same-performer".into();
        p.parts.push(sibling);
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        let (tx, _) = sync_channel(256);
        let mut seq = Sequencer::new(&p);
        engine.clock.running = true;
        engine.clock.beat = 0.1;
        seq.cue(&[p.parts[0].id.clone()], true, false, 2, &mut engine, &tx);
        assert_eq!(seq.playback(0.5)[0].count_in_remaining, Some(2));
        assert!(seq.cue_count_in_for_part(0.5, "same-performer"));
        assert_eq!(seq.playback(1.0)[0].count_in_remaining, Some(1));
        engine.clock.beat = 1.5;
        seq.tick(&mut engine, &tx);
        assert!(seq.playback(1.5)[0].playing);
        assert!(!seq.playback(1.5)[0].repeating);
        engine.clock.beat = 2.5;
        seq.tick(&mut engine, &tx);
        assert!(!seq.playback(2.5)[0].playing);
    }
    #[test]
    fn busy_performer_gets_a_bounded_successor_queue() {
        let mut p =
            pr0_core::demo_project("queue".into(), "Queue".into(), pr0_core::Mode::Conducted);
        p.parts[0].performer = Some("player".into());
        p.parts[0].loop_beats = 1.;
        let mut second = p.parts[0].clone();
        second.id = "second".into();
        p.parts.push(second);
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        let (tx, _) = sync_channel(256);
        let mut seq = Sequencer::new(&p);
        engine.clock.running = true;
        engine.clock.beat = 0.1;
        seq.cue(&[p.parts[0].id.clone()], true, false, 0, &mut engine, &tx);
        engine.clock.beat = 1.;
        seq.tick(&mut engine, &tx);
        seq.cue(&["second".into()], true, false, 0, &mut engine, &tx);
        assert_eq!(seq.playback(1.)[1].queue_position, Some(1));
        assert_eq!(seq.playback(1.)[1].scheduled_start, Some(2.));
        // Stopping the current part early does not pull its successor off the
        // already communicated boundary.
        seq.cue(&[p.parts[0].id.clone()], false, false, 0, &mut engine, &tx);
        engine.clock.beat = 1.5;
        seq.tick(&mut engine, &tx);
        assert!(!seq.playback(1.5)[1].playing);
        engine.clock.beat = 2.;
        seq.tick(&mut engine, &tx);
        let state = seq.playback(2.);
        assert!(!state[0].playing);
        assert!(state[1].playing);
    }
    #[test]
    fn repeating_part_exits_at_its_cycle_boundary_for_a_successor() {
        let mut p = pr0_core::demo_project(
            "repeat-queue".into(),
            "Repeat queue".into(),
            pr0_core::Mode::Conducted,
        );
        p.score = None;
        p.parts[0].performer = Some("player".into());
        p.parts[0].loop_beats = 1.;
        let mut second = p.parts[0].clone();
        second.id = "successor".into();
        p.parts.push(second);
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        let (tx, _) = sync_channel(256);
        let mut seq = Sequencer::new(&p);
        engine.clock.running = true;
        engine.clock.beat = 0.1;
        seq.cue(&[p.parts[0].id.clone()], true, true, 0, &mut engine, &tx);
        engine.clock.beat = 1.;
        seq.tick(&mut engine, &tx);
        engine.clock.beat = 2.1;
        seq.tick(&mut engine, &tx);
        seq.cue(&["successor".into()], true, false, 0, &mut engine, &tx);
        assert_eq!(seq.playback(2.1)[1].scheduled_start, Some(3.));
        engine.clock.beat = 2.9;
        seq.tick(&mut engine, &tx);
        assert!(seq.playback(2.9)[0].playing);
        engine.clock.beat = 3.;
        seq.tick(&mut engine, &tx);
        let state = seq.playback(3.);
        assert!(!state[0].playing);
        assert!(state[1].playing);
    }
    #[test]
    fn conducted_meter_changes_map_each_local_beat_to_the_global_pulse() {
        let mut p =
            pr0_core::demo_project("meter".into(), "Meter".into(), pr0_core::Mode::Conducted);
        p.conducted.pulse_unit = 4;
        p.score = None;
        p.parts[0].loop_beats = 4.;
        p.parts[0].performance_meters = vec![
            MeterChange {
                beat: 0.,
                beats: 2,
                unit: 4,
            },
            MeterChange {
                beat: 2.,
                beats: 4,
                unit: 8,
            },
        ];
        let seq = Sequencer::new(&p);
        assert_eq!(seq.lanes[0].length, 6.);
        assert_eq!(seq.playback(4.)[0].position, 0.);

        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        let (tx, _) = sync_channel(256);
        let mut seq = Sequencer::new(&p);
        engine.clock.running = true;
        seq.cue(&[p.parts[0].id.clone()], true, false, 0, &mut engine, &tx);
        engine.clock.beat = 1.;
        seq.tick(&mut engine, &tx);
        assert_eq!(seq.playback(5.)[0].position, 3.);
    }
    #[test]
    fn browser_midi_is_bounded_to_a_part_and_panics_held_notes() {
        let mut p = pr0_core::demo_project(
            "browser-midi".into(),
            "Browser MIDI".into(),
            pr0_core::Mode::Conducted,
        );
        p.parts[0].midi_port = Some("test-port".into());
        let mut engine = pr0_dsp::Engine::prepare(p.graph.clone(), 48000.).unwrap();
        let (tx, rx) = sync_channel(256);
        let mut seq = Sequencer::new(&p);
        seq.browser_midi(
            &p.parts[0].id,
            pr0_core::midi::Message {
                status: 0x90,
                data1: 64,
                data2: 110,
            },
            &mut engine,
            &tx,
        );
        assert!(seq.lanes[0].browser_notes[64]);
        assert!(
            matches!(rx.try_recv(), Ok(External::Message { message, .. }) if message.data1 == 64 && message.data2 == 110)
        );
        seq.browser_midi_panic(&[p.parts[0].id.clone()], &mut engine, &tx);
        assert!(!seq.lanes[0].browser_notes[64]);
        assert!(rx.try_iter().any(|event| matches!(event, External::Message { message, .. } if message.status & 0xf0 == 0xb0 && message.data1 == 123)));
    }
    #[test]
    fn staff_dynamics_scale_only_that_staff_and_override_part_dynamics() {
        let mut p = pr0_core::demo_project("dyn".into(), "dyn".into(), pr0_core::Mode::Structured);
        let part = &mut p.parts[0];
        part.notes.truncate(2);
        part.notes[0].beat = 0.;
        part.notes[1].beat = 0.;
        part.notes[1].pitch = 67;
        let staff = |id: &str| pr0_core::score::Staff {
            hidden_rests: vec![],
            key_mode: None,
            clef_changes: vec![],
            instrument_node: None,
            midi_channel: None,
            midi_port: None,
            marks: vec![],
            dynamics: None,
            curves: vec![],
            id: id.into(),
            name: id.into(),
            clef: "treble".into(),
            key_signature: None,
            transpose: 0,
        };
        let mut upper = staff("upper");
        upper.dynamics = Some(Dynamics {
            mode: DynamicsMode::Velocity,
            controller: 11,
            events: vec![AutomationEvent {
                id: "loud".into(),
                beat: 0.,
                duration: 0.,
                start: 127.,
                end: 127.,
                curve: Curve::Linear,
            }],
        });
        part.staves = vec![upper, staff("lower")];
        part.dynamics = Some(Dynamics {
            mode: DynamicsMode::Velocity,
            controller: 11,
            events: vec![AutomationEvent {
                id: "soft".into(),
                beat: 0.,
                duration: 0.,
                start: 45.,
                end: 45.,
                curve: Curve::Linear,
            }],
        });
        for (i, staff) in ["upper", "lower"].into_iter().enumerate() {
            let n = &mut part.notes[i];
            n.velocity = 90;
            n.duration = 1.;
            n.notation = Some(Notation {
                onset: None,
                written_duration: None,
                tie_to: None,
                slur_to: None,
                grace_to: None,
                articulation: None,
                octave: 0,
                staff: staff.into(),
                step: 28 + i as i16 * 4,
                alter: 0,
                voice: 1,
                base: 1.,
                dots: 0,
                tuplet_actual: 1,
                tuplet_normal: 1,
            });
        }
        assert!(p.validate().is_ok(), "{:?}", p.validate());
        let seq = Sequencer::new(&p);
        let attacks: Vec<u8> = seq.lanes[0]
            .events
            .iter()
            .filter(|e| e.velocity > 0)
            .map(|e| e.velocity)
            .collect();
        assert_eq!(attacks.len(), 2);
        assert!(
            attacks.contains(&127),
            "upper staff follows its own dynamics: {attacks:?}"
        );
        assert!(
            attacks.contains(&45),
            "lower staff falls back to part dynamics: {attacks:?}"
        );
    }
    #[test]
    fn dynamics_change_attacks_without_changing_authored_notes() {
        let mut p = pr0_core::demo_project("x".into(), "x".into(), pr0_core::Mode::Structured);
        p.parts[0].dynamics = Some(Dynamics {
            mode: DynamicsMode::Velocity,
            controller: 11,
            events: vec![AutomationEvent {
                id: "d".into(),
                beat: 0.,
                duration: 4.,
                start: 20.,
                end: 100.,
                curve: Curve::Linear,
            }],
        });
        let seq = Sequencer::new(&p);
        let attacks: Vec<_> = seq.lanes[0]
            .events
            .iter()
            .filter(|e| e.velocity > 0)
            .map(|e| e.velocity)
            .collect();
        assert_eq!(&attacks[..5], &[20, 40, 60, 80, 100]);
        assert_eq!(p.parts[0].notes[0].velocity, 90);
    }
}

#[cfg(test)]
mod metronome_map_tests {
    use super::*;
    #[test]
    fn written_meter_and_origin_follow_a_repeated_six_eight_bar() {
        let mut project =
            pr0_core::demo_project("meter".into(), "Meter".into(), pr0_core::Mode::Structured);
        project.parts[0].loop_beats = 8.;
        project.score = Some(serde_json::from_value(serde_json::json!({
            "version":1,"length":8.,"loop_score":false,"meters":[{"beat":4.,"beats":6,"unit":8}],
            "keys":[],"tempos":[],"repeats":[{"start":4.,"end":7.,"times":2,"first_ending":null}]
        })).unwrap());
        let sequencer = Sequencer::new(&project);
        assert_eq!(sequencer.metronome_position(3.), (3., 0., 4, 4));
        assert_eq!(sequencer.metronome_position(4.5), (4.5, 4., 6, 8));
        assert_eq!(sequencer.metronome_position(7.), (4., 4., 6, 8));
    }
}
