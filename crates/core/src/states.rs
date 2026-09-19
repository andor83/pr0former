//! Saved options only: never topology, assignments, provenance, or DSP history.
use crate::{ControlValue, Graph, IoConfig, Node, SampleChoice};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const MAX_STATES: usize = 64;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Options {
    #[serde(default)]
    pub control_positions: Vec<Option<f64>>,
    pub id: String,
    pub kind: String,
    pub label: String,
    pub channels: usize,
    pub parameters: BTreeMap<String, f64>,
    pub control_value: Option<ControlValue>,
    pub sample_choices: Vec<SampleChoice>,
    pub script: Option<crate::script::Script>,
    pub io: Option<IoConfig>,
}
impl Options {
    pub fn capture(n: &Node) -> Self {
        Self {
            control_positions: n.control_positions.clone(),
            id: n.id.clone(),
            kind: n.kind.clone(),
            label: n.label.clone(),
            channels: n.channels,
            parameters: n.parameters.clone(),
            control_value: n.control_value.clone(),
            sample_choices: n.sample_choices.clone(),
            script: n.script.clone(),
            io: n.io.clone(),
        }
    }
    pub fn apply(&self, n: &mut Node) {
        n.control_positions = self.control_positions.clone();
        n.label = self.label.clone();
        n.channels = self.channels;
        n.parameters = self.parameters.clone();
        n.control_value = self.control_value.clone();
        n.sample_choices = self.sample_choices.clone();
        n.script = self.script.clone();
        n.io = self.io.clone();
    }
    pub fn node(&self) -> Node {
        let mut n = Node {
            control_positions: vec![],
            states: None,
            id: self.id.clone(),
            kind: self.kind.clone(),
            label: self.label.clone(),
            channels: self.channels,
            parameters: Default::default(),
            control_value: None,
            sample_choices: vec![],
            script: None,
            io: None,
            part_id: None,
            library: None,
            parent: None,
            x: 0.,
            y: 0.,
        };
        self.apply(&mut n);
        n
    }
    pub fn assets(&self) -> BTreeSet<u32> {
        let mut ids: BTreeSet<_> = self.sample_choices.iter().map(|s| s.asset).collect();
        for (key, value) in &self.parameters {
            if key == "asset"
                || (self.kind == "granular_field"
                    && key
                        .strip_prefix("sample_")
                        .is_some_and(|s| s.parse::<usize>().is_ok()))
            {
                if *value >= 1. {
                    ids.insert(*value as u32);
                }
            }
        }
        ids
    }
    pub fn remap_assets(&mut self, ids: &BTreeMap<u32, u32>) {
        for s in &mut self.sample_choices {
            if let Some(id) = ids.get(&s.asset) {
                s.asset = *id;
            }
        }
        for (key, value) in &mut self.parameters {
            if key == "asset"
                || (self.kind == "granular_field"
                    && key
                        .strip_prefix("sample_")
                        .is_some_and(|s| s.parse::<usize>().is_ok()))
            {
                if let Some(id) = ids.get(&(*value as u32)) {
                    *value = f64::from(*id);
                }
            }
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    pub slot: u32,
    pub name: String,
    pub nodes: Vec<Options>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bank {
    pub next_slot: u32,
    pub slots: Vec<Snapshot>,
}
impl Default for Bank {
    fn default() -> Self {
        Self {
            next_slot: 1,
            slots: vec![],
        }
    }
}
impl Bank {
    pub fn validate(&self) -> Result<(), String> {
        if self.next_slot == 0 || self.slots.len() > MAX_STATES {
            return Err("Invalid state bank size or next slot".into());
        }
        let mut slots = BTreeSet::new();
        let mut names = BTreeSet::new();
        for s in &self.slots {
            if s.slot == 0 || s.slot >= self.next_slot || !slots.insert(s.slot) {
                return Err("Invalid or duplicate state slot".into());
            }
            if s.name.trim() != s.name
                || s.name.is_empty()
                || s.name.len() > 256
                || !names.insert(&s.name)
            {
                return Err("State names must be unique, trimmed, and 1–256 UTF-8 bytes".into());
            }
            if s.nodes.len() > 256 {
                return Err("State exceeds 256 nodes".into());
            }
            let mut ids = BTreeSet::new();
            for n in &s.nodes {
                if !ids.insert(&n.id) || n.label.trim().is_empty() || n.label.len() > 256 {
                    return Err("Invalid state node identity or label".into());
                }
                Graph {
                    nodes: vec![n.node()],
                    edges: vec![],
                }
                .validate_flat()?;
            }
        }
        Ok(())
    }
    pub fn select(&self, selector: &ControlValue) -> Option<&Snapshot> {
        match selector {
            ControlValue::Number(n)
                if n.is_finite() && *n >= 1. && *n <= u32::MAX as f64 && n.fract() == 0. =>
            {
                self.slots.iter().find(|s| s.slot == *n as u32)
            }
            ControlValue::Text(name) => self.slots.iter().find(|s| s.name == *name),
            _ => None,
        }
    }
    pub fn save(&mut self, nodes: Vec<Options>) -> Result<u32, String> {
        if self.slots.len() >= MAX_STATES {
            return Err("A subgraph can hold at most 64 states".into());
        }
        let mut slot = self.next_slot;
        while self.slots.iter().any(|s| s.name == format!("State {slot}")) {
            slot = slot.checked_add(1).ok_or("State slot numbers exhausted")?;
        }
        let next = slot.checked_add(1).ok_or("State slot numbers exhausted")?;
        let name = format!("State {slot}");
        self.slots.push(Snapshot { slot, name, nodes });
        self.next_slot = next;
        Ok(slot)
    }
    pub fn remap(&mut self, ids: &BTreeMap<String, String>, assets: &BTreeMap<u32, u32>) {
        for s in &mut self.slots {
            for n in &mut s.nodes {
                if let Some(id) = ids.get(&n.id) {
                    n.id = id.clone();
                }
                n.remap_assets(assets);
            }
        }
    }
}
impl Graph {
    pub fn descendants(&self, root: &str) -> Result<BTreeSet<String>, String> {
        if !self
            .nodes
            .iter()
            .any(|n| n.id == root && n.kind == "subgraph")
        {
            return Err("Subgraph missing".into());
        }
        let mut ids = BTreeSet::from([root.to_owned()]);
        loop {
            let before = ids.len();
            for n in &self.nodes {
                if n.parent.as_ref().is_some_and(|p| ids.contains(p)) {
                    ids.insert(n.id.clone());
                }
            }
            if ids.len() == before {
                break;
            }
        }
        ids.remove(root);
        Ok(ids)
    }
    pub fn capture_state(&self, root: &str) -> Result<Vec<Options>, String> {
        let ids = self.descendants(root)?;
        Ok(self
            .nodes
            .iter()
            .filter(|n| ids.contains(&n.id))
            .map(Options::capture)
            .collect())
    }
    pub fn restore_state(
        &mut self,
        root: &str,
        state: &Snapshot,
    ) -> Result<(Vec<String>, Vec<String>), String> {
        let mut candidate = self.clone();
        let ids = self.descendants(root)?;
        let mut restored = vec![];
        let mut skipped = vec![];
        for saved in &state.nodes {
            if let Some(n) = candidate
                .nodes
                .iter_mut()
                .find(|n| n.id == saved.id && n.kind == saved.kind && ids.contains(&n.id))
            {
                saved.apply(n);
                restored.push(n.id.clone());
            } else {
                skipped.push(saved.id.clone());
            }
        }
        candidate.validate()?;
        *self = candidate;
        Ok((restored, skipped))
    }
}
/// Preserve recalled fields unless an authored edit replaces that field. Parameters merge per key.
pub fn reconcile(previous: &Graph, authored: &Graph, runtime: &Graph) -> Graph {
    let mut graph = authored.clone();
    for n in &mut graph.nodes {
        let Some(old) = previous
            .nodes
            .iter()
            .find(|o| o.id == n.id && o.kind == n.kind)
        else {
            continue;
        };
        let Some(live) = runtime
            .nodes
            .iter()
            .find(|o| o.id == n.id && o.kind == n.kind)
        else {
            continue;
        };
        let mut merged = serde_json::to_value(Options::capture(n)).unwrap();
        let before = serde_json::to_value(Options::capture(old)).unwrap();
        let effective = serde_json::to_value(Options::capture(live)).unwrap();
        for key in [
            "control_positions",
            "label",
            "channels",
            "control_value",
            "sample_choices",
            "script",
            "io",
        ] {
            if merged[key] == before[key] {
                merged[key] = effective[key].clone();
            }
        }
        let keys: BTreeSet<_> = old
            .parameters
            .keys()
            .chain(n.parameters.keys())
            .chain(live.parameters.keys())
            .cloned()
            .collect();
        let params = merged["parameters"].as_object_mut().unwrap();
        for key in keys {
            if old.parameters.get(&key) == n.parameters.get(&key) {
                if let Some(v) = live.parameters.get(&key) {
                    params.insert(key, serde_json::json!(v));
                } else {
                    params.remove(&key);
                }
            }
        }
        serde_json::from_value::<Options>(merged).unwrap().apply(n);
    }
    graph
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn graph() -> Graph {
        serde_json::from_value(json!({"nodes":[
        {"id":"root","kind":"subgraph","label":"Root","x":0,"y":0,"channels":1},
        {"id":"inner","parent":"root","kind":"subgraph","label":"Inner","x":10,"y":0,"channels":1},
        {"id":"v","parent":"inner","kind":"value","label":"Value","x":40,"y":10,"channels":1,"parameters":{"value":12}},
        {"id":"outside","kind":"value","label":"Outside","x":0,"y":0,"channels":1}
    ],"edges":[]})).unwrap()
    }
    #[test]
    fn capture_is_descendant_options_only_and_recall_skips_changed_identity() {
        let mut g = graph();
        g.nodes[1].states = Some(Bank::default());
        let nodes = g.capture_state("root").unwrap();
        assert_eq!(nodes.len(), 2);
        let encoded = serde_json::to_value(&nodes).unwrap().to_string();
        for field in ["parent", "states", "library", "part_id", "\"x\"", "\"y\""] {
            assert!(!encoded.contains(field));
        }
        let saved = Snapshot {
            slot: 1,
            name: "State 1".into(),
            nodes,
        };
        g.nodes[2].parent = None;
        g.nodes[1].label = "changed".into();
        let (restored, skipped) = g.restore_state("root", &saved).unwrap();
        assert_eq!(restored, vec!["inner"]);
        assert_eq!(skipped, vec!["v"]);
        assert_eq!(g.nodes[1].label, "Inner");
        assert!(g.nodes[1].states.is_some());
    }
    #[test]
    fn numbering_names_bounds_and_typed_selectors() {
        let mut b = Bank::default();
        assert_eq!(b.save(vec![]).unwrap(), 1);
        b.slots.clear();
        assert_eq!(b.save(vec![]).unwrap(), 2);
        b.slots[0].name = "2".into();
        assert!(b.select(&ControlValue::Text("2".into())).is_some());
        assert!(b.select(&ControlValue::Text("State 2".into())).is_none());
        for value in [0., -1., 2.5, f64::NAN, f64::INFINITY] {
            assert!(b.select(&ControlValue::Number(value)).is_none());
        }
        assert!(b.validate().is_ok());
        b.slots[0].name = " a ".into();
        assert!(b.validate().is_err());
        b.slots[0].name = "é".repeat(129);
        assert!(b.validate().is_err());
        b.slots[0].name = "State 2".into();
        for _ in 1..64 {
            b.save(vec![]).unwrap();
        }
        assert!(b.save(vec![]).is_err());
        b.slots[1].name = b.slots[0].name.clone();
        assert!(b.validate().is_err());
    }
    #[test]
    fn invalid_candidate_is_atomic_and_authored_merge_is_per_field() {
        let mut g = graph();
        let old = g.clone();
        let mut nodes = g.capture_state("root").unwrap();
        nodes[1].parameters.insert("bogus".into(), 1.);
        assert!(
            g.restore_state(
                "root",
                &Snapshot {
                    slot: 1,
                    name: "Bad".into(),
                    nodes
                }
            )
            .is_err()
        );
        assert_eq!(
            serde_json::to_value(&g).unwrap(),
            serde_json::to_value(&old).unwrap()
        );
        let mut live = old.clone();
        live.nodes[2].parameters.insert("value".into(), 99.);
        live.nodes[2].label = "recalled".into();
        g.nodes[2].label = "authored".into();
        let merged = reconcile(&old, &g, &live);
        assert_eq!(merged.nodes[2].label, "authored");
        assert_eq!(merged.nodes[2].parameters["value"], 99.);
        assert_eq!(g.nodes[2].parameters["value"], 12.);
    }
    #[test]
    fn saved_only_assets_and_node_references_remap() {
        let mut o = Options::capture(&graph().nodes[2]);
        o.kind = "sample".into();
        o.parameters = BTreeMap::from([("asset".into(), 7.)]);
        let mut b = Bank::default();
        b.save(vec![o]).unwrap();
        b.remap(
            &BTreeMap::from([("v".into(), "copy".into())]),
            &BTreeMap::from([(7, 18)]),
        );
        assert_eq!(b.slots[0].nodes[0].id, "copy");
        assert_eq!(b.slots[0].nodes[0].assets(), BTreeSet::from([18]));
    }
}

/// Explicit editor intent also replaces a recalled field when the authored value
/// happens to be unchanged. This mask is request metadata, never project data.
pub fn apply_fields(
    runtime: &mut Graph,
    authored: &Graph,
    edits: &BTreeMap<String, Vec<String>>,
) -> Result<(), String> {
    for (id, fields) in edits {
        let source = authored
            .nodes
            .iter()
            .find(|n| n.id == *id)
            .ok_or("Edited node missing")?;
        let target = runtime
            .nodes
            .iter_mut()
            .find(|n| n.id == *id)
            .ok_or("Edited node missing")?;
        for field in fields {
            if let Some(key) = field.strip_prefix("parameters.") {
                if let Some(value) = source.parameters.get(key) {
                    target.parameters.insert(key.into(), *value);
                } else {
                    target.parameters.remove(key);
                }
            } else {
                match field.as_str() {
                    "label" => target.label = source.label.clone(),
                    "channels" => target.channels = source.channels,
                    "control_value" => target.control_value = source.control_value.clone(),
                    "sample_choices" => target.sample_choices = source.sample_choices.clone(),
                    "script" => target.script = source.script.clone(),
                    "io" => target.io = source.io.clone(),
                    _ => return Err("Invalid runtime edit field".into()),
                }
            }
        }
    }
    Ok(())
}
