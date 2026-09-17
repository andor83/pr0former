use crate::{
    Api, App, bad, can_edit,
    config::RuntimeConfig,
    csrf,
    import::{ImportLimits, ImportSource},
    internal, load, role, user,
};
use axum::{
    Json,
    extract::{Multipart, Path, State},
    http::HeaderMap,
};
use serde_json::Value;

pub fn directory(config: &RuntimeConfig, project: &str) -> std::path::PathBuf {
    config.project_samples_dir(project)
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
    drop(bytes);
    let rate = crate::settings::read(&app.config).sample_rate;
    // The runtime never discovers or launches a converter itself: it asks the
    // capability the host installed. Decoding, resampling and WAV writing are
    // CPU-bound and run on a blocking worker, never on an async executor and
    // never anywhere near render or device callbacks.
    let importer = app.config.importer.clone();
    let limits = ImportLimits::for_rate(rate);
    let input = ImportSource::file(&source);
    let destination = converted.clone();
    let channels = tokio::task::spawn_blocking(move || {
        importer
            .decode(&input, &limits)
            .and_then(|audio| audio.write_wav(&destination).map(|()| audio.channels()))
    })
    .await
    .map_err(internal)?
    .map_err(bad)?;
    let result = crate::sample_library::register(&app, &id, &u, &name, &converted, None).await?;
    let asset = result["asset"].as_u64().unwrap() as u32;
    let project = id.clone();
    let config = app.config.clone();
    tokio::task::spawn_blocking(move || cache_asset(&config, &project, asset, rate))
        .await
        .map_err(bad)?
        .map_err(bad)?;
    app.logs.push(
        &id,
        "info",
        &format!("Imported sample {asset}: {rate} Hz float WAV, {channels} channels"),
    );
    let _ = app
        .events
        .send(serde_json::json!({"type":"samples","project_id":id}).into());
    Ok(Json(result))
}
struct ImportDirectory(std::path::PathBuf);
impl Drop for ImportDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Project-scoped numeric IDs never name arbitrary filesystem paths.
pub fn validate_choices(config: &RuntimeConfig, project: &pr0_core::Project) -> Result<(), String> {
    for choice in project.graph.nodes.iter().flat_map(|n| &n.sample_choices) {
        if !directory(config, &project.id)
            .join(format!("{}.wav", choice.asset))
            .is_file()
        {
            return Err(format!(
                "Sample {} is not in this project; add it from the sample library first",
                choice.asset
            ));
        }
    }
    Ok(())
}

pub fn prepare(
    config: &RuntimeConfig,
    project: &pr0_core::Project,
) -> Result<pr0_dsp::Engine, String> {
    let settings = crate::settings::read(config);
    let rate = settings.sample_rate;
    crate::settings::validate_routes(project, &settings)?;
    validate_choices(config, project)?;
    cache_project(config, project, rate)?;
    let mut engine = pr0_dsp::Engine::prepare(project.graph.clone(), rate as f64)?;
    crate::scripts::prepare(&mut engine, &project.graph, settings.block_size)?;
    let (beats, unit) = project.initial_meter();
    engine.set_meter(beats, unit);
    crate::loops::restore(&config.loops_dir(), project, &mut engine)?;
    let mut total = 0;
    let shortlist: std::collections::BTreeSet<u32> = project
        .graph
        .nodes
        .iter()
        .flat_map(|n| n.sample_choices.iter().map(|s| s.asset))
        .collect();
    let flat = project.graph.flatten()?;
    for node in &project.graph.nodes {
        if !matches!(
            node.kind.as_str(),
            "sample" | "phase_vocoder" | "poly_sampler" | "granular_synth" | "granular_cloud" | "granular_field" | "convolution_reverb"
        ) {
            continue;
        }
        let default_asset = node.parameters.get("asset").copied().unwrap_or(0.) as u32;
        let dynamic = matches!(
            node.kind.as_str(),
            "sample" | "poly_sampler" | "granular_synth" | "granular_cloud" | "granular_field"
        );
        // A Granular Field always needs its own slot list; any cabled Sample N setter may
        // also point at anything on the project shortlist.
        let field = node.kind == "granular_field";
        let own: std::collections::BTreeSet<u32> = if field {
            node.sample_choices.iter().map(|s| s.asset).collect()
        } else {
            std::collections::BTreeSet::new()
        };
        let connected = dynamic
            && flat.edges.iter().any(|e| {
                e.target == node.id
                    && (e.target_port == "sample_id"
                        || (field
                            && e.target_port
                                .strip_prefix("sample_")
                                .is_some_and(|v| v.parse::<usize>().is_ok())))
            });
        let mut assets = if connected {
            shortlist.clone()
        } else {
            std::collections::BTreeSet::new()
        };
        assets.extend(own.iter().copied());
        if default_asset != 0 {
            assets.insert(default_asset);
        }
        for asset in assets {
            let mut reader = hound::WavReader::open(cache_path(config, &project.id, asset, rate))
                .map_err(|e| format!("Sample {asset}: {e}"))?;
            let spec = reader.spec();
            if node.kind == "convolution_reverb" {
                if !matches!(spec.channels, 1 | 2) {
                    return Err(format!(
                        "Convolution response must be mono or stereo; sample has {} channels",
                        spec.channels
                    ));
                }
            } else if spec.channels as usize != node.channels {
                if asset != default_asset && !own.contains(&asset) {
                    continue;
                }
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
            let length =
                (input_frames as f64 * rate as f64 / spec.sample_rate as f64).round() as usize;
            if total + length > 8_000_000 {
                return Err("Prepared sample memory exceeds 256 MB".into());
            }
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
                if node.kind == "convolution_reverb" && channels == 1 {
                    frame[1] = frame[0];
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
            if dynamic {
                engine.add_sample_choice(&node.id, asset, frames);
            } else {
                engine.set_sample(&node.id, frames);
            }
        }
    }
    crate::performance::Sequencer::prepare_players(project, &mut engine)?;
    Ok(engine)
}

fn cache_path(config: &RuntimeConfig, project: &str, asset: u32, rate: u32) -> std::path::PathBuf {
    directory(config, project).join(format!("{asset}-{rate}-v1.wav"))
}
pub fn cache_project(
    config: &RuntimeConfig,
    project: &pr0_core::Project,
    rate: u32,
) -> Result<(), String> {
    let mut assets = std::collections::BTreeSet::new();
    if let Ok(entries) = std::fs::read_dir(directory(config, &project.id)) {
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
            "sample" | "phase_vocoder" | "poly_sampler" | "granular_synth" | "granular_cloud" | "convolution_reverb"
        ) {
            let asset = node.parameters.get("asset").copied().unwrap_or(0.) as u32;
            if asset != 0 {
                assets.insert(asset);
            }
        }
    }
    for asset in assets {
        cache_asset(config, &project.id, asset, rate)?;
    }
    Ok(())
}
pub(crate) fn cache_asset(
    config: &RuntimeConfig,
    project: &str,
    asset: u32,
    rate: u32,
) -> Result<(), String> {
    let dest = cache_path(config, project, asset, rate);
    if dest.exists() {
        return Ok(());
    }
    let mut reader =
        hound::WavReader::open(directory(config, project).join(format!("{asset}.wav")))
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
    phase_step: u32,
    kernels: Vec<[f64; 65]>,
}
impl RateAdapter {
    pub fn new(from: u32, to: u32) -> Self {
        assert!(from > 0 && to > 0, "validated nonzero sample rates");
        let (mut a, mut b) = (from, to);
        while b != 0 {
            (a, b) = (b, a % b);
        }
        let phase_step = a;
        let cutoff = (to as f64 / from as f64).min(1.) * 0.94;
        // Rational rates repeat exactly. Preparing each fractional-phase kernel
        // avoids 130 trig calls per stereo output frame on the audio worker.
        let kernels = if from == to {
            Vec::new()
        } else {
            (0..to / phase_step)
                .map(|phase| {
                    let fraction = (phase * phase_step) as f64 / to as f64;
                    std::array::from_fn(|tap| {
                        let x = fraction + 32. - tap as f64;
                        let a = std::f64::consts::PI * x * cutoff;
                        let sinc = if a.abs() < 1e-10 {
                            cutoff
                        } else {
                            a.sin() / a * cutoff
                        };
                        sinc * (0.5 + 0.5 * (std::f64::consts::PI * x / 33.).cos())
                    })
                })
                .collect()
        };
        Self {
            from,
            to,
            samples: vec![],
            phase: 0,
            phase_step,
            kernels,
        }
    }
    pub fn process(&mut self, pcm: &[f32]) -> Vec<f32> {
        if self.from == self.to {
            return pcm.to_vec();
        }
        self.samples.extend_from_slice(pcm);
        let frames = self.samples.len() / 2;
        let limit = frames.saturating_sub(33) as u64 * self.to as u64;
        let available = limit.saturating_sub(self.phase).div_ceil(self.from as u64) as usize;
        let mut result = Vec::with_capacity(available * 2);
        while self.phase < limit {
            let center = (self.phase / self.to as u64) as isize;
            let phase = (self.phase % self.to as u64) as usize / self.phase_step as usize;
            let kernel = &self.kernels[phase];
            let mut value = [0_f64; 2];
            let mut sum = 0.;
            for (tap, &w) in kernel.iter().enumerate() {
                let k = center + tap as isize - 32;
                if k < 0 {
                    continue;
                }
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
    // Original per-sample trigonometric filter as an independent reference.
    fn reference(input: &[f32], from: u32, to: u32) -> Vec<f32> {
        if from == to {
            return input.to_vec();
        }
        let cutoff = (to as f64 / from as f64).min(1.) * 0.94;
        let mut phase = 0_u64;
        let mut output = Vec::new();
        while phase as f64 / to as f64 + 33. < (input.len() / 2) as f64 {
            let position = phase as f64 / to as f64;
            let center = position.floor() as isize;
            let mut sum = 0.;
            let mut value = [0.; 2];
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
                    value[ch] += input[k as usize * 2 + ch] as f64 * w;
                }
            }
            output.extend(value.map(|v| (v / sum) as f32));
            phase += from as u64;
        }
        output
    }
    #[test]
    fn prepared_resampling_matches_original_filter_at_every_supported_rate() {
        for rate in [44100, 48000, 88200, 96000] {
            for (from, to) in [(rate, 48000), (48000, rate)] {
                let input: Vec<_> = (0..4096)
                    .flat_map(|i| [(i as f32 * 0.37).sin(), (i as f32 * 1.7).cos() * 0.4])
                    .collect();
                let expected = reference(&input, from, to);
                let mut adapter = super::RateAdapter::new(from, to);
                let actual: Vec<_> = input
                    .chunks(62)
                    .flat_map(|chunk| adapter.process(chunk))
                    .collect();
                assert_eq!(actual.len(), expected.len());
                assert!(
                    actual
                        .iter()
                        .zip(&expected)
                        .all(|(a, b)| (a - b).abs() < 1e-6),
                    "{from} -> {to}"
                );
                assert!(
                    adapter.samples.len() <= 136,
                    "streaming history remains bounded"
                );
            }
        }
    }
    #[test]
    #[ignore = "manual software throughput measurement; not a device deadline test"]
    fn resampler_throughput() {
        for from in [44100, 96000] {
            let input: Vec<_> = (0..from)
                .flat_map(|i| [(i as f32 * 0.37).sin(); 2])
                .collect();
            let start = std::time::Instant::now();
            let original = reference(std::hint::black_box(&input), from, 48000);
            let old = start.elapsed();
            let mut adapter = super::RateAdapter::new(from, 48000);
            let start = std::time::Instant::now();
            let result = adapter.process(std::hint::black_box(&input));
            let new = start.elapsed();
            assert_eq!(original.len(), result.len());
            eprintln!(
                "{from} -> 48000 stereo, one second PCM: reference={old:?}, prepared={new:?}, speedup={:.1}x",
                old.as_secs_f64() / new.as_secs_f64()
            );
        }
    }

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
