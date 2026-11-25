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
            delete_remote_on_archive,
            prefer_remote_on_conflict,
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
                delete_remote_on_archive,
                prefer_remote_on_conflict,
            });
            config.upsert_profile(&name, updated);
            config.save()?;
            if reset_sync {
                conn.execute("DELETE FROM todoist_pending_commands", [])?;
                conn.execute("DELETE FROM todoist_conflicts", [])?;
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
                    println!(
                        "Delete remote on archive: {}",
                        todoist.delete_remote_on_archive
                    );
                    println!(
                        "Prefer Todoist on conflict: {}",
                        todoist.prefer_remote_on_conflict
                    );
                    match horae_core::todoist::sync_status(conn)? {
                        Some(status) if status.last_sync_at > 0 => {
                            println!("Last sync: {}", status.last_sync_at);
                            if let Some(error) = &status.last_error {
                                println!("Last sync error: {error}");
                            } else {
                                println!(
                                    "Last sync result: {} imported, {} updated, {} pushed.",
                                    status.imported, status.updated, status.pushed
                                );
                            }
                        }
                        _ => println!("Last sync: never"),
                    }
                    let conflicts = horae_core::todoist::sync_conflicts(conn)?;
                    println!("Open conflicts: {}", conflicts.len());
                    for conflict in &conflicts {
                        println!(
                            "  {}  local: {}  remote: {}",
                            conflict.todoist_id, conflict.local_title, conflict.remote_title
                        );
                    }
                }
                None => println!("Todoist is not configured for profile `{name}`."),
            }
            Ok(())
        }
        TodoistAction::Resolve { todoist_id, side } => {
            let resolution = match side.trim().to_ascii_lowercase().as_str() {
                "horae" | "local" => horae_core::todoist::ConflictResolution::KeepHorae,
                "todoist" | "remote" => horae_core::todoist::ConflictResolution::KeepTodoist,
                other => bail!(
                    "unknown side `{other}`; expected `horae` (keep the local task) or `todoist` (keep the remote task)"
                ),
            };
            horae_core::todoist::resolve_conflict(conn, &todoist_id, resolution)?;
            println!("Conflict {todoist_id} resolved as `{}`.", side.trim());
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
                "Todoist sync complete for `{name}`: {} imported, {} updated, {} pushed, {} conflicts, {} recurring tasks skipped, {} archived Horae tasks still active in Todoist, {} remote tasks deleted.",
                stats.imported,
                stats.updated,
                stats.pushed,
                stats.conflicts.len(),
                stats.recurring_skipped,
                stats.archived_local_tasks,
                stats.remote_deleted
            );
            Ok(())
        }
    }
}
