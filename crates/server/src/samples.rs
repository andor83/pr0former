use crate::{Api, App, bad, can_edit, csrf, load, role, user};
use axum::{
    Json,
    extract::{Multipart, Path, State},
    http::HeaderMap,
};
use serde_json::{Value, json};
use std::io::Cursor;

pub fn directory(project: &str) -> std::path::PathBuf {
    std::path::PathBuf::from(std::env::var("PR0_DATA").unwrap_or("data".into()))
        .join("samples")
        .join(project)
}
pub async fn upload(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
    mut form: Multipart,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    can_edit(&role(&app, &id, &u)?)?;
    load(&app, &id)?;
    let field = form
        .next_field()
        .await
        .map_err(bad)?
        .ok_or_else(|| bad("Select a WAV file"))?;
    let bytes = field.bytes().await.map_err(bad)?;
    let wav = hound::WavReader::new(Cursor::new(&bytes)).map_err(bad)?;
    let spec = wav.spec();
    if !(1..=8).contains(&spec.channels)
        || spec.sample_rate < 8000
        || spec.sample_rate > 192000
        || wav.duration() > spec.sample_rate * 30
    {
        return Err(bad(
            "Use a WAV with 1–8 channels, 8–192 kHz, and at most 30 seconds",
        ));
    }
    if spec.sample_format == hound::SampleFormat::Float && spec.bits_per_sample != 32 {
        return Err(bad("Float WAV must use 32-bit samples"));
    }
    let asset = (uuid::Uuid::new_v4().as_u128() % 999999999 + 1) as u32;
    let dir = directory(&id);
    tokio::fs::create_dir_all(&dir).await.map_err(bad)?;
    use tokio::io::AsyncWriteExt;
    let mut file = tokio::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(dir.join(format!("{asset}.wav")))
        .await
        .map_err(bad)?;
    file.write_all(&bytes).await.map_err(bad)?;
    Ok(Json(
        json!({"asset":asset,"channels":spec.channels,"sample_rate":spec.sample_rate}),
    ))
}
pub fn prepare(project: &pr0_core::Project) -> Result<pr0_dsp::Engine, String> {
    let mut engine = pr0_dsp::Engine::prepare(project.graph.clone(), 48000.)?;
    let mut total = 0;
    for node in &project.graph.nodes {
        if !matches!(node.kind.as_str(), "sample" | "phase_vocoder") {
            continue;
        }
        let asset = node.parameters.get("asset").copied().unwrap_or(0.) as u32;
        if asset == 0 {
            continue;
        }
        let mut reader =
            hound::WavReader::open(directory(&project.id).join(format!("{asset}.wav")))
                .map_err(|e| format!("Sample {asset}: {e}"))?;
        let spec = reader.spec();
        if spec.channels as usize != node.channels {
            return Err(format!(
                "Sample has {} channels; node {} has {}",
                spec.channels, node.label, node.channels
            ));
        }
        let raw: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
            reader
                .samples::<f32>()
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?
        } else {
            let scale = 2_f32.powi(spec.bits_per_sample as i32 - 1);
            reader
                .samples::<i32>()
                .map(|x| x.map(|x| x as f32 / scale))
                .collect::<Result<_, _>>()
                .map_err(|e| e.to_string())?
        };
        let channels = spec.channels as usize;
        let input_frames = raw.len() / channels;
        let length = (input_frames as f64 * 48000. / spec.sample_rate as f64).round() as usize;
        let mut frames = Vec::with_capacity(length);
        for i in 0..length {
            let position = i as f64 * spec.sample_rate as f64 / 48000.;
            let index = position as usize;
            let fraction = (position - index as f64) as f32;
            let mut frame = [0.; 8];
            for ch in 0..channels {
                let a = raw.get(index * channels + ch).copied().unwrap_or(0.);
                let b = raw.get((index + 1) * channels + ch).copied().unwrap_or(a);
                frame[ch] = a + (b - a) * fraction;
            }
            frames.push(frame);
        }
        if node.kind == "phase_vocoder" {
            frames = pr0_dsp::stretch::prepare(
                &frames,
                channels,
                node.parameters.get("speed").copied().unwrap_or(1.),
                node.parameters.get("pitch").copied().unwrap_or(0.),
            );
        }
        total += frames.len();
        if total > 8_000_000 {
            return Err("Prepared sample memory exceeds 256 MB".into());
        }
        engine.set_sample(&node.id, frames);
    }
    Ok(engine)
}
