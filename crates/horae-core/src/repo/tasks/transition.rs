//! Task mutations: capture, status transitions, scheduling, archive/purge.
//! Every state change goes through [`crate::repo::mutate`] so the mutation and
//! its audit event share one timestamp and are atomic.

use rusqlite::Connection;
use uuid::Uuid;

use crate::error::Error;
use crate::model::event;
use crate::model::task::{self, Task};
use crate::repo::log_event;
use crate::time;
use anyhow::Result;

use super::{get, has_checked_in_today, resolve_id};

/// Input for creating a task (capture).
pub struct CaptureInput {
    pub title: String,
    pub notes: String,
    pub status: task::Status,
    pub due_at: Option<i64>,
    pub tag_names: Vec<String>,
    pub rrule: Option<String>,
    pub priority: Option<String>,
    pub delegated_to: Option<String>,
    pub checklist: Vec<task::ChecklistItem>,
}

impl Default for CaptureInput {
    fn default() -> Self {
        Self {
            title: String::new(),
            notes: String::new(),
            status: task::Status::Inbox,
            due_at: None,
            tag_names: Vec::new(),
            rrule: None,
            priority: None,
            delegated_to: None,
            checklist: Vec::new(),
        }
    }
}

pub fn create_capture(conn: &Connection, input: &CaptureInput) -> Result<Task> {
    let id = Uuid::new_v4().to_string();
    let status = input.status;
    let cl_str = serde_json::to_string(&input.checklist)?;

    crate::repo::mutate(conn, |tx, now| {
        let clarified = if status != task::Status::Inbox {
            Some(now)
        } else {
            None
        };
        tx.execute(
            "INSERT INTO tasks \
             (id,title,notes,status,rrule,priority,created_at,clarified_at,due_at,updated_at,delegated_to,checklist) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
            rusqlite::params![
                id,
                input.title,
                input.notes,
                status.to_string(),
                input.rrule,
                input.priority,
                now,
                clarified,
                input.due_at,
                now,
                input.delegated_to,
                cl_str
            ],
        )?;
        let status_str = status.to_string();
        log_event(
            tx,
            &id,
            event::EV_CAPTURED,
            None,
            Some(&status_str),
            None,
            now,
        )?;
        if clarified.is_some() {
            log_event(
                tx,
                &id,
                event::EV_CLARIFIED,
                None,
                Some(&status_str),
                None,
                now,
            )?;
        }
        for tag in &input.tag_names {
            crate::repo::tags::add_tag_to_task_inner(tx, &id, tag, now)?;
        }
        Ok(())
    })?;
    get(conn, &id)
}

/// Input for modifying an existing task.
#[derive(Debug, Default, Clone)]
pub struct ModifyInput {
    pub title: Option<String>,
    pub notes: Option<String>,
    pub status: Option<task::Status>,
    pub due_at: Option<Option<i64>>,
    pub scheduled_start_at: Option<Option<i64>>,
    pub scheduled_end_at: Option<Option<i64>>,
    pub rrule: Option<Option<String>>,
    pub priority: Option<Option<String>>,
    pub delegated_to: Option<Option<String>>,
    pub add_tags: Vec<String>,
    pub remove_tags: Vec<String>,
    pub clear_tags: bool,
}

pub fn modify(conn: &Connection, id: &str, input: &ModifyInput) -> Result<Task> {
    let mut t = get(conn, id)?;
    crate::repo::mutate(conn, |tx, now| {
        if let Some(ref title) = input.title {
            if !title.is_empty() && title != &t.title {
                t.title = title.clone();
            }
        }
        if let Some(ref notes) = input.notes {
            if notes != &t.notes {
                t.notes = notes.clone();
            }
        }
        if let Some(ref delegated) = input.delegated_to {
            t.delegated_to = delegated.clone();
        }
        if let Some(new_status) = input.status {
            if new_status != t.status {
                let from = t.status;
                if from == task::Status::Inbox && t.clarified_at.is_none() {
                    t.clarified_at = Some(now);
                }
                if new_status == task::Status::Done && t.completed_at.is_none() {
                    t.completed_at = Some(now);
                }
                t.status = new_status;
                let ev = if new_status == task::Status::Done {
                    event::EV_COMPLETED
                } else {
                    event::EV_STATUS_CHANGED
                };
                let from_str = from.to_string();
                let to_str = new_status.to_string();
                log_event(tx, id, ev, Some(&from_str), Some(&to_str), None, now)?;
            }
        }
        if let Some(due_opt) = input.due_at {
            if due_opt != t.due_at {
                t.due_at = due_opt;
                log_event(tx, id, event::EV_DUE, None, None, None, now)?;
            }
        }
        let mut sched_changed = false;
        if let Some(start_opt) = input.scheduled_start_at {
            if start_opt != t.scheduled_start_at {
                t.scheduled_start_at = start_opt;
                sched_changed = true;
            }
        }
        if let Some(end_opt) = input.scheduled_end_at {
            if end_opt != t.scheduled_end_at {
                t.scheduled_end_at = end_opt;
                sched_changed = true;
            }
        }
        if let Some(ref rrule_opt) = input.rrule {
            if rrule_opt != &t.rrule {
                t.rrule = rrule_opt.clone();
                sched_changed = true;
            }
        }
        if let Some(priority_opt) = input.priority.clone() {
            if priority_opt != t.priority {
                t.priority = priority_opt;
                log_event(tx, id, event::EV_PRIORITY, None, None, None, now)?;
            }
        }
        if sched_changed {
            let meta = t
                .rrule
                .as_deref()
                .map(|r| format!("{{\"rrule\":\"{}\"}}", r));
            let status_str = t.status.to_string();
            log_event(
                tx,
                id,
                event::EV_SCHEDULED,
                None,
                Some(&status_str),
                meta.as_deref(),
                now,
            )?;
        }
        if input.clear_tags {
            let current_tags = crate::repo::tags::get_task_tags(tx, id).unwrap_or_default();
            for tag in current_tags {
                crate::repo::tags::remove_tag_from_task_inner(tx, id, &tag.name, now)?;
            }
        } else {
            for tag_name in &input.remove_tags {
                crate::repo::tags::remove_tag_from_task_inner(tx, id, tag_name, now)?;
            }
        }
        for tag_name in &input.add_tags {
            crate::repo::tags::add_tag_to_task_inner(tx, id, tag_name, now)?;
        }
        t.updated_at = now;
        tx.execute(
            "UPDATE tasks SET title=?1, notes=?2, status=?3, clarified_at=?4, completed_at=?5, \
             due_at=?6, scheduled_start_at=?7, scheduled_end_at=?8, rrule=?9, priority=?10, \
             delegated_to=?11, updated_at=?12 WHERE id=?13",
            rusqlite::params![
                t.title,
                t.notes,
                t.status.to_string(),
                t.clarified_at,
                t.completed_at,
                t.due_at,
                t.scheduled_start_at,
                t.scheduled_end_at,
                t.rrule,
                t.priority,
                t.delegated_to,
                t.updated_at,
                id
            ],
        )?;
        Ok(())
    })?;
    get(conn, id)
}

pub fn rename(conn: &Connection, id: &str, new_title: &str) -> Result<Task> {
    let now = time::now_ms();
    conn.execute(
        "UPDATE tasks SET title=?1, updated_at=?2 WHERE id=?3",
        rusqlite::params![new_title, now, id],
    )?;
    get(conn, id)
}

pub fn update_notes(conn: &Connection, id: &str, new_notes: &str) -> Result<Task> {
    let now = time::now_ms();
    conn.execute(
        "UPDATE tasks SET notes=?1, updated_at=?2 WHERE id=?3",
        rusqlite::params![new_notes, now, id],
    )?;
    get(conn, id)
}

/// Write the checklist JSON inside the mutate seam and log a `checklist` event
/// (time-datafication: every structural change leaves an audit-trail entry).
fn write_checklist(
    conn: &Connection,
    id: &str,
    checklist: &[task::ChecklistItem],
    meta: &str,
) -> Result<()> {
    let cl_str = serde_json::to_string(checklist)?;
    crate::repo::mutate(conn, |tx, now| {
        tx.execute(
            "UPDATE tasks SET checklist=?1, updated_at=?2 WHERE id=?3",
            rusqlite::params![cl_str, now, id],
        )?;
        log_event(tx, id, event::EV_CHECKLIST, None, None, Some(meta), now)?;
        Ok(())
    })?;
    Ok(())
}

/// Append a new checklist item (done=false). Returns the new item's id.
pub fn add_checklist_item(conn: &Connection, id: &str, title: &str) -> Result<Option<String>> {
    let mut task = get(conn, id)?;
    if title.trim().is_empty() {
        return Ok(None);
    }
    let item_id = Uuid::new_v4().to_string();
    task.checklist.push(task::ChecklistItem {
        id: item_id.clone(),
        title: title.to_string(),
        done: false,
    });
    write_checklist(conn, id, &task.checklist, &format!("add:{}", title))?;
    Ok(Some(item_id))
}

/// Toggle (flip `done`) a specific checklist item by id. Returns the item title if found.
pub fn toggle_checklist_item(conn: &Connection, id: &str, item_id: &str) -> Result<Option<String>> {
    let mut task = get(conn, id)?;
    let mut toggled = None;
    for item in task.checklist.iter_mut() {
        if item.id == item_id {
            item.done = !item.done;
            toggled = Some(item.title.clone());
            break;
        }
    }
    if let Some(title) = &toggled {
        write_checklist(conn, id, &task.checklist, &format!("toggle:{}", title))?;
    }
    Ok(toggled)
}

/// Quick path used by the `=` shortcut: tick the first still-undone item.
/// Returns the ticked title, or `None` when every item is already done
/// (the list is no longer auto-reset once fully checked).
pub fn toggle_next_checklist_item(conn: &Connection, id: &str) -> Result<Option<String>> {
    let task = get(conn, id)?;
    let next = task
        .checklist
        .iter()
        .find(|i| !i.done)
        .map(|i| i.id.clone());
    match next {
        Some(item_id) => toggle_checklist_item(conn, id, &item_id),
        None => Ok(None),
    }
}

/// Remove a checklist item by id. Returns the removed title if found.
pub fn delete_checklist_item(conn: &Connection, id: &str, item_id: &str) -> Result<Option<String>> {
    let mut task = get(conn, id)?;
    let before = task.checklist.len();
    let removed: Vec<String> = task
        .checklist
        .iter()
        .filter(|i| i.id == item_id)
        .map(|i| i.title.clone())
        .collect();
    task.checklist.retain(|i| i.id != item_id);
    if task.checklist.len() != before {
        write_checklist(
            conn,
            id,
            &task.checklist,
            &format!("delete:{}", removed.join(",")),
        )?;
        Ok(removed.into_iter().next())
    } else {
        Ok(None)
    }
}

/// Rename a checklist item by id. Returns the new title if found.
pub fn rename_checklist_item(
    conn: &Connection,
    id: &str,
    item_id: &str,
    new_title: &str,
) -> Result<Option<String>> {
    let mut task = get(conn, id)?;
    let mut renamed = None;
    for item in task.checklist.iter_mut() {
        if item.id == item_id {
            item.title = new_title.to_string();
            renamed = Some(new_title.to_string());
            break;
        }
    }
    if renamed.is_some() {
        write_checklist(conn, id, &task.checklist, &format!("rename:{}", new_title))?;
    }
    Ok(renamed)
}

/// Move a checklist item by id up (-1) or down (+1) within the list; no-op at edges.
pub fn move_checklist_item(conn: &Connection, id: &str, item_id: &str, dir: isize) -> Result<bool> {
    let mut task = get(conn, id)?;
    if let Some(idx) = task.checklist.iter().position(|i| i.id == item_id) {
        let new_idx = idx as isize + dir;
        if new_idx >= 0 && new_idx < task.checklist.len() as isize {
            let new_idx = new_idx as usize;
            task.checklist.swap(idx, new_idx);
            write_checklist(conn, id, &task.checklist, "move")?;
            return Ok(true);
        }
    }
    Ok(false)
}

/// Transition a task from its current status to `to_status`,
/// updating the relevant timestamp fields (time datafication).
pub fn transition(conn: &Connection, id: &str, to_status: task::Status) -> Result<Task> {
    let mut t = get(conn, id)?;
    let from = t.status;
    if from == to_status {
        return Err(Error::InvalidTransition {
            from: from.to_string(),
            to: to_status.to_string(),
        }
        .into());
    }

    // 循环习惯一天只允许打卡一次（防止把排程再次推进/重复记录打卡）。
    if to_status == task::Status::Done && t.rrule.is_some() {
        let today_start = time::local_day_bounds(0).0;
        if has_checked_in_today(conn, id, today_start)? {
            return Err(Error::AlreadyCheckedInToday(id.to_string()).into());
        }
    }

    crate::repo::mutate(conn, |tx, now| {
        if from == task::Status::Inbox && t.clarified_at.is_none() {
            t.clarified_at = Some(now);
        }

        if to_status == task::Status::Done {
            // 循环任务：把起点（排程开始时间或截止时间）推进到下一次发生，继续排程。
            // 兼容仅有 due_at + rrule（如快速录入 `~time rrule=...`）的任务。
            if let Some(rrule) = &t.rrule {
                let anchor = t.scheduled_start_at.or(t.due_at);
                if let Some(start) = anchor {
                    if let Some((next, next_end)) =
                        crate::schedule::next_window(rrule, start, t.scheduled_end_at, now)
                    {
                        log_event(
                            tx,
                            id,
                            event::EV_HABIT_COMPLETED,
                            Some(&from.to_string()),
                            Some(&task::Status::Done.to_string()),
                            None,
                            now,
                        )?;

                        if t.scheduled_start_at.is_some() {
                            t.scheduled_start_at = Some(next);
                            t.scheduled_end_at = Some(next_end);
                        }
                        if t.scheduled_start_at.is_none() && t.due_at.is_some() {
                            t.due_at = Some(next);
                        }
                        t.status = task::Status::Scheduled;
                        t.updated_at = now;

                        tx.execute(
                            "UPDATE tasks SET status=?1, clarified_at=?2, completed_at=?3, updated_at=?4, scheduled_start_at=?5, scheduled_end_at=?6, due_at=?7 WHERE id=?8",
                            rusqlite::params![t.status.to_string(), t.clarified_at, t.completed_at, t.updated_at, t.scheduled_start_at, t.scheduled_end_at, t.due_at, id],
                        )?;
                        return Ok(());
                    }
                }
            }
        }

        if to_status == task::Status::Done && t.completed_at.is_none() {
            t.completed_at = Some(now);
        }
        t.status = to_status;
        t.updated_at = now;

        tx.execute(
            "UPDATE tasks SET status=?1, clarified_at=?2, completed_at=?3, updated_at=?4, scheduled_start_at=?5, scheduled_end_at=?6 WHERE id=?7",
            rusqlite::params![t.status.to_string(), t.clarified_at, t.completed_at, t.updated_at, t.scheduled_start_at, t.scheduled_end_at, id],
        )?;
        let ev = if to_status == task::Status::Done {
            event::EV_COMPLETED
        } else {
            event::EV_STATUS_CHANGED
        };
        let from_str = from.to_string();
        let to_str = to_status.to_string();
        log_event(tx, id, ev, Some(&from_str), Some(&to_str), None, now)?;
        Ok(())
    })?;
    Ok(t)
}

/// Set a soft deadline (`due_at`) without changing the task status. Used by the
/// inbox→next planning hook so a next action keeps its status while gaining a due.
pub fn set_due(conn: &Connection, id: &str, due_ms: Option<i64>) -> Result<Task> {
    let mut t = get(conn, id)?;
    crate::repo::mutate(conn, |tx, now| {
        t.due_at = due_ms;
        t.updated_at = now;
        tx.execute(
            "UPDATE tasks SET due_at=?1, updated_at=?2 WHERE id=?3",
            rusqlite::params![t.due_at, t.updated_at, id],
        )?;
        log_event(tx, id, event::EV_DUE, None, None, None, now)?;
        Ok(())
    })?;
    get(conn, id)
}

/// Replace a task's recurrence rule, keeping its scheduled window. Used by the
/// TUI "edit rrule" action. Logs a `scheduled` event (rule change).
pub fn set_rrule(conn: &Connection, id: &str, rrule: Option<String>) -> Result<Task> {
    let mut t = get(conn, id)?;
    let from = t.status;
    crate::repo::mutate(conn, |tx, now| {
        t.rrule = rrule.clone();
        t.updated_at = now;
        tx.execute(
            "UPDATE tasks SET rrule=?1, updated_at=?2 WHERE id=?3",
            rusqlite::params![t.rrule, t.updated_at, id],
        )?;
        let meta = t
            .rrule
            .as_deref()
            .map(|r| format!("{{\"rrule\":\"{}\"}}", r));
        let to_s = t.status.to_string();
        log_event(
            tx,
            id,
            event::EV_SCHEDULED,
            Some(&from.to_string()),
            Some(&to_s),
            meta.as_deref(),
            now,
        )?;
        Ok(())
    })?;
    Ok(t)
}

/// Set a task's priority (high|medium|low|NULL), recording a `priority` event.
pub fn set_priority(conn: &Connection, id: &str, priority: Option<String>) -> Result<Task> {
    let mut t = get(conn, id)?;
    if t.priority == priority {
        return Ok(t);
    }
    crate::repo::mutate(conn, |tx, now| {
        t.priority = priority.clone();
        t.updated_at = now;
        tx.execute(
            "UPDATE tasks SET priority=?1, updated_at=?2 WHERE id=?3",
            rusqlite::params![t.priority, t.updated_at, id],
        )?;
        log_event(tx, id, event::EV_PRIORITY, None, None, None, now)?;
        Ok(())
    })?;
    get(conn, id)
}

/// Schedule a task: set planned start/end + optional recurrence, move to
/// `scheduled`, and record a `scheduled` event.
pub fn schedule(
    conn: &Connection,
    id: &str,
    start_ms: i64,
    end_ms: Option<i64>,
    rrule: Option<String>,
) -> Result<Task> {
    let mut t = get(conn, id)?;
    let from = t.status;
    crate::repo::mutate(conn, |tx, now| {
        if from == task::Status::Inbox && t.clarified_at.is_none() {
            t.clarified_at = Some(now);
        }
        t.scheduled_start_at = Some(start_ms);
        t.scheduled_end_at = end_ms;
        t.rrule = rrule.clone();
        t.status = task::Status::Scheduled;
        t.updated_at = now;
        tx.execute(
            "UPDATE tasks SET status=?1, clarified_at=?2, scheduled_start_at=?3, scheduled_end_at=?4, rrule=?5, updated_at=?6 WHERE id=?7",
            rusqlite::params![
                t.status.to_string(),
                t.clarified_at,
                t.scheduled_start_at,
                t.scheduled_end_at,
                t.rrule,
                t.updated_at,
                id
            ],
        )?;
        let meta = rrule.as_deref().map(|r| format!("{{\"rrule\":\"{}\"}}", r));
        let from_str = from.to_string();
        log_event(
            tx,
            id,
            event::EV_SCHEDULED,
            Some(&from_str),
            Some("scheduled"),
            meta.as_deref(),
            now,
        )?;
        Ok(())
    })?;
    Ok(t)
}

pub fn archive(conn: &Connection, id: &str) -> Result<Task> {
    let t = get(conn, id)?;
    let reason = if t.status == task::Status::Done {
        "completed"
    } else {
        "deleted"
    };
    crate::repo::mutate(conn, |tx, now| {
        tx.execute(
            "UPDATE tasks SET archived_at=?1, archive_reason=?2, updated_at=?3 WHERE id=?4",
            rusqlite::params![now, reason, now, id],
        )?;
        let meta = format!("{{\"reason\":\"{}\"}}", reason);
        log_event(tx, id, event::EV_ARCHIVED, None, None, Some(&meta), now)?;
        Ok(())
    })?;
    get(conn, id)
}

/// Undo a soft-delete: clear `archived_at` and record a `restored` event.
pub fn unarchive(conn: &Connection, id: &str) -> Result<Task> {
    let _ = get(conn, id)?;
    crate::repo::mutate(conn, |tx, now| {
        tx.execute(
            "UPDATE tasks SET archived_at=NULL, archive_reason=NULL, updated_at=?1 WHERE id=?2",
            rusqlite::params![now, id],
        )?;
        log_event(tx, id, event::EV_RESTORED, None, None, None, now)?;
        Ok(())
    })?;
    get(conn, id)
}

/// Permanently delete an archived task. Only archived tasks may be purged —
/// purging a live task is a destructive mistake. The `DELETE` cascades to the
/// task's `task_events`, `task_tags`, and child `tasks` rows (ON DELETE CASCADE),
/// so no event is logged for the purge itself.
pub fn purge(conn: &Connection, id: &str) -> Result<Task> {
    let id = resolve_id(conn, id)?;
    if id == super::SYSTEM_JOURNAL_ID {
        // `__journal__` 是 `horae log` 的外键锚点，删除会导致日志写入永久失败。
        return Err(Error::SystemTaskProtected(id).into());
    }
    let t = get(conn, &id)?;
    if t.archived_at.is_none() {
        return Err(Error::NotArchived(id).into());
    }
    crate::repo::mutate(conn, |tx, _now| {
        tx.execute("DELETE FROM tasks WHERE id = ?1", rusqlite::params![id])?;
        Ok(())
    })?;
    Ok(t)
}

pub fn ensure_ready_for_pomodoro(conn: &Connection, id: &str) -> Result<()> {
    let task = crate::repo::tasks::get(conn, id)?;
    if task.status != task::Status::Next && task.status != task::Status::Done {
        transition(conn, id, task::Status::Next)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::tasks::{checked_in_today, events};
    use crate::testutil::test_conn;

    fn count_rows(conn: &Connection, table: &str, task_id: &str) -> usize {
        conn.query_row(
            &format!("SELECT COUNT(*) FROM {} WHERE task_id = ?1", table),
            [task_id],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn purge_rejects_system_journal_task() {
        let (_dir, conn) = test_conn();
        // `__journal__` 由 migration 0012 创建且已归档，必须拒绝删除，
        // 否则 `horae log` 的外键写入会永久失败。
        let err = purge(&conn, super::super::SYSTEM_JOURNAL_ID).unwrap_err();
        assert!(
            format!("{}", err).contains("protected"),
            "应返回 SystemTaskProtected 错误，实际: {}",
            err
        );
        // 任务仍在
        assert!(get(&conn, super::super::SYSTEM_JOURNAL_ID).is_ok());
    }

    #[test]
    fn purge_archived_deletes_task_and_cascades() {
        let (_dir, conn) = test_conn();
        let t = create_capture(
            &conn,
            &CaptureInput {
                title: "purge-me".into(),
                status: task::Status::Next,
                tag_names: vec!["home".into()],
                ..Default::default()
            },
        )
        .unwrap();
        archive(&conn, &t.id).unwrap();

        assert!(count_rows(&conn, "task_events", &t.id) > 0, "有事件记录");
        assert_eq!(count_rows(&conn, "task_tags", &t.id), 1, "有标签关联");

        let purged = purge(&conn, &t.id).unwrap();
        assert_eq!(purged.id, t.id);
        assert!(get(&conn, &t.id).is_err(), "任务已被永久删除");
        assert_eq!(count_rows(&conn, "task_events", &t.id), 0, "事件级联删除");
        assert_eq!(count_rows(&conn, "task_tags", &t.id), 0, "标签级联删除");
    }

    #[test]
    fn purge_non_archived_fails_and_leaves_task() {
        let (_dir, conn) = test_conn();
        let t = create_capture(
            &conn,
            &CaptureInput {
                title: "live-task".into(),
                ..Default::default()
            },
        )
        .unwrap();

        let err = purge(&conn, &t.id).unwrap_err();
        assert!(
            err.to_string().contains("not archived"),
            "非归档任务应被拒绝: {}",
            err
        );
        assert!(get(&conn, &t.id).is_ok(), "任务仍在，未被删除");
    }

    #[test]
    fn checked_in_today_detects_habit_completion() {
        let (_dir, conn) = test_conn();
        let today_start = time::local_day_bounds(0).0;
        let day = 24 * 3600 * 1000i64;

        // 循环任务：排程（带 rrule）+ Done 会推进锚点并记录 habit_completed
        let habit = create_capture(
            &conn,
            &CaptureInput {
                title: "晨跑".into(),
                status: task::Status::Scheduled,
                ..Default::default()
            },
        )
        .unwrap();
        schedule(
            &conn,
            &habit.id,
            time::now_ms(),
            None,
            Some("FREQ=DAILY".into()),
        )
        .unwrap();
        assert!(
            checked_in_today(&conn, today_start).unwrap().is_empty(),
            "未打卡时集合为空"
        );
        transition(&conn, &habit.id, task::Status::Done).unwrap();

        let checked = checked_in_today(&conn, today_start).unwrap();
        assert_eq!(checked, vec![habit.id.clone()], "今日打卡后被识别");

        // 非循环任务完成不会误判为打卡
        let plain = create_capture(
            &conn,
            &CaptureInput {
                title: "一次性".into(),
                status: task::Status::Next,
                ..Default::default()
            },
        )
        .unwrap();
        transition(&conn, &plain.id, task::Status::Done).unwrap();
        assert_eq!(
            checked_in_today(&conn, today_start).unwrap(),
            vec![habit.id.clone()],
            "非循环任务不产生 habit_completed"
        );

        // 过去的事件不误判为今日打卡
        conn.execute(
            "UPDATE task_events SET at = at - ?1 WHERE event_type = ?2",
            rusqlite::params![day, event::EV_HABIT_COMPLETED],
        )
        .unwrap();
        assert!(
            checked_in_today(&conn, today_start).unwrap().is_empty(),
            "昨日打卡不计入今日"
        );
    }

    #[test]
    fn missed_recurring_slot_counts_as_overdue() {
        let (_dir, conn) = test_conn();
        let now = time::now_ms();
        // 今天的 slot（09:00 已过）未打卡 → effective_due 应等于该错过 slot 而非下次
        let anchor = now - 2 * 3600 * 1000i64;
        let t = create_capture(
            &conn,
            &CaptureInput {
                title: "每日习惯".into(),
                status: task::Status::Scheduled,
                due_at: Some(anchor),
                rrule: Some("FREQ=DAILY".into()),
                tag_names: vec![],
                ..Default::default()
            },
        )
        .unwrap();
        let eff = crate::schedule::effective_due(&t);
        assert_eq!(eff, Some(anchor), "错过 slot 即逾期，返回该 slot");
    }

    #[test]
    fn habit_rejects_second_checkin_same_day() {
        let (_dir, conn) = test_conn();

        let habit = create_capture(
            &conn,
            &CaptureInput {
                title: "晨跑".into(),
                status: task::Status::Scheduled,
                ..Default::default()
            },
        )
        .unwrap();
        schedule(
            &conn,
            &habit.id,
            time::now_ms(),
            None,
            Some("FREQ=DAILY".into()),
        )
        .unwrap();

        transition(&conn, &habit.id, task::Status::Done).unwrap();
        assert!(
            checked_in_today(&conn, time::local_day_bounds(0).0)
                .unwrap()
                .contains(&habit.id),
            "第一次打卡被记录"
        );

        let err = transition(&conn, &habit.id, task::Status::Done).unwrap_err();
        assert!(
            matches!(
                err.downcast_ref::<Error>(),
                Some(Error::AlreadyCheckedInToday(_))
            ),
            "同日第二次打卡应被拒绝，实际: {err}"
        );

        let habit_events = events(&conn, &habit.id)
            .unwrap()
            .iter()
            .filter(|e| e.event_type == event::EV_HABIT_COMPLETED)
            .count();
        assert_eq!(habit_events, 1, "不重复记录打卡事件");
    }

    #[test]
    fn checklist_add_toggle_delete_rename_move_and_audit() {
        let (_dir, conn) = test_conn();
        let t = create_capture(
            &conn,
            &CaptureInput {
                title: "task".into(),
                ..Default::default()
            },
        )
        .unwrap();

        let a = add_checklist_item(&conn, &t.id, "step A").unwrap().unwrap();
        let b = add_checklist_item(&conn, &t.id, "step B").unwrap().unwrap();
        let c = add_checklist_item(&conn, &t.id, "step C").unwrap().unwrap();
        assert_eq!(get(&conn, &t.id).unwrap().checklist.len(), 3);

        // toggle a specific item
        assert_eq!(
            toggle_checklist_item(&conn, &t.id, &a).unwrap(),
            Some("step A".to_string())
        );
        assert!(get(&conn, &t.id).unwrap().checklist[0].done);

        // toggle_next ticks the first still-undone item (B), then C
        assert_eq!(
            toggle_next_checklist_item(&conn, &t.id).unwrap(),
            Some("step B".to_string())
        );
        assert_eq!(
            toggle_next_checklist_item(&conn, &t.id).unwrap(),
            Some("step C".to_string())
        );
        // all done -> no auto-reset, returns None
        assert_eq!(toggle_next_checklist_item(&conn, &t.id).unwrap(), None);
        assert!(
            get(&conn, &t.id).unwrap().checklist.iter().all(|i| i.done),
            "全勾选后清单不应被重置"
        );

        // rename B
        assert_eq!(
            rename_checklist_item(&conn, &t.id, &b, "step B2").unwrap(),
            Some("step B2".to_string())
        );
        assert!(get(&conn, &t.id)
            .unwrap()
            .checklist
            .iter()
            .any(|i| i.title == "step B2"));

        // move A down (swap with B2)
        assert!(move_checklist_item(&conn, &t.id, &a, 1).unwrap());
        assert_eq!(get(&conn, &t.id).unwrap().checklist[1].id, a);

        // delete C
        assert_eq!(
            delete_checklist_item(&conn, &t.id, &c).unwrap(),
            Some("step C".to_string())
        );
        assert_eq!(get(&conn, &t.id).unwrap().checklist.len(), 2);

        // every structural change logged as a checklist event
        let ev_types = events(&conn, &t.id)
            .unwrap()
            .iter()
            .map(|e| e.event_type.clone())
            .collect::<Vec<_>>();
        assert!(
            ev_types.iter().any(|e| e == event::EV_CHECKLIST),
            "检查单变更应记录 EV_CHECKLIST 审计事件"
        );
    }

    #[test]
    fn checklist_move_at_edges_is_noop() {
        let (_dir, conn) = test_conn();
        let t = create_capture(
            &conn,
            &CaptureInput {
                title: "task".into(),
                ..Default::default()
            },
        )
        .unwrap();
        let a = add_checklist_item(&conn, &t.id, "only").unwrap().unwrap();
        assert!(!move_checklist_item(&conn, &t.id, &a, 1).unwrap());
        assert!(!move_checklist_item(&conn, &t.id, &a, -1).unwrap());
    }

    #[test]
    fn modify_updates_fields_and_audits_events() {
        let (_dir, conn) = test_conn();
        let t = create_capture(
            &conn,
            &CaptureInput {
                title: "initial title".into(),
                notes: "initial notes".into(),
                status: task::Status::Inbox,
                tag_names: vec!["tag1".into(), "tag2".into(), "p1".into()],
                due_at: Some(1000),
                ..Default::default()
            },
        )
        .unwrap();

        // 1. Modify title, notes, status, due, tags
        let modified = modify(
            &conn,
            &t.id,
            &ModifyInput {
                title: Some("updated title".into()),
                notes: Some("updated notes".into()),
                status: Some(task::Status::Next),
                due_at: Some(Some(2000)),
                scheduled_start_at: Some(Some(3000)),
                rrule: Some(Some("FREQ=DAILY".into())),
                add_tags: vec!["tag3".into(), "p2".into()],
                remove_tags: vec!["tag1".into(), "p1".into()],
                clear_tags: false,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(modified.title, "updated title");
        assert_eq!(modified.notes, "updated notes");
        assert_eq!(modified.status, task::Status::Next);
        assert!(modified.clarified_at.is_some());
        assert_eq!(modified.due_at, Some(2000));
        assert_eq!(modified.scheduled_start_at, Some(3000));
        assert_eq!(modified.rrule.as_deref(), Some("FREQ=DAILY"));

        let tags: Vec<String> = crate::repo::tags::get_task_tags(&conn, &t.id)
            .unwrap()
            .into_iter()
            .map(|tg| tg.name)
            .collect();
        assert!(tags.contains(&"tag2".to_string()));
        assert!(tags.contains(&"tag3".to_string()));
        assert!(tags.contains(&"p2".to_string()));
        assert!(!tags.contains(&"tag1".to_string()));
        assert!(!tags.contains(&"p1".to_string()));

        // 2. Clear due and schedule and all tags
        let cleared = modify(
            &conn,
            &t.id,
            &ModifyInput {
                due_at: Some(None),
                scheduled_start_at: Some(None),
                scheduled_end_at: Some(None),
                rrule: Some(None),
                clear_tags: true,
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(cleared.due_at, None);
        assert_eq!(cleared.scheduled_start_at, None);
        assert_eq!(cleared.scheduled_end_at, None);
        assert_eq!(cleared.rrule, None);
        let tags_after = crate::repo::tags::get_task_tags(&conn, &t.id).unwrap();
        assert!(tags_after.is_empty());

        let evs = events(&conn, &t.id).unwrap();
        assert!(evs.iter().any(|e| e.event_type == event::EV_STATUS_CHANGED));
        assert!(evs.iter().any(|e| e.event_type == event::EV_DUE));
        assert!(evs.iter().any(|e| e.event_type == event::EV_SCHEDULED));
        assert!(evs.iter().any(|e| e.event_type == event::EV_TAG_ADDED));
        assert!(evs.iter().any(|e| e.event_type == event::EV_TAG_REMOVED));
    }
}
