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

/// Map a priority letter to its tag name: `!a`→p1 (最高), `!b`→p2, `!c`→p3.
pub fn priority_tag(letter: &str) -> Option<&'static str> {
    match letter {
        "a" | "A" => Some("p1"),
        "b" | "B" => Some("p2"),
        "c" | "C" => Some("p3"),
        _ => None,
    }
}

/// Reverse of [`priority_tag`]: `p1`→'a' (最高), `p2`→'b', `p3`→'c'.
pub fn priority_letter(tag: &str) -> Option<char> {
    match tag {
        "p1" => Some('a'),
        "p2" => Some('b'),
        "p3" => Some('c'),
        _ => None,
    }
}

/// Walk words in input order using `split_whitespace` semantics.
/// Each word: `@x`→Tag, `~x`→Time, `*x`→Rrule, `!x`→Priority (each only when
/// `word.len() > 1`), else Title. `start`/`end` are byte offsets of the word
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
        let kind = if word.starts_with('@') && word.len() > 1 {
            QuickAddKind::Tag
        } else if word.starts_with('~') && word.len() > 1 {
            QuickAddKind::Time
        } else if word.starts_with('*') && word.len() > 1 {
            QuickAddKind::Rrule
        } else if word.starts_with('!') && word.len() > 1 {
            QuickAddKind::Priority
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
            QuickAddKind::Tag => tags.push(tok.text[1..].to_string()),
            QuickAddKind::Time => {
                let mut combined = tok.text[1..].to_string();
                // 吸收紧跟的裸 HH:MM，使 `~2026-08-20 15:30` 成为完整绝对时间。
                if let Some(next) = tokens.get(i + 1) {
                    if next.kind == QuickAddKind::Title && is_hhmm(&next.text) {
                        combined.push(' ');
                        combined.push_str(&next.text);
                        i += 1;
                    }
                }
                time_str = Some(combined);
            }
            QuickAddKind::Rrule => rrule = Some(parse_rrule_shorthand(&tok.text[1..])),
            QuickAddKind::Priority => {
                let letter = &tok.text[1..];
                if let Some(tag) = priority_tag(letter) {
                    priority = Some(tag.to_string());
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

/// 是否形如裸的 `HH:MM`（1-2 位小时 + 冒号 + 2 位分钟）。
fn is_hhmm(s: &str) -> bool {
    let mut parts = s.split(':');
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

pub fn parse_rrule_shorthand(s: &str) -> String {
    let lower = s.to_lowercase();
    if lower.starts_with("freq=") {
        return s.to_string();
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

/// 判定一个循环简写/标准 RRULE 是否有效。
///
/// 简写能映射成标准 RRULE（非原样 fallback），或已是 `FREQ=` 开头的 RRULE。
/// 无法被任何分支识别的词（原样 fallback）视为无效。
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn priority_letters_map_to_tags() {
        assert_eq!(priority_tag("a"), Some("p1"));
        assert_eq!(priority_tag("b"), Some("p2"));
        assert_eq!(priority_tag("c"), Some("p3"));
        assert_eq!(priority_tag("A"), Some("p1"));
        assert_eq!(priority_tag("x"), None);
        assert_eq!(priority_letter("p1"), Some('a'));
        assert_eq!(priority_letter("p3"), Some('c'));
    }

    #[test]
    fn quick_add_parses_priority() {
        let q = parse_quick_add("写周报 @work !a ~+3d");
        assert_eq!(q.title, "写周报");
        assert_eq!(q.tags, vec!["work"]);
        assert_eq!(q.priority.as_deref(), Some("p1"));
        assert_eq!(q.time_str.as_deref(), Some("+3d"));
    }

    #[test]
    fn last_priority_wins() {
        let q = parse_quick_add("任务 !a !c");
        assert_eq!(q.title, "任务");
        assert_eq!(q.priority.as_deref(), Some("p3"));
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
        assert!(rrule_valid("y"));
        assert!(rrule_valid("2w[1,3]"));
        assert!(rrule_valid("m[1,15]"));
        assert!(rrule_valid("weekday"));
        assert!(rrule_valid("FREQ=DAILY"));
        assert!(rrule_valid("FREQ=WEEKLY;BYDAY=MO,WE"));
        assert!(rrule_valid("2d[1,3]") == false);
        assert!(rrule_valid("2w[8]") == false);
        assert!(rrule_valid("2w[1,x]") == false);
        assert!(!rrule_valid("xx"));
        assert!(!rrule_valid(""));
        assert!(!rrule_valid("bogus"));
    }
}
