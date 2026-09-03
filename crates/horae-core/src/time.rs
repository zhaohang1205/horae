use anyhow::{anyhow, Result};
use chrono::{
    Datelike, Duration, Local, LocalResult, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc,
};

/// Current time as UTC milliseconds.
pub fn now_ms() -> i64 {
    Utc::now().timestamp_millis()
}

/// Process boot instant, set once by `main` before any work.
static BOOT: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();

/// 记录进程启动时刻（`main` 第一行调用；重复调用只保留第一次）。
pub fn mark_boot() {
    let _ = BOOT.set(std::time::Instant::now());
}

/// 首次查询 [`boot_elapsed_ms`] 时的快照；此后恒定返回同一值。
static BOOT_MS: std::sync::OnceLock<Option<u128>> = std::sync::OnceLock::new();

/// 距 [`mark_boot`] 的毫秒数；未打点（如单测直接构造）时为 `None`。
/// 每次进程只计算一次：首次调用即快照，重绘等后续调用拿到同一值。
pub fn boot_elapsed_ms() -> Option<u128> {
    *BOOT_MS.get_or_init(|| BOOT.get().map(|t| t.elapsed().as_millis()))
}

/// Local-day boundaries in UTC ms for a day offset (0 = today, 1 = tomorrow).
/// Returns `(start, end)` where `start` is local midnight and `end` is
/// 23:59:59.999 of the same day (both inclusive).
pub fn local_day_bounds(offset_days: i64) -> (i64, i64) {
    let day = Local::now().date_naive() + Duration::days(offset_days);
    let start =
        local_to_utc_ms(day.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap())).unwrap_or(0);
    let end =
        local_to_utc_ms(day.and_time(NaiveTime::from_hms_milli_opt(23, 59, 59, 999).unwrap()))
            .unwrap_or(0);
    (start, end)
}

/// Parse a four-digit calendar date used by task search (`MMDD`) and return
/// the inclusive local-day bounds in UTC milliseconds.
pub fn parse_date_search(s: &str) -> Result<(i64, i64)> {
    if s.len() != 4 || !s.bytes().all(|b| b.is_ascii_digit()) {
        return Err(anyhow!("date search must use MMDD, for example 0829"));
    }
    let month: u32 = s[..2].parse().unwrap();
    let day: u32 = s[2..].parse().unwrap();
    let year = Local::now().year();
    let date =
        NaiveDate::from_ymd_opt(year, month, day).ok_or_else(|| anyhow!("invalid date: {}", s))?;
    let start = local_to_utc_ms(date.and_time(NaiveTime::MIN))?;
    let end =
        local_to_utc_ms(date.and_time(NaiveTime::from_hms_milli_opt(23, 59, 59, 999).unwrap()))?;
    Ok((start, end))
}

/// Format a UTC-ms timestamp for display in the user's local timezone.
/// `None` renders as "-".
pub fn format_local(ms: Option<i64>) -> String {
    match ms {
        None => "-".to_string(),
        Some(ms) => match Utc.timestamp_millis_opt(ms).single() {
            Some(dt) => dt
                .with_timezone(&Local)
                .format("%Y-%m-%d %H:%M")
                .to_string(),
            None => ms.to_string(),
        },
    }
}

/// 把时间戳格式化为 quick-add 语法友好且易读的字符串（日期或日期+时刻），
/// 无需机器分隔符 `T`，且零点时不附带无意义的 `00:00`。
pub fn format_quick_time(ms: i64) -> String {
    match Utc.timestamp_millis_opt(ms).single() {
        Some(dt) => {
            let local = dt.with_timezone(&Local);
            if local.time() == chrono::NaiveTime::from_hms_opt(0, 0, 0).unwrap() {
                local.format("%Y-%m-%d").to_string()
            } else {
                local.format("%Y-%m-%d %H:%M").to_string()
            }
        }
        None => ms.to_string(),
    }
}

pub(crate) fn local_to_utc_ms(nd: NaiveDateTime) -> Result<i64> {
    let local_dt = match Local.from_local_datetime(&nd) {
        LocalResult::Single(dt) => dt,
        LocalResult::Ambiguous(a, _b) => a,
        LocalResult::None => return Err(anyhow!("invalid local time: {}", nd)),
    };
    Ok(local_dt.with_timezone(&Utc).timestamp_millis())
}

/// Local-midnight (UTC ms) of the day containing `ms`. Used to classify a
/// timestamp by calendar day rather than a fixed 24h window.
fn day_start_ms(ms: i64) -> i64 {
    Utc.timestamp_millis_opt(ms)
        .single()
        .map(|dt| {
            let d = dt.with_timezone(&Local).date_naive();
            local_to_utc_ms(d.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap())).unwrap_or(ms)
        })
        .unwrap_or(ms)
}

/// Whole calendar days between two timestamps (negative when `to` < `from`).
fn days_between(from_ms: i64, to_ms: i64) -> i64 {
    let d1 = Utc
        .timestamp_millis_opt(from_ms)
        .single()
        .map(|dt| dt.with_timezone(&Local).date_naive());
    let d2 = Utc
        .timestamp_millis_opt(to_ms)
        .single()
        .map(|dt| dt.with_timezone(&Local).date_naive());
    match (d1, d2) {
        (Some(a), Some(b)) => (b - a).num_days(),
        _ => (day_start_ms(to_ms) - day_start_ms(from_ms)) / (24 * 3600 * 1000i64),
    }
}

/// Compact relative description of a due/scheduled timestamp for list rows.
/// Returns `None` when `ms` is `None`. Examples: "逾期2天", "明天", "3天后",
/// "逾期5小时", "2分钟后". Past timestamps uniformly report how overdue they are
/// (逾期X分钟/小时/天); within ±24h it reports precise minutes/hours, beyond
/// that it classifies by local calendar day.
pub fn relative_due(lang: crate::i18n::Lang, ms: Option<i64>) -> Option<String> {
    let ms = ms?;
    let now = now_ms();
    let diff = ms - now;
    let day_ms = 24 * 3600 * 1000i64;

    if diff.abs() < day_ms {
        let hours = diff as f64 / (3600.0 * 1000.0);
        if hours.abs() < 1.0 {
            let m = (hours * 60.0).abs().round().max(1.0) as i64;
            return Some(if diff < 0 {
                crate::tr!(lang, "逾期{}分钟", "{}m overdue", m)
            } else {
                crate::tr!(lang, "{}分钟后", "in {}m", m)
            });
        }
        let h = hours.abs().round() as i64;
        return Some(if diff < 0 {
            crate::tr!(lang, "逾期{}小时", "{}h overdue", h)
        } else {
            crate::tr!(lang, "{}小时后", "in {}h", h)
        });
    }

    let d = days_between(now, ms);
    Some(if d >= 1 {
        if d == 1 {
            crate::tr!(lang, "明天", "tomorrow").to_string()
        } else {
            crate::tr!(lang, "{}天后", "in {}d", d)
        }
    } else {
        crate::tr!(lang, "逾期{}天", "{}d overdue", -d)
    })
}

/// Compact relative description of when a task was completed, for the Done view.
/// Returns `None` when `ms` is `None`. Examples: "3分钟前", "2小时前", "昨天", "3天前".
pub fn relative_past(lang: crate::i18n::Lang, ms: Option<i64>) -> Option<String> {
    let ms = ms?;
    let now = now_ms();
    let diff = now - ms;
    if diff < 0 {
        return None;
    }
    let day_ms = 24 * 3600 * 1000i64;
    if diff < day_ms {
        let hours = diff as f64 / (3600.0 * 1000.0);
        if hours < 1.0 {
            let m = (hours * 60.0).round().max(1.0) as i64;
            return Some(crate::tr!(lang, "{}分钟前", "{}m ago", m));
        }
        let h = hours.round() as i64;
        return Some(crate::tr!(lang, "{}小时前", "{}h ago", h));
    }
    let d = days_between(ms, now);
    if d <= 1 {
        Some(crate::tr!(lang, "昨天", "yesterday").to_string())
    } else {
        Some(crate::tr!(lang, "{}天前", "{}d ago", d))
    }
}

/// Whether a due/scheduled timestamp is overdue (strictly before now).
pub fn is_overdue(ms: Option<i64>) -> bool {
    ms.is_some_and(|m| m < now_ms())
}

/// Parse a human-friendly time string into UTC milliseconds.
/// Supports:
///   "now"
///   "+2h" / "+30m" / "+1d" / "+1w"            (relative to now)
///   "today" / "tomorrow" [ "HH:MM" ]
///   "HH:MM"                                  (today)
///   "2026-07-24"                             (date)
///   "2026-07-24 14:30" / "2026-07-24T14:30"  (datetime)
pub fn parse_time(s: &str) -> Result<i64> {
    let s_clean = s
        .trim()
        .replace('＋', "+")
        .replace('：', ":")
        .replace('。', ".")
        .replace('／', "/");
    let s = s_clean.as_str();
    let now = Local::now();
    let midnight = NaiveTime::from_hms_opt(0, 0, 0).unwrap();

    if s == "now" {
        return Ok(now.with_timezone(&Utc).timestamp_millis());
    }

    // 相对偏移（+2h / +3d / +1w），可带时刻：+3d 15:30 → 3 天后的 15:30。
    if let Some(rest) = s.strip_prefix('+') {
        let (num, unit, after_unit) = split_number_unit(rest)?;
        let dur = match unit {
            'h' => Duration::hours(num),
            'm' => Duration::minutes(num),
            'd' => Duration::days(num),
            'w' => Duration::weeks(num),
            _ => {
                return Err(anyhow!(
                    "unsupported relative unit '{}' (use h/m/d/w)",
                    unit
                ))
            }
        };
        let base = now + dur;
        let after_unit = after_unit.trim();
        if after_unit.is_empty() {
            return Ok(base.with_timezone(&Utc).timestamp_millis());
        }
        let t = parse_optional_time(after_unit, midnight)?;
        return local_to_utc_ms(base.date_naive().and_time(t));
    }

    // 中文天词：今天/明天/后天（可带 HH:MM）
    if let Some(stripped) = s.strip_prefix("今天") {
        let t = parse_optional_time(stripped.trim(), midnight)?;
        return local_to_utc_ms(now.date_naive().and_time(t));
    }
    if let Some(stripped) = s.strip_prefix("明天") {
        let t = parse_optional_time(stripped.trim(), midnight)?;
        let day = now.date_naive() + Duration::days(1);
        return local_to_utc_ms(day.and_time(t));
    }
    if let Some(stripped) = s.strip_prefix("后天") {
        let t = parse_optional_time(stripped.trim(), midnight)?;
        let day = now.date_naive() + Duration::days(2);
        return local_to_utc_ms(day.and_time(t));
    }

    // 星期几（中文）：周X / 星期X / 下周X（X ∈ 一~日, 可带 HH:MM）
    if let Some((wd, time_part, next_week)) = parse_cn_weekday(s) {
        let t = parse_optional_time(time_part, midnight)?;
        let today = now.date_naive();
        let delta = (wd.num_days_from_monday() + 7 - today.weekday().num_days_from_monday()) % 7;
        let off = (delta as i64) + if next_week { 7 } else { 0 };
        let day = today + Duration::days(off);
        return local_to_utc_ms(day.and_time(t));
    }

    // 星期几（英文）：mon / monday / next friday（可带 HH:MM）
    if let Some((wd, time_part, next_week)) = parse_en_weekday(s) {
        let t = parse_optional_time(time_part, midnight)?;
        let today = now.date_naive();
        let delta = (wd.num_days_from_monday() + 7 - today.weekday().num_days_from_monday()) % 7;
        let off = (delta as i64) + if next_week { 7 } else { 0 };
        let day = today + Duration::days(off);
        return local_to_utc_ms(day.and_time(t));
    }

    // English today/tomorrow
    if let Some(stripped) = s.strip_prefix("today") {
        let time = parse_optional_time(stripped.trim(), midnight)?;
        return local_to_utc_ms(now.date_naive().and_time(time));
    }
    if let Some(stripped) = s.strip_prefix("tomorrow") {
        let time = parse_optional_time(stripped.trim(), midnight)?;
        let tomorrow = now.date_naive() + Duration::days(1);
        return local_to_utc_ms(tomorrow.and_time(time));
    }

    // 斜杠/点/短横线日期（可带 HH:MM）：2026/8/20、8/20、2026.8.20、8-20
    if let Some((date_part, time_part)) = split_date_time(s) {
        if let Some(d) = parse_flex_date(date_part) {
            let t = parse_optional_time(time_part, midnight)?;
            return local_to_utc_ms(d.and_time(t));
        }
    }

    // pure time "HH:MM" => today if still upcoming, otherwise tomorrow
    if s.contains(':') && !s.contains('-') {
        if let Ok(t) = NaiveTime::parse_from_str(s, "%H:%M") {
            let candidate = local_to_utc_ms(now.date_naive().and_time(t))?;
            let now_ms = now.with_timezone(&Utc).timestamp_millis();
            if candidate < now_ms {
                let tomorrow = now.date_naive() + Duration::days(1);
                return local_to_utc_ms(tomorrow.and_time(t));
            }
            return Ok(candidate);
        }
    }

    let s_norm = s.replace('T', " ");
    if let Ok(dt) = NaiveDateTime::parse_from_str(&s_norm, "%Y-%m-%d %H:%M") {
        return local_to_utc_ms(dt);
    }
    if let Ok(d) = NaiveDate::parse_from_str(&s_norm, "%Y-%m-%d") {
        return local_to_utc_ms(d.and_time(NaiveTime::from_hms_opt(0, 0, 0).unwrap()));
    }

    Err(anyhow!("could not parse time: '{}'", s))
}

fn split_number_unit(s: &str) -> Result<(i64, char, &str)> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i == 0 {
        return Err(anyhow!("invalid relative time: '{}'", s));
    }
    let num: i64 = s[..i]
        .parse()
        .map_err(|_| anyhow!("invalid relative time: '{}'", s))?;
    let unit = s[i..]
        .chars()
        .next()
        .ok_or_else(|| anyhow!("missing unit in '{}'", s))?;
    let after = &s[i + unit.len_utf8()..];
    Ok((num, unit, after))
}

fn single_weekday_char(c: char) -> Option<chrono::Weekday> {
    use chrono::Weekday;
    match c {
        '一' => Some(Weekday::Mon),
        '二' => Some(Weekday::Tue),
        '三' => Some(Weekday::Wed),
        '四' => Some(Weekday::Thu),
        '五' => Some(Weekday::Fri),
        '六' => Some(Weekday::Sat),
        '日' | '天' => Some(Weekday::Sun),
        _ => None,
    }
}

/// 解析中文星期词，返回 (星期几, 剩余时刻串, 是否下周)。
/// 支持 周X / 星期X / 下周X，X ∈ 一~日。
fn parse_cn_weekday(s: &str) -> Option<(chrono::Weekday, &str, bool)> {
    for (prefix, next_week) in [("下周", true), ("星期", false), ("周", false)] {
        if let Some(body) = s.strip_prefix(prefix) {
            let c = body.chars().next()?;
            let wd = single_weekday_char(c)?;
            return Some((wd, body[c.len_utf8()..].trim(), next_week));
        }
    }
    None
}

/// 解析英文星期词，返回 (星期几, 剩余时刻串, 是否下周)。
/// 支持 mon / monday / tue / tuesday / ... / sun / sunday 及前缀 next。
fn parse_en_weekday(s: &str) -> Option<(chrono::Weekday, &str, bool)> {
    let lower = s.to_lowercase();
    let (prefix_len, next_week) = if lower.starts_with("next ") {
        (5, true)
    } else if lower.starts_with("next") {
        (4, true)
    } else {
        (0, false)
    };
    let rest = s[prefix_len..].trim_start();
    let rest_lower = &lower[s.len() - rest.len()..];

    let word_len = rest_lower
        .find(|c: char| c.is_whitespace() || c.is_ascii_digit() || c == ':')
        .unwrap_or(rest_lower.len());

    let day_part = &rest_lower[..word_len];
    let time_part = rest[word_len..].trim();

    use chrono::Weekday;
    let wd = match day_part {
        "mon" | "monday" => Weekday::Mon,
        "tue" | "tues" | "tuesday" => Weekday::Tue,
        "wed" | "wednesday" => Weekday::Wed,
        "thu" | "thur" | "thurs" | "thursday" => Weekday::Thu,
        "fri" | "friday" => Weekday::Fri,
        "sat" | "saturday" => Weekday::Sat,
        "sun" | "sunday" => Weekday::Sun,
        _ => return None,
    };
    Some((wd, time_part, next_week))
}

/// 把 "日期 [HH:MM]" 拆成 (日期部分, 时刻部分)，时刻部分可能为空串。
fn split_date_time(s: &str) -> Option<(&str, &str)> {
    if let Some(sp) = s.rfind(' ') {
        let time = &s[sp + 1..];
        let time_clean = time.replace('：', ":");
        if time_clean.len() <= 5 && time_clean.contains(':') && !time_clean.contains('-') {
            let (h, m) = time_clean.split_once(':')?;
            if !h.is_empty()
                && !m.is_empty()
                && h.chars().all(|c| c.is_ascii_digit())
                && m.chars().all(|c| c.is_ascii_digit())
                && h.len() <= 2
                && m.len() == 2
            {
                return Some((s[..sp].trim(), time));
            }
        }
    }
    Some((s.trim(), ""))
}

/// 灵活分隔日期：YYYY/M/D、M/D、YYYY.M.D、YYYY-M-D、M-D（后两者当年补零即可）。
fn parse_flex_date(date_part: &str) -> Option<NaiveDate> {
    let sep = if date_part.contains('/') {
        '/'
    } else if date_part.contains('.') {
        '.'
    } else if date_part.contains('-') {
        '-'
    } else {
        return None;
    };
    let parts: Vec<&str> = date_part.split(sep).collect();
    if parts.is_empty() || parts.len() > 3 {
        return None;
    }
    let mut nums = Vec::with_capacity(parts.len());
    for p in &parts {
        if p.is_empty() {
            return None;
        }
        nums.push(p.parse::<i32>().ok()?);
    }
    let today = Local::now().date_naive();
    match nums.len() {
        3 => {
            let (mut y, m, d) = (nums[0], nums[1], nums[2]);
            if y < 100 {
                y += 2000;
            }
            NaiveDate::from_ymd_opt(y, m as u32, d as u32)
        }
        2 => NaiveDate::from_ymd_opt(today.year(), nums[0] as u32, nums[1] as u32),
        _ => None,
    }
}

fn parse_optional_time(s: &str, default: NaiveTime) -> Result<NaiveTime> {
    let s = s.trim().replace('：', ":");
    if s.is_empty() {
        return Ok(default);
    }
    NaiveTime::parse_from_str(&s, "%H:%M").map_err(|_| anyhow!("invalid time: '{}'", s))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_ms(d: NaiveDate, t: NaiveTime) -> i64 {
        local_to_utc_ms(d.and_time(t)).unwrap()
    }
    fn midnight() -> NaiveTime {
        NaiveTime::from_hms_opt(0, 0, 0).unwrap()
    }

    #[test]
    fn parse_chinese_day_words() {
        let today = Local::now().date_naive();
        assert_eq!(parse_time("今天").unwrap(), local_ms(today, midnight()));
        assert_eq!(
            parse_time("明天").unwrap(),
            local_ms(today + Duration::days(1), midnight())
        );
        assert_eq!(
            parse_time("明天 09:30").unwrap(),
            local_ms(
                today + Duration::days(1),
                NaiveTime::from_hms_opt(9, 30, 0).unwrap()
            )
        );
        assert_eq!(
            parse_time("后天").unwrap(),
            local_ms(today + Duration::days(2), midnight())
        );
    }

    #[test]
    fn parse_chinese_weekday() {
        let today = Local::now().date_naive();
        let wd = today.weekday();
        let fri = chrono::Weekday::Fri;
        let delta = (fri.num_days_from_monday() + 7 - wd.num_days_from_monday()) % 7;
        let target = today + Duration::days(delta as i64 + 7);
        assert_eq!(
            parse_time("下周五 15:00").unwrap(),
            local_ms(target, NaiveTime::from_hms_opt(15, 0, 0).unwrap())
        );
        let wed = chrono::Weekday::Wed;
        let delta = (wed.num_days_from_monday() + 7 - wd.num_days_from_monday()) % 7;
        let target = today + Duration::days(delta as i64);
        assert_eq!(parse_time("周三").unwrap(), local_ms(target, midnight()));
        assert_eq!(
            parse_time("星期三 10:00").unwrap(),
            local_ms(target, NaiveTime::from_hms_opt(10, 0, 0).unwrap())
        );
        assert!(parse_time("周日 10:00").is_ok());
        assert!(parse_time("星期天 10:00").is_ok());
    }

    #[test]
    fn parse_english_weekday() {
        let today = Local::now().date_naive();
        let wd = today.weekday();
        let fri = chrono::Weekday::Fri;
        let delta = (fri.num_days_from_monday() + 7 - wd.num_days_from_monday()) % 7;
        let target = today + Duration::days(delta as i64 + 7);
        assert_eq!(
            parse_time("next friday 15:00").unwrap(),
            local_ms(target, NaiveTime::from_hms_opt(15, 0, 0).unwrap())
        );
        let wed = chrono::Weekday::Wed;
        let delta = (wed.num_days_from_monday() + 7 - wd.num_days_from_monday()) % 7;
        let target = today + Duration::days(delta as i64);
        assert_eq!(parse_time("wed").unwrap(), local_ms(target, midnight()));
        assert_eq!(
            parse_time("wednesday 10:00").unwrap(),
            local_ms(target, NaiveTime::from_hms_opt(10, 0, 0).unwrap())
        );
        assert!(parse_time("mon 09:30").is_ok());
        assert!(parse_time("next sun 18:00").is_ok());
    }

    #[test]
    fn parse_slash_dot_dates() {
        let d = NaiveDate::from_ymd_opt(2026, 8, 20).unwrap();
        let t = NaiveTime::from_hms_opt(15, 30, 0).unwrap();
        assert_eq!(parse_time("2026/8/20 15:30").unwrap(), local_ms(d, t));
        assert_eq!(parse_time("2026.8.20 15:30").unwrap(), local_ms(d, t));
        assert_eq!(parse_time("2026-08-20 15:30").unwrap(), local_ms(d, t));
        let now = Local::now();
        let m_d = NaiveDate::from_ymd_opt(now.year(), 8, 20).unwrap();
        assert_eq!(parse_time("8/20 15:30").unwrap(), local_ms(m_d, t));
        assert_eq!(parse_time("8-20 15:30").unwrap(), local_ms(m_d, t));
    }

    #[test]
    fn parse_date_search_returns_current_year_day_bounds() {
        let today = Local::now().date_naive();
        let input = format!("{:02}{:02}", today.month(), today.day());
        let (start, end) = parse_date_search(&input).unwrap();
        assert_eq!(start, local_ms(today, midnight()));
        assert_eq!(
            end,
            local_ms(
                today,
                NaiveTime::from_hms_milli_opt(23, 59, 59, 999).unwrap()
            )
        );
    }

    #[test]
    fn parse_date_search_rejects_invalid_dates() {
        assert!(parse_date_search("0230").is_err());
        assert!(parse_date_search("829").is_err());
        assert!(parse_date_search("1332").is_err());
    }

    #[test]
    fn parse_relative_with_clock() {
        let base = (Local::now() + Duration::days(3)).date_naive();
        let t = NaiveTime::from_hms_opt(15, 30, 0).unwrap();
        assert_eq!(parse_time("+3d 15:30").unwrap(), local_ms(base, t));
        assert!(parse_time("+2h").is_ok());
        assert!(parse_time("+1d").is_ok());
    }

    #[test]
    fn parse_fullwidth_chinese_symbols() {
        let today = Local::now().date_naive();
        let target = today + Duration::days(1);
        let t = NaiveTime::from_hms_opt(15, 30, 0).unwrap();
        assert_eq!(parse_time("明天 15：30").unwrap(), local_ms(target, t));
        assert_eq!(
            parse_time("＋3d 15：30").unwrap(),
            local_ms(today + Duration::days(3), t)
        );
        assert_eq!(
            parse_time("2026／8／20 15：30").unwrap(),
            local_ms(NaiveDate::from_ymd_opt(2026, 8, 20).unwrap(), t)
        );
        assert_eq!(
            parse_time("2026。8。20 15：30").unwrap(),
            local_ms(NaiveDate::from_ymd_opt(2026, 8, 20).unwrap(), t)
        );
    }

    #[test]
    fn boot_elapsed_snapshot_is_stable() {
        // 启动用时只计算一次：重复查询应返回同一快照值。
        std::thread::sleep(std::time::Duration::from_millis(5));
        assert_eq!(boot_elapsed_ms(), boot_elapsed_ms());
    }
}
