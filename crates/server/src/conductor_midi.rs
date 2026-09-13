//! MIDI learn and conducted control orchestration. Native callbacks reuse the
//! bounded graph MIDI rings; persistence, authorization and routing run here.
use crate::{App, audio, bad, can_conduct, load, publish, role, send, update_working_copy};
use pr0_core::{ConductedMidiBinding as Binding, Mode, Project};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::mpsc::{Receiver, SyncSender},
    time::{Duration, Instant},
};

pub struct Message {
    pub project: String,
    pub user: String,
    pub session: String,
    pub value: Value,
}
struct Local {
    user: String,
    session: String,
    devices: Vec<Value>,
    seen: Instant,
}
struct Learn {
    user: String,
    session: String,
    token: String,
    binding: Binding,
    started: Instant,
}
struct BindMode {
    project: String,
    session: String,
    seen: Instant,
}
#[derive(Default)]
struct State {
    mode: Option<BindMode>,
    learn: BTreeMap<String, Learn>,
    local: BTreeMap<String, Local>,
    selected: BTreeMap<String, String>,
    held: BTreeMap<(String, String, String, u8, u8), bool>,
}
fn bind_allowed(p: &Project, user: &str, r: &str) -> bool {
    can_conduct(p, user, r) || r == "editor"
}
fn local_allowed(p: &Project, user: &str, r: &str) -> bool {
    p.conductor.as_deref().map_or(r == "owner", |id| id == user)
}
fn event(app: &App, id: &str, user: Option<&str>, kind: &str, mut value: Value) {
    value["type"] = kind.into();
    value["project_id"] = id.into();
    if let Some(user) = user {
        value["user_id"] = user.into();
    }
    let _ = app.events.send(value.into());
}
fn inventory(app: &App, state: &State, id: &str, user: Option<&str>) {
    let local = state
        .local
        .get(id)
        .map(|local| local.devices.clone())
        .unwrap_or_default();
    let server = server_ports();
    event(
        app,
        id,
        user,
        "conductor_midi_devices",
        json!({"local":local,"server":server,"selected_set":state.selected.get(id)}),
    );
}
fn server_ports() -> Vec<String> {
    if std::env::var_os("PR0_DISABLE_NATIVE_DEVICES").is_some() {
        return vec![];
    }
    midir::MidiInput::new("pr0former conductor inventory")
        .ok()
        .map(|m| {
            m.ports()
                .iter()
                .filter_map(|p| m.port_name(p).ok())
                .take(32)
                .collect()
        })
        .unwrap_or_default()
}
fn save(app: &App, p: &mut Project) -> crate::Api<()> {
    p.validate().map_err(bad)?;
    update_working_copy(app, p)?;
    if app.graph.lock().unwrap().as_deref() == Some(&p.id) {
        send(
            app,
            audio::Command::ConductedConfig {
                project: p.id.clone(),
                layout: p.conducted.clone(),
                revision: p.revision,
            },
        )?;
    }
    publish(app, p);
    Ok(())
}
/// Normalize controls without losing their MIDI channel. Releases match the
/// original note-on control but can never become a new assignment.
fn signal(data: [u8; 3]) -> Option<(u8, u8, u8, bool)> {
    let [status, a, b] = data;
    if a > 127 || b > 127 {
        return None;
    }
    match status >> 4 {
        8 => Some((status + 16, a, 0, false)),
        9 => Some((status, a, b, b > 0)),
        10 => Some((status, a, b, true)),
        11 => Some((status, a, b, true)),
        12 => Some((status, a, 127, true)),
        13 => Some((status, 0, a, true)),
        14 => Some((status, 0, b, true)),
        _ => None,
    }
}
fn receive(
    app: &App,
    state: &mut State,
    id: &str,
    source: &str,
    device: &str,
    data: [u8; 3],
    local_user: Option<&str>,
) -> crate::Api<()> {
    let Some((status, control, value, learnable)) = signal(data) else {
        return Ok(());
    };
    let _setup = app.setup.blocking_lock();
    let mut p = load(app, id)?;
    if let Some(user) = local_user {
        let r = role(app, id, user)?;
        if !local_allowed(&p, user, &r) {
            return Err(bad("Local MIDI conductor assignment changed"));
        }
    }
    if p.mode != Mode::Conducted {
        return Ok(());
    }
    let key = (id.into(), source.into(), device.into(), status, control);
    let pressed = if status >> 4 == 9 {
        value > 0
    } else {
        value >= 64
    };
    let previous = state.held.get(&key).copied().unwrap_or(false);
    // Bound the transient edge cache even when arbitrary controls are sent.
    if state.held.len() >= 4096 {
        state.held.clear();
    }
    state.held.insert(key, pressed);
    if let Some(learn) = state.learn.get(id) {
        if learnable
            && (learn.binding.source == "any" || learn.binding.source == source)
            && (learn.binding.device.is_empty() || learn.binding.device == device)
        {
            let learn = state.learn.remove(id).unwrap();
            let r = role(app, id, &learn.user)?;
            if !bind_allowed(&p, &learn.user, &r) {
                return Err(bad("MIDI binding authority changed"));
            }
            let mut binding = learn.binding;
            binding.source = source.into();
            binding.device = device.into();
            binding.status = status;
            binding.control = control;
            if let Err(error) = binding.validate(&p) {
                event(
                    app,
                    id,
                    Some(&learn.user),
                    "conductor_midi_learned",
                    json!({"token":learn.token,"error":error}),
                );
                return Ok(());
            }
            p.conducted.midi_bindings.retain(|b| {
                !(b.action == binding.action && b.target == binding.target)
                    && !(b.source == binding.source
                        && b.device == binding.device
                        && b.status == binding.status
                        && b.control == binding.control)
            });
            p.conducted.midi_bindings.push(binding.clone());
            let result = save(app, &mut p);
            event(
                app,
                id,
                Some(&learn.user),
                "conductor_midi_learned",
                json!({"token":learn.token,"binding":binding,"error":result.as_ref().err().map(|e|&e.1)}),
            );
            result?;
        }
        // Learn is exclusive: moving a previously mapped control cannot cue.
        return Ok(());
    }
    if state.mode.as_ref().is_some_and(|mode| mode.project == id) {
        return Ok(());
    }
    for b in &p.conducted.midi_bindings {
        if b.source != source || b.device != device || b.status != status || b.control != control {
            continue;
        }
        let continuous = matches!(b.action.as_str(), "select_set" | "dynamic");
        if !continuous && (!pressed || (previous && status >> 4 != 12)) {
            continue;
        }
        if matches!(b.action.as_str(), "set" | "select_set" | "next_set") {
            let sets = &p.conducted.sets;
            if sets.is_empty() {
                continue;
            }
            let index = match b.action.as_str() {
                "set" => sets.iter().position(|s| s.id == b.target).unwrap_or(0),
                "select_set" => (value as usize * sets.len() / 128).min(sets.len() - 1),
                _ => {
                    (sets
                        .iter()
                        .position(|s| Some(&s.id) == state.selected.get(id))
                        .unwrap_or(0)
                        + 1)
                        % sets.len()
                }
            };
            state.selected.insert(id.into(), sets[index].id.clone());
            event(
                app,
                id,
                None,
                "conductor_midi_set",
                json!({"selected_set":sets[index].id}),
            );
            continue;
        }
        if app.active.lock().unwrap().as_deref() != Some(id) {
            continue;
        }
        match b.action.as_str() {
            "part" | "arm" => send(
                app,
                audio::Command::ConductedToggle {
                    project: id.into(),
                    part: b.target.clone(),
                    arm: b.action == "arm",
                },
            )?,
            "dynamic" => send(
                app,
                audio::Command::Dynamics {
                    parts: vec![],
                    value: Some(value),
                },
            )?,
            "play" | "repeat" | "stop" => send(
                app,
                audio::Command::Cue {
                    request_id: None,
                    parts: if b.action == "stop" {
                        p.parts.iter().map(|p| p.id.clone()).collect()
                    } else {
                        vec![]
                    },
                    playing: b.action != "stop",
                    repeat: b.action == "repeat",
                    count_in_pulses: if b.action == "stop" {
                        0
                    } else {
                        p.conducted.count_in_pulses
                    },
                },
            )?,
            _ => (),
        }
    }
    Ok(())
}
fn handle(app: &App, state: &mut State, m: &Message) -> crate::Api<()> {
    let v = &m.value;
    let id = &m.project;
    if matches!(
        v["type"].as_str(),
        Some("conductor_midi_disconnect" | "conductor_midi_release")
    ) {
        state.learn.retain(|_, l| l.session != m.session);
        if state
            .mode
            .as_ref()
            .is_some_and(|mode| mode.session == m.session)
        {
            state.mode = None;
        }
        if state
            .local
            .get(id)
            .is_some_and(|local| local.session == m.session)
        {
            state.local.remove(id);
            state
                .held
                .retain(|(p, source, _, _, _), _| p != id || source != "local");
            inventory(app, state, id, None);
        }
        return Ok(());
    }
    if v["type"] == "conductor_midi_heartbeat" {
        if let Some(mode) = &mut state.mode {
            if mode.session == m.session {
                mode.seen = Instant::now();
            }
        }
        if let Some(local) = state.local.get_mut(id) {
            if local.session == m.session {
                local.seen = Instant::now();
            }
        }
        return Ok(());
    }
    let p = load(app, id)?;
    let r = role(app, id, &m.user)?;
    if p.mode != Mode::Conducted {
        return Err(bad("MIDI bindings require a conducted project"));
    }
    match v["type"].as_str().unwrap_or("") {
        "conductor_midi_mode" => {
            if v["enabled"] == false {
                if state
                    .mode
                    .as_ref()
                    .is_some_and(|mode| mode.session == m.session)
                {
                    state.mode = None;
                    state.learn.retain(|_, l| l.session != m.session);
                }
            } else {
                if !bind_allowed(&p, &m.user, &r) {
                    return Err(bad("Conductor or editor access required"));
                }
                if state
                    .mode
                    .as_ref()
                    .is_some_and(|mode| mode.session != m.session)
                {
                    return Err(bad("Another editor is binding MIDI"));
                }
                state.mode = Some(BindMode {
                    project: id.clone(),
                    session: m.session.clone(),
                    seen: Instant::now(),
                });
            }
        }
        "conductor_midi_inventory" => {
            if bind_allowed(&p, &m.user, &r) {
                inventory(app, state, id, Some(&m.user));
            }
        }
        "conductor_midi_local_devices" => {
            if !local_allowed(&p, &m.user, &r) {
                return Err(bad("Only the designated conductor supplies local MIDI"));
            }
            let devices = v["devices"]
                .as_array()
                .filter(|ds| {
                    ds.len() <= 32
                        && ds.iter().all(|d| {
                            d["id"]
                                .as_str()
                                .is_some_and(|s| !s.is_empty() && s.len() <= 256)
                                && d["name"].as_str().is_some_and(|s| s.len() <= 256)
                        })
                })
                .ok_or_else(|| bad("Invalid MIDI devices"))?;
            if state
                .local
                .get(id)
                .is_some_and(|local| local.user == m.user && local.session != m.session)
            {
                return Err(bad(
                    "Local MIDI is already connected in another conductor window",
                ));
            }
            state.local.insert(
                id.clone(),
                Local {
                    user: m.user.clone(),
                    session: m.session.clone(),
                    devices: devices.clone(),
                    seen: Instant::now(),
                },
            );
            state
                .held
                .retain(|(p, source, _, _, _), _| p != id || source != "local");
            inventory(app, state, id, None);
            event(
                app,
                id,
                Some(&m.user),
                "conductor_midi_local_status",
                json!({"session_id":m.session,"connected":true}),
            );
        }
        "conductor_midi_data" => {
            if !local_allowed(&p, &m.user, &r) {
                return Err(bad("Only the designated conductor supplies local MIDI"));
            }
            let device = v["device"]
                .as_str()
                .ok_or_else(|| bad("MIDI device missing"))?;
            if !state.local.get(id).is_some_and(|local| {
                local.user == m.user
                    && local.session == m.session
                    && local.devices.iter().any(|d| d["id"] == device)
            }) {
                return Err(bad("Connect the conductor's MIDI devices first"));
            }
            if !v["data"]
                .as_array()
                .is_some_and(|data| matches!(data.len(), 2 | 3))
            {
                return Err(bad("Invalid MIDI data length"));
            }
            let bytes: Vec<u8> =
                serde_json::from_value(v["data"].clone()).map_err(|_| bad("Invalid MIDI data"))?;
            if let Some(data) = crate::node_io::channel_message(&bytes) {
                receive(app, state, id, "local", device, data, Some(&m.user))?;
            }
        }
        "conductor_midi_cancel" => {
            if state.learn.get(id).is_some_and(|l| l.session == m.session) {
                state.learn.remove(id);
            }
        }
        "conductor_midi_set" => {
            if !bind_allowed(&p, &m.user, &r) {
                return Err(bad("Conductor or editor access required"));
            }
            let selected = v["selected_set"]
                .as_str()
                .filter(|id| p.conducted.sets.iter().any(|s| s.id == *id))
                .ok_or_else(|| bad("Set missing"))?;
            state.selected.insert(id.clone(), selected.into());
            event(
                app,
                id,
                None,
                "conductor_midi_set",
                json!({"selected_set":selected}),
            );
        }
        "conductor_midi_learn" => {
            if !state
                .mode
                .as_ref()
                .is_some_and(|mode| mode.session == m.session && mode.project == *id)
            {
                return Err(bad("Enter Bind MIDI before choosing a control"));
            }
            if !bind_allowed(&p, &m.user, &r) {
                return Err(bad("Conductor or editor access required"));
            }
            if state.learn.get(id).is_some_and(|l| l.session != m.session) {
                return Err(bad("Another editor is binding MIDI"));
            }
            let token = v["token"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() <= 128)
                .ok_or_else(|| bad("Invalid MIDI learn token"))?;
            let binding: Binding = serde_json::from_value(v["binding"].clone())
                .map_err(|_| bad("Invalid MIDI learn target"))?;
            let mut check = binding.clone();
            check.source = "local".into();
            check.device = "check".into();
            check.status = 0xb0;
            check.control = 0;
            check.validate(&p).map_err(bad)?;
            if !matches!(binding.source.as_str(), "any" | "local" | "server")
                || binding.device.len() > 256
            {
                return Err(bad("Invalid MIDI source"));
            }
            state.learn.insert(
                id.clone(),
                Learn {
                    user: m.user.clone(),
                    session: m.session.clone(),
                    token: token.into(),
                    binding,
                    started: Instant::now(),
                },
            );
            event(
                app,
                id,
                Some(&m.user),
                "conductor_midi_listening",
                json!({"token":token}),
            );
        }
        "conductor_midi_remove" | "conductor_midi_device" => {
            if !bind_allowed(&p, &m.user, &r) {
                return Err(bad("Conductor or editor access required"));
            }
            let _setup = app.setup.blocking_lock();
            let mut p = load(app, id)?;
            let action = v["action"].as_str().unwrap_or("");
            let target = v["target"].as_str().unwrap_or("");
            if v["type"] == "conductor_midi_remove" {
                p.conducted
                    .midi_bindings
                    .retain(|b| b.action != action || b.target != target);
            } else {
                let b = p
                    .conducted
                    .midi_bindings
                    .iter_mut()
                    .find(|b| b.action == action && b.target == target)
                    .ok_or_else(|| bad("Binding missing"))?;
                b.source = v["source"].as_str().unwrap_or("").into();
                b.device = v["device"].as_str().unwrap_or("").into();
            }
            save(app, &mut p)?;
            state.held.retain(|(p, _, _, _, _), _| p != id);
        }
        _ => (),
    }
    Ok(())
}
pub fn start(app: App, rx: Receiver<Message>) {
    std::thread::spawn(move || {
        let mut state = State::default();
        let mut inputs = BTreeMap::<String, crate::node_io::Input>::new();
        let mut ports_at = Instant::now() - Duration::from_secs(2);
        let mut native_project = String::new();
        let mut mode_session = String::new();
        loop {
            match rx.recv_timeout(Duration::from_millis(5)) {
                Ok(m) => {
                    if let Err(e) = handle(&app, &mut state, &m) {
                        event(
                            &app,
                            &m.project,
                            Some(&m.user),
                            "conductor_midi_error",
                            json!({"error":e.1,"session_id":m.session}),
                        );
                        if matches!(
                            m.value["type"].as_str(),
                            Some("conductor_midi_local_devices" | "conductor_midi_data")
                        ) {
                            event(
                                &app,
                                &m.project,
                                Some(&m.user),
                                "conductor_midi_local_status",
                                json!({"session_id":m.session,"connected":false,"error":e.1}),
                            );
                        }
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                Err(_) => (),
            }
            let expired: Vec<_> = state
                .local
                .iter()
                .filter(|(_, local)| local.seen.elapsed() > Duration::from_secs(35))
                .map(|(id, _)| id.clone())
                .collect();
            for id in expired {
                state.local.remove(&id);
                state
                    .held
                    .retain(|(p, source, _, _, _), _| p != &id || source != "local");
                inventory(&app, &state, &id, None);
            }
            if state
                .mode
                .as_ref()
                .is_some_and(|mode| mode.seen.elapsed() > Duration::from_secs(35))
            {
                if let Some(mode) = state.mode.take() {
                    state.learn.retain(|_, l| l.session != mode.session);
                }
            }
            let next_mode = state
                .mode
                .as_ref()
                .map(|m| m.session.clone())
                .unwrap_or_default();
            if next_mode != mode_session {
                mode_session = next_mode;
                ports_at = Instant::now() - Duration::from_secs(2);
            }
            state.learn.retain(|id, l| {
                if l.started.elapsed() < Duration::from_secs(120) {
                    true
                } else {
                    event(
                        &app,
                        id,
                        Some(&l.user),
                        "conductor_midi_learned",
                        json!({"token":l.token,"error":"MIDI learn timed out"}),
                    );
                    false
                }
            });
            // One engine/project owns the native control ports; a pending learn
            // may temporarily open ports before the engine is enabled.
            let project = state
                .mode
                .as_ref()
                .map(|m| m.project.clone())
                .or_else(|| app.graph.lock().unwrap().clone())
                .unwrap_or_default();
            if native_project != project {
                inputs.clear();
                native_project = project.clone();
                ports_at = Instant::now() - Duration::from_secs(2);
            }
            if ports_at.elapsed() >= Duration::from_secs(1) {
                ports_at = Instant::now();
                let available = server_ports();
                let mut wanted = load(&app, &project)
                    .ok()
                    .map(|p| {
                        p.conducted
                            .midi_bindings
                            .into_iter()
                            .filter(|b| b.source == "server")
                            .map(|b| b.device)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                if state
                    .mode
                    .as_ref()
                    .is_some_and(|mode| mode.project == project)
                {
                    wanted.extend(available.clone());
                }
                if let Some(l) = state.learn.get(&project) {
                    if l.binding.source != "local" {
                        if l.binding.device.is_empty() {
                            wanted.extend(available.clone());
                        } else {
                            wanted.push(l.binding.device.clone());
                        }
                    }
                }
                inputs.retain(|port, _| wanted.contains(port) && available.contains(port));
                for port in wanted {
                    if !inputs.contains_key(&port) {
                        match crate::node_io::open_input(&port) {
                            Ok(input) => {
                                inputs.insert(port, input);
                            }
                            Err(error) => event(
                                &app,
                                &project,
                                None,
                                "conductor_midi_error",
                                json!({"error":error}),
                            ),
                        }
                    }
                }
            }
            for (port, input) in &mut inputs {
                for _ in 0..256 {
                    match input.control_message() {
                        Ok(Some(data)) => {
                            if let Err(e) =
                                receive(&app, &mut state, &project, "server", port, data, None)
                            {
                                event(
                                    &app,
                                    &project,
                                    None,
                                    "conductor_midi_error",
                                    json!({"error":e.1}),
                                );
                            }
                        }
                        Ok(None) => break,
                        Err(error) => {
                            state.held.retain(|(_, source, device, _, _), _| {
                                source != "server" || device != port
                            });
                            event(
                                &app,
                                &project,
                                None,
                                "conductor_midi_error",
                                json!({"error":error}),
                            );
                            break;
                        }
                    }
                }
            }
        }
    });
}
pub fn enqueue(
    tx: &SyncSender<Message>,
    project: &str,
    user: &str,
    session: &str,
    value: Value,
) -> Result<(), String> {
    tx.try_send(Message {
        project: project.into(),
        user: user.into(),
        session: session.into(),
        value,
    })
    .map_err(|_| "Conductor MIDI queue is busy; try again".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn messages_preserve_channels_and_ignore_clock_and_releases_for_learn() {
        assert_eq!(signal([0x92, 60, 100]), Some((0x92, 60, 100, true)));
        assert_eq!(signal([0x82, 60, 64]), Some((0x92, 60, 0, false)));
        assert_eq!(signal([0x92, 60, 0]), Some((0x92, 60, 0, false)));
        assert_eq!(signal([0xb7, 12, 0]), Some((0xb7, 12, 0, true)));
        assert_eq!(signal([0xe2, 127, 64]), Some((0xe2, 0, 64, true)));
        assert_eq!(signal([0xf8, 0, 0]), None);
        assert_eq!(signal([0x90, 255, 64]), None);
    }
    #[test]
    fn only_designated_conductor_supplies_local_devices() {
        let mut p = pr0_core::demo_project("id".into(), "test".into(), Mode::Conducted);
        p.conductor = Some("player".into());
        assert!(local_allowed(&p, "player", "performer"));
        assert!(bind_allowed(&p, "editor", "editor"));
        assert!(!local_allowed(&p, "editor", "editor"));
        assert!(!local_allowed(&p, "owner", "owner"));
        assert!(!bind_allowed(&p, "other", "performer"));
    }
}
