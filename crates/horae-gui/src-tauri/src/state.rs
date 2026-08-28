//! GUI 应用状态：持有 SQLite 连接（被 Tauri `State` 管理）。
use std::sync::Mutex;

use rusqlite::Connection;

/// 整个 GUI 共享的数据库连接。短临界区 `lock().unwrap()` 即可调用 `horae_core::repo::*`。
pub struct AppState(pub Mutex<Connection>);
