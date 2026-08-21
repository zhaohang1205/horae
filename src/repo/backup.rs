use rusqlite::Connection;
use std::collections::HashSet;

use crate::model::backup::{
    BackupData, BackupEvent, BackupTag, BackupTask, BackupTaskTag, BACKUP_FORMAT, BACKUP_VERSION,
};
use anyhow::{bail, Result};

/// Summary of what an import actually changed, for the CLI to report.
#[derive(Debug, Default)]
pub struct ImportStats {
    pub tasks_created: usize,
    pub tasks_skipped: usize,
    pub events_imported: usize,
    pub tags_created: usize,
    pub task_links: usize,
    pub settings_imported: usize,
    pub pomo_restored: bool,
}

/// Read every user-data table into a `BackupData`. Includes the pomodoro state
/// file so a backup is a complete restore point, not just the SQLite DB.
pub fn export_all(conn: &Connection) -> Result<BackupData> {
    Ok(BackupData {
        format: BACKUP_FORMAT.to_string(),
        version: BACKUP_VERSION,
        exported_at: crate::time::now_ms(),
        tasks: export_tasks(conn)?,
        events: export_events(conn)?,
        tags: export_tags(conn)?,
        task_tags: export_task_tags(conn)?,
        settings: export_settings(conn)?,
        pomodoro: Some(crate::repo::pomodoro::get_state()?),
    })
}

fn export_tasks(conn: &Connection) -> Result<Vec<BackupTask>> {
    let mut stmt = conn.prepare(
        "SELECT id,title,notes,status,rrule,created_at,clarified_at,due_at,\
         scheduled_start_at,scheduled_end_at,completed_at,archived_at,updated_at,\
         delegated_to,checklist,archive_reason \
         FROM tasks ORDER BY created_at, id",
    )?;
    let rows = stmt.query_map([], |r| {
        let cl_str: String = r.get(14)?;
        Ok(BackupTask {
            id: r.get(0)?,
            title: r.get(1)?,
            notes: r.get(2)?,
            status: r.get(3)?,
            rrule: r.get(4)?,
            created_at: r.get(5)?,
            clarified_at: r.get(6)?,
            due_at: r.get(7)?,
            scheduled_start_at: r.get(8)?,
            scheduled_end_at: r.get(9)?,
            completed_at: r.get(10)?,
            archived_at: r.get(11)?,
            updated_at: r.get(12)?,
            delegated_to: r.get(13)?,
            checklist: serde_json::from_str(&cl_str).unwrap_or_default(),
            archive_reason: r.get(15)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn export_events(conn: &Connection) -> Result<Vec<BackupEvent>> {
    let mut stmt = conn.prepare(
        "SELECT task_id,event_type,from_status,to_status,at,meta \
         FROM task_events ORDER BY at, id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(BackupEvent {
            task_id: r.get(0)?,
            event_type: r.get(1)?,
            from_status: r.get(2)?,
            to_status: r.get(3)?,
            at: r.get(4)?,
            meta: r.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn export_tags(conn: &Connection) -> Result<Vec<BackupTag>> {
    let mut stmt = conn.prepare(
        "SELECT name,category,is_system,color,icon,description,created_at \
         FROM tags ORDER BY name",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(BackupTag {
            name: r.get(0)?,
            category: r.get(1)?,
            is_system: r.get::<usize, i64>(2)? != 0,
            color: r.get(3)?,
            icon: r.get(4)?,
            description: r.get(5)?,
            created_at: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn export_task_tags(conn: &Connection) -> Result<Vec<BackupTaskTag>> {
    let mut stmt = conn.prepare(
        "SELECT tt.task_id, t.name, tt.added_at \
         FROM task_tags tt JOIN tags t ON t.id = tt.tag_id ORDER BY tt.task_id",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok(BackupTaskTag {
            task_id: r.get(0)?,
            tag_name: r.get(1)?,
            added_at: r.get(2)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn export_settings(conn: &Connection) -> Result<Vec<crate::model::backup::BackupSetting>> {
    let mut stmt = conn.prepare("SELECT key,value FROM settings ORDER BY key")?;
    let rows = stmt.query_map([], |r| {
        Ok(crate::model::backup::BackupSetting {
            key: r.get(0)?,
            value: r.get(1)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Restore `data` into the database.
///
/// `replace` wipes all task data (tasks, events, tags, associations, settings)
/// first so the DB ends up an exact copy of the backup — the true "restore"
/// path. Without it, the import merges: tasks whose id already exists are
/// skipped wholesale (events/tags included), everything else is added.
///
/// The restore is a raw INSERT of the recorded timeline — deliberately *no*
/// `captured`/`status_changed` events are logged for imports, since that would
/// duplicate the very history being restored.
pub fn import_all(conn: &Connection, data: &BackupData, replace: bool) -> Result<ImportStats> {
    if data.format != BACKUP_FORMAT {
        bail!(
            "unsupported backup format '{}' (expected '{}')",
            data.format,
            BACKUP_FORMAT
        );
    }
    if data.version != BACKUP_VERSION {
        bail!(
            "unsupported backup version {} (expected {})",
            data.version,
            BACKUP_VERSION
        );
    }

    // Merge mode: ids already present are skipped. Replace mode deletes every
    // task below, so nothing can pre-exist — treat the set as empty.
    let existing_ids: HashSet<String> = if replace {
        HashSet::new()
    } else {
        let mut stmt = conn.prepare("SELECT id FROM tasks")?;
        let rows = stmt.query_map([], |r| r.get::<usize, String>(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };

    let mut stats = crate::repo::mutate(conn, |tx, _now| {
        let mut stats = ImportStats::default();

        if replace {
            tx.execute("DELETE FROM task_tags", [])?;
            tx.execute("DELETE FROM task_events", [])?;
            tx.execute("DELETE FROM tasks", [])?;
            tx.execute("DELETE FROM tags", [])?;
            tx.execute("DELETE FROM settings", [])?;
        }

        // Tag reconciliation by name: reuse an existing tag (system presets are
        // seeded by migrations), create only what is missing.
        let mut tag_ids: Vec<(String, i64)> = Vec::new();
        for t in &data.tags {
            let existing = crate::repo::tags::get_tag_by_name(tx, &t.name)?;
            if let Some(tag) = existing {
                tag_ids.push((t.name.clone(), tag.id));
            } else {
                tx.execute(
                    "INSERT INTO tags (name,category,is_system,color,icon,description,created_at) \
                     VALUES (?1,?2,?3,?4,?5,?6,?7)",
                    rusqlite::params![
                        t.name,
                        t.category,
                        t.is_system as i64,
                        t.color,
                        t.icon,
                        t.description,
                        t.created_at
                    ],
                )?;
                tag_ids.push((t.name.clone(), tx.last_insert_rowid()));
                stats.tags_created += 1;
            }
        }

        // Insert tasks, keeping the original UUIDs so child links and the event
        // timeline stay intact. In merge mode, already-present ids are skipped.
        let mut imported: HashSet<String> = HashSet::new();
        for t in &data.tasks {
            if existing_ids.contains(&t.id) {
                stats.tasks_skipped += 1;
                continue;
            }
            let cl_str = serde_json::to_string(&t.checklist).unwrap_or_else(|_| "[]".to_string());
            tx.execute(
                "INSERT INTO tasks \
             (id,title,notes,status,rrule,created_at,clarified_at,due_at,\
              scheduled_start_at,scheduled_end_at,completed_at,archived_at,updated_at,\
              delegated_to,checklist,archive_reason) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
                rusqlite::params![
                    t.id,
                    t.title,
                    t.notes,
                    t.status,
                    t.rrule,
                    t.created_at,
                    t.clarified_at,
                    t.due_at,
                    t.scheduled_start_at,
                    t.scheduled_end_at,
                    t.completed_at,
                    t.archived_at,
                    t.updated_at,
                    t.delegated_to,
                    cl_str,
                    t.archive_reason
                ],
            )?;
            imported.insert(t.id.clone());
            stats.tasks_created += 1;
        }

        // Timeline: only for tasks that actually landed in this DB. In replace mode
        // that is every backup task; in merge mode only the newly imported ones.
        for e in &data.events {
            if imported.contains(&e.task_id) {
                tx.execute(
                    "INSERT INTO task_events (task_id,event_type,from_status,to_status,at,meta) \
                 VALUES (?1,?2,?3,?4,?5,?6)",
                    rusqlite::params![
                        e.task_id,
                        e.event_type,
                        e.from_status,
                        e.to_status,
                        e.at,
                        e.meta
                    ],
                )?;
                stats.events_imported += 1;
            }
        }

        // Tag associations, resolved by name → (existing/new) tag id.
        for tt in &data.task_tags {
            if !imported.contains(&tt.task_id) {
                continue;
            }
            let tag_id = tag_ids
                .iter()
                .find(|(n, _)| n == &tt.tag_name)
                .map(|(_, id)| *id);
            if let Some(tag_id) = tag_id {
                tx.execute(
                    "INSERT OR IGNORE INTO task_tags (task_id,tag_id,added_at) VALUES (?1,?2,?3)",
                    rusqlite::params![tt.task_id, tag_id, tt.added_at],
                )?;
                stats.task_links += 1;
            }
        }

        // Settings: merge upserts; replace leaves the table empty until here.
        for s in &data.settings {
            tx.execute(
                "INSERT INTO settings (key,value) VALUES (?1,?2) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                rusqlite::params![s.key, s.value],
            )?;
            stats.settings_imported += 1;
        }

        Ok(stats)
    })?;

    if let Some(state) = &data.pomodoro {
        crate::repo::pomodoro::save_state(state)?;
        stats.pomo_restored = true;
    }

    Ok(stats)
}

/// Serialize a `BackupData` to pretty JSON (used by the CLI + tests).
pub fn to_json(data: &BackupData) -> Result<String> {
    Ok(serde_json::to_string_pretty(data)?)
}

/// Parse a `BackupData` from JSON.
pub fn from_json(s: &str) -> Result<BackupData> {
    Ok(serde_json::from_str(s)?)
}
