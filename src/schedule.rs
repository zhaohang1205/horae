use anyhow::{anyhow, Result};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

use crate::model::task::{self, Task};

/// 循环规则展开的视野上限：一次展开至多生成这么多发生点。
/// 对调用方隐藏 —— 展开是模块内部策略，调用方只需要「全部发生点」。
const HORIZON: usize = 366;

/// 展开一个循环规则从锚点开始的全部发生点（inclusive anchor）。
/// 支持 FREQ=DAILY|WEEKLY|MONTHLY 加 INTERVAL、COUNT、UNTIL、BYDAY、BYMONTHDAY。
/// 视野上限为 [`HORIZON`]；COUNT/UNTIL 在其中收紧。
pub fn occurrences(rrule: &str, anchor_ms: i64) -> Result<Vec<i64>> {
    rrule_occurrences(rrule, anchor_ms, HORIZON)
}

/// 从已经展开的发生序列里挑出「最近一次已错过（逾期）的 slot，否则下一个」。
/// 供缓存了展开结果的路径复用，避免重复展开循环规则。
/// `now` 显式传入，测试可注入确定时刻。
pub fn effective_due_from(occ: &[i64], now: i64) -> Option<i64> {
    if let Some(missed) = occ.iter().rev().find(|m| **m <= now).copied() {
        return Some(missed);
    }
    occ.iter().find(|m| **m >= now).copied()
}

/// 任务的「有效截止」：对循环任务 = 最近一次已错过（逾期）的发生点，否则下一个
/// 发生点；对普通任务 = `due_at` 或 `scheduled_start_at`。用于排序/过滤、闹钟
/// 窗口、每日摘要与展示。`now` 用真实时钟。
pub fn effective_due(task: &Task) -> Option<i64> {
    if let Some(rr) = &task.rrule {
        let anchor = task.scheduled_start_at.or(task.due_at);
        if let Some(start) = anchor {
            if let Ok(occ) = occurrences(rr, start) {
                if let Some(d) = effective_due_from(&occ, crate::time::now_ms()) {
                    return Some(d);
                }
            }
            return Some(start);
        }
    }
    task.due_at.or(task.scheduled_start_at)
}

/// 把循环任务的排程窗口推进到下一次发生：给定规则、锚点与当前终点，
/// 返回 (下一开始, 下一终点)。duration 保持 = end-or-anchor 的跨距；
/// 无终点时 duration 为 0。`next_window` 是纯计算，数据库写入由调用方完成。
pub fn next_window(rrule: &str, anchor: i64, end: Option<i64>) -> Option<(i64, i64)> {
    let occ = occurrences(rrule, anchor).ok()?;
    let next = occ.into_iter().find(|m| *m > anchor)?;
    let duration = end.unwrap_or(anchor) - anchor;
    Some((next, next + duration))
}

/// 任务的展示用到期时间阶梯：归档用归档时间，已完成用完成时间，循环任务用
/// 有效截止（错过 slot 即显示其时间/逾期），否则 `due_at`/`scheduled_start_at`。
/// `cached` 是已展开的循环发生点（TUI 刷新缓存），缺省时自行展开。
pub fn display_due(task: &Task, cached: Option<&[i64]>) -> Option<i64> {
    if task.archived_at.is_some() {
        return task.archived_at;
    }
    if task.status == task::Status::Done {
        return task
            .completed_at
            .or(task.due_at)
            .or(task.scheduled_start_at);
    }
    if let Some(occ) = cached {
        return effective_due_from(occ, crate::time::now_ms())
            .or(task.scheduled_start_at)
            .or(task.due_at);
    }
    effective_due(task)
}

/// Minimal, self-contained RRULE expansion (no external crate).
/// Supports FREQ=DAILY|WEEKLY|MONTHLY with INTERVAL, COUNT, UNTIL.
/// `anchor_ms` is the task's scheduled_start_at (UTC ms). Occurrences start at
/// the anchor (inclusive) and stop at COUNT / `limit` / UNTIL.
fn rrule_occurrences(rrule: &str, anchor_ms: i64, limit: usize) -> Result<Vec<i64>> {
    let anchor = Utc
        .timestamp_millis_opt(anchor_ms)
        .single()
        .ok_or_else(|| anyhow!("invalid anchor timestamp"))?;

    let mut freq = "DAILY".to_string();
    let mut interval: i64 = 1;
    let mut count: i64 = limit as i64;
    let mut until_ms: Option<i64> = None;
    let mut byday: Vec<chrono::Weekday> = Vec::new();
    let mut bymonthday: Vec<i32> = Vec::new();

    for part in rrule.split(';') {
        if part.is_empty() {
            continue;
        }
        let (k, v) = part.split_once('=').unwrap_or((part, ""));
        match k.to_uppercase().as_str() {
            "FREQ" => freq = v.to_uppercase(),
            "INTERVAL" => interval = v.parse().unwrap_or(1).max(1),
            "COUNT" => count = v.parse().unwrap_or(limit as i64).max(1),
            "UNTIL" => until_ms = Some(parse_until(v)?),
            "BYDAY" => {
                for d in v.split(',') {
                    match d.trim().to_uppercase().as_str() {
                        "MO" => byday.push(chrono::Weekday::Mon),
                        "TU" => byday.push(chrono::Weekday::Tue),
                        "WE" => byday.push(chrono::Weekday::Wed),
                        "TH" => byday.push(chrono::Weekday::Thu),
                        "FR" => byday.push(chrono::Weekday::Fri),
                        "SA" => byday.push(chrono::Weekday::Sat),
                        "SU" => byday.push(chrono::Weekday::Sun),
                        _ => {}
                    }
                }
            }
            "BYMONTHDAY" => {
                for d in v.split(',') {
                    if let Ok(n) = d.trim().parse::<i32>() {
                        if n != 0 && n.abs() <= 31 && !bymonthday.contains(&n) {
                            bymonthday.push(n);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    let mut out = Vec::new();
    let mut cur = anchor;
    let max_iter = (count as usize).min(limit);

    if freq == "WEEKLY" && !byday.is_empty() {
        let mut occurrences_found = 0;
        let mut current_day = cur;

        while occurrences_found < max_iter {
            // 与 BYMONTHDAY 一致，用本地历日匹配星期（BYDAY 的星期属于本地时区）。
            if byday.contains(&current_day.with_timezone(&Local).weekday()) {
                let ms = current_day.timestamp_millis();
                if let Some(u) = until_ms {
                    if ms > u {
                        break;
                    }
                }
                out.push(ms);
                occurrences_found += 1;
            }
            current_day += chrono::Duration::days(1);
            if current_day.with_timezone(&Local).weekday() == chrono::Weekday::Mon && interval > 1 {
                current_day += chrono::Duration::weeks(interval - 1);
            }
        }
    } else if freq == "MONTHLY" && !bymonthday.is_empty() {
        let mut occurrences_found = 0;
        let mut current_day = cur;

        while occurrences_found < max_iter {
            let local = current_day.with_timezone(&Local);
            let last_day = days_in_month(local.year(), local.month()) as i32;
            let day = local.day() as i32;
            if bymonthday
                .iter()
                .any(|spec| month_day_matches(day, last_day, *spec))
            {
                let ms = current_day.timestamp_millis();
                if let Some(u) = until_ms {
                    if ms > u {
                        break;
                    }
                }
                out.push(ms);
                occurrences_found += 1;
            }
            current_day += chrono::Duration::days(1);
            if current_day.with_timezone(&Local).day() == 1 && interval > 1 {
                current_day = add_months(current_day, interval - 1);
            }
        }
    } else {
        for _ in 0..max_iter {
            let ms = cur.timestamp_millis();
            if let Some(u) = until_ms {
                if ms > u {
                    break;
                }
            }
            out.push(ms);
            cur = step(cur, &freq, interval)?;
        }
    }
    Ok(out)
}

fn step(dt: DateTime<Utc>, freq: &str, interval: i64) -> Result<DateTime<Utc>> {
    match freq {
        "DAILY" => Ok(dt + Duration::days(interval)),
        "WEEKLY" => Ok(dt + Duration::weeks(interval)),
        "MONTHLY" => Ok(add_months(dt, interval)),
        _ => Err(anyhow!("unsupported FREQ in RRULE: {}", freq)),
    }
}

fn add_months(dt: DateTime<Utc>, interval: i64) -> DateTime<Utc> {
    let d = dt.date_naive();
    let total = d.year() as i64 * 12 + (d.month() as i64 - 1) + interval;
    let y = (total / 12) as i32;
    let m = ((total % 12 + 12) % 12 + 1) as u32;
    let last_day = days_in_month(y, m);
    let day = d.day().min(last_day);
    let nd = NaiveDate::from_ymd_opt(y, m, day)
        .unwrap_or(d)
        .and_time(dt.time());
    Utc.from_utc_datetime(&nd)
}

fn days_in_month(y: i32, m: u32) -> u32 {
    let (year, month) = if m == 12 { (y + 1, 1) } else { (y, m + 1) };
    let first_next = NaiveDate::from_ymd_opt(year, month, 1).unwrap();
    let this_first = NaiveDate::from_ymd_opt(y, m, 1).unwrap();
    (first_next - this_first).num_days() as u32
}

/// Whether a calendar `day` (within a month of `last_day` days) matches a
/// BYMONTHDAY spec: positive specs are absolute (15), negative specs count
/// back from the last day (-1 = last day, -2 = second-to-last, ...).
fn month_day_matches(day: i32, last_day: i32, spec: i32) -> bool {
    if spec > 0 {
        day == spec
    } else {
        day == last_day + 1 + spec
    }
}

fn parse_until(v: &str) -> Result<i64> {
    if let Ok(ms) = v.parse::<i64>() {
        return Ok(ms);
    }
    let norm = v.replace('T', " ");
    if let Ok(dt) = NaiveDateTime::parse_from_str(&norm, "%Y-%m-%d %H:%M") {
        return crate::time::local_to_utc_ms(dt);
    }
    if let Ok(dt) = NaiveDateTime::parse_from_str(&norm, "%Y%m%dT%H%M%SZ") {
        return Ok(Utc.from_utc_datetime(&dt).timestamp_millis());
    }
    Err(anyhow!("invalid RRULE UNTIL: '{}'", v))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_ms(d: NaiveDate, t: chrono::NaiveTime) -> i64 {
        crate::time::local_to_utc_ms(d.and_time(t)).unwrap()
    }
    fn midnight() -> chrono::NaiveTime {
        chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap()
    }

    fn mk_task(rrule: Option<&str>, start: Option<i64>, due: Option<i64>) -> Task {
        Task {
            id: "t".into(),
            title: "t".into(),
            notes: String::new(),
            status: task::Status::Scheduled,
            rrule: rrule.map(String::from),
            created_at: 0,
            clarified_at: None,
            due_at: due,
            scheduled_start_at: start,
            scheduled_end_at: None,
            started_at: None,
            completed_at: None,
            archived_at: None,
            updated_at: 0,
            delegated_to: None,
            checklist: vec![],
            archive_reason: None,
        }
    }

    #[test]
    fn rrule_daily() {
        let anchor = local_ms(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), midnight());
        let occs = rrule_occurrences("FREQ=DAILY", anchor, 3).unwrap();
        let expect = ["2026-01-01 00:00", "2026-01-02 00:00", "2026-01-03 00:00"];
        for (i, e) in expect.iter().enumerate() {
            assert_eq!(crate::time::format_local(Some(occs[i])), *e, "{}", i);
        }
    }

    #[test]
    fn rrule_daily_interval_and_count() {
        let anchor = local_ms(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), midnight());
        let occs = rrule_occurrences("FREQ=DAILY;INTERVAL=2;COUNT=3", anchor, 100).unwrap();
        let expect = ["2026-01-01 00:00", "2026-01-03 00:00", "2026-01-05 00:00"];
        for (i, e) in expect.iter().enumerate() {
            assert_eq!(crate::time::format_local(Some(occs[i])), *e, "{}", i);
        }
    }

    #[test]
    fn rrule_weekly_byday() {
        let anchor = local_ms(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), midnight());
        let occs = rrule_occurrences("FREQ=WEEKLY;BYDAY=MO,WE,FR", anchor, 4).unwrap();
        // 2026-01-01 是周四，从它往后找周一/三/五
        let expect = [
            "2026-01-02 00:00",
            "2026-01-05 00:00",
            "2026-01-07 00:00",
            "2026-01-09 00:00",
        ];
        for (i, e) in expect.iter().enumerate() {
            assert_eq!(crate::time::format_local(Some(occs[i])), *e, "{}", i);
        }
    }

    #[test]
    fn rrule_until_stops() {
        let anchor = local_ms(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), midnight());
        let until = local_ms(NaiveDate::from_ymd_opt(2026, 1, 3).unwrap(), midnight());
        let occs = rrule_occurrences(&format!("FREQ=DAILY;UNTIL={}", until), anchor, 100).unwrap();
        assert_eq!(occs.len(), 3);
    }

    #[test]
    fn rrule_monthly_bymonthday() {
        let anchor = local_ms(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), midnight());
        let occs = rrule_occurrences("FREQ=MONTHLY;BYMONTHDAY=1,15", anchor, 6).unwrap();
        let expect = [
            "2026-01-01 00:00",
            "2026-01-15 00:00",
            "2026-02-01 00:00",
            "2026-02-15 00:00",
            "2026-03-01 00:00",
            "2026-03-15 00:00",
        ];
        assert_eq!(occs.len(), expect.len());
        for (i, e) in expect.iter().enumerate() {
            assert_eq!(
                crate::time::format_local(Some(occs[i])),
                *e,
                "occurrence {}",
                i
            );
        }
    }

    #[test]
    fn rrule_monthly_bymonthday_negative_last_day() {
        let anchor = local_ms(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), midnight());
        let occs = rrule_occurrences("FREQ=MONTHLY;BYMONTHDAY=1,-1", anchor, 6).unwrap();
        let expect = [
            "2026-01-01 00:00",
            "2026-01-31 00:00",
            "2026-02-01 00:00",
            "2026-02-28 00:00",
            "2026-03-01 00:00",
            "2026-03-31 00:00",
        ];
        assert_eq!(occs.len(), expect.len());
        for (i, e) in expect.iter().enumerate() {
            assert_eq!(
                crate::time::format_local(Some(occs[i])),
                *e,
                "occurrence {}",
                i
            );
        }
    }

    #[test]
    fn effective_due_from_picks_missed_or_next() {
        let occ = [500, 1500, 2500];
        assert_eq!(
            effective_due_from(&occ, 1000),
            Some(500),
            "错过 → 最近已错过"
        );
        assert_eq!(
            effective_due_from(&occ, 2000),
            Some(1500),
            "仍有错过 → 最近已错过"
        );
        assert_eq!(effective_due_from(&occ, 0), Some(500), "最早错过");
        assert_eq!(
            effective_due_from(&[4000], 1000),
            Some(4000),
            "无错过 → 下一个"
        );
        assert_eq!(effective_due_from(&[], 0), None, "空序列");
    }

    #[test]
    fn effective_due_recurring_uses_anchor() {
        let anchor = 1000;
        let t = mk_task(Some("FREQ=DAILY"), Some(anchor), None);
        // anchor 在过去且无错过约束时，有效截止 = 最近错过或下一个；此处 anchor=1000 远早于 now
        assert!(effective_due(&t).is_some());
    }

    #[test]
    fn next_window_advances_keeping_duration() {
        let anchor = 1000;
        let end = 2000;
        let (next, next_end) = next_window("FREQ=DAILY", anchor, Some(end)).unwrap();
        assert_eq!(next, anchor + 86_400_000);
        assert_eq!(next_end, next + (end - anchor), "duration 保持不变");
    }

    #[test]
    fn next_window_no_end_zero_duration() {
        let anchor = 1000;
        let (next, next_end) = next_window("FREQ=DAILY", anchor, None).unwrap();
        assert_eq!(next_end, next, "无终点 → duration = 0");
    }

    #[test]
    fn display_due_precedence() {
        let now = crate::time::now_ms();
        // Done → completed_at
        let mut done = mk_task(None, None, None);
        done.status = task::Status::Done;
        done.completed_at = Some(now);
        assert_eq!(display_due(&done, None), Some(now));

        // Archived → archived_at
        let mut arch = mk_task(None, None, None);
        arch.archived_at = Some(now);
        assert_eq!(display_due(&arch, None), Some(now));

        // 普通 → due_at
        let plain = mk_task(None, None, Some(now));
        assert_eq!(display_due(&plain, None), Some(now));

        // 循环 + cached → effective_due_from
        let rec = mk_task(Some("FREQ=DAILY"), Some(now), None);
        let occ = [now, now + 86_400_000];
        assert_eq!(display_due(&rec, Some(&occ)), Some(now));
    }
}
