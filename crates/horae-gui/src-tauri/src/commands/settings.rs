use horae_core::repo;

use crate::state::AppState;

pub mod fns {
    use super::*;

    pub fn get_setting(conn: &rusqlite::Connection, key: &str) -> Result<Option<String>, String> {
        repo::settings::get(conn, key).map_err(|e| e.to_string())
    }

    pub fn set_setting(conn: &rusqlite::Connection, key: &str, value: &str) -> Result<(), String> {
        repo::settings::set(conn, key, value).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn get_setting(
    state: tauri::State<'_, AppState>,
    key: String,
) -> Result<Option<String>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    fns::get_setting(&conn, &key)
}

#[tauri::command]
pub async fn set_setting(
    state: tauri::State<'_, AppState>,
    key: String,
    value: String,
) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    fns::set_setting(&conn, &key, &value)
}
