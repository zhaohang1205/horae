use super::super::app::{App, Popup, ToastKind};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use horae_core::todoist::ConflictResolution;

impl<'a> App<'a> {
    pub(super) fn handle_todoist_popup(&mut self, key: KeyEvent, popup: Popup) -> Result<()> {
        match popup {
            Popup::TodoistConfig {
                mut project_id,
                mut token_env,
                mut focus,
                mut error,
                mut delete_remote_on_archive,
                mut prefer_remote_on_conflict,
            } => {
                match key.code {
                    KeyCode::Esc => return Ok(()),
                    KeyCode::Tab | KeyCode::BackTab => focus = (focus + 1) % 4,
                    KeyCode::Enter if focus == 0 => focus = 1,
                    KeyCode::Enter if focus == 1 => focus = 2,
                    KeyCode::Enter if focus == 2 => focus = 3,
                    KeyCode::Enter => {
                        match self.save_todoist_config(
                            &project_id,
                            &token_env,
                            delete_remote_on_archive,
                            prefer_remote_on_conflict,
                        ) {
                            Ok(()) => {
                                self.flash(
                                    ToastKind::Success,
                                    tr!(
                                        self.lang,
                                        "Todoist 配置已保存。",
                                        "Todoist settings saved."
                                    ),
                                );
                                return Ok(());
                            }
                            Err(problem) => error = Some(problem.to_string()),
                        }
                    }
                    KeyCode::Backspace => {
                        if focus == 0 {
                            project_id.pop();
                        } else if focus == 1 {
                            token_env.pop();
                        }
                        error = None;
                    }
                    KeyCode::Char(' ') => {
                        if focus == 2 {
                            delete_remote_on_archive = !delete_remote_on_archive;
                        } else if focus == 3 {
                            prefer_remote_on_conflict = !prefer_remote_on_conflict;
                        }
                        error = None;
                    }
                    KeyCode::Char(c) if !c.is_control() => {
                        if focus == 0 {
                            project_id.push(c);
                        } else if focus == 1 {
                            token_env.push(c);
                        }
                        error = None;
                    }
                    _ => {}
                }
                self.popup = Some(Popup::TodoistConfig {
                    project_id,
                    token_env,
                    focus,
                    error,
                    delete_remote_on_archive,
                    prefer_remote_on_conflict,
                });
            }
            Popup::TodoistConflicts { mut selected } => {
                let len = self.todoist_panel.conflicts.len();
                if len == 0 || key.code == KeyCode::Esc {
                    return Ok(());
                }
                match key.code {
                    KeyCode::Char('j') | KeyCode::Down => selected = (selected + 1).min(len - 1),
                    KeyCode::Char('k') | KeyCode::Up => selected = selected.saturating_sub(1),
                    KeyCode::Char('h') | KeyCode::Char('H') => {
                        let id = self.todoist_panel.conflicts[selected].todoist_id.clone();
                        match self.resolve_todoist_conflict(&id, ConflictResolution::KeepHorae) {
                            Ok(Some(_)) => self.flash(
                                ToastKind::Success,
                                tr!(
                                    self.lang,
                                    "已保留 Horae 版本并完成同步。",
                                    "Kept Horae version and synchronized."
                                ),
                            ),
                            Ok(None) => self.flash(
                                ToastKind::Info,
                                tr!(
                                    self.lang,
                                    "已记录选择。请先处理剩余冲突，再同步。",
                                    "Choice saved. Resolve the remaining conflicts, then sync."
                                ),
                            ),
                            Err(error) => self.flash(
                                ToastKind::Error,
                                tr!(
                                    self.lang,
                                    "解决冲突失败：{}",
                                    "Conflict resolution failed: {}",
                                    error
                                ),
                            ),
                        }
                        if self.todoist_panel.conflicts.is_empty() {
                            return Ok(());
                        }
                        selected = selected.min(self.todoist_panel.conflicts.len() - 1);
                    }
                    KeyCode::Char('t') | KeyCode::Char('T') => {
                        let id = self.todoist_panel.conflicts[selected].todoist_id.clone();
                        match self.resolve_todoist_conflict(&id, ConflictResolution::KeepTodoist) {
                            Ok(Some(_)) => self.flash(
                                ToastKind::Success,
                                tr!(
                                    self.lang,
                                    "已采用 Todoist 版本并完成同步。",
                                    "Kept Todoist version and synchronized."
                                ),
                            ),
                            Ok(None) => self.flash(
                                ToastKind::Info,
                                tr!(
                                    self.lang,
                                    "已记录选择。请先处理剩余冲突，再同步。",
                                    "Choice saved. Resolve the remaining conflicts, then sync."
                                ),
                            ),
                            Err(error) => self.flash(
                                ToastKind::Error,
                                tr!(
                                    self.lang,
                                    "解决冲突失败：{}",
                                    "Conflict resolution failed: {}",
                                    error
                                ),
                            ),
                        }
                        if self.todoist_panel.conflicts.is_empty() {
                            return Ok(());
                        }
                        selected = selected.min(self.todoist_panel.conflicts.len() - 1);
                    }
                    _ => {}
                }
                self.popup = Some(Popup::TodoistConflicts { selected });
            }
            _ => unreachable!("only Todoist popups are routed here"),
        }
        Ok(())
    }
}
