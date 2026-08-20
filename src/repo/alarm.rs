use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Persisted alarm bookkeeping, keyed by `task_id:occurrence_ms`.
/// - `rung`: occurrences whose alarm already fired (dedup, doesn't affect the window).
/// - `skipped`: occurrences the user dismissed via `gtp alarm next` (leave the window).
/// - `last_window`: last published window (occurrence keys). `gtp alarm waybar`
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

pub fn alarm_file_path() -> PathBuf {
    let mut path = dirs::config_dir().unwrap_or_else(|| PathBuf::from("."));
    path.push("gtp");
    let _ = fs::create_dir_all(&path);
    path.push("alarm.json");
    path
}

pub fn get_state() -> Result<AlarmState> {
    let path = alarm_file_path();
    if !path.exists() {
        return Ok(AlarmState::default());
    }
    let content = fs::read_to_string(&path)?;
    let state = serde_json::from_str(&content)?;
    Ok(state)
}

pub fn save_state(state: &AlarmState) -> Result<()> {
    let path = alarm_file_path();
    let content = serde_json::to_string_pretty(state)?;
    // 先写临时文件再 rename, 避免并发读方读到半写的文件
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, &path)?;
    Ok(())
}
