use crate::repo::state::JsonStateStore;
use anyhow::Result;
use serde::{Deserialize, Serialize};

/// 心智维护提醒的去重状态：记录已经发送过的 `类型:日期` key。
/// 每日聚合摘要（digest）按本地日期去重，确保同一天至多提醒一次。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NotifyState {
    #[serde(default)]
    pub sent: Vec<String>,
}

fn store() -> JsonStateStore<NotifyState> {
    JsonStateStore::new("notify.json")
}

pub fn get_state() -> Result<NotifyState> {
    store().load()
}

pub fn save_state(state: &NotifyState) -> Result<()> {
    store().save(state)
}
