use std::path::PathBuf;
use std::time::Duration;

use rusqlite::Connection;

use crate::db::migrate;

/// Open (creating if needed) the horae SQLite database for a named profile and
/// run migrations. When `name` is `None`, the config's default profile is used.
///
/// Database files live under the XDG config dir (`~/.config/horae`); the legacy
/// `horae.db` is the default profile, so existing setups keep working.
pub fn open(name: Option<&str>) -> anyhow::Result<Connection> {
    let config = crate::config::Config::load()?;
    let (_, profile) = config.resolve_profile(name)?;
    let path = config.db_path(profile);

    let dir = path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&dir)?;

    let mut conn = Connection::open(path)?;

    // 先设 busy_timeout，让随后的 journal_mode 切换（需要写锁）也能等待而不是
    // 立即报 `database is locked`；WAL 让 CLI/TUI/pomo 多进程并发读不互相阻塞。
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA cache_size=-64000;
         PRAGMA temp_store=MEMORY;
         PRAGMA mmap_size=3000000000;",
    )?;

    migrate::run(&mut conn)?;
    Ok(conn)
}
