//! 金句 (Quotes) — the optional Quotes feature. A quote is a task tagged
//! `@quote` (system tag) with status `reference`, so it never surfaces in the
//! inbox / action workflow. This module owns the `@quote` tag constant, the
//! quote queries, and (via the feature gate in the TUI) the whole feature.

use rusqlite::Connection;

use crate::model::task::Task;
use crate::repo::tasks::{row_to_task, TASK_COLUMNS};
use anyhow::Result;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::task;
    use crate::repo::tasks::{archive, create_capture, CaptureInput};
    use crate::testutil::test_conn;

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
