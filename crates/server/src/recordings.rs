//! Private performance archives. No static/download route exposes this directory.
use pr0_dsp::{Engine, recorder::Event};
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::{File, OpenOptions},
    io::BufWriter,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        mpsc::{self, SyncSender},
    },
    time::{SystemTime, UNIX_EPOCH},
};
pub(crate) type Chunk = (String, String, usize, u32, Vec<Event>);
enum Job {
    Data(String, Vec<Chunk>),
    Barrier(mpsc::Sender<()>),
}
struct Take {
    writer: hound::WavWriter<BufWriter<File>>,
    metadata: PathBuf,
    info: serde_json::Value,
    frames: u64,
}
fn private_file(path: &Path) -> Result<File, String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|e| e.to_string())
}
fn metadata(path: &Path, value: &serde_json::Value) -> Result<(), String> {
    use std::io::Write;
    let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    let result = (|| {
        let mut file = private_file(&temp)?;
        serde_json::to_writer(&mut file, value).map_err(|e| e.to_string())?;
        file.flush()
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        std::fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temp);
    }
    result
}
impl Take {
    fn open(
        root: &Path,
        project: &str,
        node: &str,
        name: &str,
        channels: usize,
        rate: u32,
        session: &str,
        segment: u64,
    ) -> Result<Self, String> {
        if !(1..=8).contains(&channels) || rate == 0 {
            return Err("Invalid recording audio format".into());
        }
        // Project identifiers are encoded, never interpreted as filesystem paths.
        let encoded: String = project
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        let mut dir = root.to_path_buf();
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        for chunk in encoded.as_bytes().chunks(64) {
            dir.push(std::str::from_utf8(chunk).unwrap());
        }
        std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))
                .map_err(|e| e.to_string())?;
        }
        let safe: String = name
            .chars()
            .take(80)
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let safe = if safe.is_empty() { "Recording" } else { &safe };
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_millis();
        let base = format!("{safe}-{}-part{segment}_{timestamp}", uuid::Uuid::new_v4());
        let wav = dir.join(format!("{base}.wav"));
        let metadata_path = dir.join(format!("{base}.json"));
        let info = json!({"project_id":project,"node_id":node,"name":name,"timestamp_ms":timestamp as u64,"session":session,"segment":segment,"file":format!("{base}.wav"),"channels":channels,"sample_rate":rate,"bits_per_sample":32,"format":"float","complete":false,"frames":0});
        metadata(&metadata_path, &info)?;
        let writer = hound::WavWriter::new(
            BufWriter::new(private_file(&wav)?),
            hound::WavSpec {
                channels: channels as u16,
                sample_rate: rate,
                bits_per_sample: 32,
                sample_format: hound::SampleFormat::Float,
            },
        )
        .map_err(|e| e.to_string())?;
        Ok(Self {
            writer,
            metadata: metadata_path,
            info,
            frames: 0,
        })
    }
    fn finish(mut self, complete: bool) -> Result<(), String> {
        self.writer.finalize().map_err(|e| e.to_string())?;
        self.info["frames"] = self.frames.into();
        self.info["complete"] = complete.into();
        metadata(&self.metadata, &self.info)
    }
}
pub struct Store {
    tx: SyncSender<Job>,
    error: Arc<Mutex<Option<String>>>,
}
impl Store {
    pub fn new(root: PathBuf) -> Self {
        Self::at(root)
    }
    fn at(root: PathBuf) -> Self {
        let (tx, rx) = mpsc::sync_channel(2);
        let error = Arc::new(Mutex::new(None));
        let worker_error = error.clone();
        std::thread::Builder::new().name("pr0-recordings".into()).spawn(move || {
            let mut takes: BTreeMap<(String, String), Take> = BTreeMap::new();
            while let Ok(job) = rx.recv() {
                match job {
                    Job::Barrier(reply) => { let _ = reply.send(()); },
                    Job::Data(project, chunks) => for (node, name, channels, rate, events) in chunks {
                        let key = (project.clone(), node.clone());
                        let result: Result<(), String> = (|| {
                            for event in events {
                                match event {
                                    Event::Start => {
                                        if let Some(old) = takes.remove(&key) { old.finish(false)?; }
                                        takes.insert(key.clone(), Take::open(&root, &project, &node, &name, channels, rate, &uuid::Uuid::new_v4().to_string(), 1)?);
                                    },
                                    Event::Audio(frame) => {
                                        // Split before RIFF's 4 GiB limit; all segments retain the session ID.
                                        if takes.get(&key).is_some_and(|t| t.frames * channels as u64 * 4 >= if cfg!(test) { 128 } else { 1024 * 1024 * 1024 }) {
                                            let old = takes.remove(&key).unwrap();
                                            let session = old.info["session"].as_str().unwrap().to_owned();
                                            let segment = old.info["segment"].as_u64().unwrap() + 1;
                                            old.finish(true)?;
                                            takes.insert(key.clone(), Take::open(&root, &project, &node, &name, channels, rate, &session, segment)?);
                                        }
                                        if let Some(take) = takes.get_mut(&key) {
                                            for sample in &frame[..channels] { take.writer.write_sample(*sample).map_err(|e| e.to_string())?; }
                                            take.frames += 1;
                                            // Maintain a recoverable WAV header during long performances.
                                            if take.frames % rate as u64 == 0 { take.writer.flush().map_err(|e| e.to_string())?; }
                                        }
                                    },
                                    Event::Stop => { if let Some(take) = takes.remove(&key) { take.finish(true)?; } },
                                    Event::Overflow => {
                                        if let Some(take) = takes.remove(&key) { take.finish(false)?; }
                                        return Err("Recording buffer overflow; archive stopped. Check disk throughput and restart the engine".into());
                                    },
                                }
                            }
                            Ok(())
                        })();
                        if let Err(e) = result {
                            if let Some(take) = takes.remove(&key) { let _ = take.finish(false); }
                            *worker_error.lock().unwrap() = Some(format!("Recording storage: {e}"));
                        }
                    },
                }
            }
            for (_, take) in takes { let _ = take.finish(false); }
        }).expect("Start recording file worker");
        Self { tx, error }
    }
    pub(crate) fn data(&self, project: String, chunks: Vec<Chunk>) -> Result<(), String> {
        self.tx
            .send(Job::Data(project, chunks))
            .map_err(|e| e.to_string())
    }
    pub(crate) fn barrier(&self) -> Result<(), String> {
        let (tx, rx) = mpsc::channel();
        self.tx.send(Job::Barrier(tx)).map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?;
        self.error().map_or(Ok(()), Err)
    }
    pub fn reset_error(&self) {
        *self.error.lock().unwrap() = None;
    }
    pub fn error(&self) -> Option<String> {
        self.error.lock().unwrap().clone()
    }
    pub fn flush(&mut self, project: &str, engine: &mut Engine) -> Result<(), String> {
        let chunks = engine.take_record_events();
        if !chunks.is_empty() {
            self.tx
                .send(Job::Data(project.into(), chunks))
                .map_err(|e| e.to_string())?;
        }
        let (tx, rx) = mpsc::channel();
        self.tx.send(Job::Barrier(tx)).map_err(|e| e.to_string())?;
        rx.recv().map_err(|e| e.to_string())?;
        self.error().map_or(Ok(()), Err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn files(root: &Path, extension: &str) -> Vec<PathBuf> {
        let mut result = Vec::new();
        for item in std::fs::read_dir(root).unwrap() {
            let path = item.unwrap().path();
            if path.is_dir() {
                result.extend(files(&path, extension));
            } else if path.extension().is_some_and(|e| e == extension) {
                result.push(path);
            }
        }
        result
    }
    #[test]
    fn archives_preserve_channels_names_ownership_and_separate_takes() {
        let root = std::env::temp_dir().join(format!("pr0-record-{}", uuid::Uuid::new_v4()));
        let mut store = Store::at(root.clone());
        let mut engine = Engine::prepare(pr0_core::Graph::default(), 96000.).unwrap();
        let frame = [0.125, -0.25, 0.375, -0.5, 0.625, -0.75, 0.875, -1.];
        for project in ["project-a", "project-a", "project-b"] {
            store
                .tx
                .send(Job::Data(
                    project.into(),
                    vec![(
                        "node".into(),
                        "../../Night set".into(),
                        8,
                        96000,
                        vec![
                            Event::Start,
                            Event::Audio(frame),
                            Event::Audio(frame),
                            Event::Stop,
                        ],
                    )],
                ))
                .unwrap();
        }
        store.flush("project-b", &mut engine).unwrap();
        let wavs = files(&root, "wav");
        assert_eq!(wavs.len(), 3);
        for wav in &wavs {
            assert!(
                wav.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("______Night_set-")
            );
            let mut reader = hound::WavReader::open(wav).unwrap();
            assert_eq!(reader.spec().channels, 8);
            assert_eq!(reader.spec().sample_rate, 96000);
            assert_eq!(reader.spec().bits_per_sample, 32);
            assert_eq!(
                reader
                    .samples::<f32>()
                    .collect::<Result<Vec<_>, _>>()
                    .unwrap(),
                frame.repeat(2)
            );
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                assert_eq!(
                    std::fs::metadata(wav).unwrap().permissions().mode() & 0o777,
                    0o600
                );
            }
        }
        let infos: Vec<serde_json::Value> = files(&root, "json")
            .iter()
            .map(|p| serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap())
            .collect();
        assert_eq!(
            infos
                .iter()
                .filter(|v| v["project_id"] == "project-a")
                .count(),
            2
        );
        assert!(infos.iter().all(|v| v["complete"] == true
            && v["frames"] == 2
            && v["timestamp_ms"].as_u64().unwrap() > 0));
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn long_takes_split_without_losing_frames_and_overflow_is_incomplete() {
        let root = std::env::temp_dir().join(format!("pr0-record-{}", uuid::Uuid::new_v4()));
        let mut store = Store::at(root.clone());
        let mut engine = Engine::prepare(pr0_core::Graph::default(), 48000.).unwrap();
        let mut events = vec![Event::Start];
        events.extend((0..10).map(|i| Event::Audio([i as f32; 8])));
        events.push(Event::Overflow);
        store
            .tx
            .send(Job::Data(
                "p".into(),
                vec![("n".into(), "Archive".into(), 8, 48000, events)],
            ))
            .unwrap();
        assert!(
            store
                .flush("p", &mut engine)
                .unwrap_err()
                .contains("overflow")
        );
        let mut info: Vec<serde_json::Value> = files(&root, "json")
            .iter()
            .map(|p| serde_json::from_slice(&std::fs::read(p).unwrap()).unwrap())
            .collect();
        info.sort_by_key(|v| v["segment"].as_u64().unwrap());
        assert_eq!(info.len(), 3);
        assert_eq!(
            info.iter()
                .map(|v| v["frames"].as_u64().unwrap())
                .sum::<u64>(),
            10
        );
        assert_eq!(info[2]["complete"], false);
        assert!(info.iter().all(|v| v["session"] == info[0]["session"]));
        drop(store);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn unwritable_archive_path_reports_error() {
        let root = std::env::temp_dir().join(format!("pr0-record-{}", uuid::Uuid::new_v4()));
        std::fs::write(&root, b"file").unwrap();
        let mut store = Store::at(root.clone());
        let mut engine = Engine::prepare(pr0_core::Graph::default(), 48000.).unwrap();
        store
            .tx
            .send(Job::Data(
                "p".into(),
                vec![("n".into(), "Archive".into(), 2, 48000, vec![Event::Start])],
            ))
            .unwrap();
        assert!(
            store
                .flush("p", &mut engine)
                .unwrap_err()
                .contains("Recording storage")
        );
        drop(store);
        std::fs::remove_file(root).unwrap();
    }
}
