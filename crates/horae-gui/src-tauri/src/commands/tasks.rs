use horae_core::model::event::TaskEvent;
use horae_core::model::task::{self, Task};
use horae_core::repo;
use horae_core::time;
use serde::Serialize;

use crate::state::AppState;

/// 任务详情：任务本体 + 事件时间线（供 GUI 详情页展示）。
#[derive(Debug, Clone, Serialize)]
pub struct TaskDetail {
    pub task: Task,
    pub events: Vec<TaskEvent>,
}

/// 业务逻辑自由函数：直接操作 `&Connection`，无 Tauri 依赖，便于集成测试直接调用。
pub mod fns {
    use super::*;

    /// 按视图列出任务。视图名：inbox/next/scheduled/waiting/someday/reference/archived/all/today。
    pub fn list_tasks(conn: &rusqlite::Connection, view: &str) -> Result<Vec<Task>, String> {
        match view {
            "archived" => repo::tasks::list_archived(conn).map_err(|e| e.to_string()),
            "today" => {
                let all = repo::tasks::list(
                    conn,
                    &repo::tasks::ListFilter {
                        status: None,
                        tags: vec![],
                        query: None,
                        review_stale: false,
                    },
                )
                .map_err(|e| e.to_string())?;
                let (start, end) = time::local_day_bounds(0);
                Ok(all
                    .into_iter()
                    .filter(|t| {
                        t.due_at.is_some_and(|d| d >= start && d < end)
                            || t.scheduled_start_at.is_some_and(|d| d >= start && d < end)
                    })
                    .collect())
            }
            other => {
                let status = if other == "all" {
                    None
                } else {
                    Some(other.parse::<task::Status>().map_err(|e: String| e)?)
                };
                repo::tasks::list(
                    conn,
                    &repo::tasks::ListFilter {
                        status,
                        tags: vec![],
                        query: None,
                        review_stale: false,
                    },
                )
                .map_err(|e| e.to_string())
            }
        }
    }

    /// 捕获任务：复用 CLI 的 quick-add 解析逻辑（标签/~时间/RRule/优先级）。
    pub fn capture(conn: &rusqlite::Connection, raw: &str) -> Result<Task, String> {
        let qa = horae_core::parser::parse_quick_add(raw);
        if let Some(rr) = &qa.rrule {
            if !horae_core::parser::rrule_valid(rr) {
                return Err(format!(
                    "invalid rrule `{rr}`: engine only supports FREQ=DAILY|WEEKLY|MONTHLY"
                ));
            }
        }
        let mut tag_names: Vec<String> = qa.tags;
        if let Some(p) = &qa.priority {
            tag_names.push(p.clone());
        }
        let scheduled_start: Option<i64> = qa
            .time_str
            .as_deref()
            .map(time::parse_time)
            .transpose()
            .map_err(|e| e.to_string())?;

        let input = repo::tasks::CaptureInput {
            title: qa.title,
            status: if scheduled_start.is_some() {
                task::Status::Scheduled
            } else {
                task::Status::Inbox
            },
            due_at: None,
            tag_names,
            rrule: if scheduled_start.is_some() {
                None
            } else {
                qa.rrule.clone()
            },
            ..Default::default()
        };
        let created = repo::tasks::create_capture(conn, &input).map_err(|e| e.to_string())?;
        if let Some(start) = scheduled_start {
            repo::tasks::schedule(conn, &created.id, start, None, qa.rrule)
                .map_err(|e| e.to_string())?;
        }
        repo::tasks::get(conn, &created.id).map_err(|e| e.to_string())
    }

    pub fn transition(conn: &rusqlite::Connection, id: &str, status: &str) -> Result<Task, String> {
        let st = status.parse::<task::Status>().map_err(|e: String| e)?;
        repo::tasks::transition(conn, id, st).map_err(|e| e.to_string())
    }

    pub fn set_due(
        conn: &rusqlite::Connection,
        id: &str,
        due_ms: Option<i64>,
    ) -> Result<Task, String> {
        repo::tasks::set_due(conn, id, due_ms).map_err(|e| e.to_string())
    }

    pub fn schedule(
        conn: &rusqlite::Connection,
        id: &str,
        start_ms: i64,
        end_ms: Option<i64>,
    ) -> Result<Task, String> {
        repo::tasks::schedule(conn, id, start_ms, end_ms, None).map_err(|e| e.to_string())
    }

    pub fn archive(conn: &rusqlite::Connection, id: &str) -> Result<Task, String> {
        repo::tasks::archive(conn, id).map_err(|e| e.to_string())
    }

    pub fn unarchive(conn: &rusqlite::Connection, id: &str) -> Result<Task, String> {
        repo::tasks::unarchive(conn, id).map_err(|e| e.to_string())
    }

    pub fn purge(conn: &rusqlite::Connection, id: &str) -> Result<Task, String> {
        repo::tasks::purge(conn, id).map_err(|e| e.to_string())
    }

    pub fn detail(conn: &rusqlite::Connection, id: &str) -> Result<TaskDetail, String> {
        let task = repo::tasks::get(conn, id).map_err(|e| e.to_string())?;
        let events = repo::tasks::events(conn, id).map_err(|e| e.to_string())?;
        Ok(TaskDetail { task, events })
    }

    pub fn rename(conn: &rusqlite::Connection, id: &str, title: &str) -> Result<Task, String> {
        repo::tasks::rename(conn, id, title).map_err(|e| e.to_string())
    }

    pub fn update_notes(
        conn: &rusqlite::Connection,
        id: &str,
        notes: &str,
    ) -> Result<Task, String> {
        repo::tasks::update_notes(conn, id, notes).map_err(|e| e.to_string())
    }

    pub fn toggle_checklist_item(
        conn: &rusqlite::Connection,
        id: &str,
        item_id: &str,
    ) -> Result<Option<String>, String> {
        repo::tasks::toggle_checklist_item(conn, id, item_id).map_err(|e| e.to_string())
    }

    pub fn add_checklist_item(
        conn: &rusqlite::Connection,
        id: &str,
        title: &str,
    ) -> Result<Option<String>, String> {
        repo::tasks::add_checklist_item(conn, id, title).map_err(|e| e.to_string())
    }

    pub fn delete_checklist_item(
        conn: &rusqlite::Connection,
        id: &str,
        item_id: &str,
    ) -> Result<Option<String>, String> {
        repo::tasks::delete_checklist_item(conn, id, item_id).map_err(|e| e.to_string())
    }
}

macro_rules! lock {
    ($state:expr) => {
        $state.0.lock().map_err(|e| e.to_string())?
    };
}

#[tauri::command]
pub async fn list_tasks(
    state: tauri::State<'_, AppState>,
    view: String,
) -> Result<Vec<Task>, String> {
    let conn = lock!(state);
    fns::list_tasks(&conn, &view)
}

#[tauri::command]
pub async fn capture(state: tauri::State<'_, AppState>, input: String) -> Result<Task, String> {
    let conn = lock!(state);
    fns::capture(&conn, &input)
}

#[tauri::command]
pub async fn transition(
    state: tauri::State<'_, AppState>,
    id: String,
    status: String,
) -> Result<Task, String> {
    let conn = lock!(state);
    fns::transition(&conn, &id, &status)
}

#[tauri::command]
pub async fn set_due(
    state: tauri::State<'_, AppState>,
    id: String,
    due_ms: Option<i64>,
) -> Result<Task, String> {
    let conn = lock!(state);
    fns::set_due(&conn, &id, due_ms)
}

#[tauri::command]
pub async fn schedule(
    state: tauri::State<'_, AppState>,
    id: String,
    start_ms: i64,
    end_ms: Option<i64>,
) -> Result<Task, String> {
    let conn = lock!(state);
    fns::schedule(&conn, &id, start_ms, end_ms)
}

#[tauri::command]
pub async fn archive(state: tauri::State<'_, AppState>, id: String) -> Result<Task, String> {
    let conn = lock!(state);
    fns::archive(&conn, &id)
}

#[tauri::command]
pub async fn unarchive(state: tauri::State<'_, AppState>, id: String) -> Result<Task, String> {
    let conn = lock!(state);
    fns::unarchive(&conn, &id)
}

#[tauri::command]
pub async fn purge(state: tauri::State<'_, AppState>, id: String) -> Result<Task, String> {
    let conn = lock!(state);
    fns::purge(&conn, &id)
}

#[tauri::command]
pub async fn detail(state: tauri::State<'_, AppState>, id: String) -> Result<TaskDetail, String> {
    let conn = lock!(state);
    fns::detail(&conn, &id)
}

#[tauri::command]
pub async fn rename(
    state: tauri::State<'_, AppState>,
    id: String,
    title: String,
) -> Result<Task, String> {
    let conn = lock!(state);
    fns::rename(&conn, &id, &title)
}

#[tauri::command]
pub async fn update_notes(
    state: tauri::State<'_, AppState>,
    id: String,
    notes: String,
) -> Result<Task, String> {
    let conn = lock!(state);
    fns::update_notes(&conn, &id, &notes)
}

#[tauri::command]
pub async fn toggle_checklist_item(
    state: tauri::State<'_, AppState>,
    id: String,
    item_id: String,
) -> Result<Option<String>, String> {
    let conn = lock!(state);
    fns::toggle_checklist_item(&conn, &id, &item_id)
}

#[tauri::command]
pub async fn add_checklist_item(
    state: tauri::State<'_, AppState>,
    id: String,
    title: String,
) -> Result<Option<String>, String> {
    let conn = lock!(state);
    fns::add_checklist_item(&conn, &id, &title)
}

#[tauri::command]
pub async fn delete_checklist_item(
    state: tauri::State<'_, AppState>,
    id: String,
    item_id: String,
) -> Result<Option<String>, String> {
    let conn = lock!(state);
    fns::delete_checklist_item(&conn, &id, &item_id)
}
