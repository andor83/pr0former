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

const BUILTIN_EPIANO: &str = "builtin-electric-piano";
const BUILTIN_DRUMS: &str = "builtin-fm-drum-machine";
const BUILTIN_SAMPLER: &str = "builtin-drum-sampler";
/// Placeholder asset ids used inside the bundled Drum Sampler graph; insertion
/// rewrites them to the project's links to the global bundled kit (or to fresh
/// copies of the bundled bytes if an administrator removed those samples).
const BUNDLED_KIT_BASE: u32 = 900_001;
fn bundled_placeholder(index: usize) -> u32 {
    BUNDLED_KIT_BASE + index as u32
}

fn builtin_graph(id: &str) -> Option<Graph> {
    let (name, drum, sampler) = match id {
        BUILTIN_EPIANO => ("Electric Piano", false, false),
        BUILTIN_DRUMS => ("FM Drum Machine", true, false),
        BUILTIN_SAMPLER => ("Drum Sampler", true, true),
        _ => return None,
    };
    let mut nodes = vec![
        json!({"id":"root","kind":"subgraph","label":name,"x":0,"y":0,"channels":2,"parameters":{}}),
    ];
    let mut edges = Vec::new();
    if !drum {
        nodes.extend([
            json!({"id":"midi","kind":"subgraph_input_midi","label":"MIDI In","parent":"root","x":0,"y":80,"channels":1,"parameters":{}}),
            json!({"id":"fm","kind":"fm_synth","label":"Electric Piano FM","parent":"root","x":220,"y":50,"channels":2,"parameters":{"carrier_frequency":440.0,"modulator_frequency":880.0,"fm_depth":180.0,"amplitude":0.3,"release":220.0,"carrier_waveform":0.0,"modulator_waveform":0.0}}),
            json!({"id":"verb","kind":"reverb","label":"Light Reverb","parent":"root","x":470,"y":50,"channels":2,"parameters":{"decay":0.55,"mix":0.16}}),
            json!({"id":"out","kind":"subgraph_output_audio","label":"Stereo Out","parent":"root","x":720,"y":80,"channels":2,"parameters":{}}),
        ]);
        edges.extend([json!({"id":"e1","source":"midi","source_port":"out","target":"fm","target_port":"midi"}),json!({"id":"e2","source":"fm","source_port":"out","target":"verb","target_port":"in"}),json!({"id":"e3","source":"verb","source_port":"out","target":"out","target_port":"in"})]);
    } else {
        // One MIDI inlet fans out to six note decoders (General MIDI drum
        // numbers, matching the Drum pads node). Each voice takes pitch from a
        // shared constant so carrier/modulator frequencies are literal Hz,
        // velocity from the decoder or the pad's trigger inlet (send velocity
        // 1–127, return to 0 to rearm), and note_off from the decoder or the
        // trigger inlet falling back to 0.
        nodes.push(json!({"id":"midi","kind":"subgraph_input_midi","label":"MIDI In","parent":"root","x":0,"y":0,"channels":1,"parameters":{}}));
        // The sampler variant plays six one-shot project samples at their root
        // pitch; load a sample into each voice from its modal.
        nodes.push(json!({"id":"pitch","kind":"value","label":"Pad pitch","parent":"root","x":260,"y":0,"channels":1,"parameters":{"value":if sampler { 60.0 } else { 69.0 }}}));
        // Every voice is a one-shot: its decay envelope runs from the hit, so a
        // held pad or a missing note-off never leaves a drum ringing.
        let voices: [(&str, f64, f64, f64, f64, f64, f64, f64, f64); 6] = [
            // name, note, carrier Hz, modulator Hz, depth, amplitude, decay ms, release ms, modulator waveform
            ("Bass drum", 36., 55., 30., 120., 0.3, 350., 120., 0.),
            ("Snare", 38., 180., 700., 900., 0.22, 180., 80., 4.),
            ("Tom 1", 45., 120., 60., 150., 0.24, 420., 120., 0.),
            ("Tom 2", 50., 170., 85., 150., 0.24, 360., 120., 0.),
            ("Hi hat", 42., 800., 3000., 3000., 0.12, 80., 40., 4.),
            ("Cymbal", 49., 600., 2400., 2500., 0.12, 1200., 300., 4.),
        ];
        for (
            i,
            (name, note, carrier, modulator, depth, amplitude, decay, release, modulator_wave),
        ) in voices.iter().enumerate()
        {
            let y = 120.0 + i as f64 * 130.0;
            let (voice, trigger, decoder, velocity) = (
                format!("voice{i}"),
                format!("trigger{i}"),
                format!("notes{i}"),
                format!("velocity{i}"),
            );
            nodes.push(json!({"id":trigger,"kind":"subgraph_input_control","label":name,"parent":"root","x":0,"y":y,"channels":1,"parameters":{}}));
            nodes.push(json!({"id":decoder,"kind":"midi_to_control","label":format!("{name} notes"),"parent":"root","x":260,"y":y,"channels":1,"parameters":{"message_type":0.0,"number_filter":note}}));
            nodes.push(json!({"id":velocity,"kind":"max","label":format!("{name} velocity"),"parent":"root","x":520,"y":y,"channels":1,"parameters":{"a":0.0,"b":0.0}}));
            if sampler {
                nodes.push(json!({"id":voice,"kind":"poly_sampler","label":format!("{name} sample"),"parent":"root","x":780,"y":y,"channels":2,"parameters":{"asset":f64::from(bundled_placeholder(i)),"root_note":60.0,"amplitude":0.8,"loop":0.0,"release":80.0}}));
            } else {
                nodes.push(json!({"id":voice,"kind":"fm_synth","label":name,"parent":"root","x":780,"y":y,"channels":2,"parameters":{"carrier_frequency":carrier,"modulator_frequency":modulator,"fm_depth":depth,"amplitude":amplitude,"decay":decay,"sustain":0.0,"release":release,"carrier_waveform":0.0,"modulator_waveform":modulator_wave}}));
            }
            edges.push(json!({"id":format!("m{i}"),"source":"midi","source_port":"out","target":decoder,"target_port":"midi"}));
            edges.push(json!({"id":format!("p{i}"),"source":"pitch","source_port":"out","target":voice,"target_port":"pitch"}));
            edges.push(json!({"id":format!("v{i}a"),"source":decoder,"source_port":"value","target":velocity,"target_port":"a"}));
            edges.push(json!({"id":format!("v{i}b"),"source":trigger,"source_port":"out","target":velocity,"target_port":"b"}));
            edges.push(json!({"id":format!("v{i}"),"source":velocity,"source_port":"out","target":voice,"target_port":"velocity"}));
            edges.push(json!({"id":format!("t{i}"),"source":decoder,"source_port":"note_on","target":voice,"target_port":"trigger"}));
            edges.push(json!({"id":format!("t{i}pad"),"source":trigger,"source_port":"out","target":voice,"target_port":"trigger"}));
            // Samples play to their end as one-shots, so only the FM voices
            // take a release from the decoder.
            if !sampler {
                edges.push(json!({"id":format!("o{i}"),"source":decoder,"source_port":"note_off","target":voice,"target_port":"note_off"}));
            }
            let pair = i / 2;
            edges.push(json!({"id":format!("a{i}"),"source":voice,"source_port":"out","target":format!("mix{pair}"),"target_port":if i % 2 == 0 { "a" } else { "b" }}));
        }
        for (id, a, b, x, y, gain) in [
            ("mix0", "voice0", "voice1", 1040., 185., -3.),
            ("mix1", "voice2", "voice3", 1040., 445., -3.),
            ("mix2", "voice4", "voice5", 1040., 705., -3.),
            ("mix3", "mix0", "mix1", 1300., 315., 0.),
            ("mix4", "mix3", "mix2", 1560., 510., 0.),
        ] {
            nodes.push(json!({"id":id,"kind":"mixer","label":"Drum mix","parent":"root","x":x,"y":y,"channels":2,"parameters":{"gain":gain}}));
            if id == "mix3" || id == "mix4" {
                edges.push(json!({"id":format!("{id}a"),"source":a,"source_port":"out","target":id,"target_port":"a"}));
                edges.push(json!({"id":format!("{id}b"),"source":b,"source_port":"out","target":id,"target_port":"b"}));
            }
        }
        edges.push(json!({"id":"mixverb","source":"mix4","source_port":"out","target":"verb","target_port":"in"}));
        nodes.push(json!({"id":"verb","kind":"reverb","label":"Drum Room","parent":"root","x":1820,"y":510,"channels":2,"parameters":{"decay":0.35,"mix":0.08}}));
        nodes.push(json!({"id":"out","kind":"subgraph_output_audio","label":"Stereo Out","parent":"root","x":2080,"y":510,"channels":2,"parameters":{}}));
        edges.push(json!({"id":"vo","source":"verb","source_port":"out","target":"out","target_port":"in"}));
    }
    serde_json::from_value(json!({"nodes":nodes,"edges":edges})).ok()
}

#[cfg(test)]
mod builtin_tests {
    use super::*;
    #[test]
    fn builtins_have_valid_graphs() {
        for id in [BUILTIN_EPIANO, BUILTIN_DRUMS, BUILTIN_SAMPLER] {
            builtin_graph(id).unwrap().validate().unwrap();
        }
    }
    #[test]
    fn bundled_drum_kit_matches_the_sampler_preset_and_decodes() {
        let kit = crate::sample_library::BUNDLED_KIT;
        assert_eq!(kit.len(), 6);
        let graph = builtin_graph(BUILTIN_SAMPLER).unwrap();
        let referenced: BTreeSet<u32> = graph
            .nodes
            .iter()
            .filter(|n| n.kind == "poly_sampler")
            .map(|n| n.parameters["asset"] as u32)
            .collect();
        assert_eq!(
            referenced,
            (0..kit.len()).map(bundled_placeholder).collect()
        );
        for (id, _, bytes) in kit {
            let reader = hound::WavReader::new(std::io::Cursor::new(bytes)).unwrap();
            let spec = reader.spec();
            assert_eq!(
                (spec.channels, spec.sample_rate, spec.bits_per_sample),
                (2, 48000, 16),
                "{id}"
            );
            assert!(reader.duration() > 4800, "{id} is shorter than 100 ms");
        }
    }
}

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
    entries.insert(BUILTIN_EPIANO.into(), json!({"id":BUILTIN_EPIANO,"name":"Electric Piano","owned":false,"public":true,"owner":"pr0former","builtin":true,"versions":[{"version":1,"name":"FM electric piano","public":true}]}));
    entries.insert(BUILTIN_DRUMS.into(), json!({"id":BUILTIN_DRUMS,"name":"FM Drum Machine","owned":false,"public":true,"owner":"pr0former","builtin":true,"versions":[{"version":1,"name":"Six-trigger FM drums","public":true}]}));
    entries.insert(BUILTIN_SAMPLER.into(), json!({"id":BUILTIN_SAMPLER,"name":"Drum Sampler","owned":false,"public":true,"owner":"pr0former","builtin":true,"versions":[{"version":1,"name":"Six one-shot sample voices","public":true}]}));
    Ok(Json(json!(entries.into_values().collect::<Vec<_>>())))
}
fn read_version(db: &Connection, id: &str, version: u64, u: &str) -> Api<Graph> {
    if version == 1 {
        if let Some(graph) = builtin_graph(id) {
            return Ok(graph);
        }
    }
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
    crate::scripts::validate_graph(&graph, None).await.map_err(bad)?;
    // Copy originals into the immutable version, so another project does not depend on source-project files.
    let mut assets: BTreeSet<u32> = graph
        .nodes
        .iter()
        .filter(|n| {
            matches!(
                n.kind.as_str(),
                "sample" | "phase_vocoder" | "poly_sampler" | "granular_synth" | "granular_cloud"
            )
        })
        .filter_map(|n| n.parameters.get("asset"))
        .map(|v| *v as u32)
        .filter(|v| *v != 0)
        .collect();
    assets.extend(graph.nodes.iter().flat_map(|n| n.sample_choices.iter().map(|s| s.asset)));
    assets.extend(graph.nodes.iter().flat_map(field_sample_setters));
    let mut bytes = vec![];
    for asset in assets {
        bytes.push((
            asset,
            tokio::fs::read(
                crate::samples::directory(&app.config, &project).join(format!("{asset}.wav")),
            )
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
    // Library assets are copied into the project; the bundled Drum Sampler
    // instead links the project to the global bundled kit so the samples keep
    // their names and are not duplicated, falling back to the embedded bytes
    // only when an administrator has removed a bundled sample.
    let (mut graph, assets, linked) = {
        let db = app.db.lock().unwrap();
        let graph = read_version(&db, &c.library_id, c.version, &u)?;
        if c.library_id == BUILTIN_SAMPLER && c.version == 1 {
            let mut linked = Vec::new();
            let mut fallback = Vec::new();
            for (i, (sample, _, bytes)) in crate::sample_library::BUNDLED_KIT.iter().enumerate() {
                match crate::sample_library::attach(&app.config, &db, &project, sample) {
                    Ok(asset) => linked.push((bundled_placeholder(i), asset)),
                    Err(_) => fallback.push((bundled_placeholder(i), bytes.to_vec())),
                }
            }
            (graph, fallback, linked)
        } else {
            let mut query = db
                .prepare(
                    "SELECT asset,body FROM subgraph_assets WHERE library_id=?1 AND version=?2",
                )
                .map_err(internal)?;
            let assets = query
                .query_map(params![c.library_id, c.version], |r| {
                    Ok((r.get::<_, u32>(0)?, r.get::<_, Vec<u8>>(1)?))
                })
                .map_err(internal)?
                .collect::<Result<Vec<_>, _>>()
                .map_err(internal)?;
            (graph, assets, Vec::new())
        }
    };
    for n in &mut graph.nodes {
        if let Some(asset) = n.parameters.get("asset").copied() {
            if let Some((_, linked_asset)) = linked.iter().find(|(old, _)| f64::from(*old) == asset)
            {
                n.parameters
                    .insert("asset".into(), f64::from(*linked_asset));
            }
        }
    }
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
    crate::scripts::validate_graph(&candidate.graph, Some(&p.graph)).await.map_err(bad)?;
    let mut written = vec![];
    let result: Api<Json<pr0_core::Project>> = async {
        if !assets.is_empty() {
            tokio::fs::create_dir_all(crate::samples::directory(&app.config, &project))
                .await
                .map_err(internal)?;
        }
        for (old, bytes) in assets {
            let (asset, mut file, path) = loop {
                let asset = (uuid::Uuid::new_v4().as_u128() % 999999999 + 1) as u32;
                let path =
                    crate::samples::directory(&app.config, &project).join(format!("{asset}.wav"));
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
                for choice in &mut n.sample_choices {
                    if choice.asset == old { choice.asset = asset; }
                }
                if matches!(
                    n.kind.as_str(),
                    "sample" | "phase_vocoder" | "poly_sampler" | "granular_synth" | "granular_cloud"
                ) && n.parameters.get("asset").copied() == Some(old as f64)
                {
                    n.parameters.insert("asset".into(), asset as f64);
                }
                if n.kind == "granular_field" {
                    for (key, value) in n.parameters.iter_mut() {
                        if is_field_sample_setter(key) && *value == old as f64 {
                            *value = asset as f64;
                        }
                    }
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

/// `sample_N` on a Granular Field: a sample-setter parameter holding an asset id.
pub(crate) fn is_field_sample_setter(key: &str) -> bool {
    key.strip_prefix("sample_").is_some_and(|v| v.parse::<usize>().is_ok())
}
/// Nonzero sample-setter asset ids of a Granular Field node (empty for other kinds).
pub(crate) fn field_sample_setters(n: &pr0_core::Node) -> Vec<u32> {
    if n.kind != "granular_field" {
        return vec![];
    }
    n.parameters
        .iter()
        .filter(|(k, v)| is_field_sample_setter(k) && **v >= 1.)
        .map(|(_, v)| *v as u32)
        .collect()
}
