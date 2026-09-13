//! Authenticated, per-socket local MIDI source ownership. No device IDs are saved.
use crate::{App, audio, browser_midi_message, can_edit, load, role, send};
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

pub struct Session {
    app: App,
    project: String,
    user: String,
    id: String,
    allowed: Option<BTreeSet<String>>,
    held: BTreeSet<String>,
    window: Instant,
    count: usize,
}
impl Session {
    pub fn new(app: App, project: String, user: String, id: String) -> Self {
        Self {
            app,
            project,
            user,
            id,
            allowed: None,
            held: BTreeSet::new(),
            window: Instant::now(),
            count: 0,
        }
    }
    pub fn invalidate(&mut self) {
        self.allowed = None;
    }
    pub fn release(&mut self, node: &str) {
        let key = (self.project.clone(), node.to_string());
        let mut owners = owners().lock().unwrap();
        if owners.get(&key) == Some(&self.id) {
            // Queue reset before making this source available to another socket.
            let command = audio::Command::LocalMidi {
                project: self.project.clone(),
                node: node.into(),
                message: None,
            };
            if send(&self.app, command).is_err() {
                return;
            }
            owners.remove(&key);
        }
        self.held.remove(node);
    }
    pub fn handle(&mut self, value: &Value) -> Result<bool, String> {
        let node = value["node"]
            .as_str()
            .filter(|n| n.len() <= 120)
            .ok_or("Invalid local MIDI node")?;
        if value["type"] == "local_midi_reset" {
            self.release(node);
            return Ok(false);
        }
        if self.allowed.is_none() {
            let access = role(&self.app, &self.project, &self.user).and_then(|r| can_edit(&r));
            self.allowed = Some(if access.is_ok() {
                load(&self.app, &self.project)
                    .map_err(|e| e.1)?
                    .graph
                    .nodes
                    .iter()
                    .filter(|n| n.kind == "local_midi_input")
                    .map(|n| n.id.clone())
                    .collect()
            } else {
                BTreeSet::new()
            });
        }
        if !self.allowed.as_ref().unwrap().contains(node)
            || self.app.graph.lock().unwrap().as_deref() != Some(&self.project)
        {
            self.release(node);
            return Err("Enable this project's engine and select a Local MIDI Input node with editor access.".into());
        }
        if self.window.elapsed() >= Duration::from_secs(1) {
            self.window = Instant::now();
            self.count = 0;
        }
        self.count += 1;
        if self.count > 2048 {
            self.release(node);
            return Err("Local MIDI rate limit exceeded; reconnect the input.".into());
        }
        let message = if value["type"] == "local_midi" {
            Some(browser_midi_message(value).ok_or("Invalid MIDI channel message")?)
        } else {
            None
        };
        let key = (self.project.clone(), node.to_string());
        let mut owners = owners().lock().unwrap();
        if owners.get(&key).is_some_and(|owner| owner != &self.id) {
            return Err("This Local MIDI Input is connected in another session.".into());
        }
        if let Some(message) = message {
            send(
                &self.app,
                audio::Command::LocalMidi {
                    project: self.project.clone(),
                    node: node.into(),
                    message: Some(message),
                },
            )
            .map_err(|e| e.1)?;
        }
        owners.insert(key, self.id.clone());
        self.held.insert(node.into());
        Ok(true)
    }
}
impl Drop for Session {
    fn drop(&mut self) {
        // A disconnected socket can no longer retry a busy queue. Finish its resets
        // on a separate thread; keep ownership until each ordered reset is admitted.
        let nodes = std::mem::take(&mut self.held);
        if nodes.is_empty() {
            return;
        }
        let (app, project, id) = (self.app.clone(), self.project.clone(), self.id.clone());
        std::thread::spawn(move || {
            for node in nodes {
                let key = (project.clone(), node.clone());
                let mut owners = owners().lock().unwrap();
                if owners.get(&key) == Some(&id) {
                    let _ = app.engine.send(audio::Command::LocalMidi {
                        project: project.clone(),
                        node,
                        message: None,
                    });
                    owners.remove(&key);
                }
            }
        });
    }
}
