//! TUI 按键处理，按 `Mode` 分组拆分：
//! - [`normal`] — Normal 模式的共享按键组（系统键/导航/模式切换/周回顾/Esc）。
//! - [`actions`] — 任务动作、选择(Visual)、番茄钟、归档与设置按键。
//! - [`confirm`] — ConfirmArchive / ConfirmPurge / ConfirmProfileDelete 确认模式。
//! - [`input`] — 输入型模式的 Tab 补全与各 `confirm_*` 提交处理。
//! - [`checklist`] — ChecklistFocus / RenamingChecklist 检查单逐项管理。
//!
//! `AppHandlers` trait 与其实现（handle_key 分发、normal/input 入口、
//! confirm_input 路由、restore_selected）是一个整体，留在本文件；
//! 各子模块通过 `pub(super)` 方法供其调用。

use super::app::{App, Mode, ToastKind, View};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use horae_core::repo::tasks;

fn short_id(id: &str) -> String {
    id.chars().take(8).collect()
}
pub(crate) trait AppHandlers {
    fn handle_key(&mut self, key: KeyEvent) -> Result<()>;
    fn handle_paste(&mut self, text: String);
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

        // Error 级别提示保留到用户下一次按键，在新按键到来时清除旧错误。
        if self
            .toast
            .as_ref()
            .is_some_and(|t| t.kind == ToastKind::Error)
        {
            self.toast = None;
            self.status_message.clear();
        }

        if self.locked {
            return self.handle_lock_screen_key(key);
        }

        if let Some(popup) = self.popup.take() {
            match popup {
                crate::tui::app::Popup::ModuleToggles(mut idx) => {
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                            // close, implicitly handled because popup is taken and not put back
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            idx = (idx + 1) % 16;
                            self.popup = Some(crate::tui::app::Popup::ModuleToggles(idx));
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            idx = idx.checked_sub(1).unwrap_or(15);
                            self.popup = Some(crate::tui::app::Popup::ModuleToggles(idx));
                        }
                        KeyCode::Char(' ') => {
                            let mut persist_err: Option<anyhow::Error> = None;
                            match idx {
                                0 => {
                                    if let Err(e) = self.modules.set_enabled(
                                        self.conn,
                                        "module_splash",
                                        !self.modules.splash,
                                    ) {
                                        persist_err = Some(e);
                                    }
                                }
                                1 => {
                                    if let Err(e) = self.modules.set_enabled(
                                        self.conn,
                                        "module_reference",
                                        !self.modules.reference,
                                    ) {
                                        persist_err = Some(e);
                                    }
                                }
                                2 => {
                                    if let Err(e) = self.modules.set_enabled(
                                        self.conn,
                                        "module_done",
                                        !self.modules.done,
                                    ) {
                                        persist_err = Some(e);
                                    }
                                }
                                3 => {
                                    if let Err(e) = self.modules.set_enabled(
                                        self.conn,
                                        "module_archived",
                                        !self.modules.archived,
                                    ) {
                                        persist_err = Some(e);
                                    }
                                }
                                4 => {
                                    if let Err(e) = self.modules.set_enabled(
                                        self.conn,
                                        "module_tags",
                                        !self.modules.tags,
                                    ) {
                                        persist_err = Some(e);
                                    }
                                }
                                5 => {
                                    if let Err(e) = self.quotes.toggle_enabled(self.conn) {
                                        persist_err = Some(e);
                                    }
                                    if !self.quotes.enabled && self.view == View::Quotes {
                                        self.set_view(View::Inbox);
                                    }
                                }
                                6 => {
                                    if let Err(e) = self.modules.set_enabled(
                                        self.conn,
                                        "module_review",
                                        !self.modules.review,
                                    ) {
                                        persist_err = Some(e);
                                    }
                                }
                                7 => {
                                    if let Err(e) = self.modules.set_enabled(
                                        self.conn,
                                        "module_settings",
                                        !self.modules.settings,
                                    ) {
                                        persist_err = Some(e);
                                    }
                                    if !self.modules.settings && self.view == View::Settings {
                                        self.set_view(View::Inbox);
                                    }
                                }
                                8 => {
                                    use crate::tui::icons::IconStyle;
                                    self.icon_style = match self.icon_style {
                                        IconStyle::Nerd => IconStyle::Ascii,
                                        IconStyle::Ascii => IconStyle::Nerd,
                                    };
                                    self.save_setting("icons", self.icon_style.key());
                                }
                                9 => {
                                    // 启动即快速录入：翻转并持久化到 settings 表。
                                    self.start_in_capture = !self.start_in_capture;
                                    self.save_setting(
                                        "start_capture",
                                        if self.start_in_capture { "1" } else { "0" },
                                    );
                                }
                                10 => {
                                    // 补全模式切换：语法参考 (Reference) <-> 极速补全 (Speed)。
                                    use crate::tui::app::completion::CompletionStyle;
                                    self.completion_style = match self.completion_style {
                                        CompletionStyle::Reference => CompletionStyle::Speed,
                                        CompletionStyle::Speed => CompletionStyle::Reference,
                                    };
                                    self.save_setting(
                                        "completion_style",
                                        self.completion_style.key(),
                                    );
                                }
                                11 => {
                                    // 纯净录入无干扰：翻转并持久化到 settings 表。
                                    self.zen_capture = !self.zen_capture;
                                    self.save_setting(
                                        "zen_capture",
                                        if self.zen_capture { "1" } else { "0" },
                                    );
                                }
                                12 => {
                                    // 闪念录入即退出：翻转并持久化到 settings 表。
                                    self.flash_mode = !self.flash_mode;
                                    self.save_setting(
                                        "flash_mode",
                                        if self.flash_mode { "1" } else { "0" },
                                    );
                                }
                                13 => {
                                    // 农历与节气提醒：翻转并持久化到 settings 表。
                                    self.lunar_enabled = !self.lunar_enabled;
                                    self.save_setting(
                                        "lunar_reminder",
                                        if self.lunar_enabled { "1" } else { "0" },
                                    );
                                    if self.lunar_enabled {
                                        let today_date = chrono::Local::now().naive_local().date();
                                        self.calendar_info =
                                            horae_core::lunar::day_calendar_info(today_date);
                                    } else {
                                        self.calendar_info = None;
                                    }
                                }
                                14 => {
                                    // 提示音效：翻转并持久化到 settings 表。
                                    self.sound_enabled = !self.sound_enabled;
                                    self.save_setting(
                                        "sound_enabled",
                                        if self.sound_enabled { "1" } else { "0" },
                                    );
                                    if self.sound_enabled {
                                        horae_core::sound::play(
                                            horae_core::sound::SoundKind::TaskCompleted,
                                        );
                                    }
                                }
                                15 => {
                                    // 密码保护模式设置：未开启则打开设置弹窗，已开启则打开关闭/修改弹窗。
                                    if !self.password_enabled {
                                        self.popup = Some(crate::tui::app::Popup::PasswordSet {
                                            is_confirm: false,
                                            input: String::new(),
                                            first_pass: String::new(),
                                            error: None,
                                        });
                                        return Ok(());
                                    } else {
                                        self.popup =
                                            Some(crate::tui::app::Popup::PasswordDisable {
                                                input: String::new(),
                                                error: None,
                                            });
                                        return Ok(());
                                    }
                                }
                                _ => {}
                            }
                            if let Some(e) = persist_err {
                                self.flash(
                                    ToastKind::Warning,
                                    tr!(
                                        self.lang,
                                        "设置未能保存，下次启动将恢复原值: {}",
                                        "Failed to save setting (will reset next launch): {}",
                                        e
                                    ),
                                );
                            }
                            self.popup = Some(crate::tui::app::Popup::ModuleToggles(idx));
                            let r = self.reload();
                            self.note(r);
                        }
                        _ => {
                            self.popup = Some(crate::tui::app::Popup::ModuleToggles(idx));
                        }
                    }
                }
                crate::tui::app::Popup::TaskDueNow(id, _) if key.code == KeyCode::Enter => {
                    self.note(horae_core::pomo::start(self.conn, &id));
                    self.needs_clear = true;
                }
                // 其余按键（含 Esc）直接关闭到期提醒：popup 已被 take 走。
                crate::tui::app::Popup::TaskDueNow(_, _) => {}
                crate::tui::app::Popup::PasswordSet {
                    mut is_confirm,
                    mut input,
                    mut first_pass,
                    mut error,
                } => {
                    self.handle_password_set_key(
                        key,
                        &mut is_confirm,
                        &mut input,
                        &mut first_pass,
                        &mut error,
                    )?;
                }
                crate::tui::app::Popup::PasswordDisable {
                    mut input,
                    mut error,
                } => {
                    self.handle_password_disable_key(key, &mut input, &mut error)?;
                }
                crate::tui::app::Popup::PasswordChange {
                    mut step,
                    mut old_pass,
                    mut new_pass,
                    mut input,
                    mut error,
                } => {
                    self.handle_password_change_key(
                        key,
                        &mut step,
                        &mut old_pass,
                        &mut new_pass,
                        &mut input,
                        &mut error,
                    )?;
                }
            }
            return Ok(());
        }

        // 番茄钟全屏接管界面时，不能继续让输入模式拦截其快捷键。
        if self.pomo.phase != horae_core::model::pomodoro::Phase::Idle && self.mode.is_input() {
            self.set_mode(Mode::Normal);
            self.input_clear();
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
        if self.show_syntax && self.handle_syntax_navigation(key) {
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
        if self.handle_checklist_enter(key)? {
            return Ok(());
        }
        Ok(())
    }

    fn handle_input(&mut self, key: KeyEvent) -> Result<()> {
        if self.mode == Mode::ChecklistFocus {
            return self.handle_checklist_keys(key);
        }
        if self.mode == Mode::RenamingChecklist && key.code == KeyCode::Esc {
            self.set_mode(Mode::ChecklistFocus);
            self.input.clear();
            self.load_detail();
            return Ok(());
        }
        if self.mode == Mode::ConfirmArchive {
            return self.handle_confirm_archive(key);
        }
        if self.mode == Mode::ConfirmPurge {
            return self.handle_confirm_purge(key);
        }
        if self.mode == Mode::ConfirmProfileDelete {
            return self.handle_confirm_profile_delete(key);
        }
        if self.completion_active() {
            match key.code {
                // 采纳补全：Tab (通用) / Ctrl+Y (Vim)
                KeyCode::Tab => {
                    self.accept_completion();
                    return Ok(());
                }
                KeyCode::Char('y') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.accept_completion();
                    return Ok(());
                }
                // 取消候选列表：Esc / Ctrl+E (Vim) / Ctrl+G (Emacs)
                KeyCode::Esc => {
                    self.cancel_completion();
                    return Ok(());
                }
                KeyCode::Char('e' | 'g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.cancel_completion();
                    return Ok(());
                }
                // 下一项：Down / Ctrl+N / Ctrl+J / Alt+] / Alt+N / PageDown
                KeyCode::Down | KeyCode::PageDown => {
                    self.completion_down();
                    return Ok(());
                }
                KeyCode::Char('n' | 'j') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.completion_down();
                    return Ok(());
                }
                KeyCode::Char(']' | 'n') if key.modifiers.contains(KeyModifiers::ALT) => {
                    self.completion_down();
                    return Ok(());
                }
                // 上一项：Up / BackTab (Shift+Tab) / Ctrl+P / Ctrl+K / Alt+[ / Alt+P / PageUp
                KeyCode::Up | KeyCode::PageUp | KeyCode::BackTab => {
                    self.completion_up();
                    return Ok(());
                }
                KeyCode::Char('p' | 'k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.completion_up();
                    return Ok(());
                }
                KeyCode::Char('[' | 'p') if key.modifiers.contains(KeyModifiers::ALT) => {
                    self.completion_up();
                    return Ok(());
                }
                _ => {}
            }
        }

        // 语音录入/转写期间的独占按键处理 (Alt+V / F8 / Enter / Esc / Ctrl+G)
        if self.mode == Mode::Capturing && (self.is_voice_recording() || self.voice_transcribing) {
            match key.code {
                KeyCode::Esc => {
                    self.cancel_voice_recording();
                }
                KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                    self.cancel_voice_recording();
                }
                KeyCode::Enter | KeyCode::F(8) if self.is_voice_recording() => {
                    self.finish_voice_recording()?;
                }
                KeyCode::Char('v')
                    if key.modifiers.contains(KeyModifiers::ALT) && self.is_voice_recording() =>
                {
                    self.finish_voice_recording()?;
                }
                _ => {}
            }
            return Ok(());
        }

        // 启动语音录入快捷键 (Alt+V / F8，避免抢占 Ctrl+V 粘贴)
        if self.mode == Mode::Capturing
            && ((key.code == KeyCode::Char('v') && key.modifiers.contains(KeyModifiers::ALT))
                || key.code == KeyCode::F(8))
        {
            self.start_voice_recording()?;
            return Ok(());
        }

        match key.code {
            KeyCode::Esc => {
                self.organizing_id = None;
                self.show_syntax = false;
                self.syntax_scroll = 0;
                self.set_mode(Mode::Normal);
                self.input_clear();
                self.reload()?;
            }
            KeyCode::Char('g') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.organizing_id = None;
                self.show_syntax = false;
                self.syntax_scroll = 0;
                self.set_mode(Mode::Normal);
                self.input_clear();
                self.reload()?;
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
                let is_flash_capture = self.flash_mode
                    && mode == Mode::Capturing
                    && self.organizing_id.is_none()
                    && !input.trim().is_empty();
                self.set_mode(Mode::Normal);
                self.input_clear();
                self.confirm_input(mode, &input)?;
                if is_flash_capture {
                    self.should_quit = true;
                }
            }
            KeyCode::Tab => {
                self.handle_tab_completion();
            }
            KeyCode::BackTab => {
                self.completion_up();
            }
            KeyCode::PageUp => {
                if self.show_syntax {
                    self.syntax_scroll = self.syntax_scroll.saturating_sub(4);
                }
            }
            KeyCode::PageDown => {
                if self.show_syntax {
                    self.syntax_scroll = self.syntax_scroll.saturating_add(4).min(20);
                }
            }
            KeyCode::Up => {
                if self.completion_active() {
                    self.completion_up();
                } else if self.show_syntax {
                    self.syntax_scroll = self.syntax_scroll.saturating_sub(2);
                }
            }
            KeyCode::Down => {
                if self.completion_active() {
                    self.completion_down();
                } else if self.show_syntax {
                    self.syntax_scroll = self.syntax_scroll.saturating_add(2).min(20);
                }
            }
            KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.completion_active() {
                    self.completion_down();
                } else if self.show_syntax {
                    self.syntax_scroll = self.syntax_scroll.saturating_add(2).min(20);
                }
            }
            KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                if self.completion_active() {
                    self.completion_up();
                } else {
                    self.show_syntax = !self.show_syntax;
                    self.syntax_scroll = 0;
                }
            }
            KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input_delete_word_backward();
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input_kill_to_start();
            }
            KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input_kill_to_end();
            }
            KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input_home();
            }
            KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input_end();
            }
            KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input_move_left();
            }
            KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input_move_right();
            }
            KeyCode::Char('h') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input_backspace();
            }
            KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.input_delete();
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

    /// 处理剪贴板粘贴（`Event::Paste`）：把文本插入到快速录入/各输入弹层。
    /// 处于普通列表模式时，先打开快速录入再粘贴，直接满足「复制到快速录入」。
    /// 三个确认提示模式（归档/删除/删 profile）只等待 y/n，不接受文本，忽略。
    fn handle_paste(&mut self, text: String) {
        match self.mode {
            Mode::Normal => {
                self.set_mode(Mode::Capturing);
                self.input_clear();
                self.input_insert_str(&text);
            }
            Mode::ConfirmArchive | Mode::ConfirmPurge | Mode::ConfirmProfileDelete => {}
            _ if self.mode.is_input() => self.input_insert_str(&text),
            _ => {}
        }
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
            Mode::RenamingChecklist => self.confirm_checklist_rename(input)?,
            Mode::CreatingTag => self.confirm_creating_tag(input)?,
            Mode::ConfiguringPomo => self.confirm_pomo_config(input)?,
            Mode::CreatingProfile => self.confirm_creating_profile(input)?,
            Mode::RenamingProfile => self.confirm_renaming_profile(input)?,
            Mode::Normal
            | Mode::Visual
            | Mode::ChecklistFocus
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
        let mut restored_titles = Vec::new();
        let mut first_err: Option<anyhow::Error> = None;
        for id in &ids {
            let title = tasks::get(self.conn, id)
                .map(|t| t.title)
                .unwrap_or_default();
            match tasks::unarchive(self.conn, id) {
                Ok(_) => {
                    count += 1;
                    restored_titles.push((id.clone(), title));
                }
                Err(e) => {
                    if first_err.is_none() {
                        first_err = Some(e);
                    }
                }
            }
        }
        for (id, title) in restored_titles {
            self.push_undo(crate::tui::app::UndoAction::Unarchive { task_id: id, title });
        }
        if count == 1 && ids.len() == 1 {
            self.set_toast(
                tr!(self.lang, "✓ 已恢复 {}", "✓ restored {}", short_id(&ids[0])),
                true,
            );
        } else if count > 0 {
            self.set_toast(
                tr!(
                    self.lang,
                    "✓ 已恢复 {} 项 (按 u 撤销)",
                    "✓ restored {} items (press u to undo)",
                    count
                ),
                true,
            );
        } else if let Some(e) = first_err {
            self.flash(
                ToastKind::Error,
                tr!(self.lang, "恢复失败: {}", "Restore failed: {}", e),
            );
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
    pub(super) fn handle_lock_screen_key(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Enter => {
                if horae_core::password::verify_password(self.conn, &self.lock_input)? {
                    self.locked = false;
                    self.lock_input.clear();
                    self.lock_error = None;
                    self.status_message = tr!(self.lang, "已解锁", "Unlocked").to_string();
                    let r = self.reload();
                    self.note(r);
                } else {
                    self.lock_input.clear();
                    self.lock_error = Some(
                        tr!(
                            self.lang,
                            "密码错误，请重试",
                            "Incorrect password, please try again"
                        )
                        .to_string(),
                    );
                }
            }
            KeyCode::Esc | KeyCode::Char('q')
                if key.code == KeyCode::Esc || key.code == KeyCode::Char('q') =>
            {
                self.should_quit = true;
            }
            KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.should_quit = true;
            }
            KeyCode::Backspace => {
                self.lock_input.pop();
                self.lock_error = None;
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.lock_input.clear();
                self.lock_error = None;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                self.lock_input.push(c);
                self.lock_error = None;
            }
            _ => {}
        }
        Ok(())
    }

    fn handle_password_set_key(
        &mut self,
        key: KeyEvent,
        is_confirm: &mut bool,
        input: &mut String,
        first_pass: &mut String,
        error: &mut Option<String>,
    ) -> Result<()> {
        match key.code {
            KeyCode::Esc => {
                return Ok(());
            }
            KeyCode::Backspace => {
                input.pop();
                *error = None;
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                input.clear();
                *error = None;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                input.push(c);
                *error = None;
            }
            KeyCode::Enter => {
                if !*is_confirm {
                    if input.trim().is_empty() {
                        *error = Some(
                            tr!(self.lang, "密码不能为空", "Password cannot be empty").to_string(),
                        );
                    } else {
                        *first_pass = input.clone();
                        input.clear();
                        *is_confirm = true;
                        *error = None;
                    }
                } else if input == first_pass {
                    horae_core::password::set_password(self.conn, input)?;
                    self.password_enabled = true;
                    self.status_message = tr!(
                        self.lang,
                        "密码保护模式已开启",
                        "Password protection enabled"
                    )
                    .to_string();
                    let r = self.reload();
                    self.note(r);
                    return Ok(());
                } else {
                    *error = Some(
                        tr!(
                            self.lang,
                            "两次输入的密码不一致，请重试",
                            "Passwords do not match, please try again"
                        )
                        .to_string(),
                    );
                    *is_confirm = false;
                    input.clear();
                    first_pass.clear();
                }
            }
            _ => {}
        }
        self.popup = Some(crate::tui::app::Popup::PasswordSet {
            is_confirm: *is_confirm,
            input: input.clone(),
            first_pass: first_pass.clone(),
            error: error.clone(),
        });
        Ok(())
    }

    fn handle_password_disable_key(
        &mut self,
        key: KeyEvent,
        input: &mut String,
        error: &mut Option<String>,
    ) -> Result<()> {
        match key.code {
            KeyCode::Esc => return Ok(()),
            KeyCode::Tab => {
                self.popup = Some(crate::tui::app::Popup::PasswordChange {
                    step: 0,
                    old_pass: String::new(),
                    new_pass: String::new(),
                    input: String::new(),
                    error: None,
                });
                return Ok(());
            }
            KeyCode::Backspace => {
                input.pop();
                *error = None;
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                input.clear();
                *error = None;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                input.push(c);
                *error = None;
            }
            KeyCode::Enter => match horae_core::password::verify_password(self.conn, input) {
                Ok(true) => {
                    horae_core::password::disable_password(self.conn)?;
                    self.password_enabled = false;
                    self.status_message = tr!(
                        self.lang,
                        "密码保护模式已关闭",
                        "Password protection disabled"
                    )
                    .to_string();
                    let r = self.reload();
                    self.note(r);
                    return Ok(());
                }
                _ => {
                    input.clear();
                    *error = Some(
                        tr!(
                            self.lang,
                            "密码错误，无法关闭",
                            "Incorrect password, cannot disable"
                        )
                        .to_string(),
                    );
                }
            },
            _ => {}
        }
        self.popup = Some(crate::tui::app::Popup::PasswordDisable {
            input: input.clone(),
            error: error.clone(),
        });
        Ok(())
    }

    fn handle_password_change_key(
        &mut self,
        key: KeyEvent,
        step: &mut u8,
        old_pass: &mut String,
        new_pass: &mut String,
        input: &mut String,
        error: &mut Option<String>,
    ) -> Result<()> {
        match key.code {
            KeyCode::Esc => return Ok(()),
            KeyCode::Backspace => {
                input.pop();
                *error = None;
            }
            KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                input.clear();
                *error = None;
            }
            KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                input.push(c);
                *error = None;
            }
            KeyCode::Enter => match *step {
                0 => match horae_core::password::verify_password(self.conn, input) {
                    Ok(true) => {
                        *old_pass = input.clone();
                        input.clear();
                        *step = 1;
                        *error = None;
                    }
                    _ => {
                        input.clear();
                        *error = Some(
                            tr!(self.lang, "原密码错误", "Incorrect original password").to_string(),
                        );
                    }
                },
                1 => {
                    if input.trim().is_empty() {
                        *error = Some(
                            tr!(self.lang, "新密码不能为空", "New password cannot be empty")
                                .to_string(),
                        );
                    } else {
                        *new_pass = input.clone();
                        input.clear();
                        *step = 2;
                        *error = None;
                    }
                }
                _ => {
                    if input == new_pass {
                        horae_core::password::set_password(self.conn, input)?;
                        self.password_enabled = true;
                        self.status_message = tr!(
                            self.lang,
                            "密码已修改并更新",
                            "Password updated successfully"
                        )
                        .to_string();
                        let r = self.reload();
                        self.note(r);
                        return Ok(());
                    } else {
                        *error = Some(
                            tr!(
                                self.lang,
                                "两次输入的密码不一致，请重试",
                                "Passwords do not match, please try again"
                            )
                            .to_string(),
                        );
                        *step = 1;
                        input.clear();
                        new_pass.clear();
                    }
                }
            },
            _ => {}
        }
        self.popup = Some(crate::tui::app::Popup::PasswordChange {
            step: *step,
            old_pass: old_pass.clone(),
            new_pass: new_pass.clone(),
            input: input.clone(),
            error: error.clone(),
        });
        Ok(())
    }
}

mod actions;
mod checklist;
mod confirm;
mod input;
mod normal;
