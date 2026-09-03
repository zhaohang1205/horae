use anyhow::{anyhow, Result};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, TimeZone, Utc};

use crate::model::task::{self, Task};

/// 循环规则展开的视野上限：一次展开至多生成这么多发生点。
/// 对调用方隐藏 —— 展开是模块内部策略，调用方只需要「全部发生点」。
const HORIZON: usize = 366;

/// [`occurrences_since`] 前移展开基准的块数上限：每块推进约 [`HORIZON`] 个
/// 发生点，64 块足以覆盖停摆数十年的习惯，同时给畸形规则兜底。
const ROLL_LIMIT: usize = 64;

/// 展开一个循环规则从锚点开始的全部发生点（inclusive anchor）。
/// 支持 FREQ=DAILY|WEEKLY|MONTHLY|YEARLY 加 INTERVAL、COUNT、UNTIL、BYDAY、
/// BYMONTHDAY、BYMONTH。视野上限为 [`HORIZON`]；COUNT/UNTIL 在其中收紧。
pub fn occurrences(rrule: &str, anchor_ms: i64) -> Result<Vec<i64>> {
    rrule_occurrences(rrule, anchor_ms, HORIZON)
}

/// 展开一个循环规则，并**保证结果覆盖 `from_ms` 之后的一整段视野**。
///
/// 锚点早于 `from_ms` 时（典型：停摆数月的每日习惯，锚点停在几个月前），
/// [`occurrences`] 的 366 上限会被锚点年龄吃掉，展开结果整体早于今天，任务
/// 于是从今日/明日视图与逾期判定中消失。这里把展开基准前移到「不晚于 `from_ms`
/// 的最后一次发生点」再展开一次，结果形如 `[最近一次已错过的发生点, ...]`。
///
/// 带 `COUNT` 的规则按原样展开：前移基准会越过规则终点，凭空造出发生点。
pub fn occurrences_since(rrule: &str, anchor_ms: i64, from_ms: i64) -> Result<Vec<i64>> {
    if has_count(rrule) || anchor_ms >= from_ms {
        return occurrences(rrule, anchor_ms);
    }
    let mut pivot = anchor_ms;
    for _ in 0..ROLL_LIMIT {
        let chunk = rrule_occurrences(rrule, pivot, HORIZON)?;
        if chunk.last().is_some_and(|last| *last >= from_ms) {
            // 本块已跨过 from_ms：以块内最后一次不晚于 from_ms 的发生点为基准
            // 重展一次，拿到完整视野并丢掉过时的前缀。
            return match chunk.iter().rposition(|m| *m <= from_ms) {
                Some(0) | None => Ok(chunk),
                Some(i) => occurrences(rrule, chunk[i]),
            };
        }
        // 整块都早于 from_ms：基准前移到块尾再来一轮（UNTIL 截断/无后续发生点
        // 时块尾不再前进，原样返回）。
        match chunk.last().copied() {
            Some(last) if last > pivot => pivot = last,
            _ => return Ok(chunk),
        }
    }
    rrule_occurrences(rrule, pivot, HORIZON)
}

/// 规则是否用 `COUNT` 限定了总次数（这类规则不能前移展开基准）。
fn has_count(rrule: &str) -> bool {
    rrule.split(';').any(|part| {
        matches!(
            part.split_once('=')
                .unwrap_or((part, ""))
                .0
                .trim()
                .to_uppercase()
                .as_str(),
            "COUNT"
        )
    })
}

/// 任务的排程锚点：循环任务以排程起点为准（它是 RRULE 展开的基准时间），
/// 普通任务以截止时间为准。全系统（日视图、闹钟、提醒、排序）共用这一个定义。
pub fn anchor_ms(task: &Task) -> Option<i64> {
    if task.rrule.is_some() {
        task.scheduled_start_at.or(task.due_at)
    } else {
        task.due_at.or(task.scheduled_start_at)
    }
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
        if let Some(start) = anchor_ms(task) {
            if let Ok(occ) = occurrences_since(rr, start, crate::time::now_ms()) {
                if let Some(d) = effective_due_from(&occ, crate::time::now_ms()) {
                    return Some(d);
                }
            }
            return Some(start);
        }
    }
    task.due_at.or(task.scheduled_start_at)
}

/// 把循环任务的排程窗口推进到下一次发生：给定规则、锚点、当前终点与当前时刻，
/// 返回 (下一开始, 下一终点)。`duration` 保持 = end-or-anchor 的跨距；
/// 无终点时 duration 为 0。`next_window` 是纯计算，数据库写入由调用方完成。
///
/// 从「锚点与 now 中较晚者」之后找第一次发生：漏打多期的习惯打卡后直接跳到
/// `now` 之后的第一次发生，而不是只前进一步、继续留在逾期状态。
pub fn next_window(rrule: &str, anchor: i64, end: Option<i64>, now: i64) -> Option<(i64, i64)> {
    let base = anchor.max(now);
    let occ = occurrences_since(rrule, anchor, base).ok()?;
    let next = occ.into_iter().find(|m| *m > base)?;
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
/// Supports FREQ=DAILY|WEEKLY|MONTHLY|YEARLY with INTERVAL, COUNT, UNTIL, and the
/// BYDAY / BYMONTHDAY / BYMONTH qualifiers the shorthands emit.
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
    let mut bymonth: Vec<i32> = Vec::new();

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
            "BYMONTH" => {
                for m in v.split(',') {
                    if let Ok(n) = m.trim().parse::<i32>() {
                        if (1..=12).contains(&n) && !bymonth.contains(&n) {
                            bymonth.push(n);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    bymonth.sort_unstable();

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
    } else if freq == "YEARLY" && !bymonth.is_empty() {
        let mut occurrences_found = 0;
        let anchor_local = cur.with_timezone(&Local);
        let anchor_day = anchor_local.day() as i32;
        let anchor_time = anchor_local.time();
        let mut year = anchor_local.year();

        while occurrences_found < max_iter {
            for m in bymonth.iter().copied() {
                let last_day = days_in_month(year, m as u32) as i32;
                let day = anchor_day.min(last_day);
                if let Some(nd) = NaiveDate::from_ymd_opt(year, m as u32, day as u32) {
                    let local_dt = nd.and_time(anchor_time);
                    let ms = crate::time::local_to_utc_ms(local_dt)?;
                    if ms >= anchor_ms {
                        if let Some(u) = until_ms {
                            if ms > u {
                                return Ok(out);
                            }
                        }
                        out.push(ms);
                        occurrences_found += 1;
                        if occurrences_found >= max_iter {
                            break;
                        }
                    }
                }
            }
            year += interval as i32;
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
        "YEARLY" => Ok(add_months(dt, 12 * interval)),
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
            priority: None,
            created_at: 0,
            clarified_at: None,
            due_at: due,
            scheduled_start_at: start,
            scheduled_end_at: None,
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
    fn rrule_yearly() {
        let anchor = local_ms(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), midnight());
        let occs = rrule_occurrences("FREQ=YEARLY", anchor, 3).unwrap();
        let expect = ["2026-01-01 00:00", "2027-01-01 00:00", "2028-01-01 00:00"];
        for (i, e) in expect.iter().enumerate() {
            assert_eq!(crate::time::format_local(Some(occs[i])), *e, "{}", i);
        }
    }

    #[test]
    fn rrule_yearly_interval() {
        let anchor = local_ms(NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(), midnight());
        let occs = rrule_occurrences("FREQ=YEARLY;INTERVAL=2", anchor, 3).unwrap();
        let expect = ["2026-01-01 00:00", "2028-01-01 00:00", "2030-01-01 00:00"];
        for (i, e) in expect.iter().enumerate() {
            assert_eq!(crate::time::format_local(Some(occs[i])), *e, "{}", i);
        }
    }

    #[test]
    fn rrule_yearly_bymonth() {
        let anchor = local_ms(NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(), midnight());
        let occs = rrule_occurrences("FREQ=YEARLY;BYMONTH=6,12", anchor, 5).unwrap();
        let expect = [
            "2026-06-15 00:00",
            "2026-12-15 00:00",
            "2027-06-15 00:00",
            "2027-12-15 00:00",
            "2028-06-15 00:00",
        ];
        assert_eq!(occs.len(), expect.len());
        for (i, e) in expect.iter().enumerate() {
            assert_eq!(crate::time::format_local(Some(occs[i])), *e, "{}", i);
        }
    }

    #[test]
    fn rrule_yearly_bymonth_clamps_day() {
        // 锚点 2/28，BYMONTH=2,6：2 月按 28 天，6 月按锚点日 28（<=30）保留。
        let anchor = local_ms(NaiveDate::from_ymd_opt(2026, 2, 28).unwrap(), midnight());
        let occs = rrule_occurrences("FREQ=YEARLY;BYMONTH=2,6", anchor, 2).unwrap();
        let expect = ["2026-02-28 00:00", "2026-06-28 00:00"];
        for (i, e) in expect.iter().enumerate() {
            assert_eq!(crate::time::format_local(Some(occs[i])), *e, "{}", i);
        }
    }

    #[test]
    fn rrule_yearly_unsorted_bymonth_yields_chronological_occurrences() {
        let anchor = local_ms(NaiveDate::from_ymd_opt(2026, 1, 15).unwrap(), midnight());
        // 故意传入倒序的 BYMONTH=12,6，验证生成的时间戳仍严格单调递增
        let occs = rrule_occurrences("FREQ=YEARLY;BYMONTH=12,6", anchor, 4).unwrap();
        let expect = [
            "2026-06-15 00:00",
            "2026-12-15 00:00",
            "2027-06-15 00:00",
            "2027-12-15 00:00",
        ];
        assert_eq!(occs.len(), expect.len());
        for (i, e) in expect.iter().enumerate() {
            assert_eq!(crate::time::format_local(Some(occs[i])), *e, "{}", i);
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
        // now = 锚点：未过期，只前进一步
        let (next, next_end) = next_window("FREQ=DAILY", anchor, Some(end), anchor).unwrap();
        assert_eq!(next, anchor + 86_400_000);
        assert_eq!(next_end, next + (end - anchor), "duration 保持不变");
    }

    #[test]
    fn next_window_no_end_zero_duration() {
        let anchor = 1000;
        let (next, next_end) = next_window("FREQ=DAILY", anchor, None, anchor).unwrap();
        assert_eq!(next_end, next, "无终点 → duration = 0");
    }

    #[test]
    fn next_window_keeps_grid_for_just_passed_slot() {
        let now = crate::time::now_ms();
        let anchor = now - 60_000; // 1 分钟前的今日 slot
        let (next, _) = next_window("FREQ=DAILY", anchor, None, now).unwrap();
        assert_eq!(next, anchor + 86_400_000, "刚过期的 slot 只推进一步");
    }

    #[test]
    fn next_window_skips_missed_periods() {
        let now = crate::time::now_ms();
        let anchor = now - 5 * 86_400_000; // 漏打 5 天
        let (next, _) = next_window("FREQ=DAILY", anchor, None, now).unwrap();
        assert!(next > now, "跳到 now 之后的第一次发生");
        assert!(next <= now + 86_400_000, "不超过一个周期");
    }

    #[test]
    fn occurrences_since_covers_stale_anchor() {
        let now = crate::time::now_ms();
        let anchor = now - 400 * 86_400_000; // 锚点年龄超过 HORIZON=366
        let occ = occurrences_since("FREQ=DAILY", anchor, now).unwrap();
        assert!(
            occ.iter().any(|m| (*m - now).abs() <= 86_400_000),
            "展开结果覆盖今天附近"
        );
        assert!(occ.len() > 1);
        assert!(occ.windows(2).all(|w| w[0] < w[1]), "严格递增");
        assert!(
            occurrences("FREQ=DAILY", anchor).unwrap().last() < Some(&now),
            "原样展开确实覆盖不到现在（这是要修的问题）"
        );
    }

    #[test]
    fn occurrences_since_keeps_last_missed_before_from() {
        let now = crate::time::now_ms();
        let anchor = now - 3 * 86_400_000;
        let occ = occurrences_since("FREQ=WEEKLY", anchor, now).unwrap();
        let missed = occ.iter().filter(|m| **m <= now).count();
        assert_eq!(missed, 1, "只保留最近一次已错过的发生点");
        assert!(occ.iter().any(|m| *m > now), "并给出下一次发生");
    }

    #[test]
    fn occurrences_since_respects_count() {
        let now = crate::time::now_ms();
        let anchor = now - 400 * 86_400_000;
        let occ = occurrences_since("FREQ=DAILY;COUNT=3", anchor, now).unwrap();
        assert_eq!(occ.len(), 3, "COUNT 规则不前移基准，不凭空造发生点");
    }

    #[test]
    fn occurrences_since_fresh_anchor_is_plain_expansion() {
        let now = crate::time::now_ms();
        let occ = occurrences_since("FREQ=WEEKLY;BYDAY=MO,WE,FR", now, now).unwrap();
        assert_eq!(occ, occurrences("FREQ=WEEKLY;BYDAY=MO,WE,FR", now).unwrap());
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
