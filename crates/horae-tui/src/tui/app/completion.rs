use super::{App, Mode};

impl<'a> App<'a> {
    /// 输入光标前一个字符的字节偏移；已在开头则返回 0。
    fn cursor_prev(&self) -> usize {
        self.input[..self.input_cursor]
            .char_indices()
            .next_back()
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// 输入光标后一个字符的字节偏移；已在结尾则返回 len。
    fn cursor_next(&self) -> usize {
        self.input[self.input_cursor..]
            .char_indices()
            .nth(1)
            .map(|(i, _)| self.input_cursor + i)
            .unwrap_or(self.input.len())
    }

    /// 在光标处插入一个字符。
    pub(crate) fn input_insert_char(&mut self, c: char) {
        self.input.insert(self.input_cursor, c);
        self.input_cursor += c.len_utf8();
        self.refresh_completion();
    }

    /// 在光标处插入一段文本（用于剪贴板粘贴）。快速录入为单行，
    /// 换行符/制表符归一为空格，避免破坏渲染。
    pub(crate) fn input_insert_str(&mut self, s: &str) {
        let normalized: String = s
            .chars()
            .filter(|&c| c != '\r')
            .map(|c| if c == '\n' || c == '\t' { ' ' } else { c })
            .collect();
        if normalized.is_empty() {
            return;
        }
        self.input.insert_str(self.input_cursor, &normalized);
        self.input_cursor += normalized.len();
        self.refresh_completion();
    }

    /// 退格：删除光标前一个字符。
    pub(crate) fn input_backspace(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        let prev = self.cursor_prev();
        self.input.remove(prev);
        self.input_cursor = prev;
        self.refresh_completion();
    }

    /// Delete：删除光标处一个字符。
    pub(crate) fn input_delete(&mut self) {
        if self.input_cursor >= self.input.len() {
            return;
        }
        let next = self.cursor_next();
        self.input.drain(self.input_cursor..next);
        self.refresh_completion();
    }

    pub(crate) fn input_move_left(&mut self) {
        self.input_cursor = self.cursor_prev();
        self.clear_completion();
    }

    pub(crate) fn input_move_right(&mut self) {
        self.input_cursor = self.cursor_next();
        self.clear_completion();
    }

    pub(crate) fn input_home(&mut self) {
        self.input_cursor = 0;
        self.clear_completion();
    }

    pub(crate) fn input_end(&mut self) {
        self.input_cursor = self.input.len();
        self.clear_completion();
    }

    /// 删除光标前的一个词（Ctrl+W）。
    pub(crate) fn input_delete_word_backward(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        let s = &self.input[..self.input_cursor];
        let trimmed = s.trim_end();
        let target_len = if let Some(last_space) = trimmed.rfind(' ') {
            last_space + 1
        } else {
            0
        };
        self.input.drain(target_len..self.input_cursor);
        self.input_cursor = target_len;
        self.refresh_completion();
    }

    /// 清除光标到行首的内容（Ctrl+U）。
    pub(crate) fn input_kill_to_start(&mut self) {
        if self.input_cursor == 0 {
            return;
        }
        self.input.drain(..self.input_cursor);
        self.input_cursor = 0;
        self.refresh_completion();
    }

    /// 清除光标到行尾的内容（Ctrl+K）。
    pub(crate) fn input_kill_to_end(&mut self) {
        if self.input_cursor >= self.input.len() {
            return;
        }
        self.input.truncate(self.input_cursor);
        self.refresh_completion();
    }

    /// 清空输入并复位光标。
    pub(crate) fn input_clear(&mut self) {
        self.input.clear();
        self.input_cursor = 0;
        self.clear_completion();
    }

    /// 复位补全候选状态。
    pub(crate) fn clear_completion(&mut self) {
        self.completion_candidates.clear();
        self.completion_index = 0;
        self.completion_range = None;
    }

    /// 实时补全：以光标所在词为准，若以 @/~/* 开头则计算候选并填充。
    /// 仅 Capturing / Tagging / FilteringTag 模式下生效。
    pub(crate) fn refresh_completion(&mut self) {
        self.completion_candidates.clear();
        self.completion_index = 0;
        self.completion_range = None;
        if !matches!(
            self.mode,
            Mode::Capturing | Mode::Tagging | Mode::FilteringTag
        ) {
            return;
        }
        // 光标所在词。
        let head = &self.input[..self.input_cursor];
        let word_start = head
            .char_indices()
            .rev()
            .find(|(_, c)| c.is_whitespace())
            .map(|(i, _)| i + 1)
            .unwrap_or(0);
        let last_word = &head[word_start..];
        if last_word.is_empty() {
            return;
        }
        let (prefix, token) = if let Some(t) = last_word
            .strip_prefix('@')
            .or_else(|| last_word.strip_prefix('＠'))
        {
            ('@', t)
        } else if let Some(t) = last_word
            .strip_prefix('~')
            .or_else(|| last_word.strip_prefix('～'))
            .or_else(|| last_word.strip_prefix('〜'))
        {
            ('~', t)
        } else if let Some(t) = last_word
            .strip_prefix('*')
            .or_else(|| last_word.strip_prefix('＊'))
            .or_else(|| last_word.strip_prefix('×'))
        {
            ('*', t)
        } else if let Some(t) = last_word
            .strip_prefix('!')
            .or_else(|| last_word.strip_prefix('！'))
        {
            ('!', t)
        } else if matches!(self.mode, Mode::Tagging | Mode::FilteringTag) {
            // Tagging/FilteringTag 直接输入裸标签名 → 按标签补全。
            ('@', last_word)
        } else {
            return;
        };
        let candidates = match prefix {
            '@' => self.tag_candidates(token),
            '~' => {
                let base: Vec<String> = crate::tui::keys::time_candidates(self.lang)
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                filter_and_sort_candidates('~', &base, token, self.lang)
            }
            '*' => {
                let base = rrule_candidates_for(token);
                filter_and_sort_candidates('*', &base, token, self.lang)
            }
            '!' => {
                let base: Vec<String> = crate::tui::keys::PRIORITY_CANDIDATES
                    .iter()
                    .map(|s| s.to_string())
                    .collect();
                filter_and_sort_candidates('!', &base, token, self.lang)
            }
            _ => Vec::new(),
        };
        if candidates.is_empty() {
            return;
        }
        self.completion_candidates = candidates;
        self.completion_prefix = prefix;
        self.completion_range = Some((word_start, self.input_cursor));
    }

    /// 已输入 token 的正文（前缀字符之后的部分）。
    fn completion_typed(&self) -> Option<&str> {
        let (start, end) = self.completion_range?;
        let sub = &self.input[start..end];
        let body_start = if let Some((len, c)) = horae_core::parser::first_char_info(sub) {
            if matches!(
                c,
                '@' | '＠' | '~' | '～' | '〜' | '*' | '＊' | '×' | '!' | '！'
            ) {
                start + len
            } else {
                start
            }
        } else {
            start
        };
        Some(&self.input[body_start..end])
    }

    /// 当前候选相对已输入 token 的 ghost 后缀（未插入输入，仅渲染用）。
    pub(crate) fn completion_ghost(&self) -> Option<(char, String, String)> {
        let (_, end) = self.completion_range?;
        if self.input_cursor != end {
            return None; // 光标已离开 token，不再显示 ghost。
        }
        let candidate = self.completion_candidates.get(self.completion_index)?;
        let typed = self.completion_typed()?;
        let ghost = if let Some(stripped) = candidate.strip_prefix(typed) {
            stripped.to_string()
        } else if candidate.to_lowercase().starts_with(&typed.to_lowercase()) {
            candidate
                .chars()
                .skip(typed.chars().count())
                .collect::<String>()
        } else {
            String::new()
        };
        Some((self.completion_prefix, typed.to_string(), ghost))
    }

    /// 把当前候选替换进输入（补齐完整 token），光标推进到词尾。
    pub(crate) fn apply_current_completion(&mut self) {
        let Some((start, _end)) = self.completion_range else {
            return;
        };
        let Some(cand) = self.completion_candidates.get(self.completion_index) else {
            return;
        };
        let prefix_char = self.completion_prefix;
        // 词首是否已含前缀字符（中英文均归一替换为半角前缀）：Tagging 裸标签词首是首字母，不插入前缀。
        let sub = &self.input[start..];
        let has_prefix = horae_core::parser::first_char_info(sub).is_some_and(|(_, c)| {
            matches!(
                c,
                '@' | '＠' | '~' | '～' | '〜' | '*' | '＊' | '×' | '!' | '！'
            )
        });
        let tail = self.input[self.input_cursor..].to_string();
        self.input.truncate(start);
        if has_prefix {
            self.input.push(prefix_char);
        }
        self.input.push_str(cand);
        self.input.push_str(&tail);
        self.input_cursor = start + cand.len() + usize::from(has_prefix);
        self.completion_range = Some((start, self.input_cursor));
    }

    /// 标签候选：预设 + DB 全部标签（按使用频次加权降序）。
    fn tag_candidates(&self, token: &str) -> Vec<String> {
        let default_tags = ["work", "home", "errands", "quick", "focus", "quote"];
        let mut names: Vec<String> = Vec::new();
        if let Ok(db_tags) = horae_core::repo::tags::list_tags_by_frequency(self.conn) {
            for t in db_tags {
                if !names.contains(&t.name) {
                    names.push(t.name);
                }
            }
        }
        for d in default_tags {
            let s = d.to_string();
            if !names.contains(&s) {
                names.push(s);
            }
        }
        filter_and_sort_candidates('@', &names, token, self.lang)
    }
}

/// 动态推导循环候选（支持输入数字如 *2/*3 时动态生成对应间隔的周期候选）。
pub(crate) fn rrule_candidates_for(token: &str) -> Vec<String> {
    let mut list: Vec<String> = crate::tui::keys::RRULE_CANDIDATES
        .iter()
        .map(|s| s.to_string())
        .collect();

    let trimmed = token.trim();
    if let Some(first) = trimmed.chars().next() {
        if first.is_ascii_digit() {
            let num: String = trimmed.chars().take_while(|c| c.is_ascii_digit()).collect();
            list.push(format!("{}d", num));
            list.push(format!("{}w", num));
            list.push(format!("{}m", num));
            list.push(format!("{}y", num));
            list.push(format!("{}w[1,3]", num));
            list.push(format!("{}w[1..5]", num));
        }
    } else if trimmed.starts_with("m[") {
        list.push("m[1,15]".to_string());
        list.push("m[1,-1]".to_string());
        list.push("m[-1]".to_string());
    } else if trimmed.starts_with("w[") {
        list.push("w[1,3]".to_string());
        list.push("w[1..5]".to_string());
    } else if trimmed.starts_with("y[") {
        list.push("y[jan,jul]".to_string());
        list.push("y[1,7]".to_string());
    }

    list
}

/// 计算候选匹配得分（支持前缀匹配、大小写不敏感、中英文拼音首字母/别名映射、模糊子串包含）。
/// 返回 Some(score)（分值越高越优先），不匹配返回 None。
pub(crate) fn candidate_match_score(
    prefix: char,
    candidate: &str,
    token: &str,
    _lang: horae_core::i18n::Lang,
) -> Option<i32> {
    if token.is_empty() {
        return Some(100);
    }
    let cand_lower = candidate.to_lowercase();
    let token_lower = token.to_lowercase();

    // 1. 完全匹配（大小写无关）
    if cand_lower == token_lower {
        return Some(1000);
    }

    // 2. 前缀匹配（大小写无关）
    if cand_lower.starts_with(&token_lower) {
        let diff = (cand_lower.len().saturating_sub(token_lower.len())) as i32;
        return Some(800 - diff.min(300));
    }

    // 3. 语义与拼音/别名映射
    match prefix {
        '~' => {
            let matched_alias = match candidate {
                "周一" => matches!(
                    token_lower.as_str(),
                    "zy" | "zhouyi" | "mon" | "monday" | "1" | "周" | "周一"
                ),
                "周二" => matches!(
                    token_lower.as_str(),
                    "ze" | "zhouer" | "tue" | "tuesday" | "2" | "周" | "周二"
                ),
                "周三" => matches!(
                    token_lower.as_str(),
                    "zs" | "zhousan" | "wed" | "wednesday" | "3" | "周" | "周三"
                ),
                "周四" => matches!(
                    token_lower.as_str(),
                    "zsi" | "zs4" | "zhousi" | "thu" | "thursday" | "4" | "周" | "周四"
                ),
                "周五" => matches!(
                    token_lower.as_str(),
                    "zw" | "zhouwu" | "fri" | "friday" | "5" | "周" | "周五"
                ),
                "周六" => matches!(
                    token_lower.as_str(),
                    "zl" | "zhouliu" | "sat" | "saturday" | "6" | "周" | "周六"
                ),
                "周日" => matches!(
                    token_lower.as_str(),
                    "zr" | "zhouri"
                        | "zt"
                        | "zhoutian"
                        | "sun"
                        | "sunday"
                        | "7"
                        | "0"
                        | "周"
                        | "周日"
                ),
                "周末" => matches!(
                    token_lower.as_str(),
                    "zm" | "zhoumo" | "weekend" | "wknd" | "周" | "周末"
                ),
                "today" => matches!(
                    token_lower.as_str(),
                    "td" | "jt" | "jintian" | "今天" | "今"
                ),
                "tomorrow" => matches!(
                    token_lower.as_str(),
                    "tm" | "tmrw" | "mt" | "mingtian" | "明天" | "明"
                ),
                "今天" => matches!(
                    token_lower.as_str(),
                    "jt" | "jintian" | "td" | "today" | "今" | "今天"
                ),
                "明天" => matches!(
                    token_lower.as_str(),
                    "mt" | "mingtian" | "tm" | "tomorrow" | "明" | "明天"
                ),
                "后天" => matches!(
                    token_lower.as_str(),
                    "ht" | "houtian" | "in2d" | "后" | "后天"
                ),
                "下周一" => matches!(
                    token_lower.as_str(),
                    "xzy" | "xiazhouyi" | "nextmon" | "下" | "下周" | "下周一"
                ),
                "8/20" => matches!(token_lower.as_str(), "8" | "8/" | "8/20" | "0820"),
                "now" => matches!(token_lower.as_str(), "xz" | "xianzai" | "当前" | "现"),
                _ => false,
            };
            if matched_alias {
                return Some(650);
            }
        }
        '!' => {
            let matched_alias = match candidate {
                "high" => matches!(token_lower.as_str(), "h" | "1" | "p1" | "g" | "gao" | "高"),
                "medium" => matches!(
                    token_lower.as_str(),
                    "m" | "med" | "2" | "p2" | "z" | "zhong" | "中"
                ),
                "low" => matches!(token_lower.as_str(), "l" | "3" | "p3" | "d" | "di" | "低"),
                _ => false,
            };
            if matched_alias {
                return Some(650);
            }
        }
        '*' => {
            let matched_alias = match candidate {
                "weekday" => matches!(
                    token_lower.as_str(),
                    "wd" | "workday" | "gzr" | "gongzuori" | "工作日" | "工"
                ),
                "weekend" => matches!(
                    token_lower.as_str(),
                    "we" | "wknd" | "zm" | "zhoumo" | "周末"
                ),
                "m[-1]" => matches!(token_lower.as_str(), "m" | "m[" | "m[-" | "m[-1" | "月末"),
                "2d" => matches!(token_lower.as_str(), "2" | "2d" | "gd" | "geday"),
                _ => false,
            };
            if matched_alias {
                return Some(650);
            }
        }
        _ => {}
    }

    // 4. 子串包含（仅在输入长度 >= 2 时启用，避免单字符误匹配过多项）
    if token.len() >= 2 && cand_lower.contains(&token_lower) {
        return Some(300);
    }

    None
}

/// 过滤并打分排序候选列表。
pub(crate) fn filter_and_sort_candidates(
    prefix: char,
    candidates: &[String],
    token: &str,
    lang: horae_core::i18n::Lang,
) -> Vec<String> {
    let mut scored: Vec<(String, i32, usize)> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    for (idx, c) in candidates.iter().enumerate() {
        if !seen.insert(c.clone()) {
            continue;
        }
        if let Some(score) = candidate_match_score(prefix, c, token, lang) {
            scored.push((c.clone(), score, idx));
        }
    }

    // 按得分降序，同分保持原始出现顺序
    scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.2.cmp(&b.2)));
    scored.into_iter().map(|(c, _, _)| c).collect()
}

/// 自动补全模式：语法参考模式（丰富语义与范式，适合辅助学习）vs 极速补全模式（紧凑极速，适合盲打流）。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum CompletionStyle {
    #[default]
    Reference,
    Speed,
}

impl CompletionStyle {
    pub(crate) fn key(self) -> &'static str {
        match self {
            CompletionStyle::Reference => "reference",
            CompletionStyle::Speed => "speed",
        }
    }

    pub(crate) fn from_key(s: &str) -> Self {
        match s {
            "speed" | "compact" => CompletionStyle::Speed,
            _ => CompletionStyle::Reference,
        }
    }

    pub(crate) fn label(self, lang: horae_core::i18n::Lang) -> &'static str {
        match self {
            CompletionStyle::Reference => lang.tr(
                "补全风格 (语法参考模式 · 启发多样表达)",
                "Completion Style (Reference & Inspiration)",
            ),
            CompletionStyle::Speed => {
                lang.tr("补全风格 (极速补全模式)", "Completion Style (Speed)")
            }
        }
    }
}

/// 补全候选项的元数据：返回 (语义说明, 语法范式提示)。
pub(crate) fn completion_meta(
    prefix: char,
    token: &str,
    lang: horae_core::i18n::Lang,
) -> (String, String) {
    match prefix {
        '*' => match token {
            "d" => (
                lang.tr("每天 (基础日周期)", "Daily").into(),
                lang.tr(
                    "范式: *d · 支持数字间隔 *2d/*3d",
                    "Pattern: *d · numbers *2d/*3d",
                )
                .into(),
            ),
            "w" => (
                lang.tr("每周 (基础周周期)", "Weekly").into(),
                lang.tr(
                    "范式: *w · 支持数字间隔 *2w/*3w",
                    "Pattern: *w · numbers *2w/*3w",
                )
                .into(),
            ),
            "m" => (
                lang.tr("每月 (基础月周期)", "Monthly").into(),
                lang.tr(
                    "范式: *m · 支持数字间隔 *2m/*3m",
                    "Pattern: *m · numbers *2m/*3m",
                )
                .into(),
            ),
            "y" => (
                lang.tr("每年 (基础年周期)", "Yearly").into(),
                lang.tr("范式: *y · 支持数字间隔 *2y", "Pattern: *y · numbers *2y")
                    .into(),
            ),
            "weekday" => (
                lang.tr("工作日 (周一至周五)", "Weekdays (Mon-Fri)").into(),
                lang.tr("别名: *workday · 拼音 *gzr", "Alias: *workday")
                    .into(),
            ),
            "weekend" => (
                lang.tr("周末 (周六日)", "Weekend (Sat-Sun)").into(),
                lang.tr("双休日 · 拼音 *zm", "Sat & Sun").into(),
            ),
            "2w[1,3]" => (
                lang.tr("每两周 (周一、周三)", "Every 2 weeks (Mon, Wed)")
                    .into(),
                lang.tr("范式: *Nw[1..7] (1-7=周一至日)", "Pattern: *Nw[1..7]")
                    .into(),
            ),
            "m[-1]" => (
                lang.tr("月末最后一天", "Last day of month").into(),
                lang.tr(
                    "范式: *m[-1] (负数表示月末倒数)",
                    "Pattern: *m[-1] (count backwards)",
                )
                .into(),
            ),
            "m[1,-1]" => (
                lang.tr("每月 1 号与月末", "Monthly 1st & last day").into(),
                lang.tr("范式: *Nm[1..31,-1] (月首与月末)", "Pattern: *Nm[1..31,-1]")
                    .into(),
            ),
            "y[jan,jul]" => (
                lang.tr("每年 1 月与 7 月", "Yearly Jan & Jul").into(),
                lang.tr(
                    "范式: *Ny[1..12/月名] (按月循环)",
                    "Pattern: *Ny[1..12/name]",
                )
                .into(),
            ),
            "1w[mo,we]" => (
                lang.tr("每周 (周一、周三)", "Weekly (Mon, Wed)").into(),
                lang.tr(
                    "英文星期代码 (mo/tu/we/th/fr/sa/su)",
                    "Weekday codes (mo..su)",
                )
                .into(),
            ),
            "m[1,2,-2,-1]" => (
                lang.tr("月初及月末各两日", "1st,2nd & last 2 days").into(),
                lang.tr(
                    "范式: 多日组合 [1,2,-2,-1]",
                    "Pattern: multi-day [1,2,-2,-1]",
                )
                .into(),
            ),
            "2d" => (
                lang.tr("每两日 (隔日循环)", "Every 2 days").into(),
                lang.tr(
                    "范式: *Nd (动态数字推导)",
                    "Pattern: *Nd (dynamic interval)",
                )
                .into(),
            ),
            _ => (
                horae_core::parser::parse_rrule_shorthand(token),
                lang.tr("循环规则", "Recurrence").into(),
            ),
        },
        '~' => match token {
            "today" => (
                lang.tr("今天 (今日排程)", "Today").into(),
                lang.tr(
                    "可接时刻: ~today 18:00 · 拼音 ~jt/~td",
                    "With time: ~today 18:00 · alias ~td",
                )
                .into(),
            ),
            "tomorrow" => (
                lang.tr("明天 (次日排程)", "Tomorrow").into(),
                lang.tr(
                    "可接时刻: ~tomorrow 10:00 · 拼音 ~mt/~tm",
                    "With time: ~tomorrow 10:00 · alias ~tm",
                )
                .into(),
            ),
            "今天" => (
                lang.tr("今天 (自然语言天词)", "Today").into(),
                lang.tr(
                    "复合范式: 可接时刻 ~今天 18:00 · 拼音 ~jt",
                    "With time: ~today 18:00",
                )
                .into(),
            ),
            "明天" => (
                lang.tr("明天 (自然语言天词)", "Tomorrow").into(),
                lang.tr(
                    "复合范式: 可接时刻 ~明天 10:00 · 拼音 ~mt",
                    "With time: ~tomorrow 10:00",
                )
                .into(),
            ),
            "后天" => (
                lang.tr("后天 (两日后)", "In 2 days").into(),
                lang.tr(
                    "两日后排程 · 可接时刻 ~后天 09:00 · 拼音 ~ht",
                    "Due in 2 days · with time",
                )
                .into(),
            ),
            "now" => (
                lang.tr("当前时刻 (即刻排程)", "Right now").into(),
                lang.tr(
                    "设为起点: 立即进入 Scheduled 状态",
                    "Set start time (enters Scheduled)",
                )
                .into(),
            ),
            "+1h" => (
                lang.tr("1 小时后", "In 1 hour").into(),
                lang.tr(
                    "相对小时: +Nh (+2h, +3h...) / +Nm (+30m)",
                    "Relative: +Nh / +Nm",
                )
                .into(),
            ),
            "+2h" => (
                lang.tr("2 小时后", "In 2 hours").into(),
                lang.tr("相对小时: +Nh", "Relative: +Nh").into(),
            ),
            "+3h" => (
                lang.tr("3 小时后", "In 3 hours").into(),
                lang.tr("相对小时: +Nh", "Relative: +Nh").into(),
            ),
            "+4h" => (
                lang.tr("4 小时后", "In 4 hours").into(),
                lang.tr("相对小时: +Nh", "Relative: +Nh").into(),
            ),
            "+1d" => (
                lang.tr("1 天后 (明日排程)", "In 1 day").into(),
                lang.tr(
                    "相对天数: +Nd · 可接时刻 ~+1d 09:00",
                    "Relative: +Nd · with time",
                )
                .into(),
            ),
            "+2d" => (
                lang.tr("2 天后", "In 2 days").into(),
                lang.tr("相对天数: +Nd", "Relative: +Nd").into(),
            ),
            "+3d" => (
                lang.tr("3 天后", "In 3 days").into(),
                lang.tr("相对天数: +Nd", "Relative: +Nd").into(),
            ),
            "+1w" => (
                lang.tr("1 周后", "In 1 week").into(),
                lang.tr(
                    "相对周数: +Nw · 可接时刻 ~+1w 10:00",
                    "Relative: +Nw · with time",
                )
                .into(),
            ),
            "+15m" => (
                lang.tr("15 分钟后", "In 15 mins").into(),
                lang.tr("相对分钟: +Nm (+30m, +45m...)", "Relative: +Nm")
                    .into(),
            ),
            "+30m" => (
                lang.tr("30 分钟后", "In 30 mins").into(),
                lang.tr("相对分钟: +Nm", "Relative: +Nm").into(),
            ),
            "周五" => (
                lang.tr("本周五 / 下周五", "This/Next Friday").into(),
                lang.tr(
                    "星期词汇: 周一~周日 · 拼音 ~zw · 英文 ~fri",
                    "Weekday: Mon-Sun · alias ~fri",
                )
                .into(),
            ),
            "下周一" => (
                lang.tr("下周一 (下周首日)", "Next Monday").into(),
                lang.tr(
                    "跨周范式: 下周X (支持 下周一~下周日)",
                    "Next week: next Mon-Sun",
                )
                .into(),
            ),
            "周一" => (
                lang.tr("本周一 / 下周一", "This/Next Monday").into(),
                lang.tr("星期词: 周一~周日 · 拼音 ~zy", "Weekday: Mon-Sun")
                    .into(),
            ),
            "周二" => (
                lang.tr("本周二 / 下周二", "This/Next Tuesday").into(),
                lang.tr("星期词: 周一~周日 · 拼音 ~ze", "Weekday: Mon-Sun")
                    .into(),
            ),
            "周三" => (
                lang.tr("本周三 / 下周三", "This/Next Wednesday").into(),
                lang.tr("星期词: 周一~周日 · 拼音 ~zs", "Weekday: Mon-Sun")
                    .into(),
            ),
            "周四" => (
                lang.tr("本周四 / 下周四", "This/Next Thursday").into(),
                lang.tr("星期词: 周一~周日 · 拼音 ~zsi", "Weekday: Mon-Sun")
                    .into(),
            ),
            "周六" => (
                lang.tr("本周六 / 下周六", "This/Next Saturday").into(),
                lang.tr("星期词: 周一~周日 · 拼音 ~zl", "Weekday: Mon-Sun")
                    .into(),
            ),
            "周日" => (
                lang.tr("本周日 / 下周日", "This/Next Sunday").into(),
                lang.tr("星期词: 周一~周日 · 拼音 ~zr/~zt", "Weekday: Mon-Sun")
                    .into(),
            ),
            "周末" => (
                lang.tr("周末 (周六)", "Weekend (Sat)").into(),
                lang.tr("本周六零点 · 拼音 ~zm", "Upcoming Saturday").into(),
            ),
            "18:00" => (
                lang.tr("下午 6 点 (下班截止)", "06:00 PM (End of day)")
                    .into(),
                lang.tr(
                    "当日时刻: HH:MM (若已过则自动顺延至明日)",
                    "Same-day clock (next day if passed)",
                )
                .into(),
            ),
            "09:00" => (
                lang.tr("上午 9 点 (工作起点)", "09:00 AM (Work start)")
                    .into(),
                lang.tr(
                    "当日时刻: HH:MM (晨间工作起点)",
                    "Same-day clock (work start)",
                )
                .into(),
            ),
            "8/20" => (
                lang.tr("8 月 20 日 (指定月日)", "Aug 20 (Flexible date)")
                    .into(),
                lang.tr(
                    "日历范式: M/D · YYYY-MM-DD · 可接时刻",
                    "Calendar: M/D · YYYY-MM-DD · with time",
                )
                .into(),
            ),
            "mon" => (
                lang.tr("本周一 / 下周一", "This/Next Monday").into(),
                lang.tr("星期词: mon~sun", "Weekday: mon~sun").into(),
            ),
            "tue" => (
                lang.tr("本周二 / 下周二", "This/Next Tuesday").into(),
                lang.tr("星期词: mon~sun", "Weekday: mon~sun").into(),
            ),
            "wed" => (
                lang.tr("本周三 / 下周三", "This/Next Wednesday").into(),
                lang.tr("星期词: mon~sun", "Weekday: mon~sun").into(),
            ),
            "thu" => (
                lang.tr("本周四 / 下周四", "This/Next Thursday").into(),
                lang.tr("星期词: mon~sun", "Weekday: mon~sun").into(),
            ),
            "fri" => (
                lang.tr("本周五 / 下周五", "This/Next Friday").into(),
                lang.tr("星期词: mon~sun", "Weekday: mon~sun").into(),
            ),
            "sat" => (
                lang.tr("本周六 / 下周六", "This/Next Saturday").into(),
                lang.tr("星期词: mon~sun", "Weekday: mon~sun").into(),
            ),
            "sun" => (
                lang.tr("本周日 / 下周日", "This/Next Sunday").into(),
                lang.tr("星期词: mon~sun", "Weekday: mon~sun").into(),
            ),
            _ => (
                horae_core::time::parse_time(token)
                    .ok()
                    .map(|ms| horae_core::time::format_local(Some(ms)))
                    .unwrap_or_default(),
                lang.tr("时间格式", "Time format").into(),
            ),
        },
        '!' => match token {
            "high" => (
                lang.tr("高优先级 (统治级 +10000)", "High priority (+10000)")
                    .into(),
                lang.tr(
                    "启发别名: !1 · !h · !p1 · !高",
                    "Aliases: !1 · !h · !p1 · !high",
                )
                .into(),
            ),
            "medium" => (
                lang.tr("中优先级 (重要 +5000)", "Medium priority (+5000)")
                    .into(),
                lang.tr(
                    "启发别名: !2 · !med · !p2 · !中",
                    "Aliases: !2 · !med · !p2 · !med",
                )
                .into(),
            ),
            "low" => (
                lang.tr("低优先级 (次要 +1000)", "Low priority (+1000)")
                    .into(),
                lang.tr(
                    "启发别名: !3 · !l · !p3 · !低",
                    "Aliases: !3 · !l · !p3 · !low",
                )
                .into(),
            ),
            _ => (String::new(), String::new()),
        },
        '@' => match token {
            "work" => (
                lang.tr("工作 / 职场情境", "Work / Professional").into(),
                lang.tr("GTD情境: 办公室 / 业务开发", "GTD Context: work & office")
                    .into(),
            ),
            "home" => (
                lang.tr("家庭生活 / 个人", "Home / Personal").into(),
                lang.tr(
                    "GTD情境: 私人生活 / 家居杂务",
                    "GTD Context: personal & home",
                )
                .into(),
            ),
            "errands" => (
                lang.tr("外出跑腿 / 办事", "Errands / Outdoors").into(),
                lang.tr(
                    "GTD情境: 采购 / 外勤 / 出门办事",
                    "GTD Context: outdoor errands",
                )
                .into(),
            ),
            "quick" => (
                lang.tr("5分钟快速清小事", "Quick task (<5m)").into(),
                lang.tr(
                    "GTD情境: 碎片时间极速清空",
                    "GTD Context: fast small actions",
                )
                .into(),
            ),
            "focus" => (
                lang.tr("整块深度专注时间", "Deep Focus").into(),
                lang.tr("GTD情境: 高心智深度攻坚", "GTD Context: deep focus chunks")
                    .into(),
            ),
            "quote" => (
                lang.tr("灵感金句 (自动归档)", "Quote inspiration").into(),
                lang.tr(
                    "知识库标签: 自动归档至灵感看板",
                    "Quote tag: auto-archived to Quotes",
                )
                .into(),
            ),
            _ => (
                lang.tr("标签", "Tag").into(),
                lang.tr(
                    "自定义标签首次使用自动建档",
                    "New tag auto-created on first use",
                )
                .into(),
            ),
        },
        _ => (String::new(), String::new()),
    }
}
