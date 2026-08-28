use crate::repo::state::JsonStateStore;
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// Persisted alarm bookkeeping, keyed by `task_id:occurrence_ms`.
/// - `rung`: occurrences whose alarm already fired (dedup, doesn't affect the window).
/// - `skipped`: occurrences the user dismissed via `horae alarm next` (leave the window).
/// - `last_window`: last published window (occurrence keys). `horae alarm waybar`
///   compares the freshly computed window against it; when the window rolls, the
///   slot which noticed it pokes waybar (SIGRTMIN+12) so both slots re-render in
///   the same frame. Only slot 1 writes it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AlarmState {
    #[serde(default)]
    pub rung: Vec<String>,
    #[serde(default)]
    pub skipped: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub last_window: Vec<String>,
}

fn store() -> JsonStateStore<AlarmState> {
    JsonStateStore::new("alarm.json")
}

pub fn get_state() -> Result<AlarmState> {
    store().load()
}

pub fn save_state(state: &AlarmState) -> Result<()> {
    store().save(state)
}
