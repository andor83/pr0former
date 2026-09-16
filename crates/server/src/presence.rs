//! Project event sockets define live presence; ordinary HTTP requests are not leases.
use crate::*;
use std::collections::{BTreeMap, HashMap, HashSet};

const RECONNECT_GRACE: Duration = Duration::from_secs(5);
#[derive(Default)]
pub struct Presence {
    projects: BTreeMap<String, Entry>,
}
struct Entry {
    /// socket id → user id; several tabs of one user are several sockets.
    sockets: HashMap<String, String>,
    generation: String,
}
impl Presence {
    fn join(&mut self, project: &str, socket: &str, user: &str) {
        let entry = self
            .projects
            .entry(project.into())
            .or_insert_with(|| Entry {
                sockets: HashMap::new(),
                generation: uid(),
            });
        entry.sockets.insert(socket.into(), user.into());
        entry.generation = uid();
    }
    fn leave(&mut self, project: &str, socket: &str) -> Option<String> {
        let entry = self.projects.get_mut(project)?;
        if entry.sockets.remove(socket).is_none() || !entry.sockets.is_empty() {
            return None;
        }
        entry.generation = uid();
        Some(entry.generation.clone())
    }
    /// Distinct users holding an event socket on the project.
    pub fn users(&self, project: &str) -> usize {
        self.projects.get(project).map_or(0, |e| {
            e.sockets.values().collect::<HashSet<_>>().len()
        })
    }
    fn vacant(&self, project: &str, generation: &str) -> bool {
        self.projects
            .get(project)
            .is_some_and(|e| e.sockets.is_empty() && e.generation == generation)
    }
}

pub struct Lease {
    app: App,
    project: String,
    socket: String,
}
pub fn event(project: &str, users: usize) -> Value {
    json!({"type":"presence","project_id":project,"users":users})
}
impl Lease {
    pub async fn join(app: &App, project: &str, user: &str) -> Self {
        // Serialized with the final vacancy check and graph ownership changes.
        let _setup = app.setup.lock().await;
        let socket = uid();
        let users = {
            let mut presence = app.presence.lock().unwrap();
            presence.join(project, &socket, user);
            presence.users(project)
        };
        let _ = app.events.send(event(project, users).into());
        Self {
            app: app.clone(),
            project: project.into(),
            socket,
        }
    }
}
impl Drop for Lease {
    fn drop(&mut self) {
        let (app, project, socket) = (self.app.clone(), self.project.clone(), self.socket.clone());
        tokio::spawn(async move {
            let (generation, users) = {
                let _setup = app.setup.lock().await;
                let mut presence = app.presence.lock().unwrap();
                let generation = presence.leave(&project, &socket);
                (generation, presence.users(&project))
            };
            let _ = app.events.send(event(&project, users).into());
            let Some(generation) = generation else { return };
            tokio::time::sleep(RECONNECT_GRACE).await;
            let _setup = app.setup.lock().await;
            if !app.presence.lock().unwrap().vacant(&project, &generation) {
                return;
            }
            // A late disconnect from an old project must not stop a new graph.
            if app.graph.lock().unwrap().as_deref() == Some(&project) {
                if let Err(error) = stop(&app, &project).await {
                    app.logs.push(
                        &project,
                        "error",
                        &format!("Last-user shutdown failed: {error}"),
                    );
                }
            }
            app.presence.lock().unwrap().projects.remove(&project);
        });
    }
}
async fn enqueue(app: &App, command: audio::Command) -> Result<(), String> {
    let engine = app.engine.clone();
    // Backpressure waits off the runtime/render threads instead of dropping shutdown.
    tokio::task::spawn_blocking(move || engine.send(command))
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())
}
async fn stop(app: &App, project: &str) -> Result<(), String> {
    enqueue(app, audio::Command::Show(false)).await?; // stops transport/count-in, releases notes
    let (tx, rx) = oneshot::channel();
    enqueue(
        app,
        audio::Command::Enable(project.into(), false, settings::read(&app.config), tx),
    )
    .await?;
    // Enable(false) closes devices and flushes loop/archive writers before replying.
    let flushed = rx.await.map_err(|e| e.to_string())?;
    enqueue(app, audio::Command::Unload).await?;
    *app.active.lock().unwrap() = None;
    *app.graph.lock().unwrap() = None;
    let _ = app.events.send(engine_status(app));
    app.media.close_project(project).await;
    app.logs.push(
        project,
        "info",
        "Last project connection left; performance stopped and audio engine disabled",
    );
    flushed
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_tab_counts_and_reconnect_invalidates_old_cleanup() {
        let mut p = Presence::default();
        p.join("a", "tab1", "alice");
        p.join("a", "tab2", "alice");
        assert_eq!(p.users("a"), 1);
        assert!(p.leave("a", "tab1").is_none());
        let old = p.leave("a", "tab2").unwrap();
        assert!(p.vacant("a", &old));
        p.join("a", "tab3", "bob");
        assert!(!p.vacant("a", &old));
        let current = p.leave("a", "tab3").unwrap();
        assert!(!p.vacant("a", &old));
        assert!(p.vacant("a", &current));
    }
    #[test]
    fn duplicate_disconnects_and_other_projects_do_not_change_presence() {
        let mut p = Presence::default();
        p.join("a", "one", "alice");
        p.join("b", "two", "bob");
        assert_eq!((p.users("a"), p.users("b"), p.users("c")), (1, 1, 0));
        assert!(p.leave("a", "missing").is_none());
        let generation = p.leave("a", "one").unwrap();
        assert!(p.leave("a", "one").is_none());
        assert!(p.vacant("a", &generation));
        assert!(!p.vacant("b", &generation));
    }
    #[test]
    fn users_counts_people_not_tabs() {
        let mut p = Presence::default();
        p.join("a", "s1", "alice");
        p.join("a", "s2", "alice");
        p.join("a", "s3", "bob");
        assert_eq!(p.users("a"), 2);
        assert!(p.leave("a", "s3").is_none());
        assert_eq!(p.users("a"), 1);
        assert!(p.leave("a", "s1").is_none());
        assert!(p.leave("a", "s2").is_some());
        assert_eq!(p.users("a"), 0);
    }
}
