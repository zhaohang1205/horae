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
