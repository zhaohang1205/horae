use horae_core::notification::NotificationEvent;

use crate::state::AppState;

pub mod fns {
    use super::*;

    /// 计算需要弹出的提醒；使用 GUI 专用状态文件 `notify_gui.json`，与 TUI 互不干扰。
    pub fn tick_notifications(conn: &rusqlite::Connection) -> Vec<NotificationEvent> {
        let mut engine = horae_core::notification::NotificationEngine::new_gui();
        engine.tick(conn)
    }
}

#[tauri::command]
pub async fn tick_notifications(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<NotificationEvent>, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    Ok(fns::tick_notifications(&conn))
}
