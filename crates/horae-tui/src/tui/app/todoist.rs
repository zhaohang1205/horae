use anyhow::{bail, Result};
use horae_core::config::{Config, TodoistConfig};
use horae_core::todoist::{self, ConflictResolution, SyncConflict, SyncStatus};

#[derive(Default)]
pub(crate) struct TodoistPanel {
    pub(crate) config: Option<TodoistConfig>,
    pub(crate) token_available: bool,
    pub(crate) status: Option<SyncStatus>,
    pub(crate) conflicts: Vec<SyncConflict>,
}

impl<'a> super::App<'a> {
    pub(crate) fn refresh_todoist_panel(&mut self) -> Result<()> {
        let config = Config::load()?;
        let (_, profile) = config.resolve_profile(Some(&self.profile_name))?;
        self.todoist_panel.config = profile.todoist.clone();
        self.todoist_panel.token_available = profile
            .todoist
            .as_ref()
            .and_then(TodoistConfig::resolve_token)
            .is_some();
        self.todoist_panel.status = todoist::sync_status(self.conn)?;
        self.todoist_panel.conflicts = todoist::sync_conflicts(self.conn)?;
        Ok(())
    }

    pub(crate) fn save_todoist_config(
        &mut self,
        project_id: &str,
        token_env: &str,
        delete_remote_on_archive: bool,
        prefer_remote_on_conflict: bool,
    ) -> Result<()> {
        let project_id = project_id.trim();
        let token_env = token_env.trim();
        if token_env.is_empty() {
            bail!("token environment variable must not be empty");
        }
        let project_id = if project_id.is_empty() {
            let token = std::env::var(token_env)
                .ok()
                .filter(|value| !value.trim().is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("{token_env} is required to discover the Todoist Inbox project")
                })?;
            todoist::discover_inbox_project_id(&token)?
        } else {
            project_id.to_string()
        };
        let mut config = Config::load()?;
        let (name, profile) = config.resolve_profile(Some(&self.profile_name))?;
        let reset_sync = profile
            .todoist
            .as_ref()
            .is_some_and(|old| old.project_id != project_id || old.token_env != token_env);
        let mut updated = profile.clone();
        updated.todoist = Some(TodoistConfig {
            project_id,
            token_env: token_env.to_string(),
            delete_remote_on_archive,
            prefer_remote_on_conflict,
        });
        let name = name.to_string();
        config.upsert_profile(&name, updated);
        config.save()?;
        if reset_sync {
            self.conn
                .execute("DELETE FROM todoist_pending_commands", [])?;
            self.conn.execute("DELETE FROM todoist_conflicts", [])?;
            self.conn.execute("DELETE FROM task_todoist_links", [])?;
            self.conn.execute("DELETE FROM todoist_sync_state", [])?;
        }
        self.refresh_todoist_panel()
    }

    pub(crate) fn run_todoist_sync(&mut self) -> Result<todoist::SyncStats> {
        let config = self
            .todoist_panel
            .config
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("Todoist is not configured for this profile"))?;
        let token = config
            .resolve_token()
            .ok_or_else(|| anyhow::anyhow!("{} is not set", config.token_env))?;
        let result = todoist::sync(self.conn, config, &token);
        let refresh = self.refresh_todoist_panel();
        refresh?;
        result
    }

    pub(crate) fn resolve_todoist_conflict(
        &mut self,
        todoist_id: &str,
        resolution: ConflictResolution,
    ) -> Result<Option<todoist::SyncStats>> {
        todoist::resolve_conflict(self.conn, todoist_id, resolution)?;
        self.refresh_todoist_panel()?;
        if self.todoist_panel.conflicts.is_empty() {
            self.run_todoist_sync().map(Some)
        } else {
            Ok(None)
        }
    }
}
