use anyhow::Result;

use super::App;

impl<'a> App<'a> {
    /// 设置页：新建 profile（写入 config.json，不动任何数据库）。
    pub(crate) fn settings_new_profile(&mut self, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            self.status_message = crate::tr!(
                self.lang,
                "profile 名称不能为空",
                "profile name cannot be empty"
            )
            .into();
            return Ok(());
        }
        let mut config = crate::config::Config::load()?;
        if config.profile(name).is_some() {
            self.status_message = crate::tr!(
                self.lang,
                "profile 已存在: {}",
                "profile already exists: {}",
                name
            );
            return Ok(());
        }
        config.upsert_profile(
            name,
            crate::config::Profile {
                db: format!("profiles/{name}.db"),
                cloud: None,
                ntfy: None,
            },
        );
        if config.save().is_err() {
            self.status_message = crate::tr!(
                self.lang,
                "保存 config.json 失败",
                "failed to save config.json"
            )
            .into();
            return Ok(());
        }
        self.status_message = crate::tr!(
            self.lang,
            "已创建 profile: {} (下次启动可用 --profile {})",
            "created profile: {} (use --profile {} next launch)",
            name,
            name
        );
        self.refresh()?;
        Ok(())
    }

    /// 设置页：重命名当前选中的 profile。
    pub(crate) fn settings_rename_profile(&mut self, new_name: &str) -> Result<()> {
        let Some(row) = self.items.get(self.selected).cloned() else {
            return Ok(());
        };
        let new_name = new_name.trim();
        if new_name.is_empty() || new_name == row.id {
            self.status_message =
                crate::tr!(self.lang, "profile 名称无效", "invalid profile name").into();
            return Ok(());
        }
        let mut config = crate::config::Config::load()?;
        if config.profile(new_name).is_some() {
            self.status_message = crate::tr!(
                self.lang,
                "profile 已存在: {}",
                "profile already exists: {}",
                new_name
            );
            return Ok(());
        }
        if config.rename_profile(&row.id, new_name).is_err() {
            self.status_message = crate::tr!(
                self.lang,
                "profile 不存在: {}",
                "profile not found: {}",
                row.id
            );
            return Ok(());
        }
        if config.save().is_err() {
            self.status_message = crate::tr!(
                self.lang,
                "保存 config.json 失败",
                "failed to save config.json"
            )
            .into();
            return Ok(());
        }
        if self.profile_name == row.id {
            self.profile_name = new_name.to_string();
        }
        self.status_message = crate::tr!(
            self.lang,
            "已重命名: {} -> {}",
            "renamed: {} -> {}",
            row.id,
            new_name
        );
        self.refresh()?;
        Ok(())
    }

    /// 设置页：删除当前选中的 profile（仅从 config.json 移除，db 文件保留）。
    pub(crate) fn settings_delete_profile(&mut self, name: &str) -> Result<()> {
        let mut config = crate::config::Config::load()?;
        if config.remove_profile(name).is_none() {
            self.status_message = crate::tr!(
                self.lang,
                "profile 不存在: {}",
                "profile not found: {}",
                name
            );
            return Ok(());
        }
        // 删除默认 profile 时把默认改派给剩余第一个。
        if config.default_profile == name {
            config.default_profile = config
                .profile_names()
                .first()
                .cloned()
                .unwrap_or_else(|| "default".to_string());
        }
        if config.save().is_err() {
            self.status_message = crate::tr!(
                self.lang,
                "保存 config.json 失败",
                "failed to save config.json"
            )
            .into();
            return Ok(());
        }
        self.status_message = crate::tr!(
            self.lang,
            "已删除 profile: {} (db 文件保留)",
            "deleted profile: {} (db file kept)",
            name
        );
        self.refresh()?;
        Ok(())
    }

    /// 设置页：把选中的 profile 设为默认（下次无 --profile 启动生效）。
    pub(crate) fn settings_set_default(&mut self) -> Result<()> {
        let Some(row) = self.items.get(self.selected).cloned() else {
            return Ok(());
        };
        let mut config = crate::config::Config::load()?;
        if config.set_default(&row.id).is_err() {
            self.status_message = crate::tr!(
                self.lang,
                "profile 不存在: {}",
                "profile not found: {}",
                row.id
            );
            return Ok(());
        }
        if config.save().is_err() {
            self.status_message = crate::tr!(
                self.lang,
                "保存 config.json 失败",
                "failed to save config.json"
            )
            .into();
            return Ok(());
        }
        self.status_message = crate::tr!(
            self.lang,
            "默认 profile 已设为 {} (下次启动生效)",
            "default profile set to {} (applies next launch)",
            row.id
        );
        self.refresh()?;
        Ok(())
    }
}
