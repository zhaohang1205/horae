use crate::model::pomodoro::PomoState;
use crate::repo::state::JsonStateStore;
use anyhow::Result;

fn store() -> JsonStateStore<PomoState> {
    JsonStateStore::new("pomo.json")
}

pub fn get_state() -> Result<PomoState> {
    store().load()
}

pub fn save_state(state: &PomoState) -> Result<()> {
    store().save(state)
}
