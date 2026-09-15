//! Read-only desktop-audio topology. Never called by the audio worker.
//!
//! Compiled everywhere so the `/api/system/audio/linux` route and its types
//! exist on every host, but only Linux reaches the discovery and route lookup
//! below; elsewhere they answer an empty topology.
#![cfg_attr(not(target_os = "linux"), allow(dead_code))]
use axum::{Json, extract::State, http::HeaderMap};
use serde_json::{Value, json};
use std::{
    process::Stdio,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};
use tokio::io::AsyncReadExt;

// Only fixed commands are accepted; no request data enters command arguments.
async fn query(program: &str, args: &[&str]) -> Result<Value, String> {
    let work = async {
        let mut child = tokio::process::Command::new(program)
            .args(args)
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("{program}: {e}"))?;
        let mut bytes = Vec::new();
        child
            .stdout
            .take()
            .unwrap()
            .take(2 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)
            .await
            .map_err(|e| e.to_string())?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err(format!("{program}: inventory exceeds 2 MiB"));
        }
        if !child.wait().await.map_err(|e| e.to_string())?.success() {
            return Err(format!(
                "{program}: cannot query the audio server (check the server user's desktop session and tool version)"
            ));
        }
        serde_json::from_slice(&bytes).map_err(|_| format!("{program}: expected JSON output"))
    };
    tokio::time::timeout(Duration::from_secs(3), work)
        .await
        .map_err(|_| format!("{program}: discovery timed out"))?
}
fn text(value: &Value) -> String {
    value.as_str().unwrap_or("").chars().take(512).collect()
}
#[cfg(any(target_os = "linux", test))]
fn safe_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 180
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"._-".contains(&b))
}
#[cfg(any(target_os = "linux", test))]
fn pulse_routes(snapshot: &Value) -> Vec<Value> {
    if snapshot["backend"] == "PipeWire (native snapshot)" {
        return vec![];
    }
    snapshot["endpoints"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| {
            let name = v["name"].as_str()?;
            if !safe_name(name) || v["device_class"] == "monitor" || name.ends_with(".monitor") {
                return None;
            }
            let channels = v["channel_map"]
                .as_str()?
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .count();
            if !(1..=64).contains(&channels) {
                return None;
            }
            Some(
                json!({"name":format!("pulse:DEVICE={name}"),"label":v["description"],
            "backend":"PulseAudio / PipeWire","input":v["direction"]=="Input","channels":channels}),
            )
        })
        .collect()
}
static ROUTES: OnceLock<Mutex<Vec<Value>>> = OnceLock::new();
/// Called from blocking discovery only. No shell, global defaults or config files.
#[cfg(target_os = "linux")]
pub fn discover_routes() -> Vec<Value> {
    if !cfg!(target_os = "linux") {
        return vec![];
    }
    let mut routes = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .ok()
        .map(|runtime| pulse_routes(&runtime.block_on(inspect())))
        .unwrap_or_default();
    // Explicit raw ALSA PCMs, including HDMI device numbers other than zero.
    // Stable card IDs are used instead of boot-dependent card indices.
    if let Ok(pcms) = std::fs::read_to_string("/proc/asound/pcm") {
        for line in pcms.lines().take(128) {
            let Some((address, rest)) = line.split_once(':') else {
                continue;
            };
            let Some((card, device)) = address.trim().split_once('-') else {
                continue;
            };
            let (Ok(card), Ok(device)) = (card.parse::<u32>(), device.parse::<u32>()) else {
                continue;
            };
            let Ok(id) = std::fs::read_to_string(format!("/proc/asound/card{card}/id")) else {
                continue;
            };
            let id = id.trim();
            if !safe_name(id) {
                continue;
            }
            for (input, kind) in [(false, "playback"), (true, "capture")] {
                if rest.contains(kind) {
                    routes.push(json!({"name":format!("hw:CARD={id},DEV={device}"),
                    "label":rest.split(':').next().unwrap_or(rest).trim(),"backend":"ALSA direct (exclusive)","input":input}));
                }
            }
        }
    }
    *ROUTES.get_or_init(Default::default).lock().unwrap() = routes.clone();
    routes
}
pub fn route(name: &str, input: bool) -> Option<Value> {
    ROUTES
        .get_or_init(Default::default)
        .lock()
        .unwrap()
        .iter()
        .find(|v| v["name"] == name && v["input"] == input)
        .cloned()
}
// pactl versions expose named collections as either arrays or keyed objects.
fn named(value: &Value) -> Vec<Value> {
    let values: Vec<(String, &Value)> = match value {
        Value::Array(items) => items
            .iter()
            .take(256)
            .map(|v| (text(&v["name"]), v))
            .collect(),
        Value::Object(items) => items
            .iter()
            .take(256)
            .map(|(k, v)| (k.clone(), v))
            .collect(),
        _ => vec![],
    };
    values.into_iter().map(|(name,v)| json!({"name":name,"description":text(&v["description"]),
        "availability":text(v.get("availability").or_else(||v.get("available")).unwrap_or(&Value::Null)),
        "type":text(&v["type"])})).collect()
}
fn pulse(info: &Value, sinks: &Value, sources: &Value, cards: &Value) -> Result<Value, String> {
    if !info.is_object() || !sinks.is_array() || !sources.is_array() || !cards.is_array() {
        return Err("pactl: unexpected inventory shape".into());
    }
    let endpoints = |rows: &Value, direction: &str, default: &Value| {
        rows.as_array().unwrap().iter().take(256).map(|v| {
        let props=&v["properties"];
        json!({"name":text(&v["name"]),"description":text(&v["description"]),"direction":direction,
            "default":v["name"].is_string() && &v["name"]==default,"state":text(&v["state"]),
            "card":v["card"].as_u64(),"channel_map":text(&v["channel_map"]),
            "sample_specification":text(&v["sample_specification"]),"active_port":text(&v["active_port"]),
            "ports":named(&v["ports"]),"device_class":text(&props["device.class"]),
            "bus":text(&props["device.bus"]),"alsa_card":text(&props["alsa.card"]),
            "alsa_device":text(&props["alsa.device"]),"hardware_path":text(&props["device.bus_path"])})
    }).collect::<Vec<_>>()
    };
    let mut nodes = endpoints(sinks, "Output", &info["default_sink_name"]);
    nodes.extend(endpoints(sources, "Input", &info["default_source_name"]));
    let cards = cards
        .as_array()
        .unwrap()
        .iter()
        .take(256)
        .map(|v| {
            json!({
        "name":text(&v["name"]),"description":text(&v["properties"]["device.description"]),
        "index":v["index"].as_u64(),"active_profile":text(&v["active_profile"]),
        "profiles":named(&v["profiles"]),"ports":named(&v["ports"])})
        })
        .collect::<Vec<_>>();
    Ok(
        json!({"available":true,"backend":text(&info["server_name"]),"endpoints":nodes,"cards":cards}),
    )
}
fn pipewire(dump: &Value) -> Result<Value, String> {
    let rows = dump.as_array().ok_or("pw-dump: expected an array")?;
    let nodes=rows.iter().filter_map(|v| {
        let p=&v["info"]["props"];
        let direction=match p["media.class"].as_str()? {"Audio/Sink"=>"Output","Audio/Source"|"Audio/Source/Virtual"=>"Input",_=>return None};
        Some(json!({"name":text(&p["node.name"]),"description":text(&p["node.description"]),
            "direction":direction,"state":text(&v["info"]["state"]),"ports":[],
            "channel_map":p["audio.position"].as_str().map(str::to_owned).unwrap_or_else(||if p["audio.position"].is_array(){p["audio.position"].to_string()}else{String::new()}),
            "bus":text(&p["device.api"]),"hardware_path":text(&p["object.path"])}))
    }).take(256).collect::<Vec<_>>();
    Ok(
        json!({"available":true,"backend":"PipeWire (native snapshot)","endpoints":nodes,"cards":[],
        "notice":"PulseAudio compatibility discovery was unavailable. Native PipeWire nodes are shown; port availability, card profiles and defaults are not inferred."}),
    )
}
async fn inspect() -> Value {
    let (info, sinks, sources, cards) = tokio::join!(
        query("pactl", &["--format=json", "info"]),
        query("pactl", &["--format=json", "list", "sinks"]),
        query("pactl", &["--format=json", "list", "sources"]),
        query("pactl", &["--format=json", "list", "cards"])
    );
    let result = info.and_then(|i| {
        sinks.and_then(|s| sources.and_then(|r| cards.and_then(|c| pulse(&i, &s, &r, &c))))
    });
    match result {
        Ok(v) => v,
        Err(pulse_error) => match query("pw-dump", &[]).await.and_then(|v| pipewire(&v)) {
            Ok(mut v) => {
                v["pulse_error"] = json!(pulse_error);
                v
            }
            Err(error) => {
                json!({"available":false,"endpoints":[],"cards":[],"error":format!("{pulse_error}. {error}. Install pactl (often pulseaudio-utils or libpulse) or pw-dump, and run pr0former as the desktop-session user. No services were started.")})
            }
        },
    }
}
pub async fn get(State(app): State<crate::App>, headers: HeaderMap) -> crate::Api<Json<Value>> {
    crate::accounts::admin(&app, &headers)?;
    if !cfg!(target_os = "linux") || !app.config.native_devices {
        return Ok(Json(json!({"supported":false})));
    }
    static CACHE: tokio::sync::Mutex<Option<(Instant, Value)>> =
        tokio::sync::Mutex::const_new(None);
    let mut cache = CACHE.lock().await;
    if cache
        .as_ref()
        .is_none_or(|(at, _)| at.elapsed() >= Duration::from_secs(5))
    {
        let mut v = inspect().await;
        v["supported"] = json!(true);
        *cache = Some((Instant::now(), v));
    }
    Ok(Json(cache.as_ref().unwrap().1.clone()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pulse_lists_hdmi_ports_profiles_defaults_and_monitors() {
        let info = json!({"server_name":"PulseAudio (on PipeWire)","default_sink_name":"hdmi"});
        let sinks = json!([{"name":"hdmi","description":"Display HDMI","active_port":"hdmi-output-0","ports":[{"name":"hdmi-output-0","description":"HDMI","availability":"available"}],"properties":{"alsa.card":"1","alsa.device":"3"}}]);
        let sources = json!([{"name":"hdmi.monitor","properties":{"device.class":"monitor"}}]);
        let cards = json!([{"name":"card","active_profile":"output:hdmi-stereo","profiles":{"output:hdmi-stereo":{"description":"HDMI stereo","available":"yes"}},"ports":{"analog-output":{"description":"Line out","availability":"not available"}}}]);
        let v = pulse(&info, &sinks, &sources, &cards).unwrap();
        assert_eq!(v["endpoints"][0]["default"], true);
        assert_eq!(v["endpoints"][0]["alsa_device"], "3");
        assert_eq!(v["endpoints"][1]["device_class"], "monitor");
        assert_eq!(v["cards"][0]["ports"][0]["name"], "analog-output");
        assert_eq!(v["cards"][0]["profiles"][0]["availability"], "yes");
        assert!(pulse(&info, &Value::Null, &sources, &cards).is_err());
    }
    #[test]
    fn pipewire_fallback_does_not_label_streams_as_physical_devices() {
        let dump = json!([{"info":{"props":{"media.class":"Audio/Sink","node.name":"analog","node.description":"Analog stereo","audio.position":["FL","FR"]}}},{"info":{"props":{"media.class":"Stream/Output/Audio","node.name":"player"}}}]);
        let v = pipewire(&dump).unwrap();
        assert_eq!(v["endpoints"].as_array().unwrap().len(), 1);
        assert_eq!(v["endpoints"][0]["description"], "Analog stereo");
        assert!(v["endpoints"][0].get("default").is_none());
        assert!(pulse_routes(&v).is_empty());
        assert!(pipewire(&json!({})).is_err());
    }
    #[tokio::test]
    async fn named_routes_use_safe_names_and_endpoint_channels() {
        let v = json!({"endpoints":[{"name":"alsa_output.usb-Studio.analog-stereo","description":"Studio USB","direction":"Output","channel_map":"front-left,front-right"},{"name":"x.monitor","direction":"Input","channel_map":"mono"},{"name":"bad\",DEVICE=default","channel_map":"mono"}]});
        let routes = pulse_routes(&v);
        assert_eq!(routes.len(), 1);
        assert_eq!(
            routes[0]["name"],
            "pulse:DEVICE=alsa_output.usb-Studio.analog-stereo"
        );
        assert_eq!(routes[0]["channels"], 2);
        assert!(!safe_name(""));
        assert!(!safe_name("../../pcm"));
    }
    #[tokio::test]
    async fn missing_tool_has_actionable_error() {
        assert!(
            query("pr0former-nonexistent-audio-tool", &[])
                .await
                .unwrap_err()
                .contains("pr0former-nonexistent-audio-tool")
        );
    }
}
