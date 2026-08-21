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
    if let Some(t) = get_tag_by_name(conn, name)? {
        return Ok(t.id);
    }
    let category = if name == "p1" || name == "p2" || name == "p3" {
        "priority"
    } else {
        "custom"
    };
    let now = time::now_ms();
    conn.execute(
        "INSERT INTO tags (name, category, is_system, created_at) VALUES (?1, ?2, 0, ?3)",
        rusqlite::params![name, category, now],
    )?;
    Ok(conn.last_insert_rowid())
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
        let tag = get_tag_by_name(tx, tag_name)?
            .ok_or_else(|| Error::TagNotFound(tag_name.to_string()))?;
        let deleted = tx.execute(
            "DELETE FROM task_tags WHERE task_id = ?1 AND tag_id = ?2",
            rusqlite::params![task_id, tag.id],
        )?;
        if deleted == 0 {
            return Err(Error::TagNotFound(tag_name.to_string()).into());
        }
        let meta = format!("{{\"name\":\"{}\"}}", tag_name);
        log_event(
            tx,
            task_id,
            crate::model::event::EV_TAG_REMOVED,
            None,
            None,
            Some(&meta),
            now,
        )?;
        Ok(())
    })
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
