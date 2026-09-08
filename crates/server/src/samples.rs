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
    let rate = crate::settings::read().sample_rate;
    let project_id = id.clone();
    tokio::task::spawn_blocking(move || cache_asset(&project_id, asset, rate))
        .await
        .map_err(bad)?
        .map_err(bad)?;
    app.logs.push(
        &id,
        "info",
        &format!("Imported sample {asset}; cached at {rate} Hz (original retained)"),
    );
    Ok(Json(
        json!({"asset":asset,"channels":spec.channels,"sample_rate":spec.sample_rate}),
    ))
}
pub fn prepare(project: &pr0_core::Project) -> Result<pr0_dsp::Engine, String> {
    let rate = crate::settings::read().sample_rate;
    crate::settings::validate_routes(project, &crate::settings::read())?;
    cache_project(project, rate)?;
    let mut engine = pr0_dsp::Engine::prepare(project.graph.clone(), rate as f64)?;
    let mut total = 0;
    for node in &project.graph.nodes {
        if !matches!(
            node.kind.as_str(),
            "sample" | "phase_vocoder" | "poly_sampler"
        ) {
            continue;
        }
        let asset = node.parameters.get("asset").copied().unwrap_or(0.) as u32;
        if asset == 0 {
            continue;
        }
        let mut reader = hound::WavReader::open(cache_path(&project.id, asset, rate))
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
        let length = (input_frames as f64 * rate as f64 / spec.sample_rate as f64).round() as usize;
        let mut frames = Vec::with_capacity(length);
        for i in 0..length {
            let position = i as f64 * spec.sample_rate as f64 / rate as f64;
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

fn cache_path(project: &str, asset: u32, rate: u32) -> std::path::PathBuf {
    directory(project).join(format!("{asset}-{rate}-v1.wav"))
}
pub fn cache_project(project: &pr0_core::Project, rate: u32) -> Result<(), String> {
    let mut assets = std::collections::BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(directory(&project.id)) {
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().and_then(|s| s.to_str()) == Some("wav") {
                if let Some(asset) = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .and_then(|s| s.parse::<u32>().ok())
                {
                    assets.insert(asset);
                }
            }
        }
    }
    for node in &project.graph.nodes {
        if matches!(
            node.kind.as_str(),
            "sample" | "phase_vocoder" | "poly_sampler"
        ) {
            let asset = node.parameters.get("asset").copied().unwrap_or(0.) as u32;
            if asset != 0 {
                assets.insert(asset);
            }
        }
    }
    for asset in assets {
        cache_asset(&project.id, asset, rate)?;
    }
    Ok(())
}
fn cache_asset(project: &str, asset: u32, rate: u32) -> Result<(), String> {
    let dest = cache_path(project, asset, rate);
    if dest.exists() {
        return Ok(());
    }
    let mut reader = hound::WavReader::open(directory(project).join(format!("{asset}.wav")))
        .map_err(|e| e.to_string())?;
    let spec = reader.spec();
    let raw: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
        reader
            .samples::<f32>()
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    } else {
        let scale = 2_f32.powi(spec.bits_per_sample as i32 - 1);
        reader
            .samples::<i32>()
            .map(|v| v.map(|x| x as f32 / scale))
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?
    };
    if raw.iter().any(|v| !v.is_finite()) {
        return Err("WAV contains nonfinite samples".into());
    }
    let pcm = resample(&raw, spec.channels as usize, spec.sample_rate, rate);
    let tmp = dest.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let mut writer = hound::WavWriter::create(
        &tmp,
        hound::WavSpec {
            channels: spec.channels,
            sample_rate: rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        },
    )
    .map_err(|e| e.to_string())?;
    for v in pcm {
        writer.write_sample(v).map_err(|e| e.to_string())?;
    }
    writer.finalize().map_err(|e| e.to_string())?;
    std::fs::rename(tmp, dest).map_err(|e| e.to_string())
}
/// Windowed-sinc conversion with a low-pass cutoff when downsampling. Preparation only.
pub fn resample(raw: &[f32], channels: usize, from: u32, to: u32) -> Vec<f32> {
    if from == to {
        return raw.to_vec();
    }
    let frames = raw.len() / channels;
    let length = (frames as f64 * to as f64 / from as f64).round() as usize;
    let cutoff = (to as f64 / from as f64).min(1.) * 0.94;
    let mut output = vec![0.; length * channels];
    for i in 0..length {
        let position = i as f64 * from as f64 / to as f64;
        let center = position.floor() as isize;
        let mut weight = 0.;
        for k in center - 32..=center + 32 {
            if k < 0 || k >= frames as isize {
                continue;
            }
            let x = position - k as f64;
            let a = std::f64::consts::PI * x * cutoff;
            let w = if a.abs() < 1e-10 {
                cutoff
            } else {
                a.sin() / a * cutoff
            } * (0.5 + 0.5 * (std::f64::consts::PI * x / 33.).cos());
            weight += w;
            for ch in 0..channels {
                output[i * channels + ch] += (raw[k as usize * channels + ch] as f64 * w) as f32;
            }
        }
        if weight.abs() > 1e-10 {
            for ch in 0..channels {
                output[i * channels + ch] /= weight as f32;
            }
        }
    }
    output
}
#[cfg(test)]
mod tests {
    #[test]
    fn conversion_preserves_duration_and_dc() {
        let converted = super::resample(&vec![0.25; 4800 * 2], 2, 48000, 44100);
        assert_eq!(converted.len(), 4410 * 2);
        assert!(converted.iter().all(|v| (*v - 0.25).abs() < 1e-5));
    }
    #[test]
    fn downsampling_filters_above_nyquist() {
        let raw: Vec<f32> = (0..9600)
            .map(|i| (i as f32 * std::f32::consts::TAU * 30000. / 96000.).sin())
            .collect();
        let converted = super::resample(&raw, 1, 96000, 44100);
        let rms = (converted[100..converted.len() - 100]
            .iter()
            .map(|v| v * v)
            .sum::<f32>()
            / (converted.len() - 200) as f32)
            .sqrt();
        assert!(rms < 0.01, "{rms}");
    }
}

/// Stateful streaming rate adapter for WebRTC's fixed 48 kHz contract.
/// Retains filter history and fractional phase across packets. Not called by DSP/callbacks.
pub struct RateAdapter {
    from: u32,
    to: u32,
    samples: Vec<f32>,
    phase: u64,
}
impl RateAdapter {
    pub fn new(from: u32, to: u32) -> Self {
        Self {
            from,
            to,
            samples: vec![],
            phase: 0,
        }
    }
    pub fn process(&mut self, pcm: &[f32]) -> Vec<f32> {
        if self.from == self.to {
            return pcm.to_vec();
        }
        self.samples.extend_from_slice(pcm);
        let frames = self.samples.len() / 2;
        let cutoff = (self.to as f64 / self.from as f64).min(1.) * 0.94;
        let mut result = Vec::new();
        while self.phase as f64 / self.to as f64 + 33. < frames as f64 {
            let position = self.phase as f64 / self.to as f64;
            let center = position.floor() as isize;
            let mut value = [0_f64; 2];
            let mut sum = 0.;
            for k in center - 32..=center + 32 {
                if k < 0 {
                    continue;
                }
                let x = position - k as f64;
                let a = std::f64::consts::PI * x * cutoff;
                let w = if a.abs() < 1e-10 {
                    cutoff
                } else {
                    a.sin() / a * cutoff
                } * (0.5 + 0.5 * (std::f64::consts::PI * x / 33.).cos());
                sum += w;
                for ch in 0..2 {
                    value[ch] += self.samples[k as usize * 2 + ch] as f64 * w;
                }
            }
            result.extend(value.map(|v| (v / sum) as f32));
            self.phase += self.from as u64;
        }
        let consumed = (self.phase as usize / self.to as usize).saturating_sub(33);
        self.samples.drain(..consumed * 2);
        self.phase -= consumed as u64 * self.to as u64;
        result
    }
}
#[cfg(test)]
mod streaming_tests {
    #[test]
    fn packet_boundaries_do_not_change_conversion() {
        for (from, to) in [
            (44100, 48000),
            (48000, 44100),
            (96000, 48000),
            (48000, 88200),
        ] {
            let input: Vec<f32> = (0..4800)
                .flat_map(|i| [(i as f32 * 0.1).sin(); 2])
                .collect();
            let expected = super::RateAdapter::new(from, to).process(&input);
            let mut adapter = super::RateAdapter::new(from, to);
            let actual: Vec<f32> = input.chunks(256).flat_map(|p| adapter.process(p)).collect();
            assert_eq!(expected.len(), actual.len());
            assert!(
                expected
                    .iter()
                    .zip(&actual)
                    .all(|(a, b)| (a - b).abs() < 1e-5)
            );
        }
    }
}
