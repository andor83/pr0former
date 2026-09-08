use crate::{Api, App, Failure, bad, can_edit, csrf, internal, load, now, role, uid, user};
use axum::{
    Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use pr0_core::{Graph, LibraryRef};
use rusqlite::{Connection, params};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch("CREATE TABLE IF NOT EXISTS subgraph_library(id TEXT PRIMARY KEY,owner TEXT NOT NULL REFERENCES users(id),name TEXT NOT NULL,public INTEGER NOT NULL);
    CREATE TABLE IF NOT EXISTS subgraph_versions(library_id TEXT NOT NULL REFERENCES subgraph_library(id),version INTEGER NOT NULL,name TEXT NOT NULL,public INTEGER NOT NULL,body TEXT NOT NULL,created INTEGER NOT NULL,PRIMARY KEY(library_id,version));
    CREATE TABLE IF NOT EXISTS subgraph_assets(library_id TEXT NOT NULL,version INTEGER NOT NULL,asset INTEGER NOT NULL,body BLOB NOT NULL,PRIMARY KEY(library_id,version,asset),FOREIGN KEY(library_id,version) REFERENCES subgraph_versions(library_id,version));")
}
pub async fn list(State(app): State<App>, headers: HeaderMap) -> Api<Json<Value>> {
    let u = user(&app, &headers)?;
    let db = app.db.lock().unwrap();
    let mut query = db.prepare("SELECT l.id,l.name,l.owner,l.public,u.username,v.version,v.name,v.public FROM subgraph_library l JOIN users u ON u.id=l.owner JOIN subgraph_versions v ON v.library_id=l.id WHERE l.owner=?1 OR v.public=1 ORDER BY l.name,v.version DESC").map_err(internal)?;
    let rows = query
        .query_map([&u], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, bool>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, u64>(5)?,
                r.get::<_, String>(6)?,
                r.get::<_, bool>(7)?,
            ))
        })
        .map_err(internal)?;
    let mut entries = BTreeMap::<String, Value>::new();
    for row in rows {
        let (id, name, owner, public, username, version, version_name, published) =
            row.map_err(internal)?;
        let entry = entries.entry(id.clone()).or_insert_with(|| json!({"id":id,"name":name,"owned":owner==u,"public":public,"owner":username,"versions":[]}));
        entry["versions"]
            .as_array_mut()
            .unwrap()
            .push(json!({"version":version,"name":version_name,"public":published}));
    }
    Ok(Json(json!(entries.into_values().collect::<Vec<_>>())))
}
fn read_version(db: &Connection, id: &str, version: u64, u: &str) -> Api<Graph> {
    let body: String = db.query_row("SELECT v.body FROM subgraph_versions v JOIN subgraph_library l ON l.id=v.library_id WHERE l.id=?1 AND v.version=?2 AND (l.owner=?3 OR v.public=1)", params![id,version,u], |r| r.get(0))
        .map_err(|_| Failure(StatusCode::NOT_FOUND,"Subgraph version is unavailable".into()))?;
    serde_json::from_str(&body).map_err(internal)
}
pub async fn get(
    State(app): State<App>,
    headers: HeaderMap,
    Path((id, version)): Path<(String, u64)>,
) -> Api<Json<Graph>> {
    let u = user(&app, &headers)?;
    Ok(Json(read_version(
        &app.db.lock().unwrap(),
        &id,
        version,
        &u,
    )?))
}
#[derive(Deserialize)]
pub struct Save {
    node: String,
    revision: u64,
    name: String,
    public: bool,
    library_id: Option<String>,
}
pub async fn save(
    State(app): State<App>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Json(c): Json<Save>,
) -> Api<Json<Value>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    can_edit(&role(&app, &project, &u)?)?;
    let _guard = app.setup.lock().await;
    let p = load(&app, &project)?;
    if p.revision != c.revision {
        return Err(Failure(
            StatusCode::CONFLICT,
            "Project changed; refresh before saving a subgraph".into(),
        ));
    }
    if c.name.trim().is_empty() || c.name.len() > 256 {
        return Err(bad("Use a library name of 1–256 bytes"));
    }
    if !p
        .graph
        .nodes
        .iter()
        .any(|n| n.id == c.node && n.kind == "subgraph")
    {
        return Err(bad("Select a subgraph"));
    }
    let mut included = BTreeSet::from([c.node.clone()]);
    loop {
        let before = included.len();
        for n in &p.graph.nodes {
            if n.parent.as_ref().is_some_and(|id| included.contains(id)) {
                included.insert(n.id.clone());
            }
        }
        if included.len() == before {
            break;
        }
    }
    let mut graph = Graph {
        nodes: p
            .graph
            .nodes
            .iter()
            .filter(|n| included.contains(&n.id))
            .cloned()
            .collect(),
        edges: p
            .graph
            .edges
            .iter()
            .filter(|e| included.contains(&e.source) && included.contains(&e.target))
            .cloned()
            .collect(),
    };
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == c.node)
        .unwrap()
        .parent = None;
    graph.validate().map_err(bad)?;
    // Copy originals into the immutable version, so another project does not depend on source-project files.
    let assets: BTreeSet<u32> = graph
        .nodes
        .iter()
        .filter(|n| matches!(n.kind.as_str(), "sample" | "phase_vocoder" | "poly_sampler"))
        .filter_map(|n| n.parameters.get("asset"))
        .map(|v| *v as u32)
        .filter(|v| *v != 0)
        .collect();
    let mut bytes = vec![];
    for asset in assets {
        bytes.push((
            asset,
            tokio::fs::read(crate::samples::directory(&project).join(format!("{asset}.wav")))
                .await
                .map_err(|_| {
                    bad(format!(
                        "Sample {asset} is missing; restore it before saving this subgraph"
                    ))
                })?,
        ));
    }
    let mut db = app.db.lock().unwrap();
    let tx = db.transaction().map_err(internal)?;
    let id = c.library_id.clone().unwrap_or_else(uid);
    if c.library_id.is_some() {
        let (owner, public): (String, bool) = tx
            .query_row(
                "SELECT owner,public FROM subgraph_library WHERE id=?1",
                [&id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(|_| bad("Library entry missing"))?;
        if owner != u {
            return Err(Failure(
                StatusCode::FORBIDDEN,
                "Save your own copy to modify another user's library entry".into(),
            ));
        }
        if public && !c.public {
            return Err(bad(
                "A public subgraph cannot become private; save a new private copy",
            ));
        }
        tx.execute(
            "UPDATE subgraph_library SET name=?1,public=?2 WHERE id=?3",
            params![c.name.trim(), c.public, id],
        )
        .map_err(internal)?;
    } else {
        tx.execute(
            "INSERT INTO subgraph_library(id,owner,name,public) VALUES(?1,?2,?3,?4)",
            params![id, u, c.name.trim(), c.public],
        )
        .map_err(internal)?;
    }
    let version: u64 = tx
        .query_row(
            "SELECT COALESCE(MAX(version),0)+1 FROM subgraph_versions WHERE library_id=?1",
            [&id],
            |r| r.get(0),
        )
        .map_err(internal)?;
    graph
        .nodes
        .iter_mut()
        .find(|n| n.id == c.node)
        .unwrap()
        .library = Some(LibraryRef {
        id: id.clone(),
        version,
    });
    tx.execute("INSERT INTO subgraph_versions(library_id,version,name,public,body,created) VALUES(?1,?2,?3,?4,?5,?6)",params![id,version,c.name.trim(),c.public,serde_json::to_string(&graph).map_err(internal)?,now()]).map_err(internal)?;
    for (asset, bytes) in bytes {
        tx.execute(
            "INSERT INTO subgraph_assets(library_id,version,asset,body) VALUES(?1,?2,?3,?4)",
            params![id, version, asset, bytes],
        )
        .map_err(internal)?;
    }
    tx.commit().map_err(internal)?;
    Ok(Json(json!({"id":id,"version":version,"public":c.public})))
}
#[derive(Deserialize)]
pub struct Insert {
    library_id: String,
    version: u64,
    revision: u64,
    parent: Option<String>,
    x: f64,
    y: f64,
}
pub async fn insert(
    State(app): State<App>,
    headers: HeaderMap,
    Path(project): Path<String>,
    Json(c): Json<Insert>,
) -> Api<Json<pr0_core::Project>> {
    csrf(&headers)?;
    let u = user(&app, &headers)?;
    can_edit(&role(&app, &project, &u)?)?;
    let mut p = load(&app, &project)?;
    if p.revision != c.revision {
        return Err(Failure(
            StatusCode::CONFLICT,
            "Project changed; try inserting again".into(),
        ));
    }
    let (mut graph, assets) = {
        let db = app.db.lock().unwrap();
        let graph = read_version(&db, &c.library_id, c.version, &u)?;
        let mut query = db
            .prepare("SELECT asset,body FROM subgraph_assets WHERE library_id=?1 AND version=?2")
            .map_err(internal)?;
        let assets = query
            .query_map(params![c.library_id, c.version], |r| {
                Ok((r.get::<_, u32>(0)?, r.get::<_, Vec<u8>>(1)?))
            })
            .map_err(internal)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(internal)?;
        (graph, assets)
    };
    let root = graph
        .nodes
        .iter()
        .find(|n| n.parent.is_none())
        .ok_or_else(|| bad("Library root missing"))?
        .id
        .clone();
    let ids: BTreeMap<_, _> = graph.nodes.iter().map(|n| (n.id.clone(), uid())).collect();
    let containers: BTreeSet<_> = graph
        .nodes
        .iter()
        .filter(|n| n.kind == "subgraph")
        .map(|n| n.id.clone())
        .collect();
    for n in &mut graph.nodes {
        // Library copies cannot inherit a source part from another performance.
        n.part_id = None;
        if n.id == root {
            n.parent = c.parent.clone();
            n.x = c.x;
            n.y = c.y;
        } else {
            n.parent = n.parent.as_ref().and_then(|id| ids.get(id).cloned());
        }
        n.id = ids[&n.id].clone();
    }
    for e in &mut graph.edges {
        e.id = uid();
        if containers.contains(&e.source) {
            e.source_port = ids
                .get(&e.source_port)
                .cloned()
                .ok_or_else(|| bad("Library output missing"))?;
        }
        if containers.contains(&e.target) {
            e.target_port = ids
                .get(&e.target_port)
                .cloned()
                .ok_or_else(|| bad("Library input missing"))?;
        }
        e.source = ids[&e.source].clone();
        e.target = ids[&e.target].clone();
    }
    // Validate topology, positions and project limits before writing any imported files.
    let mut candidate = p.clone();
    candidate.graph.nodes.extend(graph.nodes.clone());
    candidate.graph.edges.extend(graph.edges.clone());
    candidate.validate().map_err(bad)?;
    let mut written = vec![];
    let result: Api<Json<pr0_core::Project>> = async {
        if !assets.is_empty() {
            tokio::fs::create_dir_all(crate::samples::directory(&project))
                .await
                .map_err(internal)?;
        }
        for (old, bytes) in assets {
            let (asset, mut file, path) = loop {
                let asset = (uuid::Uuid::new_v4().as_u128() % 999999999 + 1) as u32;
                let path = crate::samples::directory(&project).join(format!("{asset}.wav"));
                match tokio::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&path)
                    .await
                {
                    Ok(file) => break (asset, file, path),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(internal(e)),
                }
            };
            written.push(path);
            use tokio::io::AsyncWriteExt;
            file.write_all(&bytes).await.map_err(internal)?;
            for n in &mut graph.nodes {
                if matches!(n.kind.as_str(), "sample" | "phase_vocoder" | "poly_sampler")
                    && n.parameters.get("asset").copied() == Some(old as f64)
                {
                    n.parameters.insert("asset".into(), asset as f64);
                }
            }
        }
        p.graph.nodes.extend(graph.nodes);
        p.graph.edges.extend(graph.edges);
        crate::update_project(State(app.clone()), headers, Path(project.clone()), Json(p)).await
    }
    .await;
    if result.is_err()
        && !load(&app, &project)
            .ok()
            .is_some_and(|p| p.graph.nodes.iter().any(|n| n.id == ids[&root]))
    {
        for path in written {
            let _ = tokio::fs::remove_file(path).await;
        }
    }
    result
}
