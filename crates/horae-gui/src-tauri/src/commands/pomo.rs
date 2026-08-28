use horae_core::model::pomodoro::PomoState;
use horae_core::repo;

use crate::state::AppState;

pub mod fns {
    use super::*;

    /// 读取番茄钟状态（基于文件，不经数据库连接）。
    pub fn pomo_state() -> Result<PomoState, String> {
        repo::pomodoro::get_state().map_err(|e| e.to_string())
    }

    /// 标记任务为可进入番茄钟（校验状态），守护进程启动留待 Phase 3。
    pub fn start_pomo(conn: &rusqlite::Connection, id: &str) -> Result<(), String> {
        repo::tasks::ensure_ready_for_pomodoro(conn, id).map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn pomo_state() -> Result<PomoState, String> {
    fns::pomo_state()
}

#[tauri::command]
pub async fn start_pomo(state: tauri::State<'_, AppState>, id: String) -> Result<(), String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    fns::start_pomo(&conn, &id)
}
