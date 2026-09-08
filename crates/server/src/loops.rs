//! Project-owned recordings. WAV I/O runs on a dedicated worker, never in DSP render.
use pr0_dsp::Engine;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, SyncSender, TrySendError},
    },
};

type Snapshot = (String, u8, usize, u32, Vec<f32>);
enum Job {
    Save(String, Snapshot),
    Barrier(mpsc::Sender<()>),
}
pub struct Store {
    tx: SyncSender<Job>,
    pending: Option<Job>,
    error: Arc<Mutex<Option<String>>>,
}
fn root() -> PathBuf {
    PathBuf::from(std::env::var("PR0_DATA").unwrap_or("data".into())).join("loops")
}
fn path(root: &Path, project: &str, node: &str, track: u8) -> PathBuf {
    // Encode all user-controlled identifiers; chunking also avoids filename limits.
    let mut path = root.to_path_buf();
    for id in [project, node] {
        path.push("id");
        let encoded: String = id.as_bytes().iter().map(|b| format!("{b:02x}")).collect();
        for chunk in encoded.as_bytes().chunks(64) {
            path.push(std::str::from_utf8(chunk).unwrap());
        }
        path.push("end");
    }
    path.join(format!("{track}.wav"))
}
fn write(root: &Path, project: &str, snapshot: Snapshot) -> Result<(), String> {
    let (node, track, channels, rate, audio) = snapshot;
    let file = path(root, project, &node, track);
    if audio.is_empty() {
        return match std::fs::remove_file(file) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        };
    }
    std::fs::create_dir_all(file.parent().unwrap()).map_err(|e| e.to_string())?;
    let temporary = file.with_extension("wav.tmp");
    let result = (|| {
        let mut writer = hound::WavWriter::create(
            &temporary,
            hound::WavSpec {
                channels: channels as u16,
                sample_rate: rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .map_err(|e| e.to_string())?;
        for sample in audio {
            writer.write_sample(sample).map_err(|e| e.to_string())?;
        }
        writer.finalize().map_err(|e| e.to_string())?;
        std::fs::File::open(&temporary)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())?;
        std::fs::rename(&temporary, file).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary);
    }
    result
}
impl Store {
    pub fn new() -> Self {
        Self::at(root())
    }
    fn at(root: PathBuf) -> Self {
        let (tx, rx) = mpsc::sync_channel(1);
        let error = Arc::new(Mutex::new(None));
        let worker_error = error.clone();
        std::thread::Builder::new()
            .name("pr0-loop-files".into())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    match job {
                        Job::Save(project, snapshot) => {
                            if let Err(e) = write(&root, &project, snapshot) {
                                *worker_error.lock().unwrap() = Some(format!("Loop storage: {e}"));
                            }
                        }
                        Job::Barrier(reply) => {
                            let _ = reply.send(());
                        }
                    }
                }
            })
            .expect("Start loop file worker");
        Self {
            tx,
            pending: None,
            error,
        }
    }
    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }
    pub fn poll(&mut self, project: &str, engine: &mut Engine) {
        let job = self.pending.take().or_else(|| {
            engine
                .take_loop_snapshot()
                .map(|s| Job::Save(project.into(), s))
        });
        if let Some(job) = job {
            match self.tx.try_send(job) {
                Ok(()) => {}
                Err(TrySendError::Full(job)) => self.pending = Some(job),
                Err(TrySendError::Disconnected(_)) => {
                    *self.error.lock().unwrap() = Some("Loop storage worker unavailable".into())
                }
            }
        }
    }
    // Between blocks only. Flush before acknowledging disable/clear or dropping an engine.
    pub fn flush(&mut self, project: &str, engine: &mut Engine) -> Result<(), String> {
        if let Some(job) = self.pending.take() {
            self.tx.send(job).map_err(|e| e.to_string())?;
        }
        while let Some(snapshot) = engine.take_loop_snapshot() {
            self.tx
                .send(Job::Save(project.into(), snapshot))
                .map_err(|e| e.to_string())?;
        }
        let (tx, rx) = mpsc::channel();
        self.tx.send(Job::Barrier(tx)).map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?;
        self.error().map_or(Ok(()), Err)
    }
}
pub fn clear_saved(project: &str, node: &str, track: u8) -> Result<(), String> {
    write(&root(), project, (node.into(), track, 1, 48000, vec![]))
}
pub fn restore(project: &pr0_core::Project, engine: &mut Engine) -> Result<(), String> {
    restore_at(&root(), project, engine)
}
fn restore_at(root: &Path, project: &pr0_core::Project, engine: &mut Engine) -> Result<(), String> {
    for node in project.graph.nodes.iter().filter(|n| n.kind == "looper") {
        for track in 1..=8 {
            let file = path(root, &project.id, &node.id, track);
            let mut reader = match hound::WavReader::open(file) {
                Ok(r) => r,
                Err(hound::Error::IoError(e)) if e.kind() == std::io::ErrorKind::NotFound => {
                    continue;
                }
                Err(e) => return Err(format!("Saved loop {} track {track}: {e}", node.label)),
            };
            let spec = reader.spec();
            if spec.sample_format != hound::SampleFormat::Float
                || spec.bits_per_sample != 32
                || spec.channels == 0
                || spec.channels > 8
                || reader.len() > 512 * 1024 * 1024 / 4
            {
                return Err("Invalid saved loop WAV".into());
            }
            let audio = reader
                .samples::<f32>()
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            engine.restore_loop(
                &node.id,
                track,
                &audio,
                spec.channels as usize,
                spec.sample_rate,
            )?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn wav_tracks_replace_restore_and_clear_in_worker_order() {
        let root = std::env::temp_dir().join(format!("pr0-loops-{}", uuid::Uuid::new_v4()));
        let mut p =
            pr0_core::demo_project("project".into(), "Loops".into(), pr0_core::Mode::Freeform);
        p.graph.edges.clear();
        p.graph.nodes.truncate(1);
        let node = &mut p.graph.nodes[0];
        node.id = "../../outside/長い名前".into();
        node.kind = "looper".into();
        node.channels = 2;
        node.parameters = [("max_seconds".into(), 1.), ("loop_mode".into(), 0.)].into();
        let id = node.id.clone();
        let mut engine = Engine::prepare(p.graph.clone(), 100.).unwrap();
        let mut store = Store::at(root.clone());
        store
            .tx
            .send(Job::Save(
                p.id.clone(),
                (id.clone(), 1, 2, 100, vec![0.25, -0.25, 0.5, -0.5]),
            ))
            .unwrap();
        store
            .tx
            .send(Job::Save(
                p.id.clone(),
                (id.clone(), 8, 2, 100, vec![0.75, -0.75]),
            ))
            .unwrap();
        store.flush(&p.id, &mut engine).unwrap();
        restore_at(&root, &p, &mut engine).unwrap();
        assert_eq!(engine.telemetry()[&id]["_track_1_seconds"], 0.02);
        assert_eq!(engine.telemetry()[&id]["_track_8_seconds"], 0.01);
        let file = path(&root, &p.id, &id, 1);
        assert!(file.starts_with(&root));
        let pcm = hound::WavReader::open(&file)
            .unwrap()
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(pcm, [0.25, -0.25, 0.5, -0.5]);
        store.pending = Some(Job::Save(
            p.id.clone(),
            (id.clone(), 1, 2, 100, vec![0.125, -0.125]),
        ));
        store.flush(&p.id, &mut engine).unwrap();
        assert_eq!(
            hound::WavReader::open(&file)
                .unwrap()
                .samples::<f32>()
                .collect::<Result<Vec<_>, _>>()
                .unwrap(),
            [0.125, -0.125]
        );
        // A queued replacement must complete before a subsequent clear.
        store.pending = Some(Job::Save(
            p.id.clone(),
            (id.clone(), 1, 2, 100, vec![1., -1.]),
        ));
        engine.clear_loop(&id, 1).unwrap();
        store.flush(&p.id, &mut engine).unwrap();
        assert!(!file.exists());
        assert!(path(&root, &p.id, &id, 8).exists());
        let mut restarted = Engine::prepare(p.graph.clone(), 200.).unwrap();
        restore_at(&root, &p, &mut restarted).unwrap();
        assert_eq!(restarted.telemetry()[&id]["_track_1_seconds"], 0.);
        assert_eq!(restarted.telemetry()[&id]["_track_8_seconds"], 0.01);
        assert!(engine.clear_loop(&id, 0).is_err());
        assert!(engine.clear_loop("missing", 1).is_err());
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn storage_failure_is_reported() {
        let root = std::env::temp_dir().join(format!("pr0-loops-{}", uuid::Uuid::new_v4()));
        std::fs::write(&root, b"not a directory").unwrap();
        let mut store = Store::at(root.clone());
        let p = pr0_core::demo_project("project".into(), "Loops".into(), pr0_core::Mode::Freeform);
        let mut engine = Engine::prepare(p.graph, 100.).unwrap();
        store.pending = Some(Job::Save(
            p.id.clone(),
            ("node".into(), 1, 1, 100, vec![1.]),
        ));
        assert!(
            store
                .flush(&p.id, &mut engine)
                .unwrap_err()
                .contains("Loop storage")
        );
        drop(store);
        std::fs::remove_file(root).unwrap();
    }
}
