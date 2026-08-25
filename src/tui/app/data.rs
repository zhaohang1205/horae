use crate::model::task::{self, Task};
use crate::repo::tags;
use crate::repo::tasks::{self, ListFilter};
use crate::tui::row_from_tags_with_due;
use anyhow::Result;

use super::{App, DetailData, Row, View};

/// 有状态的 7 个主视图（Inbox..Done），用于按状态统计计数。
const STATUS_VIEWS: [View; 7] = [
    View::Inbox,
    View::Next,
    View::Waiting,
    View::Scheduled,
    View::Someday,
    View::Reference,
    View::Done,
];

/// 今日/明日列表元素：(任务, 展示用到期时间)。
type DayList = Vec<(task::Task, i64)>;

impl<'a> App<'a> {
    pub(crate) fn total_count(&self) -> usize {
        STATUS_VIEWS
            .iter()
            .map(|v| self.counts.get(v).copied().unwrap_or(0))
            .sum()
    }

    pub(crate) fn context_count(&self, v: View) -> usize {
        self.counts.get(&v).copied().unwrap_or(0)
    }

    /// 从已经一次性取出的 `all`（未归档、含标签/搜索过滤）里算出今日/明日列表。
    /// 循环规则只展开一次，结果写入 `rrule_cache` 供本刷新周期内的列表行复用。
    /// `checked_today` 是今日已打卡的循环任务 id 集合（由 `refresh` 一次性查询）。
    fn day_lists_from(
        &mut self,
        all: &[Task],
        checked_today: &std::collections::HashSet<String>,
    ) -> (DayList, DayList) {
        let (t0s, t0e) = crate::time::local_day_bounds(0);
        let (t1s, t1e) = crate::time::local_day_bounds(1);

        let mut today = Vec::new();
        let mut tomorrow = Vec::new();
        let now = crate::time::now_ms();
        for t in all {
            if t.status == task::Status::Done {
                continue;
            }
            let anchor = t.scheduled_start_at.or(t.due_at);
            let occs = match &t.rrule {
                Some(rr) => {
                    let occ = anchor.and_then(|a| crate::schedule::occurrences(rr, a).ok());
                    if let Some(ref v) = occ {
                        self.rrule_cache.insert(t.id.clone(), v.clone());
                    }
                    occ
                }
                None => None,
            };
            // 今日/明日命中 ⇔ 锚点时间落在该日结束之前（含逾期结转）。
            let (d0, d1) = match &occs {
                Some(occs) => (
                    occs.iter().find(|m| **m >= t0s && **m <= t0e).copied(),
                    occs.iter().find(|m| **m >= t1s && **m <= t1e).copied(),
                ),
                None => (anchor.filter(|d| *d <= t0e), anchor.filter(|d| *d <= t1e)),
            };
            // 今日已打卡的循环任务：若其下一次执行不在今日窗口内（d0 未命中），仍保留在
            // 今日视图展示下一次执行时间；d0 命中时由下方 match 统一入列，避免重复。
            if t.rrule.is_some() && checked_today.contains(&t.id) && d0.is_none() {
                if let Some(first) = occs
                    .as_ref()
                    .and_then(|o| o.iter().find(|m| **m >= now).copied())
                {
                    today.push((t.clone(), first));
                }
            }
            match (d0, d1) {
                (Some(a), Some(b)) => {
                    today.push((t.clone(), a));
                    tomorrow.push((t.clone(), b));
                }
                (Some(a), None) => today.push((t.clone(), a)),
                (None, Some(b)) => tomorrow.push((t.clone(), b)),
                (None, None) => {}
            }
        }
        (today, tomorrow)
    }

    /// 任务的展示用到期时间：归档用归档时间，已完成用完成时间，循环任务优先用
    /// 缓存的展开结果算 effective_due（避免重复展开），否则回退到自由函数。
    fn row_due(&self, t: &Task) -> Option<i64> {
        let cached = self.rrule_cache.get(&t.id).map(|v| v.as_slice());
        crate::schedule::display_due(t, cached)
    }

    pub(crate) fn refresh(&mut self) -> Result<()> {
        self.items.clear();
        self.rrule_cache.clear();
        let today_start = crate::time::local_day_bounds(0).0;
        let checked_today: std::collections::HashSet<String> =
            tasks::checked_in_today(self.conn, today_start)
                .unwrap_or_default()
                .into_iter()
                .collect();

        // 一次取全未归档任务（含标签/搜索过滤），今日/明日与各状态视图共用，
        // 避免重复全表扫描与重复 RRULE 展开。
        let mut tag_f = vec![];
        if let Some(ref tf) = self.tag_filter {
            tag_f.push(tf.clone());
        }
        let all = tasks::list(
            self.conn,
            &ListFilter {
                status: None,
                tags: tag_f,
                query: if self.search_query.is_empty() {
                    None
                } else {
                    Some(self.search_query.clone())
                },
                review_stale: false,
            },
        )?;

        let (today, tomorrow) = self.day_lists_from(&all, &checked_today);
        self.counts.insert(View::Today, today.len());
        self.counts.insert(View::Tomorrow, tomorrow.len());
        self.refresh_counts()?;

        // 标签视图单独构建行（没有任务主体）。
        if self.view == View::Tags {
            if let Ok(all_tags) = tags::list_tags(self.conn) {
                for t in all_tags {
                    self.items.push(Row {
                        id: t.id.to_string(),
                        title: format!("@{}", t.name),
                        status: t.category,
                        due: None,
                        tags: vec![],
                        indent: 0,
                        done: None,
                        total: None,
                        archive_reason: None,
                        checked_in_today: false,
                    });
                }
            }
            if self.selected >= self.items.len() {
                self.selected = self.items.len().saturating_sub(1);
            }
            return Ok(());
        }

        // 设置视图单独构建行（读取 config.json 的 profile 列表）。
        if self.view == View::Settings {
            if let Ok(config) = crate::config::Config::load() {
                for name in config.profile_names() {
                    let profile = config.profile(&name);
                    let is_default = config.default_profile == name;
                    let is_current = self.profile_name == name;
                    let db = profile.map(|p| p.db.clone()).unwrap_or_default();
                    let tags = if is_current {
                        vec![crate::tr!(self.lang, "当前", "current").to_string(), db]
                    } else if is_default {
                        vec![crate::tr!(self.lang, "默认", "default").to_string(), db]
                    } else {
                        vec![db]
                    };
                    self.items.push(Row {
                        id: name.clone(),
                        title: name,
                        status: String::new(),
                        due: None,
                        tags,
                        indent: 0,
                        done: None,
                        total: None,
                        archive_reason: None,
                        checked_in_today: false,
                    });
                }
            }
            if self.selected >= self.items.len() {
                self.selected = self.items.len().saturating_sub(1);
            }
            return Ok(());
        }

        // 加载当前视图的任务（今日/明日带展示用到期时间）。
        let tasks: Vec<(task::Task, Option<i64>)> = match self.view {
            View::Today | View::Tomorrow => {
                let mut ts = if self.view == View::Today {
                    today
                } else {
                    tomorrow
                };
                ts.sort_by_key(|(_, due)| *due);
                ts.into_iter().map(|(t, d)| (t, Some(d))).collect()
            }
            View::Archived => tasks::list_archived(self.conn)?
                .into_iter()
                .map(|t| (t, None))
                .collect(),
            View::Review => tasks::list(
                self.conn,
                &ListFilter {
                    status: None,
                    tags: vec![],
                    query: if self.search_query.is_empty() {
                        None
                    } else {
                        Some(self.search_query.clone())
                    },
                    review_stale: true,
                },
            )?
            .into_iter()
            .map(|t| (t, None))
            .collect(),
            View::Quotes => self
                .quotes
                .list(
                    self.conn,
                    if self.search_query.is_empty() {
                        None
                    } else {
                        Some(self.search_query.as_str())
                    },
                    self.tag_filter.as_deref(),
                )?
                .into_iter()
                // 金句行展示创建时间（"3天前"），而非 overdue/due。
                .map(|t| {
                    let created = t.created_at;
                    (t, Some(created))
                })
                .collect(),
            _ => {
                if let Some(s) = self.view.status() {
                    let target = s.parse::<task::Status>().unwrap_or(task::Status::Inbox);
                    // 金句仅存在于金句视图：Reference 视图排除 @quote 任务
                    // （功能关闭时回归普通标签行为）。
                    let exclude_quotes = self.quotes.enabled && target == task::Status::Reference;
                    let quote_ids: std::collections::HashSet<String> = if exclude_quotes {
                        self.quotes.exclude_ids(self.conn)?.into_iter().collect()
                    } else {
                        std::collections::HashSet::new()
                    };
                    all.iter()
                        .filter(|t| {
                            t.status == target && !(exclude_quotes && quote_ids.contains(&t.id))
                        })
                        .cloned()
                        .map(|t| (t, None))
                        .collect()
                } else {
                    Vec::new()
                }
            }
        };

        // 单次查询取所有行的标签，避免逐行 `get_task_tags`。
        let ids: Vec<&str> = tasks.iter().map(|(t, _)| t.id.as_str()).collect();
        let tag_map = tags::get_tags_for_tasks(self.conn, &ids)?;
        for (t, due) in tasks {
            let base_due = due.or_else(|| self.row_due(&t));
            let mut row = row_from_tags_with_due(
                &t,
                0,
                tag_map.get(&t.id).cloned().unwrap_or_default(),
                base_due,
            );
            row.checked_in_today = checked_today.contains(&t.id);
            self.items.push(row);
        }

        if self.selected >= self.items.len() {
            self.selected = self.items.len().saturating_sub(1);
        }
        Ok(())
    }

    /// 一次算好所有视图计数（除今日/明日已在 `refresh` 中赋值），渲染时零查询。
    fn refresh_counts(&mut self) -> Result<()> {
        self.counts.insert(View::Review, 0);
        self.counts
            .insert(View::Archived, tasks::count_archived(self.conn)?);
        self.counts.insert(View::Tags, tags::count_tags(self.conn)?);
        if self.quotes.enabled {
            self.counts.insert(
                View::Quotes,
                self.quotes.count(
                    self.conn,
                    if self.search_query.is_empty() {
                        None
                    } else {
                        Some(self.search_query.as_str())
                    },
                )?,
            );
        }
        let query = if self.search_query.is_empty() {
            None
        } else {
            Some(self.search_query.as_str())
        };
        let by_status = tasks::count_by_status(self.conn, query)?;
        for v in STATUS_VIEWS {
            let s = v.status().expect("status view");
            self.counts
                .insert(v, by_status.get(s).copied().unwrap_or(0));
        }
        // 金句仅存在于金句视图：Reference 徽标排除带 @quote 的参考任务，
        // 与列表保持一致。
        if self.quotes.enabled {
            let quotes_in_ref = self.quotes.count_in_status(self.conn, "reference", query)?;
            let ref_badge = self.counts.get(&View::Reference).copied().unwrap_or(0);
            self.counts
                .insert(View::Reference, ref_badge.saturating_sub(quotes_in_ref));
        }
        Ok(())
    }

    /// 刷新列表并重新加载详情（编辑/操作后的统一收尾）。
    pub(crate) fn reload(&mut self) -> Result<()> {
        self.refresh()?;
        self.load_detail();
        Ok(())
    }

    pub(crate) fn load_detail(&mut self) {
        self.detail = None;
        if let Some(row) = self.items.get(self.selected) {
            if let Ok(task) = tasks::get(self.conn, &row.id) {
                let tg = tags::get_task_tags(self.conn, &row.id).unwrap_or_default();
                let ev = tasks::events(self.conn, &row.id).unwrap_or_default();
                self.detail = Some(DetailData {
                    task,
                    tags: tg,
                    events: ev,
                });
            }
        }
    }
}
