use super::app::{App, Mode, Pane, View};
use crate::model::task::{self};
use crate::repo::tasks::CaptureInput;
use crate::repo::{tags, tasks};
use crate::time;
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}
pub(crate) trait AppHandlers {
    fn handle_key(&mut self, key: KeyEvent) -> Result<()>;
    fn handle_normal(&mut self, key: KeyEvent) -> Result<()>;
    fn handle_input(&mut self, key: KeyEvent) -> Result<()>;
    fn confirm_input(&mut self, mode: Mode, input: &str) -> Result<()>;
    fn restore_selected(&mut self) -> Result<()>;
}

impl<'a> AppHandlers for App<'a> {
    fn handle_key(&mut self, key: KeyEvent) -> Result<()> {
        if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
            self.should_quit = true;
            return Ok(());
        }

        if let Some(popup) = self.popup.take() {
            match key.code {
                KeyCode::Enter => {
                    if let crate::tui::app::Popup::TaskDueNow(id, _) = popup {
                        self.note(crate::commands::pomo::start(self.conn, &id));
                        self.needs_clear = true;
                    }
                }
                _ => {
                    // Default to close on any key
                    if let crate::tui::app::Popup::TaskDueNow(_, _) = popup {
                        if key.code != KeyCode::Esc {
                            // If they typed something else, maybe restore the popup? Let's just close on anything for now.
                        }
                    }
                }
            }
            return Ok(());
        }

        match self.mode {
            Mode::Normal | Mode::Visual => self.handle_normal(key),
            _ => self.handle_input(key),
        }
    }

    fn handle_normal(&mut self, key: KeyEvent) -> Result<()> {
        if self.show_help && self.handle_help_navigation(key) {
            return Ok(());
        }
        if self.handle_escape(key)? {
            return Ok(());
        }
        if self.handle_system_keys(key)? {
            return Ok(());
        }
        if self.handle_navigation_keys(key)? {
            return Ok(());
        }
        if self.handle_settings_keys(key)? {
            return Ok(());
        }
        if self.handle_review_keys(key)? {
            return Ok(());
        }
        if self.handle_mode_switch_keys(key)? {
            return Ok(());
        }
        if self.handle_task_action_keys(key)? {
            return Ok(());
        }
        if self.handle_selection_keys(key)? {
            return Ok(());
        }
        if self.handle_pomodoro_keys(key)? {
            return Ok(());
        }
        if self.handle_archive_keys(key)? {
            return Ok(());
        }
        Ok(())
    }

    fn handle_input(&mut self, key: KeyEvent) -> Result<()> {
        if self.mode == Mode::ConfirmArchive {
            return self.handle_confirm_archive(key);
        }
        if self.mode == Mode::ConfirmPurge {
            return self.handle_confirm_purge(key);
        }
        if self.mode == Mode::ConfirmProfileDelete {
            return self.handle_confirm_profile_delete(key);
        }
        match key.code {
            KeyCode::Esc => {
                if self.completion_active() {
                    // 候选列表打开：Esc 关闭并保留当前输入。
                    self.cancel_completion();
                    return Ok(());
                }
                self.organizing_id = None;
                self.set_mode(Mode::Normal);
                self.input_clear();
                self.reload()?;
            }
            KeyCode::Tab | KeyCode::Right if self.completion_active() => {
                // 候选激活：Tab/→ 采纳 ghost 补全（补齐 token，留在编辑）。
                self.accept_completion();
                return Ok(());
            }
            KeyCode::Enter => {
                // 候选激活时先补齐当前候选，再提交（提交的是补全后的完整输入）。
                if self.completion_active() {
                    self.apply_current_completion();
                }
                if self.mode == Mode::Capturing {
                    if let Err(msg) = self.validate_capture_input(&self.input) {
                        self.status_message = msg;
                        self.clear_completion();
                        return Ok(());
                    }
                }
                let input = self.input.clone();
                let mode = self.mode;
                self.set_mode(Mode::Normal);
                self.input_clear();
                self.confirm_input(mode, &input)?;
            }
            KeyCode::Tab => {
                self.handle_tab_completion();
            }
            KeyCode::Up => self.completion_up(),
            KeyCode::Down => self.completion_down(),
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.completion_down();
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.completion_up();
            }
            KeyCode::Backspace => self.input_backspace(),
            KeyCode::Delete => self.input_delete(),
            KeyCode::Left => self.input_move_left(),
            KeyCode::Right => self.input_move_right(),
            KeyCode::Home => self.input_home(),
            KeyCode::End => self.input_end(),
            KeyCode::Char(c) => self.input_insert_char(c),
            _ => {}
        }
        Ok(())
    }

    fn confirm_input(&mut self, mode: Mode, input: &str) -> Result<()> {
        match mode {
            Mode::Search => self.confirm_search(input)?,
            Mode::FilteringTag => self.confirm_filter_tag(input)?,
            Mode::Capturing => self.confirm_capture(input)?,
            Mode::Tagging => self.confirm_tagging(input)?,
            Mode::WaitingWho => self.confirm_waiting_who(input)?,
            Mode::WaitingWhen => self.confirm_waiting_when(input)?,
            Mode::ChecklistAdding => self.confirm_checklist_adding(input)?,
            Mode::CreatingTag => self.confirm_creating_tag(input)?,
            Mode::ConfiguringPomo => self.confirm_pomo_config(input)?,
            Mode::CreatingProfile => self.confirm_creating_profile(input)?,
            Mode::RenamingProfile => self.confirm_renaming_profile(input)?,
            Mode::Normal
            | Mode::Visual
            | Mode::ConfirmArchive
            | Mode::ConfirmPurge
            | Mode::ConfirmProfileDelete => {}
        }
        Ok(())
    }

    /// Restore the currently selected archived task (only meaningful in the
    /// Archived view). No-op outside that view or if nothing is selected.
    fn restore_selected(&mut self) -> Result<()> {
        if self.view != View::Archived {
            return Ok(());
        }
        let mut ids = vec![];
        if !self.selected_ids.is_empty() {
            ids.extend(self.selected_ids.iter().cloned());
        } else if let Some(row) = self.items.get(self.selected).cloned() {
            ids.push(row.id);
        }
        if ids.is_empty() {
            return Ok(());
        }
        let mut count = 0;
        for id in &ids {
            if tasks::unarchive(self.conn, id).is_ok() {
                count += 1;
            }
        }
        if count == 1 && ids.len() == 1 {
            self.status_message =
                crate::tr!(self.lang, "已恢复 {}", "restored {}", short_id(&ids[0]));
        } else {
            self.status_message = crate::tr!(self.lang, "已恢复 {} 项", "restored {} items", count);
        }
        if !self.selected_ids.is_empty() {
            self.selected_ids.clear();
            self.visual_start_idx = None;
        }
        self.reload()?;
        Ok(())
    }
}

impl<'a> App<'a> {
    /// 帮助抽屉打开时的滚动键，命中则消费该按键。
    fn handle_help_navigation(&mut self, key: KeyEvent) -> bool {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down | KeyCode::PageDown => {
                self.help_scroll = self.help_scroll.saturating_add(1);
                true
            }
            KeyCode::Char('k') | KeyCode::Up | KeyCode::PageUp => {
                self.help_scroll = self.help_scroll.saturating_sub(1);
                true
            }
            KeyCode::Char('g') => {
                self.help_scroll = 0;
                true
            }
            KeyCode::Char('G') => {
                self.help_scroll = usize::MAX;
                true
            }
            KeyCode::Esc => {
                self.show_help = false;
                self.help_scroll = 0;
                true
            }
            _ => false,
        }
    }

    fn handle_escape(&mut self, key: KeyEvent) -> Result<bool> {
        if key.code != KeyCode::Esc {
            return Ok(false);
        }
        if self.is_reviewing {
            self.is_reviewing = false;
            self.review_step = 0;
            self.status_message =
                crate::tr!(self.lang, "周回顾已取消", "Weekly Review cancelled").into();
            {
                let r = self.reload();
                self.note(r);
            }
        } else if self.mode == Mode::Visual {
            self.set_mode(Mode::Normal);
            self.visual_start_idx = None;
            self.selected_ids.clear();
            self.status_message =
                crate::tr!(self.lang, "已退出可视模式", "Exited visual mode").into();
            {
                let r = self.reload();
                self.note(r);
            }
        } else if self.tag_filter.is_some() || !self.search_query.is_empty() {
            self.tag_filter = None;
            self.search_query.clear();
            self.status_message = crate::tr!(self.lang, "已清除过滤", "Cleared filters").into();
            {
                let r = self.reload();
                self.note(r);
            }
        } else if !self.selected_ids.is_empty() {
            self.selected_ids.clear();
            self.status_message = crate::tr!(self.lang, "已清除选择", "Selection cleared").into();
        } else {
            self.hide_pomo_banner = true;
            self.status_message.clear();
        }
        Ok(true)
    }

    /// 系统级按键：退出、帮助、快捷键条、主题、语言、语法面板。
    fn handle_system_keys(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') | KeyCode::F(1) => self.show_help = !self.show_help,
            KeyCode::F(2) => {
                self.show_shortcut_bar = !self.show_shortcut_bar;
                self.status_message = if self.show_shortcut_bar {
                    crate::tr!(self.lang, "已显示快捷键条", "Shortcut bar shown").into()
                } else {
                    crate::tr!(
                        self.lang,
                        "已隐藏快捷键条 (F2 显示)",
                        "Shortcut bar hidden (F2 to show)"
                    )
                    .into()
                };
            }
            KeyCode::F(5) => {
                self.theme = self.theme.toggle();
                let saved = crate::repo::settings::set(
                    self.conn,
                    "theme",
                    if self.theme.is_dark { "mocha" } else { "latte" },
                );
                if self.note(saved) {
                    self.status_message = if self.theme.is_dark {
                        crate::tr!(
                            self.lang,
                            "主题: Catppuccin 摩卡 (深色)",
                            "Theme: Catppuccin Mocha (Dark)"
                        )
                        .to_string()
                    } else {
                        crate::tr!(
                            self.lang,
                            "主题: Catppuccin 拿铁 (亮色)",
                            "Theme: Catppuccin Latte (Light)"
                        )
                        .to_string()
                    };
                }
            }
            KeyCode::F(6) => {
                self.lang = match self.lang {
                    crate::i18n::Lang::Zh => crate::i18n::Lang::En,
                    crate::i18n::Lang::En => crate::i18n::Lang::Zh,
                };
                let key = match self.lang {
                    crate::i18n::Lang::Zh => "zh",
                    crate::i18n::Lang::En => "en",
                };
                let saved = crate::repo::settings::set(self.conn, "lang", key);
                if self.note(saved) {
                    self.status_message = match self.lang {
                        crate::i18n::Lang::Zh => "语言已切换为中文 (F6 切换)".to_string(),
                        crate::i18n::Lang::En => {
                            "Language switched to English (F6 to toggle)".to_string()
                        }
                    };
                }
            }
            KeyCode::F(7) => {
                self.quotes_enabled = !self.quotes_enabled;
                let saved = crate::repo::settings::set(
                    self.conn,
                    "quotes",
                    if self.quotes_enabled { "1" } else { "0" },
                );
                // 停用时若正停留在金句视图，跳回收件箱避免停留在隐藏视图。
                if !self.quotes_enabled && self.view == View::Quotes {
                    self.set_view(View::Inbox);
                }
                if self.note(saved) {
                    self.status_message = if self.quotes_enabled {
                        crate::tr!(
                            self.lang,
                            "金句功能已启用 (视图 0, 快捷键 \")",
                            "Quotes enabled (view 0, key \")"
                        )
                        .to_string()
                    } else {
                        crate::tr!(
                            self.lang,
                            "金句功能已停用 (F7 再开)",
                            "Quotes disabled (F7 to re-enable)"
                        )
                        .to_string()
                    };
                }
                {
                    let r = self.reload();
                    self.note(r);
                }
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.show_syntax = !self.show_syntax;
            }
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.select_all();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.invert_selection();
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn handle_navigation_keys(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('h') | KeyCode::Left => {
                self.pane = match self.pane {
                    Pane::Right => Pane::Center,
                    Pane::Center => Pane::Left,
                    Pane::Left => Pane::Left,
                };
            }
            KeyCode::Char('l') | KeyCode::Right => {
                self.pane = match self.pane {
                    Pane::Left => Pane::Center,
                    Pane::Center => Pane::Right,
                    Pane::Right => Pane::Right,
                };
            }
            KeyCode::Down | KeyCode::Char('j') => {
                if self.pane == Pane::Left && self.mode != Mode::Visual {
                    self.next_view(1);
                } else {
                    self.move_sel(1);
                }
            }
            KeyCode::Up | KeyCode::Char('k') => {
                if self.pane == Pane::Left && self.mode != Mode::Visual {
                    self.next_view(-1);
                } else {
                    self.move_sel(-1);
                }
            }
            KeyCode::Char(d) if d.is_ascii_digit() => {
                if let Some(v) = View::from_digit(d) {
                    // 金句视图功能未启用时，数字 0 不响应。
                    if v == View::Quotes && !self.quotes_enabled {
                        return Ok(true);
                    }
                    self.set_view(v);
                }
            }
            KeyCode::Char('J') => self.set_view(View::Today),
            KeyCode::Char('K') => self.set_view(View::Tomorrow),
            KeyCode::F(8) => self.set_view(View::Settings),
            KeyCode::Char('g') => self.move_sel(-10000),
            KeyCode::Char('G') => self.move_sel(10000),
            _ => return Ok(false),
        }
        Ok(true)
    }

    fn handle_review_keys(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('r') => {
                self.is_reviewing = true;
                self.review_step = 1;
                self.set_view(View::Inbox);
                self.status_message =
                    crate::tr!(self.lang, "周回顾已开始", "Weekly Review started").into();
            }
            KeyCode::Char('R') if self.is_reviewing => {
                self.review_step += 1;
                match self.review_step {
                    2 => self.set_view(View::Waiting),
                    3 => self.set_view(View::Someday),
                    4 => self.set_view(View::Done),
                    _ => {
                        self.is_reviewing = false;
                        self.review_step = 0;
                        self.set_view(View::Next);
                        self.status_message =
                            crate::tr!(self.lang, "每周回顾完成! 🎉", "Weekly Review Complete! 🎉")
                                .into();
                    }
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 进入各类输入/编辑模式的按键。
    fn handle_mode_switch_keys(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('v') | KeyCode::Char('V') => {
                if self.mode == Mode::Visual {
                    self.set_mode(Mode::Normal);
                    self.visual_start_idx = None;
                    self.selected_ids.clear();
                    self.status_message =
                        crate::tr!(self.lang, "已退出可视模式", "Exited visual mode").into();
                } else {
                    self.set_mode(Mode::Visual);
                    self.visual_start_idx = Some(self.selected);
                    self.update_visual_selection();
                    self.status_message = "-- VISUAL --".into();
                }
                {
                    let r = self.refresh();
                    self.note(r);
                }
            }
            KeyCode::Char('a') => {
                // 任何视图（含标签库）一律快速录入：新建任务。
                self.set_mode(Mode::Capturing);
                self.input.clear();
            }
            KeyCode::Char('e') => {
                // 全量编辑：与 Enter 同源的一句话编辑器，预填当前任务内容。
                self.open_organize()?;
            }
            KeyCode::Char('w') => {
                self.set_mode(Mode::WaitingWho);
                self.input.clear();
            }
            KeyCode::Char('T') => {
                // 批量打标签（单选或多选）。
                self.set_mode(Mode::Tagging);
                self.input.clear();
            }
            KeyCode::Char('c') if self.view == View::Tags => {
                // 标签库视图：新增自定义标签。
                self.set_mode(Mode::CreatingTag);
                self.input.clear();
            }
            KeyCode::Char('[') => {
                let pomo = crate::repo::pomodoro::get_state().unwrap_or_default();
                self.input = format!(
                    "{};{};{}",
                    pomo.config.work_mins,
                    pomo.config.short_break_mins,
                    pomo.config.long_break_mins
                );
                self.set_mode(Mode::ConfiguringPomo);
            }
            KeyCode::Char('C') if self.items.get(self.selected).is_some() => {
                self.set_mode(Mode::ChecklistAdding);
                self.input.clear();
            }
            KeyCode::Char('/') => {
                self.input = self.search_query.clone();
                self.set_mode(Mode::Search);
            }
            KeyCode::Char('f') => {
                self.input = self.tag_filter.clone().unwrap_or_default();
                self.set_mode(Mode::FilteringTag);
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 设置页按键：n 新建 / r 重命名 / d 删除 / s 设为默认（仅 Settings 视图生效）。
    fn handle_settings_keys(&mut self, key: KeyEvent) -> Result<bool> {
        if self.view != View::Settings {
            return Ok(false);
        }
        match key.code {
            KeyCode::Char('n') => {
                self.set_mode(Mode::CreatingProfile);
                self.input.clear();
            }
            KeyCode::Char('r') => {
                let Some(row) = self.items.get(self.selected).cloned() else {
                    return Ok(true);
                };
                self.input = row.id.clone();
                self.set_mode(Mode::RenamingProfile);
            }
            KeyCode::Char('d') => {
                let Some(row) = self.items.get(self.selected).cloned() else {
                    return Ok(true);
                };
                self.pending_profile_delete = Some(row.id.clone());
                self.set_mode(Mode::ConfirmProfileDelete);
            }
            KeyCode::Char('s') => {
                self.settings_set_default()?;
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 对当前选中任务执行的直接操作按键。
    fn handle_task_action_keys(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('n') => {
                if let Some(row) = self.items.get(self.selected).cloned() {
                    if let Ok(task) = tasks::get(self.conn, &row.id) {
                        crossterm::terminal::disable_raw_mode()?;
                        crossterm::execute!(
                            std::io::stdout(),
                            crossterm::terminal::LeaveAlternateScreen
                        )?;

                        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
                        let temp_path = std::env::temp_dir()
                            .join(format!("horae_notes_{}.md", uuid::Uuid::new_v4()));
                        use std::io::Write;
                        let mut file = std::fs::File::create(&temp_path)?;
                        file.write_all(task.notes.as_bytes())?;
                        drop(file);

                        let _ = std::process::Command::new(editor).arg(&temp_path).status();

                        let new_notes = std::fs::read_to_string(&temp_path).unwrap_or_default();
                        let _ = std::fs::remove_file(&temp_path);
                        if new_notes != task.notes {
                            self.note(tasks::update_notes(self.conn, &task.id, &new_notes));
                        }

                        crossterm::terminal::enable_raw_mode()?;
                        crossterm::execute!(
                            std::io::stdout(),
                            crossterm::terminal::EnterAlternateScreen
                        )?;
                        self.needs_clear = true;
                        self.load_detail();
                    }
                }
            }
            KeyCode::Char('x') => {
                self.act_on_selected(task::Status::Done)?;
                self.move_sel(1);
            }
            KeyCode::Char('s') => self.act_on_selected(task::Status::Someday)?,
            KeyCode::Char('"') if self.quotes_enabled => {
                // 金句移入/移出：加/摘 @quote 标签，工作态流转为 reference。
                self.toggle_quotes()?;
            }
            KeyCode::Char('u') => {
                let r = self.restore_selected();
                self.note(r);
            }
            KeyCode::Enter => self.open_organize()?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 批量选择相关按键：Space 切换当前行（Ctrl+a 全选 / Ctrl+i 反选在系统键处理）。
    fn handle_selection_keys(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char(' ') => {
                self.toggle_selected();
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 番茄钟相关按键：P 启动/续杯，= 勾选检查单，S 停止。
    fn handle_pomodoro_keys(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('=') => {
                if let Some(row) = self.items.get(self.selected).cloned() {
                    if let Ok(Some(result)) =
                        crate::repo::tasks::toggle_next_checklist_item(self.conn, &row.id)
                    {
                        match result {
                            crate::repo::tasks::ToggleResult::Checked(title) => {
                                self.status_message =
                                    crate::tr!(self.lang, "打卡: {}", "Checked: {}", title);
                            }
                            crate::repo::tasks::ToggleResult::Reset => {
                                self.status_message =
                                    crate::tr!(self.lang, "已重置检查单", "Checklist reset")
                                        .to_string();
                            }
                        }
                        self.load_detail();
                    }
                }
            }
            KeyCode::Char('P') => {
                // 休息后或跨天 idle 的续杯：优先当前选中任务。
                if let Ok(pomo) = crate::repo::pomodoro::get_state() {
                    let is_in_break = matches!(
                        pomo.phase,
                        crate::model::pomodoro::Phase::ShortBreak
                            | crate::model::pomodoro::Phase::LongBreak
                    );
                    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                    let today_active = pomo.last_date.as_deref() == Some(today.as_str());
                    let is_post_break_idle = pomo.phase == crate::model::pomodoro::Phase::Idle
                        && today_active
                        && (pomo.today_count > 0 || pomo.task_id.is_some());
                    if is_in_break || is_post_break_idle {
                        let target_id = self
                            .items
                            .get(self.selected)
                            .map(|r| r.id.clone())
                            .or(pomo.task_id);
                        if let Some(tid) = target_id {
                            self.note(crate::repo::tasks::ensure_ready_for_pomodoro(
                                self.conn, &tid,
                            ));
                            {
                                let r = self.refresh();
                                self.note(r);
                            }
                            if self.note(crate::commands::pomo::start(self.conn, &tid)) {
                                self.status_message = crate::tr!(
                                    self.lang,
                                    "🚀 零摩擦开启新一轮专注！ ({})",
                                    "🚀 Frictionless new focus round! ({})",
                                    short_id(&tid)
                                );
                            }
                            self.load_detail();
                            return Ok(true);
                        }
                    }
                }
                let target_id = self
                    .items
                    .get(self.selected)
                    .map(|r| r.id.clone())
                    .or_else(|| {
                        crate::repo::pomodoro::get_state()
                            .ok()
                            .and_then(|s| s.task_id)
                    });
                if let Some(tid) = target_id {
                    self.note(crate::repo::tasks::ensure_ready_for_pomodoro(
                        self.conn, &tid,
                    ));
                    {
                        let r = self.refresh();
                        self.note(r);
                    }
                    if self.note(crate::commands::pomo::start(self.conn, &tid)) {
                        self.status_message = crate::tr!(
                            self.lang,
                            "🎯 已为 {} 开启专注与番茄钟",
                            "🎯 Focus & Pomodoro started for {}",
                            short_id(&tid)
                        );
                    }
                    self.load_detail();
                }
            }
            KeyCode::Char('S') => {
                if self.note(crate::commands::pomo::stop()) {
                    self.status_message.clear();
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 归档 / 永久删除（归档箱、标签库）按键。
    fn handle_archive_keys(&mut self, key: KeyEvent) -> Result<bool> {
        if !matches!(
            key.code,
            KeyCode::Char('A') | KeyCode::Char('D') | KeyCode::Delete
        ) {
            return Ok(false);
        }
        // 归档箱视图：D / Delete 触发永久删除（带确认）。A 仍走归档逻辑。
        if self.view == View::Archived && matches!(key.code, KeyCode::Char('D') | KeyCode::Delete) {
            let mut ids = vec![];
            if !self.selected_ids.is_empty() {
                ids.extend(self.selected_ids.iter().cloned());
            } else if let Some(row) = self.items.get(self.selected).cloned() {
                ids.push(row.id);
            }
            if ids.is_empty() {
                return Ok(true);
            }
            self.pending_purge_ids = ids;
            self.set_mode(Mode::ConfirmPurge);
            self.status_message = crate::tr!(
                self.lang,
                "永久删除归档箱中 {} 项? (y/Enter 确认, n/Esc 取消)",
                "Permanently delete {} archived item(s)? (y/Enter confirm, n/Esc cancel)",
                self.pending_purge_ids.len()
            )
            .to_string();
            return Ok(true);
        }
        if self.view == View::Tags {
            if let Some(row) = self.items.get(self.selected).cloned() {
                let tag_name = row.title.trim_start_matches('@');
                match tags::delete_tag(self.conn, tag_name) {
                    Ok(_) => {
                        self.status_message =
                            crate::tr!(self.lang, "已删除标签 @{}", "Tag @{} deleted", tag_name);
                        self.refresh()?;
                    }
                    Err(e) => {
                        self.status_message =
                            crate::tr!(self.lang, "删除失败: {}", "Delete failed: {}", e);
                    }
                }
            }
            return Ok(true);
        }
        let mut ids = vec![];
        if !self.selected_ids.is_empty() {
            ids.extend(self.selected_ids.iter().cloned());
        } else if let Some(row) = self.items.get(self.selected).cloned() {
            ids.push(row.id);
        }
        if ids.is_empty() {
            return Ok(true);
        }
        self.pending_archive_ids = ids;
        self.set_mode(Mode::ConfirmArchive);
        self.status_message = crate::tr!(
            self.lang,
            "确认归档 {} 项? (y/Enter 确认, n/Esc 取消)",
            "Archive {} items? (y/Enter confirm, n/Esc cancel)",
            self.pending_archive_ids.len()
        );
        Ok(true)
    }

    fn handle_confirm_archive(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                let ids = std::mem::take(&mut self.pending_archive_ids);
                let had_selection = !self.selected_ids.is_empty();
                let mut count = 0;
                for id in &ids {
                    if let Ok(task) = tasks::get(self.conn, id) {
                        if matches!(
                            task.status,
                            task::Status::Done | task::Status::Waiting | task::Status::Someday
                        ) {
                            if let Ok(pomo) = crate::repo::pomodoro::get_state() {
                                if pomo.task_id.as_deref() == Some(id) {
                                    let _ = crate::commands::pomo::stop();
                                }
                            }
                        }
                        if tasks::archive(self.conn, id).is_ok() {
                            count += 1;
                        }
                    }
                }
                self.set_mode(Mode::Normal);
                if had_selection {
                    self.selected_ids.clear();
                    self.visual_start_idx = None;
                }
                self.status_message =
                    crate::tr!(self.lang, "已归档 {} 项", "archived {} items", count);
                self.reload()?;
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.pending_archive_ids.clear();
                let had_selection = !self.selected_ids.is_empty();
                self.set_mode(Mode::Normal);
                if had_selection {
                    self.selected_ids.clear();
                    self.visual_start_idx = None;
                }
                self.status_message =
                    crate::tr!(self.lang, "归档已取消", "Archive cancelled").into();
                self.reload()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_confirm_purge(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                let ids = std::mem::take(&mut self.pending_purge_ids);
                let had_selection = !self.selected_ids.is_empty();
                let mut count = 0;
                for id in &ids {
                    // 若当前番茄钟正聚焦于该任务，先停止它再删除。
                    if let Ok(pomo) = crate::repo::pomodoro::get_state() {
                        if pomo.task_id.as_deref() == Some(id.as_str()) {
                            let _ = crate::commands::pomo::stop();
                        }
                    }
                    if tasks::purge(self.conn, id).is_ok() {
                        count += 1;
                    }
                }
                self.set_mode(Mode::Normal);
                if had_selection {
                    self.selected_ids.clear();
                    self.visual_start_idx = None;
                }
                self.status_message = crate::tr!(
                    self.lang,
                    "已永久删除 {} 项",
                    "permanently deleted {} items",
                    count
                );
                self.reload()?;
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.pending_purge_ids.clear();
                let had_selection = !self.selected_ids.is_empty();
                self.set_mode(Mode::Normal);
                if had_selection {
                    self.selected_ids.clear();
                    self.visual_start_idx = None;
                }
                self.status_message = crate::tr!(self.lang, "删除已取消", "Purge cancelled").into();
                self.reload()?;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_confirm_profile_delete(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                let name = self.pending_profile_delete.take().unwrap_or_default();
                self.set_mode(Mode::Normal);
                self.settings_delete_profile(&name)?;
                self.reload()?;
            }
            KeyCode::Esc | KeyCode::Char('n') | KeyCode::Char('N') => {
                self.pending_profile_delete = None;
                self.set_mode(Mode::Normal);
                self.status_message =
                    crate::tr!(self.lang, "删除已取消", "Delete cancelled").into();
            }
            _ => {}
        }
        Ok(())
    }

    /// Tab：候选激活时向下推进；未激活时尝试补全当前词（正常情况下
    /// 实时补全已在输入时填充候选，此处兜底）。
    fn handle_tab_completion(&mut self) {
        if self.completion_active() {
            let n = self.completion_candidates.len();
            if n > 0 {
                self.completion_index = (self.completion_index + 1) % n;
                self.apply_current_completion();
            }
        } else {
            self.refresh_completion();
        }
    }

    /// 候选激活时上移选择。
    fn completion_up(&mut self) {
        if !self.completion_active() {
            return;
        }
        let n = self.completion_candidates.len();
        if n > 0 {
            self.completion_index = (self.completion_index + n - 1) % n;
            self.apply_current_completion();
        }
    }

    /// 候选激活时下移选择。
    fn completion_down(&mut self) {
        if !self.completion_active() {
            return;
        }
        let n = self.completion_candidates.len();
        if n > 0 {
            self.completion_index = (self.completion_index + 1) % n;
            self.apply_current_completion();
        }
    }

    /// 是否正在补全（候选列表打开且光标落在被补全 token 内）。
    pub(crate) fn completion_active(&self) -> bool {
        !self.completion_candidates.is_empty()
            && self
                .completion_range
                .map(|(s, e)| s <= self.input_cursor && self.input_cursor <= e)
                .unwrap_or(false)
    }

    /// 接受当前候选：补全完整 token，若光标后无内容则追加一个空格避免粘连，随后关闭候选。
    fn accept_completion(&mut self) {
        if self.completion_active() {
            self.apply_current_completion();
            if self.input[self.input_cursor..].is_empty() {
                self.input_insert_char(' ');
            }
        }
        self.clear_completion();
    }

    /// 取消补全：关闭候选（保留当前输入，由用户自行继续输入）。
    fn cancel_completion(&mut self) {
        self.clear_completion();
    }

    fn confirm_search(&mut self, input: &str) -> Result<()> {
        self.search_query = input.trim().to_string();
        if self.search_query.is_empty() {
            self.status_message = crate::tr!(self.lang, "已清除搜索", "Search cleared").into();
        } else {
            self.status_message =
                crate::tr!(self.lang, "搜索: {}", "Search: {}", self.search_query);
        }
        self.reload()
    }

    fn confirm_filter_tag(&mut self, input: &str) -> Result<()> {
        let t = input.trim().to_string();
        if t.is_empty() {
            self.tag_filter = None;
            self.status_message =
                crate::tr!(self.lang, "已清除标签过滤", "Tag filter cleared").into();
        } else {
            self.tag_filter = Some(t.clone());
            self.status_message = crate::tr!(self.lang, "过滤标签: @{}", "Filter tag: @{}", t);
        }
        self.reload()
    }

    /// 提交前校验捕获/编辑输入：时间必须可解析，循环必须有效；
    /// 时间解析为过去时刻却带循环时视为冲突。失败返回错误文案。
    fn validate_capture_input(&self, input: &str) -> Result<(), String> {
        let quick_add = crate::parser::parse_quick_add(input);
        if let Some(rr) = &quick_add.rrule {
            if !crate::parser::rrule_valid(rr) {
                return Err(crate::tr!(self.lang, "循环无效: {}", "bad rrule: {}", rr).to_string());
            }
        }
        if let Some(ts) = &quick_add.time_str {
            let parsed = time::parse_time(ts).map_err(|e| {
                crate::tr!(self.lang, "时间无效: {}", "bad time: {}", e).to_string()
            })?;
            if quick_add.rrule.is_some() && parsed < crate::time::now_ms() {
                return Err(crate::tr!(
                    self.lang,
                    "循环任务的时间需在未来 ({} 已过去)",
                    "recurring time must be in the future ({} is past)",
                    ts
                )
                .to_string());
            }
        }
        Ok(())
    }

    fn confirm_capture(&mut self, input: &str) -> Result<()> {
        let raw_input = input.trim();
        if raw_input.is_empty() {
            self.organizing_id = None;
            return Ok(());
        }
        let quick_add = crate::parser::parse_quick_add(raw_input);
        if let Some(id) = self.organizing_id.take() {
            // Inbox 滞留任务再编辑：与 capture 同一句话编辑器
            let Ok(task) = tasks::get(self.conn, &id) else {
                self.reload()?;
                return Ok(());
            };
            let start_ms = match &quick_add.time_str {
                Some(t) => match time::parse_time(t) {
                    Ok(ms) => Some(ms),
                    Err(e) => {
                        self.status_message =
                            crate::tr!(self.lang, "时间无效: {}", "bad time: {}", e);
                        self.reload()?;
                        return Ok(());
                    }
                },
                None => None,
            };
            // 循环有效性校验。
            if let Some(rr) = &quick_add.rrule {
                if !crate::parser::rrule_valid(rr) {
                    self.status_message =
                        crate::tr!(self.lang, "循环无效: {}", "bad rrule: {}", rr);
                    self.reload()?;
                    return Ok(());
                }
            }
            let mut ok = true;
            if !quick_add.title.is_empty() {
                ok &= self.note(tasks::rename(self.conn, &id, &quick_add.title));
            }
            let mut tag_names = quick_add.tags;
            if let Some(p) = &quick_add.priority {
                tag_names.push(p.clone());
            }
            let new_set: std::collections::HashSet<String> = tag_names.iter().cloned().collect();
            let old_tags = crate::repo::tags::get_task_tags(self.conn, &id).unwrap_or_default();
            for tg in &old_tags {
                if !new_set.contains(&tg.name) {
                    ok &= self.note(crate::repo::tags::remove_tag_from_task(
                        self.conn, &id, &tg.name,
                    ));
                }
            }
            for name in &tag_names {
                ok &= self.note(crate::repo::tags::add_tag_to_task(self.conn, &id, name));
            }
            // @quote 路由：组织/编辑时出现金句标签 → 工作态任务流转为参考资料，离开收件箱。
            let is_quote = self.quotes_enabled && new_set.contains(crate::repo::tasks::QUOTE_TAG);
            if is_quote && !matches!(task.status, task::Status::Reference | task::Status::Done) {
                ok &= self.note(tasks::transition(self.conn, &id, task::Status::Reference));
            }
            // ~time → 排程起点（自动分类 Inbox→Scheduled）；无时间则仅改周期。
            // 金句不参与排程：时间/周期让位给金句路由。
            if is_quote {
                // 金句保持 reference，忽略 ~time 与 *rrule 的排程效果。
            } else if let Some(start) = start_ms {
                if Some(start) != task.scheduled_start_at || quick_add.rrule != task.rrule {
                    ok &= self.note(tasks::schedule(
                        self.conn,
                        &id,
                        start,
                        None,
                        quick_add.rrule.clone(),
                    ));
                }
            } else if quick_add.rrule != task.rrule {
                ok &= self.note(tasks::set_rrule(self.conn, &id, quick_add.rrule.clone()));
            }
            if ok {
                self.status_message =
                    crate::tr!(self.lang, "已组织 {}", "organized {}", short_id(&id));
            }
            self.reload()?;
        } else {
            // 新建捕获：~time → 排程起点（创建后 schedule 设 scheduled_start_at, 状态 Scheduled, 无终点）
            let time_str = quick_add.time_str.clone();
            let rrule = quick_add.rrule;
            if let Some(ts) = &time_str {
                if let Err(e) = time::parse_time(ts) {
                    self.status_message = crate::tr!(self.lang, "时间无效: {}", "bad time: {}", e);
                    return Ok(());
                }
            }
            if let Some(rr) = &rrule {
                if !crate::parser::rrule_valid(rr) {
                    self.status_message =
                        crate::tr!(self.lang, "循环无效: {}", "bad rrule: {}", rr);
                    return Ok(());
                }
            }
            let mut tag_names = quick_add.tags;
            if let Some(p) = &quick_add.priority {
                tag_names.push(p.clone());
            }
            // @quote 路由：捕获输入含金句标签 → 直接创建为 reference + @quote，
            // 自动进入金句视图，不落收件箱。~time/*rrule 让位给金句。
            let is_quote =
                self.quotes_enabled && tag_names.iter().any(|t| t == crate::repo::tasks::QUOTE_TAG);
            let t = tasks::create_capture(
                self.conn,
                &CaptureInput {
                    title: quick_add.title,
                    status: if is_quote {
                        task::Status::Reference
                    } else if time_str.is_some() {
                        task::Status::Scheduled
                    } else {
                        task::Status::Inbox
                    },
                    due_at: None,
                    tag_names,
                    rrule: if time_str.is_some() || is_quote {
                        None
                    } else {
                        rrule.clone()
                    },
                    ..Default::default()
                },
            )?;
            if is_quote {
                self.set_view(View::Quotes);
                self.status_message = crate::tr!(
                    self.lang,
                    "已加入金句 {}",
                    "added to quotes {}",
                    short_id(&t.id)
                );
            } else {
                let scheduled_ok = if let Some(ts) = &time_str {
                    let start = time::parse_time(ts).unwrap();
                    self.note(tasks::schedule(self.conn, &t.id, start, None, rrule))
                } else {
                    true
                };
                self.set_view(View::Inbox);
                if scheduled_ok {
                    self.status_message =
                        crate::tr!(self.lang, "已捕获 {}", "captured {}", short_id(&t.id));
                }
            }
        }
        Ok(())
    }

    fn confirm_tagging(&mut self, input: &str) -> Result<()> {
        let name = input.trim();
        if !name.is_empty() {
            let mut ids = vec![];
            if !self.selected_ids.is_empty() {
                ids.extend(self.selected_ids.iter().cloned());
            } else if let Some(row) = self.items.get(self.selected).cloned() {
                ids.push(row.id);
            }

            let mut count = 0;
            for id in ids {
                if tags::add_tag_to_task(self.conn, &id, name).is_ok() {
                    count += 1;
                }
            }
            self.status_message = crate::tr!(
                self.lang,
                "已为 {} 项添加标签 +{}",
                "tagged {} items with +{}",
                count,
                name
            );
            self.selected_ids.clear();
            self.visual_start_idx = None;
            self.reload()?;
        }
        Ok(())
    }

    fn confirm_waiting_who(&mut self, input: &str) -> Result<()> {
        if let Some(row) = self.items.get(self.selected).cloned() {
            let who = input.trim();
            if !who.is_empty() {
                let new_title = format!("{} [Wait: {}]", row.title, who);
                tasks::rename(self.conn, &row.id, &new_title)?;
            }
            self.set_mode(Mode::WaitingWhen);
            self.input.clear();
            return Ok(());
        }
        Ok(())
    }

    fn confirm_waiting_when(&mut self, input: &str) -> Result<()> {
        let mut start_s = input.trim();
        if start_s.is_empty() {
            start_s = "+1d";
        }
        if let Some(row) = self.items.get(self.selected).cloned() {
            match time::parse_time(start_s) {
                Ok(start_ms) => {
                    tasks::schedule(self.conn, &row.id, start_ms, None, None)?;
                    let t = tasks::transition(self.conn, &row.id, task::Status::Waiting)?;
                    self.status_message =
                        crate::tr!(self.lang, "{} -> 等待中", "{} -> waiting", short_id(&t.id));
                    self.reload()?;
                }
                Err(e) => {
                    self.status_message = crate::tr!(self.lang, "时间无效: {}", "bad time: {}", e)
                }
            }
        }
        Ok(())
    }

    fn confirm_checklist_adding(&mut self, input: &str) -> Result<()> {
        if !input.is_empty() {
            if let Some(row) = self.items.get(self.selected).cloned() {
                if let Ok(mut task) = tasks::get(self.conn, &row.id) {
                    task.checklist.push(task::ChecklistItem {
                        id: uuid::Uuid::new_v4().to_string(),
                        title: input.to_string(),
                        done: false,
                    });
                    if self.note(tasks::update_checklist(
                        self.conn,
                        &task.id,
                        &task.checklist,
                    )) {
                        self.status_message =
                            crate::tr!(self.lang, "检查单 +1", "Checklist +1").to_string();
                    }
                    self.load_detail();
                }
            }
        }
        self.set_mode(Mode::Normal);
        self.input.clear();
        Ok(())
    }

    fn confirm_creating_tag(&mut self, input: &str) -> Result<()> {
        let name = input.trim().trim_start_matches('@');
        if !name.is_empty() && tags::find_or_create_tag(self.conn, name).is_ok() {
            self.status_message = crate::tr!(self.lang, "已创建标签: {}", "created tag: {}", name);
            self.refresh()?;
        }
        self.set_mode(Mode::Normal);
        self.input.clear();
        Ok(())
    }

    fn confirm_creating_profile(&mut self, input: &str) -> Result<()> {
        self.settings_new_profile(input)?;
        self.set_mode(Mode::Normal);
        self.input.clear();
        Ok(())
    }

    fn confirm_renaming_profile(&mut self, input: &str) -> Result<()> {
        self.settings_rename_profile(input)?;
        self.set_mode(Mode::Normal);
        self.input.clear();
        Ok(())
    }

    fn confirm_pomo_config(&mut self, input: &str) -> Result<()> {
        let parts: Vec<&str> = input.split(';').map(|s| s.trim()).collect();
        if parts.len() == 3 {
            if let (Ok(w), Ok(s), Ok(l)) = (
                parts[0].parse::<u32>(),
                parts[1].parse::<u32>(),
                parts[2].parse::<u32>(),
            ) {
                if w > 0 && s > 0 && l > 0 {
                    let mut pomo = crate::repo::pomodoro::get_state().unwrap_or_default();
                    pomo.config.work_mins = w;
                    pomo.config.short_break_mins = s;
                    pomo.config.long_break_mins = l;
                    if crate::repo::pomodoro::save_state(&pomo).is_ok() {
                        self.status_message = crate::tr!(
                            self.lang,
                            "🍅 番茄钟配置已更新: 工作 {}m / 短休 {}m / 长休 {}m",
                            "🍅 Pomo config updated: work {}m / short {}m / long {}m",
                            w,
                            s,
                            l
                        );
                    }
                } else {
                    self.status_message =
                        crate::tr!(self.lang, "时长必须大于0", "lengths must be > 0").into();
                }
            } else {
                self.status_message = crate::tr!(
                    self.lang,
                    "配置格式错误 (示例: 25;5;15)",
                    "invalid format (e.g. 25;5;15)"
                )
                .into();
            }
        } else {
            self.status_message = crate::tr!(
                self.lang,
                "格式必须包含3项 (工作;短休;长休)",
                "must have 3 parts (work;short;long)"
            )
            .into();
        }
        self.set_mode(Mode::Normal);
        self.input.clear();
        Ok(())
    }
}
