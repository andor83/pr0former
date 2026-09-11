//! Working-copy updates retain optimistic concurrency without creating history rows.
//! A server-side timer coalesces dirty edits into one snapshot per minute.
use rusqlite::{Connection, params};
use serde::Serialize;

pub const AUTOSAVE_SECONDS: u64 = 60;

#[derive(Debug, Serialize)]
pub struct SaveStatus {
    pub revision: u64,
    pub change_revision: u64,
    pub dirty: bool,
}

#[derive(Debug, Serialize)]
pub struct RevisionEntry {
    pub revision: u64,
    pub current: bool,
}

pub fn list(db: &Connection, id: &str, current: u64) -> rusqlite::Result<Vec<RevisionEntry>> {
    let mut stmt =
        db.prepare("SELECT revision FROM revisions WHERE project_id=?1 ORDER BY revision DESC")?;
    stmt.query_map([id], |r| {
        let revision: u64 = r.get(0)?;
        Ok(RevisionEntry {
            revision,
            current: revision == current,
        })
    })?
    .collect()
}

pub fn body(db: &Connection, id: &str, revision: u64) -> rusqlite::Result<String> {
    db.query_row(
        "SELECT body FROM revisions WHERE project_id=?1 AND revision=?2",
        params![id, revision],
        |r| r.get(0),
    )
}

pub fn migrate(db: &Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS pending_saves(
        project_id TEXT PRIMARY KEY REFERENCES projects(id), due_at INTEGER NOT NULL);",
    )
}

pub fn status(db: &Connection, id: &str) -> rusqlite::Result<SaveStatus> {
    db.query_row(
        "SELECT COUNT(r.revision)-1, COALESCE(MAX(r.revision),0), p.revision
         FROM projects p JOIN revisions r ON r.project_id=p.id WHERE p.id=?1 GROUP BY p.id",
        [id],
        |r| {
            let change_revision: u64 = r.get(1)?;
            Ok(SaveStatus {
                revision: r.get(0)?,
                change_revision,
                dirty: r.get::<_, u64>(2)? > change_revision,
            })
        },
    )
}

/// Call inside the working-copy transaction. Further edits retain the first deadline.
pub fn schedule(db: &Connection, id: &str, now: u64) -> rusqlite::Result<()> {
    db.execute(
        "INSERT OR IGNORE INTO pending_saves(project_id,due_at) VALUES(?1,?2)",
        params![id, now + AUTOSAVE_SECONDS],
    )?;
    Ok(())
}

/// Save the latest accepted working copy, never a potentially stale browser copy.
/// Repeated saves with no changes are idempotent.
pub fn snapshot(db: &mut Connection, id: &str) -> rusqlite::Result<SaveStatus> {
    let tx = db.transaction()?;
    tx.execute(
        "INSERT OR IGNORE INTO revisions(project_id,revision,body)
        SELECT id,revision,body FROM projects WHERE id=?1",
        [id],
    )?;
    tx.execute("DELETE FROM pending_saves WHERE project_id=?1", [id])?;
    let saved = status(&tx, id)?;
    tx.commit()?;
    Ok(saved)
}

pub fn due(db: &Connection, now: u64) -> rusqlite::Result<Vec<String>> {
    db.prepare("SELECT project_id FROM pending_saves WHERE due_at<=?1")?
        .query_map([now], |r| r.get(0))?
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    fn database() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE projects(id TEXT PRIMARY KEY,revision INTEGER,body TEXT);
            CREATE TABLE revisions(project_id TEXT,revision INTEGER,body TEXT,PRIMARY KEY(project_id,revision));
            INSERT INTO projects VALUES('p',0,'initial');
            INSERT INTO revisions VALUES('p',0,'initial');").unwrap();
        migrate(&db).unwrap();
        db
    }
    fn edit(db: &Connection, revision: u64, now: u64) {
        db.execute(
            "UPDATE projects SET revision=?1,body=?2 WHERE id='p'",
            params![revision, format!("edit {revision}")],
        )
        .unwrap();
        schedule(db, "p", now).unwrap();
    }
    #[test]
    fn thousands_of_live_edits_produce_one_snapshot_per_minute() {
        let mut db = database();
        for revision in 1..=2000 {
            edit(&db, revision, 100 + (revision / 40) as u64);
        }
        assert_eq!(status(&db, "p").unwrap().revision, 0);
        assert!(status(&db, "p").unwrap().dirty);
        assert!(due(&db, 159).unwrap().is_empty());
        assert_eq!(due(&db, 160).unwrap(), vec!["p"]);
        let saved = snapshot(&mut db, "p").unwrap();
        assert_eq!(
            (saved.revision, saved.change_revision, saved.dirty),
            (1, 2000, false)
        );
        let body: String = db
            .query_row("SELECT body FROM revisions WHERE revision=2000", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(body, "edit 2000");
        edit(&db, 2001, 161);
        assert!(due(&db, 220).unwrap().is_empty());
        assert_eq!(due(&db, 221).unwrap(), vec!["p"]);
    }
    #[test]
    fn manual_save_is_immediate_idempotent_and_resets_autosave() {
        let mut db = database();
        edit(&db, 1, 100);
        assert_eq!(snapshot(&mut db, "p").unwrap().revision, 1);
        assert_eq!(snapshot(&mut db, "p").unwrap().revision, 1);
        assert!(due(&db, 1000).unwrap().is_empty());
        edit(&db, 2, 120);
        assert!(due(&db, 179).unwrap().is_empty());
        assert_eq!(due(&db, 180).unwrap(), vec!["p"]);
        assert_eq!(snapshot(&mut db, "p").unwrap().revision, 2);
    }
    #[test]
    fn migration_preserves_existing_history_and_pending_deadlines() {
        let db = database();
        db.execute_batch("INSERT INTO revisions VALUES('p',500,'legacy'); UPDATE projects SET revision=500 WHERE id='p';").unwrap();
        schedule(&db, "p", 100).unwrap();
        migrate(&db).unwrap();
        assert_eq!(status(&db, "p").unwrap().revision, 1);
        assert_eq!(status(&db, "p").unwrap().change_revision, 500);
        assert_eq!(due(&db, 160).unwrap(), vec!["p"]);
    }
}
