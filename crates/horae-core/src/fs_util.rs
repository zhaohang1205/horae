use std::path::{Path, PathBuf};

/// 检查路径的所有上级祖先目录，拒绝任何不安全的祖先：
/// 1. 祖先目录不能为符号链接（防路径劫持）
/// 2. 祖先目录必须为目录类型
/// 3. 全局可写（world-writable，如 /tmp）目录必须设置粘滞位 (Sticky bit: 01000 / S_ISVTX)
/// 4. 祖先目录所有者必须为 root (0) 或当前用户 (expected_uid)
#[cfg(unix)]
pub fn check_safe_ancestors(path: &Path, expected_uid: u32) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::PermissionsExt;

    for ancestor in path.ancestors().skip(1) {
        if ancestor.as_os_str().is_empty() {
            continue;
        }
        let meta = match std::fs::symlink_metadata(ancestor) {
            Ok(m) => m,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };

        if meta.file_type().is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "security check failed: ancestor '{}' is a symlink",
                    ancestor.display()
                ),
            ));
        }

        if !meta.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "security check failed: ancestor '{}' is not a directory",
                    ancestor.display()
                ),
            ));
        }

        let mode = meta.permissions().mode();
        let uid = meta.uid();

        // 全局可写目录若未设置粘滞位，其他用户可任意重命名或替换目录项
        if mode & 0o002 != 0 && mode & 0o1000 == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "security check failed: ancestor '{}' is world-writable without sticky bit (mode {:o})",
                    ancestor.display(),
                    mode & 0o7777
                ),
            ));
        }

        // 祖先目录所有者必须为 root (0) 或当前用户 (expected_uid)
        if uid != 0 && uid != expected_uid {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "security check failed: ancestor '{}' is owned by uid {}, expected 0 or {}",
                    ancestor.display(),
                    uid,
                    expected_uid
                ),
            ));
        }
    }
    Ok(())
}

/// 针对指定 UID 确保目录存在且具有私有访问权限 (Unix 0700: rwx------)。
/// 校验所有上级祖先安全性，若目录不存在则逐级创建为 0700。
/// 若目录已存在，则验证不是符号链接、属于 expected_uid，并收紧权限传播错误。
#[cfg(unix)]
pub fn ensure_private_dir_for_uid(dir: &Path, expected_uid: u32) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::PermissionsExt;

    // 1. 递归校验所有既有祖先目录的安全性
    check_safe_ancestors(dir, expected_uid)?;

    // 2. 逐级创建不存在的目录路径，确保每个新建的层级均为 0700
    if !dir.exists() {
        let mut cur = PathBuf::new();
        for component in dir.components() {
            cur.push(component);
            if !cur.exists() {
                let mut b = std::fs::DirBuilder::new();
                b.mode(0o700);
                match b.create(&cur) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(e) => return Err(e),
                }
            }
        }
    }

    // 3. 严格校验目标目录的元数据与所有权
    let metadata = std::fs::symlink_metadata(dir)?;
    if metadata.file_type().is_symlink() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("security check failed: '{}' is a symlink", dir.display()),
        ));
    }
    if !metadata.is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "security check failed: '{}' is not a directory",
                dir.display()
            ),
        ));
    }
    if metadata.uid() != expected_uid {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!(
                "security check failed: '{}' is owned by uid {}, expected {}",
                dir.display(),
                metadata.uid(),
                expected_uid
            ),
        ));
    }

    // 4. 收紧权限至 0700，并传播权限设置错误
    let mut perms = metadata.permissions();
    if perms.mode() & 0o077 != 0 {
        perms.set_mode(perms.mode() & !0o077);
        std::fs::set_permissions(dir, perms)?;
    }
    Ok(())
}

/// 确保目录存在且具有私有访问权限 (Unix 0700: rwx------)。
/// 若目录不存在则递归创建并设置 0700 权限。
/// 若目录已存在，则验证不是符号链接、属于当前用户，并收紧其权限去除 group 和 other 的访问权限。
pub fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let current_uid = unsafe { libc::getuid() };
        ensure_private_dir_for_uid(dir, current_uid)
    }
    #[cfg(not(unix))]
    {
        std::fs::create_dir_all(dir)
    }
}

/// 针对指定 UID 确保已有文件具有私有访问权限 (Unix 0600: rw-------)。
#[cfg(unix)]
pub fn ensure_private_file_for_uid(path: &Path, expected_uid: u32) -> std::io::Result<()> {
    use std::os::unix::fs::MetadataExt;
    use std::os::unix::fs::PermissionsExt;

    if path.exists() {
        let metadata = std::fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("security check failed: '{}' is a symlink", path.display()),
            ));
        }
        if metadata.is_dir() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!("security check failed: '{}' is a directory", path.display()),
            ));
        }
        if metadata.uid() != expected_uid {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                format!(
                    "security check failed: '{}' is owned by uid {}, expected {}",
                    path.display(),
                    metadata.uid(),
                    expected_uid
                ),
            ));
        }
        let mut perms = metadata.permissions();
        if perms.mode() & 0o077 != 0 {
            perms.set_mode(perms.mode() & !0o077);
            std::fs::set_permissions(path, perms)?;
        }
    }
    Ok(())
}

/// 确保已有文件具有私有访问权限 (Unix 0600: rw-------)。
pub fn ensure_private_file(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        let current_uid = unsafe { libc::getuid() };
        ensure_private_file_for_uid(path, current_uid)
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(())
    }
}

/// 在指定临时父目录下创建附带 UID 的私有临时子目录 (/tmp/horae-<uid>/<subdir>)，
/// 校验 base_temp 祖先安全性，确保 fallback parent 本身及 subdir 均具有 0700 权限且所有者为 uid。
#[cfg(unix)]
pub fn private_temp_dir_in(base_temp: &Path, subdir: &str, uid: u32) -> std::io::Result<PathBuf> {
    // 1. 验证 base_temp 本身及其所有祖先目录的安全性
    check_safe_ancestors(&base_temp.join("dummy"), uid)?;

    // 2. 确保回退父目录 (/tmp/horae-<uid>) 本身为受害者所有且权限为 0700
    let fallback_parent = base_temp.join(format!("horae-{}", uid));
    ensure_private_dir_for_uid(&fallback_parent, uid)?;

    // 3. 在受害者独占的父目录下创建并确保 subdir 为 0700
    let leaf = fallback_parent.join(subdir);
    ensure_private_dir_for_uid(&leaf, uid)?;
    Ok(leaf)
}

/// 获取或创建具有严格私有权限 (Unix 0700: rwx------) 的运行时/临时工作目录。
/// 优先使用用户专属运行时目录 ($XDG_RUNTIME_DIR/horae/<subdir>)，在现代 Linux 上通常为基于内存的
/// tmpfs (/run/user/<uid>)，权限原生即为 0700，速度快且不落磁盘。
/// 若运行时目录不可用，则安全回退至系统临时目录下附带 UID 的私有目录 (/tmp/horae-<uid>/<subdir>)，
/// 严密确保回退父目录本身为当前用户独占拥有的 0700 目录，拒绝任何不安全祖先与被攻击者抢占的父/叶节点。
pub fn private_temp_dir(subdir: &str) -> std::io::Result<PathBuf> {
    #[cfg(unix)]
    {
        let uid = unsafe { libc::getuid() };
        if let Some(runtime_dir) = dirs::runtime_dir() {
            let parent = runtime_dir.join("horae");
            let dir = parent.join(subdir);
            if ensure_private_dir_for_uid(&parent, uid).is_ok()
                && ensure_private_dir_for_uid(&dir, uid).is_ok()
            {
                return Ok(dir);
            }
        }

        private_temp_dir_in(&std::env::temp_dir(), subdir, uid)
    }
    #[cfg(not(unix))]
    {
        if let Some(runtime_dir) = dirs::runtime_dir() {
            let dir = runtime_dir.join("horae").join(subdir);
            if ensure_private_dir(&dir).is_ok() {
                return Ok(dir);
            }
        }
        let base = std::env::temp_dir().join("horae").join(subdir);
        ensure_private_dir(&base)?;
        Ok(base)
    }
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
            .custom_flags(libc::O_NOFOLLOW)
            .mode(0o600)
            .open(path)?;
        file.write_all(contents.as_ref())?;
        file.flush()?;
        drop(file);
        ensure_private_file(path)?;
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
    if wal.exists() {
        ensure_private_file(&wal)?;
    }
    if shm.exists() {
        ensure_private_file(&shm)?;
    }
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

    #[test]
    #[cfg(unix)]
    fn test_reject_attacker_owned_parent_and_leaf() {
        let temp = tempdir().unwrap();
        let current_uid = unsafe { libc::getuid() };
        let attacker_uid = current_uid + 999;

        // 1. Attacker pre-creates fallback parent
        let fallback_parent = temp.path().join(format!("horae-{}", current_uid));
        std::fs::create_dir(&fallback_parent).unwrap();

        // Calling ensure_private_dir_for_uid with current_uid works for victim
        assert!(ensure_private_dir_for_uid(&fallback_parent, current_uid).is_ok());

        // But when validated against victim uid, if it were owned by attacker, it is rejected
        let err = ensure_private_dir_for_uid(&fallback_parent, attacker_uid).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(err.to_string().contains("security check failed"));

        // 2. Attacker-owned leaf
        let leaf = fallback_parent.join("voice");
        std::fs::create_dir(&leaf).unwrap();
        let err_leaf = ensure_private_dir_for_uid(&leaf, attacker_uid).unwrap_err();
        assert_eq!(err_leaf.kind(), std::io::ErrorKind::PermissionDenied);

        // 3. private_temp_dir_in with attacker fallback parent
        let err_fallback = private_temp_dir_in(temp.path(), "voice", attacker_uid).unwrap_err();
        assert_eq!(err_fallback.kind(), std::io::ErrorKind::PermissionDenied);
    }

    #[test]
    #[cfg(unix)]
    fn test_reject_symlink_parent_and_leaf() {
        let temp = tempdir().unwrap();
        let current_uid = unsafe { libc::getuid() };

        let real_dir = temp.path().join("real_target");
        std::fs::create_dir(&real_dir).unwrap();

        // 1. Fallback parent is a symlink
        let symlink_parent = temp.path().join(format!("horae-{}", current_uid));
        std::os::unix::fs::symlink(&real_dir, &symlink_parent).unwrap();

        let err = ensure_private_dir_for_uid(&symlink_parent, current_uid).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(err.to_string().contains("is a symlink"));

        let err_temp = private_temp_dir_in(temp.path(), "voice", current_uid).unwrap_err();
        assert_eq!(err_temp.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(err_temp.to_string().contains("is a symlink"));

        // 2. Leaf is a symlink
        std::fs::remove_file(&symlink_parent).unwrap();
        std::fs::create_dir(&symlink_parent).unwrap();
        let symlink_leaf = symlink_parent.join("voice");
        std::os::unix::fs::symlink(&real_dir, &symlink_leaf).unwrap();

        let err_leaf = private_temp_dir_in(temp.path(), "voice", current_uid).unwrap_err();
        assert_eq!(err_leaf.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(err_leaf.to_string().contains("is a symlink"));
    }

    #[test]
    #[cfg(unix)]
    fn test_reject_unsafe_ancestors() {
        use std::os::unix::fs::PermissionsExt;
        let temp = tempdir().unwrap();
        let current_uid = unsafe { libc::getuid() };

        let unsafe_ancestor = temp.path().join("insecure_base");
        std::fs::create_dir(&unsafe_ancestor).unwrap();

        // Set mode 0777 without sticky bit (0o1000)
        let mut perms = std::fs::metadata(&unsafe_ancestor).unwrap().permissions();
        perms.set_mode(0o777);
        std::fs::set_permissions(&unsafe_ancestor, perms).unwrap();

        let target = unsafe_ancestor.join("sub/target");
        let err = check_safe_ancestors(&target, current_uid).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(err
            .to_string()
            .contains("world-writable without sticky bit"));

        // When sticky bit is added (0o1777), check passes
        let mut sticky_perms = std::fs::metadata(&unsafe_ancestor).unwrap().permissions();
        sticky_perms.set_mode(0o1777);
        std::fs::set_permissions(&unsafe_ancestor, sticky_perms).unwrap();
        assert!(check_safe_ancestors(&target, current_uid).is_ok());

        // Ancestor owned by untrusted UID
        let untrusted_uid = current_uid + 1;
        let err_uid = check_safe_ancestors(&target, untrusted_uid).unwrap_err();
        assert_eq!(err_uid.kind(), std::io::ErrorKind::PermissionDenied);
        assert!(err_uid.to_string().contains("owned by uid"));
    }
}
