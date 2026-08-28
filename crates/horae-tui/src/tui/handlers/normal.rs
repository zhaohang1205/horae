use crate::tui::app::{App, Mode, Pane, View};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

impl<'a> App<'a> {
    /// 帮助抽屉打开时的滚动键，命中则消费该按键。
    pub(super) fn handle_help_navigation(&mut self, key: KeyEvent) -> bool {
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

    pub(super) fn handle_escape(&mut self, key: KeyEvent) -> Result<bool> {
        if key.code != KeyCode::Esc {
            return Ok(false);
        }
        if self.is_reviewing {
            self.is_reviewing = false;
            self.review_step = 0;
            self.status_message = tr!(self.lang, "周回顾已取消", "Weekly Review cancelled").into();
            {
                let r = self.reload();
                self.note(r);
            }
        } else if self.mode == Mode::Visual {
            self.set_mode(Mode::Normal);
            self.visual_start_idx = None;
            self.selected_ids.clear();
            self.status_message = tr!(self.lang, "已退出可视模式", "Exited visual mode").into();
            {
                let r = self.reload();
                self.note(r);
            }
        } else if self.tag_filter.is_some() || !self.search_query.is_empty() {
            self.tag_filter = None;
            self.search_query.clear();
            self.status_message = tr!(self.lang, "已清除过滤", "Cleared filters").into();
            {
                let r = self.reload();
                self.note(r);
            }
        } else if !self.selected_ids.is_empty() {
            self.selected_ids.clear();
            self.status_message = tr!(self.lang, "已清除选择", "Selection cleared").into();
        } else {
            self.hide_pomo_banner = true;
            self.status_message.clear();
        }
        Ok(true)
    }

    /// 系统级按键：退出、帮助、快捷键条、主题、语言、语法面板。
    pub(super) fn handle_system_keys(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('q') => self.should_quit = true,
            KeyCode::Char('?') | KeyCode::F(1) => self.show_help = !self.show_help,
            KeyCode::F(2) => {
                self.show_shortcut_bar = !self.show_shortcut_bar;
                self.status_message = if self.show_shortcut_bar {
                    tr!(self.lang, "已显示快捷键条", "Shortcut bar shown").into()
                } else {
                    tr!(
                        self.lang,
                        "已隐藏快捷键条 (F2 显示)",
                        "Shortcut bar hidden (F2 to show)"
                    )
                    .into()
                };
            }
            KeyCode::F(5) => {
                self.theme = self.theme.toggle();
                let saved = horae_core::repo::settings::set(
                    self.conn,
                    "theme",
                    if self.theme.is_dark { "mocha" } else { "latte" },
                );
                if self.note(saved) {
                    self.status_message = if self.theme.is_dark {
                        tr!(
                            self.lang,
                            "主题: Catppuccin 摩卡 (深色)",
                            "Theme: Catppuccin Mocha (Dark)"
                        )
                        .to_string()
                    } else {
                        tr!(
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
                    horae_core::i18n::Lang::Zh => horae_core::i18n::Lang::En,
                    horae_core::i18n::Lang::En => horae_core::i18n::Lang::Zh,
                };
                let key = match self.lang {
                    horae_core::i18n::Lang::Zh => "zh",
                    horae_core::i18n::Lang::En => "en",
                };
                let saved = horae_core::repo::settings::set(self.conn, "lang", key);
                if self.note(saved) {
                    self.status_message = match self.lang {
                        horae_core::i18n::Lang::Zh => "语言已切换为中文 (F6 切换)".to_string(),
                        horae_core::i18n::Lang::En => {
                            "Language switched to English (F6 to toggle)".to_string()
                        }
                    };
                }
            }
            KeyCode::F(7) => {
                self.popup = Some(crate::tui::app::Popup::ModuleToggles(0));
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

    pub(super) fn handle_navigation_keys(&mut self, key: KeyEvent) -> Result<bool> {
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
                    if v == View::Quotes && !self.quotes.enabled {
                        return Ok(true);
                    }
                    if v == View::Reference && !self.modules.reference {
                        return Ok(true);
                    }
                    if v == View::Done && !self.modules.done {
                        return Ok(true);
                    }
                    if v == View::Archived && !self.modules.archived {
                        return Ok(true);
                    }
                    if v == View::Tags && !self.modules.tags {
                        return Ok(true);
                    }
                    self.set_view(v);
                }
            }
            KeyCode::Char('J') => self.set_view(View::Today),
            KeyCode::Char('K') => self.set_view(View::Tomorrow),
            KeyCode::Char('M') => {
                if !self.modules.settings {
                    return Ok(true);
                }
                self.set_view(View::Settings);
            }
            KeyCode::Char('W') => self.set_view(View::Workflow),
            KeyCode::Char('g') => self.move_sel(-10000),
            KeyCode::Char('G') => self.move_sel(10000),
            _ => return Ok(false),
        }
        Ok(true)
    }

    pub(super) fn handle_review_keys(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('r') => {
                if !self.modules.review {
                    return Ok(true);
                }
                self.is_reviewing = true;
                self.review_step = 1;
                self.set_view(View::Inbox);
                self.status_message =
                    tr!(self.lang, "周回顾已开始", "Weekly Review started").into();
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
                            tr!(self.lang, "每周回顾完成! 🎉", "Weekly Review Complete! 🎉").into();
                    }
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 进入各类输入/编辑模式的按键。
    pub(super) fn handle_mode_switch_keys(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('v') | KeyCode::Char('V') => {
                if self.mode == Mode::Visual {
                    self.set_mode(Mode::Normal);
                    self.visual_start_idx = None;
                    self.selected_ids.clear();
                    self.status_message =
                        tr!(self.lang, "已退出可视模式", "Exited visual mode").into();
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
                let pomo = horae_core::repo::pomodoro::get_state().unwrap_or_default();
                self.input = format!(
                    "{};{};{};{}",
                    pomo.config.work_mins,
                    pomo.config.short_break_mins,
                    pomo.config.long_break_mins,
                    pomo.config.long_break_interval
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
}
