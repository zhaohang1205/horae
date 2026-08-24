//! 测试辅助：临时文件 SQLite 连接（已应用迁移）。
use rusqlite::Connection;
use tempfile::TempDir;

/// 建一个应用好迁移的临时 DB。返回 (dir, conn)，dir 用于让 TempDir 保持存活。
pub fn test_conn() -> (TempDir, Connection) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("horae.db");
    let mut conn = Connection::open(&path).unwrap();
    crate::db::migrate::run(&mut conn).unwrap();
    (dir, conn)
}

/// 进程全局互斥锁：`HORAE_CONFIG_DIR` 是环境变量（进程全局），
/// 所有依赖它的测试必须持锁执行，避免并行测试互相踩踏。
pub static CONFIG_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 在隔离的临时配置目录下执行 `f`，结束后（含 panic 路径）恢复环境变量，
/// 防止污染同进程内的其它测试（如 `Config::load` 的回退行为测试）。
pub fn with_test_config_dir(f: impl FnOnce()) {
    let _guard = CONFIG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let tmp = tempfile::tempdir().unwrap();
    std::env::set_var("HORAE_CONFIG_DIR", tmp.path());
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    std::env::remove_var("HORAE_CONFIG_DIR");
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}

/// 持锁执行 `f` 并保证期间 `HORAE_CONFIG_DIR` 未设置（结束后恢复原值）。
/// 供依赖真实 config_dir 布局（含 `horae/` 后缀）的测试使用。
pub fn with_no_config_dir(f: impl FnOnce()) {
    let _guard = CONFIG_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let saved = std::env::var_os("HORAE_CONFIG_DIR");
    std::env::remove_var("HORAE_CONFIG_DIR");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    match saved {
        Some(v) => std::env::set_var("HORAE_CONFIG_DIR", v),
        None => std::env::remove_var("HORAE_CONFIG_DIR"),
    }
    if let Err(payload) = result {
        std::panic::resume_unwind(payload);
    }
}
