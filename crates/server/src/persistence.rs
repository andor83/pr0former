//! Ordered off-audio ownership of file barriers and retired engines.
use pr0_dsp::Engine;
use std::{
    collections::BTreeMap,
    sync::mpsc::{self, Receiver, SyncSender, TrySendError},
};
use tokio::sync::oneshot;
type LoopChunk = (String, u8, usize, u32, usize, usize, Vec<f32>);
enum Job {
    #[cfg(test)]
    Hold(Receiver<()>, SyncSender<()>),
    Reset,
    Data(String, Vec<crate::recordings::Chunk>, Option<LoopChunk>),
    Retire(String, Box<Engine>),
    Discard(Box<Engine>),
    Barrier(oneshot::Sender<Result<(), String>>),
}
pub struct Persistence {
    tx: SyncSender<Job>,
    pending: Option<Job>,
    errors: Receiver<Option<String>>,
    reset_requested: bool,
    barrier: Option<oneshot::Sender<Result<(), String>>>,
    error: Option<String>,
}
impl Persistence {
    /// Storage roots come from the host's [`crate::config::RuntimeConfig`], so
    /// two runtimes in one process never share loop or recording files.
    pub fn new(loops_root: std::path::PathBuf, recordings_root: std::path::PathBuf) -> Self {
        let (tx, rx) = mpsc::sync_channel(8);
        let (errors_tx, errors) = mpsc::channel();
        std::thread::Builder::new()
            .name("pr0-persistence".into())
            .spawn(move || {
                let loops = crate::loops::Store::new(loops_root);
                let mut recordings = crate::recordings::Store::new(recordings_root);
                let mut partial: BTreeMap<(String, String, u8), Vec<f32>> = BTreeMap::new();
                let mut last_error = None;
                while let Ok(job) = rx.recv() {
                    let result = match job {
                        Job::Reset => {
                            recordings.reset_error();
                            loops.reset_error();
                            Ok(())
                        }
                        #[cfg(test)]
                        Job::Hold(release, entered) => {
                            entered.send(()).unwrap();
                            release.recv().unwrap();
                            Ok(())
                        }
                        Job::Data(project, records, chunk) => {
                            let mut result = Ok(());
                            if !records.is_empty() {
                                result = recordings.data(project.clone(), records);
                            }
                            if let Some((node, track, channels, rate, offset, total, audio)) = chunk
                            {
                                let key = (project.clone(), node.clone(), track);
                                let buffer = partial.entry(key.clone()).or_default();
                                if offset == 0 {
                                    buffer.clear();
                                }
                                if buffer.len() == offset {
                                    buffer.extend(audio);
                                    if buffer.len() == total {
                                        let data = partial.remove(&key).unwrap();
                                        result = result.and(
                                            loops
                                                .save(project, (node, track, channels, rate, data)),
                                        );
                                    }
                                } else {
                                    result = Err("Loop snapshot sequence interrupted".into());
                                }
                            }
                            result
                        }
                        Job::Discard(engine) => {drop(engine); Ok(())}
                        Job::Retire(project, mut engine) => {
                            engine.finish_loops();
                            engine.finish_recordings();
                            let mut result = recordings.flush(&project, &mut engine);
                            while let Some(snapshot) = engine.take_loop_snapshot() {
                                partial.remove(&(project.clone(), snapshot.0.clone(), snapshot.1));
                                result = result.and(loops.save(project.clone(), snapshot));
                            }
                            result = result.and(loops.barrier());
                            // Destruction of all retired DSP storage happens on this thread.
                            drop(engine);
                            result
                        }
                        Job::Barrier(reply) => {
                            let result = recordings.barrier().and(loops.barrier());
                            let _ = reply.send(result.clone());
                            result
                        }
                    };
                    let error = result
                        .err()
                        .or_else(|| recordings.error())
                        .or_else(|| loops.error());
                    if error != last_error {
                        let _ = errors_tx.send(error.clone());
                        last_error = error;
                    }
                }
                let _ = recordings.barrier();
                let _ = loops.barrier();
            })
            .expect("Start persistence worker");
        Self {
            tx,
            pending: None,
            barrier: None,
            reset_requested: false,
            errors,
            error: None,
        }
    }
    /// Apply command backpressure, never rendering backpressure.
    pub fn ready(&mut self) -> bool {
        while let Ok(error) = self.errors.try_recv() {
            self.error = error;
        }
        if let Some(job) = self.pending.take() {
            self.send(job);
        }
        if self.pending.is_none() && self.reset_requested {
            self.reset_requested = false;
            self.send(Job::Reset);
        }
        if self.pending.is_none() {
            if let Some(reply) = self.barrier.take() {
                self.send(Job::Barrier(reply));
            }
        }
        self.pending.is_none()
    }
    fn send(&mut self, job: Job) {
        assert!(self.pending.is_none());
        match self.tx.try_send(job) {
            Ok(()) => {}
            Err(TrySendError::Full(job)) => self.pending = Some(job),
            Err(TrySendError::Disconnected(job)) => {
                self.error = Some("Persistence worker unavailable; restart the server".into());
                self.pending = Some(job);
            }
        }
    }
    pub fn poll(&mut self, project: &str, engine: &mut Engine) {
        if !self.ready() {
            return;
        }
        if self.error.is_some() {
            engine.finish_recordings();
        }
        let records = engine.take_record_events();
        let chunk = engine.take_loop_chunk(4096);
        if !records.is_empty() || chunk.is_some() {
            self.send(Job::Data(project.into(), records, chunk));
        }
    }
    /// Uninstalled preparation has no runtime recordings to flush.
    pub fn discard(&mut self, engine: Box<Engine>) { self.send(Job::Discard(engine)); }
    pub fn retire(&mut self, project: String, engine: Engine) {
        self.send(Job::Retire(project, Box::new(engine)));
    }
    /// Call only after ready(); acknowledgment includes all preceding writes/retirements.
    pub fn barrier(&mut self, reply: oneshot::Sender<Result<(), String>>) {
        if self.pending.is_some() {
            self.barrier = Some(reply);
        } else {
            self.send(Job::Barrier(reply));
        }
    }
    pub fn clear(&mut self, project: String, node: String, track: u8) {
        self.send(Job::Data(
            project,
            vec![],
            Some((node, track, 1, 48000, 0, 0, vec![])),
        ));
    }
    pub fn reset(&mut self) {
        self.error = None;
        self.reset_requested = true;
    }
    pub fn error(&self) -> Option<String> {
        self.error.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn slow_storage_backpressures_edits_without_waiting_on_the_render_worker() {
        let directory = std::env::temp_dir().join(format!("pr0-persistence-{}", uuid::Uuid::new_v4()));
        let mut store = Persistence::new(directory.join("loops"), directory.join("recordings"));
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let (entered_tx, entered_rx) = mpsc::sync_channel(1);
        store.tx.send(Job::Hold(release_rx, entered_tx)).unwrap();
        entered_rx.recv().unwrap();
        let graph = pr0_core::Graph {
            nodes: vec![],
            edges: vec![],
        };
        for _ in 0..9 {
            store.retire(
                "test".into(),
                Engine::prepare(graph.clone(), 48000.).unwrap(),
            );
        }
        assert!(!store.ready());
        let mut live = Engine::prepare(graph, 48000.).unwrap();
        let initial = live.clock.sample;
        for _ in 0..100 {
            assert!(!store.ready());
            live.render(&[], &mut [[0.; 8]; 128]);
            store.poll("test", &mut live);
        }
        assert_eq!(live.clock.sample - initial, 12800);
        let (ack, mut done) = oneshot::channel();
        store.barrier(ack);
        assert!(matches!(
            done.try_recv(),
            Err(oneshot::error::TryRecvError::Empty)
        ));
        release_tx.send(()).unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !store.ready() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        done.blocking_recv().unwrap().unwrap();
    }
}
