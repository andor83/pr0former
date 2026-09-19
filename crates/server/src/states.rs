//! State preparation is serialized off the audio worker; the watch mailbox keeps only the latest pending recall.
use crate::*;
use pr0_core::ControlValue;
use std::sync::atomic::{AtomicU64, Ordering};
#[derive(Clone)]
pub(crate) struct Recalls {
    pub generation: Arc<AtomicU64>,
    diagnostics: Arc<Mutex<std::collections::VecDeque<Value>>>,
    sender: tokio::sync::watch::Sender<Option<Arc<Request>>>,
}
struct Request {
    source_generation: Option<u64>,
    project: String,
    node: String,
    selector: ControlValue,
    generation: u64,
    reply: Mutex<Option<oneshot::Sender<Result<Value, String>>>>,
}
pub(crate) struct Install {
    pub runtime_generation: u64,
    pub generation: u64,
    pub current: Arc<AtomicU64>,
    pub node: String,
    pub restored: Vec<String>,
    pub skipped: Vec<String>,
    pub reply: oneshot::Sender<Result<Value, String>>,
}
impl Recalls {
    pub fn new() -> Self {
        let (sender, _) = tokio::sync::watch::channel(None);
        Self {
            sender,
            generation: Arc::new(AtomicU64::new(0)),
            diagnostics: Default::default(),
        }
    }
    fn submit(
        &self,
        project: String,
        node: String,
        selector: ControlValue,
    ) -> oneshot::Receiver<Result<Value, String>> {
        self.submit_at(project, node, selector, None)
    }
    fn submit_at(
        &self,
        project: String,
        node: String,
        selector: ControlValue,
        source_generation: Option<u64>,
    ) -> oneshot::Receiver<Result<Value, String>> {
        let (tx, rx) = oneshot::channel();
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let request = Arc::new(Request {
            source_generation,
            project,
            node,
            selector,
            generation,
            reply: Mutex::new(Some(tx)),
        });
        self.sender.send_if_modified(|pending| {
            if pending.as_ref().is_some_and(|r| r.generation > generation) {
                false
            } else {
                *pending = Some(request.clone());
                true
            }
        });
        rx
    }
}
pub(crate) fn start(app: App) {
    let mut requests = app.recalls.sender.subscribe();
    let receiver_app = app.clone();
    let mut events = app.events.subscribe();
    tokio::spawn(async move {
        loop {
            let event = match events.recv().await {
                Ok(event) => event,
                Err(broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            };
            let v = &event.value;
            if v["type"] == "state_request" {
                if let (Some(project), Some(node), Ok(selector)) = (
                    v["project_id"].as_str(),
                    v["node"].as_str(),
                    serde_json::from_value(v["selector"].clone()),
                ) {
                    receiver_app.recalls.submit_at(
                        project.into(),
                        node.into(),
                        selector,
                        v["runtime_generation"].as_u64(),
                    );
                }
            }
        }
    });
    tokio::spawn(async move {
        while requests.changed().await.is_ok() {
            let Some(request) = requests.borrow_and_update().clone() else {
                continue;
            };
            let result = prepare(&app, &request).await.map_err(|e| e.1);
            if let Err(error) = &result {
                if app.recalls.generation.load(Ordering::SeqCst) == request.generation {
                    let diagnostic = json!({"type":"state_status","project_id":request.project,"node":request.node,"status":"failed","error":error});
                    let mut history = app.recalls.diagnostics.lock().unwrap();
                    if history.len() == 32 {
                        history.pop_front();
                    }
                    history.push_back(diagnostic.clone());
                    let _ = app.events.send(diagnostic.into());
                }
            }
            if let Some(reply) = request.reply.lock().unwrap().take() {
                let _ = reply.send(result);
            }
        }
    });
}
pub(crate) async fn snapshot(app: &App, id: &str) -> Api<(Project, u64)> {
    let (reply, rx) = oneshot::channel();
    send(
        app,
        audio::Command::Snapshot {
            project: id.into(),
            reply,
        },
    )?;
    rx.await.map_err(internal)?.map_err(bad)
}
async fn prepare(app: &App, request: &Request) -> Api<Value> {
    let _guard = app.setup.lock().await;
    let current = || app.recalls.generation.load(Ordering::SeqCst) == request.generation;
    if !current() {
        return Err(bad("Recall superseded"));
    }
    if app.graph.lock().unwrap().as_deref() != Some(&request.project) {
        return Err(bad("Enable this project's engine before loading a state"));
    }
    let authored = load(app, &request.project)?;
    let saved = authored
        .graph
        .nodes
        .iter()
        .find(|n| n.id == request.node)
        .and_then(|n| n.states.as_ref())
        .and_then(|b| b.select(&request.selector))
        .ok_or_else(|| bad("Unknown state selector"))?
        .clone();
    let _=app.events.send(json!({"type":"state_status","project_id":request.project,"node":request.node,"status":"preparing"}).into());
    let (mut candidate, generation) = snapshot(app, &request.project).await?;
    if request.source_generation.is_some_and(|g| g != generation) {
        return Err(bad("Recall superseded by a runtime change"));
    }
    if candidate.revision != authored.revision {
        return Err(bad("Runtime revision changed; try again"));
    }
    // Banks are authored metadata; snapshots never restore them.
    for n in &mut candidate.graph.nodes {
        n.states = authored
            .graph
            .nodes
            .iter()
            .find(|v| v.id == n.id)
            .and_then(|v| v.states.clone());
    }
    let (restored, skipped) = candidate
        .graph
        .restore_state(&request.node, &saved)
        .map_err(bad)?;
    candidate.validate().map_err(bad)?;
    scripts::validate_graph(&candidate.graph, None)
        .await
        .map_err(bad)?;
    project_bundle::validate_assets(&app.config, &candidate).map_err(bad)?;
    settings::validate_route_changes(&authored, &candidate, &settings::read(&app.config))
        .map_err(bad)?;
    let config = app.config.clone();
    let p = candidate.clone();
    let engine = tokio::task::spawn_blocking(move || samples::prepare(&config, &p))
        .await
        .map_err(internal)?
        .map_err(bad)?;
    if !current() {
        return Err(bad("Recall superseded"));
    }
    let (reply, rx) = oneshot::channel();
    send(
        app,
        audio::Command::Replace {
            project: candidate,
            engine: Box::new(engine),
            recall: Some(Install {
                runtime_generation: generation,
                generation: request.generation,
                current: app.recalls.generation.clone(),
                node: request.node.clone(),
                restored,
                skipped,
                reply,
            }),
        },
    )?;
    rx.await.map_err(internal)?.map_err(bad)
}
#[derive(Deserialize)]
pub(crate) struct Edit {
    revision: u64,
    action: String,
    slot: Option<u32>,
    name: Option<String>,
}
pub(crate) async fn manage(
    State(app): State<App>,
    headers: HeaderMap,
    Path((id, node)): Path<(String, String)>,
    Json(c): Json<Edit>,
) -> Api<Json<Project>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    can_edit(&role(&app, &id, &u)?)?;
    let guard = app.setup.lock().await;
    if app.performance.lock().unwrap().as_deref() == Some(&id) {
        return Err(bad("Exit performance mode before editing states"));
    }
    let mut p = load(&app, &id)?;
    if p.revision != c.revision {
        return Err(Failure(StatusCode::CONFLICT, "Stale state edit".into()));
    }
    p.graph.descendants(&node).map_err(bad)?;
    let captured = if c.action == "save" {
        let graph = if app.graph.lock().unwrap().as_deref() == Some(&id) {
            snapshot(&app, &id).await?.0.graph
        } else {
            p.graph.clone()
        };
        Some(graph.capture_state(&node).map_err(bad)?)
    } else {
        None
    };
    let n = p.graph.nodes.iter_mut().find(|n| n.id == node).unwrap();
    let bank = n.states.get_or_insert_with(Default::default);
    match c.action.as_str() {
        "save" => {
            bank.save(captured.unwrap()).map_err(bad)?;
        }
        "rename" => {
            let slot = bank
                .slots
                .iter_mut()
                .find(|s| Some(s.slot) == c.slot)
                .ok_or_else(|| bad("State missing"))?;
            slot.name = c.name.unwrap_or_default().trim().into();
        }
        "delete" => {
            if !bank.slots.iter().any(|s| Some(s.slot) == c.slot) {
                return Err(bad("State missing"));
            }
            bank.slots.retain(|s| Some(s.slot) != c.slot);
        }
        _ => return Err(bad("Unknown state action")),
    }
    bank.validate().map_err(bad)?;
    drop(guard);
    update_project(State(app), headers, Path(id), Json(p)).await
}
#[derive(Deserialize)]
pub(crate) struct Recall {
    selector: ControlValue,
}
pub(crate) async fn recall(
    State(app): State<App>,
    headers: HeaderMap,
    Path((id, node)): Path<(String, String)>,
    Json(c): Json<Recall>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    can_edit(&role(&app, &id, &u)?)?;
    if app.graph.lock().unwrap().as_deref() != Some(&id) {
        return Err(bad("Enable this project's engine before loading a state"));
    }
    let p = load(&app, &id)?;
    if p.graph
        .nodes
        .iter()
        .find(|n| n.id == node)
        .and_then(|n| n.states.as_ref())
        .and_then(|b| b.select(&c.selector))
        .is_none()
    {
        return Err(bad("Unknown state selector"));
    }
    Ok(Json(
        app.recalls
            .submit(id, node, c.selector)
            .await
            .map_err(|_| bad("Recall superseded"))?
            .map_err(bad)?,
    ))
}
pub(crate) async fn effective(
    State(app): State<App>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Api<Json<Value>> {
    let u = user(&app, &headers)?;
    role(&app, &id, &u)?;
    if app.graph.lock().unwrap().as_deref() != Some(&id) {
        return Ok(Json(json!({"nodes":[]})));
    }
    let (p, _) = snapshot(&app, &id).await?;
    Ok(Json(
        json!({"nodes":p.graph.nodes.iter().map(pr0_core::states::Options::capture).collect::<Vec<_>>(),"diagnostics":app.recalls.diagnostics.lock().unwrap().iter().filter(|v|v["project_id"]==id).cloned().collect::<Vec<_>>()}),
    ))
}

/// Validate saved-only resources as well as the active configuration, including offline edits.
pub(crate) async fn validate_banks(app: &App, p: &Project) -> Api<()> {
    for node in &p.graph.nodes {
        if let Some(bank) = &node.states {
            for slot in &bank.slots {
                let graph = pr0_core::Graph {
                    nodes: slot.nodes.iter().map(|n| n.node()).collect(),
                    edges: vec![],
                };
                scripts::validate_graph(&graph, None).await.map_err(bad)?;
                for saved in &slot.nodes {
                    for asset in saved.assets() {
                        if !samples::directory(&app.config, &p.id)
                            .join(format!("{asset}.wav"))
                            .is_file()
                        {
                            return Err(bad(format!(
                                "State sample {asset} is not in this project"
                            )));
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn ok<T>(result: Api<T>) -> T {
        result.unwrap_or_else(|e| panic!("{}: {}", e.0, e.1))
    }
    async fn fixture() -> (App, HeaderMap, Project) {
        let path = std::env::temp_dir().join(format!("pr0-states-{}", uid()));
        let mut config = RuntimeConfig::embedded(path);
        config.native_devices = false;
        let assembled = assemble(Arc::new(config)).unwrap();
        let app = assembled.app;
        let mut headers = HeaderMap::new();
        headers.insert("x-pr0former", "1".parse().unwrap());
        headers.insert(
            "cookie",
            format!("pr0_session={}", assembled.session.unwrap())
                .parse()
                .unwrap(),
        );
        let mut p = ok(create_project(
            State(app.clone()),
            headers.clone(),
            Json(Create {
                name: "States".into(),
                mode: Mode::Structured,
            }),
        )
        .await)
        .0;
        p.parts.clear();
        p.graph=serde_json::from_value(json!({"nodes":[
            {"id":"root","kind":"subgraph","label":"Root","channels":1,"x":0,"y":0},
            {"id":"v","parent":"root","kind":"value","label":"Value","channels":1,"x":0,"y":0,"parameters":{"value":5}},
            {"id":"s","parent":"root","kind":"smooth_change","label":"Smooth","channels":1,"x":100,"y":0,"parameters":{"time":500}}
        ],"edges":[{"id":"vs","source":"v","source_port":"out","target":"s","target_port":"in"}]})).unwrap();
        let p = ok(update_project(
            State(app.clone()),
            headers.clone(),
            Path(p.id.clone()),
            Json(p),
        )
        .await)
        .0;
        (app, headers, p)
    }
    async fn edit(
        app: &App,
        headers: &HeaderMap,
        p: &Project,
        action: &str,
        slot: Option<u32>,
        name: Option<&str>,
    ) -> Api<Json<Project>> {
        manage(
            State(app.clone()),
            headers.clone(),
            Path((p.id.clone(), "root".into())),
            Json(Edit {
                revision: p.revision,
                action: action.into(),
                slot,
                name: name.map(str::to_owned),
            }),
        )
        .await
    }
    async fn shutdown(app: &App) {
        let (tx, rx) = oneshot::channel();
        app.engine.send(audio::Command::Shutdown(tx)).unwrap();
        rx.await.unwrap().unwrap();
    }
    #[tokio::test]
    async fn management_permissions_revisions_names_and_monotonic_undo() {
        let (app, h, p) = fixture().await;
        assert!(
            recall(
                State(app.clone()),
                h.clone(),
                Path((p.id.clone(), "root".into())),
                Json(Recall {
                    selector: ControlValue::Number(1.)
                })
            )
            .await
            .is_err()
        );
        let saved = ok(edit(&app, &h, &p, "save", None, None).await).0;
        assert_eq!(
            saved.graph.nodes[0].states.as_ref().unwrap().slots[0]
                .nodes
                .len(),
            2
        );
        assert_eq!(
            edit(&app, &h, &p, "save", None, None)
                .await
                .err()
                .unwrap()
                .0,
            StatusCode::CONFLICT
        );
        assert!(
            edit(&app, &h, &saved, "rename", Some(1), Some(" "))
                .await
                .is_err()
        );
        let renamed = ok(edit(&app, &h, &saved, "rename", Some(1), Some(" First ")).await).0;
        assert_eq!(
            renamed.graph.nodes[0].states.as_ref().unwrap().slots[0].name,
            "First"
        );
        let mut undo = p.clone();
        undo.revision = renamed.revision;
        let undone = ok(update_project(
            State(app.clone()),
            h.clone(),
            Path(p.id.clone()),
            Json(undo),
        )
        .await)
        .0;
        let second = ok(edit(&app, &h, &undone, "save", None, None).await).0;
        assert_eq!(
            second.graph.nodes[0].states.as_ref().unwrap().slots[0].slot,
            2
        );
        let user = ok(user(&app, &h));
        app.db
            .lock()
            .unwrap()
            .execute(
                "UPDATE members SET role='viewer' WHERE project_id=?1 AND user_id=?2",
                params![p.id, user],
            )
            .unwrap();
        assert_eq!(
            edit(&app, &h, &second, "save", None, None)
                .await
                .err()
                .unwrap()
                .0,
            StatusCode::FORBIDDEN
        );
        shutdown(&app).await;
    }
    #[tokio::test]
    async fn recall_is_transient_captures_runtime_and_reconciles_authored_edits() {
        let (app, h, p) = fixture().await;
        let mut p = ok(edit(&app, &h, &p, "save", None, None).await).0;
        p.graph.nodes[1].parameters.insert("value".into(), 30.);
        p.graph.nodes[1].label = "Authored".into();
        p = ok(update_project(State(app.clone()), h.clone(), Path(p.id.clone()), Json(p)).await).0;
        ok(start_graph(&app, &p.id).await);
        *app.performance.lock().unwrap() = Some(p.id.clone());
        let event = ok(recall(
            State(app.clone()),
            h.clone(),
            Path((p.id.clone(), "root".into())),
            Json(Recall {
                selector: ControlValue::Number(1.),
            }),
        )
        .await)
        .0;
        assert_eq!(event["restored"], json!(["v", "s"]));
        assert!(event["event_id"].as_str().is_some());
        let authored = ok(load(&app, &p.id));
        assert_eq!(authored.revision, p.revision);
        assert_eq!(authored.graph.nodes[1].parameters["value"], 30.);
        let runtime = ok(snapshot(&app, &p.id).await).0;
        assert_eq!(runtime.graph.nodes[1].parameters["value"], 5.);
        *app.performance.lock().unwrap() = None;
        let mut field_headers = h.clone();
        field_headers.insert("x-pr0former-fields", r#"{"v":["label"]}"#.parse().unwrap());
        p = ok(update_project(
            State(app.clone()),
            field_headers,
            Path(p.id.clone()),
            Json(p),
        )
        .await)
        .0;
        let effective = ok(snapshot(&app, &p.id).await).0;
        assert_eq!(effective.graph.nodes[1].label, "Authored");
        assert_eq!(effective.graph.nodes[1].parameters["value"], 5.);

        p = ok(edit(&app, &h, &p, "save", None, None).await).0;
        assert_eq!(
            p.graph.nodes[0].states.as_ref().unwrap().slots[1].nodes[0].parameters["value"],
            5.
        );
        p.graph.nodes[1].label = "Renamed".into();
        p = ok(update_project(State(app.clone()), h.clone(), Path(p.id.clone()), Json(p)).await).0;
        assert_eq!(
            ok(snapshot(&app, &p.id).await).0.graph.nodes[1].parameters["value"],
            5.
        );
        let saved = ok(parameter(
            State(app.clone()),
            h.clone(),
            Path(p.id.clone()),
            Json(ParameterEdit {
                node: "v".into(),
                parameter: "value".into(),
                value: 40.,
                revision: p.revision,
            }),
        )
        .await)
        .0;
        assert_eq!(
            ok(snapshot(&app, &p.id).await).0.graph.nodes[1].parameters["value"],
            40.
        );
        send(&app, audio::Command::Unload).ok();
        *app.graph.lock().unwrap() = None;
        ok(start_graph(&app, &p.id).await);
        assert_eq!(
            ok(snapshot(&app, &p.id).await).0.graph.nodes[1].parameters["value"],
            saved.graph.nodes[1].parameters["value"]
        );
        shutdown(&app).await;
    }
    #[tokio::test]
    async fn latest_pending_request_wins_and_stale_install_cannot_change_runtime() {
        let (app, h, p) = fixture().await;
        let p = ok(edit(&app, &h, &p, "save", None, None).await).0;
        ok(start_graph(&app, &p.id).await);
        let guard = app.setup.lock().await;
        let first = app
            .recalls
            .submit(p.id.clone(), "root".into(), ControlValue::Number(1.));
        tokio::task::yield_now().await;
        let second = app
            .recalls
            .submit(p.id.clone(), "root".into(), ControlValue::Number(1.));
        drop(guard);
        assert!(!first.await.is_ok_and(|v| v.is_ok()));
        assert!(second.await.unwrap().is_ok());
        let (mut candidate, generation) = ok(snapshot(&app, &p.id).await);
        candidate.graph.nodes[1]
            .parameters
            .insert("value".into(), 90.);
        let engine = samples::prepare(&app.config, &candidate).unwrap();
        let (tx, rx) = oneshot::channel();
        ok(send(
            &app,
            audio::Command::Replace {
                project: candidate,
                engine: Box::new(engine),
                recall: Some(Install {
                    runtime_generation: generation + 1,
                    generation: app.recalls.generation.load(Ordering::SeqCst),
                    current: app.recalls.generation.clone(),
                    node: "root".into(),
                    restored: vec!["v".into()],
                    skipped: vec![],
                    reply: tx,
                }),
            },
        ));
        assert!(rx.await.unwrap().is_err());
        assert_eq!(
            ok(snapshot(&app, &p.id).await).0.graph.nodes[1].parameters["value"],
            5.
        );
        shutdown(&app).await;
    }
    #[tokio::test]
    async fn malformed_and_foreign_asset_snapshots_are_rejected_offline() {
        let (app, h, p) = fixture().await;
        let mut p = ok(edit(&app, &h, &p, "save", None, None).await).0;
        p.graph.nodes[0].states.as_mut().unwrap().slots[0].nodes[0]
            .parameters
            .insert("bogus".into(), 1.);
        assert!(
            update_project(
                State(app.clone()),
                h.clone(),
                Path(p.id.clone()),
                Json(p.clone())
            )
            .await
            .is_err()
        );
        let saved = &mut p.graph.nodes[0].states.as_mut().unwrap().slots[0].nodes[0];
        saved.kind = "sample".into();
        saved.parameters = std::collections::BTreeMap::from([("asset".into(), 999999999.)]);
        assert!(
            update_project(State(app.clone()), h, Path(p.id.clone()), Json(p))
                .await
                .is_err()
        );
        shutdown(&app).await;
    }
    #[tokio::test]
    async fn library_insertion_remaps_saved_only_assets_and_descendants() {
        let (app, h, p) = fixture().await;
        let mut p = ok(edit(&app, &h, &p, "save", None, None).await).0;
        let directory = samples::directory(&app.config, &p.id);
        std::fs::create_dir_all(&directory).unwrap();
        let mut wav = hound::WavWriter::create(
            directory.join("7.wav"),
            hound::WavSpec {
                channels: 1,
                sample_rate: 48000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..128 {
            wav.write_sample(0_i16).unwrap();
        }
        wav.finalize().unwrap();
        let saved = &mut p.graph.nodes[0].states.as_mut().unwrap().slots[0].nodes[0];
        saved.kind = "sample".into();
        saved.parameters = std::collections::BTreeMap::from([("asset".into(), 7.)]);
        p = ok(update_project(State(app.clone()), h.clone(), Path(p.id.clone()), Json(p)).await).0;
        let library=ok(crate::subgraphs::save(State(app.clone()),h.clone(),Path(p.id.clone()),Json(serde_json::from_value(json!({"node":"root","revision":p.revision,"name":"Saved sample","public":false})).unwrap())).await).0;
        let destination = ok(create_project(
            State(app.clone()),
            h.clone(),
            Json(Create {
                name: "Destination".into(),
                mode: Mode::Freeform,
            }),
        )
        .await)
        .0;
        let inserted=ok(crate::subgraphs::insert(State(app.clone()),h,Path(destination.id.clone()),Json(serde_json::from_value(json!({"library_id":library["id"],"version":library["version"],"revision":destination.revision,"parent":null,"x":0,"y":0})).unwrap())).await).0;
        let root = inserted
            .graph
            .nodes
            .iter()
            .find(|n| n.states.is_some())
            .unwrap();
        let saved = &root.states.as_ref().unwrap().slots[0].nodes[0];
        assert_ne!(saved.id, "v");
        assert!(inserted.graph.nodes.iter().any(|n| n.id == saved.id));
        let asset = saved.parameters["asset"] as u32;
        assert_ne!(asset, 7);
        assert!(
            samples::directory(&app.config, &destination.id)
                .join(format!("{asset}.wav"))
                .is_file()
        );
        assert!(project_bundle::validate_assets(&app.config, &inserted).is_ok());
        shutdown(&app).await;
    }
}
