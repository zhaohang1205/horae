use serde::{Deserialize, Serialize};

use crate::model::pomodoro::PomoState;
use crate::model::task::ChecklistItem;

/// Serialized identity of the backup file. `export` always writes `version: 2`;
/// `import` refuses anything it does not understand, so future format changes
/// can bump this instead of silently mis-parsing.
pub const BACKUP_FORMAT: &str = "horae-backup";
pub const BACKUP_VERSION: u32 = 2;

/// Full-database backup: tasks, event timeline, tags, settings, pomodoro state,
/// and the optional Todoist cursor, identity map, and retry queue.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupData {
    pub format: String,
    pub version: u32,
    pub exported_at: i64,
    #[serde(default)]
    pub tasks: Vec<BackupTask>,
    #[serde(default)]
    pub events: Vec<BackupEvent>,
    #[serde(default)]
    pub tags: Vec<BackupTag>,
    #[serde(default)]
    pub task_tags: Vec<BackupTaskTag>,
    #[serde(default)]
    pub settings: Vec<BackupSetting>,
    /// Absent in backups from before the pomo state was included.
    #[serde(default)]
    pub pomodoro: Option<PomoState>,
    /// Absent in backups created before optional Todoist sync was added.
    #[serde(default)]
    pub todoist_sync: Option<BackupTodoistSync>,
    #[serde(default)]
    pub todoist_links: Vec<BackupTodoistLink>,
    #[serde(default)]
    pub todoist_pending_commands: Vec<BackupTodoistPending>,
    #[serde(default)]
    pub todoist_conflicts: Vec<BackupTodoistConflict>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupTodoistSync {
    pub sync_token: String,
    pub completed_since_ms: i64,
    pub updated_at: i64,
    #[serde(default)]
    pub last_sync_at: i64,
    #[serde(default)]
    pub last_imported: usize,
    #[serde(default)]
    pub last_updated: usize,
    #[serde(default)]
    pub last_pushed: usize,
    #[serde(default)]
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupTodoistLink {
    pub task_id: String,
    pub todoist_id: String,
    pub local_hash: String,
    pub remote_hash: String,
    pub remote_checked: bool,
    pub remote_deleted: bool,
    #[serde(default)]
    pub out_of_scope: bool,
    pub last_synced_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupTodoistPending {
    pub command_uuid: String,
    pub task_id: String,
    pub temp_id: Option<String>,
    pub command_json: String,
    pub local_hash: String,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupTodoistConflict {
    pub todoist_id: String,
    pub task_id: String,
    pub remote_json: String,
    pub detected_at: i64,
    pub resolution: Option<String>,
}

/// A task row with every live column of the `tasks` table. The dead columns
/// (`kind`, `parent_id`, `organized_at`, `started_at`, `project_type`) were
/// dropped from the schema in migration 0011 and are no longer backed up.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupTask {
    pub id: String,
    pub title: String,
    pub notes: String,
    pub status: String,
    #[serde(default)]
    pub priority: Option<String>,
    pub rrule: Option<String>,
    pub created_at: i64,
    pub clarified_at: Option<i64>,
    pub due_at: Option<i64>,
    pub scheduled_start_at: Option<i64>,
    pub scheduled_end_at: Option<i64>,
    pub completed_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub updated_at: i64,
    pub delegated_to: Option<String>,
    pub checklist: Vec<ChecklistItem>,
    pub archive_reason: Option<String>,
}

/// A `task_events` row without its autoincrement `id` (re-assigned on import).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEvent {
    pub task_id: String,
    pub event_type: String,
    pub from_status: Option<String>,
    pub to_status: Option<String>,
    pub at: i64,
    pub meta: Option<String>,
}

/// A tag row without its integer `id`; tags are reconciled by name on import so
/// ids never conflict across databases.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupTag {
    pub name: String,
    pub category: String,
    pub is_system: bool,
    pub color: Option<String>,
    pub icon: Option<String>,
    pub description: Option<String>,
    pub created_at: i64,
}

/// A `task_tags` association, keyed by task id + tag name (not tag id).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupTaskTag {
    pub task_id: String,
    pub tag_name: String,
    pub added_at: i64,
}

/// A `settings` row (key → value).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupSetting {
    pub key: String,
    pub value: String,
}
