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

use super::app::{App, Mode, View};
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

        if let Some(popup) = self.popup.take() {
            match popup {
                crate::tui::app::Popup::ModuleToggles(mut idx) => {
                    match key.code {
                        KeyCode::Esc | KeyCode::Enter | KeyCode::Char('q') => {
                            // close, implicitly handled because popup is taken and not put back
                        }
                        KeyCode::Char('j') | KeyCode::Down => {
                            idx = (idx + 1) % 11;
                            self.popup = Some(crate::tui::app::Popup::ModuleToggles(idx));
                        }
                        KeyCode::Char('k') | KeyCode::Up => {
                            idx = idx.checked_sub(1).unwrap_or(10);
                            self.popup = Some(crate::tui::app::Popup::ModuleToggles(idx));
                        }
                        KeyCode::Char(' ') => {
                            match idx {
                                0 => {
                                    let _ = self.modules.set_enabled(
                                        self.conn,
                                        "module_splash",
                                        !self.modules.splash,
                                    );
                                }
                                1 => {
                                    let _ = self.modules.set_enabled(
                                        self.conn,
                                        "module_reference",
                                        !self.modules.reference,
                                    );
                                }
                                2 => {
                                    let _ = self.modules.set_enabled(
                                        self.conn,
                                        "module_done",
                                        !self.modules.done,
                                    );
                                }
                                3 => {
                                    let _ = self.modules.set_enabled(
                                        self.conn,
                                        "module_archived",
                                        !self.modules.archived,
                                    );
                                }
                                4 => {
                                    let _ = self.modules.set_enabled(
                                        self.conn,
                                        "module_tags",
                                        !self.modules.tags,
                                    );
                                }
                                5 => {
                                    let _ = self.quotes.toggle_enabled(self.conn);
                                    if !self.quotes.enabled && self.view == View::Quotes {
                                        self.set_view(View::Inbox);
                                    }
                                }
                                6 => {
                                    let _ = self.modules.set_enabled(
                                        self.conn,
                                        "module_review",
                                        !self.modules.review,
                                    );
                                }
                                7 => {
                                    let _ = self.modules.set_enabled(
                                        self.conn,
                                        "module_settings",
                                        !self.modules.settings,
                                    );
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
                                    let _ = horae_core::repo::settings::set(
                                        self.conn,
                                        "icons",
                                        self.icon_style.key(),
                                    );
                                }
                                9 => {
                                    // 启动即快速录入：翻转并持久化到 settings 表。
                                    self.start_in_capture = !self.start_in_capture;
                                    let _ = horae_core::repo::settings::set(
                                        self.conn,
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
                                    let _ = horae_core::repo::settings::set(
                                        self.conn,
                                        "completion_style",
                                        self.completion_style.key(),
                                    );
                                }
                                _ => {}
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
                crate::tui::app::Popup::TaskDueNow(_, _) => {}
                _ => {
                    // other popups close on any key
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
            KeyCode::Tab if self.completion_active() => {
                // 候选激活：Tab 采纳补全（补齐 token，留在编辑）。
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
        for id in &ids {
            if tasks::unarchive(self.conn, id).is_ok() {
                count += 1;
            }
        }
        if count == 1 && ids.len() == 1 {
            self.status_message = tr!(self.lang, "已恢复 {}", "restored {}", short_id(&ids[0]));
        } else {
            self.status_message = tr!(self.lang, "已恢复 {} 项", "restored {} items", count);
        }
        if !self.selected_ids.is_empty() {
            self.selected_ids.clear();
            self.visual_start_idx = None;
        }
        self.reload()?;
        Ok(())
    }
}

mod actions;
mod checklist;
mod confirm;
mod input;
mod normal;
