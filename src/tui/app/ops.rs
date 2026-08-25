use crate::model::task::{self, Task};
use crate::repo::tasks;
use anyhow::Result;

use super::{App, Mode, View};

impl<'a> App<'a> {
    /// 回车进入组织/编辑模式：与 capture 同一个一句话编辑器，预填当前任务内容。
    pub(crate) fn open_organize(&mut self) -> Result<()> {
        if self.mode == Mode::Visual {
            self.set_mode(Mode::Normal);
            self.selected_ids.clear();
            self.visual_start_idx = None;
            self.status_message = crate::tr!(
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
        self.status_message = crate::tr!(
            self.lang,
            "组织: 编辑 @标签 ~时间 *周期 (空/Esc 跳过)",
            "organize: edit @tags ~time *rrule (empty/Esc to skip)"
        )
        .into();
        Ok(())
    }

    /// 把任务序列化成 quick-add 一句话（标题 @标签 ~时间 *周期），可解析回原字段。
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
            s.push_str(&crate::time::format_quick_time(start));
        }
        if let Some(rr) = &task.rrule {
            s.push(' ');
            s.push('*');
            s.push_str(rr);
        }
        s
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
                    self.status_message = crate::tr!(
                        self.lang,
                        "已是 {} 状态",
                        "already {}",
                        crate::tui::status_cn(self.lang, task.status)
                    );
                    return Ok(());
                }
                // 习惯打卡一天一次：今日已打过卡则只提示，不重复推进排程。
                let already_checked_in = to == task::Status::Done
                    && task.rrule.is_some()
                    && crate::repo::tasks::checked_in_today(
                        self.conn,
                        crate::time::local_day_bounds(0).0,
                    )
                    .unwrap_or_default()
                    .iter()
                    .any(|tid| tid == id);
                if already_checked_in {
                    self.status_message = crate::tr!(
                        self.lang,
                        "{} 今日已打卡",
                        "{} already checked in today",
                        &id[..8]
                    );
                } else {
                    // 如果当前变动状态的任务正处于 Pomodoro 专注中，且新状态为 Done/Waiting，终止番茄钟
                    if let Ok(pomo) = crate::repo::pomodoro::get_state() {
                        if pomo.task_id.as_deref() == Some(id)
                            && matches!(
                                to,
                                task::Status::Done | task::Status::Waiting | task::Status::Someday
                            )
                        {
                            let _ = crate::commands::pomo::stop();
                        }
                    }
                    let t = tasks::transition(self.conn, id, to)?;
                    self.status_message = format!(
                        "{} -> {}",
                        &t.id[..8],
                        crate::tui::status_cn(self.lang, t.status)
                    );
                }
            }
        } else {
            let mut count = 0;
            for id in &ids {
                if let Ok(task) = tasks::get(self.conn, id) {
                    if task.status != to
                        && task.status != task::Status::Scheduled
                        && tasks::transition(self.conn, id, to).is_ok()
                    {
                        count += 1;
                        if let Ok(pomo) = crate::repo::pomodoro::get_state() {
                            if pomo.task_id.as_deref() == Some(id)
                                && matches!(
                                    to,
                                    task::Status::Done
                                        | task::Status::Waiting
                                        | task::Status::Someday
                                )
                            {
                                let _ = crate::commands::pomo::stop();
                            }
                        }
                    }
                }
            }
            self.status_message =
                crate::tr!(self.lang, "批量 {} {} 项", "Bulk {} {} items", to, count);
        }

        if !self.selected_ids.is_empty() {
            self.set_mode(Mode::Normal);
            self.selected_ids.clear();
            self.visual_start_idx = None;
        }

        if to == task::Status::Done {
            let _ = crate::commands::notify::completed_feedback(self.conn);
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
            crate::tr!(
                self.lang,
                "已加入金句 ({} 项)",
                "added to quotes ({} item(s))",
                added
            )
        } else if removed > 0 && added == 0 {
            crate::tr!(
                self.lang,
                "已移出金句 ({} 项)",
                "removed from quotes ({} item(s))",
                removed
            )
        } else {
            crate::tr!(
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
}
