use crate::{Api, App, bad, can_edit, csrf, internal, load, role, user};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, header},
    response::{IntoResponse, Response},
};
use rusqlite::{Connection, OptionalExtension, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::PathBuf;

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS sample_library(id TEXT PRIMARY KEY,owner TEXT NOT NULL,origin TEXT NOT NULL,name TEXT NOT NULL,description TEXT NOT NULL DEFAULT '',tags TEXT NOT NULL DEFAULT '',category TEXT NOT NULL DEFAULT '',musical_key TEXT NOT NULL DEFAULT '',bpm REAL,global INTEGER NOT NULL DEFAULT 0,channels INTEGER NOT NULL,sample_rate INTEGER NOT NULL,frames INTEGER NOT NULL,revision INTEGER NOT NULL DEFAULT 0); CREATE TABLE IF NOT EXISTS project_samples(project TEXT NOT NULL,sample TEXT NOT NULL REFERENCES sample_library(id),asset INTEGER NOT NULL,PRIMARY KEY(project,sample),UNIQUE(project,asset)); CREATE INDEX IF NOT EXISTS sample_owner ON sample_library(owner); CREATE INDEX IF NOT EXISTS sample_global ON sample_library(global);")?;
    let exists: bool = db
        .prepare("PRAGMA table_info(sample_library)")?
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|column| column == "root_note");
    if !exists {
        db.execute("ALTER TABLE sample_library ADD COLUMN root_note INTEGER CHECK(root_note BETWEEN 0 AND 127)", [])?;
    }
    Ok(())
}
pub fn path(id: &str) -> PathBuf {
    PathBuf::from(std::env::var("PR0_DATA").unwrap_or("data".into()))
        .join("sample-library")
        .join(format!("{id}.wav"))
}
fn asset(project: &str) -> u32 {
    loop {
        let id = (uuid::Uuid::new_v4().as_u128() % 999999999 + 1) as u32;
        if !crate::samples::directory(project)
            .join(format!("{id}.wav"))
            .exists()
        {
            return id;
        }
    }
}
fn entry(db: &Connection, id: &str, project: &str, u: &str) -> Api<Value> {
    let mut v: Value=db.query_row("SELECT s.id,s.owner,s.origin,s.name,s.description,s.tags,s.category,s.musical_key,s.bpm,s.global,s.channels,s.sample_rate,s.frames,s.revision,u.username,(SELECT asset FROM project_samples WHERE project=?2 AND sample=s.id),s.root_note FROM sample_library s JOIN users u ON u.id=s.owner WHERE s.id=?1",params![id,project],|r|Ok(json!({"id":r.get::<_,String>(0)?,"owner":r.get::<_,String>(1)?,"origin":r.get::<_,String>(2)?,"name":r.get::<_,String>(3)?,"description":r.get::<_,String>(4)?,"tags":r.get::<_,String>(5)?,"category":r.get::<_,String>(6)?,"musical_key":r.get::<_,String>(7)?,"bpm":r.get::<_,Option<f64>>(8)?,"global":r.get::<_,bool>(9)?,"channels":r.get::<_,u32>(10)?,"sample_rate":r.get::<_,u32>(11)?,"frames":r.get::<_,u64>(12)?,"revision":r.get::<_,u64>(13)?,"author":r.get::<_,String>(14)?,"asset":r.get::<_,Option<u32>>(15)?,"root_note":r.get::<_,Option<u8>>(16)?}))).map_err(|_|bad("Sample unavailable"))?;
    let owned = v["owner"] == u;
    let project_edit:bool=db.query_row("SELECT EXISTS(SELECT 1 FROM members WHERE project_id=?1 AND user_id=?2 AND role IN ('owner','conductor','editor'))",params![v["origin"].as_str().unwrap_or(""),u],|r|r.get(0)).map_err(internal)?;
    v["can_edit"] = json!(owned || project_edit);
    v["can_publish"] = json!(owned);
    v["duration"] =
        json!(v["frames"].as_u64().unwrap_or(0) as f64 / v["sample_rate"].as_f64().unwrap_or(1.));
    Ok(v)
}
fn accessible(v: &Value, u: &str) -> bool {
    v["owner"] == u || v["global"] == true || !v["asset"].is_null()
}
// Register immutable audio and associate it with a project. Caller holds setup lock.
pub async fn register(
    app: &App,
    project: &str,
    u: &str,
    name: &str,
    source: &std::path::Path,
    existing: Option<u32>,
) -> Api<Value> {
    let id = crate::uid();
    let local = existing.unwrap_or_else(|| asset(project));
    let reader = hound::WavReader::open(source).map_err(bad)?;
    let spec = reader.spec();
    let frames = reader.duration();
    drop(reader);
    let target = path(&id);
    tokio::fs::create_dir_all(target.parent().unwrap())
        .await
        .map_err(bad)?;
    tokio::fs::copy(source, &target).await.map_err(bad)?;
    let dir = crate::samples::directory(project);
    tokio::fs::create_dir_all(&dir).await.map_err(bad)?;
    let localpath = dir.join(format!("{local}.wav"));
    if existing.is_none() {
        tokio::fs::copy(&target, &localpath).await.map_err(bad)?;
    }
    let mut db = app.db.lock().unwrap();
    let tx = db.transaction().map_err(internal)?;
    tx.execute("INSERT INTO sample_library(id,owner,origin,name,channels,sample_rate,frames) VALUES(?1,?2,?3,?4,?5,?6,?7)",params![id,u,project,name,spec.channels,spec.sample_rate,frames]).map_err(internal)?;
    tx.execute(
        "INSERT INTO project_samples(project,sample,asset) VALUES(?1,?2,?3)",
        params![project, id, local],
    )
    .map_err(internal)?;
    tx.commit().map_err(internal)?;
    entry(&db, &id, project, u)
}
// Discover pre-catalog WAVs, including samples embedded by subgraph insertion.
async fn legacy(app: &App, project: &str, u: &str) -> Api<()> {
    let owner: String = app
        .db
        .lock()
        .unwrap()
        .query_row(
            "SELECT user_id FROM members WHERE project_id=?1 AND role='owner' LIMIT 1",
            [project],
            |r| r.get(0),
        )
        .map_err(internal)?;
    let Ok(mut files) = tokio::fs::read_dir(crate::samples::directory(project)).await else {
        return Ok(());
    };
    while let Some(file) = files.next_entry().await.map_err(bad)? {
        let path = file.path();
        if path.extension().and_then(|x| x.to_str()) != Some("wav") {
            continue;
        }
        let Some(asset) = path
            .file_stem()
            .and_then(|x| x.to_str())
            .and_then(|x| x.parse::<u32>().ok())
        else {
            continue;
        };
        let exists: bool = app
            .db
            .lock()
            .unwrap()
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM project_samples WHERE project=?1 AND asset=?2)",
                params![project, asset],
                |r| r.get(0),
            )
            .map_err(internal)?;
        if !exists {
            register(
                app,
                project,
                &owner,
                &format!("Sample {asset}"),
                &path,
                Some(asset),
            )
            .await?;
        }
    }
    let _ = u;
    Ok(())
}
async fn listing(app: App, headers: HeaderMap, project: String, all: bool) -> Api<Json<Value>> {
    let u = user(&app, &headers)?;
    role(&app, &project, &u)?;
    load(&app, &project)?;
    let _guard = app.setup.lock().await;
    legacy(&app, &project, &u).await?;
    let db = app.db.lock().unwrap();
    let sql = if all {
        "SELECT id FROM sample_library WHERE owner=?1 OR global=1 ORDER BY name COLLATE NOCASE"
    } else {
        "SELECT sample FROM project_samples WHERE project=?1 ORDER BY asset"
    };
    let mut q = db.prepare(sql).map_err(internal)?;
    let ids = q
        .query_map([if all { &u } else { &project }], |r| r.get::<_, String>(0))
        .map_err(internal)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(internal)?;
    let values = ids
        .iter()
        .map(|id| entry(&db, id, &project, &u))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Json(json!(values)))
}
pub async fn list(
    State(app): State<App>,
    headers: HeaderMap,
    Path(project): Path<String>,
) -> Api<Json<Value>> {
    listing(app, headers, project, false).await
}
pub async fn browse(
    State(app): State<App>,
    headers: HeaderMap,
    Path(project): Path<String>,
) -> Api<Json<Value>> {
    listing(app, headers, project, true).await
}
#[derive(Deserialize)]
pub struct Metadata {
    name: String,
    description: String,
    tags: String,
    category: String,
    musical_key: String,
    bpm: Option<f64>,
    root_note: Option<u8>,
    global: bool,
    revision: u64,
}
pub async fn edit(
    State(app): State<App>,
    headers: HeaderMap,
    Path((project, id)): Path<(String, String)>,
    Json(m): Json<Metadata>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    role(&app, &project, &u)?;
    let _guard = app.setup.lock().await;
    if m.name.trim().is_empty()
        || m.name.len() > 256
        || m.description.len() > 4096
        || m.tags.len() > 1024
        || m.category.len() > 128
        || m.root_note.is_some_and(|note| note > 127)
        || m.musical_key.len() > 64
        || m.bpm
            .is_some_and(|v| !v.is_finite() || !(1. ..=400.).contains(&v))
    {
        return Err(bad("Invalid sample metadata"));
    }
    let db = app.db.lock().unwrap();
    let old = entry(&db, &id, &project, &u)?;
    if !accessible(&old, &u) || old["can_edit"] != true {
        return Err(crate::Failure(
            axum::http::StatusCode::FORBIDDEN,
            "Sample metadata is read-only".into(),
        ));
    }
    if old["global"] != m.global && old["can_publish"] != true {
        return Err(crate::Failure(
            axum::http::StatusCode::FORBIDDEN,
            "Only the sample owner can change global sharing".into(),
        ));
    }
    let changed=db.execute("UPDATE sample_library SET name=?1,description=?2,tags=?3,category=?4,musical_key=?5,bpm=?6,global=?7,root_note=?10,revision=revision+1 WHERE id=?8 AND revision=?9",params![m.name.trim(),m.description,m.tags,m.category,m.musical_key,m.bpm,m.global,id,m.revision,m.root_note]).map_err(internal)?;
    if changed == 0 {
        return Err(crate::Failure(
            axum::http::StatusCode::CONFLICT,
            "Sample metadata changed; reopen it before saving".into(),
        ));
    }
    let _ = app
        .events
        .send(json!({"type":"samples","project_id":project}));
    entry(&db, &id, &project, &u).map(Json)
}
pub async fn add(
    State(app): State<App>,
    headers: HeaderMap,
    Path((project, id)): Path<(String, String)>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    can_edit(&role(&app, &project, &u)?)?;
    let _guard = app.setup.lock().await;
    let old = entry(&app.db.lock().unwrap(), &id, &project, &u)?;
    if !accessible(&old, &u) {
        return Err(bad("Sample unavailable"));
    }
    if !old["asset"].is_null() {
        return Ok(Json(old));
    }
    let local = asset(&project);
    let dir = crate::samples::directory(&project);
    tokio::fs::create_dir_all(&dir).await.map_err(bad)?;
    tokio::fs::copy(path(&id), dir.join(format!("{local}.wav")))
        .await
        .map_err(bad)?;
    let p = project.clone();
    let rate = crate::settings::read().sample_rate;
    tokio::task::spawn_blocking(move || crate::samples::cache_asset(&p, local, rate))
        .await
        .map_err(internal)?
        .map_err(bad)?;
    let db = app.db.lock().unwrap();
    db.execute(
        "INSERT INTO project_samples(project,sample,asset) VALUES(?1,?2,?3)",
        params![project, id, local],
    )
    .map_err(internal)?;
    let _ = app
        .events
        .send(json!({"type":"samples","project_id":project}));
    entry(&db, &id, &project, &u).map(Json)
}
pub async fn audio(
    State(app): State<App>,
    headers: HeaderMap,
    Path((project, id)): Path<(String, String)>,
) -> Api<Response> {
    let u = user(&app, &headers)?;
    role(&app, &project, &u)?;
    let v = entry(&app.db.lock().unwrap(), &id, &project, &u)?;
    if !accessible(&v, &u) {
        return Err(bad("Sample unavailable"));
    }
    let bytes = tokio::fs::read(path(&id)).await.map_err(bad)?;
    Ok((
        [
            (header::CONTENT_TYPE, "audio/wav"),
            (header::CACHE_CONTROL, "private, no-store"),
        ],
        bytes,
    )
        .into_response())
}

/// Apply metadata defaults only on new sampler/sample assignments; manual edits remain authoritative.
pub fn assign_roots(
    db: &Connection,
    previous: &pr0_core::Project,
    project: &mut pr0_core::Project,
) -> Api<()> {
    for node in &mut project.graph.nodes {
        if node.kind != "poly_sampler"
            || project
                .graph
                .edges
                .iter()
                .any(|e| e.target == node.id && e.target_port == "root_note")
        {
            continue;
        }
        let Some(asset) = node.parameters.get("asset").copied() else {
            continue;
        };
        if !asset.is_finite() || asset.fract() != 0. || asset <= 0. {
            continue;
        }
        if previous.graph.nodes.iter().any(|old| {
            old.id == node.id
                && old.kind == node.kind
                && old.parameters.get("asset") == Some(&asset)
        }) {
            continue;
        }
        let root: Option<u8> = db.query_row("SELECT s.root_note FROM sample_library s JOIN project_samples p ON p.sample=s.id WHERE p.project=?1 AND p.asset=?2", params![project.id, asset as i64], |r| r.get(0)).optional().map_err(internal)?.flatten();
        if let Some(root) = root {
            node.parameters.insert("root_note".into(), root as f64);
        }
    }
    Ok(())
}

#[cfg(test)]
mod root_tests {
    use super::*;
    #[test]
    fn migration_adds_nullable_root_to_existing_catalog_once() {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE sample_library(id TEXT PRIMARY KEY,owner TEXT,global INTEGER); INSERT INTO sample_library VALUES('legacy','user',0);").unwrap();
        migrate(&db).unwrap();
        migrate(&db).unwrap();
        assert_eq!(
            db.query_row(
                "SELECT root_note FROM sample_library WHERE id='legacy'",
                [],
                |r| r.get::<_, Option<u8>>(0)
            )
            .unwrap(),
            None
        );
        assert!(
            db.execute("UPDATE sample_library SET root_note=128", [])
                .is_err()
        );
    }
    #[test]
    fn assignment_uses_project_metadata_and_preserves_manual_and_connected_roots() {
        let db = Connection::open_in_memory().unwrap();
        migrate(&db).unwrap();
        db.execute_batch("INSERT INTO sample_library(id,owner,origin,name,channels,sample_rate,frames,root_note) VALUES('a','u','p','A',2,48000,100,69),('b','u','p','B',2,48000,100,NULL),('c','u','q','C',2,48000,100,0); INSERT INTO project_samples VALUES('p','a',10),('p','b',11),('q','c',10);").unwrap();
        let mut project =
            pr0_core::demo_project("p".into(), "Roots".into(), pr0_core::Mode::Freeform);
        project.graph.edges.clear();
        project.graph.nodes.truncate(1);
        let node = &mut project.graph.nodes[0];
        node.kind = "poly_sampler".into();
        node.id = "sampler".into();
        node.parameters = [("asset".into(), 10.), ("root_note".into(), 60.)].into();
        let mut previous = project.clone();
        previous.graph.nodes.clear();
        assert!(assign_roots(&db, &previous, &mut project).is_ok());
        assert_eq!(project.graph.nodes[0].parameters["root_note"], 69.);
        let previous_assigned = project.clone();
        project.graph.nodes[0]
            .parameters
            .insert("root_note".into(), 72.);
        assert!(assign_roots(&db, &previous_assigned, &mut project).is_ok());
        assert_eq!(project.graph.nodes[0].parameters["root_note"], 72.);
        project.graph.nodes[0]
            .parameters
            .insert("asset".into(), 11.);
        assert!(assign_roots(&db, &previous_assigned, &mut project).is_ok());
        assert_eq!(project.graph.nodes[0].parameters["root_note"], 72.);
        project.id = "q".into();
        project.graph.nodes[0]
            .parameters
            .insert("asset".into(), 10.);
        assert!(assign_roots(&db, &previous, &mut project).is_ok());
        assert_eq!(project.graph.nodes[0].parameters["root_note"], 0.);
        project.graph.nodes[0]
            .parameters
            .insert("root_note".into(), 45.);
        project.graph.edges.push(pr0_core::Edge {
            id: "drive".into(),
            source: "source".into(),
            source_port: "out".into(),
            target: "sampler".into(),
            target_port: "root_note".into(),
        });
        assert!(assign_roots(&db, &previous, &mut project).is_ok());
        assert_eq!(project.graph.nodes[0].parameters["root_note"], 45.);
    }
}
