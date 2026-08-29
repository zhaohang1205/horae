use horae_core::model::pomodoro::PomoState;
use horae_core::pomo;
use horae_core::repo;

use crate::state::AppState;

pub mod fns {
    use super::*;

    /// 读取番茄钟状态（基于文件，不经数据库连接）。
    pub fn pomo_state() -> Result<PomoState, String> {
        repo::pomodoro::get_state().map_err(|e| e.to_string())
    }

    /// 进入 Work 相位并写状态（不拉 daemon，通知交前端）。返回最新状态供前端起计时。
    pub fn start_pomo(conn: &rusqlite::Connection, id: &str) -> Result<PomoState, String> {
        pomo::begin_session(conn, id).map_err(|e| e.to_string())
    }

    /// 推进相位机（Work→休息 / 休息→Idle），持久化并返回新状态。
    pub fn complete_pomo(conn: &rusqlite::Connection) -> Result<PomoState, String> {
        pomo::complete(conn).map_err(|e| e.to_string())
    }

    /// 终止番茄钟：置 Idle 并清零 cycle/streak（不拉 daemon，通知交前端）。
    pub fn stop_pomo() -> Result<PomoState, String> {
        pomo::stop().map_err(|e| e.to_string())?;
        repo::pomodoro::get_state().map_err(|e| e.to_string())
    }
}

#[tauri::command]
pub async fn pomo_state() -> Result<PomoState, String> {
    fns::pomo_state()
}

#[tauri::command]
pub async fn start_pomo(
    state: tauri::State<'_, AppState>,
    id: String,
) -> Result<PomoState, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    fns::start_pomo(&conn, &id)
}

#[tauri::command]
pub async fn pomo_complete(state: tauri::State<'_, AppState>) -> Result<PomoState, String> {
    let conn = state.0.lock().map_err(|e| e.to_string())?;
    fns::complete_pomo(&conn)
}

#[tauri::command]
pub async fn pomo_stop() -> Result<PomoState, String> {
    fns::stop_pomo()
}
