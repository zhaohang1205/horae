//! Task data access, split by responsibility:
//! - [`query`] — read-only queries (get / list / counts / stale / events).
//! - [`transition`] — mutations (capture, status changes, schedule, archive, purge).
//! - [`quotes`] — the 金句 (Quotes) view, an optional feature gated by settings.
//!
//! The public surface is re-exported here so callers keep using
//! `crate::repo::tasks::*` unchanged.

mod query;
mod quotes;
mod transition;

pub use query::{
    checked_in_today, count_archived, count_by_status, count_completed_since, events, get, list,
    list_archived, list_stale_inbox, list_stale_waiting, resolve_id, ListFilter,
};
pub use quotes::{count_quotes, count_quotes_in_status, list_quotes, quote_task_ids, QUOTE_TAG};
pub use transition::{
    add_checklist_item, archive, create_capture, delete_checklist_item, ensure_ready_for_pomodoro,
    move_checklist_item, purge, rename, rename_checklist_item, schedule, set_due, set_rrule,
    toggle_checklist_item, toggle_next_checklist_item, transition, unarchive, update_notes,
    CaptureInput,
};

use crate::model::task::Task;

/// 系统内置日志任务的固定 ID（由 migration 0012 创建，`horae log` 依赖它）。
/// 普通查询/归档视图会排除它，且不允许被 purge。
pub const SYSTEM_JOURNAL_ID: &str = "__journal__";

/// Columns for the `tasks` table, shared by every row-mapping query.
pub(crate) const TASK_COLUMNS: &str = "id,title,notes,status,rrule,created_at,clarified_at,\
        due_at,scheduled_start_at,scheduled_end_at,completed_at,archived_at,updated_at,\
        delegated_to,checklist,archive_reason";

pub(crate) fn row_to_task(r: &rusqlite::Row) -> rusqlite::Result<Task> {
    let status_str: String = r.get(3)?;
    let delegated_to: Option<String> = r.get(13)?;
    let cl_str: String = r.get(14)?;

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
        completed_at: r.get(10)?,
        archived_at: r.get(11)?,
        updated_at: r.get(12)?,
        delegated_to,
        checklist: serde_json::from_str(&cl_str).unwrap_or_default(),
        archive_reason: r.get(15)?,
    })
}
