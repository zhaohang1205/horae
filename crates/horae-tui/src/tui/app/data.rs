use crate::tui::row_from_tags_with_due;
use anyhow::Result;
use horae_core::model::task::{self, Task};
use horae_core::repo::tags;
use horae_core::repo::tasks::{self, ListFilter};

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
    /// 循环规则只展开一次（覆盖今日窗口起的一整段视野），结果写入 `rrule_cache`
    /// 供本刷新周期内的列表行复用。
    ///
    /// 语义（两个视图彼此独立，可以各入列一次）：
    /// - **今日** = 今日窗口内的任务 **加上** 逾期任务。循环任务取今日的发生点，
    ///   今日没有发生点时回落到最近一次已错过的发生点（逾期）。
    /// - **明日** = **仅**明日窗口内的任务；逾期与今日未完成的任务不再结转。
    /// - 两个视图都只收**今天能动手的**状态（`Next` / `Scheduled`）：等待中、将来
    ///   也许、参考资料与未澄清的收件箱留在各自的状态视图里，逾期仍由每日摘要与
    ///   周回顾覆盖。
    fn day_lists_from(&mut self, all: &[Task]) -> (DayList, DayList) {
        let (t0s, t0e) = horae_core::time::local_day_bounds(0);
        let (t1s, t1e) = horae_core::time::local_day_bounds(1);

        let mut today = Vec::new();
        let mut tomorrow = Vec::new();
        for t in all {
            if !matches!(t.status, task::Status::Next | task::Status::Scheduled) {
                continue;
            }
            let anchor = horae_core::schedule::anchor_ms(t);
            let occs = match &t.rrule {
                Some(rr) => {
                    let occ = anchor
                        .and_then(|a| horae_core::schedule::occurrences_since(rr, a, t0s).ok());
                    if let Some(ref v) = occ {
                        self.rrule_cache.insert(t.id.clone(), v.clone());
                    }
                    occ
                }
                None => None,
            };
            // 今日：今日窗口内的发生点，否则最近一次已错过的发生点（逾期结转）。
            let d0 = match occs.as_deref() {
                Some(o) => o
                    .iter()
                    .find(|m| **m >= t0s && **m <= t0e)
                    .or_else(|| o.iter().rev().find(|m| **m < t0s))
                    .copied(),
                None => anchor.filter(|d| *d <= t0e),
            };
            // 明日：严格落在明日窗口内 —— 逾期与今日任务不结转。
            let d1 = match occs.as_deref() {
                Some(o) => o.iter().find(|m| **m >= t1s && **m <= t1e).copied(),
                None => anchor.filter(|d| *d >= t1s && *d <= t1e),
            };
            if let Some(a) = d0 {
                today.push((t.clone(), a));
            }
            if let Some(b) = d1 {
                tomorrow.push((t.clone(), b));
            }
        }
        (today, tomorrow)
    }

    /// 任务的展示用到期时间：归档用归档时间，已完成用完成时间，循环任务优先用
    /// 缓存的展开结果算 effective_due（避免重复展开），否则回退到自由函数。
    fn row_due(&self, t: &Task) -> Option<i64> {
        let cached = self.rrule_cache.get(&t.id).map(|v| v.as_slice());
        horae_core::schedule::display_due(t, cached)
    }

    pub(crate) fn refresh(&mut self) -> Result<()> {
        self.items.clear();
        self.rrule_cache.clear();
        let today_start = horae_core::time::local_day_bounds(0).0;
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
        let date_search = if self.search_query.len() == 4
            && self.search_query.bytes().all(|b| b.is_ascii_digit())
        {
            Some(horae_core::time::parse_date_search(&self.search_query)?)
        } else {
            None
        };
        let mut all = tasks::list(
            self.conn,
            &ListFilter {
                status: None,
                tags: tag_f,
                query: if self.search_query.is_empty() || date_search.is_some() {
                    None
                } else {
                    Some(self.search_query.clone())
                },
                review_stale: false,
            },
        )?;
        if let Some((start, end)) = date_search {
            all.retain(|t| {
                horae_core::schedule::effective_due(t)
                    .map(|due| due >= start && due <= end)
                    .unwrap_or(false)
            });
        }

        let (today, tomorrow) = self.day_lists_from(&all);
        self.counts.insert(View::Today, today.len());
        self.counts.insert(View::Tomorrow, tomorrow.len());
        self.refresh_counts()?;

        if self.lunar_enabled {
            let today_date = chrono::Local::now().naive_local().date();
            self.calendar_info = horae_core::lunar::day_calendar_info(today_date);
        } else {
            self.calendar_info = None;
        }

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
                        priority: None,
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
            if let Ok(config) = horae_core::config::Config::load() {
                for name in config.profile_names() {
                    let profile = config.profile(&name);
                    let is_default = config.default_profile == name;
                    let is_current = self.profile_name == name;
                    let db = profile.map(|p| p.db.clone()).unwrap_or_default();
                    let tags = if is_current {
                        vec![tr!(self.lang, "当前", "current").to_string(), db]
                    } else if is_default {
                        vec![tr!(self.lang, "默认", "default").to_string(), db]
                    } else {
                        vec![db]
                    };
                    self.items.push(Row {
                        id: name.clone(),
                        title: name,
                        status: String::new(),
                        due: None,
                        tags,
                        priority: None,
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
        let date_search = if self.search_query.len() == 4
            && self.search_query.bytes().all(|b| b.is_ascii_digit())
        {
            Some(horae_core::time::parse_date_search(&self.search_query)?)
        } else {
            None
        };
        let query = if self.search_query.is_empty() || date_search.is_some() {
            None
        } else {
            Some(self.search_query.as_str())
        };
        let by_status = if let Some((start, end)) = date_search {
            let mut date_tasks = tasks::list(
                self.conn,
                &ListFilter {
                    status: None,
                    tags: self.tag_filter.iter().cloned().collect(),
                    query: None,
                    review_stale: false,
                },
            )?;
            date_tasks.retain(|t| {
                horae_core::schedule::effective_due(t)
                    .map(|due| due >= start && due <= end)
                    .unwrap_or(false)
            });
            let mut counts = std::collections::HashMap::new();
            for task in date_tasks {
                *counts.entry(task.status.to_string()).or_insert(0) += 1;
            }
            counts
        } else {
            tasks::count_by_status(self.conn, query)?
        };
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
