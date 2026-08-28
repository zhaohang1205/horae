use horae_core::config;
use serde::Serialize;

/// 已配置的全部 profile 及其默认项（GUI 设置页展示用）。
#[derive(Debug, Clone, Serialize)]
pub struct Profiles {
    pub default: String,
    pub names: Vec<String>,
}

pub mod fns {
    use super::*;

    pub fn list_profiles() -> Result<Profiles, String> {
        let cfg = config::Config::load().map_err(|e| e.to_string())?;
        let names = cfg.profile_names();
        Ok(Profiles {
            default: cfg.default_profile,
            names,
        })
    }
}

#[tauri::command]
pub async fn list_profiles() -> Result<Profiles, String> {
    fns::list_profiles()
}
