use std::path::PathBuf;
use std::sync::{OnceLock, RwLock};
use std::time::Duration;

use rusqlite::Connection;

use crate::db::migrate;

static ACTIVE_PROFILE: OnceLock<RwLock<Option<String>>> = OnceLock::new();

pub fn active_profile() -> Option<String> {
    ACTIVE_PROFILE
        .get()
        .and_then(|lock| lock.read().ok().and_then(|g| g.clone()))
}

/// Open (creating if needed) the horae SQLite database for a named profile and
/// run migrations. When `name` is `None`, the config's default profile is used.
///
/// Database files live under the XDG config dir (`~/.config/horae`); the legacy
/// `horae.db` is the default profile, so existing setups keep working.
pub fn open(name: Option<&str>) -> anyhow::Result<Connection> {
    let config = crate::config::Config::load()?;
    let (resolved_name, profile) = config.resolve_profile(name)?;
    if let Ok(mut guard) = ACTIVE_PROFILE.get_or_init(|| RwLock::new(None)).write() {
        *guard = Some(resolved_name.to_string());
    }
    let path = config.db_path(profile);

    let dir = path
        .parent()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    crate::fs_util::ensure_private_dir(&dir)?;

    let mut conn = Connection::open(&path)?;
    let _ = crate::fs_util::ensure_sqlite_files_private(&path);

    // 先设 busy_timeout，让随后的 journal_mode 切换（需要写锁）也能等待而不是
    // 立即报 `database is locked`；WAL 让 CLI/TUI/pomo 多进程并发读不互相阻塞。
    conn.busy_timeout(Duration::from_secs(5))?;
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;
         PRAGMA synchronous=NORMAL;
         PRAGMA cache_size=-2000;
         PRAGMA temp_store=MEMORY;",
    )?;

    migrate::run(&mut conn)?;
    Ok(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(unix)]
    fn test_conn_open_secures_directory_and_file_permissions() {
        use std::os::unix::fs::PermissionsExt;
        crate::testutil::with_test_config_dir(|| {
            let conn = open(None).expect("open connection");
            drop(conn);

            let cfg = crate::config::Config::load().expect("load config");
            let (_, profile) = cfg.resolve_profile(None).expect("resolve profile");
            let db_path = cfg.db_path(profile);
            let dir = db_path.parent().expect("db parent dir");

            let dir_meta = std::fs::metadata(dir).expect("dir metadata");
            assert_eq!(
                dir_meta.permissions().mode() & 0o777,
                0o700,
                "database directory must be 0700"
            );

            let db_meta = std::fs::metadata(&db_path).expect("db file metadata");
            assert_eq!(
                db_meta.permissions().mode() & 0o777,
                0o600,
                "database file must be 0600"
            );
        });
    }
}
