use std::marker::PhantomData;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use serde::de::DeserializeOwned;
use serde::Serialize;

/// 测试隔离：置为 true 时 `JsonStateStore::new` 构造出的存储 `load`/`save`
/// 均空操作，避免测试读到真实 `~/.config/horae/*.json`（如运行中的 pomo daemon
/// 状态）。单一全局开关：alarm/notify/pomo 三个状态文件在测试里都不该碰真实
/// 文件。开关在构造时采样（sticky），TUI/backup 测试开头置一次即可。
static TEST_OVERRIDE: AtomicBool = AtomicBool::new(false);

/// 测试隔离开关（见 [`TEST_OVERRIDE`]）。
#[cfg(any(test, feature = "test-util"))]
pub fn set_test_override() {
    TEST_OVERRIDE.store(true, Ordering::Relaxed);
}

/// 一个 JSON 状态文件存储：`~/.config/horae/<filename>` 的 load/save。
/// 文件缺失时 `load` 返回 `T::default()`；写入用 tmp+rename 原子替换，
/// 避免并发读方读到半写的文件。
pub struct JsonStateStore<T> {
    path: PathBuf,
    override_on: bool,
    _marker: PhantomData<T>,
}

impl<T: Default + Serialize + DeserializeOwned> JsonStateStore<T> {
    /// 构造指向 `~/.config/horae/<filename>` 的存储（目录不存在则创建）。
    /// 测试期间（`TEST_OVERRIDE` 置位后）构造的存储 load/save 均为空操作。
    pub fn new(filename: &str) -> Self {
        let mut path = crate::config::config_dir();
        let _ = std::fs::create_dir_all(&path);
        path.push(filename);
        Self {
            path,
            override_on: TEST_OVERRIDE.load(Ordering::Relaxed),
            _marker: PhantomData,
        }
    }

    /// 构造指向显式路径的存储（测试用，永不进入 override 状态，
    /// 也绕开全局开关，与并行测试互不干扰）。
    #[cfg(test)]
    pub fn at(path: PathBuf) -> Self {
        Self {
            path,
            override_on: false,
            _marker: PhantomData,
        }
    }

    /// 读取状态；文件缺失时返回 `T::default()`。
    pub fn load(&self) -> Result<T> {
        if self.override_on {
            return Ok(T::default());
        }
        if !self.path.exists() {
            return Ok(T::default());
        }
        let content = std::fs::read_to_string(&self.path)?;
        Ok(serde_json::from_str(&content)?)
    }

    /// 原子写入状态：先写临时文件再 rename。
    pub fn save(&self, state: &T) -> Result<()> {
        if self.override_on {
            return Ok(());
        }
        let content = serde_json::to_string_pretty(state)?;
        let tmp = self.path.with_extension("json.tmp");
        std::fs::write(&tmp, content)?;
        std::fs::rename(&tmp, &self.path)?;
        Ok(())
    }

    /// 状态文件的完整路径（供测试断言文件确实落在预期位置）。
    #[cfg(test)]
    pub fn file_path(&self) -> &std::path::Path {
        &self.path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Default, PartialEq, Serialize, Deserialize)]
    struct Fake {
        n: u32,
    }

    /// 返回 (store, dir)：dir 保持存活，避免临时目录被提前清理。
    fn temp_store(name: &str) -> (JsonStateStore<Fake>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let store = JsonStateStore::at(dir.path().join(name));
        (store, dir)
    }

    #[test]
    fn load_missing_file_returns_default() {
        let (store, _dir) = temp_store("missing.json");
        assert_eq!(store.load().unwrap(), Fake::default());
    }

    #[test]
    fn save_then_load_roundtrips() {
        let (store, _dir) = temp_store("roundtrip.json");
        store.save(&Fake { n: 7 }).unwrap();
        assert_eq!(store.load().unwrap(), Fake { n: 7 });
        assert!(store.file_path().exists(), "文件已写入");
    }

    #[test]
    fn save_overwrites_existing() {
        let (store, _dir) = temp_store("overwrite.json");
        store.save(&Fake { n: 1 }).unwrap();
        store.save(&Fake { n: 2 }).unwrap();
        assert_eq!(store.load().unwrap(), Fake { n: 2 });
    }

    #[test]
    fn override_store_loads_default_and_noops_save() {
        set_test_override();
        // 构造时采样：override 置位后 new 出的存储进入隔离态。
        let store: JsonStateStore<Fake> = JsonStateStore::new("unit-override.json");
        assert_eq!(
            store.load().unwrap(),
            Fake::default(),
            "开关下 load 返回 default"
        );
        store.save(&Fake { n: 7 }).unwrap();
        assert!(!store.file_path().exists(), "测试期间不写真实文件");
    }
}
