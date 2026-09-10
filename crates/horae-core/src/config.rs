use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use anyhow::Context;
use serde::{Deserialize, Serialize};

const CONFIG_FILE: &str = "config.json";
const DEFAULT_PROFILE: &str = "default";
const DEFAULT_DB: &str = "horae.db";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudConfig {
    pub url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_env: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub db: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cloud: Option<CloudConfig>,
    /// ntfy 手机提醒推送配置（可选；未配置则 watch 的 ntfy stage 为空操作）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ntfy: Option<NtfyConfig>,
    /// 飞书消息与提醒推送配置（可选；未配置则 watch 的 feishu stage 为空操作）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feishu: Option<FeishuConfig>,
}

/// ntfy 推送配置：桌面 `watch` 守护进程在任务到点前（默认 10 分钟）向手机发
/// 原生推送。凭据沿用 `CloudConfig` 惯例——token 只走环境变量，永不落盘。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NtfyConfig {
    /// ntfy 服务地址，如 `https://ntfy.sh`（自建填自己的域名）。
    pub url: String,
    /// 订阅主题（相当于口令，建议用随机串）。
    pub topic: String,
    /// 读取 Bearer token 的环境变量名（ntfy 主题设了访问令牌时填写）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token_env: Option<String>,
    /// 推送优先级 1–5（5 = 强制提醒），默认 5。
    #[serde(default = "default_ntfy_priority")]
    pub priority: u8,
    /// 提前多少分钟推送，默认 10。
    #[serde(default = "default_ntfy_lead")]
    pub lead_minutes: u64,
    /// ntfy `Tags` 头（emoji 短码，逗号分隔），可选。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tags: Option<String>,
}

fn default_ntfy_priority() -> u8 {
    5
}

fn default_ntfy_lead() -> u64 {
    10
}

/// 飞书 (Feishu / Lark) 消息与提醒推送配置。
/// 桌面 `watch` 守护进程在任务到点前（默认 10 分钟）向飞书 Webhook 机器人发送卡片消息。
/// 签名密钥沿用项目惯例——仅走环境变量，永不落盘。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeishuConfig {
    /// 飞书群自定义机器人 Webhook 地址（用于单向群通知/每日简报）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub webhook_url: Option<String>,
    /// 签名加签密钥（Secret）。支持直接在 config.json 中配置，也支持通过 secret_env 引用环境变量。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret: Option<String>,
    /// 读取签名加签密钥（Secret）的环境变量名（可选）。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub secret_env: Option<String>,
    /// 提前多少分钟推送，默认 10。
    #[serde(default = "default_feishu_lead")]
    pub lead_minutes: u64,
    /// 每日晨报推送时间（如 "08:30"），可选；配置后守护进程每日定时推送 Today 任务简报。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub daily_briefing: Option<String>,

    // ── Phase 2 自建应用与长连接双向交互配置 ──
    /// 飞书自建应用 App ID (如 cli_a1b2c3d4)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_id: Option<String>,
    /// 飞书自建应用 App Secret
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_secret: Option<String>,
    /// 读取 App Secret 的环境变量名 (如 FEISHU_APP_SECRET)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_secret_env: Option<String>,
    /// 是否开启 WebSocket 长连接随手记与卡片交互，默认 true (当配置了 app_id 时)
    #[serde(default = "default_true")]
    pub ws_enabled: bool,

    // ── Phase 2B 飞书原生任务同步配置 ──
    /// 是否开启飞书官方任务 (Tasks v2) 双向同步，默认 false
    #[serde(default)]
    pub task_sync: bool,
    /// 任务同步周期（分钟），默认 15
    #[serde(default = "default_sync_interval")]
    pub task_sync_interval_mins: u64,
}

impl FeishuConfig {
    /// 解析实际使用的 secret：
    /// 1. 优先读取显式配置的 `secret`；
    /// 2. 其次读取 `secret_env` 环境变量名对应的值；
    /// 3. 若 `secret_env` 无法在环境变量中找到（例如用户把密钥直接写在了 secret_env 里），智能回退使用该字符串。
    pub fn resolve_secret(&self) -> Option<String> {
        if let Some(sec) = &self.secret {
            let trimmed = sec.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        if let Some(env_val) = &self.secret_env {
            let trimmed = env_val.trim();
            if !trimmed.is_empty() {
                if let Ok(val) = std::env::var(trimmed) {
                    return Some(val);
                }
                return Some(trimmed.to_string());
            }
        }
        None
    }

    /// 解析飞书自建应用的 App Secret：
    /// 1. 优先读取显式配置的 `app_secret`；
    /// 2. 其次读取 `app_secret_env` 环境变量名对应的值；
    /// 3. 若 `app_secret_env` 无法在环境变量中找到，智能回退使用该字符串。
    pub fn resolve_app_secret(&self) -> Option<String> {
        if let Some(sec) = &self.app_secret {
            let trimmed = sec.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        if let Some(env_val) = &self.app_secret_env {
            let trimmed = env_val.trim();
            if !trimmed.is_empty() {
                if let Ok(val) = std::env::var(trimmed) {
                    return Some(val);
                }
                return Some(trimmed.to_string());
            }
        }
        None
    }

    /// 是否完整配置了自建应用（App ID 与 App Secret 皆具备）
    pub fn is_app_configured(&self) -> bool {
        self.app_id.as_ref().is_some_and(|id| !id.trim().is_empty())
            && self.resolve_app_secret().is_some()
    }
}

fn default_feishu_lead() -> u64 {
    10
}

fn default_true() -> bool {
    true
}

fn default_sync_interval() -> u64 {
    15
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_profile_name")]
    pub default_profile: String,
    #[serde(default)]
    pub profiles: BTreeMap<String, Profile>,
}

fn default_profile_name() -> String {
    DEFAULT_PROFILE.to_string()
}

/// horae config dir: `~/.config/horae` (falls back to `.` when no config dir).
/// `HORAE_CONFIG_DIR` env overrides it (used by tests / power users).
pub fn config_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os("HORAE_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("horae")
}

impl Config {
    pub fn path() -> PathBuf {
        config_dir().join(CONFIG_FILE)
    }

    /// Load config from disk. When the file is missing or malformed, fall back
    /// to a single default profile pointing at `horae.db` (backwards compatible).
    pub fn load() -> anyhow::Result<Self> {
        let path = Self::path();
        let raw = match fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e).context("read config"),
        };
        match serde_json::from_str(&raw) {
            Ok(cfg) => Ok(cfg),
            Err(e) => {
                eprintln!("warning: config.json malformed, using defaults ({e})");
                Ok(Self::default())
            }
        }
    }

    /// Atomically write config to disk (tmp file + rename).
    pub fn save(&self) -> anyhow::Result<()> {
        let dir = config_dir();
        fs::create_dir_all(&dir)?;
        let path = Self::path();
        let tmp = dir.join(format!("{CONFIG_FILE}.{}.tmp", std::process::id()));
        let json = serde_json::to_string_pretty(self)?;
        fs::write(&tmp, json)?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    pub fn profile(&self, name: &str) -> Option<&Profile> {
        self.profiles.get(name)
    }

    pub fn profile_names(&self) -> Vec<String> {
        self.profiles.keys().cloned().collect()
    }

    /// The profile the given name resolves to: the named profile when it exists,
    /// otherwise the default profile. Unknown names error.
    pub fn resolve_profile<'a>(
        &'a self,
        name: Option<&'a str>,
    ) -> anyhow::Result<(&'a str, &'a Profile)> {
        let key = name.unwrap_or(self.default_profile.as_str());
        match self.profiles.get(key) {
            Some(p) => Ok((key, p)),
            None => anyhow::bail!("unknown profile: {key}"),
        }
    }

    /// Absolute path of a profile's database file. Relative paths are resolved
    /// against the horae config dir.
    pub fn db_path(&self, profile: &Profile) -> PathBuf {
        let p = PathBuf::from(&profile.db);
        if p.is_absolute() {
            p
        } else {
            config_dir().join(p)
        }
    }

    pub fn upsert_profile(&mut self, name: &str, profile: Profile) {
        self.profiles.insert(name.to_string(), profile);
    }

    pub fn remove_profile(&mut self, name: &str) -> Option<Profile> {
        self.profiles.remove(name)
    }

    pub fn rename_profile(&mut self, from: &str, to: &str) -> anyhow::Result<()> {
        let profile = self
            .profiles
            .remove(from)
            .with_context(|| format!("unknown profile: {from}"))?;
        self.profiles.insert(to.to_string(), profile);
        if self.default_profile == from {
            self.default_profile = to.to_string();
        }
        Ok(())
    }

    pub fn set_default(&mut self, name: &str) -> anyhow::Result<()> {
        if !self.profiles.contains_key(name) {
            anyhow::bail!("unknown profile: {name}");
        }
        self.default_profile = name.to_string();
        Ok(())
    }
}

impl Default for Config {
    fn default() -> Self {
        let mut profiles = BTreeMap::new();
        profiles.insert(
            DEFAULT_PROFILE.to_string(),
            Profile {
                db: DEFAULT_DB.to_string(),
                cloud: None,
                ntfy: None,
                feishu: None,
            },
        );
        Self {
            default_profile: DEFAULT_PROFILE.to_string(),
            profiles,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_missing_file_falls_back_to_default() {
        // 隔离到空配置目录：既避免读到真实用户配置，也不与其它
        // 依赖 HORAE_CONFIG_DIR 的并行测试竞争。
        crate::testutil::with_test_config_dir(|| {
            let cfg = Config::load().unwrap();
            assert_eq!(cfg.default_profile, "default");
            assert_eq!(cfg.profile_names(), vec!["default"]);
            assert_eq!(
                cfg.profile("default").unwrap().db,
                "horae.db",
                "默认 profile 指向旧 horae.db，向后兼容"
            );
        });
    }

    #[test]
    fn save_and_reload_round_trip() {
        let mut cfg = Config::default();
        cfg.upsert_profile(
            "work",
            Profile {
                db: "profiles/work.db".to_string(),
                cloud: None,
                ntfy: None,
                feishu: None,
            },
        );
        cfg.upsert_profile(
            "personal",
            Profile {
                db: "profiles/personal.db".to_string(),
                cloud: Some(CloudConfig {
                    url: "libsql://example.turso.io".to_string(),
                    token_env: Some("HORAE_TURSO_TOKEN".to_string()),
                }),
                ntfy: None,
                feishu: None,
            },
        );
        cfg.set_default("work").unwrap();

        let json = serde_json::to_string_pretty(&cfg).unwrap();
        let back: Config = serde_json::from_str(&json).unwrap();
        assert_eq!(back.default_profile, "work");
        assert_eq!(
            back.profile("personal")
                .unwrap()
                .cloud
                .as_ref()
                .unwrap()
                .url,
            "libsql://example.turso.io"
        );
        assert!(back
            .profile("personal")
            .unwrap()
            .cloud
            .as_ref()
            .unwrap()
            .token_env
            .is_some());
        assert_eq!(back.profile("work").unwrap().db, "profiles/work.db");
    }

    #[test]
    fn db_path_keeps_profile_file() {
        // 断言含 `horae/` 前缀，必须保证 HORAE_CONFIG_DIR 未被并行测试设置
        crate::testutil::with_no_config_dir(|| {
            let mut config = Config::default();
            config.upsert_profile(
                "p",
                Profile {
                    db: "profiles/p.db".to_string(),
                    cloud: None,
                    ntfy: None,
                    feishu: None,
                },
            );
            let path = config.db_path(config.profile("p").unwrap());
            assert!(path.to_string_lossy().ends_with("horae/profiles/p.db"));
        });
    }

    #[test]
    fn resolve_profile_unknown_name_errors() {
        let cfg = Config::default();
        assert!(cfg.resolve_profile(Some("nope")).is_err());
        let (name, p) = cfg.resolve_profile(None).unwrap();
        assert_eq!(name, "default");
        assert_eq!(p.db, "horae.db");
    }

    #[test]
    fn db_path_relative_resolves_under_config_dir() {
        // 隔离 HORAE_CONFIG_DIR，避免并行测试污染导致路径断言不稳定。
        crate::testutil::with_no_config_dir(|| {
            let cfg = Config::default();
            let p = cfg.db_path(cfg.profile("default").unwrap());
            assert!(p.is_absolute());
            assert!(p.to_string_lossy().ends_with("horae/horae.db"));
        });
    }
}
