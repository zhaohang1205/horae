use anyhow::Result;
use horae_core::model::task::{self, Task};
use horae_core::repo::tasks;

use super::{App, Mode, View};

impl<'a> App<'a> {
    /// 回车进入组织/编辑模式：与 capture 同一个一句话编辑器，预填当前任务内容。
    pub(crate) fn open_organize(&mut self) -> Result<()> {
        if self.mode == Mode::Visual {
            self.set_mode(Mode::Normal);
            self.selected_ids.clear();
            self.visual_start_idx = None;
            self.status_message = tr!(
                self.lang,
                "可视模式不支持编辑",
                "editing unavailable in visual mode"
            )
            .into();
            return Ok(());
        }
        if matches!(self.view, View::Tags | View::Archived | View::Settings) {
            return Ok(());
        }
        let Some(row) = self.items.get(self.selected).cloned() else {
            return Ok(());
        };
        let Ok(task) = tasks::get(self.conn, &row.id) else {
            return Ok(());
        };
        self.organizing_id = Some(task.id.clone());
        self.input = self.task_to_quick_add(&task);
        self.set_mode(Mode::Capturing);
        self.status_message = tr!(
            self.lang,
            "组织: 编辑 @标签 ~时间 *周期 (空/Esc 跳过)",
            "organize: edit @tags ~time *rrule (empty/Esc to skip)"
        )
        .into();
        Ok(())
    }

    /// 把任务序列化成 quick-add 一句话（标题 @标签 ~时间 *周期 !优先级），
    /// 严禁展开语法（周期使用简写如 *d / *2w[1,3]，时间使用自然日期与时刻），
    /// 消除用户的认知负担和心理障碍。
    pub(crate) fn task_to_quick_add(&self, task: &Task) -> String {
        let row = crate::tui::row_from(task, 0, self.conn)
            .unwrap_or_else(|_| crate::tui::row_from_tags(task, 0, Vec::new()));
        let mut s = task.title.clone();
        for tag in &row.tags {
            s.push(' ');
            s.push('@');
            s.push_str(tag);
        }
        if let Some(start) = task.scheduled_start_at {
            s.push_str(" ~");
            s.push_str(&horae_core::time::format_quick_time(start));
        }
        if let Some(rr) = &task.rrule {
            s.push(' ');
            s.push('*');
            s.push_str(&horae_core::parser::rrule_to_shorthand(rr));
        }
        if let Some(ref p) = task.priority {
            s.push(' ');
            s.push('!');
            s.push_str(p);
        }
        s
    }

    /// 在番茄钟全屏专注模式下直接完成当前专注任务并结算番茄钟。
    pub(crate) fn complete_pomodoro_task(&mut self, task_id: &str) -> Result<()> {
        let task = tasks::get(self.conn, task_id)?;
        let prev_status = task.status;
        let title = task.title.clone();

        // 1. 状态流转为 Done
        let t = tasks::transition(self.conn, task_id, task::Status::Done)?;
        self.push_undo(super::UndoAction::StatusChange {
            task_id: task_id.to_string(),
            from: prev_status,
            to: t.status,
            title: title.clone(),
        });

        // 2. 终止并重置番茄钟
        let _ = horae_core::pomo::stop();
        self.force_reload_pomo();

        // 3. 播放完成音效/桌面提示
        let _ = horae_core::notify::completed_feedback(self.conn);

        // 4. 弹出成功 Toast
        self.set_toast(
            tr!(
                self.lang,
                "🎉 专注达成并已完成: {} (按 u 撤销)",
                "🎉 Focus completed: {} (press u to undo)",
                title
            ),
            true,
        );

        // 5. 刷新视图
        self.refresh()?;
        self.load_detail();
        Ok(())
    }

    pub(crate) fn act_on_selected(&mut self, to: task::Status) -> Result<()> {
        let mut ids = vec![];
        if !self.selected_ids.is_empty() {
            ids.extend(self.selected_ids.iter().cloned());
        } else if let Some(row) = self.items.get(self.selected).cloned() {
            ids.push(row.id);
        }

        if ids.is_empty() {
            return Ok(());
        }

        if ids.len() == 1 {
            let id = &ids[0];
            if let Ok(task) = tasks::get(self.conn, id) {
                if task.status == to {
                    self.set_toast(
                        tr!(
                            self.lang,
                            "已是 {} 状态",
                            "already {}",
                            crate::tui::status_cn(self.lang, task.status)
                        ),
                        false,
                    );
                    return Ok(());
                }
                // 习惯打卡一天一次：今日已打过卡则只提示，不重复推进排程。
                let already_checked_in = to == task::Status::Done
                    && task.rrule.is_some()
                    && horae_core::repo::tasks::has_checked_in_today(
                        self.conn,
                        id,
                        horae_core::time::local_day_bounds(0).0,
                    )
                    .unwrap_or(false);
                if already_checked_in {
                    self.set_toast(
                        tr!(
                            self.lang,
                            "{} 今日已打卡",
                            "{} already checked in today",
                            &id[..8]
                        ),
                        false,
                    );
                } else {
                    let prev_status = task.status;
                    // 如果当前变动状态的任务正处于 Pomodoro 专注中，且新状态为 Done/Waiting，终止番茄钟
                    if let Ok(pomo) = horae_core::repo::pomodoro::get_state() {
                        if pomo.task_id.as_deref() == Some(id)
                            && matches!(
                                to,
                                task::Status::Done | task::Status::Waiting | task::Status::Someday
                            )
                        {
                            let _ = horae_core::pomo::stop();
                        }
                    }
                    let t = tasks::transition(self.conn, id, to)?;
                    self.push_undo(super::UndoAction::StatusChange {
                        task_id: id.clone(),
                        from: prev_status,
                        to: t.status,
                        title: task.title.clone(),
                    });
                    if to == task::Status::Done {
                        self.set_toast(
                            tr!(
                                self.lang,
                                "✓ 已完成: {} (按 u 撤销)",
                                "✓ Done: {} (press u to undo)",
                                task.title
                            ),
                            true,
                        );
                    } else {
                        self.set_toast(
                            format!(
                                "{} -> {} (u 撤销)",
                                &t.id[..8],
                                crate::tui::status_cn(self.lang, t.status)
                            ),
                            true,
                        );
                    }
                }
            }
        } else {
            let mut records = Vec::new();
            for id in &ids {
                if let Ok(task) = tasks::get(self.conn, id) {
                    if task.status != to && task.status != task::Status::Scheduled {
                        let prev_status = task.status;
                        if tasks::transition(self.conn, id, to).is_ok() {
                            records.push((id.clone(), prev_status, to));
                            if let Ok(pomo) = horae_core::repo::pomodoro::get_state() {
                                if pomo.task_id.as_deref() == Some(id)
                                    && matches!(
                                        to,
                                        task::Status::Done
                                            | task::Status::Waiting
                                            | task::Status::Someday
                                    )
                                {
                                    let _ = horae_core::pomo::stop();
                                }
                            }
                        }
                    }
                }
            }
            let count = records.len();
            if !records.is_empty() {
                self.push_undo(super::UndoAction::BulkStatusChange { records });
            }
            self.set_toast(
                tr!(
                    self.lang,
                    "批量 {} {} 项 (按 u 撤销)",
                    "Bulk {} {} items (press u to undo)",
                    to,
                    count
                ),
                true,
            );
        }

        if !self.selected_ids.is_empty() {
            self.set_mode(Mode::Normal);
            self.selected_ids.clear();
            self.visual_start_idx = None;
        }

        if to == task::Status::Done {
            let _ = horae_core::notify::completed_feedback(self.conn);
        }

        self.refresh()?;
        self.load_detail();
        Ok(())
    }

    /// 金句移入/移出（`"` 键）：加 `quote` 标签并把工作态任务流转为 reference，
    /// 使条目离开收件箱等行动流；已有该标签则摘除（移出金句视图）。支持多选。
    pub(crate) fn toggle_quotes(&mut self) -> Result<()> {
        let mut ids = vec![];
        if !self.selected_ids.is_empty() {
            ids.extend(self.selected_ids.iter().cloned());
        } else if let Some(row) = self.items.get(self.selected).cloned() {
            ids.push(row.id);
        }
        if ids.is_empty() {
            return Ok(());
        }
        let mut added = 0;
        let mut removed = 0;
        for id in &ids {
            match self.quotes.toggle_tag(self.conn, id) {
                Ok(Some(true)) => added += 1,
                Ok(Some(false)) => removed += 1,
                _ => {}
            }
        }
        self.status_message = if added > 0 && removed == 0 {
            tr!(
                self.lang,
                "已加入金句 ({} 项)",
                "added to quotes ({} item(s))",
                added
            )
        } else if removed > 0 && added == 0 {
            tr!(
                self.lang,
                "已移出金句 ({} 项)",
                "removed from quotes ({} item(s))",
                removed
            )
        } else {
            tr!(
                self.lang,
                "金句: +{} / -{}",
                "quotes: +{} / -{}",
                added,
                removed
            )
        };
        if !self.selected_ids.is_empty() {
            self.set_mode(Mode::Normal);
            self.selected_ids.clear();
            self.visual_start_idx = None;
        }
        self.reload()?;
        Ok(())
    }

    /// 弹出/更新瞬时 Toast 通知（3 秒自动淡出）。
    pub(crate) fn set_toast(&mut self, message: impl Into<String>, is_success: bool) {
        let msg = message.into();
        self.status_message = msg.clone();
        self.toast = Some(super::Toast {
            message: msg,
            created_at_ms: horae_core::time::now_ms(),
            duration_ms: 3000,
            is_success,
        });
    }

    /// 压入一步可撤销操作。
    pub(crate) fn push_undo(&mut self, action: super::UndoAction) {
        if self.undo_stack.len() >= 50 {
            self.undo_stack.remove(0);
        }
        self.undo_stack.push(action);
        self.redo_stack.clear();
    }

    /// 撤销上一步操作。
    pub(crate) fn undo(&mut self) -> Result<bool> {
        let Some(action) = self.undo_stack.pop() else {
            self.set_toast(
                tr!(self.lang, "已没有可撤销的操作", "Nothing to undo"),
                false,
            );
            return Ok(false);
        };

        match &action {
            super::UndoAction::StatusChange {
                task_id,
                from,
                to: _,
                title,
            } => {
                let _ = tasks::transition(self.conn, task_id, *from)?;
                self.set_toast(
                    tr!(
                        self.lang,
                        "⤺ 已撤销: 恢复「{}」为 {}",
                        "⤺ Undone: restored '{}' to {}",
                        title,
                        crate::tui::status_cn(self.lang, *from)
                    ),
                    true,
                );
            }
            super::UndoAction::BulkStatusChange { records } => {
                let mut count = 0;
                for (id, from, _) in records {
                    if tasks::transition(self.conn, id, *from).is_ok() {
                        count += 1;
                    }
                }
                self.set_toast(
                    tr!(
                        self.lang,
                        "⤺ 已撤销: 恢复 {} 个任务的状态",
                        "⤺ Undone: restored status for {} tasks",
                        count
                    ),
                    true,
                );
            }
            super::UndoAction::Archive {
                task_id,
                from_status,
                title,
            } => {
                tasks::unarchive(self.conn, task_id)?;
                let _ = tasks::transition(self.conn, task_id, *from_status);
                self.set_toast(
                    tr!(
                        self.lang,
                        "⤺ 已撤销: 移出归档「{}」",
                        "⤺ Undone: unarchived '{}'",
                        title
                    ),
                    true,
                );
            }
            super::UndoAction::BulkArchive { records } => {
                let mut count = 0;
                for (id, from_status) in records {
                    if tasks::unarchive(self.conn, id).is_ok() {
                        let _ = tasks::transition(self.conn, id, *from_status);
                        count += 1;
                    }
                }
                self.set_toast(
                    tr!(
                        self.lang,
                        "⤺ 已撤销: 移出归档 {} 项",
                        "⤺ Undone: unarchived {} items",
                        count
                    ),
                    true,
                );
            }
            super::UndoAction::Unarchive { task_id, title } => {
                let _ = tasks::archive(self.conn, task_id);
                self.set_toast(
                    tr!(
                        self.lang,
                        "⤺ 已撤销: 重新归档「{}」",
                        "⤺ Undone: re-archived '{}'",
                        title
                    ),
                    true,
                );
            }
            super::UndoAction::Created { task_id, title } => {
                let _ = tasks::archive(self.conn, task_id);
                self.set_toast(
                    tr!(
                        self.lang,
                        "⤺ 已撤销: 删除新建的「{}」",
                        "⤺ Undone: deleted created '{}'",
                        title
                    ),
                    true,
                );
            }
            super::UndoAction::ChecklistToggled {
                task_id,
                item_id,
                item_title,
            } => {
                let _ = tasks::toggle_checklist_item(self.conn, task_id, item_id);
                self.set_toast(
                    tr!(
                        self.lang,
                        "⤺ 已撤销: 切换检查单「{}」",
                        "⤺ Undone: toggled checklist '{}'",
                        item_title
                    ),
                    true,
                );
            }
        }

        self.redo_stack.push(action);
        self.refresh()?;
        self.load_detail();
        Ok(true)
    }

    /// 重做上一步撤销的操作。
    pub(crate) fn redo(&mut self) -> Result<bool> {
        let Some(action) = self.redo_stack.pop() else {
            self.set_toast(
                tr!(self.lang, "已没有可重做的操作", "Nothing to redo"),
                false,
            );
            return Ok(false);
        };

        match &action {
            super::UndoAction::StatusChange {
                task_id,
                from: _,
                to,
                title,
            } => {
                let _ = tasks::transition(self.conn, task_id, *to)?;
                self.set_toast(
                    tr!(
                        self.lang,
                        "↷ 已重做: 将「{}」流转为 {}",
                        "↷ Redone: moved '{}' to {}",
                        title,
                        crate::tui::status_cn(self.lang, *to)
                    ),
                    true,
                );
            }
            super::UndoAction::BulkStatusChange { records } => {
                let mut count = 0;
                for (id, _, to) in records {
                    if tasks::transition(self.conn, id, *to).is_ok() {
                        count += 1;
                    }
                }
                self.set_toast(
                    tr!(
                        self.lang,
                        "↷ 已重做: 批量流转 {} 项",
                        "↷ Redone: bulk transitioned {} items",
                        count
                    ),
                    true,
                );
            }
            super::UndoAction::Archive {
                task_id,
                from_status: _,
                title,
            } => {
                let _ = tasks::archive(self.conn, task_id);
                self.set_toast(
                    tr!(
                        self.lang,
                        "↷ 已重做: 归档「{}」",
                        "↷ Redone: archived '{}'",
                        title
                    ),
                    true,
                );
            }
            super::UndoAction::BulkArchive { records } => {
                let mut count = 0;
                for (id, _) in records {
                    if tasks::archive(self.conn, id).is_ok() {
                        count += 1;
                    }
                }
                self.set_toast(
                    tr!(
                        self.lang,
                        "↷ 已重做: 批量归档 {} 项",
                        "↷ Redone: bulk archived {} items",
                        count
                    ),
                    true,
                );
            }
            super::UndoAction::Unarchive { task_id, title } => {
                tasks::unarchive(self.conn, task_id)?;
                self.set_toast(
                    tr!(
                        self.lang,
                        "↷ 已重做: 移出归档「{}」",
                        "↷ Redone: unarchived '{}'",
                        title
                    ),
                    true,
                );
            }
            super::UndoAction::Created { task_id, title } => {
                tasks::unarchive(self.conn, task_id)?;
                self.set_toast(
                    tr!(
                        self.lang,
                        "↷ 已重做: 恢复新建的「{}」",
                        "↷ Redone: recreated '{}'",
                        title
                    ),
                    true,
                );
            }
            super::UndoAction::ChecklistToggled {
                task_id,
                item_id,
                item_title,
            } => {
                let _ = tasks::toggle_checklist_item(self.conn, task_id, item_id);
                self.set_toast(
                    tr!(
                        self.lang,
                        "↷ 已重做: 切换检查单「{}」",
                        "↷ Redone: toggled checklist '{}'",
                        item_title
                    ),
                    true,
                );
            }
        }

        self.undo_stack.push(action);
        self.refresh()?;
        self.load_detail();
        Ok(true)
    }
}
