use std::path::{Path, PathBuf};

/// 确保目录存在且具有私有访问权限 (Unix 0700: rwx------)。
/// 若目录不存在则递归创建并设置 0700 权限。
/// 若目录已存在，则验证不是符号链接、属于当前用户，并收紧其权限去除 group 和 other 的访问权限。
pub fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        use std::os::unix::fs::MetadataExt;
        use std::os::unix::fs::PermissionsExt;

        if !dir.exists() {
            let mut builder = std::fs::DirBuilder::new();
            builder.recursive(true);
            builder.mode(0o700);
            builder.create(dir)?;
        }
        if let Ok(metadata) = std::fs::symlink_metadata(dir) {
            if metadata.file_type().is_symlink() {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!("security check failed: '{}' is a symlink", dir.display()),
                ));
            }
            let current_uid = unsafe { libc::getuid() };
            if metadata.uid() != current_uid {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    format!(
                        "security check failed: '{}' is owned by uid {}, expected {}",
                        dir.display(),
                        metadata.uid(),
                        current_uid
                    ),
                ));
            }
            let mut perms = metadata.permissions();
            if perms.mode() & 0o077 != 0 {
                perms.set_mode(perms.mode() & !0o077);
                let _ = std::fs::set_permissions(dir, perms);
            }
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// 确保已有文件具有私有访问权限 (Unix 0600: rw-------)。
pub fn ensure_private_file(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        use std::os::unix::fs::PermissionsExt;
        if path.exists() {
            if let Ok(metadata) = std::fs::symlink_metadata(path) {
                if metadata.file_type().is_symlink() {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!("security check failed: '{}' is a symlink", path.display()),
                    ));
                }
                let current_uid = unsafe { libc::getuid() };
                if metadata.uid() != current_uid {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        format!(
                            "security check failed: '{}' is owned by uid {}, expected {}",
                            path.display(),
                            metadata.uid(),
                            current_uid
                        ),
                    ));
                }
                let mut perms = metadata.permissions();
                if perms.mode() & 0o077 != 0 {
                    perms.set_mode(perms.mode() & !0o077);
                    let _ = std::fs::set_permissions(path, perms);
                }
            }
        }
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// 获取或创建具有严格私有权限 (Unix 0700: rwx------) 的运行时/临时工作目录。
/// 优先使用用户专属运行时目录 ($XDG_RUNTIME_DIR/horae/<subdir>)，在现代 Linux 上通常为基于内存的
/// tmpfs (/run/user/<uid>)，权限原生即为 0700，速度快且不落磁盘。
/// 若运行时目录不可用，则安全回退至系统临时目录下附带 UID 的私有目录 (/tmp/horae-<uid>/<subdir>)，
/// 并强制进行防符号链接劫持与所有者安全校验。
pub fn private_temp_dir(subdir: &str) -> std::io::Result<PathBuf> {
    if let Some(runtime_dir) = dirs::runtime_dir() {
        let dir = runtime_dir.join("horae").join(subdir);
        if ensure_private_dir(&dir).is_ok() {
            return Ok(dir);
        }
    }

    #[cfg(unix)]
    let base = {
        let uid = unsafe { libc::getuid() };
        std::env::temp_dir()
            .join(format!("horae-{}", uid))
            .join(subdir)
    };
    #[cfg(not(unix))]
    let base = std::env::temp_dir().join("horae").join(subdir);

    ensure_private_dir(&base)?;
    Ok(base)
}

/// 写入私有文件，确保新建文件以 0600 模式打开创建并刷新。
pub fn write_private_file(path: &Path, contents: impl AsRef<[u8]>) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(contents.as_ref())?;
        file.flush()?;
        drop(file);
        let _ = ensure_private_file(path);
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, contents)
    }
}

/// 针对 SQLite 数据库文件及其 WAL/SHM 辅助文件加固权限 (0600)。
pub fn ensure_sqlite_files_private(path: &Path) -> std::io::Result<()> {
    ensure_private_file(path)?;
    let p_str = path.to_string_lossy();
    let wal = PathBuf::from(format!("{p_str}-wal"));
    let shm = PathBuf::from(format!("{p_str}-shm"));
    let _ = ensure_private_file(&wal);
    let _ = ensure_private_file(&shm);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    #[cfg(unix)]
    fn test_ensure_private_dir_and_file() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempdir().unwrap();
        let target_dir = temp.path().join("sub/private_dir");

        ensure_private_dir(&target_dir).unwrap();
        assert!(target_dir.is_dir());
        let meta = std::fs::metadata(&target_dir).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o700);

        // Relax permissions to 0755, then re-ensure
        let mut perms = meta.permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&target_dir, perms).unwrap();
        ensure_private_dir(&target_dir).unwrap();
        assert_eq!(
            std::fs::metadata(&target_dir).unwrap().permissions().mode() & 0o777,
            0o700
        );

        // Test private file
        let file_path = target_dir.join("test.txt");
        write_private_file(&file_path, b"secret data").unwrap();
        assert_eq!(
            std::fs::metadata(&file_path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        // Relax file permissions to 0644, then re-ensure
        let mut fperms = std::fs::metadata(&file_path).unwrap().permissions();
        fperms.set_mode(0o644);
        std::fs::set_permissions(&file_path, fperms).unwrap();
        ensure_private_file(&file_path).unwrap();
        assert_eq!(
            std::fs::metadata(&file_path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        // Relax file permissions to 0644, then overwrite via write_private_file
        let mut fperms2 = std::fs::metadata(&file_path).unwrap().permissions();
        fperms2.set_mode(0o644);
        std::fs::set_permissions(&file_path, fperms2).unwrap();
        write_private_file(&file_path, b"updated secret").unwrap();
        assert_eq!(
            std::fs::metadata(&file_path).unwrap().permissions().mode() & 0o777,
            0o600
        );

        // Test symlink rejection for directory
        let real_dir = temp.path().join("real_dir");
        std::fs::create_dir(&real_dir).unwrap();
        let link_dir = temp.path().join("symlink_dir");
        std::os::unix::fs::symlink(&real_dir, &link_dir).unwrap();
        assert!(ensure_private_dir(&link_dir).is_err());

        // Test symlink rejection for file
        let real_file = temp.path().join("real_file.txt");
        std::fs::write(&real_file, b"test").unwrap();
        let link_file = temp.path().join("symlink_file.txt");
        std::os::unix::fs::symlink(&real_file, &link_file).unwrap();
        assert!(ensure_private_file(&link_file).is_err());
    }

    #[test]
    #[cfg(unix)]
    fn test_private_temp_dir() {
        use std::os::unix::fs::PermissionsExt;
        let voice_dir = private_temp_dir("test_voice").unwrap();
        assert!(voice_dir.is_dir());
        let meta = std::fs::metadata(&voice_dir).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o700);
        let _ = std::fs::remove_dir_all(&voice_dir);
    }
}
