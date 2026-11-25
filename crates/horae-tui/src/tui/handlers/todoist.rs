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
            } => {
                match key.code {
                    KeyCode::Esc => return Ok(()),
                    KeyCode::Tab | KeyCode::BackTab => focus = 1 - focus,
                    KeyCode::Enter if focus == 0 => focus = 1,
                    KeyCode::Enter => match self.save_todoist_config(&project_id, &token_env) {
                        Ok(()) => {
                            self.flash(
                                ToastKind::Success,
                                tr!(self.lang, "Todoist 配置已保存。", "Todoist settings saved."),
                            );
                            return Ok(());
                        }
                        Err(problem) => error = Some(problem.to_string()),
                    },
                    KeyCode::Backspace => {
                        if focus == 0 {
                            project_id.pop();
                        } else {
                            token_env.pop();
                        }
                        error = None;
                    }
                    KeyCode::Char(c) if !c.is_control() => {
                        if focus == 0 {
                            project_id.push(c);
                        } else {
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
                            Ok(_) => self.flash(
                                ToastKind::Success,
                                tr!(
                                    self.lang,
                                    "已保留 Horae 版本并尝试同步。",
                                    "Kept Horae version and attempted sync."
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
                            Ok(_) => self.flash(
                                ToastKind::Success,
                                tr!(self.lang, "已采用 Todoist 版本。", "Kept Todoist version."),
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
