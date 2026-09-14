//! Ordered GUI piano events on the authenticated project socket, without HTTP RTTs.
use crate::{audio, can_edit, load, role, send, App};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
type Key = (String, String);
static OWNERS: OnceLock<Mutex<BTreeMap<Key, String>>> = OnceLock::new();
fn owners() -> &'static Mutex<BTreeMap<Key, String>> {
    OWNERS.get_or_init(Default::default)
}
#[derive(Default)]
struct Held {
    notes: BTreeSet<u8>,
    bent: bool,
}
pub struct Session {
    app: App,
    project: String,
    user: String,
    id: String,
    allowed: Option<BTreeMap<String, bool>>,
    held: BTreeMap<String, Held>,
    controls: Option<BTreeMap<String, (f64, f64, f64)>>,
    window: Instant,
    count: usize,
}
pub fn message(v: &Value) -> Result<pr0_core::midi::Message, String> {
    let note: crate::PianoNote =
        serde_json::from_value(v.clone()).map_err(|_| "Invalid piano event")?;
    match (note.pitch, note.velocity, note.bend) {
        (Some(pitch), Some(velocity), None) if pitch <= 127 && velocity <= 127 => {
            Ok(pr0_core::midi::Message {
                status: if velocity == 0 { 0x80 } else { 0x90 },
                data1: pitch,
                data2: velocity,
            })
        }
        (None, None, Some(bend)) if bend <= 16383 => Ok(pr0_core::midi::Message {
            status: 0xe0,
            data1: (bend & 127) as u8,
            data2: (bend >> 7) as u8,
        }),
        _ => Err("Invalid piano note or pitch bend".into()),
    }
}
impl Session {
    pub fn new(app: App, project: String, user: String, id: String) -> Self {
        Self {
            app,
            project,
            user,
            id,
            allowed: None,
            controls: None,
            held: BTreeMap::new(),
            window: Instant::now(),
            count: 0,
        }
    }
    pub fn invalidate(&mut self) {
        self.allowed = None;
        self.controls = None;
    }
    pub fn control(&mut self, v: &Value) -> Result<(), String> {
        let node = v["node"].as_str().ok_or("Control node required")?;
        let value = v["value"]
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or("Finite control value required")?;
        let revision = v["revision"].as_u64().ok_or("Control revision required")?;
        if self.controls.is_none() {
            role(&self.app, &self.project, &self.user)
                .and_then(|r| can_edit(&r))
                .map_err(|e| e.1)?;
            let p = load(&self.app, &self.project).map_err(|e| e.1)?;
            let graph = p.graph.flatten()?;
            self.controls = Some(
                graph
                    .nodes
                    .iter()
                    .filter(|n| n.kind == "control_input")
                    .map(|n| {
                        (
                            n.id.clone(),
                            (
                                *n.parameters.get("mode").unwrap_or(&2.),
                                *n.parameters.get("min").unwrap_or(&-100000.),
                                *n.parameters.get("max").unwrap_or(&100000.),
                            ),
                        )
                    })
                    .collect(),
            );
        }
        let &(mode, min, max) = self
            .controls
            .as_ref()
            .unwrap()
            .get(node)
            .ok_or("Control is unavailable")?;
        if !(1. ..=3.).contains(&mode)
            || value < min
            || value > max
            || (mode == 1. && value.fract() != 0.)
            || self.app.graph.lock().unwrap().as_deref() != Some(&self.project)
        {
            return Err("Invalid live control value or engine is disabled".into());
        }
        if self.window.elapsed() >= Duration::from_secs(1) {
            self.window = Instant::now();
            self.count = 0;
        }
        self.count += 1;
        if self.count > 2048 {
            return Err("Live control rate limit exceeded".into());
        }
        send(
            &self.app,
            audio::Command::LiveControl {
                project: self.project.clone(),
                node: node.into(),
                value,
                revision,
            },
        )
        .map_err(|e| e.1)
    }
    pub fn handle(&mut self, value: &Value) -> Result<(), String> {
        let node = value["node"]
            .as_str()
            .filter(|s| !s.is_empty() && s.len() <= 120)
            .ok_or("Invalid piano node")?;
        let message = message(value)?;
        if self.allowed.is_none() {
            let access = role(&self.app, &self.project, &self.user).and_then(|r| can_edit(&r));
            self.allowed = Some(if access.is_ok() {
                load(&self.app, &self.project)
                    .map_err(|e| e.1)?
                    .graph
                    .nodes
                    .iter()
                    .filter(|n| matches!(n.kind.as_str(), "piano" | "drum_pads"))
                    .map(|n| (n.id.clone(), n.kind == "piano"))
                    .collect()
            } else {
                BTreeMap::new()
            });
        }
        if !self
            .allowed
            .as_ref()
            .unwrap()
            .get(node)
            .is_some_and(|piano| *piano || message.status != 0xe0)
            || self.app.graph.lock().unwrap().as_deref() != Some(&self.project)
        {
            return Err(
                "Piano requires an enabled engine, a piano/drum node and editor access".into(),
            );
        }
        if self.window.elapsed() >= Duration::from_secs(1) {
            self.window = Instant::now();
            self.count = 0;
        }
        self.count += 1;
        if self.count > 2048 {
            return Err("Piano event rate exceeded; reconnect before playing".into());
        }
        let key = (self.project.clone(), node.to_string());
        let mut ownership = owners().lock().unwrap();
        if ownership.get(&key).is_some_and(|id| id != &self.id) {
            return Err("This piano currently has held notes in another session".into());
        }
        send(
            &self.app,
            audio::Command::Piano {
                project: self.project.clone(),
                node: node.into(),
                message,
            },
        )
        .map_err(|e| e.1)?;
        let held = self.held.entry(node.into()).or_default();
        match message.status {
            0x90 => {
                held.notes.insert(message.data1);
            }
            0x80 => {
                held.notes.remove(&message.data1);
            }
            0xe0 => held.bent = message.data1 != 0 || message.data2 != 64,
            _ => unreachable!(),
        }
        if held.notes.is_empty() && !held.bent {
            self.held.remove(node);
            ownership.remove(&key);
        } else {
            ownership.insert(key, self.id.clone());
        }
        Ok(())
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        let held = std::mem::take(&mut self.held);
        if held.is_empty() {
            return;
        }
        let (app, project, id) = (self.app.clone(), self.project.clone(), self.id.clone());
        // Cleanup may wait for queue space, but never on the socket or DSP thread.
        // Ownership remains until all ordered releases have been admitted.
        std::thread::spawn(move || {
            for (node, held) in held {
                let key = (project.clone(), node.clone());
                if owners().lock().unwrap().get(&key) != Some(&id) {
                    continue;
                }
                for pitch in held.notes {
                    let _ = app.engine.send(audio::Command::Piano {
                        project: project.clone(),
                        node: node.clone(),
                        message: pr0_core::midi::Message {
                            status: 0x80,
                            data1: pitch,
                            data2: 0,
                        },
                    });
                }
                if held.bent {
                    let _ = app.engine.send(audio::Command::Piano {
                        project: project.clone(),
                        node,
                        message: pr0_core::midi::Message {
                            status: 0xe0,
                            data1: 0,
                            data2: 64,
                        },
                    });
                }
                owners().lock().unwrap().remove(&key);
            }
        });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn validates_note_release_and_bend_packets() {
        assert_eq!(
            message(&serde_json::json!({"node":"p","pitch":60,"velocity":0}))
                .unwrap()
                .status,
            0x80
        );
        assert_eq!(
            message(&serde_json::json!({"node":"p","bend":16383}))
                .unwrap()
                .data2,
            127
        );
        for v in [
            serde_json::json!({"node":"p","pitch":128,"velocity":100}),
            serde_json::json!({"node":"p","bend":16384}),
            serde_json::json!({"node":"p","pitch":60,"velocity":100,"bend":0}),
        ] {
            assert!(message(&v).is_err());
        }
    }
}
