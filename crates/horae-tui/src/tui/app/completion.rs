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
            '~' => crate::tui::keys::time_candidates(self.lang)
                .iter()
                .filter(|c| c.starts_with(token))
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            '*' => crate::tui::keys::RRULE_CANDIDATES
                .iter()
                .filter(|c| c.starts_with(token))
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            '!' => crate::tui::keys::PRIORITY_CANDIDATES
                .iter()
                .filter(|c| c.starts_with(token))
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
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
        let ghost = candidate
            .strip_prefix(typed)
            .map(|s| s.to_string())
            .unwrap_or_default();
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

    /// 标签候选：预设 + DB 全部标签（含自定义）。
    fn tag_candidates(&self, token: &str) -> Vec<String> {
        let default_tags = ["home", "work", "errands", "quick", "focus"];
        let mut names: Vec<String> = default_tags.iter().map(|s| s.to_string()).collect();
        if let Ok(db_tags) = horae_core::repo::tags::list_tags(self.conn) {
            for t in db_tags {
                if !names.contains(&t.name) {
                    names.push(t.name);
                }
            }
        }
        // 前缀精确匹配的候选排最前（先按前缀，再按预设优先）。
        names.sort_by_key(|n| !n.starts_with(token));
        names.into_iter().filter(|n| n.starts_with(token)).collect()
    }
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
            CompletionStyle::Reference => {
                lang.tr("补全风格 (语法参考模式)", "Completion (Cheat-Sheet Guide)")
            }
            CompletionStyle::Speed => {
                lang.tr("补全风格 (极速补全模式)", "Completion (Speed & Compact)")
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
                lang.tr("每天", "Daily").into(),
                lang.tr("基础周期", "Basic period").into(),
            ),
            "w" => (
                lang.tr("每周", "Weekly").into(),
                lang.tr("基础周期", "Basic period").into(),
            ),
            "m" => (
                lang.tr("每月", "Monthly").into(),
                lang.tr("基础周期", "Basic period").into(),
            ),
            "y" => (
                lang.tr("每年", "Yearly").into(),
                lang.tr("基础周期", "Basic period").into(),
            ),
            "weekday" => (
                lang.tr("工作日 (周一至周五)", "Weekdays (Mon-Fri)").into(),
                lang.tr("别名: workday", "Alias: workday").into(),
            ),
            "weekend" => (
                lang.tr("周末 (周六日)", "Weekend (Sat-Sun)").into(),
                lang.tr("周六与周日", "Sat & Sun").into(),
            ),
            "2w[1,3]" => (
                lang.tr("每两周 (周一、周三)", "Every 2 weeks (Mon, Wed)")
                    .into(),
                lang.tr("范式: *Nw[1..7]", "Pattern: *Nw[1..7]").into(),
            ),
            "m[1,2,-2,-1]" => (
                lang.tr("月初及月末各两日", "1st,2nd & last 2 days").into(),
                lang.tr("范式: *Nm[1..31,-1]", "Pattern: *Nm[1..31,-1]")
                    .into(),
            ),
            "m[1,-1]" => (
                lang.tr("每月 1 号与月末", "Monthly 1st & last day").into(),
                lang.tr("范式: *Nm[1..31,-1]", "Pattern: *Nm[1..31,-1]")
                    .into(),
            ),
            "y[jan,jul]" => (
                lang.tr("每年 1 月与 7 月", "Yearly Jan & Jul").into(),
                lang.tr("范式: *Ny[1..12/月名]", "Pattern: *Ny[1..12/name]")
                    .into(),
            ),
            "1w[mo,we]" => (
                lang.tr("每周 (周一、周三)", "Weekly (Mon, Wed)").into(),
                lang.tr("英文星期代码", "English weekday codes").into(),
            ),
            _ => (
                horae_core::parser::parse_rrule_shorthand(token),
                lang.tr("循环规则", "Recurrence").into(),
            ),
        },
        '~' => match token {
            "today" => (
                lang.tr("今天", "Today").into(),
                lang.tr("可接时刻: ~today 18:00", "With time: ~today 18:00")
                    .into(),
            ),
            "tomorrow" => (
                lang.tr("明天", "Tomorrow").into(),
                lang.tr("可接时刻: ~tomorrow 10:00", "With time: ~tomorrow 10:00")
                    .into(),
            ),
            "now" => (
                lang.tr("当前时刻", "Right now").into(),
                lang.tr("设为排程起点", "Set start time").into(),
            ),
            "后天" => (
                lang.tr("后天 (两日后)", "In 2 days").into(),
                lang.tr("两日后截止", "Due in 2 days").into(),
            ),
            "+1h" => (
                lang.tr("1 小时后", "In 1 hour").into(),
                lang.tr("相对小时: +Nh", "Relative: +Nh").into(),
            ),
            "+2h" => (
                lang.tr("2 小时后", "In 2 hours").into(),
                lang.tr("相对小时: +Nh", "Relative: +Nh").into(),
            ),
            "+3h" => (
                lang.tr("3 小时后", "In 3 hours").into(),
                lang.tr("相对小时: +Nh", "Relative: +Nh").into(),
            ),
            "+1d" => (
                lang.tr("1 天后", "In 1 day").into(),
                lang.tr("相对天数: +Nd", "Relative: +Nd").into(),
            ),
            "+2d" => (
                lang.tr("2 天后", "In 2 days").into(),
                lang.tr("相对天数: +Nd", "Relative: +Nd").into(),
            ),
            "+1w" => (
                lang.tr("1 周后", "In 1 week").into(),
                lang.tr("相对周数: +Nw", "Relative: +Nw").into(),
            ),
            "周一" => (
                lang.tr("本周一 / 下周一", "This/Next Monday").into(),
                lang.tr("星期词: 周一~周日", "Weekday: Mon-Sun").into(),
            ),
            "周二" => (
                lang.tr("本周二 / 下周二", "This/Next Tuesday").into(),
                lang.tr("星期词: 周一~周日", "Weekday: Mon-Sun").into(),
            ),
            "周三" => (
                lang.tr("本周三 / 下周三", "This/Next Wednesday").into(),
                lang.tr("星期词: 周一~周日", "Weekday: Mon-Sun").into(),
            ),
            "周四" => (
                lang.tr("本周四 / 下周四", "This/Next Thursday").into(),
                lang.tr("星期词: 周一~周日", "Weekday: Mon-Sun").into(),
            ),
            "周五" => (
                lang.tr("本周五 / 下周五", "This/Next Friday").into(),
                lang.tr("星期词: 周一~周日", "Weekday: Mon-Sun").into(),
            ),
            "周六" => (
                lang.tr("本周六 / 下周六", "This/Next Saturday").into(),
                lang.tr("星期词: 周一~周日", "Weekday: Mon-Sun").into(),
            ),
            "周日" => (
                lang.tr("本周日 / 下周日", "This/Next Sunday").into(),
                lang.tr("星期词: 周一~周日", "Weekday: Mon-Sun").into(),
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
                lang.tr("高优先级", "High priority").into(),
                lang.tr("统治级 (+10000 权重)", "Top priority (+10000 weight)")
                    .into(),
            ),
            "medium" => (
                lang.tr("中优先级", "Medium priority").into(),
                lang.tr("重要 (+5000 权重)", "Medium priority (+5000 weight)")
                    .into(),
            ),
            "low" => (
                lang.tr("低优先级", "Low priority").into(),
                lang.tr("次要 (+1000 权重)", "Low priority (+1000 weight)")
                    .into(),
            ),
            _ => (String::new(), String::new()),
        },
        '@' => match token {
            "home" => (
                lang.tr("家庭生活 / 个人", "Home / Personal").into(),
                lang.tr("情境标签", "Context tag").into(),
            ),
            "work" => (
                lang.tr("工作 / 职场", "Work / Professional").into(),
                lang.tr("情境标签", "Context tag").into(),
            ),
            "errands" => (
                lang.tr("外出跑腿 / 办事", "Errands / Outdoors").into(),
                lang.tr("情境标签", "Context tag").into(),
            ),
            "quick" => (
                lang.tr("5分钟快速清小事", "Quick task (<5m)").into(),
                lang.tr("情境标签", "Context tag").into(),
            ),
            "focus" => (
                lang.tr("整块深度专注时间", "Deep Focus").into(),
                lang.tr("情境标签", "Context tag").into(),
            ),
            "quote" => (
                lang.tr("灵感金句 (自动归档)", "Quote inspiration").into(),
                lang.tr("金句标签", "Quote tag").into(),
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
