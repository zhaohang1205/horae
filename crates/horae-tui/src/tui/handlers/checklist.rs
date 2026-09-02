use crate::tui::app::{App, Mode};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use horae_core::repo::tasks;

impl<'a> App<'a> {
    /// 从普通模式按 `Tab` 进入检查单逐项管理（仅当当前任务含检查单）。
    pub(super) fn handle_checklist_enter(&mut self, key: KeyEvent) -> Result<bool> {
        if key.code == KeyCode::Tab && !key.modifiers.contains(KeyModifiers::CONTROL) {
            if let Some(row) = self.items.get(self.selected).cloned() {
                if row.done.is_some() {
                    if let Ok(task) = tasks::get(self.conn, &row.id) {
                        if !task.checklist.is_empty() {
                            let cursor = task
                                .checklist
                                .iter()
                                .position(|i| !i.done)
                                .unwrap_or(0)
                                .min(task.checklist.len() - 1);
                            self.checklist_cursor = Some(cursor);
                            self.set_mode(Mode::ChecklistFocus);
                            self.status_message = tr!(
                                self.lang,
                                "检查单管理：j/k 移动 · Space 勾选 · d 删除 · J/K 排序 · e 改名 · Tab 退出",
                                "managing: j/k move · Space tick · d delete · J/K reorder · e rename · Tab exit"
                            )
                            .to_string();
                            self.load_detail();
                            return Ok(true);
                        }
                    }
                }
            }
        }
        Ok(false)
    }

    /// 检查单管理模式下处理光标移动与各项操作。
    pub(super) fn handle_checklist_keys(&mut self, key: KeyEvent) -> Result<()> {
        match key.code {
            KeyCode::Tab | KeyCode::Esc => {
                self.set_mode(Mode::Normal);
                self.checklist_cursor = None;
                self.load_detail();
            }
            KeyCode::Char('j') | KeyCode::Down => self.checklist_cursor_move(1),
            KeyCode::Char('k') | KeyCode::Up => self.checklist_cursor_move(-1),
            KeyCode::Char(' ') | KeyCode::Enter => self.checklist_toggle(),
            KeyCode::Char('d') | KeyCode::Delete => self.checklist_delete(),
            KeyCode::Char('J') => self.checklist_move(1),
            KeyCode::Char('K') => self.checklist_move(-1),
            KeyCode::Char('e') => self.checklist_rename_start(),
            _ => {}
        }
        Ok(())
    }

    fn checklist_cursor_move(&mut self, delta: isize) {
        let idx = match (
            self.items.get(self.selected).cloned(),
            self.checklist_cursor,
        ) {
            (Some(row), Some(cur)) => {
                if let Ok(task) = tasks::get(self.conn, &row.id) {
                    let len = task.checklist.len();
                    if len == 0 {
                        return;
                    }
                    Some((cur as isize + delta).clamp(0, (len - 1) as isize) as usize)
                } else {
                    None
                }
            }
            _ => None,
        };
        if let Some(i) = idx {
            self.checklist_cursor = Some(i);
        }
    }

    fn checklist_toggle(&mut self) {
        if let (Some(row), Some(cur)) = (
            self.items.get(self.selected).cloned(),
            self.checklist_cursor,
        ) {
            if let Ok(task) = tasks::get(self.conn, &row.id) {
                if let Some(item) = task.checklist.get(cur) {
                    let item_id = item.id.clone();
                    let title = item.title.clone();
                    if tasks::toggle_checklist_item(self.conn, &row.id, &item_id).is_ok() {
                        self.push_undo(crate::tui::app::UndoAction::ChecklistToggled {
                            task_id: row.id.clone(),
                            item_id,
                            item_title: title.clone(),
                        });
                        self.set_toast(
                            tr!(
                                self.lang,
                                "✓ 打卡: {} (按 u 撤销)",
                                "✓ Checked: {} (press u to undo)",
                                title
                            ),
                            true,
                        );
                        self.load_detail();
                    }
                }
            }
        }
    }

    fn checklist_delete(&mut self) {
        if let (Some(row), Some(cur)) = (
            self.items.get(self.selected).cloned(),
            self.checklist_cursor,
        ) {
            if let Ok(task) = tasks::get(self.conn, &row.id) {
                if let Some(item) = task.checklist.get(cur) {
                    let item_id = item.id.clone();
                    if tasks::delete_checklist_item(self.conn, &row.id, &item_id).is_ok() {
                        let len = task.checklist.len().saturating_sub(1);
                        if len == 0 {
                            self.checklist_cursor = None;
                            self.set_mode(Mode::Normal);
                        } else {
                            self.checklist_cursor = Some(cur.min(len - 1));
                        }
                        self.status_message =
                            tr!(self.lang, "已删除检查项", "Checklist item deleted").to_string();
                        self.load_detail();
                    }
                }
            }
        }
    }

    fn checklist_move(&mut self, dir: isize) {
        if let (Some(row), Some(cur)) = (
            self.items.get(self.selected).cloned(),
            self.checklist_cursor,
        ) {
            if let Ok(task) = tasks::get(self.conn, &row.id) {
                if let Some(item) = task.checklist.get(cur) {
                    let item_id = item.id.clone();
                    let max = task.checklist.len().saturating_sub(1) as isize;
                    if tasks::move_checklist_item(self.conn, &row.id, &item_id, dir)
                        .unwrap_or(false)
                    {
                        self.checklist_cursor = Some((cur as isize + dir).clamp(0, max) as usize);
                        self.load_detail();
                    }
                }
            }
        }
    }

    fn checklist_rename_start(&mut self) {
        if let (Some(row), Some(cur)) = (
            self.items.get(self.selected).cloned(),
            self.checklist_cursor,
        ) {
            if let Ok(task) = tasks::get(self.conn, &row.id) {
                if let Some(item) = task.checklist.get(cur) {
                    self.input.clear();
                    self.input.push_str(&item.title);
                    self.input_cursor = self.input.len();
                    self.set_mode(Mode::RenamingChecklist);
                }
            }
        }
    }

    pub(super) fn confirm_checklist_rename(&mut self, input: &str) -> Result<()> {
        if let (Some(row), Some(cur)) = (
            self.items.get(self.selected).cloned(),
            self.checklist_cursor,
        ) {
            if let Ok(task) = tasks::get(self.conn, &row.id) {
                if let Some(item) = task.checklist.get(cur) {
                    let item_id = item.id.clone();
                    if !input.trim().is_empty() {
                        tasks::rename_checklist_item(self.conn, &row.id, &item_id, input.trim())?;
                        self.status_message = tr!(self.lang, "已改名", "Renamed").to_string();
                    }
                }
            }
        }
        self.set_mode(Mode::ChecklistFocus);
        self.load_detail();
        Ok(())
    }
}
