//! Persisted script source and the manifest derived by the server's JS runtime.
use crate::{Descriptor, Node, Port, Signal};
use serde::{Deserialize, Serialize};

pub const MAX_SCRIPTS: usize = 16;
pub const MAX_SOURCE: usize = 64 * 1024;
pub const MAX_PORTS: usize = 8;
pub const MAX_BINDINGS: usize = 16;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Script {
    #[serde(default)]
    pub revision: u64,
    pub source: String,
    #[serde(default)]
    pub inputs: Vec<Input>,
    #[serde(default)]
    pub outputs: Vec<Input>,
    #[serde(default)]
    pub bindings: Vec<Binding>,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub name: String,
    pub initial: f64,
}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    /// send: observe a named Send control; receive: observe Receive control;
    /// publish: a script-owned virtual Send control, consumed by Receive nodes.
    pub kind: String,
    pub name: String,
}
impl Script {
    pub fn validate(&self) -> Result<(), String> {
        if self.source.len() > MAX_SOURCE {
            return Err("Script source exceeds 64 KiB".into());
        }
        for ports in [&self.inputs, &self.outputs] {
            if ports.len() > MAX_PORTS {
                return Err("Scripts support up to eight numeric ports per direction".into());
            }
            let mut names = std::collections::BTreeSet::new();
            for port in ports {
                if !valid_name(&port.name)
                    || port.name == "midi"
                    || !names.insert(&port.name)
                    || !port.initial.is_finite()
                {
                    return Err("Script port names must be unique identifiers (1–48 bytes), cannot be midi, and require finite defaults".into());
                }
            }
        }
        if self.bindings.len() > MAX_BINDINGS
            || self.bindings.iter().any(|b| {
                !matches!(b.kind.as_str(), "send" | "receive" | "publish")
                    || b.name.is_empty()
                    || b.name.len() > 256
            })
        {
            return Err(
                "Scripts support sixteen named control bindings with names of 1–256 bytes".into(),
            );
        }
        Ok(())
    }
}
fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 48
        && name.bytes().enumerate().all(|(i, b)| {
            b == b'_' || b.is_ascii_alphabetic() || (i > 0 && (b.is_ascii_digit() || b == b'-'))
        })
}
pub fn descriptor(node: &Node, base: &Descriptor) -> Descriptor {
    let mut d = base.clone();
    if node.kind == "js_control" {
        let ports = |items: &[Input]| {
            items
                .iter()
                .map(|p| Port {
                    id: p.name.clone(),
                    label: p.name.clone(),
                    signal: Signal::Control,
                    fixed_channels: None,
                })
                .chain(std::iter::once(Port {
                    id: "midi".into(),
                    label: "MIDI".into(),
                    signal: Signal::Midi,
                    fixed_channels: None,
                }))
                .collect()
        };
        let empty = Script::default();
        let script = node.script.as_ref().unwrap_or(&empty);
        d.inputs = ports(&script.inputs);
        d.outputs = ports(&script.outputs);
    }
    d
}
