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
        self.refresh_completion();
    }

    pub(crate) fn input_move_right(&mut self) {
        self.input_cursor = self.cursor_next();
        self.refresh_completion();
    }

    pub(crate) fn input_home(&mut self) {
        self.input_cursor = 0;
        self.refresh_completion();
    }

    pub(crate) fn input_end(&mut self) {
        self.input_cursor = self.input.len();
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
        } else if matches!(self.mode, Mode::Tagging | Mode::FilteringTag) {
            // Tagging/FilteringTag 直接输入裸标签名 → 按标签补全。
            ('@', last_word)
        } else {
            return;
        };
        if token.is_empty() {
            return;
        }
        let candidates = match prefix {
            '@' => self.tag_candidates(token),
            '~' => crate::tui::keys::TIME_CANDIDATES
                .iter()
                .filter(|c| c.starts_with(token))
                .map(|s| s.to_string())
                .collect::<Vec<_>>(),
            '*' => crate::tui::keys::RRULE_CANDIDATES
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
            if matches!(c, '@' | '＠' | '~' | '～' | '〜' | '*' | '＊' | '×') {
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
        let has_prefix = horae_core::parser::first_char_info(sub)
            .is_some_and(|(_, c)| matches!(c, '@' | '＠' | '~' | '～' | '〜' | '*' | '＊' | '×'));
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
