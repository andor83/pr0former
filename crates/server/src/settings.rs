use crate::{Api, App, audio, bad, config::RuntimeConfig, csrf, internal, load, role, send, user};
use axum::{
    Json,
    extract::{Path, State},
    http::HeaderMap,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Interface {
    pub id: u32,
    pub name: String,
    pub enabled: bool,
    pub correct_latency: bool,
    pub latency_ms: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct InputInterface {
    pub id: u32,
    pub name: String,
    pub enabled: bool,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub sample_rate: u32,
    #[serde(default = "default_block_size")]
    pub block_size: usize,
    pub interfaces: Vec<Interface>,
    #[serde(default)]
    pub input_interfaces: Vec<InputInterface>,
}
fn default_block_size() -> usize {
    128
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            block_size: 128,
            interfaces: vec![],
            input_interfaces: vec![],
        }
    }
}
/// What a host needs in order to prepare platform audio *before* the runtime
/// opens a device: the format the engine will ask the driver for, and whether
/// any native input is enabled.
///
/// iOS is the reason this is public. An `AVAudioSession` category, preferred
/// sample rate and preferred I/O buffer duration have to be set before CoreAudio
/// opens a stream, and the record category — which is what triggers the
/// microphone permission prompt — must be selected only when an input feature is
/// actually configured.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AudioPreferences {
    pub sample_rate: u32,
    pub block_size: usize,
    /// Whether the saved settings enable at least one native input interface.
    pub capture_inputs: bool,
}

impl AudioPreferences {
    /// The engine block as a device buffer duration in seconds, which is the
    /// unit CoreAudio's preferred I/O duration uses. It is a *preference*: the
    /// driver picks the callback size, and the runtime's output queue already
    /// adapts to whatever it chooses.
    pub fn io_duration(&self) -> f64 {
        self.block_size as f64 / self.sample_rate as f64
    }

    /// The preferences implied by one settings value.
    ///
    /// The orchestration worker uses this rather than [`preferences`] because it
    /// already holds the exact settings it is about to open devices with —
    /// which, after a settings change that enables an input, is newer than
    /// anything a host read at launch.
    pub fn from_settings(settings: &Settings) -> Self {
        Self {
            sample_rate: settings.sample_rate,
            block_size: settings.block_size,
            capture_inputs: settings.input_interfaces.iter().any(|i| i.enabled),
        }
    }
}

/// Reads the host's saved audio preferences. Pure file reading: it enumerates
/// nothing, opens no device and starts no engine, so a host may call it before
/// [`crate::start`] and again on every platform lifecycle event.
pub fn preferences(config: &RuntimeConfig) -> AudioPreferences {
    AudioPreferences::from_settings(&read(config))
}

pub fn read(config: &RuntimeConfig) -> Settings {
    std::fs::read(config.audio_settings_path())
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .filter(|s| validate(s).is_ok())
        .unwrap_or_default()
}
/// Merge only newly discovered devices. A stored disabled choice survives
/// disconnection/reconnection and is never overwritten by discovery.
///
/// Routes that [`crate::native_audio::opt_in_route`] marks — the explicit Linux
/// PCM names, and the iOS capture route — are added disabled, so discovering
/// them never reserves hardware or asks a user for a permission.
fn merge_discovered(s: &mut Settings, outputs: &[(u32, String)], inputs: &[(u32, String)]) -> bool {
    let mut changed = false;
    for (id, name) in outputs {
        if name.starts_with("wasapi:") {
            if let Some(i)=s.interfaces.iter_mut().find(|i|i.id==*id && !i.name.starts_with("wasapi:")) {i.name=name.clone();changed=true;}
        }
        if s.interfaces.len() < 64 && name.len() <= 256 && !s.interfaces.iter().any(|i| i.id == *id)
        {
            s.interfaces.push(Interface {
                id: *id,
                name: name.clone(),
                enabled: !crate::native_audio::opt_in_route(name),
                correct_latency: false,
                latency_ms: 0.,
            });
            changed = true;
        }
    }
    for (id, name) in inputs {
        if name.starts_with("wasapi:") {
            if let Some(i)=s.input_interfaces.iter_mut().find(|i|i.id==*id && !i.name.starts_with("wasapi:")) {i.name=name.clone();changed=true;}
        }
        if s.input_interfaces.len() < 64
            && name.len() <= 256
            && !s.input_interfaces.iter().any(|i| i.id == *id)
        {
            s.input_interfaces.push(InputInterface {
                id: *id,
                name: name.clone(),
                // Never enabled by discovery for a permission-gated capture
                // route: enabling one is what selects the recording category,
                // and that must follow a performer's choice rather than the
                // fact that the platform has a microphone.
                enabled: !crate::native_audio::opt_in_route(name),
            });
            changed = true;
        }
    }
    changed
}
/// Caller holds the setup mutex. Discovery does not open or restart streams.
pub async fn discover(config: &std::sync::Arc<RuntimeConfig>) -> Api<Settings> {
    let config = config.clone();
    tokio::task::spawn_blocking(move || {
        let mut settings = read(&config);
        if merge_discovered(
            &mut settings,
            &audio::output_devices(&config),
            &audio::input_devices(&config),
        ) {
            let bytes = serde_json::to_vec_pretty(&settings).map_err(internal)?;
            let path = config.audio_settings_path();
            let temp = path.with_extension("tmp");
            std::fs::write(&temp, bytes).map_err(internal)?;
            std::fs::rename(temp, &path).map_err(internal)?;
        }
        Ok(settings)
    })
    .await
    .map_err(internal)?
}
pub fn validate(s: &Settings) -> Result<(), String> {
    if ![44100, 48000, 88200, 96000].contains(&s.sample_rate) {
        return Err("Choose 44.1, 48, 88.2, or 96 kHz".into());
    }
    if ![32, 64, 128, 256, 512, 1024].contains(&s.block_size) {
        return Err("Choose a DSP block size of 32, 64, 128, 256, 512, or 1024 frames".into());
    }
    if s.interfaces.len() > 64 {
        return Err("Too many interfaces".into());
    }
    let mut ids = std::collections::HashSet::new();
    for i in &s.interfaces {
        if i.id == 0
            || i.id > 999999999
            || !ids.insert(i.id)
            || i.name.len() > 256
            || !i.latency_ms.is_finite()
            || !(0.0..=1000.0).contains(&i.latency_ms)
        {
            return Err("Invalid interface or latency (0–1000 ms)".into());
        }
    }
    let mut ids = std::collections::HashSet::new();
    if s.input_interfaces.len() > 64 {
        return Err("Too many input interfaces".into());
    }
    for i in &s.input_interfaces {
        if i.id == 0 || i.id > 999999999 || !ids.insert(i.id) || i.name.len() > 256 {
            return Err("Invalid input interface".into());
        }
    }
    Ok(())
}
pub fn validate_route_changes(
    previous: &pr0_core::Project,
    next: &pr0_core::Project,
    s: &Settings,
) -> Result<(), String> {
    // Retain unavailable routes in dormant projects so they can be repaired one
    // node at a time after global interface changes. New routes must be enabled.
    let mut changed = next.clone();
    changed.graph.nodes.retain(|node| {
        matches!(node.kind.as_str(), "output" | "input")
            && !previous.graph.nodes.iter().any(|old| {
                old.id == node.id
                    && old.kind == node.kind
                    && old.parameters.get("interface").copied().unwrap_or(0.)
                        == node.parameters.get("interface").copied().unwrap_or(0.)
            })
    });
    validate_routes(&changed, s)
}
pub fn validate_routes(p: &pr0_core::Project, s: &Settings) -> Result<(), String> {
    for n in &p.graph.nodes {
        if matches!(n.kind.as_str(), "output" | "input") {
            let id = n.parameters.get("interface").copied().unwrap_or(0.);
            if id.fract() != 0.
                || (id != 0.
                    && !(if n.kind == "input" {
                        s.input_interfaces
                            .iter()
                            .any(|i| i.enabled && i.id as f64 == id)
                    } else {
                        s.interfaces.iter().any(|i| i.enabled && i.id as f64 == id)
                    }))
            {
                return Err(format!("{}: select an enabled audio interface", n.label));
            }
        }
    }
    Ok(())
}
pub async fn get(State(app): State<App>, headers: HeaderMap) -> Api<Json<Settings>> {
    crate::accounts::admin(&app, &headers)?;
    let _guard = app.setup.lock().await;
    Ok(Json(discover(&app.config).await?))
}
pub async fn put(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(s): Json<Settings>,
) -> Api<Json<Settings>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    crate::accounts::admin(&app, &headers)?;
    role(&app, &id, &u)?;
    let _guard = app.setup.lock().await;
    apply(&app, &id, s, None).await
}
#[derive(Deserialize)]
pub struct DeviceEdit {
    direction: String,
    id: u32,
    enabled: bool,
}
pub async fn device(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    Json(edit): Json<DeviceEdit>,
) -> Api<Json<Settings>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    crate::accounts::admin(&app, &headers)?;
    role(&app, &id, &u)?;
    let _guard = app.setup.lock().await;
    // Enabling a capture route is the moment a performer asks for the
    // microphone, so that is when the platform prompt belongs — not several
    // actions later, when the engine happens to open a stream. Non-blocking,
    // asked only when nobody has decided yet, and a no-op on every host that
    // installs no capture capability. The answer is read again, and enforced,
    // before capture is actually opened.
    if edit.direction == "input" && edit.enabled {
        crate::native_audio::request_capture(&app.config);
    }
    let mut settings = discover(&app.config).await?;
    match edit.direction.as_str() {
        "input" => {
            settings
                .input_interfaces
                .iter_mut()
                .find(|i| i.id == edit.id)
                .ok_or_else(|| bad("Unknown input device"))?
                .enabled = edit.enabled
        }
        "output" => {
            settings
                .interfaces
                .iter_mut()
                .find(|i| i.id == edit.id)
                .ok_or_else(|| bad("Unknown output device"))?
                .enabled = edit.enabled
        }
        _ => return Err(bad("Choose input or output")),
    }
    apply(&app, &id, settings, Some((&edit.direction, edit.id))).await
}
async fn apply(
    app: &App,
    id: &str,
    s: Settings,
    device: Option<(&str, u32)>,
) -> Api<Json<Settings>> {
    if app.active.lock().unwrap().is_some() {
        return Err(bad("Deactivate the show before changing system audio"));
    }
    validate(&s).map_err(bad)?;
    let devices_config = app.config.clone();
    let (detected,detected_inputs) = tokio::task::spawn_blocking(move ||(audio::output_devices(&devices_config),audio::input_devices(&devices_config))).await.map_err(internal)?;
    for i in &s.interfaces {
        if i.enabled
            && device.is_none_or(|target| target == ("output", i.id))
            && !detected.iter().any(|d| d.0 == i.id && d.1 == i.name)
        {
            return Err(bad(
                "Selected interface is no longer available; refresh devices",
            ));
        }
    }
    for i in &s.input_interfaces {
        if i.enabled
            && device.is_none_or(|target| target == ("input", i.id))
            && !detected_inputs.iter().any(|d| d.0 == i.id && d.1 == i.name)
        {
            return Err(bad(
                "Selected input is no longer available; refresh devices",
            ));
        }
    }
    let p = load(&app, &id)?;
    let prepared = p.clone();
    let rate = s.sample_rate;
    app.logs.push(
        &id,
        "info",
        "Preparing clip caches for system audio settings",
    );
    let cache_config = app.config.clone();
    tokio::task::spawn_blocking(move || crate::samples::cache_project(&cache_config, &prepared, rate))
        .await
        .map_err(internal)?
        .map_err(bad)?;
    let bytes = serde_json::to_vec_pretty(&s).map_err(internal)?;
    let path = app.config.audio_settings_path();
    let temp = path.with_extension("tmp");
    tokio::fs::write(&temp, bytes).await.map_err(internal)?;
    tokio::fs::rename(temp, &path).await.map_err(internal)?;
    // Close devices so subsequent enablement uses the newly committed settings.
    let (tx, rx) = tokio::sync::oneshot::channel();
    send(
        &app,
        audio::Command::Enable(id.to_owned(), false, s.clone(), tx),
    )?;
    rx.await.map_err(internal)?.map_err(bad)?;
    send(&app, audio::Command::Unload)?;
    *app.graph.lock().unwrap() = None;
    let _ = app.events.send(super::engine_status(app));
    app.logs.push(
        &id,
        "info",
        &format!(
            "System sample rate set to {} Hz; clip caches ready",
            s.sample_rate
        ),
    );
    app.logs.push(
        &id,
        "info",
        &format!("DSP block size set to {} frames", s.block_size),
    );
    let _ = app
        .events
        .send(json!({"type":"system_audio","settings":s}).into());
    Ok(Json(s))
}
pub async fn logs(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<Json<Value>> {
    let u = user(&app, &headers)?;
    role(&app, &id, &u)?;
    Ok(Json(json!(app.logs.snapshot(&id))))
}
#[derive(Default)]
pub struct Logs {
    rows: std::sync::Mutex<std::collections::VecDeque<Value>>,
    sequence: std::sync::atomic::AtomicU64,
}
impl Logs {
    pub fn push(&self, project: &str, level: &str, message: &str) {
        let mut rows = self.rows.lock().unwrap();
        let seq = self
            .sequence
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        rows.push_back(json!({"sequence":seq,"project_id":project,"level":level,"message":message,"time":std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_millis()}));
        if rows.len() > 2000 {
            rows.pop_front();
        }
    }
    pub fn snapshot(&self, id: &str) -> Vec<Value> {
        self.rows
            .lock()
            .unwrap()
            .iter()
            .filter(|v| v["project_id"] == id)
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What a platform audio-session bridge is given before any device is
    /// opened. Reading preferences must never depend on an engine, a device or
    /// a saved file existing.
    #[test]
    fn audio_preferences_describe_the_engine_format_and_input_use() {
        let directory = std::env::temp_dir().join(format!("pr0-preferences-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        let config = RuntimeConfig::new(&directory);
        // No saved settings at all: the defaults are what the engine will use.
        let defaults = preferences(&config);
        assert_eq!(defaults.sample_rate, 48000);
        assert_eq!(defaults.block_size, 128);
        assert!(!defaults.capture_inputs);
        assert!((defaults.io_duration() - 128. / 48000.).abs() < f64::EPSILON);

        let mut settings = Settings::default();
        settings.sample_rate = 44100;
        settings.block_size = 256;
        settings.input_interfaces.push(InputInterface {
            id: 1,
            name: "Disabled input".into(),
            enabled: false,
        });
        std::fs::write(
            config.audio_settings_path(),
            serde_json::to_vec(&settings).unwrap(),
        )
        .unwrap();
        let saved = preferences(&config);
        assert_eq!((saved.sample_rate, saved.block_size), (44100, 256));
        // A stored but unchecked input is not an input feature in use, so a
        // host must not select a recording category for it.
        assert!(!saved.capture_inputs);

        settings.input_interfaces[0].enabled = true;
        std::fs::write(
            config.audio_settings_path(),
            serde_json::to_vec(&settings).unwrap(),
        )
        .unwrap();
        assert!(preferences(&config).capture_inputs);
        let _ = std::fs::remove_dir_all(&directory);
    }

    #[test]
    fn explicit_linux_routes_require_opt_in() {
        let mut s=Settings::default();
        merge_discovered(&mut s,&[(1,"pulse:DEVICE=hdmi".into()),(2,"hw:CARD=USB,DEV=0".into())],&[(3,"hw:CARD=USB,DEV=0".into())]);
        assert!(s.interfaces.iter().all(|i|!i.enabled));
        assert!(!s.input_interfaces[0].enabled);
        s.interfaces[0].enabled=true;
        merge_discovered(&mut s,&[(1,"pulse:DEVICE=hdmi".into())],&[]);
        assert!(s.interfaces[0].enabled);
    }
    #[test]
    fn windows_identity_upgrade_preserves_route_id_and_saved_preferences() {
        let mut s=Settings::default();
        merge_discovered(&mut s,&[(42,"Speakers".into())],&[]);
        s.interfaces[0].enabled=false;
        s.interfaces[0].latency_ms=12.;
        assert!(merge_discovered(&mut s,&[(42,"wasapi:endpoint-a".into())],&[]));
        assert_eq!(s.interfaces.len(),1);
        assert_eq!(s.interfaces[0].name,"wasapi:endpoint-a");
        assert!(!s.interfaces[0].enabled);
        assert_eq!(s.interfaces[0].latency_ms,12.);
    }
    #[test]
    fn discovery_enables_new_devices_and_preserves_disabled_choices_across_reconnect() {
        let mut s = Settings::default();
        assert!(merge_discovered(
            &mut s,
            &[(1, "Output".into())],
            &[(2, "Input".into())]
        ));
        assert!(s.interfaces[0].enabled && s.input_interfaces[0].enabled);
        s.interfaces[0].enabled = false;
        s.input_interfaces[0].enabled = false;
        let saved = serde_json::to_vec(&s).unwrap();
        let mut restored: Settings = serde_json::from_slice(&saved).unwrap();
        assert!(!merge_discovered(&mut restored, &[], &[]));
        assert!(merge_discovered(
            &mut restored,
            &[(1, "Output".into()), (3, "New output".into())],
            &[(2, "Input".into()), (4, "New input".into())]
        ));
        assert!(!restored.interfaces[0].enabled && !restored.input_interfaces[0].enabled);
        assert!(restored.interfaces[1].enabled && restored.input_interfaces[1].enabled);
    }
    #[test]
    fn native_input_routes_require_enabled_inputs_and_legacy_settings_load() {
        let mut settings: Settings =
            serde_json::from_str(r#"{"sample_rate":48000,"interfaces":[]}"#).unwrap();
        assert!(settings.input_interfaces.is_empty());
        let mut project = pr0_core::demo_project("x".into(), "x".into(), pr0_core::Mode::Freeform);
        project.graph.nodes.retain(|n| n.id == "tone");
        project.graph.edges.clear();
        project.graph.nodes[0].kind = "input".into();
        project.graph.nodes[0].parameters = [("interface".into(), 123.)].into();
        assert!(validate_routes(&project, &settings).is_err());
        settings.input_interfaces.push(InputInterface {
            id: 123,
            name: "Input".into(),
            enabled: true,
        });
        assert!(validate_routes(&project, &settings).is_ok());
        settings.input_interfaces[0].enabled = false;
        assert!(validate_routes(&project, &settings).is_err());
        project.graph.nodes[0]
            .parameters
            .insert("interface".into(), 0.);
        assert!(validate_routes(&project, &settings).is_ok());
        settings
            .input_interfaces
            .push(settings.input_interfaces[0].clone());
        assert!(validate(&settings).is_err());
    }

    #[test]
    fn rejects_invalid_rates_routes_and_latencies() {
        let mut s = Settings::default();
        s.sample_rate = 12345;
        assert!(validate(&s).is_err());
        s.sample_rate = 44100;
        s.interfaces.push(Interface {
            id: 1,
            name: "Test".into(),
            enabled: true,
            correct_latency: true,
            latency_ms: f64::NAN,
        });
        assert!(validate(&s).is_err());
        s.interfaces[0].latency_ms = 10.;
        assert!(validate(&s).is_ok());
        s.interfaces.push(s.interfaces[0].clone());
        assert!(validate(&s).is_err());
    }
    #[test]
    fn dormant_routes_can_be_repaired_individually() {
        let mut previous =
            pr0_core::demo_project("x".into(), "x".into(), pr0_core::Mode::Structured);
        let output = previous
            .graph
            .nodes
            .iter_mut()
            .find(|n| n.kind == "output")
            .unwrap();
        output.parameters.insert("interface".into(), 1.);
        let mut other = output.clone();
        other.id = "other".into();
        previous.graph.nodes.push(other);
        let mut next = previous.clone();
        next.graph
            .nodes
            .iter_mut()
            .find(|n| n.id == "out")
            .unwrap()
            .parameters
            .insert("interface".into(), 0.);
        assert!(validate_route_changes(&previous, &next, &Settings::default()).is_ok());
        assert!(validate_routes(&next, &Settings::default()).is_err());
        next.graph
            .nodes
            .iter_mut()
            .find(|n| n.id == "other")
            .unwrap()
            .parameters
            .insert("interface".into(), 2.);
        assert!(validate_route_changes(&previous, &next, &Settings::default()).is_err());
    }
    #[test]
    fn console_is_bounded_and_project_scoped() {
        let logs = Logs::default();
        for _ in 0..2100 {
            logs.push("one", "info", "event");
        }
        logs.push("two", "error", "failure");
        assert_eq!(logs.snapshot("one").len(), 1999);
        assert_eq!(logs.snapshot("two").len(), 1);
        assert!(logs.snapshot("other").is_empty());
    }
}

/// Graph clients need the current audio contract, not permission to edit system settings.
pub async fn public_config(State(app): State<App>, headers: HeaderMap) -> Api<Json<Value>> {
    user(&app, &headers)?;
    let s = read(&app.config);
    Ok(Json(
        serde_json::json!({"sample_rate":s.sample_rate,"block_size":s.block_size,"interfaces":s.interfaces.iter().map(|i|serde_json::json!({"id":i.id,"name":i.name,"enabled":i.enabled})).collect::<Vec<_>>(),"input_interfaces":s.input_interfaces.iter().map(|i|serde_json::json!({"id":i.id,"name":i.name,"enabled":i.enabled})).collect::<Vec<_>>()}),
    ))
}
