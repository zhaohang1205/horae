use rusqlite::Connection;

#[derive(Clone, Copy, Debug)]
pub struct ModuleVisibility {
    pub splash: bool,
    pub reference: bool,
    pub done: bool,
    pub archived: bool,
    pub tags: bool,
    pub review: bool,
    pub settings: bool,
}

impl ModuleVisibility {
    pub fn load(conn: &Connection) -> Self {
        let get_bool = |key: &str| -> bool {
            !matches!(
                crate::repo::settings::get(conn, key)
                    .ok()
                    .flatten()
                    .as_deref(),
                Some("0")
            )
        };
        Self {
            splash: get_bool("module_splash"),
            reference: get_bool("module_reference"),
            done: get_bool("module_done"),
            archived: get_bool("module_archived"),
            tags: get_bool("module_tags"),
            review: get_bool("module_review"),
            settings: get_bool("module_settings"),
        }
    }

    pub fn set_enabled(
        &mut self,
        conn: &Connection,
        key: &str,
        enabled: bool,
    ) -> anyhow::Result<()> {
        // 先更新内存并校验 key，再落库：未知 key 直接报错，
        // 避免"DB 已写入而内存未更新"的不一致。
        match key {
            "module_splash" => self.splash = enabled,
            "module_reference" => self.reference = enabled,
            "module_done" => self.done = enabled,
            "module_archived" => self.archived = enabled,
            "module_tags" => self.tags = enabled,
            "module_review" => self.review = enabled,
            "module_settings" => self.settings = enabled,
            _ => anyhow::bail!("unknown module key: {}", key),
        }
        let val = if enabled { "1" } else { "0" };
        crate::repo::settings::set(conn, key, val)?;
        Ok(())
    }
}
