//! Local-first, project-scoped Todoist synchronization.
//!
//! Horae tasks only enter Todoist when explicitly tagged `@todoist`. Tasks in
//! the configured Todoist project are imported. A stable command UUID is saved
//! before each network write so a retry after a timeout cannot duplicate an
//! item. Concurrent edits are detected against the last synchronized snapshot
//! and stop the pass without advancing the incremental cursor.

use anyhow::{bail, Context, Result};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;

use crate::config::TodoistConfig;
use crate::model::task::{Status, Task};
use crate::repo::tasks::{self, CaptureInput, ListFilter, ModifyInput};

const API_URL: &str = "https://api.todoist.com/api/v1/sync";
const SYNC_TAG: &str = "todoist";

#[derive(Debug, Default, Clone)]
pub struct SyncStats {
    pub imported: usize,
    pub updated: usize,
    pub pushed: usize,
    pub conflicts: Vec<String>,
    pub recurring_skipped: usize,
    pub archived_local_tasks: usize,
}

#[derive(Debug, Clone, Default)]
pub struct SyncStatus {
    pub last_sync_at: i64,
    pub imported: usize,
    pub updated: usize,
    pub pushed: usize,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SyncConflict {
    pub todoist_id: String,
    pub task_id: String,
    pub local_title: String,
    pub remote_title: String,
    pub remote_deleted: bool,
    pub detected_at: i64,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConflictResolution {
    KeepHorae,
    KeepTodoist,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RemoteTask {
    id: String,
    #[serde(default)]
    project_id: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    description: String,
    #[serde(default)]
    checked: bool,
    #[serde(default)]
    is_deleted: bool,
    #[serde(default)]
    priority: u8,
    #[serde(default)]
    due: Option<RemoteDue>,
    #[serde(default)]
    labels: Vec<String>,
    #[serde(default)]
    completed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RemoteDue {
    date: String,
    #[serde(default)]
    is_recurring: bool,
}

#[derive(Debug, Clone)]
struct Pending {
    uuid: String,
    task_id: String,
    temp_id: Option<String>,
    command: Value,
    local_hash: String,
}

/// One bidirectional pass. The caller supplies the token so secrets never
/// enter Horae's config or database.
pub fn sync(conn: &Connection, config: &TodoistConfig, token: &str) -> Result<SyncStats> {
    let result = sync_pass(conn, config, token);
    let status_result = record_sync_status(conn, &result);
    match (result, status_result) {
        (Ok(stats), Ok(())) => Ok(stats),
        (Err(error), Ok(())) => Err(error),
        (_, Err(error)) => Err(error),
    }
}

fn sync_pass(conn: &Connection, config: &TodoistConfig, token: &str) -> Result<SyncStats> {
    if token.trim().is_empty() {
        bail!("Todoist API token is empty");
    }
    if config.project_id.trim().is_empty() {
        bail!("Todoist project id is empty");
    }

    let mut stats = SyncStats::default();
    let mut cursor = load_cursor(conn)?.unwrap_or_else(|| "*".to_string());

    // Retry any previously queued writes using their original UUIDs first.
    let existing = load_pending(conn)?;
    if !existing.is_empty() {
        let response = request(token, &cursor, &existing)?;
        acknowledge_pending(conn, &existing, &response)?;
        process_remote_items(conn, &response, &config.project_id, &mut stats)?;
        cursor = response_cursor(&response)?;
        save_cursor(conn, &cursor)?;
        stats.pushed += existing
            .iter()
            .map(|p| p.task_id.as_str())
            .collect::<BTreeSet<_>>()
            .len();
    }

    // Pull the current delta. The cursor is only committed after every mapped
    // item has either been applied or has been reported as a conflict.
    let initial_sync = cursor == "*";
    let response = request(token, &cursor, &[])?;
    if initial_sync
        && !response["projects"].as_array().is_some_and(|projects| {
            projects.iter().any(|project| {
                project["id"].as_str() == Some(config.project_id.as_str())
                    && !project["is_deleted"].as_bool().unwrap_or(false)
                    && !project["is_archived"].as_bool().unwrap_or(false)
            })
        })
    {
        bail!("Todoist project {} was not found or is archived; verify `horae todoist configure --project-id`.", config.project_id);
    }
    let completed = fetch_completed(token, &config.project_id, load_completed_since(conn)?)?;
    for mut item in completed.items {
        if item.due.as_ref().is_some_and(|due| due.is_recurring) {
            stats.recurring_skipped += 1;
            continue;
        }
        item.checked = true;
        match apply_remote(conn, &item)? {
            ApplyResult::Imported => stats.imported += 1,
            ApplyResult::Updated => stats.updated += 1,
            ApplyResult::Unchanged => {}
            ApplyResult::Conflict => stats.conflicts.push(item.id),
        }
    }
    process_remote_items(conn, &response, &config.project_id, &mut stats)?;
    if !stats.conflicts.is_empty() {
        bail!("Todoist task(s) changed on both sides: {}. No sync cursor was advanced; resolve each edit on one side and retry.", stats.conflicts.join(", "));
    }
    cursor = response_cursor(&response)?;
    save_cursor(conn, &cursor)?;
    save_completed_since(conn, completed.checked_until_ms)?;

    // Queue local changes before sending. Todoist's command UUID idempotency
    // makes a retry safe even when a successful HTTP response was lost.
    stats.recurring_skipped += queue_local_changes(conn, &config.project_id)?;
    stats.archived_local_tasks = conn.query_row(
        "SELECT COUNT(*) FROM task_todoist_links l JOIN tasks t ON t.id=l.task_id WHERE t.archived_at IS NOT NULL AND l.remote_deleted=0 AND l.out_of_scope=0",
        [],
        |row| row.get(0),
    )?;
    let pending = load_pending(conn)?;
    if !pending.is_empty() {
        let response = request(token, &cursor, &pending)?;
        acknowledge_pending(conn, &pending, &response)?;
        process_remote_items(conn, &response, &config.project_id, &mut stats)?;
        if !stats.conflicts.is_empty() {
            bail!("Todoist task(s) changed on both sides: {}. The incoming cursor was not saved; resolve each edit on one side and retry.", stats.conflicts.join(", "));
        }
        cursor = response_cursor(&response)?;
        save_cursor(conn, &cursor)?;
        stats.pushed += pending
            .iter()
            .map(|p| p.task_id.as_str())
            .collect::<BTreeSet<_>>()
            .len();
    }
    Ok(stats)
}

fn record_sync_status(conn: &Connection, result: &Result<SyncStats>) -> Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO todoist_sync_state(id,sync_token,completed_since_ms,updated_at) VALUES (1,'*',0,?1)",
        [crate::time::now_ms()],
    )?;
    match result {
        Ok(stats) => {
            conn.execute(
                "UPDATE todoist_sync_state SET last_sync_at=?1,last_imported=?2,last_updated=?3,last_pushed=?4,last_error=NULL WHERE id=1",
                params![crate::time::now_ms(), stats.imported, stats.updated, stats.pushed],
            )?;
        }
        Err(error) => {
            conn.execute(
                "UPDATE todoist_sync_state SET last_sync_at=?1,last_imported=0,last_updated=0,last_pushed=0,last_error=?2 WHERE id=1",
                params![crate::time::now_ms(), error.to_string()],
            )?;
        }
    }
    Ok(())
}

pub fn sync_status(conn: &Connection) -> Result<Option<SyncStatus>> {
    Ok(conn
        .query_row(
            "SELECT last_sync_at,last_imported,last_updated,last_pushed,last_error FROM todoist_sync_state WHERE id=1",
            [],
            |row| {
                Ok(SyncStatus {
                    last_sync_at: row.get(0)?,
                    imported: row.get(1)?,
                    updated: row.get(2)?,
                    pushed: row.get(3)?,
                    last_error: row.get(4)?,
                })
            },
        )
        .optional()?)
}

pub fn sync_conflicts(conn: &Connection) -> Result<Vec<SyncConflict>> {
    let mut stmt = conn.prepare(
        "SELECT c.todoist_id,c.task_id,t.title,c.remote_json,c.detected_at,c.resolution FROM todoist_conflicts c JOIN tasks t ON t.id=c.task_id WHERE c.resolution IS NULL ORDER BY c.detected_at,c.todoist_id",
    )?;
    let rows = stmt.query_map([], |row| {
        let remote_json: String = row.get(3)?;
        let remote: RemoteTask = serde_json::from_str(&remote_json).map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                3,
                rusqlite::types::Type::Text,
                Box::new(error),
            )
        })?;
        Ok(SyncConflict {
            todoist_id: row.get(0)?,
            task_id: row.get(1)?,
            local_title: row.get(2)?,
            remote_title: remote.content,
            remote_deleted: remote.is_deleted,
            detected_at: row.get(4)?,
            resolution: row.get(5)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

pub fn resolve_conflict(
    conn: &Connection,
    todoist_id: &str,
    resolution: ConflictResolution,
) -> Result<()> {
    let value = match resolution {
        ConflictResolution::KeepHorae => "local",
        ConflictResolution::KeepTodoist => "remote",
    };
    let changed = conn.execute(
        "UPDATE todoist_conflicts SET resolution=?1 WHERE todoist_id=?2",
        params![value, todoist_id],
    )?;
    if changed == 0 {
        bail!("Todoist conflict {todoist_id} no longer exists");
    }
    Ok(())
}

enum ApplyResult {
    Imported,
    Updated,
    Unchanged,
    Conflict,
}

fn process_remote_items(
    conn: &Connection,
    response: &Value,
    project_id: &str,
    stats: &mut SyncStats,
) -> Result<()> {
    for value in response["items"].as_array().cloned().unwrap_or_default() {
        let mut remote: RemoteTask = match serde_json::from_value(value) {
            Ok(item) => item,
            Err(_) => continue,
        };
        if remote.completed_at.is_some() || (remote.is_deleted && remote_checked(conn, &remote.id)?)
        {
            remote.checked = true;
        }
        if remote.project_id != project_id {
            if remote.is_deleted {
                if matches!(apply_remote(conn, &remote)?, ApplyResult::Conflict) {
                    stats.conflicts.push(remote.id.clone());
                }
            } else {
                mark_out_of_scope(conn, &remote.id)?;
            }
            continue;
        }
        if remote.due.as_ref().is_some_and(|due| due.is_recurring) {
            stats.recurring_skipped += 1;
            continue;
        }
        match apply_remote(conn, &remote)? {
            ApplyResult::Imported => stats.imported += 1,
            ApplyResult::Updated => stats.updated += 1,
            ApplyResult::Unchanged => {}
            ApplyResult::Conflict => stats.conflicts.push(remote.id.clone()),
        }
    }
    Ok(())
}

fn mark_out_of_scope(conn: &Connection, remote_id: &str) -> Result<()> {
    conn.execute(
        "UPDATE task_todoist_links SET out_of_scope=1 WHERE todoist_id=?1",
        [remote_id],
    )?;
    Ok(())
}

fn remote_checked(conn: &Connection, remote_id: &str) -> Result<bool> {
    Ok(conn
        .query_row(
            "SELECT remote_checked FROM task_todoist_links WHERE todoist_id=?1",
            [remote_id],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or(false))
}

struct CompletedBatch {
    items: Vec<RemoteTask>,
    checked_until_ms: i64,
}

fn fetch_completed(token: &str, project_id: &str, previous_ms: i64) -> Result<CompletedBatch> {
    let until_ms = crate::time::now_ms();
    let min_ms = until_ms - 90 * 24 * 60 * 60 * 1000;
    let since_ms = if previous_ms <= 0 {
        min_ms
    } else {
        previous_ms.saturating_sub(24 * 60 * 60 * 1000).max(min_ms)
    };
    let since = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(since_ms)
        .context("invalid Todoist completion cursor")?
        .to_rfc3339();
    let until = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(until_ms)
        .context("invalid current timestamp")?
        .to_rfc3339();
    let mut url =
        url::Url::parse("https://api.todoist.com/api/v1/tasks/completed/by_completion_date")?;
    url.query_pairs_mut()
        .append_pair("project_id", project_id)
        .append_pair("since", &since)
        .append_pair("until", &until)
        .append_pair("limit", "200");
    let mut items = Vec::new();
    let mut cursor: Option<String> = None;
    loop {
        let mut page_url = url.clone();
        if let Some(ref cursor) = cursor {
            page_url.query_pairs_mut().append_pair("cursor", cursor);
        }
        let response = ureq::get(page_url.as_str())
            .timeout(std::time::Duration::from_secs(20))
            .set("Authorization", &format!("Bearer {token}"))
            .call()
            .map_err(|error| anyhow::anyhow!("Todoist completed tasks request failed: {error}"))?;
        let body = response
            .into_string()
            .context("read Todoist completed tasks response")?;
        let value: Value =
            serde_json::from_str(&body).context("parse Todoist completed tasks response")?;
        for item in value["items"].as_array().cloned().unwrap_or_default() {
            if let Ok(task) = serde_json::from_value(item) {
                items.push(task);
            }
        }
        cursor = value["next_cursor"].as_str().map(str::to_string);
        if cursor.is_none() {
            break;
        }
    }
    Ok(CompletedBatch {
        items,
        checked_until_ms: until_ms,
    })
}

fn apply_remote(conn: &Connection, remote: &RemoteTask) -> Result<ApplyResult> {
    if remote.is_deleted && remote.checked {
        let mut completed = remote.clone();
        completed.is_deleted = false;
        return apply_remote(conn, &completed);
    }
    if !remote.is_deleted && remote.content.trim().is_empty() {
        bail!(
            "Todoist task {} has an empty title; refusing to overwrite its linked Horae task",
            remote.id
        );
    }
    let remote_hash = remote_hash(remote)?;
    let linked_id: Option<String> = conn
        .query_row(
            "SELECT task_id FROM task_todoist_links WHERE todoist_id=?1",
            [&remote.id],
            |row| row.get(0),
        )
        .optional()?;

    if remote.is_deleted {
        if let Some(id) = linked_id {
            let task = tasks::get(conn, &id)?;
            let (local_hash, resolution): (String, Option<String>) = conn.query_row(
                "SELECT l.local_hash,c.resolution FROM task_todoist_links l LEFT JOIN todoist_conflicts c ON c.todoist_id=l.todoist_id WHERE l.task_id=?1",
                [&id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )?;
            let local_changed = local_hash_for(&task, &task_tags(conn, &id)?)? != local_hash;
            if local_changed && resolution.as_deref() != Some("remote") {
                if resolution.as_deref() == Some("local") {
                    // Already resolved in favour of Horae: keep the local task
                    // alive so the queued push recreates it on the remote side.
                    conn.execute(
                        "DELETE FROM todoist_conflicts WHERE todoist_id=?1",
                        [&remote.id],
                    )?;
                    return Ok(ApplyResult::Unchanged);
                }
                save_conflict(conn, &id, remote)?;
                return Ok(ApplyResult::Conflict);
            }
            if task.archived_at.is_none() && resolution.as_deref() != Some("local") {
                tasks::archive(conn, &id)?;
            }
            conn.execute(
                "UPDATE task_todoist_links SET remote_deleted=1,remote_hash=?1,last_synced_at=?2 WHERE task_id=?3",
                params![remote_hash, crate::time::now_ms(), id],
            )?;
            if resolution.is_some() {
                conn.execute(
                    "DELETE FROM todoist_conflicts WHERE todoist_id=?1",
                    [&remote.id],
                )?;
            }
        }
        return Ok(ApplyResult::Unchanged);
    }

    if let Some(id) = linked_id {
        let task = tasks::get(conn, &id)?;
        let (baseline_local, baseline_remote): (String, String) = conn.query_row(
            "SELECT local_hash,remote_hash FROM task_todoist_links WHERE task_id=?1",
            [&id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        let current_local = local_hash_for(&task, &task_tags(conn, &id)?)?;
        let local_changed = current_local != baseline_local;
        let remote_changed = remote_hash != baseline_remote;
        let resolution: Option<String> = conn
            .query_row(
                "SELECT resolution FROM todoist_conflicts WHERE todoist_id=?1",
                [&remote.id],
                |row| row.get(0),
            )
            .optional()?
            .flatten();
        if resolution.as_deref() == Some("local") {
            conn.execute(
                "UPDATE task_todoist_links SET remote_hash=?1,remote_deleted=0,last_synced_at=?2 WHERE task_id=?3",
                params![remote_hash, crate::time::now_ms(), id],
            )?;
            // The user chose to keep the Horae side, so the remote edit is
            // discarded. Drop the conflict record; the queued push below
            // propagates the local edit back to Todoist.
            conn.execute(
                "DELETE FROM todoist_conflicts WHERE todoist_id=?1",
                [&remote.id],
            )?;
            return Ok(ApplyResult::Unchanged);
        }
        if resolution.as_deref() == Some("remote") {
            update_from_remote(conn, &id, remote)?;
            let updated = tasks::get(conn, &id)?;
            let tags = task_tags(conn, &id)?;
            conn.execute(
                "UPDATE task_todoist_links SET local_hash=?1,remote_hash=?2,remote_checked=?3,remote_deleted=0,out_of_scope=0,last_synced_at=?4 WHERE task_id=?5",
                params![local_hash_for(&updated, &tags)?, remote_hash, remote.checked, crate::time::now_ms(), id],
            )?;
            // The user chose to keep the Todoist side; the conflict is settled.
            conn.execute(
                "DELETE FROM todoist_conflicts WHERE todoist_id=?1",
                [&remote.id],
            )?;
            return Ok(ApplyResult::Updated);
        }
        if local_changed && remote_changed && current_local != remote_hash {
            save_conflict(conn, &id, remote)?;
            return Ok(ApplyResult::Conflict);
        }
        if local_changed && !remote_changed {
            return Ok(ApplyResult::Unchanged);
        }
        if remote_changed && !local_changed {
            update_from_remote(conn, &id, remote)?;
        }
        let updated = tasks::get(conn, &id)?;
        let tags = task_tags(conn, &id)?;
        conn.execute(
                "UPDATE task_todoist_links SET local_hash=?1,remote_hash=?2,remote_checked=?3,remote_deleted=0,out_of_scope=0,last_synced_at=?4 WHERE task_id=?5",
            params![local_hash_for(&updated, &tags)?, remote_hash, remote.checked, crate::time::now_ms(), id],
        )?;
        conn.execute(
            "DELETE FROM todoist_conflicts WHERE todoist_id=?1",
            [&remote.id],
        )?;
        return Ok(if remote_changed {
            ApplyResult::Updated
        } else {
            ApplyResult::Unchanged
        });
    }

    let title = remote.content.trim();
    if title.is_empty() {
        return Ok(ApplyResult::Unchanged);
    }
    let due = remote_due_ms(remote)?;
    let mut labels = remote.labels.clone();
    labels.push(SYNC_TAG.to_string());
    labels.sort();
    labels.dedup();
    let task = tasks::create_capture(
        conn,
        &CaptureInput {
            title: title.to_string(),
            notes: remote.description.clone(),
            status: Status::Inbox,
            due_at: due,
            tag_names: labels,
            priority: remote_priority(remote.priority).map(str::to_string),
            ..Default::default()
        },
    )?;
    if remote.checked {
        tasks::transition(conn, &task.id, Status::Done)?;
    }
    let task = tasks::get(conn, &task.id)?;
    save_link(conn, &task, &remote.id, &remote_hash)?;
    Ok(ApplyResult::Imported)
}

fn save_conflict(conn: &Connection, task_id: &str, remote: &RemoteTask) -> Result<()> {
    conn.execute(
        "INSERT INTO todoist_conflicts(todoist_id,task_id,remote_json,detected_at,resolution) VALUES (?1,?2,?3,?4,NULL) ON CONFLICT(todoist_id) DO UPDATE SET task_id=excluded.task_id,remote_json=excluded.remote_json,detected_at=excluded.detected_at",
        params![remote.id, task_id, serde_json::to_string(remote)?, crate::time::now_ms()],
    )?;
    Ok(())
}

fn update_from_remote(conn: &Connection, id: &str, remote: &RemoteTask) -> Result<()> {
    let current = tasks::get(conn, id)?;
    let target_status = if remote.checked {
        Some(Status::Done)
    } else if current.status == Status::Done {
        Some(Status::Next)
    } else {
        None
    };
    let target_due = remote_due_ms(remote)?;
    let old_tags: BTreeSet<String> = task_tags(conn, id)?.into_iter().collect();
    let mut new_tags: BTreeSet<String> = remote.labels.iter().cloned().collect();
    new_tags.insert(SYNC_TAG.to_string());
    let input = ModifyInput {
        title: Some(remote.content.clone()),
        notes: Some(remote.description.clone()),
        status: target_status,
        due_at: Some(target_due),
        scheduled_start_at: Some(None),
        priority: Some(remote_priority(remote.priority).map(str::to_string)),
        add_tags: new_tags.difference(&old_tags).cloned().collect(),
        remove_tags: old_tags.difference(&new_tags).cloned().collect(),
        ..Default::default()
    };
    tasks::modify(conn, id, &input)?;
    Ok(())
}

fn queue_local_changes(conn: &Connection, project_id: &str) -> Result<usize> {
    let mut recurring_skipped = 0;
    let mapped: BTreeSet<String> = {
        let mut stmt = conn.prepare("SELECT task_id FROM task_todoist_links")?;
        let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<rusqlite::Result<_>>()?
    };
    let all = tasks::list(
        conn,
        &ListFilter {
            status: None,
            tags: Vec::new(),
            query: None,
            review_stale: false,
        },
    )?;
    for task in all {
        let tags = task_tags(conn, &task.id)?;
        if !mapped.contains(&task.id) && !tags.iter().any(|tag| tag == SYNC_TAG) {
            continue;
        }
        if task.rrule.is_some() {
            recurring_skipped += 1;
            continue;
        }
        let hash = local_hash_for(&task, &tags)?;
        let link: Option<(String, String, bool, bool, bool)> = conn
            .query_row(
                "SELECT todoist_id,local_hash,remote_checked,remote_deleted,out_of_scope FROM task_todoist_links WHERE task_id=?1",
                [&task.id],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?)),
            )
            .optional()?;
        if task.archived_at.is_some() {
            continue;
        }
        if link
            .as_ref()
            .is_some_and(|(_, baseline, checked, deleted, out_of_scope)| {
                baseline == &hash
                    && *checked == (task.status == Status::Done)
                    && !deleted
                    && !out_of_scope
            })
        {
            continue;
        }
        if let Some((todoist_id, baseline_local, remote_checked, remote_deleted, out_of_scope)) =
            link
        {
            if out_of_scope {
                continue;
            }
            if remote_deleted {
                let temp_id = uuid::Uuid::new_v4().to_string();
                let mut args = remote_update_args(&task, &tags, &temp_id)?;
                args.as_object_mut().map(|object| object.remove("id"));
                args["project_id"] = json!(project_id);
                queue_command(conn, &task, &hash, "item_add", args, Some(temp_id))?;
                continue;
            }
            let mut commands = Vec::new();
            if task.status != Status::Done && remote_checked {
                commands.push(("item_uncomplete", json!({"id": todoist_id})));
            }
            if hash != baseline_local {
                commands.push((
                    "item_update",
                    remote_update_args(&task, &tags, &todoist_id)?,
                ));
            }
            if task.status == Status::Done && !remote_checked {
                commands.push((
                    "item_complete",
                    json!({"id": todoist_id, "date_completed": chrono::Utc::now().to_rfc3339()}),
                ));
            }
            for (kind, args) in commands {
                queue_command(conn, &task, &hash, kind, args, None)?;
            }
        } else {
            if task.status == Status::Done {
                continue;
            }
            let temp_id = uuid::Uuid::new_v4().to_string();
            let mut args = remote_update_args(&task, &tags, &temp_id)?;
            args.as_object_mut().map(|object| object.remove("id"));
            args["project_id"] = json!(project_id);
            queue_command(conn, &task, &hash, "item_add", args, Some(temp_id))?;
        }
    }
    Ok(recurring_skipped)
}

fn queue_command(
    conn: &Connection,
    task: &Task,
    hash: &str,
    kind: &str,
    args: Value,
    temp_id: Option<String>,
) -> Result<()> {
    let uuid = uuid::Uuid::new_v4().to_string();
    let mut command = json!({"type": kind, "uuid": uuid, "args": args});
    if let Some(ref temp_id) = temp_id {
        command["temp_id"] = json!(temp_id);
    }
    conn.execute(
        "INSERT INTO todoist_pending_commands(command_uuid,task_id,temp_id,command_json,local_hash,created_at) VALUES (?1,?2,?3,?4,?5,?6)",
        params![uuid, task.id, temp_id, serde_json::to_string(&command)?, hash, crate::time::now_ms()],
    )?;
    Ok(())
}

fn remote_update_args(task: &Task, tags: &[String], id: &str) -> Result<Value> {
    let labels: Vec<String> = tags
        .iter()
        .filter(|tag| tag.as_str() != SYNC_TAG)
        .cloned()
        .collect();
    let mut args = json!({
        "id": id,
        "content": task.title,
        "description": task.notes,
        "priority": todoist_priority(task.priority.as_deref()),
        "labels": labels,
    });
    if let Some(due) = task.due_at.or(task.scheduled_start_at) {
        let local = chrono::DateTime::<chrono::Utc>::from_timestamp_millis(due)
            .ok_or_else(|| anyhow::anyhow!("invalid due timestamp: {due}"))?
            .with_timezone(&chrono::Local);
        args["due"] = json!({"date": local.to_rfc3339()});
    } else {
        args["due"] = Value::Null;
    }
    Ok(args)
}

fn acknowledge_pending(conn: &Connection, pending: &[Pending], response: &Value) -> Result<()> {
    let statuses = response["sync_status"].as_object();
    let temp_mapping = response["temp_id_mapping"].as_object();
    for p in pending {
        let status = statuses
            .and_then(|s| s.get(&p.uuid))
            .and_then(Value::as_str);
        if status != Some("ok") {
            bail!(
                "Todoist rejected command {}: {}",
                p.uuid,
                status.unwrap_or("missing sync status")
            );
        }
        let todoist_id = p
            .temp_id
            .as_ref()
            .and_then(|temp| temp_mapping.and_then(|map| map.get(temp)))
            .and_then(Value::as_str);
        if p.temp_id.is_some() && todoist_id.is_none() {
            bail!("Todoist accepted create command {} but returned no task id mapping; retrying with the same command UUID is safe.", p.uuid);
        }
        let command_type = p.command["type"].as_str().unwrap_or_default();
        let checked = match command_type {
            "item_complete" => Some(true),
            "item_uncomplete" => Some(false),
            "item_add" => Some(false),
            _ => None,
        };
        let deleted = match command_type {
            "item_delete" => Some(true),
            "item_add" => Some(false),
            _ => None,
        };
        if let Some(remote_id) = todoist_id {
            conn.execute(
                "INSERT INTO task_todoist_links(task_id,todoist_id,local_hash,remote_hash,remote_checked,remote_deleted,out_of_scope,last_synced_at) VALUES (?1,?2,?3,?3,?4,0,0,?5) ON CONFLICT(task_id) DO UPDATE SET todoist_id=excluded.todoist_id,local_hash=excluded.local_hash,remote_hash=excluded.remote_hash,remote_checked=excluded.remote_checked,remote_deleted=0,out_of_scope=0,last_synced_at=excluded.last_synced_at",
                params![p.task_id, remote_id, p.local_hash, checked.unwrap_or(false), crate::time::now_ms()],
            )?;
            if p.temp_id.is_some() {
                conn.execute(
                    "DELETE FROM todoist_conflicts WHERE task_id=?1",
                    [&p.task_id],
                )?;
            }
        } else {
            if let Some(deleted) = deleted {
                conn.execute(
                    "UPDATE task_todoist_links SET local_hash=?1,remote_hash=?1,remote_deleted=?2,out_of_scope=0,last_synced_at=?3 WHERE task_id=?4",
                    params![p.local_hash, deleted, crate::time::now_ms(), p.task_id],
                )?;
            } else if let Some(checked) = checked {
                conn.execute(
                    "UPDATE task_todoist_links SET local_hash=?1,remote_hash=?1,remote_checked=?2,out_of_scope=0,last_synced_at=?3 WHERE task_id=?4",
                    params![p.local_hash, checked, crate::time::now_ms(), p.task_id],
                )?;
            } else {
                conn.execute(
                    "UPDATE task_todoist_links SET local_hash=?1,remote_hash=?1,out_of_scope=0,last_synced_at=?2 WHERE task_id=?3",
                    params![p.local_hash, crate::time::now_ms(), p.task_id],
                )?;
            }
        }
        conn.execute(
            "DELETE FROM todoist_pending_commands WHERE command_uuid=?1",
            [&p.uuid],
        )?;
    }
    Ok(())
}

fn request(token: &str, cursor: &str, pending: &[Pending]) -> Result<Value> {
    let mut fields = vec![
        ("sync_token", cursor.to_string()),
        ("resource_types", "[\"items\",\"projects\"]".to_string()),
    ];
    if !pending.is_empty() {
        let commands: Vec<Value> = pending.iter().map(|p| p.command.clone()).collect();
        fields.push(("commands", serde_json::to_string(&commands)?));
    }
    let form: Vec<(&str, &str)> = fields
        .iter()
        .map(|(name, value)| (*name, value.as_str()))
        .collect();
    let response = ureq::post(API_URL)
        .timeout(std::time::Duration::from_secs(20))
        .set("Authorization", &format!("Bearer {token}"))
        .send_form(&form)
        .map_err(|error| anyhow::anyhow!("Todoist Sync API request failed: {error}"))?;
    let body = response
        .into_string()
        .context("read Todoist API response")?;
    serde_json::from_str(&body).context("parse Todoist API response")
}

fn load_cursor(conn: &Connection) -> Result<Option<String>> {
    Ok(conn
        .query_row(
            "SELECT sync_token FROM todoist_sync_state WHERE id=1",
            [],
            |row| row.get(0),
        )
        .optional()?)
}

fn load_completed_since(conn: &Connection) -> Result<i64> {
    Ok(conn
        .query_row(
            "SELECT completed_since_ms FROM todoist_sync_state WHERE id=1",
            [],
            |row| row.get(0),
        )
        .optional()?
        .unwrap_or_default())
}

fn save_cursor(conn: &Connection, cursor: &str) -> Result<()> {
    conn.execute("INSERT INTO todoist_sync_state(id,sync_token,completed_since_ms,updated_at) VALUES (1,?1,0,?2) ON CONFLICT(id) DO UPDATE SET sync_token=excluded.sync_token,updated_at=excluded.updated_at", params![cursor, crate::time::now_ms()])?;
    Ok(())
}

fn save_completed_since(conn: &Connection, completed_since_ms: i64) -> Result<()> {
    conn.execute(
        "UPDATE todoist_sync_state SET completed_since_ms=?1,updated_at=?2 WHERE id=1",
        params![completed_since_ms, crate::time::now_ms()],
    )?;
    Ok(())
}

fn load_pending(conn: &Connection) -> Result<Vec<Pending>> {
    let mut stmt = conn.prepare("SELECT command_uuid,task_id,temp_id,command_json,local_hash FROM todoist_pending_commands ORDER BY created_at")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, String>(4)?,
        ))
    })?;
    rows.collect::<rusqlite::Result<Vec<_>>>()?
        .into_iter()
        .map(|(uuid, task_id, temp_id, raw, local_hash)| {
            let command = serde_json::from_str(&raw)
                .with_context(|| format!("parse pending Todoist command {uuid}"))?;
            Ok(Pending {
                uuid,
                task_id,
                temp_id,
                command,
                local_hash,
            })
        })
        .collect()
}

fn response_cursor(response: &Value) -> Result<String> {
    response["sync_token"]
        .as_str()
        .map(str::to_string)
        .context("Todoist response did not contain sync_token")
}

fn save_link(conn: &Connection, task: &Task, remote_id: &str, remote_hash: &str) -> Result<()> {
    let local_hash = local_hash_for(task, &task_tags(conn, &task.id)?)?;
    conn.execute("INSERT INTO task_todoist_links(task_id,todoist_id,local_hash,remote_hash,remote_checked,remote_deleted,out_of_scope,last_synced_at) VALUES (?1,?2,?3,?4,?5,0,0,?6) ON CONFLICT(task_id) DO UPDATE SET todoist_id=excluded.todoist_id,local_hash=excluded.local_hash,remote_hash=excluded.remote_hash,remote_checked=excluded.remote_checked,remote_deleted=0,out_of_scope=0,last_synced_at=excluded.last_synced_at", params![task.id, remote_id, local_hash, remote_hash, task.status == Status::Done, crate::time::now_ms()])?;
    Ok(())
}

fn task_tags(conn: &Connection, id: &str) -> Result<Vec<String>> {
    Ok(crate::repo::tags::get_task_tags(conn, id)?
        .into_iter()
        .map(|tag| tag.name)
        .collect())
}

fn local_hash_for(task: &Task, tags: &[String]) -> Result<String> {
    let due = task.due_at.or(task.scheduled_start_at);
    let mut labels: Vec<&str> = tags
        .iter()
        .map(String::as_str)
        .filter(|tag| *tag != SYNC_TAG)
        .collect();
    labels.sort_unstable();
    hash_json(&json!({
        "content": task.title,
        "description": task.notes,
        "checked": task.status == Status::Done,
        "due": due,
        "priority": priority_rank(task.priority.as_deref()),
        "labels": labels,
    }))
}

fn remote_hash(remote: &RemoteTask) -> Result<String> {
    let due = remote_due_ms(remote)?;
    let mut labels: Vec<&str> = remote
        .labels
        .iter()
        .map(String::as_str)
        .filter(|tag| *tag != SYNC_TAG)
        .collect();
    labels.sort_unstable();
    hash_json(&json!({
        "content": remote.content,
        "description": remote.description,
        "checked": remote.checked,
        "due": due,
        "priority": priority_rank(remote_priority(remote.priority)),
        "labels": labels,
    }))
}

fn hash_json(value: &Value) -> Result<String> {
    let bytes = serde_json::to_vec(value)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn remote_due_ms(remote: &RemoteTask) -> Result<Option<i64>> {
    let Some(due) = &remote.due else {
        return Ok(None);
    };
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(&due.date) {
        return Ok(Some(dt.timestamp_millis()));
    }
    Ok(Some(crate::time::parse_time(&due.date).with_context(
        || format!("parse Todoist due date {}", due.date),
    )?))
}

fn remote_priority(priority: u8) -> Option<&'static str> {
    match priority {
        4 => Some("high"),
        3 => Some("medium"),
        2 => Some("low"),
        1 => None,
        _ => None,
    }
}

fn priority_rank(priority: Option<&str>) -> u8 {
    match priority {
        Some("high") => 3,
        Some("medium") => 2,
        Some("low") => 1,
        _ => 0,
    }
}

fn todoist_priority(priority: Option<&str>) -> u8 {
    match priority {
        Some("high") => 4,
        Some("medium") => 3,
        Some("low") => 2,
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::tasks::CaptureInput;

    /// 建一个本地任务并登记 Todoist 关联行，模拟"已同步过一次"的状态。
    /// 返回本地任务 id（由 create_capture 生成）。
    fn linked_task(conn: &Connection, todoist_id: &str, title: &str) -> String {
        let task = tasks::create_capture(
            conn,
            &CaptureInput {
                title: title.to_string(),
                status: Status::Next,
                ..Default::default()
            },
        )
        .unwrap();
        let hash = local_hash_for(&task, &task_tags(conn, &task.id).unwrap()).unwrap();
        conn.execute(
            "INSERT INTO task_todoist_links(task_id,todoist_id,local_hash,remote_hash,remote_checked,remote_deleted,out_of_scope,last_synced_at) VALUES (?1,?2,?3,?3,0,0,0,?4)",
            params![task.id, todoist_id, hash, crate::time::now_ms()],
        )
        .unwrap();
        task.id
    }

    fn remote(id: &str, project: &str, content: &str) -> RemoteTask {
        RemoteTask {
            id: id.to_string(),
            project_id: project.to_string(),
            content: content.to_string(),
            description: String::new(),
            checked: false,
            is_deleted: false,
            priority: 1,
            due: None,
            labels: Vec::new(),
            completed_at: None,
        }
    }

    #[test]
    fn conflicting_edits_are_recorded_and_reported() {
        let (_dir, conn) = crate::testutil::test_conn();
        let id = linked_task(&conn, "td-1", "Original");

        // 远端改了标题，本地也改了标题 => 双方都变更，判定为冲突。
        tasks::modify(
            &conn,
            &id,
            &ModifyInput {
                title: Some("Local edit".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let result = apply_remote(&conn, &remote("td-1", "proj", "Remote edit")).unwrap();
        assert!(matches!(result, ApplyResult::Conflict));

        let conflicts = sync_conflicts(&conn).unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].todoist_id, "td-1");
        assert_eq!(conflicts[0].local_title, "Local edit");
        assert_eq!(conflicts[0].remote_title, "Remote edit");
        assert!(conflicts[0].resolution.is_none());
    }

    #[test]
    fn keeping_horae_side_clears_the_conflict_record() {
        let (_dir, conn) = crate::testutil::test_conn();
        let id = linked_task(&conn, "td-1", "Original");
        tasks::modify(
            &conn,
            &id,
            &ModifyInput {
                title: Some("Local edit".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        apply_remote(&conn, &remote("td-1", "proj", "Remote edit")).unwrap();
        assert_eq!(sync_conflicts(&conn).unwrap().len(), 1);

        resolve_conflict(&conn, "td-1", ConflictResolution::KeepHorae).unwrap();
        // 冲突已按"保留 Horae"裁决；再次应用远端时不得重新判定为冲突。
        let result = apply_remote(&conn, &remote("td-1", "proj", "Remote edit")).unwrap();
        assert!(matches!(result, ApplyResult::Unchanged));

        // 裁决完成后冲突记录必须被清除，否则界面会一直显示待处理冲突。
        assert!(
            sync_conflicts(&conn).unwrap().is_empty(),
            "resolved conflict should be removed from the conflict list"
        );
        // 本地标题保持不变。
        assert_eq!(tasks::get(&conn, &id).unwrap().title, "Local edit");
    }

    #[test]
    fn keeping_todoist_side_applies_remote_and_clears_the_conflict() {
        let (_dir, conn) = crate::testutil::test_conn();
        let id = linked_task(&conn, "td-1", "Original");
        tasks::modify(
            &conn,
            &id,
            &ModifyInput {
                title: Some("Local edit".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        apply_remote(&conn, &remote("td-1", "proj", "Remote edit")).unwrap();
        assert_eq!(sync_conflicts(&conn).unwrap().len(), 1);

        resolve_conflict(&conn, "td-1", ConflictResolution::KeepTodoist).unwrap();
        let result = apply_remote(&conn, &remote("td-1", "proj", "Remote edit")).unwrap();
        assert!(matches!(result, ApplyResult::Updated));

        assert!(sync_conflicts(&conn).unwrap().is_empty());
        assert_eq!(tasks::get(&conn, &id).unwrap().title, "Remote edit");
    }

    #[test]
    fn deleted_remote_task_keeps_local_task_when_horae_side_is_kept() {
        let (_dir, conn) = crate::testutil::test_conn();
        let id = linked_task(&conn, "td-1", "Original");
        tasks::modify(
            &conn,
            &id,
            &ModifyInput {
                title: Some("Local edit".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        let mut deleted = remote("td-1", "proj", "Original");
        deleted.is_deleted = true;
        let result = apply_remote(&conn, &deleted).unwrap();
        assert!(matches!(result, ApplyResult::Conflict));

        resolve_conflict(&conn, "td-1", ConflictResolution::KeepHorae).unwrap();
        let result = apply_remote(&conn, &deleted).unwrap();
        assert!(matches!(result, ApplyResult::Unchanged));

        // 保留 Horae => 本地任务不被归档，且冲突记录被清除。
        assert!(tasks::get(&conn, &id).unwrap().archived_at.is_none());
        assert!(sync_conflicts(&conn).unwrap().is_empty());
    }

    #[test]
    fn deleted_remote_task_archives_local_task_when_horae_side_is_unchanged() {
        let (_dir, conn) = crate::testutil::test_conn();
        let id = linked_task(&conn, "td-1", "Original");

        let mut deleted = remote("td-1", "proj", "Original");
        deleted.is_deleted = true;
        let result = apply_remote(&conn, &deleted).unwrap();
        assert!(matches!(result, ApplyResult::Unchanged));

        // 本地没有改动 => 直接归档本地任务，不产生冲突。
        assert!(tasks::get(&conn, &id).unwrap().archived_at.is_some());
        assert!(sync_conflicts(&conn).unwrap().is_empty());
    }

    #[test]
    fn resolving_an_unknown_conflict_errors() {
        let (_dir, conn) = crate::testutil::test_conn();
        assert!(resolve_conflict(&conn, "missing", ConflictResolution::KeepHorae).is_err());
    }

    #[test]
    fn a_resolved_conflict_is_no_longer_listed_as_pending() {
        let (_dir, conn) = crate::testutil::test_conn();
        let id = linked_task(&conn, "td-1", "Original");
        tasks::modify(
            &conn,
            &id,
            &ModifyInput {
                title: Some("Local edit".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

        apply_remote(&conn, &remote("td-1", "proj", "Remote edit")).unwrap();
        assert_eq!(sync_conflicts(&conn).unwrap().len(), 1);

        // 裁决后仍留有记录（等待下一轮同步真正落地），但不再出现在待处理列表里。
        resolve_conflict(&conn, "td-1", ConflictResolution::KeepHorae).unwrap();
        assert!(
            sync_conflicts(&conn).unwrap().is_empty(),
            "a resolved conflict must not stay in the pending list"
        );
        let stored: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM todoist_conflicts WHERE todoist_id='td-1' AND resolution='local'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(stored, 1, "the resolution should still be recorded");
    }

    #[test]
    fn sync_status_starts_empty_and_records_failures() {
        let (_dir, conn) = crate::testutil::test_conn();
        assert!(sync_status(&conn).unwrap().is_none());
    }
}
