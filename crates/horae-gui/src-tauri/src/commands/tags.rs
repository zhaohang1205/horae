use horae_core::model::tag::Tag;
use horae_core::repo;

use crate::state::AppState;

pub mod fns {
    use super::*;

    pub fn list_tags(conn: &rusqlite::Connection) -> Result<Vec<Tag>, String> {
        repo::tags::list_tags(conn).map_err(|e| e.to_string())
    }

    pub fn create_tag(conn: &rusqlite::Connection, name: &str) -> Result<i64, String> {
        repo::tags::find_or_create_tag(conn, name).map_err(|e| e.to_string())
    }

    pub fn delete_tag(conn: &rusqlite::Connection, name: &str) -> Result<(), String> {
        repo::tags::delete_tag(conn, name).map_err(|e| e.to_string())
    }

    pub fn add_tag_to_task(
        conn: &rusqlite::Connection,
        task_id: &str,
        tag_name: &str,
    ) -> Result<(), String> {
        repo::tags::add_tag_to_task(conn, task_id, tag_name).map_err(|e| e.to_string())
    }

    pub fn remove_tag_from_task(
        conn: &rusqlite::Connection,
        task_id: &str,
        tag_name: &str,
    ) -> Result<(), String> {
        repo::tags::remove_tag_from_task(conn, task_id, tag_name).map_err(|e| e.to_string())
    }

    pub fn get_task_tags(conn: &rusqlite::Connection, task_id: &str) -> Result<Vec<Tag>, String> {
        repo::tags::get_task_tags(conn, task_id).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn list_tags(state: tauri::State<'_, AppState>) -> Result<Vec<Tag>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    fns::list_tags(&conn)
}

#[tauri::command]
pub async fn create_tag(state: tauri::State<'_, AppState>, name: String) -> Result<i64, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    fns::create_tag(&conn, &name)
}

#[tauri::command]
pub async fn delete_tag(state: tauri::State<'_, AppState>, name: String) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    fns::delete_tag(&conn, &name)
}

#[tauri::command]
pub async fn add_tag_to_task(
    state: tauri::State<'_, AppState>,
    task_id: String,
    tag_name: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    fns::add_tag_to_task(&conn, &task_id, &tag_name)
}

#[tauri::command]
pub async fn remove_tag_from_task(
    state: tauri::State<'_, AppState>,
    task_id: String,
    tag_name: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    fns::remove_tag_from_task(&conn, &task_id, &tag_name)
}

#[tauri::command]
pub async fn get_task_tags(
    state: tauri::State<'_, AppState>,
    task_id: String,
) -> Result<Vec<Tag>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    fns::get_task_tags(&conn, &task_id)
}
