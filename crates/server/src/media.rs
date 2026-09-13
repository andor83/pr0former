//! WebRTC/Opus adapter. Codec and network work never runs in a device callback.
use crate::{Api, App, Failure, bad, csrf, role, user};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, Mutex as StdMutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tokio::sync::{Mutex, broadcast};
use webrtc::{
    api::{
        APIBuilder, interceptor_registry::register_default_interceptors, media_engine::MediaEngine,
    },
    interceptor::registry::Registry,
    media::Sample,
    peer_connection::{
        RTCPeerConnection, configuration::RTCConfiguration,
        peer_connection_state::RTCPeerConnectionState,
        sdp::session_description::RTCSessionDescription,
    },
    rtp_transceiver::rtp_codec::{RTCRtpCodecCapability, RTPCodecType},
    track::track_local::{TrackLocal, track_local_static_sample::TrackLocalStaticSample},
};

#[derive(Clone)]
pub struct AudioBlock {
    pub project: String,
    pub pcm: Arc<Vec<f32>>,
    pub monitors: BTreeMap<String, Arc<Vec<f32>>>,
}
struct MonitorSubscription {
    engine: std::sync::mpsc::SyncSender<crate::audio::Command>,
    session: String,
    project: String,
    node: Option<String>,
}
impl MonitorSubscription {
    fn refresh(&self, enabled: bool) {
        let _ = self.engine.try_send(crate::audio::Command::Monitor {
            session: self.session.clone(),
            project: self.project.clone(),
            node: self.node.clone(),
            enabled,
        });
    }
}
impl Drop for MonitorSubscription {
    fn drop(&mut self) {
        self.refresh(false);
    }
}
type Peers = Arc<Mutex<BTreeMap<String, Arc<RTCPeerConnection>>>>;
type Admissions = Arc<StdMutex<BTreeMap<String, Arc<AtomicBool>>>>;
struct InputSource {
    token: Arc<AtomicBool>,
    session: String,
    project: String,
    node: String,
    user_name: String,
    machine_name: String,
}
type InputSources = Arc<StdMutex<BTreeMap<String, InputSource>>>;
struct SessionLease {
    inputs: InputSources,
    input_key: Option<String>,
    key: String,
    cancelled: Arc<AtomicBool>,
    admissions: Admissions,
    peers: Peers,
    peer: Option<Arc<RTCPeerConnection>>,
}
fn release_slot(admissions: &Admissions, key: &str, token: &Arc<AtomicBool>) {
    let mut slots = admissions.lock().unwrap();
    if slots
        .get(key)
        .is_some_and(|current| Arc::ptr_eq(current, token))
    {
        slots.remove(key);
    }
}
impl Drop for SessionLease {
    fn drop(&mut self) {
        if let Some(key) = &self.input_key {
            let mut inputs = self.inputs.lock().unwrap();
            if inputs.get(key).is_some_and(|source| Arc::ptr_eq(&source.token, &self.cancelled)) { inputs.remove(key); }
        }
        if let Some(peer) = self.peer.take() {
            let (key, token, admissions, peers) = (
                self.key.clone(),
                self.cancelled.clone(),
                self.admissions.clone(),
                self.peers.clone(),
            );
            tokio::spawn(async move {
                let _ = peer.close().await;
                let mut registered = peers.lock().await;
                if registered
                    .get(&key)
                    .is_some_and(|current| Arc::ptr_eq(current, &peer))
                {
                    registered.remove(&key);
                }
                release_slot(&admissions, &key, &token);
            });
        } else {
            release_slot(&self.admissions, &self.key, &self.cancelled);
        }
    }
}
pub struct Media {
    inputs: InputSources,
    pub audio: broadcast::Sender<AudioBlock>,
    pub peers: Peers,
    admissions: Admissions,
}
impl Media {
    pub async fn close_project_user(&self, project: &str, user: &str) {
        let key = format!("{project}/{user}");
        let token = self.admissions.lock().unwrap().get(&key).cloned();
        if let Some(token) = &token {
            token.store(true, Ordering::Release);
        }
        let peer = self.peers.lock().await.remove(&key);
        if let Some(peer) = peer {
            let _ = tokio::time::timeout(Duration::from_secs(5), peer.close()).await;
        }
        if let Some(token) = token {
            release_slot(&self.admissions, &key, &token);
        }
    }

    pub async fn close_project(&self, project: &str) {
        let prefix = format!("{project}/");
        {
            let slots = self.admissions.lock().unwrap();
            for (key, cancelled) in slots.iter() {
                if key.starts_with(&prefix) {
                    cancelled.store(true, Ordering::Release);
                }
            }
        }
        let removed = {
            let mut peers = self.peers.lock().await;
            let keys: Vec<_> = peers
                .keys()
                .filter(|key| key.starts_with(&prefix))
                .cloned()
                .collect();
            keys.into_iter()
                .filter_map(|key| peers.remove(&key))
                .collect::<Vec<_>>()
        };
        let mut closing = tokio::task::JoinSet::new();
        for peer in removed {
            closing.spawn(async move {
                let _ = tokio::time::timeout(Duration::from_secs(5), peer.close()).await;
            });
        }
        while closing.join_next().await.is_some() {}
    }

    pub async fn close_user(&self, user: &str) {
        let suffix = format!("/{user}");
        {
            let slots = self.admissions.lock().unwrap();
            for (key, cancelled) in slots.iter() {
                if key.ends_with(&suffix) {
                    cancelled.store(true, Ordering::Release);
                }
            }
        }
        let removed = {
            let mut peers = self.peers.lock().await;
            let keys: Vec<_> = peers
                .keys()
                .filter(|key| key.ends_with(&suffix))
                .cloned()
                .collect();
            keys.into_iter()
                .filter_map(|key| peers.remove(&key))
                .collect::<Vec<_>>()
        };
        let mut closing = tokio::task::JoinSet::new();
        for peer in removed {
            closing.spawn(async move {
                let _ = tokio::time::timeout(Duration::from_secs(5), peer.close()).await;
            });
        }
        while closing.join_next().await.is_some() {}
    }

    fn reserve(&self, key: String) -> Api<SessionLease> {
        let mut slots = self.admissions.lock().unwrap();
        if slots.contains_key(&key) {
            return Err(Failure(
                StatusCode::CONFLICT,
                "Disconnect the existing audio session before reconnecting".into(),
            ));
        }
        if slots.len() >= 32 {
            return Err(bad("32 audio sessions already connected or negotiating"));
        }
        let cancelled = Arc::new(AtomicBool::new(false));
        slots.insert(key.clone(), cancelled.clone());
        Ok(SessionLease {
            inputs: self.inputs.clone(), input_key: None,
            key,
            cancelled,
            admissions: self.admissions.clone(),
            peers: self.peers.clone(),
            peer: None,
        })
    }
    pub fn new() -> Self {
        let (audio, _) = broadcast::channel(crate::monitor_packets::QUEUED_PACKETS);
        Self {
            inputs: Arc::new(StdMutex::new(BTreeMap::new())),
            audio,
            peers: Arc::new(Mutex::new(BTreeMap::new())),
            admissions: Arc::new(StdMutex::new(BTreeMap::new())),
        }
    }
}
#[derive(Deserialize)]
pub struct Offer {
    pub machine_name: Option<String>,
    pub sdp: String,
    pub input_node: Option<String>,
    pub monitor_node: Option<String>,
}

pub async fn inputs(State(app): State<App>, headers: HeaderMap, Path(id): Path<String>) -> Api<Json<Value>> {
    let u=user(&app,&headers)?; role(&app,&id,&u)?;
    let peers=app.media.peers.lock().await;
    let sources=app.media.inputs.lock().unwrap();
    let inputs: Vec<Value> = sources.values().filter(|s|s.project==id && !s.token.load(Ordering::Acquire))
        .map(|s| json!({"project_id":s.project,"node":s.node,"user_name":s.user_name,"machine_name":s.machine_name,"state":peers.get(&s.session).map(|p|p.connection_state().to_string()).unwrap_or_else(||"connecting".into())})).collect();
    Ok(Json(json!(inputs)))
}

pub async fn offer(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(request): Json<Offer>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    let membership = role(&app, &id, &u)?;
    app.logs
        .push(&id, "info", "Browser audio negotiation requested");
    if app.graph.lock().unwrap().as_deref() != Some(&id) {
        return Err(bad(
            "Enable the audio engine for the show before connecting audio",
        ));
    }
    if let Some(node) = &request.monitor_node {
        let project = crate::load(&app, &id)?;
        if !project
            .graph
            .nodes
            .iter()
            .any(|n| n.id == *node && n.kind == "monitor_output")
        {
            return Err(bad("Select a monitor output from this project"));
        }
    }
    let monitor_node = request.monitor_node.clone();
    let input = request.input_node.clone();
    if let Some(node) = &input {
        let project = crate::load(&app, &id)?;
        if !project
            .graph
            .nodes
            .iter()
            .any(|n| n.id == *node && n.kind == "browser_input")
        {
            return Err(bad("Select a local audio input node"));
        }
        if membership != "owner"
            && !project.parts.iter().any(|p| {
                p.performer.as_deref() == Some(&u) && p.instrument_node.as_deref() == Some(node)
            })
        {
            return Err(Failure(
                StatusCode::FORBIDDEN,
                "This local audio input is not assigned to you".into(),
            ));
        }
    }
    let key = format!("{id}/{u}");
    let machine_name = request.machine_name.as_deref().unwrap_or("Browser device").trim();
    if machine_name.is_empty() || machine_name.chars().count() > 80 || machine_name.chars().any(char::is_control) { return Err(bad("Machine name must contain 1–80 printable characters")); }
    let mut lease = app.media.reserve(key.clone())?;
    if let Some(node) = &input {
        let user_name: String = app.db.lock().unwrap().query_row("SELECT username FROM users WHERE id=?1", [&u], |r|r.get(0)).map_err(bad)?;
        let input_key = format!("{id}/{node}");
        let mut inputs = app.media.inputs.lock().unwrap();
        if inputs.contains_key(&input_key) { return Err(Failure(StatusCode::CONFLICT, "This local audio input is already connected from another session".into())); }
        inputs.insert(input_key.clone(), InputSource {token:lease.cancelled.clone(),session:key.clone(),project:id.clone(),node:node.clone(),user_name,machine_name:machine_name.to_owned()});
        lease.input_key=Some(input_key);
    }
    let mut media = MediaEngine::default();
    media.register_default_codecs().map_err(bad)?;
    let registry = register_default_interceptors(Registry::new(), &mut media).map_err(bad)?;
    let mut settings = webrtc::api::setting_engine::SettingEngine::default();
    settings.set_include_loopback_candidate(true);
    // This LAN server advertises numeric host candidates. Browsers initiate ICE
    // checks; peer-reflexive candidates avoid resolving private .local names.
    settings.set_lite(true);
    settings.set_ice_multicast_dns_mode(webrtc::ice::mdns::MulticastDnsMode::Disabled);
    let api = APIBuilder::new()
        .with_setting_engine(settings)
        .with_media_engine(media)
        .with_interceptor_registry(registry)
        .build();
    let peer = Arc::new(
        api.new_peer_connection(RTCConfiguration::default())
            .await
            .map_err(bad)?,
    );
    lease.peer = Some(peer.clone());
    let track = Arc::new(TrackLocalStaticSample::new(
        RTCRtpCodecCapability {
            mime_type: "audio/opus".into(),
            clock_rate: 48000,
            channels: 2,
            sdp_fmtp_line: "minptime=10;useinbandfec=1;stereo=1".into(),
            ..Default::default()
        },
        "monitor".into(),
        "pr0former".into(),
    ));
    let sender = peer
        .add_track(track.clone() as Arc<dyn TrackLocal + Send + Sync>)
        .await
        .map_err(bad)?;
    tokio::spawn(async move {
        let mut buffer = [0_u8; 1500];
        while sender.read(&mut buffer).await.is_ok() {}
    });
    let engine = app.engine.clone();
    peer.on_track(Box::new(move |remote, _, _| {
        let input = input.clone();
        let engine = engine.clone();
        Box::pin(async move {
            if remote.kind() != RTPCodecType::Audio {
                return;
            }
            let Some(node) = input else { return };
            let Ok(mut decoder) = opus::Decoder::new(48000, opus::Channels::Stereo) else {
                return;
            };
            let mut pcm = vec![0_f32; 5760 * 2];
            while let Ok((packet, _)) = remote.read_rtp().await {
                match decoder.decode_float(&packet.payload, &mut pcm, false) {
                    Ok(frames) => {
                        let _ = engine.try_send(crate::audio::Command::BrowserInput {
                            node: node.clone(),
                            pcm: pcm[..frames * 2]
                                .chunks_exact(2)
                                .map(|f| [f[0], f[1]])
                                .collect(),
                        });
                    }
                    Err(e) => tracing::warn!("Opus decode: {e}"),
                }
            }
        })
    }));
    peer.set_remote_description(RTCSessionDescription::offer(request.sdp).map_err(bad)?)
        .await
        .map_err(bad)?;
    let answer = peer.create_answer(None).await.map_err(bad)?;
    let mut gathering = peer.gathering_complete_promise().await;
    peer.set_local_description(answer).await.map_err(bad)?;
    tokio::time::timeout(Duration::from_secs(10), gathering.recv())
        .await
        .map_err(|_| bad("ICE gathering timed out"))?;
    let sdp = peer
        .local_description()
        .await
        .ok_or_else(|| bad("No local SDP"))?;
    let mut audio = app.media.audio.subscribe();
    let subscription = MonitorSubscription {
        engine: app.engine.clone(),
        session: uuid::Uuid::new_v4().to_string(),
        project: id.clone(),
        node: monitor_node.clone(),
    };
    let pc = peer.clone();
    let project_id = id.clone();
    {
        let mut peers = app.media.peers.lock().await;
        if lease.cancelled.load(Ordering::Acquire)
            || app.graph.lock().unwrap().as_deref() != Some(&id)
        {
            return Err(bad(
                "Audio connection was cancelled or the show was deactivated",
            ));
        }
        peers.insert(key, peer.clone());
    }
    tokio::spawn(async move {
        let subscription = subscription;
        let mut subscription_seen = std::time::Instant::now() - Duration::from_secs(3);
        let _lease = lease;
        let Ok(mut encoder) =
            opus::Encoder::new(48000, opus::Channels::Stereo, opus::Application::Audio)
        else {
            return;
        };
        let _ = encoder.set_bitrate(opus::Bitrate::Bits(128000));
        let _ = encoder.set_inband_fec(true);
        let _ = encoder.set_signal(opus::Signal::Music);
        let mut pending = Vec::with_capacity(1920);
        let mut encoded = [0_u8; 4000];
        let mut disconnected = 0;
        loop {
            let state = pc.connection_state();
            if matches!(
                state,
                RTCPeerConnectionState::Closed | RTCPeerConnectionState::Failed
            ) {
                break;
            }
            if state == RTCPeerConnectionState::Disconnected {
                disconnected += 1;
                if disconnected > 50 {
                    break;
                }
            } else {
                disconnected = 0;
            }
            if subscription_seen.elapsed() >= Duration::from_secs(2) {
                subscription.refresh(true);
                subscription_seen = std::time::Instant::now();
            }
            let next = tokio::time::timeout(Duration::from_secs(2), audio.recv()).await;
            let block = match next {
                Ok(Ok(block)) => block,
                Ok(Err(broadcast::error::RecvError::Lagged(_))) => {
                    pending.clear();
                    continue;
                }
                Ok(Err(_)) => break,
                Err(_) => continue,
            };
            if block.project != project_id {
                continue;
            }
            let selected = monitor_node
                .as_ref()
                .and_then(|node| block.monitors.get(node));
            let missing = monitor_node.is_some() && selected.is_none();
            let pcm = selected.unwrap_or(&block.pcm);
            for sample in pcm.iter() {
                pending.push(if missing { 0. } else { *sample });
                if pending.len() == 960 {
                    if let Ok(len) = encoder.encode_float(&pending, &mut encoded) {
                        let _ = track
                            .write_sample(&Sample {
                                data: bytes::Bytes::copy_from_slice(&encoded[..len]),
                                duration: Duration::from_millis(10),
                                ..Default::default()
                            })
                            .await;
                    }
                    pending.clear();
                }
            }
        }
    });
    app.logs
        .push(&id, "info", "Browser audio negotiation completed");
    Ok(Json(json!({"type":"answer","sdp":sdp.sdp})))
}
pub async fn disconnect(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    role(&app, &id, &u)?;
    app.logs
        .push(&id, "info", "Browser audio disconnect requested");
    let key = format!("{id}/{u}");
    let token = app.media.admissions.lock().unwrap().get(&key).cloned();
    if let Some(token) = &token {
        token.store(true, Ordering::Release);
    }
    let peer = app.media.peers.lock().await.remove(&key);
    if let Some(peer) = peer {
        peer.close().await.map_err(bad)?;
        if let Some(token) = token {
            release_slot(&app.media.admissions, &key, &token);
        }
    }
    Ok(Json(json!({"ok":true})))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn closing_project_cancels_only_its_pending_media() {
        let media = Media::new();
        let local = media.reserve("a/user".into()).ok().unwrap();
        let other = media.reserve("ab/user".into()).ok().unwrap();
        media.close_project("a").await;
        assert!(local.cancelled.load(Ordering::Acquire));
        assert!(!other.cancelled.load(Ordering::Acquire));
        drop(local);
        assert!(media.reserve("a/user".into()).is_ok());
    }
    #[test]
    fn pending_sessions_count_toward_capacity_and_release_on_error() {
        let media = Media::new();
        let mut leases: Vec<_> = (0..32)
            .map(|i| media.reserve(i.to_string()).ok().unwrap())
            .collect();
        assert!(media.reserve("overflow".into()).is_err());
        assert!(media.reserve("0".into()).is_err());
        leases.pop();
        assert!(media.reserve("replacement".into()).is_ok());
        drop(leases);
        assert!(media.admissions.lock().unwrap().is_empty());
    }
    #[test]
    fn simultaneous_offers_cannot_exceed_capacity() {
        let media = Media::new();
        let barrier = std::sync::Barrier::new(65);
        let admitted = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for i in 0..64 {
                let (media, barrier, admitted) = (&media, &barrier, &admitted);
                scope.spawn(move || {
                    let lease = media.reserve(i.to_string()).ok();
                    if lease.is_some() {
                        admitted.fetch_add(1, Ordering::Relaxed);
                    }
                    barrier.wait();
                    drop(lease);
                });
            }
            barrier.wait();
        });
        assert_eq!(admitted.load(Ordering::Relaxed), 32);
        assert!(media.admissions.lock().unwrap().is_empty());
    }
    #[test]
    fn retired_cleanup_cannot_remove_a_new_reservation() {
        let media = Media::new();
        let old = media.reserve("player".into()).ok().unwrap();
        release_slot(&media.admissions, "player", &old.cancelled);
        let new = media.reserve("player".into()).ok().unwrap();
        drop(old);
        assert!(media.reserve("player".into()).is_err());
        drop(new);
        assert!(media.reserve("player".into()).is_ok());
    }
}
