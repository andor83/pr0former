//! UDP I/O and configuration run outside DSP and device callbacks.
use crate::{Api, App, bad, csrf, internal, role, user};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    net::{Ipv4Addr, SocketAddr, ToSocketAddrs, UdpSocket},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Settings {
    pub receive_enabled: bool,
    pub bind_addresses: Vec<Ipv4Addr>,
    pub receive_port: u16,
    pub send_enabled: bool,
    pub send_address: Ipv4Addr,
    pub send_port: u16,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            receive_enabled: false,
            bind_addresses: vec![Ipv4Addr::LOCALHOST],
            receive_port: 9000,
            send_enabled: true,
            send_address: Ipv4Addr::UNSPECIFIED,
            send_port: 0,
        }
    }
}
impl Settings {
    fn validate(&self) -> Result<(), String> {
        if self.receive_port == 0
            || self.bind_addresses.len() > 64
            || (self.receive_enabled && self.bind_addresses.is_empty())
        {
            return Err("Select receive interfaces and a port from 1–65535".into());
        }
        let unique: std::collections::HashSet<_> = self.bind_addresses.iter().collect();
        if unique.len() != self.bind_addresses.len()
            || (self.bind_addresses.len() > 1
                && self.bind_addresses.contains(&Ipv4Addr::UNSPECIFIED))
        {
            return Err(
                "Choose all interfaces (0.0.0.0) or individual interfaces, without duplicates"
                    .into(),
            );
        }
        Ok(())
    }
}
struct Sockets {
    receive: Vec<UdpSocket>,
    send: Option<UdpSocket>,
}
struct StateData {
    settings: Settings,
    sockets: Sockets,
    error: Option<String>,
    received: u64,
    rejected: u64,
    sent: u64,
}
pub struct Runtime {
    state: Mutex<StateData>,
    destinations: Mutex<std::collections::BTreeMap<String, (Instant, Result<SocketAddr, String>)>>,
    /// Where configuration changes are persisted. `None` for the unconfigured
    /// runtime used by tests, which binds sockets but owns no host paths.
    settings_path: Option<std::path::PathBuf>,
}
impl Default for Runtime {
    fn default() -> Self {
        let settings = Settings::default();
        let send = UdpSocket::bind((settings.send_address, 0)).ok();
        if let Some(socket) = &send {
            let _ = socket.set_nonblocking(true);
        }
        Self {
            settings_path: None,
            destinations: Mutex::new(std::collections::BTreeMap::new()),
            state: Mutex::new(StateData {
                settings,
                sockets: Sockets {
                    receive: vec![],
                    send,
                },
                error: None,
                received: 0,
                rejected: 0,
                sent: 0,
            }),
        }
    }
}
impl Runtime {
    pub fn load(config: &crate::config::RuntimeConfig) -> Arc<Self> {
        let path = config.osc_settings_path();
        let runtime = Arc::new(Self {
            settings_path: Some(path.clone()),
            ..Self::default()
        });
        if let Ok(bytes) = std::fs::read(&path) {
            match serde_json::from_slice::<Settings>(&bytes)
                .map_err(|e| e.to_string())
                .and_then(|s| runtime.prepare(&s).map(|sockets| (s, sockets)))
            {
                Ok((s, sockets)) => runtime.apply(s, sockets),
                Err(error) => {
                    runtime.state.lock().unwrap().error =
                        Some(format!("Saved OSC configuration could not start: {error}"))
                }
            }
        }
        runtime
    }
    fn prepare(&self, s: &Settings) -> Result<Sockets, String> {
        Self::bind(s)
    }
    fn bind(s: &Settings) -> Result<Sockets, String> {
        s.validate()?;
        let socket = |ip: Ipv4Addr, port: u16| -> Result<UdpSocket, String> {
            let socket = UdpSocket::bind((ip, port))
                .map_err(|e| format!("Cannot bind OSC {ip}:{port}: {e}"))?;
            socket.set_nonblocking(true).map_err(|e| e.to_string())?;
            Ok(socket)
        };
        let receive = if s.receive_enabled {
            s.bind_addresses
                .iter()
                .map(|ip| socket(*ip, s.receive_port))
                .collect::<Result<Vec<_>, _>>()?
        } else {
            vec![]
        };
        let send = if s.send_enabled {
            Some(socket(s.send_address, s.send_port)?)
        } else {
            None
        };
        Ok(Sockets { receive, send })
    }
    fn configure(&self, s: Settings) -> Result<(), String> {
        s.validate()?;
        let mut state = self.state.lock().unwrap();
        let previous = state.settings.clone();
        state.sockets = Sockets {
            receive: vec![],
            send: None,
        };
        let result = Self::bind(&s).and_then(|sockets| {
            if let Some(path) = &self.settings_path {
                let temp = path.with_extension("tmp");
                std::fs::write(
                    &temp,
                    serde_json::to_vec_pretty(&s).map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;
                std::fs::rename(temp, path).map_err(|e| e.to_string())?;
            }
            Ok(sockets)
        });
        match result {
            Ok(sockets) => {
                state.sockets = sockets;
                state.settings = s;
                state.error = None;
                Ok(())
            }
            Err(error) => {
                match Self::bind(&previous) {
                    Ok(sockets) => state.sockets = sockets,
                    Err(rollback) => {
                        state.error = Some(format!(
                            "Previous OSC bindings could not be restored: {rollback}"
                        ))
                    }
                }
                Err(error)
            }
        }
    }
    fn apply(&self, settings: Settings, sockets: Sockets) {
        let mut state = self.state.lock().unwrap();
        state.settings = settings;
        state.sockets = sockets;
        state.error = None;
    }
    pub fn reject(&self) {
        self.state.lock().unwrap().rejected += 1;
    }
    /// Called only by the external node output worker, never by render or callbacks.
    pub fn send_host(&self, bytes: &[u8], destination: &str) -> Result<(), String> {
        let address = if let Ok(address) = destination.parse::<SocketAddr>() {
            address
        } else {
            let mut cache = self.destinations.lock().unwrap();
            let now = Instant::now();
            if !cache
                .get(destination)
                .is_some_and(|(until, _)| *until > now)
            {
                let result = destination
                    .to_socket_addrs()
                    .map_err(|error| {
                        format!("Cannot resolve OSC destination {destination}: {error}")
                    })
                    .and_then(|mut addresses| {
                        addresses.find(SocketAddr::is_ipv4).ok_or_else(|| {
                            format!("OSC destination {destination} has no IPv4 address")
                        })
                    });
                let ttl = if result.is_ok() { 60 } else { 5 };
                if cache.len() >= 128 {
                    cache.clear();
                }
                cache.insert(
                    destination.into(),
                    (Instant::now() + Duration::from_secs(ttl), result),
                );
            }
            cache[destination].1.clone()?
        };
        self.send(bytes, address);
        Ok(())
    }
    pub fn send(&self, bytes: &[u8], destination: SocketAddr) {
        let mut state = self.state.lock().unwrap();
        if let Some(socket) = &state.sockets.send {
            match socket.send_to(bytes, destination) {
                Ok(_) => state.sent += 1,
                Err(e) => state.error = Some(format!("OSC send to {destination}: {e}")),
            }
        }
    }
    fn snapshot(&self) -> Value {
        let state = self.state.lock().unwrap();
        let interfaces = if_addrs::get_if_addrs();
        let interface_error = interfaces.as_ref().err().map(ToString::to_string);
        let mut interfaces: Vec<Value> = interfaces
            .unwrap_or_default()
            .into_iter()
            .filter_map(|i| match i.ip() {
                std::net::IpAddr::V4(ip) => Some(json!({"name":i.name,"address":ip})),
                _ => None,
            })
            .collect();
        interfaces.sort_by_key(|i| i["address"].to_string());
        interfaces.dedup_by(|a, b| a["address"] == b["address"]);
        json!({"settings":state.settings,"interfaces":interfaces,"interface_error":interface_error,"error":state.error,"receiving":state.sockets.receive.iter().filter_map(|s|s.local_addr().ok()).collect::<Vec<_>>(),"sending_from":state.sockets.send.as_ref().and_then(|s|s.local_addr().ok()),"received":state.received,"rejected":state.rejected,"sent":state.sent})
    }
    pub fn listen(self: &Arc<Self>, app: &App) {
        let runtime = self.clone();
        let active = app.graph.clone();
        let tx = app.engine.clone();
        std::thread::Builder::new()
            .name("pr0-osc-input".into())
            .spawn(move || {
                let mut buffer = [0u8; 8192];
                loop {
                    let mut messages = vec![];
                    {
                        let mut state = runtime.state.lock().unwrap();
                        let mut received = 0;
                        let mut rejected = 0;
                        let mut error = None;
                        for socket in &state.sockets.receive {
                            for _ in 0..32 {
                                if received >= 32 {
                                    break;
                                }
                                match socket.recv_from(&mut buffer) {
                                    Ok((n, _)) => {
                                        received += 1;
                                        match if buffer.first() == Some(&b'/') {
                                            rosc::decoder::decode_udp(&buffer[..n]).ok()
                                        } else {
                                            None
                                        } {
                                            Some((rest, rosc::OscPacket::Message(m)))
                                                if rest.is_empty()
                                                    && m.addr.starts_with('/') && m.addr.len() <= 256 =>
                                            {
                                                messages.push(m)
                                            }
                                            _ => rejected += 1,
                                        }
                                    }
                                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                                    Err(e) => {
                                        error = Some(e.to_string());
                                        break;
                                    }
                                }
                            }
                        }
                        state.received += received;
                        state.rejected += rejected;
                        if error.is_some() {
                            state.error = error;
                        }
                    }
                    if let Some(project) = active.lock().unwrap().clone() {
                        for message in messages {
                            if tx
                                .try_send(crate::audio::Command::Osc {
                                    project: project.clone(),
                                    message,
                                })
                                .is_err()
                            {
                                runtime.state.lock().unwrap().rejected += 1;
                            }
                        }
                    } else {
                        runtime.state.lock().unwrap().rejected += messages.len() as u64;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
            })
            .expect("Start OSC receiver");
    }
}
pub enum Action {
    Transport(&'static str),
    Tempo(f64),
    Control(String, pr0_core::ControlValue),
    Bang(String),
}
pub fn number(v: &rosc::OscType) -> Option<f64> {
    let n = match v {
        rosc::OscType::Int(n) => *n as f64,
        rosc::OscType::Float(n) => *n as f64,
        rosc::OscType::Double(n) => *n,
        _ => return None,
    };
    n.is_finite().then_some(n)
}
pub fn action(m: &rosc::OscMessage) -> Option<Action> {
    match m.addr.as_str() {
        "/pr0former/play" | "/pr0former/pause" | "/pr0former/stop" if m.args.is_empty() => {
            Some(Action::Transport(match m.addr.as_str() {
                "/pr0former/play" => "play",
                "/pr0former/pause" => "pause",
                _ => "stop",
            }))
        }
        "/pr0former/tempo" if m.args.len() == 1 => number(&m.args[0])
            .filter(|n| (1.0..=400.0).contains(n))
            .map(Action::Tempo),
        _ => {
            let path = m.addr.strip_prefix("/pr0former/node/")?;
            let (node, op) = path.split_once('/')?;
            if node.is_empty() || node.len() > 128 {
                return None;
            }
            match (op, m.args.as_slice()) {
                ("control", [rosc::OscType::String(s)]) if s.len() <= 256 => Some(Action::Control(
                    node.into(),
                    pr0_core::ControlValue::Text(s.clone()),
                )),
                ("control", [v]) => number(v)
                    .map(|n| Action::Control(node.into(), pr0_core::ControlValue::Number(n))),
                ("bang", []) => Some(Action::Bang(node.into())),
                _ => None,
            }
        }
    }
}
pub async fn get(State(app): State<App>, headers: HeaderMap) -> Api<Json<Value>> {
    crate::accounts::admin(&app, &headers)?;
    Ok(Json(app.osc.snapshot()))
}
pub async fn put(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(s): Json<Settings>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    crate::accounts::admin(&app, &headers)?;
    role(&app, &id, &u)?;
    let _guard = app.setup.lock().await;
    if app.active.lock().unwrap().is_some() {
        return Err(bad("Deactivate the show before changing OSC bindings"));
    }
    let runtime = app.osc.clone();
    tokio::task::spawn_blocking(move || runtime.configure(s))
        .await
        .map_err(internal)?
        .map_err(bad)?;
    Ok(Json(app.osc.snapshot()))
}
