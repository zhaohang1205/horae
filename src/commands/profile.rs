use anyhow::Context;

use crate::cli::ProfileAction;
use crate::config::{Config, Profile};

/// Run a profile-management action. Operates only on config.json — no database
/// is opened, so these are safe to run while other gtp processes are active.
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
    for name in names {
        let profile = config.profile(&name).expect("listed name must exist");
        let default = if config.default_profile == name {
            "  (default)"
        } else {
            ""
        };
        let cloud = profile
            .cloud
            .as_ref()
            .map(|c| format!("  cloud={}", c.url))
            .unwrap_or_default();
        println!("{name:12} db={}{}{}", profile.db, cloud, default);
    }
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
    use crate::config::CloudConfig;

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
        let mut config = Config::default();
        config.upsert_profile(
            "p",
            Profile {
                db: "profiles/p.db".to_string(),
                cloud: None,
            },
        );
        let path = config.db_path(config.profile("p").unwrap());
        assert!(path.to_string_lossy().ends_with("gtp/profiles/p.db"));
    }
}
