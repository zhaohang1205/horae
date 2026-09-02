use anyhow::Context;

use crate::cli::ProfileAction;
use horae_core::config::{Config, Profile};

/// Run a profile-management action. Operates only on config.json — no database
/// is opened, so these are safe to run while other horae processes are active.
pub fn run(action: ProfileAction) -> anyhow::Result<()> {
    let mut config = Config::load()?;
    match action {
        ProfileAction::List => list(&config),
        ProfileAction::New { name, db } => new(&mut config, &name, db.as_deref()),
        ProfileAction::Rename { from, to } => rename(&mut config, &from, &to),
        ProfileAction::Rm { name } => rm(&mut config, &name),
        ProfileAction::SetDefault { name } => set_default(&mut config, &name),
    }
}

fn list(config: &Config) -> anyhow::Result<()> {
    let names = config.profile_names();
    if names.is_empty() {
        println!("(no profiles)");
        return Ok(());
    }
    let mut table = comfy_table::Table::new();
    table
        .load_preset(comfy_table::presets::NOTHING)
        .set_content_arrangement(comfy_table::ContentArrangement::Dynamic);

    for name in names {
        let profile = config.profile(&name).expect("listed name must exist");
        let default = if config.default_profile == name {
            "(default)"
        } else {
            ""
        };
        let cloud = profile
            .cloud
            .as_ref()
            .map(|c| format!("cloud={}", c.url))
            .unwrap_or_default();
        let db_str = format!("db={}", profile.db);
        table.add_row(vec![&name, &db_str, &cloud, default]);
    }
    println!("{table}");
    Ok(())
}

fn new(config: &mut Config, name: &str, db: Option<&str>) -> anyhow::Result<()> {
    if config.profiles.contains_key(name) {
        anyhow::bail!("profile already exists: {name}");
    }
    let owned;
    let db = match db {
        Some(d) => d,
        None => {
            owned = format!("profiles/{name}.db");
            &owned
        }
    };
    config.upsert_profile(
        name,
        Profile {
            db: db.to_string(),
            cloud: None,
            ntfy: None,
        },
    );
    config.save()?;
    println!("created profile `{name}` (db={db})");
    Ok(())
}

fn rename(config: &mut Config, from: &str, to: &str) -> anyhow::Result<()> {
    config.rename_profile(from, to)?;
    config.save()?;
    println!("renamed profile `{from}` -> `{to}`");
    Ok(())
}

fn rm(config: &mut Config, name: &str) -> anyhow::Result<()> {
    if config.profiles.len() <= 1 {
        anyhow::bail!("cannot remove the last profile");
    }
    config
        .remove_profile(name)
        .with_context(|| format!("unknown profile: {name}"))?;
    if config.default_profile == name {
        config.default_profile = config
            .profiles
            .keys()
            .next()
            .cloned()
            .unwrap_or_else(|| "default".to_string());
    }
    config.save()?;
    println!("removed profile `{name}` (database file kept)");
    Ok(())
}

fn set_default(config: &mut Config, name: &str) -> anyhow::Result<()> {
    config.set_default(name)?;
    config.save()?;
    println!("default profile is now `{name}`");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::ProfileAction;
    use horae_core::config::CloudConfig;

    /// 把配置写入重定向到临时目录（沿用项目 HORAE_CONFIG_DIR 惯例，
    /// 由 testutil 统一持锁 + panic 安全恢复）。
    fn with_config_dir(f: impl FnOnce()) {
        horae_core::testutil::with_test_config_dir(f);
    }

    #[test]
    fn new_rm_rename_set_default_update_config() {
        let mut config = Config::default();
        config.upsert_profile(
            "work",
            Profile {
                db: "profiles/work.db".to_string(),
                cloud: Some(CloudConfig {
                    url: "libsql://x.turso.io".to_string(),
                    token_env: None,
                }),
                ntfy: None,
            },
        );
        assert!(config.profile("work").unwrap().cloud.is_some());
        config.set_default("work").unwrap();
        assert_eq!(config.default_profile, "work");
        config.rename_profile("work", "work2").unwrap();
        assert_eq!(config.default_profile, "work2");
        config.remove_profile("work2").unwrap();
        assert!(!config.profiles.contains_key("work2"));
        assert_eq!(
            config.default_profile, "work2",
            "remove_profile 不动 default；default 改派由 rm 命令处理"
        );
    }

    #[test]
    fn db_path_keeps_profile_file() {
        horae_core::testutil::with_no_config_dir(|| {
            let mut config = Config::default();
            config.upsert_profile(
                "p",
                Profile {
                    db: "profiles/p.db".to_string(),
                    cloud: None,
                    ntfy: None,
                },
            );
            let path = config.db_path(config.profile("p").unwrap());
            assert!(path.to_string_lossy().ends_with("horae/profiles/p.db"));
        });
    }

    #[test]
    fn new_defaults_db_path_and_rejects_duplicates() {
        with_config_dir(|| {
            run(ProfileAction::New {
                name: "work".into(),
                db: None,
            })
            .unwrap();

            let config = Config::load().unwrap();
            let p = config.profile("work").unwrap();
            assert_eq!(
                p.db, "profiles/work.db",
                "缺省 db 路径应为 profiles/<name>.db"
            );
            assert!(p.cloud.is_none());

            // 重名创建应被拒绝
            let err = run(ProfileAction::New {
                name: "work".into(),
                db: None,
            })
            .unwrap_err();
            assert!(err.to_string().contains("already exists"), "{}", err);
        });
    }

    #[test]
    fn new_accepts_custom_db_path() {
        with_config_dir(|| {
            run(ProfileAction::New {
                name: "x".into(),
                db: Some("/abs/custom.db".into()),
            })
            .unwrap();
            let config = Config::load().unwrap();
            assert_eq!(config.profile("x").unwrap().db, "/abs/custom.db");
        });
    }

    #[test]
    fn rm_rejects_last_profile_and_unknown_profile() {
        with_config_dir(|| {
            // 默认配置只有 default 一个 profile，不允许删除
            let err = run(ProfileAction::Rm {
                name: "default".into(),
            })
            .unwrap_err();
            assert!(
                err.to_string().contains("cannot remove the last profile"),
                "{}",
                err
            );

            // 不存在的 profile
            run(ProfileAction::New {
                name: "work".into(),
                db: None,
            })
            .unwrap();
            let err = run(ProfileAction::Rm {
                name: "ghost".into(),
            })
            .unwrap_err();
            assert!(err.to_string().contains("unknown profile"), "{}", err);
        });
    }

    #[test]
    fn rm_default_reassigns_to_remaining_profile() {
        with_config_dir(|| {
            run(ProfileAction::New {
                name: "work".into(),
                db: None,
            })
            .unwrap();
            run(ProfileAction::Rm {
                name: "default".into(),
            })
            .unwrap();

            let config = Config::load().unwrap();
            assert!(!config.profiles.contains_key("default"));
            assert_eq!(
                config.default_profile, "work",
                "删除默认 profile 后应改派到剩余 profile"
            );
        });
    }

    #[test]
    fn rename_moves_default_and_rejects_unknown() {
        with_config_dir(|| {
            let err = run(ProfileAction::Rename {
                from: "ghost".into(),
                to: "main".into(),
            })
            .unwrap_err();
            assert!(err.to_string().contains("unknown profile"), "{}", err);

            run(ProfileAction::Rename {
                from: "default".into(),
                to: "main".into(),
            })
            .unwrap();
            let config = Config::load().unwrap();
            assert!(config.profile("main").is_some());
            assert!(!config.profiles.contains_key("default"));
            assert_eq!(
                config.default_profile, "main",
                "重命名默认 profile 后 default 指针应跟随"
            );
        });
    }

    #[test]
    fn set_default_persists_and_rejects_unknown() {
        with_config_dir(|| {
            let err = run(ProfileAction::SetDefault {
                name: "ghost".into(),
            })
            .unwrap_err();
            assert!(err.to_string().contains("unknown profile"), "{}", err);

            run(ProfileAction::New {
                name: "work".into(),
                db: None,
            })
            .unwrap();
            run(ProfileAction::SetDefault {
                name: "work".into(),
            })
            .unwrap();
            assert_eq!(Config::load().unwrap().default_profile, "work");
        });
    }
}
