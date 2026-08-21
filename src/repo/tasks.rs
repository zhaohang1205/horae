use rusqlite::Connection;
use uuid::Uuid;

use crate::error::Error;
use crate::model::event;
use crate::model::task::{self, Task};
use crate::repo::log_event;
use crate::time;
use anyhow::Result;

/// Columns for the `tasks` table, shared by every row-mapping query.
const TASK_COLUMNS: &str = "id,title,notes,status,rrule,created_at,clarified_at,\
        due_at,scheduled_start_at,scheduled_end_at,started_at,completed_at,archived_at,updated_at,\
        delegated_to,checklist,archive_reason";

/// Input for creating a task (capture).
pub struct CaptureInput {
    pub title: String,
    pub status: task::Status,
    pub due_at: Option<i64>,
    pub tag_names: Vec<String>,
    pub rrule: Option<String>,
    pub delegated_to: Option<String>,
    pub checklist: Vec<task::ChecklistItem>,
}

impl Default for CaptureInput {
    fn default() -> Self {
        Self {
            title: String::new(),
            status: task::Status::Inbox,
            due_at: None,
            tag_names: Vec::new(),
            rrule: None,
            delegated_to: None,
            checklist: Vec::new(),
        }
    }
}

pub fn get(conn: &Connection, id: &str) -> Result<Task> {
    let mut stmt = conn.prepare(&format!("SELECT {} FROM tasks WHERE id = ?1", TASK_COLUMNS))?;
    let mut rows = stmt.query_map([id], row_to_task)?;
    rows.next()
        .transpose()?
        .ok_or_else(|| Error::TaskNotFound(id.to_string()).into())
}

pub fn create_capture(conn: &Connection, input: &CaptureInput) -> Result<Task> {
    let id = Uuid::new_v4().to_string();
    let status = input.status;
    let cl_str = serde_json::to_string(&input.checklist).unwrap_or_else(|_| "[]".to_string());

    crate::repo::mutate(conn, |tx, now| {
        let clarified = if status != task::Status::Inbox {
            Some(now)
        } else {
            None
        };
        tx.execute(
            "INSERT INTO tasks \
             (id,title,notes,status,rrule,created_at,clarified_at,due_at,updated_at,delegated_to,checklist) \
             VALUES (?1,?2,'',?3,?4,?5,?6,?7,?8,?9,?10)",
            rusqlite::params![
                id,
                input.title,
                status.to_string(),
                input.rrule,
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

/// Resolve a task reference to its full id: exact match, else a unique id
/// prefix (like git), else `TaskNotFound`.
pub fn resolve_id(conn: &Connection, key: &str) -> Result<String> {
    // 一次查询同时命中精确匹配与 id 前缀（精确匹配排在最前），避免先 `get` 再前缀查询两次往返。
    let mut stmt = conn.prepare(
        "SELECT id FROM tasks WHERE id = ?1 OR id LIKE ?1 || '%' \
         ORDER BY (id = ?1) DESC LIMIT 2",
    )?;
    let rows = stmt.query_map([key], |r| r.get::<usize, String>(0))?;
    let ids: Vec<String> = rows.collect::<rusqlite::Result<_>>()?;
    match ids.as_slice() {
        // 精确匹配优先：即使同时存在多个前缀命中，精确命中仍然胜出（git 语义）。
        [first, ..] if first.as_str() == key => Ok(key.to_string()),
        [first] => Ok(first.clone()),
        [] => Err(Error::TaskNotFound(key.to_string()).into()),
        _ => anyhow::bail!("ambiguous id prefix: {}", key),
    }
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

pub fn update_checklist(
    conn: &Connection,
    id: &str,
    checklist: &Vec<task::ChecklistItem>,
) -> Result<Task> {
    let now = time::now_ms();
    let cl_str = serde_json::to_string(checklist).unwrap_or_else(|_| "[]".to_string());
    conn.execute(
        "UPDATE tasks SET checklist=?1, updated_at=?2 WHERE id=?3",
        rusqlite::params![cl_str, now, id],
    )?;
    get(conn, id)
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
        if checked_in_today(conn, today_start)?
            .iter()
            .any(|tid| tid == id)
        {
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
                        crate::schedule::next_window(rrule, start, t.scheduled_end_at)
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
                            "UPDATE tasks SET status=?1, clarified_at=?2, completed_at=?3, updated_at=?4, started_at=?5, scheduled_start_at=?6, scheduled_end_at=?7, due_at=?8 WHERE id=?9",
                            rusqlite::params![t.status.to_string(), t.clarified_at, t.completed_at, t.updated_at, t.started_at, t.scheduled_start_at, t.scheduled_end_at, t.due_at, id],
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
            "UPDATE tasks SET status=?1, clarified_at=?2, completed_at=?3, updated_at=?4, started_at=?5, scheduled_start_at=?6, scheduled_end_at=?7 WHERE id=?8",
            rusqlite::params![t.status.to_string(), t.clarified_at, t.completed_at, t.updated_at, t.started_at, t.scheduled_start_at, t.scheduled_end_at, id],
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
        log_event(tx, id, event::EV_ARCHIVED, None, None, Some(reason), now)?;
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

/// Count of archived (soft-deleted) tasks, for the guide sidebar badge.
pub fn count_archived(conn: &Connection) -> Result<usize> {
    let c: usize = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE archived_at IS NOT NULL",
        [],
        |r| r.get(0),
    )?;
    Ok(c)
}

/// The tag name backing the 金句 (Quotes) view.
pub const QUOTE_TAG: &str = "quote";

/// WHERE fragment selecting tasks carrying the `quote` tag (not archived).
/// Optional extra tag + search query narrow it further, mirroring `filter_where`.
fn quote_filter(
    query: Option<&str>,
    extra_tag: Option<&str>,
) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
    let mut sql = String::from(
        " AND id IN (SELECT task_id FROM task_tags tt JOIN tags g ON g.id=tt.tag_id WHERE g.name = 'quote')",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(t) = extra_tag {
        sql.push_str(
            " AND id IN (SELECT task_id FROM task_tags tt JOIN tags g ON g.id=tt.tag_id WHERE g.name = ?)",
        );
        params.push(Box::new(t.to_string()));
    }
    if let Some(q) = query {
        sql.push_str(" AND (title LIKE ? OR notes LIKE ?)");
        let like_q = format!("%{}%", q);
        params.push(Box::new(like_q.clone()));
        params.push(Box::new(like_q));
    }
    (sql, params)
}

/// List tasks tagged `quote` (金句视图), newest first — a feed-like notebook.
/// Quotes reuse the task model: status is `reference`, so they never surface in
/// the inbox / action workflow.
pub fn list_quotes(
    conn: &Connection,
    query: Option<&str>,
    extra_tag: Option<&str>,
) -> Result<Vec<Task>> {
    let (where_sql, params) = quote_filter(query, extra_tag);
    let sql = format!(
        "SELECT {} FROM tasks WHERE archived_at IS NULL{where_sql} \
         ORDER BY created_at DESC",
        TASK_COLUMNS
    );
    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(param_refs.as_slice(), row_to_task)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Count of quote-tagged (unarchived) tasks, for the guide sidebar badge.
pub fn count_quotes(conn: &Connection, query: Option<&str>) -> Result<usize> {
    let (where_sql, params) = quote_filter(query, None);
    let sql = format!("SELECT COUNT(*) FROM tasks WHERE archived_at IS NULL{where_sql}");
    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let c: usize = stmt.query_row(param_refs.as_slice(), |r| r.get(0))?;
    Ok(c)
}

/// Ids of all unarchived tasks tagged `quote` (any status). Used by the
/// Reference view to exclude quotes so quotes live only in the Quotes view.
pub fn quote_task_ids(conn: &Connection) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT tt.task_id FROM task_tags tt \
         JOIN tags g ON g.id = tt.tag_id \
         JOIN tasks t ON t.id = tt.task_id \
         WHERE g.name = ?1 AND t.archived_at IS NULL",
    )?;
    let rows = stmt.query_map([QUOTE_TAG], |r| r.get::<usize, String>(0))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Count of quote-tagged (unarchived) tasks in a specific status. Precise
/// counterpart of [`count_quotes`] for the Reference badge subtraction: only
/// quotes that are actually `reference` live in the Reference view.
pub fn count_quotes_in_status(
    conn: &Connection,
    status: &str,
    query: Option<&str>,
) -> Result<usize> {
    let mut sql = String::from(
        "SELECT COUNT(*) FROM tasks WHERE archived_at IS NULL AND status = ?1 \
         AND id IN (SELECT task_id FROM task_tags tt JOIN tags g ON g.id=tt.tag_id WHERE g.name = 'quote')",
    );
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(status.to_string())];
    if let Some(q) = query {
        sql.push_str(" AND (title LIKE ? OR notes LIKE ?)");
        let like_q = format!("%{}%", q);
        params.push(Box::new(like_q.clone()));
        params.push(Box::new(like_q));
    }
    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let c: usize = stmt.query_row(param_refs.as_slice(), |r| r.get(0))?;
    Ok(c)
}

/// Tasks whose `due_at` falls in the inclusive `[start_ms, end_ms]` window.
/// Lightweight query returning only the columns the due-notification check needs,
/// instead of scanning every task row on each tick.
pub fn due_in_range(
    conn: &Connection,
    start_ms: i64,
    end_ms: i64,
) -> Result<Vec<(String, String, Option<i64>)>> {
    let mut stmt = conn.prepare(
        "SELECT id, title, due_at FROM tasks \
         WHERE archived_at IS NULL AND due_at BETWEEN ?1 AND ?2",
    )?;
    let rows = stmt.query_map(rusqlite::params![start_ms, end_ms], |r| {
        Ok((r.get(0)?, r.get(1)?, r.get(2)?))
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// List only archived (soft-deleted) tasks, for the restore UI.
pub fn list_archived(conn: &Connection) -> Result<Vec<Task>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {} FROM tasks WHERE archived_at IS NOT NULL \
         ORDER BY archived_at DESC",
        TASK_COLUMNS
    ))?;
    let rows = stmt.query_map([], row_to_task)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub struct ListFilter {
    pub status: Option<task::Status>,
    pub tags: Vec<String>,
    pub query: Option<String>,
    pub review_stale: bool,
}

pub fn list(conn: &Connection, f: &ListFilter) -> Result<Vec<Task>> {
    let (where_sql, params) = filter_where(f);
    let sql = format!(
        "SELECT {} FROM tasks WHERE archived_at IS NULL{where_sql} \
         ORDER BY (scheduled_start_at IS NOT NULL) DESC, scheduled_start_at ASC, due_at ASC, created_at ASC",
        TASK_COLUMNS
    );
    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(param_refs.as_slice(), row_to_task)?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 未归档任务按状态分组计数（`status 字符串 -> 数量`），一次 GROUP BY 取代逐状态
/// COUNT。仅按搜索条件过滤，供侧栏徽标复用。
pub fn count_by_status(
    conn: &Connection,
    query: Option<&str>,
) -> Result<std::collections::HashMap<String, usize>> {
    let f = ListFilter {
        status: None,
        tags: vec![],
        query: query.map(String::from),
        review_stale: false,
    };
    let (where_sql, params) = filter_where(&f);
    let sql = format!(
        "SELECT status, COUNT(*) FROM tasks WHERE archived_at IS NULL{where_sql} GROUP BY status"
    );
    let mut stmt = conn.prepare(&sql)?;
    let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();
    let rows = stmt.query_map(param_refs.as_slice(), |r| {
        Ok((r.get::<_, String>(0)?, r.get::<_, usize>(1)?))
    })?;
    Ok(rows.collect::<rusqlite::Result<_>>()?)
}

/// Build the `WHERE` fragment (including its leading ` AND …`) and the bound
/// parameters shared by `list` and `count`.
fn filter_where(f: &ListFilter) -> (String, Vec<Box<dyn rusqlite::ToSql>>) {
    let mut sql = String::new();
    let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(s) = &f.status {
        sql.push_str(" AND status = ?");
        params.push(Box::new(s.to_string()));
    }
    if f.review_stale {
        let seven_days_ago = crate::time::now_ms() - 7 * 24 * 3600 * 1000;
        sql.push_str(" AND (updated_at < ? OR updated_at IS NULL)");
        params.push(Box::new(seven_days_ago));
    }
    for tag in &f.tags {
        sql.push_str(
            " AND id IN (SELECT task_id FROM task_tags tt JOIN tags g ON g.id=tt.tag_id WHERE g.name = ?)",
        );
        params.push(Box::new(tag.clone()));
    }
    if let Some(q) = &f.query {
        sql.push_str(" AND (title LIKE ? OR notes LIKE ?)");
        let like_q = format!("%{}%", q);
        params.push(Box::new(like_q.clone()));
        params.push(Box::new(like_q));
    }
    (sql, params)
}

/// Inbox 中在 `before_ms` 之前收集、至今未澄清的任务 (id, title)，
/// 用于心智维护的收件箱滞留提醒。按收集时间升序（最旧的在前）。
pub fn list_stale_inbox(conn: &Connection, before_ms: i64) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, title FROM tasks \
         WHERE archived_at IS NULL AND status = 'inbox' AND created_at < ?1 \
         ORDER BY created_at ASC",
    )?;
    let rows = stmt.query_map([before_ms], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// Waiting 中 `before_ms` 以来未再动过的任务 (id, title)，
/// 用于心智维护的等待老化提醒。按最后变动时间升序。
pub fn list_stale_waiting(conn: &Connection, before_ms: i64) -> Result<Vec<(String, String)>> {
    let mut stmt = conn.prepare(
        "SELECT id, title FROM tasks \
         WHERE archived_at IS NULL AND status = 'waiting' AND updated_at < ?1 \
         ORDER BY updated_at ASC",
    )?;
    let rows = stmt.query_map([before_ms], |r| Ok((r.get(0)?, r.get(1)?)))?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

/// 自 `since_ms` 以来完成的任务数（含随后归档的已完成任务）。
pub fn count_completed_since(conn: &Connection, since_ms: i64) -> Result<usize> {
    let c: usize = conn.query_row(
        "SELECT COUNT(*) FROM tasks WHERE completed_at IS NOT NULL AND completed_at >= ?1",
        [since_ms],
        |r| r.get(0),
    )?;
    Ok(c)
}

/// 今日已打卡的循环任务 id：存在 `habit_completed` 事件且时间 >= `since_ms`。
/// 循环任务完成时不会置 `completed_at`（重新排程而非结束），故只能靠事件判断。
pub fn checked_in_today(conn: &Connection, since_ms: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        "SELECT DISTINCT task_id FROM task_events \
         WHERE event_type = ?1 AND at >= ?2",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![event::EV_HABIT_COMPLETED, since_ms],
        |r| r.get::<_, String>(0),
    )?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

pub fn events(conn: &Connection, task_id: &str) -> Result<Vec<event::TaskEvent>> {
    let mut stmt = conn.prepare(
        "SELECT id,task_id,event_type,from_status,to_status,at,meta \
         FROM task_events WHERE task_id = ?1 ORDER BY at ASC",
    )?;
    let rows = stmt.query_map([task_id], |r| {
        Ok(event::TaskEvent {
            id: r.get(0)?,
            task_id: r.get(1)?,
            event_type: r.get(2)?,
            from_status: r.get(3)?,
            to_status: r.get(4)?,
            at: r.get(5)?,
            meta: r.get(6)?,
        })
    })?;
    Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
}

fn row_to_task(r: &rusqlite::Row) -> rusqlite::Result<Task> {
    let status_str: String = r.get(3)?;
    let delegated_to: Option<String> = r.get(14)?;
    let cl_str: String = r.get(15)?;

    Ok(Task {
        id: r.get(0)?,
        title: r.get(1)?,
        notes: r.get(2)?,
        status: status_str
            .parse()
            .unwrap_or(crate::model::task::Status::Inbox),
        rrule: r.get(4)?,
        created_at: r.get(5)?,
        clarified_at: r.get(6)?,
        due_at: r.get(7)?,
        scheduled_start_at: r.get(8)?,
        scheduled_end_at: r.get(9)?,
        started_at: r.get(10)?,
        completed_at: r.get(11)?,
        archived_at: r.get(12)?,
        updated_at: r.get(13)?,
        delegated_to,
        checklist: serde_json::from_str(&cl_str).unwrap_or_default(),
        archive_reason: r.get(16)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn list_quotes_filters_and_orders_newest_first() {
        let (_dir, conn) = test_conn();
        // 普通收件箱任务（不带 quote）
        create_capture(
            &conn,
            &CaptureInput {
                title: "普通任务".into(),
                status: task::Status::Inbox,
                ..Default::default()
            },
        )
        .unwrap();
        // 两条金句（@quote + reference）
        let q1 = create_capture(
            &conn,
            &CaptureInput {
                title: "金句一".into(),
                status: task::Status::Reference,
                tag_names: vec![QUOTE_TAG.to_string()],
                ..Default::default()
            },
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let q2 = create_capture(
            &conn,
            &CaptureInput {
                title: "金句二".into(),
                status: task::Status::Reference,
                tag_names: vec![QUOTE_TAG.to_string()],
                ..Default::default()
            },
        )
        .unwrap();

        assert_eq!(count_quotes(&conn, None).unwrap(), 2);
        let all = list_quotes(&conn, None, None).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, q2.id, "新的在前");
        assert_eq!(all[1].id, q1.id, "旧的在后");

        // quote_task_ids / count_quotes_in_status（供 Reference 视图排除金句）
        let mut ids = quote_task_ids(&conn).unwrap();
        ids.sort();
        let mut expect = vec![q1.id.clone(), q2.id.clone()];
        expect.sort();
        assert_eq!(ids, expect);
        assert_eq!(count_quotes_in_status(&conn, "reference", None).unwrap(), 2);
        assert_eq!(count_quotes_in_status(&conn, "next", None).unwrap(), 0);

        // 搜索过滤
        let found = list_quotes(&conn, Some("金句二"), None).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, q2.id);

        // 附加标签过滤
        let none = list_quotes(&conn, None, Some("不存在的标签")).unwrap();
        assert!(none.is_empty());

        // 归档的金句不计入
        archive(&conn, &q2.id).unwrap();
        assert_eq!(count_quotes(&conn, None).unwrap(), 1);
        let all = list_quotes(&conn, None, None).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, q1.id);
        assert_eq!(quote_task_ids(&conn).unwrap(), vec![q1.id.clone()]);
        assert_eq!(
            count_quotes_in_status(&conn, "reference", None).unwrap(),
            1,
            "归档的金句不参与 Reference 徽标减法"
        );
    }
}
