use crate::{Api, App, bad, can_edit, csrf, load, role, user};
use axum::{
    Json,
    extract::{Multipart, Path, State},
    http::HeaderMap,
};
use serde_json::Value;

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
    let _guard = app.setup.lock().await;
    let field = form
        .next_field()
        .await
        .map_err(bad)?
        .ok_or_else(|| bad("Select an audio file"))?;
    let name = field
        .file_name()
        .unwrap_or("Imported sample")
        .chars()
        .take(256)
        .collect::<String>();
    let bytes = field.bytes().await.map_err(bad)?;
    let temporary =
        ImportDirectory(std::env::temp_dir().join(format!("pr0-audio-{}", uuid::Uuid::new_v4())));
    tokio::fs::create_dir(&temporary.0).await.map_err(bad)?;
    let source = temporary.0.join("input");
    let converted = temporary.0.join("converted.wav");
    tokio::fs::write(&source, &bytes).await.map_err(bad)?;
    let rate = crate::settings::read().sample_rate;
    // fd is seekable but cannot open nested local files or network URLs from uploaded playlists.
    let mut command = tokio::process::Command::new("ffmpeg");
    command
        .kill_on_drop(true)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-threads",
            "1",
            "-protocol_whitelist",
            "fd",
            "-i",
            "fd:",
            "-map",
            "0:a:0",
            "-vn",
            "-sn",
            "-dn",
            "-map_metadata",
            "-1",
            "-t",
            "31",
            "-ar",
        ])
        .arg(rate.to_string())
        .args(["-c:a", "pcm_f32le", "-fs", "200000000", "-f", "wav"])
        .arg(&converted)
        .stdin(std::process::Stdio::from(
            std::fs::File::open(&source).map_err(bad)?,
        ));
    let output = tokio::time::timeout(std::time::Duration::from_secs(60), command.output())
        .await
        .map_err(|_| bad("Audio conversion timed out"))?
        .map_err(|e| bad(format!("FFmpeg is required for audio import: {e}")))?;
    if !output.status.success() {
        return Err(bad(format!(
            "Audio conversion failed. Install FFmpeg with the fd protocol and select a supported audio file: {}",
            String::from_utf8_lossy(&output.stderr)
                .chars()
                .take(600)
                .collect::<String>()
        )));
    }
    let wav = hound::WavReader::open(&converted).map_err(bad)?;
    let spec = wav.spec();
    let frames = wav.duration();
    if !(1..=8).contains(&spec.channels) || frames == 0 || frames > rate * 30 {
        return Err(bad("Use audio with 1–8 channels and at most 30 seconds"));
    }
    drop(wav);
    let result = crate::sample_library::register(&app, &id, &u, &name, &converted, None).await?;
    let asset = result["asset"].as_u64().unwrap() as u32;
    let project = id.clone();
    tokio::task::spawn_blocking(move || cache_asset(&project, asset, rate))
        .await
        .map_err(bad)?
        .map_err(bad)?;
    app.logs.push(
        &id,
        "info",
        &format!(
            "Imported sample {asset}: {rate} Hz float WAV, {} channels",
            spec.channels
        ),
    );
    let _ = app
        .events
        .send(serde_json::json!({"type":"samples","project_id":id}));
    Ok(Json(result))
}
struct ImportDirectory(std::path::PathBuf);
impl Drop for ImportDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub fn prepare(project: &pr0_core::Project) -> Result<pr0_dsp::Engine, String> {
    let rate = crate::settings::read().sample_rate;
    crate::settings::validate_routes(project, &crate::settings::read())?;
    cache_project(project, rate)?;
    let mut engine = pr0_dsp::Engine::prepare(project.graph.clone(), rate as f64)?;
    let (beats, unit) = project.initial_meter();
    engine.set_meter(beats, unit);
    crate::loops::restore(project, &mut engine)?;
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
pub(crate) fn cache_asset(project: &str, asset: u32, rate: u32) -> Result<(), String> {
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
