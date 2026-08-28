//! Read-only task queries: fetching, listing, counting, stale detection, and
//! the audit timeline. No writes happen here — see [`super::transition`].

use rusqlite::Connection;

use crate::error::Error;
use crate::model::event;
use crate::model::task::{self, Task};
use crate::repo::tasks::{row_to_task, TASK_COLUMNS};
use anyhow::Result;

pub fn get(conn: &Connection, id: &str) -> Result<Task> {
    let mut stmt = conn.prepare(&format!("SELECT {} FROM tasks WHERE id = ?1", TASK_COLUMNS))?;
    let mut rows = stmt.query_map([id], row_to_task)?;
    rows.next()
        .transpose()?
        .ok_or_else(|| Error::TaskNotFound(id.to_string()).into())
}

/// Resolve a task reference to its full id: exact match, else a unique id
/// prefix (like git), else a unique exact title among non-archived tasks,
/// else `TaskNotFound`.
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
        [] => resolve_by_title(conn, key),
        _ => anyhow::bail!("ambiguous id prefix: {}", key),
    }
}

/// Fallback of `resolve_id`: a unique exact title among visible (non-archived)
/// tasks resolves to its id. Archived tasks are only addressable by id.
fn resolve_by_title(conn: &Connection, key: &str) -> Result<String> {
    let mut stmt =
        conn.prepare("SELECT id FROM tasks WHERE title = ?1 AND archived_at IS NULL ORDER BY id")?;
    let rows = stmt.query_map([key], |r| r.get::<usize, String>(0))?;
    let ids: Vec<String> = rows.collect::<rusqlite::Result<_>>()?;
    match ids.as_slice() {
        [id] => Ok(id.clone()),
        [] => Err(Error::TaskNotFound(key.to_string()).into()),
        _ => anyhow::bail!("ambiguous title: {key}"),
    }
}

/// Count of archived (soft-deleted) tasks, for the guide sidebar badge.
pub fn count_archived(conn: &Connection) -> Result<usize> {
    let c: usize = conn.query_row(
        // 与 list_archived/filter_where 一致：排除系统内置任务。
        "SELECT COUNT(*) FROM tasks WHERE archived_at IS NOT NULL AND id != '__journal__'",
        [],
        |r| r.get(0),
    )?;
    Ok(c)
}

/// Tasks whose `due_at` falls in the inclusive `[start_ms, end_ms]` window.
/// Lightweight query returning only the columns the due-notification check needs,
/// instead of scanning every task row on each tick.
///
/// 预留给将来的快速到期扫描；当前通知引擎经 `tasks::list` +
/// `effective_due` 取数（循环任务需要重排），此查询暂无调用方。
#[expect(dead_code)]
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
        "SELECT {} FROM tasks WHERE archived_at IS NOT NULL AND id != '__journal__' \
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
    let mut sql = String::from(" AND id != '__journal__'");
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
        sql.push_str(" AND (title LIKE ? ESCAPE '\\' OR notes LIKE ? ESCAPE '\\')");
        let escaped = q
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let like_q = format!("%{}%", escaped);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::repo::tasks::create_capture;
    use crate::repo::tasks::CaptureInput;
    use crate::testutil::test_conn;

    #[test]
    fn count_archived_excludes_system_journal() {
        let (_dir, conn) = test_conn();
        // migration 0012 插入的 __journal__ 是归档态，但它是系统内置任务，
        // 不应计入归档徽标（与 list_archived 的排除逻辑保持一致）。
        assert_eq!(
            count_archived(&conn).unwrap(),
            0,
            "空库归档数应为 0（不含系统 journal）"
        );

        let t = crate::repo::tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "to-archive".into(),
                ..Default::default()
            },
        )
        .unwrap();
        crate::repo::tasks::archive(&conn, &t.id).unwrap();
        assert_eq!(count_archived(&conn).unwrap(), 1);

        // 归档视图同样只含真实任务
        assert_eq!(list_archived(&conn).unwrap().len(), 1);
    }

    fn mk_task(conn: &Connection, title: &str) -> crate::model::task::Task {
        create_capture(
            conn,
            &CaptureInput {
                title: title.into(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn resolve_id_falls_back_to_unique_title() {
        let (_dir, conn) = test_conn();
        let t = mk_task(&conn, "写周报");

        assert_eq!(resolve_id(&conn, "写周报").unwrap(), t.id);
        assert!(resolve_id(&conn, "不存在").is_err());
    }

    #[test]
    fn resolve_id_rejects_ambiguous_title() {
        let (_dir, conn) = test_conn();
        mk_task(&conn, "重复");
        mk_task(&conn, "重复");
        let err = resolve_id(&conn, "重复").unwrap_err();
        assert!(err.to_string().contains("ambiguous title"), "{}", err);
    }

    #[test]
    fn resolve_id_by_title_ignores_archived_tasks() {
        let (_dir, conn) = test_conn();
        let t = mk_task(&conn, "已归档的标题");
        crate::repo::tasks::archive(&conn, &t.id).unwrap();

        // 归档任务不可按标题寻址，只能按 id
        assert!(resolve_id(&conn, "已归档的标题").is_err());
        assert_eq!(resolve_id(&conn, &t.id).unwrap(), t.id);

        // 同名新任务出现后按标题解析到可见的那个
        let t2 = mk_task(&conn, "已归档的标题");
        assert_eq!(resolve_id(&conn, "已归档的标题").unwrap(), t2.id);
    }

    #[test]
    fn resolve_id_exact_id_beats_title_of_other_task() {
        let (_dir, conn) = test_conn();
        let a = mk_task(&conn, "A");
        mk_task(&conn, &a.id); // 另一个任务标题恰好是 A 的完整 id

        // id 精确匹配优先于标题回退（git 语义）
        assert_eq!(resolve_id(&conn, &a.id).unwrap(), a.id);
    }
}
