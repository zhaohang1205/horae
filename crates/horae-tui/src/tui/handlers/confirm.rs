use crate::tui::app::{App, Mode, ToastKind};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use horae_core::model::task;
use horae_core::repo::tasks;

impl<'a> App<'a> {
    pub(super) fn handle_confirm_archive(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Enter | KeyCode::Char('y') | KeyCode::Char('Y') => {
                let ids = std::mem::take(&mut self.pending_archive_ids);
                let had_selection = !self.selected_ids.is_empty();
                let mut count = 0;
                let mut archived_records = Vec::new();
                let mut first_err: Option<anyhow::Error> = None;
                for id in &ids {
                    match tasks::get(self.conn, id) {
                        Ok(task) => {
                            let prev_status = task.status;
                            if matches!(
                                task.status,
                                task::Status::Done | task::Status::Waiting | task::Status::Someday
                            ) {
                                if let Ok(pomo) = horae_core::repo::pomodoro::get_state() {
                                    if pomo.task_id.as_deref() == Some(id) {
                                        let _ = horae_core::pomo::stop(Some(self.conn), false);
                                    }
                                }
                            }
                            match tasks::archive(self.conn, id) {
                                Ok(_) => {
                                    count += 1;
                                    archived_records.push((
                                        id.clone(),
                                        prev_status,
                                        task.title.clone(),
                                    ));
                                }
                                Err(e) => {
                                    if first_err.is_none() {
                                        first_err = Some(e);
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            if first_err.is_none() {
                                first_err = Some(e);
                            }
                        }
                    }
                }
                if archived_records.len() == 1 {
                    let (id, status, title) = archived_records.remove(0);
                    self.push_undo(crate::tui::app::UndoAction::Archive {
                        task_id: id,
                        from_status: status,
                        title,
                    });
                } else if !archived_records.is_empty() {
                    let records = archived_records
                        .into_iter()
                        .map(|(id, st, _)| (id, st))
                        .collect();
                    self.push_undo(crate::tui::app::UndoAction::BulkArchive { records });
                }
                self.set_mode(Mode::Normal);
                if had_selection {
                    self.selected_ids.clear();
                    self.visual_start_idx = None;
                }
                if count > 0 {
                    self.set_toast(
                        tr!(
                            self.lang,
                            "📦 已归档 {} 项 (按 u 撤销)",
                            "📦 archived {} items (press u to undo)",
                            count
                        ),
                        true,
                    );
                } else if let Some(e) = first_err {
                    self.flash(
                        ToastKind::Error,
                        tr!(self.lang, "归档失败: {}", "Archive failed: {}", e),
                    );
                }
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
                self.status_message = tr!(self.lang, "归档已取消", "Archive cancelled").into();
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
                let mut first_err: Option<anyhow::Error> = None;
                for id in &ids {
                    // 若当前番茄钟正聚焦于该任务，先停止它再删除。
                    if let Ok(pomo) = horae_core::repo::pomodoro::get_state() {
                        if pomo.task_id.as_deref() == Some(id.as_str()) {
                            let _ = horae_core::pomo::stop(Some(self.conn), false);
                        }
                    }
                    match tasks::purge(self.conn, id) {
                        Ok(_) => count += 1,
                        Err(e) => {
                            if first_err.is_none() {
                                first_err = Some(e);
                            }
                        }
                    }
                }
                self.set_mode(Mode::Normal);
                if had_selection {
                    self.selected_ids.clear();
                    self.visual_start_idx = None;
                }
                if count > 0 {
                    self.set_toast(
                        tr!(
                            self.lang,
                            "已永久删除 {} 项",
                            "permanently deleted {} items",
                            count
                        ),
                        true,
                    );
                } else if let Some(e) = first_err {
                    self.flash(
                        ToastKind::Error,
                        tr!(self.lang, "永久删除失败: {}", "Purge failed: {}", e),
                    );
                }
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
                self.status_message = tr!(self.lang, "删除已取消", "Purge cancelled").into();
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
                self.status_message = tr!(self.lang, "删除已取消", "Delete cancelled").into();
            }
            _ => {}
        }
        Ok(())
    }
}
