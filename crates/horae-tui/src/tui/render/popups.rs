use super::AppRender;
use crate::tui::app::{App, Mode};
use ratatui::symbols::border;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
    Frame,
};

impl<'a> App<'a> {
    /// 批量操作确认弹层：归档 / 永久删除的居中醒目确认框。
    pub(super) fn render_confirm_overlay(&mut self, f: &mut Frame, size: Rect) {
        let (title, desc) = match self.mode {
            Mode::ConfirmPurge => (
                tr!(
                    self.lang,
                    " ⚠ 确认永久删除 ",
                    " ⚠ Confirm permanent delete "
                ),
                tr!(
                    self.lang,
                    "将永久删除 {} 项，不可恢复。",
                    "Permanently delete {} item(s). This cannot be undone.",
                    self.pending_purge_ids.len()
                ),
            ),
            Mode::ConfirmProfileDelete => (
                tr!(
                    self.lang,
                    " ⚠ 确认删除 profile ",
                    " ⚠ Confirm delete profile "
                ),
                tr!(
                    self.lang,
                    "从配置移除 profile `{}`（数据库文件保留）。",
                    "Remove profile `{}` from config (db file kept).",
                    self.pending_profile_delete.as_deref().unwrap_or("")
                ),
            ),
            _ => (
                tr!(self.lang, " ⚠ 确认归档 ", " ⚠ Confirm archive "),
                tr!(
                    self.lang,
                    "将归档 {} 项。",
                    "Archive {} item(s).",
                    self.pending_archive_ids.len()
                ),
            ),
        };

        let lines = vec![
            Line::from(Span::styled(
                format!(" {}", desc),
                Style::default().fg(self.theme.fg),
            )),
            Line::from(""),
            Line::from(vec![
                Span::styled(
                    tr!(self.lang, "  [y/Enter] ", "  [y/Enter] "),
                    Style::default()
                        .fg(self.theme.text_success)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(tr!(self.lang, "确认  ", "Confirm  ")),
                Span::styled(
                    tr!(self.lang, "[n/Esc] ", "[n/Esc] "),
                    Style::default()
                        .fg(self.theme.text_urgent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(tr!(self.lang, "取消", "Cancel")),
            ]),
        ];
        let height = lines.len() as u16 + 2;
        let area = self.centered_rect(58, height, size);

        f.render_widget(ratatui::widgets::Clear, area);
        let block = Block::default()
            .title(title)
            .borders(Borders::ALL)
            .border_set(border::ROUNDED)
            .padding(ratatui::widgets::Padding::horizontal(1))
            .border_style(
                Style::default()
                    .fg(self.theme.text_urgent)
                    .add_modifier(Modifier::BOLD),
            );
        f.render_widget(Paragraph::new(lines).block(block), area);
    }

    /// 弹出框：模块开关 / 任务到期提醒。
    pub(super) fn render_popups(&mut self, f: &mut Frame, size: Rect) {
        let Some(ref popup) = self.popup else { return };
        match popup {
            crate::tui::app::Popup::ModuleToggles(idx) => {
                let area = self.centered_rect(46, 22, size);
                f.render_widget(ratatui::widgets::Clear, area);
                let block = Block::default()
                    .title(tr!(self.lang, " 模块显示设置 ", " Module Visibility "))
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .border_style(Style::default().fg(self.theme.accent));

                let mut items = vec![];
                let nerd = matches!(self.icon_style, crate::tui::icons::IconStyle::Nerd);
                let icons_label = if nerd {
                    tr!(self.lang, "图标 (Nerd Font)", "Icons (Nerd Font)")
                } else {
                    tr!(self.lang, "图标 (ASCII 回退)", "Icons (ASCII fallback)")
                };
                let is_ref = matches!(
                    self.completion_style,
                    crate::tui::app::completion::CompletionStyle::Reference
                );
                let completion_label = self.completion_style.label(self.lang);
                let opts = [
                    (
                        self.modules.splash,
                        tr!(self.lang, "开屏页 (Splash)", "Splash screen"),
                    ),
                    (
                        self.modules.reference,
                        tr!(self.lang, "6 参考资料 (Reference)", "6 Reference"),
                    ),
                    (
                        self.modules.done,
                        tr!(self.lang, "7 已完成 (Done)", "7 Done"),
                    ),
                    (
                        self.modules.archived,
                        tr!(self.lang, "8 归档箱 (Archived)", "8 Archived"),
                    ),
                    (
                        self.modules.tags,
                        tr!(self.lang, "9 标签库 (Tags)", "9 Tags"),
                    ),
                    (
                        self.quotes.enabled,
                        tr!(self.lang, "0 金句 (Quotes)", "0 Quotes"),
                    ),
                    (
                        self.modules.review,
                        tr!(self.lang, "r 周回顾 (Review)", "r Review"),
                    ),
                    (
                        self.modules.settings,
                        tr!(self.lang, "M 设置 (Settings)", "M Settings"),
                    ),
                    (nerd, icons_label),
                    (
                        self.start_in_capture,
                        tr!(
                            self.lang,
                            "启动即快速录入 (Capture)",
                            "Start in capture mode"
                        ),
                    ),
                    (is_ref, completion_label),
                    (
                        self.zen_capture,
                        tr!(
                            self.lang,
                            "纯净录入无干扰 (Zen)",
                            "Zen capture (no distractions)"
                        ),
                    ),
                    (
                        self.flash_mode,
                        tr!(
                            self.lang,
                            "闪念录入即退出 (Flash)",
                            "Flash mode (exit on capture)"
                        ),
                    ),
                    (
                        self.lunar_enabled,
                        tr!(
                            self.lang,
                            "农历与节气提醒 (Lunar)",
                            "Lunar & holiday reminders (Lunar)"
                        ),
                    ),
                    (
                        self.sound_enabled,
                        tr!(self.lang, "提示音效 (Sound)", "Sound effects (Sound)"),
                    ),
                    (
                        self.password_enabled,
                        tr!(
                            self.lang,
                            "密码保护模式 (Password)",
                            "Password protection (Password)"
                        ),
                    ),
                ];
                for (i, (enabled, name)) in opts.iter().enumerate() {
                    let checkbox = if *enabled { "[x]" } else { "[ ]" };
                    let mut style = Style::default();
                    if i == *idx {
                        style = style
                            .fg(self.theme.bg)
                            .bg(self.theme.accent)
                            .add_modifier(Modifier::BOLD);
                    } else if !*enabled {
                        style = style.fg(self.theme.text_dim);
                    }
                    items.push(ratatui::widgets::ListItem::new(Line::from(Span::styled(
                        format!(" {} {} ", checkbox, name),
                        style,
                    ))));
                }

                let list = ratatui::widgets::List::new(items).block(block);
                f.render_widget(list, area);
            }
            crate::tui::app::Popup::TaskDueNow(_, title) => {
                let mut lines = vec![Line::from(tr!(
                    self.lang,
                    "有任务到期或已到开始时间，需要立即处理：",
                    "A task is due or starting now, handle it now:"
                ))];
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    format!(" 「{}」 ", title),
                    Style::default()
                        .fg(self.theme.text_urgent)
                        .add_modifier(Modifier::BOLD),
                )));
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    tr!(
                        self.lang,
                        " [Enter] 一键进入番茄钟  |  [Esc] 忽略 ",
                        " [Enter] start pomodoro  |  [Esc] dismiss "
                    ),
                    Style::default().fg(self.theme.text_dim),
                )));

                let area = self.centered_rect(50, 10, size);
                f.render_widget(ratatui::widgets::Clear, area);
                let block = Block::default()
                    .title(tr!(self.lang, " ⏰ 任务提醒! ", " ⏰ Task due! "))
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .border_style(Style::default().fg(self.theme.text_urgent));
                f.render_widget(
                    Paragraph::new(lines)
                        .block(block)
                        .alignment(Alignment::Center),
                    area,
                );
            }
            crate::tui::app::Popup::TodoistConfig {
                project_id,
                token_env,
                focus,
                error,
            } => {
                let area = self.centered_rect(68, 15, size);
                f.render_widget(ratatui::widgets::Clear, area);
                let block = Block::default()
                    .title(tr!(
                        self.lang,
                        " Todoist 项目配置 ",
                        " Todoist Project Setup "
                    ))
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .border_style(Style::default().fg(self.theme.accent));
                let field_style = |selected: bool| {
                    if selected {
                        Style::default()
                            .fg(self.theme.accent)
                            .add_modifier(Modifier::BOLD)
                    } else {
                        Style::default().fg(self.theme.fg)
                    }
                };
                let mut lines = vec![
                    Line::from(tr!(
                        self.lang,
                        "选择要同步的 Todoist 项目。个人 Token 不会写入 Horae。",
                        "Choose a Todoist project. Your personal token is never stored by Horae."
                    )),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(
                            tr!(self.lang, "项目 ID：", "Project ID: "),
                            Style::default().fg(self.theme.text_dim),
                        ),
                        Span::styled(project_id.clone(), field_style(*focus == 0)),
                    ]),
                    Line::from(vec![
                        Span::styled(
                            tr!(
                                self.lang,
                                "Token 环境变量：",
                                "Token environment variable: "
                            ),
                            Style::default().fg(self.theme.text_dim),
                        ),
                        Span::styled(token_env.clone(), field_style(*focus == 1)),
                    ]),
                    Line::from(""),
                    Line::from(tr!(
                        self.lang,
                        "先在 shell 中设置 Token 环境变量，再按 y 执行同步。",
                        "Set the token in your shell, then press y in Settings to sync."
                    )),
                ];
                if let Some(error) = error {
                    lines.push(Line::from(Span::styled(
                        format!("✗ {error}"),
                        Style::default().fg(self.theme.text_urgent),
                    )));
                }
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    tr!(
                        self.lang,
                        "[Tab] 切换字段  [Enter] 下一项/保存  [Esc] 取消",
                        "[Tab] Switch field  [Enter] Next/Save  [Esc] Cancel"
                    ),
                    Style::default().fg(self.theme.text_dim),
                )));
                f.render_widget(Paragraph::new(lines).block(block), area);
            }
            crate::tui::app::Popup::TodoistConflicts { selected } => {
                let height = size.height.saturating_sub(6).max(12);
                let area = self.centered_rect(76, height, size);
                f.render_widget(ratatui::widgets::Clear, area);
                let conflicts = &self.todoist_panel.conflicts;
                let title = tr!(
                    self.lang,
                    " 待处理的同步冲突（{}） ",
                    " Unresolved Sync Conflicts ({}) ",
                    conflicts.len()
                );
                let block = Block::default()
                    .title(title)
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .border_style(Style::default().fg(self.theme.text_urgent));
                let mut lines = Vec::new();
                if conflicts.is_empty() {
                    lines.push(Line::from(tr!(
                        self.lang,
                        "当前没有待处理冲突。",
                        "No unresolved conflicts."
                    )));
                } else {
                    let start = selected.saturating_sub(2);
                    let end = (start + 5).min(conflicts.len());
                    for (idx, conflict) in conflicts.iter().enumerate().take(end).skip(start) {
                        let active = idx == *selected;
                        let style = if active {
                            Style::default()
                                .fg(self.theme.bg)
                                .bg(self.theme.accent)
                                .add_modifier(Modifier::BOLD)
                        } else {
                            Style::default().fg(self.theme.fg)
                        };
                        lines.push(Line::from(Span::styled(
                            format!(
                                "{} {}",
                                if active { "›" } else { " " },
                                conflict.local_title
                            ),
                            style,
                        )));
                        lines.push(Line::from(vec![
                            Span::styled(
                                tr!(self.lang, "  Horae: ", "  Horae: "),
                                Style::default().fg(self.theme.text_dim),
                            ),
                            Span::styled(
                                conflict.local_title.clone(),
                                Style::default().fg(self.theme.fg),
                            ),
                        ]));
                        lines.push(Line::from(vec![
                            Span::styled(
                                tr!(self.lang, "  Todoist: ", "  Todoist: "),
                                Style::default().fg(self.theme.text_dim),
                            ),
                            Span::styled(
                                if conflict.remote_deleted {
                                    tr!(self.lang, "远端任务已删除", "Task deleted in Todoist")
                                        .to_string()
                                } else {
                                    conflict.remote_title.clone()
                                },
                                Style::default().fg(self.theme.text_success),
                            ),
                        ]));
                        lines.push(Line::from(""));
                    }
                    lines.push(Line::from(Span::styled(
                        tr!(
                            self.lang,
                            "[h] 保留 Horae 并推送  [t] 采用 Todoist  [j/k] 选择  [Esc] 关闭",
                            "[h] Keep Horae and push  [t] Keep Todoist  [j/k] Select  [Esc] Close"
                        ),
                        Style::default().fg(self.theme.text_dim),
                    )));
                }
                f.render_widget(
                    Paragraph::new(lines)
                        .block(block)
                        .wrap(ratatui::widgets::Wrap { trim: true }),
                    area,
                );
            }
            crate::tui::app::Popup::PasswordSet {
                is_confirm,
                input,
                error,
                ..
            } => {
                let area = self.centered_rect(54, 11, size);
                f.render_widget(ratatui::widgets::Clear, area);
                let block = Block::default()
                    .title(tr!(
                        self.lang,
                        " 🔐 设置密码保护 ",
                        " 🔐 Set Password Protection "
                    ))
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .border_style(Style::default().fg(self.theme.accent));

                let prompt = if *is_confirm {
                    tr!(
                        self.lang,
                        "请再次输入以确认新密码：",
                        "Please confirm new password:"
                    )
                } else {
                    tr!(self.lang, "请输入新密码：", "Please enter new password:")
                };

                let masked = "● ".repeat(input.chars().count());
                let is_empty = masked.is_empty();
                let mut lines = vec![
                    Line::from(""),
                    Line::from(Span::styled(prompt, Style::default().fg(self.theme.fg))),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(" [ ", Style::default().fg(self.theme.accent)),
                        Span::styled(
                            if is_empty {
                                tr!(self.lang, "输入密码...", "enter password...").to_string()
                            } else {
                                masked
                            },
                            if is_empty {
                                Style::default().fg(self.theme.text_dim)
                            } else {
                                Style::default()
                                    .fg(self.theme.fg)
                                    .add_modifier(Modifier::BOLD)
                            },
                        ),
                        Span::styled(" ]", Style::default().fg(self.theme.accent)),
                    ]),
                    Line::from(""),
                ];

                if let Some(err) = error {
                    lines.push(Line::from(Span::styled(
                        format!(" ✗ {}", err),
                        Style::default().fg(self.theme.text_urgent),
                    )));
                } else {
                    lines.push(Line::from(""));
                }

                lines.push(Line::from(Span::styled(
                    tr!(
                        self.lang,
                        " [Enter] 下一步/确认  •  [Esc] 取消 ",
                        " [Enter] Next/Confirm  •  [Esc] Cancel "
                    ),
                    Style::default().fg(self.theme.text_dim),
                )));

                f.render_widget(
                    Paragraph::new(lines)
                        .block(block)
                        .alignment(Alignment::Center),
                    area,
                );
            }
            crate::tui::app::Popup::PasswordDisable { input, error } => {
                let area = self.centered_rect(54, 11, size);
                f.render_widget(ratatui::widgets::Clear, area);
                let block = Block::default()
                    .title(tr!(
                        self.lang,
                        " 🔓 关闭密码保护 ",
                        " 🔓 Disable Password Protection "
                    ))
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .border_style(Style::default().fg(self.theme.accent));

                let prompt = tr!(
                    self.lang,
                    "请输入当前密码以关闭保护：",
                    "Enter current password to disable:"
                );

                let masked = "● ".repeat(input.chars().count());
                let is_empty = masked.is_empty();
                let mut lines = vec![
                    Line::from(""),
                    Line::from(Span::styled(prompt, Style::default().fg(self.theme.fg))),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(" [ ", Style::default().fg(self.theme.accent)),
                        Span::styled(
                            if is_empty {
                                tr!(self.lang, "输入密码...", "enter password...").to_string()
                            } else {
                                masked
                            },
                            if is_empty {
                                Style::default().fg(self.theme.text_dim)
                            } else {
                                Style::default()
                                    .fg(self.theme.fg)
                                    .add_modifier(Modifier::BOLD)
                            },
                        ),
                        Span::styled(" ]", Style::default().fg(self.theme.accent)),
                    ]),
                    Line::from(""),
                ];

                if let Some(err) = error {
                    lines.push(Line::from(Span::styled(
                        format!(" ✗ {}", err),
                        Style::default().fg(self.theme.text_urgent),
                    )));
                } else {
                    lines.push(Line::from(""));
                }

                lines.push(Line::from(Span::styled(
                    tr!(
                        self.lang,
                        " [Enter] 确认关闭  •  [Tab] 修改密码  •  [Esc] 取消 ",
                        " [Enter] Confirm Disable  •  [Tab] Change Password  •  [Esc] Cancel "
                    ),
                    Style::default().fg(self.theme.text_dim),
                )));

                f.render_widget(
                    Paragraph::new(lines)
                        .block(block)
                        .alignment(Alignment::Center),
                    area,
                );
            }
            crate::tui::app::Popup::PasswordChange {
                step, input, error, ..
            } => {
                let area = self.centered_rect(54, 11, size);
                f.render_widget(ratatui::widgets::Clear, area);
                let block = Block::default()
                    .title(tr!(self.lang, " 🔑 修改保护密码 ", " 🔑 Change Password "))
                    .borders(Borders::ALL)
                    .border_set(border::ROUNDED)
                    .border_style(Style::default().fg(self.theme.accent));

                let prompt = match *step {
                    0 => tr!(self.lang, "请输入当前原密码：", "Enter current password:"),
                    1 => tr!(self.lang, "请输入新密码：", "Enter new password:"),
                    _ => tr!(
                        self.lang,
                        "请再次输入以确认新密码：",
                        "Confirm new password:"
                    ),
                };

                let masked = "● ".repeat(input.chars().count());
                let is_empty = masked.is_empty();
                let mut lines = vec![
                    Line::from(""),
                    Line::from(Span::styled(prompt, Style::default().fg(self.theme.fg))),
                    Line::from(""),
                    Line::from(vec![
                        Span::styled(" [ ", Style::default().fg(self.theme.accent)),
                        Span::styled(
                            if is_empty {
                                tr!(self.lang, "输入密码...", "enter password...").to_string()
                            } else {
                                masked
                            },
                            if is_empty {
                                Style::default().fg(self.theme.text_dim)
                            } else {
                                Style::default()
                                    .fg(self.theme.fg)
                                    .add_modifier(Modifier::BOLD)
                            },
                        ),
                        Span::styled(" ]", Style::default().fg(self.theme.accent)),
                    ]),
                    Line::from(""),
                ];

                if let Some(err) = error {
                    lines.push(Line::from(Span::styled(
                        format!(" ✗ {}", err),
                        Style::default().fg(self.theme.text_urgent),
                    )));
                } else {
                    lines.push(Line::from(""));
                }

                lines.push(Line::from(Span::styled(
                    tr!(
                        self.lang,
                        " [Enter] 下一步/确认  •  [Esc] 取消 ",
                        " [Enter] Next/Confirm  •  [Esc] Cancel "
                    ),
                    Style::default().fg(self.theme.text_dim),
                )));

                f.render_widget(
                    Paragraph::new(lines)
                        .block(block)
                        .alignment(Alignment::Center),
                    area,
                );
            }
        }
    }
}
