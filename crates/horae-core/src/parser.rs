pub struct QuickAdd {
    pub title: String,
    pub tags: Vec<String>,
    pub time_str: Option<String>,
    pub rrule: Option<String>,
    /// 优先级, 归一化为系统标签名: `!a`→p1 (最高), `!b`→p2, `!c`→p3.
    pub priority: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickAddKind {
    Title,
    Tag,
    Time,
    Rrule,
    Priority,
}

#[derive(Debug)]
pub struct QuickAddToken {
    pub kind: QuickAddKind,
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Map a priority token (after `!`) to its canonical value: `!high` → "high"
/// (最高), `!medium` → "medium", `!low` → "low". Case-insensitive.
/// Returns `None` for unrecognized input (e.g. `!foo`), which callers fall back
/// to treating the word as a normal title.
pub fn priority_value(token: &str) -> Option<&'static str> {
    match token.to_ascii_lowercase().as_str() {
        "high" | "h" | "1" | "p1" | "高" => Some("high"),
        "medium" | "med" | "m" | "2" | "p2" | "中" => Some("medium"),
        "low" | "l" | "3" | "p3" | "低" => Some("low"),
        _ => None,
    }
}

/// Walk words in input order using `split_whitespace` semantics.
/// Each word: `@x`/`＠x`→Tag, `~x`/`～x`/`〜x`→Time, `*x`/`＊x`/`×x`→Rrule, `!x`/`！x`→Priority (each only when
/// length after prefix > 0), else Title. `start`/`end` are byte offsets of the word
/// INCLUDING its prefix.
pub fn tokenize_quick_add(input: &str) -> Vec<QuickAddToken> {
    let mut tokens = Vec::new();
    let mut chars = input.char_indices().peekable();
    while matches!(chars.peek(), Some((_, c)) if c.is_whitespace()) {
        chars.next();
    }
    while let Some((start, _)) = chars.peek().copied() {
        let mut end = start;
        for (idx, c) in chars.by_ref() {
            if c.is_whitespace() {
                end = idx;
                break;
            }
            end = idx + c.len_utf8();
        }
        let word = &input[start..end];
        let kind = if let Some((prefix_len, c)) = first_char_info(word) {
            if word.len() > prefix_len {
                match c {
                    '@' | '＠' => QuickAddKind::Tag,
                    '~' | '～' | '〜' => QuickAddKind::Time,
                    '*' | '＊' | '×' => QuickAddKind::Rrule,
                    '!' | '！' => QuickAddKind::Priority,
                    _ => QuickAddKind::Title,
                }
            } else {
                QuickAddKind::Title
            }
        } else {
            QuickAddKind::Title
        };
        tokens.push(QuickAddToken {
            kind,
            start,
            end,
            text: word.to_string(),
        });
        while matches!(chars.peek(), Some((_, c)) if c.is_whitespace()) {
            chars.next();
        }
    }
    tokens
}

/// Helper: returns (prefix_byte_len, char) of the first char in str.
pub fn first_char_info(s: &str) -> Option<(usize, char)> {
    let c = s.chars().next()?;
    Some((c.len_utf8(), c))
}

/// Helper: strip the first char (regardless of byte length) from a token text.
pub fn strip_token_prefix(s: &str) -> &str {
    match first_char_info(s) {
        Some((len, _)) => &s[len..],
        None => s,
    }
}

pub fn parse_quick_add(input: &str) -> QuickAdd {
    let tokens = tokenize_quick_add(input);
    let mut title_parts = Vec::new();
    let mut tags = Vec::new();
    let mut time_str = None;
    let mut rrule = None;
    let mut priority = None;

    let mut i = 0;
    while i < tokens.len() {
        let tok = &tokens[i];
        match tok.kind {
            QuickAddKind::Title => title_parts.push(tok.text.clone()),
            QuickAddKind::Tag => tags.push(strip_token_prefix(&tok.text).to_string()),
            QuickAddKind::Time => {
                let mut combined = strip_token_prefix(&tok.text).to_string();
                // 吸收紧跟的裸 HH:MM，使 `~2026-08-20 15:30` / `～2026-08-20 15：30` 成为完整绝对时间。
                if let Some(next) = tokens.get(i + 1) {
                    if next.kind == QuickAddKind::Title && is_hhmm(&next.text) {
                        combined.push(' ');
                        combined.push_str(&next.text);
                        i += 1;
                    }
                }
                time_str = Some(combined);
            }
            QuickAddKind::Rrule => {
                rrule = Some(parse_rrule_shorthand(strip_token_prefix(&tok.text)))
            }
            QuickAddKind::Priority => {
                let val = strip_token_prefix(&tok.text);
                if let Some(p) = priority_value(val) {
                    priority = Some(p.to_string());
                } else {
                    // 无法识别的 !x 词按普通标题处理, 不静默丢弃
                    title_parts.push(tok.text.clone());
                }
            }
        }
        i += 1;
    }

    QuickAdd {
        title: title_parts.join(" "),
        tags,
        time_str,
        rrule,
        priority,
    }
}

/// 是否形如裸的 `HH:MM`（1-2 位小时 + 冒号/中文冒号 + 2 位分钟）。
fn is_hhmm(s: &str) -> bool {
    let normalized = s.replace('：', ":");
    let mut parts = normalized.split(':');
    match (parts.next(), parts.next(), parts.next()) {
        (Some(h), Some(m), None) => {
            !h.is_empty()
                && !m.is_empty()
                && h.len() <= 2
                && m.len() == 2
                && h.chars().all(|c| c.is_ascii_digit())
                && m.chars().all(|c| c.is_ascii_digit())
        }
        _ => false,
    }
}

/// 归一化循环简写中的全角符号（括号、逗号、加减号等）。
fn normalize_rrule_symbols(s: &str) -> String {
    s.replace('［', "[")
        .replace('］', "]")
        .replace('【', "[")
        .replace('】', "]")
        .replace('，', ",")
        .replace('－', "-")
        .replace('＋', "+")
}

pub fn parse_rrule_shorthand(s: &str) -> String {
    let normalized = normalize_rrule_symbols(s);
    let lower = normalized.to_lowercase();
    if lower.starts_with("freq=") {
        return normalized;
    }

    match lower.as_str() {
        // 单字母简写：`*d` → 每天, `*w` → 每周, `*m` → 每月, `*y` → 每年。
        "d" => return "FREQ=DAILY".to_string(),
        "w" => return "FREQ=WEEKLY".to_string(),
        "m" => return "FREQ=MONTHLY".to_string(),
        "y" => return "FREQ=YEARLY".to_string(),
        "daily" => return "FREQ=DAILY".to_string(),
        "weekly" => return "FREQ=WEEKLY".to_string(),
        "monthly" => return "FREQ=MONTHLY".to_string(),
        "yearly" => return "FREQ=YEARLY".to_string(),
        "weekday" | "workday" => return "FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR".to_string(),
        "weekend" => return "FREQ=WEEKLY;BYDAY=SA,SU".to_string(),
        _ => {}
    }

    // Try to match patterns like "1d", "2w", "3m", "4y"
    if let Some(pos) = lower.find(|c: char| !c.is_ascii_digit()) {
        if pos > 0 && pos == lower.len() - 1 {
            let num_str = &lower[..pos];
            if let Ok(num) = num_str.parse::<u32>() {
                let unit = &lower[pos..];
                let freq = match unit {
                    "d" => "DAILY",
                    "w" => "WEEKLY",
                    "m" => "MONTHLY",
                    "y" => "YEARLY",
                    _ => "",
                };
                if !freq.is_empty() {
                    return format!("FREQ={};INTERVAL={}", freq, num);
                }
            }
        }
    }

    // "m[1,15]" / "2m[1,15]" → 每(隔)N月的指定几号；
    // "2w[1,3]" / "w[mo,we]" / "2w[0,7]" → 每 N 周的指定星期
    if lower.contains('[') && lower.ends_with(']') {
        if let Some(open) = lower.find('[') {
            let head = &lower[..open];
            let body = &lower[open + 1..lower.len() - 1];
            if let Some(interval) = parse_monthly_head(head) {
                if let Some(days) = parse_month_days(body) {
                    let mut out = String::from("FREQ=MONTHLY");
                    if let Some(iv) = interval {
                        if iv > 1 {
                            out.push_str(&format!(";INTERVAL={}", iv));
                        }
                    }
                    let joined = days
                        .iter()
                        .map(|d| d.to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    out.push_str(&format!(";BYMONTHDAY={}", joined));
                    return out;
                }
            }
            if let Some(interval) = parse_weekly_head(head) {
                if let Some(days) = parse_day_codes(body) {
                    if !days.is_empty() {
                        let mut out = String::from("FREQ=WEEKLY");
                        if let Some(iv) = interval {
                            if iv > 1 {
                                out.push_str(&format!(";INTERVAL={}", iv));
                            }
                        }
                        out.push_str(&format!(";BYDAY={}", days.join(",")));
                        return out;
                    }
                }
            }
            if let Some(interval) = parse_yearly_head(head) {
                if let Some(months) = parse_month_codes(body) {
                    let mut out = String::from("FREQ=YEARLY");
                    if let Some(iv) = interval {
                        if iv > 1 {
                            out.push_str(&format!(";INTERVAL={}", iv));
                        }
                    }
                    let joined = months
                        .iter()
                        .map(|m| m.to_string())
                        .collect::<Vec<_>>()
                        .join(",");
                    out.push_str(&format!(";BYMONTH={}", joined));
                    return out;
                }
            }
        }
    }

    // Try to match comma separated days like "mon,we,fri"
    let mut days = Vec::new();
    let mut valid = true;
    for part in lower.split(',') {
        match day_code(part.trim()) {
            Some(code) => days.push(code),
            None => {
                valid = false;
                break;
            }
        }
    }
    if valid && !days.is_empty() {
        return format!("FREQ=WEEKLY;BYDAY={}", days.join(","));
    }

    // fallback
    s.to_string()
}

/// 把已规范化的 RRULE（如 `FREQ=DAILY` / `FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE`）
/// 还原为用户友好的紧凑简写语法（如 `d` / `2w[1,3]`），用于编辑区回填，避免向用户展示机器语法。
pub fn rrule_to_shorthand(rrule: &str) -> String {
    let mut clean = rrule.trim();
    if let Some(stripped) = clean.strip_prefix("RRULE:") {
        clean = stripped.trim();
    } else if let Some(stripped) = clean.strip_prefix("rrule:") {
        clean = stripped.trim();
    }
    if !clean.to_ascii_uppercase().contains("FREQ=") {
        return clean.to_string();
    }

    let mut freq = None;
    let mut interval = 1u32;
    let mut byday = None;
    let mut bymonthday = None;
    let mut bymonth = None;

    for part in clean.split(';') {
        let mut kv = part.splitn(2, '=');
        let k = kv.next().unwrap_or("").trim().to_ascii_uppercase();
        let v = kv.next().unwrap_or("").trim();
        match k.as_str() {
            "FREQ" => freq = Some(v.to_ascii_uppercase()),
            "INTERVAL" => {
                if let Ok(n) = v.parse::<u32>() {
                    interval = n;
                }
            }
            "BYDAY" => byday = Some(v.to_ascii_uppercase()),
            "BYMONTHDAY" => bymonthday = Some(v.to_string()),
            "BYMONTH" => bymonth = Some(v.to_string()),
            _ => {
                // 若包含非常规字段（如 UNTIL/COUNT），保持原样避免破坏语义
                return clean.to_string();
            }
        }
    }

    let Some(freq) = freq else {
        return clean.to_string();
    };

    let iv_str = if interval > 1 {
        interval.to_string()
    } else {
        String::new()
    };

    match freq.as_str() {
        "DAILY" => {
            if byday.is_none() && bymonthday.is_none() && bymonth.is_none() {
                if interval == 1 {
                    "d".to_string()
                } else {
                    format!("{}d", interval)
                }
            } else {
                clean.to_string()
            }
        }
        "WEEKLY" => {
            if bymonthday.is_none() && bymonth.is_none() {
                match byday.as_deref() {
                    None => {
                        if interval == 1 {
                            "w".to_string()
                        } else {
                            format!("{}w", interval)
                        }
                    }
                    Some(days) => {
                        let mut nums = Vec::new();
                        let mut ok = true;
                        for d in days.split(',') {
                            let n = match d.trim().to_ascii_uppercase().as_str() {
                                "MO" => 1,
                                "TU" => 2,
                                "WE" => 3,
                                "TH" => 4,
                                "FR" => 5,
                                "SA" => 6,
                                "SU" => 7,
                                _ => {
                                    ok = false;
                                    break;
                                }
                            };
                            if !nums.contains(&n) {
                                nums.push(n);
                            }
                        }
                        if ok {
                            nums.sort_unstable();
                            if nums == [1, 2, 3, 4, 5] && interval == 1 {
                                "weekday".to_string()
                            } else if nums == [6, 7] && interval == 1 {
                                "weekend".to_string()
                            } else {
                                let s_nums: Vec<String> =
                                    nums.into_iter().map(|n| n.to_string()).collect();
                                format!("{}w[{}]", iv_str, s_nums.join(","))
                            }
                        } else {
                            clean.to_string()
                        }
                    }
                }
            } else {
                clean.to_string()
            }
        }
        "MONTHLY" => {
            if byday.is_none() && bymonth.is_none() {
                match bymonthday.as_deref() {
                    None => {
                        if interval == 1 {
                            "m".to_string()
                        } else {
                            format!("{}m", interval)
                        }
                    }
                    Some(days) => {
                        format!("{}m[{}]", iv_str, days)
                    }
                }
            } else {
                clean.to_string()
            }
        }
        "YEARLY" => {
            if byday.is_none() && bymonthday.is_none() {
                match bymonth.as_deref() {
                    None => {
                        if interval == 1 {
                            "y".to_string()
                        } else {
                            format!("{}y", interval)
                        }
                    }
                    Some(months) => {
                        format!("{}y[{}]", iv_str, months)
                    }
                }
            } else {
                clean.to_string()
            }
        }
        _ => clean.to_string(),
    }
}

/// 把循环规则（简写或标准 RRULE）格式化为人类易读的自然语言描述（中/英），用于实时预览与详情展示。
pub fn rrule_friendly_desc(s: &str, lang: crate::i18n::Lang) -> String {
    let normalized = parse_rrule_shorthand(s);
    let shorthand = rrule_to_shorthand(&normalized);
    match shorthand.as_str() {
        "d" => lang.tr("每天", "Daily").to_string(),
        "w" => lang.tr("每周", "Weekly").to_string(),
        "m" => lang.tr("每月", "Monthly").to_string(),
        "y" => lang.tr("每年", "Yearly").to_string(),
        "weekday" => lang
            .tr("工作日 (周一至五)", "Weekdays (Mon-Fri)")
            .to_string(),
        "weekend" => lang.tr("周末 (周六日)", "Weekends (Sat-Sun)").to_string(),
        _ => {
            if let Some(rest) = shorthand.strip_suffix('d') {
                if let Ok(n) = rest.parse::<u32>() {
                    return crate::tr!(lang, "每 {} 天", "Every {} days", n).to_string();
                }
            }
            if let Some(rest) = shorthand.strip_suffix('w') {
                if let Ok(n) = rest.parse::<u32>() {
                    return crate::tr!(lang, "每 {} 周", "Every {} weeks", n).to_string();
                }
            }
            if let Some(rest) = shorthand.strip_suffix('m') {
                if let Ok(n) = rest.parse::<u32>() {
                    return crate::tr!(lang, "每 {} 个月", "Every {} months", n).to_string();
                }
            }
            if let Some(rest) = shorthand.strip_suffix('y') {
                if let Ok(n) = rest.parse::<u32>() {
                    return crate::tr!(lang, "每 {} 年", "Every {} years", n).to_string();
                }
            }
            if shorthand.contains('w') && shorthand.contains('[') {
                if lang.is_zh() {
                    let mut s = shorthand
                        .replace('w', "周")
                        .replace('[', "(周")
                        .replace(']', ")");
                    s = s.replace(',', ",周");
                    return s;
                } else {
                    return format!("Every week {}", shorthand);
                }
            }
            if shorthand.contains('m') && shorthand.contains('[') {
                if lang.is_zh() {
                    return shorthand
                        .replace('m', "月")
                        .replace('[', "(")
                        .replace(']', "日)");
                } else {
                    return format!("Monthly {}", shorthand);
                }
            }
            if shorthand.contains('y') && shorthand.contains('[') {
                if lang.is_zh() {
                    return shorthand
                        .replace('y', "年")
                        .replace('[', "(")
                        .replace(']', "月)");
                } else {
                    return format!("Yearly {}", shorthand);
                }
            }
            normalized
        }
    }
}

/// 判定一个循环简写/标准 RRULE 是否有效。
///
/// 简写能映射成标准 RRULE（非原样 fallback），或已是 `FREQ=` 开头的 RRULE。
/// 无法被任何分支识别的词（原样 fallback）视为无效。支持
/// `FREQ=DAILY|WEEKLY|MONTHLY|YEARLY` 及其 INTERVAL/BYDAY/BYMONTHDAY/BYMONTH 组合。
pub fn rrule_valid(rrule: &str) -> bool {
    let trimmed = rrule.trim();
    if trimmed.is_empty() {
        return false;
    }
    let resolved = parse_rrule_shorthand(trimmed);
    let looks_like_rrule = resolved.to_lowercase().starts_with("freq=");
    resolved != trimmed || looks_like_rrule
}

/// Map a weekday token (name or number) to its two-letter RRULE code.
/// Numbers follow ISO: 1=周一 … 7=周日, 0=周日(别名)。
fn day_code(part: &str) -> Option<&'static str> {
    let p = part.to_lowercase();
    match p.as_str() {
        "mo" | "mon" | "monday" | "1" => Some("MO"),
        "tu" | "tue" | "tuesday" | "2" => Some("TU"),
        "we" | "wed" | "wednesday" | "3" => Some("WE"),
        "th" | "thu" | "thursday" | "4" => Some("TH"),
        "fr" | "fri" | "friday" | "5" => Some("FR"),
        "sa" | "sat" | "saturday" | "6" => Some("SA"),
        "su" | "sun" | "sunday" | "7" | "0" => Some("SU"),
        _ => None,
    }
}

/// Parse the head of a bracketed weekly shorthand (empty or `<N>w`), returning
/// the interval (None when omitted). Only WEEKLY is supported since BYDAY only
/// expands for weekly recurrences.
fn parse_weekly_head(head: &str) -> Option<Option<u32>> {
    if head.is_empty() {
        return Some(None);
    }
    let h = head.to_lowercase();
    let last = h.chars().last()?;
    if last != 'w' {
        return None;
    }
    let num_part = &h[..h.len() - 1];
    if num_part.is_empty() {
        return Some(None);
    }
    num_part.parse::<u32>().ok().map(Some)
}

/// Parse the head of a bracketed monthly shorthand (`m` or `<N>m`), returning
/// the interval (None when omitted). An empty head is NOT monthly — a bare
/// `[1,3]` stays weekly (weekday codes).
fn parse_monthly_head(head: &str) -> Option<Option<u32>> {
    let h = head.to_lowercase();
    let last = h.chars().last()?;
    if last != 'm' {
        return None;
    }
    let num_part = &h[..h.len() - 1];
    if num_part.is_empty() {
        return Some(None);
    }
    num_part.parse::<u32>().ok().map(Some)
}

/// Parse the head of a bracketed yearly shorthand (`y` or `<N>y`), returning
/// the interval (None when omitted).
fn parse_yearly_head(head: &str) -> Option<Option<u32>> {
    let h = head.to_lowercase();
    let last = h.chars().last()?;
    if last != 'y' {
        return None;
    }
    let num_part = &h[..h.len() - 1];
    if num_part.is_empty() {
        return Some(None);
    }
    num_part.parse::<u32>().ok().map(Some)
}

/// Parse a comma-separated day-of-month list into numbers, deduplicated in
/// first-appearance order. Supports 1-31 and negative counts from the end of
/// the month (-1 = last day, -2 = second-to-last, ...). Invalid entries
/// (0, |n|>31, non-numeric) → None.
fn parse_month_days(body: &str) -> Option<Vec<i32>> {
    if body.trim().is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for part in body.split(',') {
        let n = part.trim().parse::<i32>().ok()?;
        if n == 0 || n.abs() > 31 {
            return None;
        }
        if !out.contains(&n) {
            out.push(n);
        }
    }
    Some(out)
}

/// Parse a comma-separated weekday list (numbers or names) into RRULE codes,
/// deduplicated in first-appearance order.
fn parse_day_codes(body: &str) -> Option<Vec<&'static str>> {
    if body.trim().is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for part in body.split(',') {
        let code = day_code(part.trim())?;
        if !out.contains(&code) {
            out.push(code);
        }
    }
    Some(out)
}

/// Parse a comma-separated month list (numbers 1-12 or names) into RRULE
/// BYMONTH numbers, deduplicated and sorted in ascending order. Invalid entries → None.
fn parse_month_codes(body: &str) -> Option<Vec<i32>> {
    if body.trim().is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for part in body.split(',') {
        let n = month_number(part.trim())?;
        if !out.contains(&n) {
            out.push(n);
        }
    }
    out.sort_unstable();
    Some(out)
}

/// Map a month token (number 1-12 or English name/abbr) to its 1-based number.
fn month_number(part: &str) -> Option<i32> {
    let p = part.to_lowercase();
    match p.as_str() {
        "1" | "jan" | "january" => Some(1),
        "2" | "feb" | "february" => Some(2),
        "3" | "mar" | "march" => Some(3),
        "4" | "apr" | "april" => Some(4),
        "5" | "may" => Some(5),
        "6" | "jun" | "june" => Some(6),
        "7" | "jul" | "july" => Some(7),
        "8" | "aug" | "august" => Some(8),
        "9" | "sep" | "sept" | "september" => Some(9),
        "10" | "oct" | "october" => Some(10),
        "11" | "nov" | "november" => Some(11),
        "12" | "dec" | "december" => Some(12),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_value_maps_to_canonical() {
        assert_eq!(priority_value("high"), Some("high"));
        assert_eq!(priority_value("h"), Some("high"));
        assert_eq!(priority_value("1"), Some("high"));
        assert_eq!(priority_value("p1"), Some("high"));
        assert_eq!(priority_value("高"), Some("high"));
        assert_eq!(priority_value("Medium"), Some("medium"));
        assert_eq!(priority_value("med"), Some("medium"));
        assert_eq!(priority_value("m"), Some("medium"));
        assert_eq!(priority_value("2"), Some("medium"));
        assert_eq!(priority_value("中"), Some("medium"));
        assert_eq!(priority_value("LOW"), Some("low"));
        assert_eq!(priority_value("l"), Some("low"));
        assert_eq!(priority_value("3"), Some("low"));
        assert_eq!(priority_value("p3"), Some("low"));
        assert_eq!(priority_value("低"), Some("low"));
        assert_eq!(priority_value("x"), None);
    }

    #[test]
    fn quick_add_parses_priority() {
        let q = parse_quick_add("写周报 @work !high ~+3d");
        assert_eq!(q.title, "写周报");
        assert_eq!(q.tags, vec!["work"]);
        assert_eq!(q.priority.as_deref(), Some("high"));
        assert_eq!(q.time_str.as_deref(), Some("+3d"));

        let q2 = parse_quick_add("极速任务 @work !h");
        assert_eq!(q2.priority.as_deref(), Some("high"));
        let q3 = parse_quick_add("次要任务 @work !3");
        assert_eq!(q3.priority.as_deref(), Some("low"));
        let q4 = parse_quick_add("中文标记 !高");
        assert_eq!(q4.priority.as_deref(), Some("high"));
    }

    #[test]
    fn last_priority_wins() {
        let q = parse_quick_add("任务 !high !low");
        assert_eq!(q.title, "任务");
        assert_eq!(q.priority.as_deref(), Some("low"));
    }

    #[test]
    fn unknown_priority_stays_in_title() {
        let q = parse_quick_add("无效 !z 保留");
        assert_eq!(q.title, "无效 !z 保留");
        assert_eq!(q.priority, None);
    }

    #[test]
    fn rrule_single_letter_shorthand() {
        assert_eq!(parse_rrule_shorthand("d"), "FREQ=DAILY");
        assert_eq!(parse_rrule_shorthand("w"), "FREQ=WEEKLY");
        assert_eq!(parse_rrule_shorthand("m"), "FREQ=MONTHLY");
        assert_eq!(parse_rrule_shorthand("y"), "FREQ=YEARLY");
        // 大小写不敏感
        assert_eq!(parse_rrule_shorthand("D"), "FREQ=DAILY");
    }

    #[test]
    fn rrule_single_letter_shorthand_quick_add() {
        let q = parse_quick_add("晨跑 *d ~07:00");
        assert_eq!(q.title, "晨跑");
        assert_eq!(q.rrule.as_deref(), Some("FREQ=DAILY"));
        assert_eq!(q.time_str.as_deref(), Some("07:00"));
    }

    #[test]
    fn time_absorbs_following_hhmm() {
        let q = parse_quick_add("开会 ~2026-08-20 15:30 @work");
        assert_eq!(q.title, "开会");
        assert_eq!(q.time_str.as_deref(), Some("2026-08-20 15:30"));
        assert_eq!(q.tags, vec!["work"]);

        let q = parse_quick_add("买牛奶 ~明天 09:00");
        assert_eq!(q.time_str.as_deref(), Some("明天 09:00"));
        assert_eq!(q.title, "买牛奶");

        // 无 ~ 前缀的裸时刻不被吸收，留在标题
        let q = parse_quick_add("报告 15:30 ~today");
        assert_eq!(q.time_str.as_deref(), Some("today"));
        assert!(q.title.contains("15:30"), "裸时刻留在标题: {}", q.title);
    }

    #[test]
    fn rrule_bracket_shorthand() {
        // 每两周的周一、周三
        assert_eq!(
            parse_rrule_shorthand("2w[1,3]"),
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE"
        );
        // 无间隔 → 每周
        assert_eq!(parse_rrule_shorthand("w[1,3]"), "FREQ=WEEKLY;BYDAY=MO,WE");
        assert_eq!(parse_rrule_shorthand("[1,3]"), "FREQ=WEEKLY;BYDAY=MO,WE");
        // 名称形式 + 大小写不敏感
        assert_eq!(
            parse_rrule_shorthand("2W[Mon,WE]"),
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE"
        );
        // 0/7 均为周日, 去重
        assert_eq!(
            parse_rrule_shorthand("2w[0,7]"),
            "FREQ=WEEKLY;INTERVAL=2;BYDAY=SU"
        );
        // interval=1 省略
        assert_eq!(parse_rrule_shorthand("1w[5,6]"), "FREQ=WEEKLY;BYDAY=FR,SA");
    }

    #[test]
    fn rrule_monthly_bracket_shorthand() {
        // 每月 1 号、15 号
        assert_eq!(
            parse_rrule_shorthand("m[1,15]"),
            "FREQ=MONTHLY;BYMONTHDAY=1,15"
        );
        // 每隔 2 个月的 1 号、15 号
        assert_eq!(
            parse_rrule_shorthand("2m[1,15]"),
            "FREQ=MONTHLY;INTERVAL=2;BYMONTHDAY=1,15"
        );
        // 大小写不敏感 + 去重
        assert_eq!(
            parse_rrule_shorthand("M[1,1,15]"),
            "FREQ=MONTHLY;BYMONTHDAY=1,15"
        );
        // interval=1 省略
        assert_eq!(
            parse_rrule_shorthand("1m[31]"),
            "FREQ=MONTHLY;BYMONTHDAY=31"
        );
        // 负数表示从月末倒数：-1=最后一天
        assert_eq!(
            parse_rrule_shorthand("m[1,-1]"),
            "FREQ=MONTHLY;BYMONTHDAY=1,-1"
        );
        assert_eq!(parse_rrule_shorthand("m[-1]"), "FREQ=MONTHLY;BYMONTHDAY=-1");
        // 无效天数原样返回
        assert_eq!(parse_rrule_shorthand("m[0]"), "m[0]");
        assert_eq!(parse_rrule_shorthand("m[32]"), "m[32]");
        assert_eq!(parse_rrule_shorthand("m[-32]"), "m[-32]");
        assert_eq!(parse_rrule_shorthand("m[1,x]"), "m[1,x]");
        // 裸括号无 m 前缀仍是星期
        assert_eq!(parse_rrule_shorthand("[1,3]"), "FREQ=WEEKLY;BYDAY=MO,WE");
    }

    #[test]
    fn rrule_monthly_bracket_shorthand_quick_add() {
        let q = parse_quick_add("交房租 *m[1,15] @home");
        assert_eq!(q.title, "交房租");
        assert_eq!(q.rrule.as_deref(), Some("FREQ=MONTHLY;BYMONTHDAY=1,15"));
        assert_eq!(q.tags, vec!["home"]);
    }

    #[test]
    fn rrule_bracket_shorthand_quick_add() {
        let q = parse_quick_add("上体育课 *2w[1,3] ~明天");
        assert_eq!(q.title, "上体育课");
        assert_eq!(
            q.rrule.as_deref(),
            Some("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE")
        );
        assert_eq!(q.time_str.as_deref(), Some("明天"));
    }

    #[test]
    fn rrule_bracket_invalid_falls_through() {
        // 非 weekly 单位不支持括号, 原样返回
        assert_eq!(parse_rrule_shorthand("2d[1,3]"), "2d[1,3]");
        // 无效星期号
        assert_eq!(parse_rrule_shorthand("2w[8]"), "2w[8]");
        assert_eq!(parse_rrule_shorthand("2w[1,x]"), "2w[1,x]");
    }

    #[test]
    fn rrule_valid_accepts_known_rejects_garbage() {
        assert!(rrule_valid("d"));
        assert!(rrule_valid("w"));
        assert!(rrule_valid("m"));
        assert!(rrule_valid("y"), "YEARLY 现已被引擎支持");
        assert!(rrule_valid("4y"), "间隔年循环同样有效");
        assert!(rrule_valid("yearly"));
        assert!(rrule_valid("FREQ=YEARLY"));
        assert!(rrule_valid("y[jan,jul]"));
        assert!(rrule_valid("2y[6]"));
        assert!(rrule_valid("2w[1,3]"));
        assert!(rrule_valid("m[1,15]"));
        assert!(rrule_valid("weekday"));
        assert!(rrule_valid("FREQ=DAILY"));
        assert!(rrule_valid("FREQ=WEEKLY;BYDAY=MO,WE"));
        assert!(!rrule_valid("2d[1,3]"));
        assert!(!rrule_valid("2w[8]"));
        assert!(!rrule_valid("2w[1,x]"));
        assert!(!rrule_valid("xx"));
        assert!(!rrule_valid(""));
        assert!(!rrule_valid("bogus"));
    }

    #[test]
    fn fullwidth_chinese_symbols_quick_add() {
        let q = parse_quick_add("写周报 ＠work ！high ～+3d");
        assert_eq!(q.title, "写周报");
        assert_eq!(q.tags, vec!["work"]);
        assert_eq!(q.priority.as_deref(), Some("high"));
        assert_eq!(q.time_str.as_deref(), Some("+3d"));

        let q2 = parse_quick_add("开会 ～2026-08-20 15：30 ＠work");
        assert_eq!(q2.title, "开会");
        assert_eq!(q2.time_str.as_deref(), Some("2026-08-20 15：30"));
        assert_eq!(q2.tags, vec!["work"]);

        let q3 = parse_quick_add("上课 ＊2w［1，3］ 〜明天");
        assert_eq!(q3.title, "上课");
        assert_eq!(
            q3.rrule.as_deref(),
            Some("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE")
        );
        assert_eq!(q3.time_str.as_deref(), Some("明天"));

        let q4 = parse_quick_add("交房租 ＊m【1，15】 ＠home");
        assert_eq!(q4.title, "交房租");
        assert_eq!(q4.rrule.as_deref(), Some("FREQ=MONTHLY;BYMONTHDAY=1,15"));
        assert_eq!(q4.tags, vec!["home"]);
    }

    #[test]
    fn rrule_yearly_bymonth_sorts_and_dedups() {
        assert_eq!(parse_rrule_shorthand("y[12,1]"), "FREQ=YEARLY;BYMONTH=1,12");
        assert_eq!(
            parse_rrule_shorthand("y[dec,jul,jan,dec]"),
            "FREQ=YEARLY;BYMONTH=1,7,12"
        );
        assert_eq!(
            parse_rrule_shorthand("2y[10,2]"),
            "FREQ=YEARLY;INTERVAL=2;BYMONTH=2,10"
        );
    }

    #[test]
    fn rrule_to_shorthand_and_friendly_desc() {
        assert_eq!(rrule_to_shorthand("FREQ=DAILY"), "d");
        assert_eq!(rrule_to_shorthand("FREQ=DAILY;INTERVAL=2"), "2d");
        assert_eq!(rrule_to_shorthand("FREQ=WEEKLY"), "w");
        assert_eq!(rrule_to_shorthand("FREQ=WEEKLY;INTERVAL=3"), "3w");
        assert_eq!(
            rrule_to_shorthand("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR"),
            "weekday"
        );
        assert_eq!(rrule_to_shorthand("FREQ=WEEKLY;BYDAY=SA,SU"), "weekend");
        assert_eq!(rrule_to_shorthand("FREQ=WEEKLY;BYDAY=MO,WE"), "w[1,3]");
        assert_eq!(
            rrule_to_shorthand("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE"),
            "2w[1,3]"
        );
        assert_eq!(rrule_to_shorthand("FREQ=MONTHLY"), "m");
        assert_eq!(
            rrule_to_shorthand("FREQ=MONTHLY;BYMONTHDAY=1,15"),
            "m[1,15]"
        );
        assert_eq!(
            rrule_to_shorthand("FREQ=MONTHLY;INTERVAL=2;BYMONTHDAY=1,15"),
            "2m[1,15]"
        );
        assert_eq!(rrule_to_shorthand("FREQ=YEARLY"), "y");
        assert_eq!(rrule_to_shorthand("FREQ=YEARLY;BYMONTH=1,7"), "y[1,7]");
        assert_eq!(
            rrule_to_shorthand("FREQ=YEARLY;INTERVAL=2;BYMONTH=1,6"),
            "2y[1,6]"
        );

        // Roundtrip check
        for shorthand in [
            "d", "2d", "w", "3w", "weekday", "weekend", "w[1,3]", "2w[1,3]", "m", "m[1,15]",
            "2m[1,15]", "y", "y[1,7]", "2y[1,6]",
        ] {
            let parsed = parse_rrule_shorthand(shorthand);
            assert_eq!(
                rrule_to_shorthand(&parsed),
                shorthand,
                "Roundtrip failed for {shorthand}"
            );
        }

        // Friendly descriptions
        assert_eq!(rrule_friendly_desc("d", crate::i18n::Lang::Zh), "每天");
        assert_eq!(
            rrule_friendly_desc("2w[1,3]", crate::i18n::Lang::Zh),
            "2周(周1,周3)"
        );
        assert_eq!(
            rrule_friendly_desc("weekday", crate::i18n::Lang::En),
            "Weekdays (Mon-Fri)"
        );
    }

    #[test]
    fn rrule_reverse_mapping_comprehensive_matrix() {
        // 1. 基础单字母与多倍周期
        assert_eq!(rrule_to_shorthand("FREQ=DAILY"), "d");
        assert_eq!(rrule_to_shorthand("FREQ=DAILY;INTERVAL=1"), "d");
        assert_eq!(rrule_to_shorthand("FREQ=DAILY;INTERVAL=3"), "3d");
        assert_eq!(rrule_to_shorthand("FREQ=DAILY;INTERVAL=14"), "14d");

        assert_eq!(rrule_to_shorthand("FREQ=WEEKLY"), "w");
        assert_eq!(rrule_to_shorthand("FREQ=WEEKLY;INTERVAL=1"), "w");
        assert_eq!(rrule_to_shorthand("FREQ=WEEKLY;INTERVAL=2"), "2w");
        assert_eq!(rrule_to_shorthand("FREQ=WEEKLY;INTERVAL=5"), "5w");

        assert_eq!(rrule_to_shorthand("FREQ=MONTHLY"), "m");
        assert_eq!(rrule_to_shorthand("FREQ=MONTHLY;INTERVAL=1"), "m");
        assert_eq!(rrule_to_shorthand("FREQ=MONTHLY;INTERVAL=3"), "3m");
        assert_eq!(rrule_to_shorthand("FREQ=MONTHLY;INTERVAL=6"), "6m");

        assert_eq!(rrule_to_shorthand("FREQ=YEARLY"), "y");
        assert_eq!(rrule_to_shorthand("FREQ=YEARLY;INTERVAL=1"), "y");
        assert_eq!(rrule_to_shorthand("FREQ=YEARLY;INTERVAL=2"), "2y");

        // 2. 别名与特殊星期组
        assert_eq!(
            rrule_to_shorthand("FREQ=WEEKLY;BYDAY=MO,TU,WE,TH,FR"),
            "weekday"
        );
        assert_eq!(
            rrule_to_shorthand("FREQ=WEEKLY;BYDAY=FR,TH,WE,TU,MO"),
            "weekday"
        );
        assert_eq!(rrule_to_shorthand("FREQ=WEEKLY;BYDAY=SA,SU"), "weekend");
        assert_eq!(rrule_to_shorthand("FREQ=WEEKLY;BYDAY=SU,SA"), "weekend");
        // 带有 interval 的工作日/周末保持 Nw[...]
        assert_eq!(
            rrule_to_shorthand("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,TU,WE,TH,FR"),
            "2w[1,2,3,4,5]"
        );
        assert_eq!(
            rrule_to_shorthand("FREQ=WEEKLY;INTERVAL=3;BYDAY=SA,SU"),
            "3w[6,7]"
        );

        // 3. 各种星期组合（单日、多日、乱序、英文缩写）
        assert_eq!(rrule_to_shorthand("FREQ=WEEKLY;BYDAY=MO"), "w[1]");
        assert_eq!(rrule_to_shorthand("FREQ=WEEKLY;BYDAY=SU"), "w[7]");
        assert_eq!(rrule_to_shorthand("FREQ=WEEKLY;BYDAY=MO,WE"), "w[1,3]");
        assert_eq!(rrule_to_shorthand("FREQ=WEEKLY;BYDAY=WE,MO"), "w[1,3]");
        assert_eq!(
            rrule_to_shorthand("FREQ=WEEKLY;INTERVAL=2;BYDAY=MO,WE,FR"),
            "2w[1,3,5]"
        );
        assert_eq!(
            rrule_to_shorthand("FREQ=WEEKLY;INTERVAL=4;BYDAY=TU,TH,SA"),
            "4w[2,4,6]"
        );

        // 4. 月度按日（单日、多日、正数、负数倒数）
        assert_eq!(rrule_to_shorthand("FREQ=MONTHLY;BYMONTHDAY=1"), "m[1]");
        assert_eq!(rrule_to_shorthand("FREQ=MONTHLY;BYMONTHDAY=15"), "m[15]");
        assert_eq!(
            rrule_to_shorthand("FREQ=MONTHLY;BYMONTHDAY=1,15"),
            "m[1,15]"
        );
        assert_eq!(
            rrule_to_shorthand("FREQ=MONTHLY;INTERVAL=2;BYMONTHDAY=1,15"),
            "2m[1,15]"
        );
        assert_eq!(rrule_to_shorthand("FREQ=MONTHLY;BYMONTHDAY=-1"), "m[-1]");
        assert_eq!(
            rrule_to_shorthand("FREQ=MONTHLY;BYMONTHDAY=1,2,-2,-1"),
            "m[1,2,-2,-1]"
        );

        // 5. 年度按月（单月、多月、隔年）
        assert_eq!(rrule_to_shorthand("FREQ=YEARLY;BYMONTH=6"), "y[6]");
        assert_eq!(rrule_to_shorthand("FREQ=YEARLY;BYMONTH=1,7"), "y[1,7]");
        assert_eq!(
            rrule_to_shorthand("FREQ=YEARLY;INTERVAL=2;BYMONTH=6"),
            "2y[6]"
        );
        assert_eq!(
            rrule_to_shorthand("FREQ=YEARLY;INTERVAL=3;BYMONTH=1,6,12"),
            "3y[1,6,12]"
        );

        // 6. 属性无序与大小写容错
        assert_eq!(
            rrule_to_shorthand("byday=mo,we;interval=2;freq=weekly"),
            "2w[1,3]"
        );
        assert_eq!(
            rrule_to_shorthand("bymonth=1,7;freq=yearly;interval=2"),
            "2y[1,7]"
        );

        // 7. 高级/非简写规则安全兜底（不损坏数据，原样保留）
        assert_eq!(
            rrule_to_shorthand("FREQ=DAILY;COUNT=5"),
            "FREQ=DAILY;COUNT=5"
        );
        assert_eq!(
            rrule_to_shorthand("FREQ=WEEKLY;UNTIL=20261231T235959Z"),
            "FREQ=WEEKLY;UNTIL=20261231T235959Z"
        );
        assert_eq!(
            rrule_to_shorthand("FREQ=HOURLY;INTERVAL=2"),
            "FREQ=HOURLY;INTERVAL=2"
        );
        assert_eq!(rrule_to_shorthand("d"), "d");
        assert_eq!(rrule_to_shorthand("2w[1,3]"), "2w[1,3]");

        // 8. 闭环 Roundtrip：所有简写先 parse 为标准 RRULE，再逆向映射必须完全自洽
        let test_cases = [
            "d",
            "daily",
            "2d",
            "14d",
            "w",
            "weekly",
            "2w",
            "4w",
            "weekday",
            "workday",
            "weekend",
            "w[1]",
            "w[7]",
            "w[1,3]",
            "w[mo,we]",
            "2w[1,3]",
            "4w[2,4,6]",
            "m",
            "monthly",
            "2m",
            "6m",
            "m[1]",
            "m[15]",
            "m[1,15]",
            "2m[1,15]",
            "m[-1]",
            "m[1,2,-2,-1]",
            "y",
            "yearly",
            "2y",
            "y[6]",
            "y[1,7]",
            "2y[6]",
            "y[jan,jul]",
        ];
        for input in test_cases {
            let parsed = parse_rrule_shorthand(input);
            let shorthand = rrule_to_shorthand(&parsed);
            let re_parsed = parse_rrule_shorthand(&shorthand);
            assert_eq!(
                parsed, re_parsed,
                "逆向映射与正向解析必须保证标准 RFC 规则完全幂等！输入: {input} -> 简写: {shorthand}"
            );
        }
    }
}
