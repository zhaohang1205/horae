use crate::model::task;
use crate::repo::tasks;
use crate::tui::app::{App, Mode};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};

impl<'a> App<'a> {
    pub(super) fn handle_confirm_archive(&mut self, key: KeyEvent) -> Result<()> {
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

    pub(super) fn handle_confirm_purge(&mut self, key: KeyEvent) -> Result<()> {
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

    pub(super) fn handle_confirm_profile_delete(&mut self, key: KeyEvent) -> Result<()> {
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
}
