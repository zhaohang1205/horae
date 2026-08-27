use super::short_id;
use crate::model::task;
use crate::repo::tasks::CaptureInput;
use crate::repo::{tags, tasks};
use crate::time;
use crate::tui::app::{App, Mode, View};
use anyhow::Result;

impl<'a> App<'a> {
    /// Tab：候选激活时向下推进；未激活时尝试补全当前词（正常情况下
    /// 实时补全已在输入时填充候选，此处兜底）。
    pub(super) fn handle_tab_completion(&mut self) {
        if self.completion_active() {
            let n = self.completion_candidates.len();
            if n > 0 {
                self.completion_index = (self.completion_index + 1) % n;
                self.apply_current_completion();
            }
        } else {
            self.refresh_completion();
        }
    }

    /// 候选激活时上移选择。
    pub(super) fn completion_up(&mut self) {
        if !self.completion_active() {
            return;
        }
        let n = self.completion_candidates.len();
        if n > 0 {
            self.completion_index = (self.completion_index + n - 1) % n;
            self.apply_current_completion();
        }
    }

    /// 候选激活时下移选择。
    pub(super) fn completion_down(&mut self) {
        if !self.completion_active() {
            return;
        }
        let n = self.completion_candidates.len();
        if n > 0 {
            self.completion_index = (self.completion_index + 1) % n;
            self.apply_current_completion();
        }
    }

    /// 是否正在补全（候选列表打开且光标落在被补全 token 内）。
    pub(crate) fn completion_active(&self) -> bool {
        !self.completion_candidates.is_empty()
            && self
                .completion_range
                .map(|(s, e)| s <= self.input_cursor && self.input_cursor <= e)
                .unwrap_or(false)
    }

    /// 接受当前候选：补全完整 token，若光标后无内容则追加一个空格避免粘连，随后关闭候选。
    pub(super) fn accept_completion(&mut self) {
        if self.completion_active() {
            self.apply_current_completion();
            if self.input[self.input_cursor..].is_empty() {
                self.input_insert_char(' ');
            }
        }
        self.clear_completion();
    }

    /// 取消补全：关闭候选（保留当前输入，由用户自行继续输入）。
    pub(super) fn cancel_completion(&mut self) {
        self.clear_completion();
    }

    pub(super) fn confirm_search(&mut self, input: &str) -> Result<()> {
        let query = input.trim();
        let date_search = if query.len() == 4 && query.bytes().all(|b| b.is_ascii_digit()) {
            Some(time::parse_date_search(query)?)
        } else {
            None
        };
        self.search_query = query.to_string();
        if self.search_query.is_empty() {
            self.status_message = crate::tr!(self.lang, "已清除搜索", "Search cleared").into();
        } else if date_search.is_some() {
            self.status_message = crate::tr!(
                self.lang,
                "按日期搜索: {}",
                "Date search: {}",
                self.search_query
            );
        } else {
            self.status_message =
                crate::tr!(self.lang, "搜索: {}", "Search: {}", self.search_query);
        }
        self.reload()
    }

    pub(super) fn confirm_filter_tag(&mut self, input: &str) -> Result<()> {
        let t = input.trim().to_string();
        if t.is_empty() {
            self.tag_filter = None;
            self.status_message =
                crate::tr!(self.lang, "已清除标签过滤", "Tag filter cleared").into();
        } else {
            self.tag_filter = Some(t.clone());
            self.status_message = crate::tr!(self.lang, "过滤标签: @{}", "Filter tag: @{}", t);
        }
        self.reload()
    }

    /// 提交前校验捕获/编辑输入：时间必须可解析，循环必须有效；
    /// 时间解析为过去时刻却带循环时视为冲突。失败返回错误文案。
    pub(super) fn validate_capture_input(&self, input: &str) -> Result<(), String> {
        let quick_add = crate::parser::parse_quick_add(input);
        if let Some(rr) = &quick_add.rrule {
            if !crate::parser::rrule_valid(rr) {
                return Err(crate::tr!(self.lang, "循环无效: {}", "bad rrule: {}", rr).to_string());
            }
        }
        if let Some(ts) = &quick_add.time_str {
            let parsed = time::parse_time(ts).map_err(|e| {
                crate::tr!(self.lang, "时间无效: {}", "bad time: {}", e).to_string()
            })?;
            if quick_add.rrule.is_some() && parsed < crate::time::now_ms() {
                return Err(crate::tr!(
                    self.lang,
                    "循环任务的时间需在未来 ({} 已过去)",
                    "recurring time must be in the future ({} is past)",
                    ts
                )
                .to_string());
            }
        }
        Ok(())
    }

    pub(super) fn confirm_capture(&mut self, input: &str) -> Result<()> {
        let raw_input = input.trim();
        if raw_input.is_empty() {
            self.organizing_id = None;
            return Ok(());
        }
        let quick_add = crate::parser::parse_quick_add(raw_input);
        if let Some(id) = self.organizing_id.take() {
            // Inbox 滞留任务再编辑：与 capture 同一句话编辑器
            let Ok(task) = tasks::get(self.conn, &id) else {
                self.reload()?;
                return Ok(());
            };
            let start_ms = match &quick_add.time_str {
                Some(t) => match time::parse_time(t) {
                    Ok(ms) => Some(ms),
                    Err(e) => {
                        self.status_message =
                            crate::tr!(self.lang, "时间无效: {}", "bad time: {}", e);
                        self.reload()?;
                        return Ok(());
                    }
                },
                None => None,
            };
            // 循环有效性校验。
            if let Some(rr) = &quick_add.rrule {
                if !crate::parser::rrule_valid(rr) {
                    self.status_message =
                        crate::tr!(self.lang, "循环无效: {}", "bad rrule: {}", rr);
                    self.reload()?;
                    return Ok(());
                }
            }
            let mut ok = true;
            if !quick_add.title.is_empty() {
                ok &= self.note(tasks::rename(self.conn, &id, &quick_add.title));
            }
            let mut tag_names = quick_add.tags;
            if let Some(p) = &quick_add.priority {
                tag_names.push(p.clone());
            }
            let new_set: std::collections::HashSet<String> = tag_names.iter().cloned().collect();
            let old_tags = crate::repo::tags::get_task_tags(self.conn, &id).unwrap_or_default();
            for tg in &old_tags {
                if !new_set.contains(&tg.name) {
                    ok &= self.note(crate::repo::tags::remove_tag_from_task(
                        self.conn, &id, &tg.name,
                    ));
                }
            }
            for name in &tag_names {
                ok &= self.note(crate::repo::tags::add_tag_to_task(self.conn, &id, name));
            }
            // @quote 路由：组织/编辑时出现金句标签 → 工作态任务流转为参考资料，离开收件箱。
            let is_quote = self.quotes.enabled && new_set.contains(crate::repo::tasks::QUOTE_TAG);
            if is_quote && !matches!(task.status, task::Status::Reference | task::Status::Done) {
                ok &= self.note(tasks::transition(self.conn, &id, task::Status::Reference));
            }
            // ~time → 排程起点（自动分类 Inbox→Scheduled）；无时间则仅改周期。
            // 金句不参与排程：时间/周期让位给金句路由。
            if is_quote {
                // 金句保持 reference，忽略 ~time 与 *rrule 的排程效果。
            } else if let Some(start) = start_ms {
                if Some(start) != task.scheduled_start_at || quick_add.rrule != task.rrule {
                    ok &= self.note(tasks::schedule(
                        self.conn,
                        &id,
                        start,
                        None,
                        quick_add.rrule.clone(),
                    ));
                }
            } else if quick_add.rrule != task.rrule {
                ok &= self.note(tasks::set_rrule(self.conn, &id, quick_add.rrule.clone()));
            }
            if ok {
                self.status_message =
                    crate::tr!(self.lang, "已组织 {}", "organized {}", short_id(&id));
            }
            self.reload()?;
        } else {
            // 新建捕获：~time → 排程起点（创建后 schedule 设 scheduled_start_at, 状态 Scheduled, 无终点）
            let time_str = quick_add.time_str.clone();
            let rrule = quick_add.rrule;
            if let Some(ts) = &time_str {
                if let Err(e) = time::parse_time(ts) {
                    self.status_message = crate::tr!(self.lang, "时间无效: {}", "bad time: {}", e);
                    return Ok(());
                }
            }
            if let Some(rr) = &rrule {
                if !crate::parser::rrule_valid(rr) {
                    self.status_message =
                        crate::tr!(self.lang, "循环无效: {}", "bad rrule: {}", rr);
                    return Ok(());
                }
            }
            let mut tag_names = quick_add.tags;
            if let Some(p) = &quick_add.priority {
                tag_names.push(p.clone());
            }
            // @quote 路由：捕获输入含金句标签 → 直接创建为 reference + @quote，
            // 自动进入金句视图，不落收件箱。~time/*rrule 让位给金句。
            let is_quote =
                self.quotes.enabled && tag_names.iter().any(|t| t == crate::repo::tasks::QUOTE_TAG);
            let t = tasks::create_capture(
                self.conn,
                &CaptureInput {
                    title: quick_add.title,
                    status: if is_quote {
                        task::Status::Reference
                    } else if time_str.is_some() {
                        task::Status::Scheduled
                    } else {
                        task::Status::Inbox
                    },
                    due_at: None,
                    tag_names,
                    rrule: if time_str.is_some() || is_quote {
                        None
                    } else {
                        rrule.clone()
                    },
                    ..Default::default()
                },
            )?;
            if is_quote {
                self.set_view(View::Quotes);
                self.status_message = crate::tr!(
                    self.lang,
                    "已加入金句 {}",
                    "added to quotes {}",
                    short_id(&t.id)
                );
            } else {
                let scheduled_ok = if let Some(ts) = &time_str {
                    let start = time::parse_time(ts).unwrap();
                    self.note(tasks::schedule(self.conn, &t.id, start, None, rrule))
                } else {
                    true
                };
                self.set_view(View::Inbox);
                if scheduled_ok {
                    self.status_message =
                        crate::tr!(self.lang, "已捕获 {}", "captured {}", short_id(&t.id));
                }
            }
        }
        Ok(())
    }

    pub(super) fn confirm_tagging(&mut self, input: &str) -> Result<()> {
        let name = input.trim();
        if !name.is_empty() {
            let mut ids = vec![];
            if !self.selected_ids.is_empty() {
                ids.extend(self.selected_ids.iter().cloned());
            } else if let Some(row) = self.items.get(self.selected).cloned() {
                ids.push(row.id);
            }

            let mut count = 0;
            for id in ids {
                if tags::add_tag_to_task(self.conn, &id, name).is_ok() {
                    count += 1;
                }
            }
            self.status_message = crate::tr!(
                self.lang,
                "已为 {} 项添加标签 +{}",
                "tagged {} items with +{}",
                count,
                name
            );
            self.selected_ids.clear();
            self.visual_start_idx = None;
            self.reload()?;
        }
        Ok(())
    }

    pub(super) fn confirm_waiting_who(&mut self, input: &str) -> Result<()> {
        if let Some(row) = self.items.get(self.selected).cloned() {
            let who = input.trim();
            if !who.is_empty() {
                let new_title = format!("{} [Wait: {}]", row.title, who);
                tasks::rename(self.conn, &row.id, &new_title)?;
            }
            self.set_mode(Mode::WaitingWhen);
            self.input.clear();
            return Ok(());
        }
        Ok(())
    }

    pub(super) fn confirm_waiting_when(&mut self, input: &str) -> Result<()> {
        let mut start_s = input.trim();
        if start_s.is_empty() {
            start_s = "+1d";
        }
        if let Some(row) = self.items.get(self.selected).cloned() {
            match time::parse_time(start_s) {
                Ok(start_ms) => {
                    tasks::schedule(self.conn, &row.id, start_ms, None, None)?;
                    let t = tasks::transition(self.conn, &row.id, task::Status::Waiting)?;
                    self.status_message =
                        crate::tr!(self.lang, "{} -> 等待中", "{} -> waiting", short_id(&t.id));
                    self.reload()?;
                }
                Err(e) => {
                    self.status_message = crate::tr!(self.lang, "时间无效: {}", "bad time: {}", e)
                }
            }
        }
        Ok(())
    }

    pub(super) fn confirm_checklist_adding(&mut self, input: &str) -> Result<()> {
        if !input.is_empty() {
            if let Some(row) = self.items.get(self.selected).cloned() {
                if tasks::add_checklist_item(self.conn, &row.id, input).is_ok() {
                    self.status_message =
                        crate::tr!(self.lang, "检查单 +1", "Checklist +1").to_string();
                }
                self.load_detail();
            }
        }
        self.set_mode(Mode::Normal);
        self.input.clear();
        Ok(())
    }

    pub(super) fn confirm_creating_tag(&mut self, input: &str) -> Result<()> {
        let name = input.trim().trim_start_matches('@');
        if !name.is_empty() && tags::find_or_create_tag(self.conn, name).is_ok() {
            self.status_message = crate::tr!(self.lang, "已创建标签: {}", "created tag: {}", name);
            self.refresh()?;
        }
        self.set_mode(Mode::Normal);
        self.input.clear();
        Ok(())
    }

    pub(super) fn confirm_creating_profile(&mut self, input: &str) -> Result<()> {
        self.settings_new_profile(input)?;
        self.set_mode(Mode::Normal);
        self.input.clear();
        Ok(())
    }

    pub(super) fn confirm_renaming_profile(&mut self, input: &str) -> Result<()> {
        self.settings_rename_profile(input)?;
        self.set_mode(Mode::Normal);
        self.input.clear();
        Ok(())
    }

    pub(super) fn confirm_pomo_config(&mut self, input: &str) -> Result<()> {
        let parts: Vec<&str> = input.split(';').map(|s| s.trim()).collect();
        if !(3..=4).contains(&parts.len()) {
            self.status_message = crate::tr!(
                self.lang,
                "格式需包含3或4项 (工作;短休;长休[;长休周期])",
                "need 3 or 4 parts (work;short;long[;interval])"
            )
            .into();
            self.set_mode(Mode::Normal);
            self.input.clear();
            return Ok(());
        }

        let w = parts[0].parse::<u32>();
        let s = parts[1].parse::<u32>();
        let l = parts[2].parse::<u32>();
        let i = if parts.len() == 4 {
            parts[3].parse::<u32>()
        } else {
            Ok(crate::model::pomodoro::PomoConfig::default().long_break_interval)
        };

        if let (Ok(w), Ok(s), Ok(l), Ok(i)) = (w, s, l, i) {
            if w > 0 && s > 0 && l > 0 && i > 0 {
                let mut pomo = crate::repo::pomodoro::get_state().unwrap_or_default();
                pomo.config.work_mins = w;
                pomo.config.short_break_mins = s;
                pomo.config.long_break_mins = l;
                pomo.config.long_break_interval = i;
                if crate::repo::pomodoro::save_state(&pomo).is_ok() {
                    self.status_message = crate::tr!(
                        self.lang,
                        "🍅 番茄钟配置已更新: 工作 {}m / 短休 {}m / 长休 {}m / 长休周期 {} 个",
                        "🍅 Pomo config updated: work {}m / short {}m / long {}m / interval {}",
                        w,
                        s,
                        l,
                        i
                    );
                }
            } else {
                self.status_message = crate::tr!(
                    self.lang,
                    "时长与周期必须大于0",
                    "lengths & interval must be > 0"
                )
                .into();
            }
        } else {
            self.status_message = crate::tr!(
                self.lang,
                "配置格式错误 (示例: 25;5;15;4)",
                "invalid format (e.g. 25;5;15;4)"
            )
            .into();
        }
        self.set_mode(Mode::Normal);
        self.input.clear();
        Ok(())
    }
}
