//! Portable .pr0 ZIPs. Archive names are never used as filesystem paths.
use crate::{Api, App, bad, can_edit, config::RuntimeConfig, csrf, load, role, samples, user};
use axum::{
    Json,
    extract::{Multipart, Path, State},
    http::{HeaderMap, header},
    response::IntoResponse,
};
use pr0_core::Project;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Cursor, Read, Write},
    path::PathBuf,
};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};
const LIMIT: usize = 256 * 1024 * 1024;

fn assets(p: &Project) -> BTreeSet<u32> {
    let mut ids = BTreeSet::new();
    for n in &p.graph.nodes {
        if [
            "sample",
            "phase_vocoder",
            "poly_sampler",
            "granular_synth",
            "granular_cloud",
            "convolution_reverb",
        ]
        .contains(&n.kind.as_str())
        {
            let id = n.parameters.get("asset").copied().unwrap_or(0.) as u32;
            if id != 0 {
                ids.insert(id);
            }
        }
        ids.extend(n.sample_choices.iter().map(|c| c.asset));
    }
    ids
}
pub(crate) fn validate_assets(config: &RuntimeConfig, p: &Project) -> Result<(), String> {
    for id in assets(p) {
        if !samples::directory(config, &p.id)
            .join(format!("{id}.wav"))
            .is_file()
        {
            return Err(format!(
                "Sample {id} is missing. Export the source project as a .pr0 bundle including samples; legacy JSON does not contain audio."
            ));
        }
    }
    Ok(())
}
fn portable(p: &mut Project) {
    p.conductor = None;
    p.local_audio_assignments.clear();
    p.conducted.midi_bindings.clear();
    for part in &mut p.parts {
        part.performer = None;
    }
}
fn encode(config: &RuntimeConfig, mut p: Project) -> Result<Vec<u8>, String> {
    validate_assets(config, &p)?;
    let dir = samples::directory(config, &p.id);
    let mut ids = assets(&p);
    // Include unused imported samples too, but never generated resampling caches.
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for entry in entries {
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().is_some_and(|x| x == "wav") {
                if let Some(id) = path
                    .file_stem()
                    .and_then(|x| x.to_str())
                    .and_then(|x| x.parse::<u32>().ok())
                {
                    ids.insert(id);
                }
            }
        }
    }
    if ids.len() > 1024 { return Err("Project bundle exceeds 1,024 samples".into()); }
    portable(&mut p);
    p.id.clear();
    p.revision = 0;
    let json = serde_json::to_vec_pretty(&serde_json::json!({"format":"pr0former-project", "version":1, "project_schema":1, "software":{"name":"pr0former", "version":env!("CARGO_PKG_VERSION"), "build":crate::build_info::json()}, "project":p})).map_err(|e| e.to_string())?;
    let mut total = json.len();
    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("project.json", options)
        .map_err(|e| e.to_string())?;
    zip.write_all(&json).map_err(|e| e.to_string())?;
    for id in ids {
        let path = dir.join(format!("{id}.wav"));
        let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
        if size > LIMIT as u64 || total.saturating_add(size as usize) > LIMIT {
            return Err("Project bundle exceeds 256 MiB".into());
        }
        total += size as usize;
        zip.start_file(format!("samples/{id}.wav"), options)
            .map_err(|e| e.to_string())?;
        let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        std::io::copy(&mut file, &mut zip).map_err(|e| e.to_string())?;
    }
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}
fn decode(bytes: Vec<u8>) -> Result<(Project, BTreeMap<u32, Vec<u8>>), String> {
    let mut zip =
        ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("Invalid .pr0 ZIP: {e}"))?;
    if zip.len() > 1025 {
        return Err("Too many bundle entries".into());
    }
    let mut project = None;
    let mut files = BTreeMap::new();
    let mut total = 0usize;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(|e| e.to_string())?;
        let name = file.name().to_string();
        if file.is_dir() && name == "samples/" {
            continue;
        }
        let id = name
            .strip_prefix("samples/")
            .and_then(|s| s.strip_suffix(".wav"))
            .and_then(|s| s.parse::<u32>().ok())
            .filter(|id| *id > 0 && name == format!("samples/{id}.wav"));
        if name != "project.json" && id.is_none() {
            return Err(format!("Unsupported bundle entry: {name}"));
        }
        let max = if name == "project.json" {
            4 * 1024 * 1024
        } else {
            LIMIT - total
        };
        if file.size() > max as u64 {
            return Err("Expanded bundle exceeds its size limit".into());
        }
        let mut data = Vec::new();
        file.by_ref()
            .take(max as u64 + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        total = total.saturating_add(data.len());
        if data.len() > max || total > LIMIT {
            return Err("Expanded bundle exceeds 256 MiB".into());
        }
        if let Some(id) = id {
            let mut wav = hound::WavReader::new(Cursor::new(&data))
                .map_err(|e| format!("Sample {id}: {e}"))?;
            let spec = wav.spec();
            if !(1..=8).contains(&spec.channels)
                || spec.sample_rate == 0
                || wav.duration() == 0
                || wav.duration() as u64 > spec.sample_rate as u64 * 30
            {
                return Err(format!(
                    "Sample {id}: use 1–8 channels and at most 30 seconds"
                ));
            }
            if spec.sample_format == hound::SampleFormat::Float {
                for s in wav.samples::<f32>() {
                    if !s.map_err(|e| e.to_string())?.is_finite() {
                        return Err("Nonfinite sample audio".into());
                    }
                }
            } else {
                for s in wav.samples::<i32>() {
                    s.map_err(|e| e.to_string())?;
                }
            }
            if files.insert(id, data).is_some() {
                return Err("Duplicate sample entry".into());
            }
        } else {
            if project.is_some() {
                return Err("Duplicate project.json".into());
            }
            let v: serde_json::Value = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
            if v["format"] != "pr0former-project" || v["version"] != 1 || v["project_schema"] != 1 {
                return Err("Unsupported project bundle/schema version. Update pr0former before importing this file.".into());
            }
            check_version(
                v["software"]["version"]
                    .as_str()
                    .ok_or("Bundle is missing the exporting software version")?,
            )?;
            project = Some(
                serde_json::from_value::<Project>(v["project"].clone())
                    .map_err(|e| e.to_string())?,
            );
        }
    }
    let mut p = project.ok_or("Bundle is missing project.json")?;
    portable(&mut p);
    p.validate()?;
    for id in assets(&p) {
        if !files.contains_key(&id) {
            return Err(format!("Bundle is missing samples/{id}.wav"));
        }
    }
    Ok((p, files))
}
fn check_version(exported: &str) -> Result<(), String> {
    fn version(value: &str) -> Option<(u32, u32, u32)> {
        let mut parts = value.split(['-', '+']).next()?.split('.');
        let result = (
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
            parts.next()?.parse().ok()?,
        );
        if parts.next().is_some() {
            None
        } else {
            Some(result)
        }
    }
    let incoming = version(exported).ok_or("Invalid exporting software version")?;
    let current = version(env!("CARGO_PKG_VERSION")).unwrap();
    if incoming > current {
        return Err(format!(
            "This bundle was exported by pr0former {exported}; this server is {}. Update the server before importing.",
            env!("CARGO_PKG_VERSION")
        ));
    }
    Ok(())
}
async fn upload(mut form: Multipart) -> Api<(Vec<u8>, BTreeMap<String, String>)> {
    let mut bytes = None;
    let mut fields = BTreeMap::new();
    while let Some(field) = form.next_field().await.map_err(bad)? {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            if bytes.is_some() {
                return Err(bad("Select one bundle"));
            }
            bytes = Some(field.bytes().await.map_err(bad)?.to_vec());
        } else {
            let value = field.bytes().await.map_err(bad)?;
            if value.len() > 512 || !["name", "target", "revision"].contains(&name.as_str()) {
                return Err(bad("Invalid import option"));
            }
            fields.insert(name, String::from_utf8(value.to_vec()).map_err(bad)?);
        }
    }
    Ok((bytes.ok_or_else(|| bad("Select a .pr0 bundle"))?, fields))
}
pub async fn export(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<impl IntoResponse> {
    let u = user(&app, &headers)?;
    role(&app, &id, &u)?;
    let _guard = app.setup.lock().await;
    let p = load(&app, &id)?;
    let config = app.config.clone();
    let data = tokio::task::spawn_blocking(move || encode(&config, p))
        .await
        .map_err(bad)?
        .map_err(bad)?;
    Ok((
        [
            (header::CONTENT_TYPE, "application/zip"),
            (
                header::CONTENT_DISPOSITION,
                "attachment; filename=project.pr0",
            ),
        ],
        data,
    ))
}
pub async fn inspect(
    State(app): State<App>,
    headers: HeaderMap,
    form: Multipart,
) -> Api<Json<Project>> {
    csrf(&headers)?;
    user(&app, &headers)?;
    let (bytes, _) = upload(form).await?;
    let (p, _) = tokio::task::spawn_blocking(move || decode(bytes))
        .await
        .map_err(bad)?
        .map_err(bad)?;
    Ok(Json(p))
}
struct Staged {
    paths: Vec<PathBuf>,
    keep: bool,
}
impl Drop for Staged {
    fn drop(&mut self) {
        if !self.keep {
            for path in &self.paths {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}
pub async fn import(
    State(app): State<App>,
    headers: HeaderMap,
    form: Multipart,
) -> Api<Json<Project>> {
    csrf(&headers)?;
    let owner = user(&app, &headers)?;
    let (bytes, fields) = upload(form).await?;
    let (mut p, files) = tokio::task::spawn_blocking(move || decode(bytes))
        .await
        .map_err(bad)?
        .map_err(bad)?;
    let guard = app.setup.lock().await;
    let target = fields.get("target");
    if let Some(id) = target {
        can_edit(&role(&app, id, &owner)?)?;
        load(&app, id)?;
    }
    p.id = target.cloned().unwrap_or_else(crate::uid);
    if let Some(name) = fields.get("name") {
        p.name = name.trim().to_string();
    }
    p.revision = if target.is_some() {
        fields
            .get("revision")
            .ok_or_else(|| bad("Overwrite revision required"))?
            .parse()
            .map_err(bad)?
    } else {
        0
    };
    p.validate().map_err(bad)?;
    let dir = samples::directory(&app.config, &p.id);
    tokio::fs::create_dir_all(&dir).await.map_err(bad)?;
    let mut staged = Staged {
        paths: vec![],
        keep: false,
    };
    let mut mapping = BTreeMap::new();
    for (old, bytes) in files {
        // create_new prevents replacing audio used by existing project revisions.
        let (id, path, mut file) = loop {
            let id = (uuid::Uuid::new_v4().as_u128() % 999999999 + 1) as u32;
            let path = dir.join(format!("{id}.wav"));
            match tokio::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .await
            {
                Ok(file) => break (id, path, file),
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(bad(e)),
            }
        };
        staged.paths.push(path);
        tokio::io::AsyncWriteExt::write_all(&mut file, &bytes)
            .await
            .map_err(bad)?;
        mapping.insert(old, id);
    }
    for n in &mut p.graph.nodes {
        if [
            "sample",
            "phase_vocoder",
            "poly_sampler",
            "granular_synth",
            "granular_cloud",
            "convolution_reverb",
        ]
        .contains(&n.kind.as_str())
        {
            if let Some(asset) = n.parameters.get_mut("asset") {
                if let Some(id) = mapping.get(&(*asset as u32)) {
                    *asset = *id as f64;
                }
            }
        }
        for c in &mut n.sample_choices {
            c.asset = mapping[&c.asset];
        }
    }
    let result = if let Some(id) = target {
        drop(guard);
        crate::update_project(State(app.clone()), headers, Path(id.clone()), Json(p)).await
    } else {
        crate::store_import(&app, owner, p).await
    };
    // A live-update notification can fail after persistence. Never remove audio
    // already referenced by the committed project in that case.
    if result.is_ok()
        || target.is_some_and(|id| {
            load(&app, id).ok().is_some_and(|p| {
                assets(&p)
                    .iter()
                    .any(|id| mapping.values().any(|mapped| mapped == id))
            })
        })
    {
        staged.keep = true;
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bundle(entries: Vec<(&str, Vec<u8>)>) -> Vec<u8> {
        let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            zip.start_file(name, SimpleFileOptions::default()).unwrap();
            zip.write_all(&bytes).unwrap();
        }
        zip.finish().unwrap().into_inner()
    }
    #[test]
    fn exports_versioned_manifest_and_reads_it() {
        let p = pr0_core::demo_project(crate::uid(), "Bundle".into(), pr0_core::Mode::Structured);
        let encoded = encode(&RuntimeConfig::new(std::env::temp_dir()), p).unwrap();
        let mut zip = ZipArchive::new(Cursor::new(&encoded)).unwrap();
        let manifest: serde_json::Value =
            serde_json::from_reader(zip.by_name("project.json").unwrap()).unwrap();
        assert_eq!(manifest["software"]["version"], env!("CARGO_PKG_VERSION"));
        assert!(manifest["software"]["build"]["git_commit"].is_string());
        assert_eq!(manifest["project_schema"], 1);
        assert_eq!(decode(encoded).unwrap().0.name, "Bundle");
    }
    #[test]
    fn rejects_traversal_unknown_formats_and_newer_engines() {
        assert!(
            decode(bundle(vec![("../outside.wav", vec![])]))
                .unwrap_err()
                .contains("Unsupported bundle entry")
        );
        assert!(
            decode(bundle(vec![(
                "project.json",
                b"{\"version\":999}".to_vec()
            )]))
            .unwrap_err()
            .contains("version")
        );
        assert!(check_version("999.0.0").unwrap_err().contains("Update"));
        assert!(check_version("0.0.1").is_ok());
        assert!(check_version("invalid").is_err());
    }
}
