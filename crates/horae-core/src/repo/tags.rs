use rusqlite::Connection;

use crate::error::Error;
use crate::model::tag::Tag;
use crate::repo::log_event;
use crate::time;
use anyhow::Result;

/// Count of all tags, for the guide sidebar badge.
pub fn count_tags(conn: &Connection) -> Result<usize> {
    let c: usize = conn.query_row("SELECT COUNT(*) FROM tags", [], |r| r.get(0))?;
    Ok(c)
}

pub fn list_tags(conn: &Connection) -> Result<Vec<Tag>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, category, is_system, color, icon, description, created_at \
         FROM tags ORDER BY category, name",
    )?;
    let rows = stmt.query_map([], row_to_tag)?;
    let mut out = Vec::new();
    for t in rows {
        out.push(t?);
    }
    Ok(out)
}

pub fn get_tag_by_name(conn: &Connection, name: &str) -> Result<Option<Tag>> {
    let mut stmt = conn.prepare(
        "SELECT id, name, category, is_system, color, icon, description, created_at \
         FROM tags WHERE name = ?1",
    )?;
    let mut rows = stmt.query_map([name], row_to_tag)?;
    Ok(rows.next().transpose()?)
}

/// Return the tag id for `name`, creating a custom tag if it doesn't exist.
pub fn find_or_create_tag(conn: &Connection, name: &str) -> Result<i64> {
    let category = "custom";
    let now = time::now_ms();
    conn.execute(
        "INSERT OR IGNORE INTO tags (name, category, is_system, created_at) VALUES (?1, ?2, 0, ?3)",
        rusqlite::params![name, category, now],
    )?;
    let tag = get_tag_by_name(conn, name)?
        .ok_or_else(|| anyhow::anyhow!("failed to find or create tag: {}", name))?;
    Ok(tag.id)
}

pub fn add_tag_to_task(conn: &Connection, task_id: &str, tag_name: &str) -> Result<()> {
    crate::repo::mutate(conn, |tx, now| {
        add_tag_to_task_inner(tx, task_id, tag_name, now)
    })
}

/// 在事务内给任务加标签并写审计事件。`now` 由外层事务传入，保证与其它事件
/// 时间戳一致（时间数据化）。
pub(crate) fn add_tag_to_task_inner(
    conn: &Connection,
    task_id: &str,
    tag_name: &str,
    now: i64,
) -> Result<()> {
    let tag_id = find_or_create_tag(conn, tag_name)?;
    let exists: i64 = conn.query_row(
        "SELECT COUNT(*) FROM task_tags WHERE task_id = ?1 AND tag_id = ?2",
        rusqlite::params![task_id, tag_id],
        |r| r.get(0),
    )?;
    if exists > 0 {
        return Ok(());
    }
    conn.execute(
        "INSERT INTO task_tags (task_id, tag_id, added_at) VALUES (?1, ?2, ?3)",
        rusqlite::params![task_id, tag_id, now],
    )?;
    let meta = format!("{{\"name\":\"{}\"}}", tag_name);
    log_event(
        conn,
        task_id,
        crate::model::event::EV_TAG_ADDED,
        None,
        None,
        Some(&meta),
        now,
    )?;
    Ok(())
}

pub fn remove_tag_from_task(conn: &Connection, task_id: &str, tag_name: &str) -> Result<()> {
    crate::repo::mutate(conn, |tx, now| {
        let removed = remove_tag_from_task_inner(tx, task_id, tag_name, now)?;
        if !removed {
            return Err(Error::TagNotFound(tag_name.to_string()).into());
        }
        Ok(())
    })
}

pub(crate) fn remove_tag_from_task_inner(
    conn: &Connection,
    task_id: &str,
    tag_name: &str,
    now: i64,
) -> Result<bool> {
    let Some(tag) = get_tag_by_name(conn, tag_name)? else {
        return Ok(false);
    };
    let deleted = conn.execute(
        "DELETE FROM task_tags WHERE task_id = ?1 AND tag_id = ?2",
        rusqlite::params![task_id, tag.id],
    )?;
    if deleted > 0 {
        let meta = format!("{{\"name\":\"{}\"}}", tag_name);
        log_event(
            conn,
            task_id,
            crate::model::event::EV_TAG_REMOVED,
            None,
            None,
            Some(&meta),
            now,
        )?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Delete a tag from the tags table and remove all its associations in task_tags.
/// System tags (is_system = 1) cannot be deleted.
pub fn delete_tag(conn: &Connection, tag_name: &str) -> Result<()> {
    crate::repo::mutate(conn, |tx, now| {
        if let Some(tag) = get_tag_by_name(tx, tag_name)? {
            if tag.is_system {
                anyhow::bail!("系统预设标签不能删除");
            }
            // 查找当前绑定该标签的所有任务 ID，为其补齐 tag_removed 审计日志
            let mut stmt = tx.prepare("SELECT task_id FROM task_tags WHERE tag_id = ?1")?;
            let task_ids: Vec<String> = stmt
                .query_map([tag.id], |r| r.get(0))?
                .filter_map(|r| r.ok())
                .collect();
            drop(stmt);

            let meta = format!("{{\"name\":\"{}\"}}", tag_name);
            for task_id in task_ids {
                log_event(
                    tx,
                    &task_id,
                    crate::model::event::EV_TAG_REMOVED,
                    None,
                    None,
                    Some(&meta),
                    now,
                )?;
            }

            tx.execute(
                "DELETE FROM task_tags WHERE tag_id = ?1",
                rusqlite::params![tag.id],
            )?;
            tx.execute("DELETE FROM tags WHERE id = ?1", rusqlite::params![tag.id])?;
        }
        Ok(())
    })
}

pub fn get_task_tags(conn: &Connection, task_id: &str) -> Result<Vec<Tag>> {
    let mut stmt = conn.prepare(
        "SELECT t.id, t.name, t.category, t.is_system, t.color, t.icon, t.description, t.created_at \
         FROM tags t JOIN task_tags tt ON tt.tag_id = t.id WHERE tt.task_id = ?1 \
         ORDER BY t.category, t.name",
    )?;
    let rows = stmt.query_map([task_id], row_to_tag)?;
    let mut out = Vec::new();
    for t in rows {
        out.push(t?);
    }
    Ok(out)
}

/// 单次查询取出一组任务的标签名，返回 `task_id -> 标签名列表`。
/// 供列表刷新批量使用，替代逐行 `get_task_tags`。
/// 按批查询（每批 500），规避 SQLite 默认 `SQLITE_MAX_VARIABLE_NUMBER`(999) 上限。
pub fn get_tags_for_tasks(
    conn: &Connection,
    ids: &[&str],
) -> Result<std::collections::HashMap<String, Vec<String>>> {
    let mut out: std::collections::HashMap<String, Vec<String>> = std::collections::HashMap::new();
    const BATCH: usize = 500;
    for chunk in ids.chunks(BATCH) {
        let placeholders = vec!["?"; chunk.len()].join(",");
        let sql = format!(
            "SELECT tt.task_id, t.name FROM tags t JOIN task_tags tt ON tt.tag_id = t.id \
             WHERE tt.task_id IN ({}) ORDER BY t.category, t.name",
            placeholders
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter().copied()), |r| {
            Ok((r.get::<usize, String>(0)?, r.get::<usize, String>(1)?))
        })?;
        for r in rows {
            let (tid, name) = r?;
            out.entry(tid).or_default().push(name);
        }
    }
    Ok(out)
}

fn row_to_tag(r: &rusqlite::Row) -> rusqlite::Result<Tag> {
    Ok(Tag {
        id: r.get(0)?,
        name: r.get(1)?,
        category: r.get(2)?,
        is_system: r.get::<usize, i64>(3)? != 0,
        color: r.get(4)?,
        icon: r.get(5)?,
        description: r.get(6)?,
        created_at: r.get(7)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::event::{EV_TAG_ADDED, EV_TAG_REMOVED};
    use crate::model::task::Status;
    use crate::repo::tasks::{create_capture, CaptureInput};
    use crate::testutil::test_conn;

    fn mk_task(conn: &Connection, title: &str, tags: &[&str]) -> crate::model::task::Task {
        create_capture(
            conn,
            &CaptureInput {
                title: title.into(),
                status: Status::Next,
                tag_names: tags.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            },
        )
        .unwrap()
    }

    fn count_events(conn: &Connection, task_id: &str, ev: &str) -> i64 {
        conn.query_row(
            "SELECT COUNT(*) FROM task_events WHERE task_id = ?1 AND event_type = ?2",
            rusqlite::params![task_id, ev],
            |r| r.get(0),
        )
        .unwrap()
    }

    #[test]
    fn find_or_create_classifies_custom_and_is_idempotent() {
        let (_dir, conn) = test_conn();
        let urgent = get_tag_by_name(&conn, "urgent").unwrap();
        assert!(urgent.is_none(), "前置：urgent 尚不存在");
        let _ = find_or_create_tag(&conn, "urgent").unwrap();
        let urgent = get_tag_by_name(&conn, "urgent").unwrap().unwrap();
        assert_eq!(urgent.category, "custom");
        assert!(!urgent.is_system);

        // 幂等：重复创建返回同一 id，且不产生重复行
        let again = find_or_create_tag(&conn, "urgent").unwrap();
        assert_eq!(again, urgent.id);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM tags WHERE name = 'urgent'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn add_tag_to_task_is_idempotent_without_duplicate_audit() {
        let (_dir, conn) = test_conn();
        let t = mk_task(&conn, "带标签", &[]);

        add_tag_to_task(&conn, &t.id, "side").unwrap();
        add_tag_to_task(&conn, &t.id, "side").unwrap();

        let n: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM task_tags WHERE task_id = ?1",
                rusqlite::params![t.id],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(n, 1, "重复添加不应产生重复关联行");
        assert_eq!(
            count_events(&conn, &t.id, EV_TAG_ADDED),
            1,
            "第二次添加不应再写审计事件"
        );
    }

    #[test]
    fn remove_tag_errors_when_missing_or_not_attached() {
        let (_dir, conn) = test_conn();
        let a = mk_task(&conn, "A", &["home"]);
        let b = mk_task(&conn, "B", &[]);

        // 标签本身不存在
        let err = remove_tag_from_task(&conn, &a.id, "ghost").unwrap_err();
        assert!(err.to_string().contains("tag not found"), "{}", err);
        // 标签存在但未绑定该任务
        let err = remove_tag_from_task(&conn, &b.id, "home").unwrap_err();
        assert!(
            err.to_string().contains("tag not found: home"),
            "未绑定的移除也应报 TagNotFound: {}",
            err
        );
        // 任务 A 的 home 关联未被误删
        assert_eq!(get_task_tags(&conn, &a.id).unwrap().len(), 1);
    }

    #[test]
    fn delete_tag_rejects_system_tags() {
        let (_dir, conn) = test_conn();
        let err = delete_tag(&conn, "home").unwrap_err();
        assert!(err.to_string().contains("系统预设标签不能删除"), "{}", err);
        assert!(get_tag_by_name(&conn, "home").unwrap().is_some());
    }

    #[test]
    fn delete_unknown_tag_is_silent_noop() {
        let (_dir, conn) = test_conn();
        delete_tag(&conn, "ghost").unwrap();
        assert!(get_tag_by_name(&conn, "ghost").unwrap().is_none());
    }

    #[test]
    fn delete_custom_tag_cascades_with_audit_per_task() {
        let (_dir, conn) = test_conn();
        let a = mk_task(&conn, "A", &[]);
        let b = mk_task(&conn, "B", &[]);
        let c = mk_task(&conn, "C", &[]);
        for t in [&a, &b] {
            add_tag_to_task(&conn, &t.id, "side-project").unwrap();
        }

        delete_tag(&conn, "side-project").unwrap();

        assert!(get_tag_by_name(&conn, "side-project").unwrap().is_none());
        for t in [&a, &b] {
            let n: i64 = conn
                .query_row(
                    "SELECT COUNT(*) FROM task_tags WHERE task_id = ?1",
                    rusqlite::params![t.id],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(n, 0, "关联行应被级联清除");
            assert_eq!(
                count_events(&conn, &t.id, EV_TAG_REMOVED),
                1,
                "每个绑定任务都应补写一条 tag_removed 审计"
            );
        }
        // 未绑定该标签的任务 C 不应有审计事件
        assert_eq!(count_events(&conn, &c.id, EV_TAG_REMOVED), 0);
    }

    #[test]
    fn get_tags_for_tasks_batches_over_500_ids() {
        let (_dir, conn) = test_conn();
        let a = mk_task(&conn, "A", &["work"]);
        let b = mk_task(&conn, "B", &["home"]);

        // 501 个 id（含大量不存在的占位 id）触发分批路径
        let mut ids: Vec<&str> = vec![a.id.as_str(), b.id.as_str()];
        ids.extend((0..499).map(|_| "no-such-task"));
        assert_eq!(ids.len(), 501);

        let map = get_tags_for_tasks(&conn, &ids).unwrap();
        assert_eq!(
            map.get(a.id.as_str()).unwrap(),
            &vec!["work".to_string()],
            "分批后第一批仍应完整返回"
        );
        assert_eq!(
            map.get(b.id.as_str()).unwrap(),
            &vec!["home".to_string()],
            "第 501 个 id 落入第二批，不应丢失"
        );
    }
}
