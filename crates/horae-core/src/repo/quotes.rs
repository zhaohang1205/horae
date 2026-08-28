//! The optional Quotes feature gate (`repo::quotes`).
//!
//! This module owns the gate (`Quotes`) and exposes the unified interface
//! for the TUI to consume. It wraps the query logic from `repo::tasks::quotes`.

use anyhow::Result;
use rusqlite::Connection;

use crate::model::task::Task;
use crate::repo::tasks;

/// A feature gate for Quotes.
#[derive(Clone, Copy, Debug)]
pub struct Quotes {
    pub enabled: bool,
}

impl Quotes {
    /// Read the enabled state from `settings` (key: "quotes", value "1" = enabled).
    pub fn load(conn: &Connection) -> Self {
        let enabled = matches!(
            crate::repo::settings::get(conn, "quotes")
                .ok()
                .flatten()
                .as_deref(),
            Some("1")
        );
        Self { enabled }
    }

    /// Update the enabled state in the database.
    pub fn set_enabled(&mut self, conn: &Connection, enabled: bool) -> Result<()> {
        self.enabled = enabled;
        crate::repo::settings::set(conn, "quotes", if enabled { "1" } else { "0" })?;
        Ok(())
    }

    /// Toggle the enabled state.
    pub fn toggle_enabled(&mut self, conn: &Connection) -> Result<bool> {
        self.set_enabled(conn, !self.enabled)?;
        Ok(self.enabled)
    }

    /// List quotes. If disabled, returns empty.
    pub fn list(
        &self,
        conn: &Connection,
        query: Option<&str>,
        extra_tag: Option<&str>,
    ) -> Result<Vec<Task>> {
        if !self.enabled {
            return Ok(vec![]);
        }
        tasks::list_quotes(conn, query, extra_tag)
    }

    /// Count all quotes. If disabled, returns 0.
    pub fn count(&self, conn: &Connection, query: Option<&str>) -> Result<usize> {
        if !self.enabled {
            return Ok(0);
        }
        tasks::count_quotes(conn, query)
    }

    /// Count quotes in a specific status. If disabled, returns 0.
    pub fn count_in_status(
        &self,
        conn: &Connection,
        status: &str,
        query: Option<&str>,
    ) -> Result<usize> {
        if !self.enabled {
            return Ok(0);
        }
        tasks::count_quotes_in_status(conn, status, query)
    }

    /// Returns ids of all quote-tagged tasks to exclude from Reference view.
    /// If disabled, returns empty.
    pub fn exclude_ids(&self, conn: &Connection) -> Result<Vec<String>> {
        if !self.enabled {
            return Ok(vec![]);
        }
        tasks::quote_task_ids(conn)
    }

    /// Toggle the `@quote` tag on a given task ID.
    /// If adding the tag, also transitions active (work) statuses to Reference,
    /// recording the original status in the event meta (`{"quote_from":...}`).
    /// If removing the tag, restores that recorded status so the toggle is
    /// symmetric and the task does not stay stuck in Reference.
    /// Returns true if the tag was added, false if it was removed.
    pub fn toggle_tag(&self, conn: &Connection, task_id: &str) -> Result<Option<bool>> {
        if !self.enabled {
            return Ok(None);
        }
        let task = tasks::get(conn, task_id)?;
        if task.archived_at.is_some() {
            return Ok(None); // Do not touch archived
        }
        let has_quote = crate::repo::tags::get_task_tags(conn, task_id)
            .unwrap_or_default()
            .iter()
            .any(|t| t.name == tasks::QUOTE_TAG);
        if has_quote {
            crate::repo::tags::remove_tag_from_task(conn, task_id, tasks::QUOTE_TAG)?;
            self.restore_status_before_quote(conn, task_id)?;
            Ok(Some(false))
        } else {
            crate::repo::mutate(conn, |tx, now| {
                crate::repo::tags::add_tag_to_task_inner(tx, task_id, tasks::QUOTE_TAG, now)?;
                if !matches!(
                    task.status,
                    crate::model::task::Status::Reference | crate::model::task::Status::Done
                ) {
                    tx.execute(
                        "UPDATE tasks SET status=?1, updated_at=?2 WHERE id=?3",
                        rusqlite::params![
                            crate::model::task::Status::Reference.to_string(),
                            now,
                            task_id
                        ],
                    )?;
                    let meta = format!("{{\"quote_from\":\"{}\"}}", task.status);
                    crate::repo::log_event(
                        tx,
                        task_id,
                        crate::model::event::EV_STATUS_CHANGED,
                        Some(&task.status.to_string()),
                        Some(&crate::model::task::Status::Reference.to_string()),
                        Some(&meta),
                        now,
                    )?;
                }
                Ok(())
            })?;
            Ok(Some(true))
        }
    }

    /// 摘除 @quote 后恢复入库前的状态。仅当任务仍停留在 Reference 且存在
    /// 带 `quote_from` meta 的流转记录时才回退——用户若已手动移动过状态，
    /// 则尊重现状不动。
    fn restore_status_before_quote(&self, conn: &Connection, task_id: &str) -> Result<()> {
        let cur = tasks::get(conn, task_id)?;
        if cur.status != crate::model::task::Status::Reference {
            return Ok(());
        }
        let mut stmt = conn.prepare(
            "SELECT meta FROM task_events \
             WHERE task_id = ?1 AND event_type = ?2 AND to_status = 'reference' \
               AND from_status IS NOT NULL \
             ORDER BY at DESC LIMIT 10",
        )?;
        let metas: Vec<Option<String>> = stmt
            .query_map(
                rusqlite::params![task_id, crate::model::event::EV_STATUS_CHANGED],
                |r| r.get(0),
            )?
            .filter_map(|r| r.ok())
            .collect();
        drop(stmt);
        let origin = metas.iter().flatten().find_map(|m| {
            serde_json::from_str::<serde_json::Value>(m)
                .ok()?
                .get("quote_from")?
                .as_str()
                .map(str::to_string)
        });
        if let Some(from) = origin {
            if let Ok(status) = from.parse::<crate::model::task::Status>() {
                if !matches!(
                    status,
                    crate::model::task::Status::Reference | crate::model::task::Status::Done
                ) {
                    tasks::transition(conn, task_id, status)?;
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::task::Status;
    use crate::repo::tasks::CaptureInput;
    use crate::testutil::test_conn;

    #[test]
    fn toggle_tag_round_trip_restores_status() {
        let (_dir, conn) = test_conn();
        let q = Quotes { enabled: true };
        let t = crate::repo::tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "灵感".into(),
                status: Status::Next,
                ..Default::default()
            },
        )
        .unwrap();

        // 入库：Next → Reference
        assert_eq!(q.toggle_tag(&conn, &t.id).unwrap(), Some(true));
        assert_eq!(tasks::get(&conn, &t.id).unwrap().status, Status::Reference);

        // 摘除：应恢复回 Next，而不是卡在 Reference
        assert_eq!(q.toggle_tag(&conn, &t.id).unwrap(), Some(false));
        assert_eq!(tasks::get(&conn, &t.id).unwrap().status, Status::Next);
    }

    #[test]
    fn toggle_tag_respects_manual_move_after_add() {
        let (_dir, conn) = test_conn();
        let q = Quotes { enabled: true };
        let t = crate::repo::tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "手动流转".into(),
                status: Status::Inbox,
                ..Default::default()
            },
        )
        .unwrap();
        q.toggle_tag(&conn, &t.id).unwrap();
        // 用户手动把任务移到 Waiting
        tasks::transition(&conn, &t.id, Status::Waiting).unwrap();
        // 摘除金句标签不应覆盖用户的手动流转
        q.toggle_tag(&conn, &t.id).unwrap();
        assert_eq!(tasks::get(&conn, &t.id).unwrap().status, Status::Waiting);
    }

    #[test]
    fn toggle_tag_leaves_already_reference_tasks_alone() {
        let (_dir, conn) = test_conn();
        let q = Quotes { enabled: true };
        // 本来就是 Reference 的任务（非 quote 流转产生）
        let t = crate::repo::tasks::create_capture(
            &conn,
            &CaptureInput {
                title: "原生参考资料".into(),
                status: Status::Reference,
                ..Default::default()
            },
        )
        .unwrap();
        q.toggle_tag(&conn, &t.id).unwrap(); // 加标签，状态不变
        q.toggle_tag(&conn, &t.id).unwrap(); // 摘标签
        assert_eq!(
            tasks::get(&conn, &t.id).unwrap().status,
            Status::Reference,
            "无 quote_from 记录时不应回退状态"
        );
    }
}
