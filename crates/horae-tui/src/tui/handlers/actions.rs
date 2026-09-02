use super::short_id;
use super::AppHandlers;
use crate::tui::app::{App, Mode, View};
use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent};
use horae_core::model::task;
use horae_core::repo::{tags, tasks};

impl<'a> App<'a> {
    /// 设置页按键：n 新建 / r 重命名 / d 删除 / s 设为默认（仅 Settings 视图生效）。
    pub(super) fn handle_settings_keys(&mut self, key: KeyEvent) -> Result<bool> {
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
    pub(super) fn handle_task_action_keys(&mut self, key: KeyEvent) -> Result<bool> {
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

                        let result = (|| -> Result<()> {
                            use std::io::Write;
                            let mut file = std::fs::File::create(&temp_path)?;
                            file.write_all(task.notes.as_bytes())?;
                            drop(file);

                            let status = std::process::Command::new("sh")
                                .arg("-c")
                                .arg(format!("{} \"{}\"", editor, temp_path.display()))
                                .status();
                            if let Err(e) = status {
                                anyhow::bail!("failed to launch editor: {}", e);
                            }
                            Ok(())
                        })();

                        // Always restore terminal state, even if editor failed
                        crossterm::terminal::enable_raw_mode()?;
                        crossterm::execute!(
                            std::io::stdout(),
                            crossterm::terminal::EnterAlternateScreen
                        )?;
                        self.needs_clear = true;

                        // 无论编辑器是否成功都读取并清理临时文件，避免泄漏。
                        let new_notes = std::fs::read_to_string(&temp_path).unwrap_or_default();
                        let _ = std::fs::remove_file(&temp_path);
                        if let Err(e) = result {
                            self.status_message = format!("editor error: {}", e);
                        } else if new_notes != task.notes {
                            self.note(tasks::update_notes(self.conn, &task.id, &new_notes));
                        }
                        self.load_detail();
                    }
                }
            }
            KeyCode::Char('x') => {
                if self.pomo.phase != horae_core::model::pomodoro::Phase::Idle {
                    if let Some(ref tid) = self.pomo.task_id.clone() {
                        self.complete_pomodoro_task(tid)?;
                        return Ok(true);
                    } else {
                        let _ = horae_core::pomo::stop();
                        self.force_reload_pomo();
                        self.set_toast(
                            tr!(self.lang, "🎉 专注达成！", "🎉 Focus completed!"),
                            true,
                        );
                        self.refresh()?;
                        return Ok(true);
                    }
                }
                self.act_on_selected(task::Status::Done)?;
                self.move_sel(1);
            }
            KeyCode::Char('s') => self.act_on_selected(task::Status::Someday)?,
            KeyCode::Char('"') if self.quotes.enabled => {
                // 金句移入/移出：加/摘 @quote 标签，工作态流转为 reference。
                self.toggle_quotes()?;
            }
            KeyCode::Char('u') => {
                if self.view == View::Archived {
                    let r = self.restore_selected();
                    self.note(r);
                } else {
                    let _ = self.undo()?;
                }
            }
            KeyCode::Char('r')
                if key
                    .modifiers
                    .contains(crossterm::event::KeyModifiers::CONTROL) =>
            {
                let _ = self.redo()?;
            }
            KeyCode::Enter => self.open_organize()?,
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 批量选择相关按键：Space 切换当前行（Ctrl+a 全选 / Ctrl+i 反选在系统键处理）。
    /// 在番茄钟专注态下，Space 优先勾选当前专注任务的下一项检查单。
    pub(super) fn handle_selection_keys(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char(' ') => {
                if self.pomo.phase == horae_core::model::pomodoro::Phase::Work {
                    if let Some(ref tid) = self.pomo.task_id {
                        if let Ok(task) = horae_core::repo::tasks::get(self.conn, tid) {
                            if let Some(item) = task.checklist.iter().find(|i| !i.done) {
                                let item_id = item.id.clone();
                                let item_title = item.title.clone();
                                if let Ok(Some(_)) = horae_core::repo::tasks::toggle_checklist_item(
                                    self.conn, tid, &item_id,
                                ) {
                                    self.push_undo(crate::tui::app::UndoAction::ChecklistToggled {
                                        task_id: tid.clone(),
                                        item_id,
                                        item_title: item_title.clone(),
                                    });
                                    self.set_toast(
                                        tr!(
                                            self.lang,
                                            "✓ 打卡子项: {} (按 u 撤销)",
                                            "✓ Checked step: {} (press u to undo)",
                                            item_title
                                        ),
                                        true,
                                    );
                                    self.load_detail();
                                    return Ok(true);
                                }
                            }
                        }
                    }
                } else if matches!(
                    self.pomo.phase,
                    horae_core::model::pomodoro::Phase::ShortBreak
                        | horae_core::model::pomodoro::Phase::LongBreak
                ) {
                    let target_id = self
                        .pomo
                        .task_id
                        .clone()
                        .or_else(|| self.items.get(self.selected).map(|r| r.id.clone()));
                    if let Some(ref tid) = target_id {
                        self.note(horae_core::repo::tasks::ensure_ready_for_pomodoro(
                            self.conn, tid,
                        ));
                        if self.note(horae_core::pomo::start(self.conn, tid)) {
                            self.force_reload_pomo();
                            self.set_toast(
                                tr!(
                                    self.lang,
                                    "🚀 零摩擦开启新一轮专注！",
                                    "🚀 Started new focus round!"
                                ),
                                true,
                            );
                        }
                        self.load_detail();
                        return Ok(true);
                    }
                }
                self.toggle_selected();
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 番茄钟相关按键：P 启动/续杯，= 勾选检查单，S 停止。
    pub(super) fn handle_pomodoro_keys(&mut self, key: KeyEvent) -> Result<bool> {
        match key.code {
            KeyCode::Char('=') => {
                let target_id = if self.pomo.phase == horae_core::model::pomodoro::Phase::Work {
                    self.pomo
                        .task_id
                        .clone()
                        .or_else(|| self.items.get(self.selected).map(|r| r.id.clone()))
                } else {
                    self.items.get(self.selected).map(|r| r.id.clone())
                };
                if let Some(tid) = target_id {
                    if let Ok(task) = horae_core::repo::tasks::get(self.conn, &tid) {
                        if let Some(item) = task.checklist.iter().find(|i| !i.done) {
                            let item_id = item.id.clone();
                            let item_title = item.title.clone();
                            if let Ok(Some(_)) = horae_core::repo::tasks::toggle_checklist_item(
                                self.conn, &tid, &item_id,
                            ) {
                                self.push_undo(crate::tui::app::UndoAction::ChecklistToggled {
                                    task_id: tid.clone(),
                                    item_id,
                                    item_title: item_title.clone(),
                                });
                                self.set_toast(
                                    tr!(
                                        self.lang,
                                        "✓ 打卡子项: {} (按 u 撤销)",
                                        "✓ Checked step: {} (press u to undo)",
                                        item_title
                                    ),
                                    true,
                                );
                            }
                        } else {
                            self.set_toast(
                                tr!(
                                    self.lang,
                                    "检查单已全部完成，可按 x 标记任务完成",
                                    "All steps done — press x to complete the task"
                                ),
                                true,
                            );
                        }
                    }
                    self.load_detail();
                }
            }
            KeyCode::Char('P') => {
                // 休息后或跨天 idle 的续杯：优先当前选中任务。
                if let Ok(pomo) = horae_core::repo::pomodoro::get_state() {
                    let is_in_break = matches!(
                        pomo.phase,
                        horae_core::model::pomodoro::Phase::ShortBreak
                            | horae_core::model::pomodoro::Phase::LongBreak
                    );
                    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
                    let today_active = pomo.last_date.as_deref() == Some(today.as_str());
                    let is_post_break_idle = pomo.phase == horae_core::model::pomodoro::Phase::Idle
                        && today_active
                        && (pomo.today_count > 0 || pomo.task_id.is_some());
                    if is_in_break || is_post_break_idle {
                        let target_id = self
                            .items
                            .get(self.selected)
                            .map(|r| r.id.clone())
                            .or(pomo.task_id);
                        if let Some(tid) = target_id {
                            self.note(horae_core::repo::tasks::ensure_ready_for_pomodoro(
                                self.conn, &tid,
                            ));
                            {
                                let r = self.refresh();
                                self.note(r);
                            }
                            if self.note(horae_core::pomo::start(self.conn, &tid)) {
                                self.force_reload_pomo();
                                self.status_message = tr!(
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
                        horae_core::repo::pomodoro::get_state()
                            .ok()
                            .and_then(|s| s.task_id)
                    });
                if let Some(tid) = target_id {
                    self.note(horae_core::repo::tasks::ensure_ready_for_pomodoro(
                        self.conn, &tid,
                    ));
                    {
                        let r = self.refresh();
                        self.note(r);
                    }
                    if self.note(horae_core::pomo::start(self.conn, &tid)) {
                        self.force_reload_pomo();
                        self.status_message = tr!(
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
                if self.note(horae_core::pomo::stop()) {
                    self.force_reload_pomo();
                    self.status_message.clear();
                    self.refresh()?;
                }
            }
            _ => return Ok(false),
        }
        Ok(true)
    }

    /// 归档 / 永久删除（归档箱、标签库）按键。
    pub(super) fn handle_archive_keys(&mut self, key: KeyEvent) -> Result<bool> {
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
            self.status_message = tr!(
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
                            tr!(self.lang, "已删除标签 @{}", "Tag @{} deleted", tag_name);
                        self.refresh()?;
                    }
                    Err(e) => {
                        self.status_message =
                            tr!(self.lang, "删除失败: {}", "Delete failed: {}", e);
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
        self.status_message = tr!(
            self.lang,
            "确认归档 {} 项? (y/Enter 确认, n/Esc 取消)",
            "Archive {} items? (y/Enter confirm, n/Esc cancel)",
            self.pending_archive_ids.len()
        );
        Ok(true)
    }
}
