//! `horae todoist` command-line integration.

use anyhow::{bail, Result};
use rusqlite::Connection;

use crate::cli::TodoistAction;
use horae_core::config::{Config, TodoistConfig};

pub fn run(conn: &Connection, action: TodoistAction, profile: Option<&str>) -> Result<()> {
    match action {
        TodoistAction::Configure {
            project_id,
            token_env,
        } => {
            if project_id.trim().is_empty() || token_env.trim().is_empty() {
                bail!("project id and token environment variable must not be empty");
            }
            let mut config = Config::load()?;
            let (name, profile_config) = config.resolve_profile(profile)?;
            let name = name.to_string();
            let reset_sync = profile_config.todoist.as_ref().is_some_and(|old| {
                old.project_id != project_id.trim() || old.token_env != token_env.trim()
            });
            let mut updated = profile_config.clone();
            updated.todoist = Some(TodoistConfig {
                project_id: project_id.trim().to_string(),
                token_env: token_env.trim().to_string(),
            });
            config.upsert_profile(&name, updated);
            config.save()?;
            if reset_sync {
                conn.execute("DELETE FROM todoist_pending_commands", [])?;
                conn.execute("DELETE FROM task_todoist_links", [])?;
                conn.execute("DELETE FROM todoist_sync_state", [])?;
                println!("Todoist target changed; local sync mappings were reset. Existing tasks and old Todoist tasks were left untouched.");
            }
            println!("Todoist configured for profile `{name}` and project {project_id}.");
            println!(
                "Set {token_env} in your shell environment before running `horae todoist sync`."
            );
            Ok(())
        }
        TodoistAction::Status => {
            let config = Config::load()?;
            let (name, profile_config) = config.resolve_profile(profile)?;
            match &profile_config.todoist {
                Some(todoist) => {
                    println!("Profile: {name}");
                    println!("Project: {}", todoist.project_id);
                    println!("Token env: {}", todoist.token_env);
                    println!(
                        "Token: {}",
                        if todoist.resolve_token().is_some() {
                            "available"
                        } else {
                            "missing"
                        }
                    );
                }
                None => println!("Todoist is not configured for profile `{name}`."),
            }
            Ok(())
        }
        TodoistAction::Sync => {
            let config = Config::load()?;
            let (name, profile_config) = config.resolve_profile(profile)?;
            let todoist = profile_config.todoist.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Todoist is not configured. Run `horae todoist configure --project-id <ID>` first.")
            })?;
            let token = todoist.resolve_token().ok_or_else(|| {
                anyhow::anyhow!(
                    "Todoist API token is not available; set {} in the environment.",
                    todoist.token_env
                )
            })?;
            let stats = horae_core::todoist::sync(conn, todoist, &token)?;
            println!(
                "Todoist sync complete for `{name}`: {} imported, {} updated, {} pushed, {} conflicts, {} recurring tasks skipped, {} archived Horae tasks still active in Todoist.",
                stats.imported,
                stats.updated,
                stats.pushed,
                stats.conflicts.len(),
                stats.recurring_skipped,
                stats.archived_local_tasks
            );
            Ok(())
        }
    }
}
