//! Persisted notation metadata. Legacy flat scores remain valid without this data.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Staff {
    #[serde(default)]
    pub key_mode: Option<String>,
    #[serde(default)]
    pub clef_changes: Vec<ClefChange>,
    #[serde(default)]
    pub instrument_node: Option<String>,
    #[serde(default)]
    pub midi_channel: Option<u8>,
    #[serde(default)]
    pub midi_port: Option<String>,
    pub id: String,
    pub name: String,
    pub clef: String,
    #[serde(default)]
    pub key_signature: Option<String>,
    #[serde(default)]
    pub transpose: i8,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClefChange {
    pub beat: f64,
    pub clef: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notation {
    #[serde(default)]
    pub onset: Option<Rational>,
    #[serde(default)]
    pub written_duration: Option<Rational>,
    #[serde(default)]
    pub tie_to: Option<String>,
    #[serde(default)]
    pub slur_to: Option<String>,
    #[serde(default)]
    pub grace_to: Option<String>,
    #[serde(default)]
    pub articulation: Option<String>,
    #[serde(default)]
    pub octave: i8,
    pub staff: String,
    /// Absolute diatonic step: C4 = 28. Written spelling is independent of MIDI pitch.
    pub step: i16,
    pub alter: i8,
    pub voice: u8,
    pub base: f64,
    pub dots: u8,
    #[serde(default = "one")]
    pub tuplet_actual: u8,
    #[serde(default = "one")]
    pub tuplet_normal: u8,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rational {
    pub numerator: i64,
    pub denominator: u32,
}
impl Rational {
    pub fn value(&self) -> f64 {
        self.numerator as f64 / self.denominator as f64
    }
    pub fn valid(&self) -> bool {
        self.denominator > 0
            && self.denominator <= 1_000_000_000
            && self.numerator >= 0
            && self.value() <= 8192.
    }
}
fn one() -> u8 {
    1
}
pub fn validate(part: &crate::Part) -> Result<(), String> {
    if part.staves.len() > 8 {
        return Err("At most eight staves per part".into());
    }
    let mut ids = BTreeSet::new();
    for s in &part.staves {
        let mut last = -1.;
        for c in &s.clef_changes {
            if !c.beat.is_finite()
                || c.beat < 0.
                || c.beat > 4096.
                || c.beat <= last
                || !["treble", "bass", "alto", "tenor"].contains(&c.clef.as_str())
            {
                return Err("Invalid staff clef change".into());
            }
            last = c.beat;
        }
        if s.clef_changes.len() > 1024 {
            return Err("Too many clef changes".into());
        }
        if s.key_mode
            .as_ref()
            .is_some_and(|m| !["major", "minor"].contains(&m.as_str()))
            || s.midi_channel.is_some_and(|c| !(1..=16).contains(&c))
            || s.midi_port
                .as_ref()
                .is_some_and(|p| p.is_empty() || p.len() > 256)
            || s.id.is_empty()
            || s.id.len() > 120
            || !ids.insert(&s.id)
            || s.name.trim().is_empty()
            || s.name.len() > 120
            || !["treble", "bass", "alto", "tenor"].contains(&s.clef.as_str())
            || !(-48..=48).contains(&s.transpose)
            || s.key_signature.as_ref().is_some_and(|k| {
                ![
                    "Cb", "Gb", "Db", "Ab", "Eb", "Bb", "F", "C", "G", "D", "A", "E", "B", "F#",
                    "C#",
                ]
                .contains(&k.as_str())
            })
        {
            return Err("Invalid score staff".into());
        }
    }
    let by_id: std::collections::BTreeMap<_, _> =
        part.notes.iter().map(|n| (n.id.as_str(), n)).collect();
    let mut notes = BTreeSet::new();
    for n in &part.notes {
        if n.id.is_empty() || !notes.insert(&n.id) {
            return Err("Note IDs must be unique within a part".into());
        }
        if let Some(v) = &n.notation {
            if v.onset
                .as_ref()
                .is_some_and(|r| !r.valid() || (r.value() - n.beat).abs() > 1e-8)
                || v.written_duration
                    .as_ref()
                    .is_some_and(|r| !r.valid() || (r.value() - n.duration).abs() > 1e-8)
            {
                return Err("Rational notation time must agree with playback time".into());
            }
            if !(-2..=2).contains(&v.octave)
                || v.articulation.as_ref().is_some_and(|a| {
                    !["staccato", "tenuto", "accent", "marcato"].contains(&a.as_str())
                })
                || !ids.contains(&v.staff)
                || !(-70..=140).contains(&v.step)
                || !(-2..=2).contains(&v.alter)
                || !(1..=4).contains(&v.voice)
                || v.dots > 2
                || !v.base.is_finite()
                || v.base <= 0.
                || v.base > 4096.
                || !(1..=32).contains(&v.tuplet_actual)
                || !(1..=32).contains(&v.tuplet_normal)
            {
                return Err("Invalid note notation".into());
            }
            let duration = v.base * (2. - 2_f64.powi(-(v.dots as i32))) * v.tuplet_normal as f64
                / v.tuplet_actual as f64;
            let staff = part.staves.iter().find(|s| s.id == v.staff).unwrap();
            let pitch = (v.step.div_euclid(7) + 1) * 12
                + [0, 2, 4, 5, 7, 9, 11][v.step.rem_euclid(7) as usize]
                + v.alter as i16
                + staff.transpose as i16
                + v.octave as i16 * 12;
            for target in [&v.tie_to, &v.slur_to, &v.grace_to].into_iter().flatten() {
                let Some(other) = by_id.get(target.as_str()) else {
                    return Err("Notation references a missing note".into());
                };
                if other.id == n.id
                    || n.rest
                    || other.rest
                    || other.beat < n.beat
                    || v.grace_to.as_ref() == Some(target)
                        && other
                            .notation
                            .as_ref()
                            .is_some_and(|v| v.grace_to.is_some())
                {
                    return Err("Notation must link forward to another pitched note".into());
                }
                if v.tie_to.as_ref() == Some(target)
                    && (other.pitch != n.pitch || (n.beat + n.duration - other.beat).abs() > 1e-8)
                {
                    return Err("Ties require adjacent notes of the same pitch".into());
                }
                if v.grace_to.as_ref() == Some(target) && (other.beat - n.beat).abs() > 1e-8 {
                    return Err("Grace notes share the principal note onset".into());
                }
            }
            if (duration - n.duration).abs() > 1e-8 || (!n.rest && pitch != n.pitch as i16) {
                return Err("Notation must agree with sounding pitch and duration".into());
            }
        }
    }
    let mut voices = std::collections::BTreeMap::<(&str, u8), Vec<&crate::Note>>::new();
    for n in &part.notes {
        if let Some(v) = &n.notation {
            if v.grace_to.is_none() {
                voices.entry((&v.staff, v.voice)).or_default().push(n);
            }
        }
    }
    for notes in voices.values_mut() {
        notes.sort_by(|a, b| a.beat.total_cmp(&b.beat));
        let mut onset = -1.;
        let mut end = 0.;
        for n in notes {
            if (n.beat - onset).abs() > 1e-8 {
                if n.beat < end - 1e-8 {
                    return Err("Overlapping rhythmic events require separate voices".into());
                }
                onset = n.beat;
            }
            end = end.max(n.beat + n.duration);
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_and_spelled_transposing_notes_validate() {
        let mut p = crate::demo_project("x".into(), "x".into(), crate::Mode::Structured);
        assert!(p.validate().is_ok());
        p.parts[0].staves.push(Staff {
            key_mode: None,
            clef_changes: vec![],
            instrument_node: None,
            midi_channel: None,
            midi_port: None,
            id: "s".into(),
            name: "Upper".into(),
            clef: "treble".into(),
            key_signature: None,
            transpose: -2,
        });
        let n = &mut p.parts[0].notes[0];
        n.pitch = 60;
        n.notation = Some(Notation {
            onset: None,
            written_duration: None,
            tie_to: None,
            slur_to: None,
            grace_to: None,
            articulation: None,
            octave: 0,
            staff: "s".into(),
            step: 29,
            alter: 0,
            voice: 1,
            base: 0.5,
            dots: 1,
            tuplet_actual: 1,
            tuplet_normal: 1,
        });
        assert!(p.validate().is_ok());
        p.parts[0].notes[0].notation.as_mut().unwrap().staff = "missing".into();
        assert!(p.validate().is_err());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MeterChange {
    pub beat: f64,
    pub beats: u8,
    pub unit: u8,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct KeyChange {
    pub beat: f64,
    pub key: String,
    #[serde(default)]
    pub mode: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Repeat {
    pub start: f64,
    pub end: f64,
    pub times: u8,
    #[serde(default)]
    pub first_ending: Option<f64>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Navigation {
    pub at: f64,
    pub target: f64,
    #[serde(default)]
    pub fine: Option<f64>,
    #[serde(default)]
    pub coda: Option<[f64; 2]>,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Timeline {
    pub version: u8,
    pub length: f64,
    pub loop_score: bool,
    #[serde(default)]
    pub meters: Vec<MeterChange>,
    #[serde(default)]
    pub keys: Vec<KeyChange>,
    #[serde(default)]
    pub repeats: Vec<Repeat>,
    #[serde(default)]
    pub navigation: Option<Navigation>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct Span {
    pub start: f64,
    pub end: f64,
    pub elapsed: f64,
}
impl Timeline {
    pub fn validate(&self) -> Result<(), String> {
        let valid = |b: f64| b.is_finite() && b >= 0. && b <= self.length;
        if self.version != 1
            || !self.length.is_finite()
            || !(0.25..=4096.).contains(&self.length)
            || self.meters.len() > 1024
            || self.keys.len() > 1024
            || self.repeats.len() > 64
        {
            return Err("Invalid score timeline".into());
        }
        let mut last = -1.;
        for m in &self.meters {
            if !valid(m.beat)
                || m.beat <= last
                || !(1..=16).contains(&m.beats)
                || ![1, 2, 4, 8, 16, 32].contains(&m.unit)
            {
                return Err("Invalid or unordered meter change".into());
            }
            last = m.beat;
        }
        last = -1.;
        for k in &self.keys {
            if k.mode
                .as_ref()
                .is_some_and(|m| !["major", "minor"].contains(&m.as_str()))
                || !valid(k.beat)
                || k.beat <= last
                || ![
                    "Cb", "Gb", "Db", "Ab", "Eb", "Bb", "F", "C", "G", "D", "A", "E", "B", "F#",
                    "C#",
                ]
                .contains(&k.key.as_str())
            {
                return Err("Invalid or unordered key change".into());
            }
            last = k.beat;
        }
        last = 0.;
        for r in &self.repeats {
            if !valid(r.start)
                || !valid(r.end)
                || r.start < last
                || r.end <= r.start
                || !(2..=32).contains(&r.times)
                || r.first_ending
                    .is_some_and(|b| !valid(b) || b <= r.start || b >= r.end)
            {
                return Err("Repeats must be ordered, disjoint ranges with 2–32 passes".into());
            }
            last = r.end;
        }
        if let Some(n) = &self.navigation {
            if !valid(n.at)
                || !valid(n.target)
                || n.target >= n.at
                || n.fine.is_some_and(|b| !valid(b) || b <= n.target)
                || n.coda
                    .is_some_and(|[a, b]| !valid(a) || !valid(b) || a <= n.target || b <= a)
                || n.fine.is_some() && n.coda.is_some()
            {
                return Err("Invalid D.C./D.S. navigation target, fine or coda".into());
            }
        }
        Ok(())
    }
    /// Bounded traversal preparation, never called from render or callbacks.
    pub fn spans(&self) -> Vec<Span> {
        let mut ranges = Vec::new();
        let mut cursor = 0.;
        let end = self
            .navigation
            .as_ref()
            .map(|n| n.at)
            .unwrap_or(self.length);
        for r in &self.repeats {
            if r.start >= end {
                break;
            }
            if cursor < r.start {
                ranges.push((cursor, r.start.min(end)));
            }
            for pass in 0..r.times {
                let stop = if pass == r.times - 1 {
                    r.first_ending.unwrap_or(r.end)
                } else {
                    r.end
                };
                ranges.push((r.start, stop.min(end)));
            }
            cursor = r.end;
            if cursor >= end {
                break;
            }
        }
        if cursor < end {
            ranges.push((cursor, end));
        }
        if let Some(n) = &self.navigation {
            if let Some([from, to]) = n.coda {
                ranges.push((n.target, from));
                ranges.push((to, self.length));
            } else {
                ranges.push((n.target, n.fine.unwrap_or(self.length)));
            }
        }
        let mut elapsed = 0.;
        ranges
            .into_iter()
            .filter(|(a, b)| b > a)
            .map(|(start, end)| {
                let s = Span {
                    start,
                    end,
                    elapsed,
                };
                elapsed += end - start;
                s
            })
            .collect()
    }
}
#[cfg(test)]
mod timeline_tests {
    use super::*;
    #[test]
    fn repeats_endings_and_navigation_are_bounded() {
        let mut t = Timeline {
            version: 1,
            length: 16.,
            loop_score: false,
            meters: vec![],
            keys: vec![],
            repeats: vec![Repeat {
                start: 0.,
                end: 8.,
                times: 2,
                first_ending: Some(4.),
            }],
            navigation: None,
        };
        assert!(t.validate().is_ok());
        assert_eq!(
            t.spans()
                .iter()
                .map(|s| (s.start, s.end, s.elapsed))
                .collect::<Vec<_>>(),
            vec![(0., 8., 0.), (0., 4., 8.), (8., 16., 12.)]
        );
        t.navigation = Some(Navigation {
            at: 16.,
            target: 4.,
            fine: Some(12.),
            coda: None,
        });
        assert_eq!(t.spans().last().unwrap().end, 12.);
        t.repeats[0].times = 255;
        assert!(t.validate().is_err());
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum Curve {
    Step,
    Linear,
    EaseIn,
    EaseOut,
    SCurve,
}
impl Curve {
    pub fn at(self, t: f64) -> f64 {
        let t = t.clamp(0., 1.);
        match self {
            Self::Step => {
                if t < 1. {
                    0.
                } else {
                    1.
                }
            }
            Self::Linear => t,
            Self::EaseIn => t * t,
            Self::EaseOut => 1. - (1. - t) * (1. - t),
            Self::SCurve => t * t * (3. - 2. * t),
        }
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Note,
    Cc,
    Bend,
    Program,
    Pressure,
    PolyPressure,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutomationEvent {
    pub id: String,
    pub beat: f64,
    pub duration: f64,
    pub start: f64,
    pub end: f64,
    pub curve: Curve,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AutomationLane {
    #[serde(default)]
    pub initial: Option<f64>,
    pub id: String,
    pub name: String,
    pub channel: u8,
    pub message: MessageKind,
    pub number: u8,
    pub events: Vec<AutomationEvent>,
}
impl AutomationLane {
    pub fn message_at(&self, value: f64) -> crate::midi::Message {
        let value = value.round() as u16;
        let channel = self.channel - 1;
        let (status, data1, data2) = match self.message {
            MessageKind::Note => (0x90, self.number, value as u8),
            MessageKind::Cc => (0xb0, self.number, value as u8),
            MessageKind::Bend => (0xe0, (value & 127) as u8, (value >> 7) as u8),
            MessageKind::Program => (0xc0, value as u8, 0),
            MessageKind::Pressure => (0xd0, value as u8, 0),
            MessageKind::PolyPressure => (0xa0, self.number, value as u8),
        };
        crate::midi::Message {
            status: status | channel,
            data1,
            data2,
        }
    }
}
pub fn validate_automation(lanes: &[AutomationLane]) -> Result<(), String> {
    if lanes.len() > 32 || lanes.iter().map(|l| l.events.len()).sum::<usize>() > 10000 {
        return Err("At most 32 MIDI lanes and 10,000 automation events per part".into());
    }
    let mut ids = BTreeSet::new();
    let mut targets = BTreeSet::new();
    for l in lanes {
        if l.id.is_empty()
            || !ids.insert(l.id.clone())
            || l.name.trim().is_empty()
            || l.name.len() > 120
            || !(1..=16).contains(&l.channel)
            || l.number > 127
        {
            return Err("Invalid MIDI lane".into());
        }
        if !targets.insert((
            l.channel,
            l.message as u8,
            if matches!(
                l.message,
                MessageKind::Cc | MessageKind::Note | MessageKind::PolyPressure
            ) {
                l.number
            } else {
                0
            },
        )) {
            return Err("Use one lane per MIDI channel/message/controller target".into());
        }
        let max = if l.message == MessageKind::Bend {
            16383.
        } else {
            127.
        };
        if l.initial
            .is_some_and(|v| !v.is_finite() || v < 0. || v > max)
        {
            return Err("Invalid MIDI initial value".into());
        }
        let mut end = -1.;
        for e in &l.events {
            if e.id.is_empty()
                || !ids.insert(e.id.clone())
                || !e.beat.is_finite()
                || !(0. ..=4096.).contains(&e.beat)
                || e.beat < end
                || !e.duration.is_finite()
                || !(0. ..=4096.).contains(&e.duration)
                || !e.start.is_finite()
                || !e.end.is_finite()
                || !(0. ..=max).contains(&e.start)
                || !(0. ..=max).contains(&e.end)
                || l.message == MessageKind::Note && e.duration <= 0.
                || l.message == MessageKind::Program && e.duration != 0.
            {
                return Err("Invalid or overlapping MIDI events".into());
            }
            end = e.beat + e.duration;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum DynamicsMode {
    Velocity,
    Cc,
    Both,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Dynamics {
    pub mode: DynamicsMode,
    pub controller: u8,
    pub events: Vec<AutomationEvent>,
}
impl Dynamics {
    pub fn lane(&self, channel: u8) -> AutomationLane {
        AutomationLane {
            initial: Some(127.),
            id: "score-dynamics".into(),
            name: "Dynamics".into(),
            channel,
            message: MessageKind::Cc,
            number: self.controller,
            events: self.events.clone(),
        }
    }
    pub fn velocity(&self, beat: f64, velocity: u8) -> u8 {
        if self.mode == DynamicsMode::Cc {
            return velocity;
        }
        let Some(e) = self.events.iter().rev().find(|e| e.beat <= beat) else {
            return velocity;
        };
        let t = if e.duration == 0. {
            1.
        } else {
            (beat - e.beat) / e.duration
        };
        let level = e.start + (e.end - e.start) * e.curve.at(t);
        (f64::from(velocity) / 90. * level).round().clamp(0., 127.) as u8
    }
}
