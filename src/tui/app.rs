use anyhow::Result;
use ratatui::widgets::ListState;
use rusqlite::Connection;

use super::row_from_tags_with_due;
use crate::model::event::TaskEvent;
use crate::model::pomodoro::PomoState;
use crate::model::tag::Tag;
use crate::model::task::{self, Task};
use crate::repo::tags;
use crate::repo::tasks::{self, ListFilter};

pub(crate) fn visual_len(s: &str) -> usize {
    s.chars()
        .map(|c| {
            if c.is_ascii() || ('\u{E000}'..='\u{F8FF}').contains(&c) {
                1
            } else {
                2
            }
        })
        .sum()
}

pub(crate) fn pad_right(s: &str, width: usize) -> String {
    let len = visual_len(s);
    if len < width {
        format!("{}{}", s, " ".repeat(width - len))
    } else {
        s.to_string()
    }
}

/// 检查可执行文件是否存在于 PATH 中（不 spawn 进程，避免探测副作用）。
fn command_in_path(name: &str) -> bool {
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    std::env::split_paths(&path).any(|dir| dir.join(name).is_file())
}

/// GTD 的七个状态（数据层不变）。界面里只有 Inbox 和 Next 是“主视图”，
/// 其余状态作为可折叠的“上下文分组”放在左侧引导栏，既保持可达，
/// 又不会把前台铺得太满造成心理负担。
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub(crate) enum View {
    Inbox,
    Today,
    Tomorrow,
    Next,
    Waiting,
    Scheduled,
    Someday,
    Reference,
    Done,
    Review,
    Archived,
    Tags,
    Quotes,
    Settings,
}

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

impl View {
    /// 状态视图对应的状态字符串（用于查询与中文展示）。
    pub(crate) fn status(self) -> Option<&'static str> {
        match self {
            View::Inbox => Some("inbox"),
            View::Next => Some("next"),
            View::Waiting => Some("waiting"),
            View::Scheduled => Some("scheduled"),
            View::Someday => Some("someday"),
            View::Reference => Some("reference"),
            View::Done => Some("done"),
            View::Today
            | View::Tomorrow
            | View::Review
            | View::Archived
            | View::Tags
            | View::Quotes
            | View::Settings => None,
        }
    }

    /// 数字键 1-9 映射到的视图（0 = 金句，仅在功能启用时可用）。
    pub(crate) fn from_digit(d: char) -> Option<View> {
        match d {
            '1' => Some(View::Inbox),
            '2' => Some(View::Next),
            '3' => Some(View::Waiting),
            '4' => Some(View::Scheduled),
            '5' => Some(View::Someday),
            '6' => Some(View::Reference),
            '7' => Some(View::Done),
            '8' => Some(View::Archived),
            '9' => Some(View::Tags),
            '0' => Some(View::Quotes),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Mode {
    Normal,
    Capturing,
    Tagging,
    WaitingWho,
    WaitingWhen,
    Search,
    ChecklistAdding,
    Visual,
    FilteringTag,
    /// 归档前确认：收集待归档的 id，等待 y/Enter 确认或 n/Esc 取消。
    ConfirmArchive,
    /// 永久删除确认：等待 y/Enter 确认或 n/Esc 取消。
    ConfirmPurge,
    /// 新增自定义标签
    CreatingTag,
    /// 配置番茄钟时长 (工作;短休;长休)
    ConfiguringPomo,
    /// 新建 profile（输入名称）
    CreatingProfile,
    /// 重命名 profile（输入新名称）
    RenamingProfile,
    /// 删除 profile 确认：等待 y/Enter 确认或 n/Esc 取消。
    ConfirmProfileDelete,
}

impl Mode {
    pub(crate) fn is_input(&self) -> bool {
        !matches!(self, Mode::Normal | Mode::Visual)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Pane {
    Left,
    Center,
    Right,
}

#[derive(Clone, PartialEq, Eq)]
pub(crate) enum Popup {
    /// Show today's tasks summary on startup
    TodayTasks(Vec<String>),
    /// Prompt to enter Pomodoro mode for a scheduled task
    TaskDueNow(String, String), // task_id, task_title
    /// Feature toggles modal (current selected index)
    ModuleToggles(usize),
}

#[derive(Clone)]
pub(crate) struct Row {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) status: String,
    pub(crate) due: Option<i64>,
    pub(crate) tags: Vec<String>,
    pub(crate) indent: usize,
    /// 完成进度（用于项目/带检查单的任务）：已完成数，None 表示无进度概念。
    pub(crate) done: Option<usize>,
    /// 完成进度：总数。
    pub(crate) total: Option<usize>,
    /// 归档原因（仅归档箱视图非空）：completed | deleted。
    pub(crate) archive_reason: Option<String>,
    /// 循环任务今日是否已打卡（存在今日的 habit_completed 事件）。
    pub(crate) checked_in_today: bool,
}

pub(crate) struct DetailData {
    pub(crate) task: Task,
    pub(crate) tags: Vec<Tag>,
    pub(crate) events: Vec<TaskEvent>,
}

pub(crate) struct App<'a> {
    pub(crate) conn: &'a Connection,
    pub(crate) view: View,
    pub(crate) items: Vec<Row>,
    pub(crate) selected: usize,
    pub(crate) list_state: ListState,
    pub(crate) detail: Option<DetailData>,
    pub(crate) mode: Mode,
    pub(crate) pane: Pane,
    pub(crate) input: String,
    /// 输入光标位置（`input` 的字节偏移，始终落在字符边界）。
    pub(crate) input_cursor: usize,
    /// 组织/编辑模式正在编辑的任务 id。
    pub(crate) organizing_id: Option<String>,
    pub(crate) status_message: String,
    pub(crate) lang: crate::i18n::Lang,
    pub(crate) show_help: bool,
    pub(crate) show_syntax: bool,
    pub(crate) show_shortcut_bar: bool,
    pub(crate) help_scroll: usize,
    pub(crate) should_quit: bool,
    pub(crate) search_query: String,
    pub(crate) tag_filter: Option<String>,
    pub(crate) visual_start_idx: Option<usize>,
    pub(crate) selected_ids: std::collections::HashSet<String>,
    pub(crate) is_reviewing: bool,
    pub(crate) review_step: u8,
    pub(crate) needs_clear: bool,
    pub(crate) pending_archive_ids: Vec<String>,
    pub(crate) pending_purge_ids: Vec<String>,
    pub(crate) hide_pomo_banner: bool,
    pub(crate) theme: crate::tui::theme::Theme,
    pub(crate) popup: Option<Popup>,
    pub(crate) notification_engine: crate::notification::NotificationEngine,
    /// 上次执行每日摘要检查的时间戳（毫秒），用于 60s 节流（Bug4）。
    pub(crate) last_notify_check_ms: i64,
    /// 各视图计数缓存：`refresh` 时一次性算好，渲染帧内零 DB 查询。
    pub(crate) counts: std::collections::HashMap<View, usize>,
    /// 循环任务展开结果缓存（task_id -> 发生序列）：一次刷新内每个循环规则只
    /// 展开一次，列表行与今日/明日视图复用，避免重复展开。
    pub(crate) rrule_cache: std::collections::HashMap<String, Vec<i64>>,
    /// 番茄钟状态缓存：每帧渲染只读这一份快照，避免每帧多次读 `pomo.json`。
    pub(crate) pomo: PomoState,
    /// 上次读取番茄状态的时间戳（毫秒），用于按 TTL 限频重读。
    pub(crate) pomo_loaded_ms: i64,
    /// 当前补全候选列表（Tab 多候选循环用）。
    pub(crate) completion_candidates: Vec<String>,
    /// 当前高亮的候选下标。
    pub(crate) completion_index: usize,
    /// 被补全 token 的字节区间 `[start, end)`（含 `@`/`~`/`*` 前缀）。
    pub(crate) completion_range: Option<(usize, usize)>,
    /// 被补全 token 的前缀字符（`@`/`~`/`*`；Tagging 裸标签也视为 `@`）。
    pub(crate) completion_prefix: char,
    /// 金句 (Quotes) 特性门控与查询。
    pub(crate) quotes: crate::repo::quotes::Quotes,
    /// 模块显示门控 (Splash, Reference, Done, etc.)
    pub(crate) modules: crate::repo::modules::ModuleVisibility,
    /// 当前会话使用的 profile 名（用于设置页标记与显示）。
    pub(crate) profile_name: String,
    /// 设置页待删除的 profile 名。
    pub(crate) pending_profile_delete: Option<String>,
}

impl<'a> App<'a> {
    pub(crate) fn new(conn: &'a Connection) -> Result<Self> {
        // 从 settings 表恢复语言与主题。
        let lang = match crate::repo::settings::get(conn, "lang")
            .ok()
            .flatten()
            .as_deref()
        {
            Some("en") => crate::i18n::Lang::En,
            _ => crate::i18n::Lang::Zh,
        };
        let theme = match crate::repo::settings::get(conn, "theme")
            .ok()
            .flatten()
            .as_deref()
        {
            Some("latte") => crate::tui::theme::Theme::catppuccin_latte(),
            _ => crate::tui::theme::Theme::catppuccin_mocha(),
        };
        let quotes = crate::repo::quotes::Quotes::load(conn);
        let modules = crate::repo::modules::ModuleVisibility::load(conn);
        let mut app = App {
            conn,
            view: View::Inbox,
            items: Vec::new(),
            selected: 0,
            list_state: ListState::default(),
            detail: None,
            mode: Mode::Normal,
            pane: Pane::Left,
            input: String::new(),
            input_cursor: 0,
            organizing_id: None,
            status_message: crate::tr!(lang, "按 '?' 或 F1 查看帮助", "Press '?' or F1 for help")
                .to_string(),
            lang,
            show_help: false,
            show_syntax: false,
            show_shortcut_bar: true,
            help_scroll: 0,
            should_quit: false,
            search_query: String::new(),
            tag_filter: None,
            visual_start_idx: None,
            selected_ids: std::collections::HashSet::new(),
            is_reviewing: false,
            review_step: 0,
            needs_clear: false,
            pending_archive_ids: Vec::new(),
            pending_purge_ids: Vec::new(),
            hide_pomo_banner: false,
            theme,
            popup: None,
            notification_engine: crate::notification::NotificationEngine::new(),
            last_notify_check_ms: 0,
            counts: std::collections::HashMap::new(),
            rrule_cache: std::collections::HashMap::new(),
            pomo: crate::repo::pomodoro::get_state().unwrap_or_default(),
            pomo_loaded_ms: 0,
            completion_candidates: Vec::new(),
            completion_index: 0,
            completion_range: None,
            completion_prefix: '@',
            quotes,
            modules,
            profile_name: String::new(),
            pending_profile_delete: None,
        };
        app.refresh()?;

        // --- Startup Today Tasks Popup ---
        let all_tasks = tasks::list(
            conn,
            &ListFilter {
                status: None,
                tags: vec![],
                query: None,
                review_stale: false,
            },
        )
        .unwrap_or_default();

        let today_start = chrono::Local::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc()
            .timestamp();
        let today_end = chrono::Local::now()
            .date_naive()
            .and_hms_opt(23, 59, 59)
            .unwrap()
            .and_utc()
            .timestamp();

        let mut todays = Vec::new();
        for t in &all_tasks {
            if let Some(due) = t.due_at {
                if due >= today_start && due <= today_end {
                    todays.push(t.title.clone());
                }
            }
        }

        if !todays.is_empty() {
            app.popup = Some(Popup::TodayTasks(todays));
        }

        app.load_detail();
        app.switch_to_english_ime();
        Ok(app)
    }

    pub(crate) fn check_notifications(&mut self) {
        let now_ms = crate::time::now_ms();
        // Bug4 修复：每日摘要检查每 60s 至多一次，避免每帧都读 notify.json。
        if now_ms - self.last_notify_check_ms >= 60_000 {
            let _ = crate::commands::notify::check(self.conn);
            self.last_notify_check_ms = now_ms;
        }

        let events = self.notification_engine.tick(self.conn);
        for event in events {
            match event {
                // Bug5 修复：InOneHour/InTenMins 只有 title 一个字段，去掉多余的 ..
                crate::notification::NotificationEvent::InOneHour { title } => {
                    crate::commands::notify::desktop("任务即将在1小时后开始", &title);
                }
                crate::notification::NotificationEvent::InTenMins { title } => {
                    crate::commands::notify::desktop("任务即将在10分钟后开始", &title);
                }
                crate::notification::NotificationEvent::Now { id, title } => {
                    crate::commands::notify::desktop("任务现在开始!", &title);
                    self.popup = Some(Popup::TaskDueNow(id, title));
                    self.needs_clear = true; // force redraw to show popup
                }
            }
        }
    }

    /// 按需刷新番茄钟状态快照（每 ~500ms 至多重读一次 `pomo.json`），供渲染帧内
    /// 复用，避免每帧多次磁盘读 + JSON 解析。
    pub(crate) fn refresh_pomo(&mut self) {
        let now = crate::time::now_ms();
        if now - self.pomo_loaded_ms < 500 {
            return;
        }
        self.pomo_loaded_ms = now;
        self.pomo = crate::repo::pomodoro::get_state().unwrap_or_default();
    }

    pub(crate) fn set_mode(&mut self, new_mode: Mode) {
        let old_mode = self.mode;
        self.mode = new_mode;
        if old_mode.is_input() && !new_mode.is_input() {
            self.switch_to_english_ime();
        }
        // 进入输入模式时，把光标定位到末尾（预填内容后默认在尾部继续输入）。
        if !old_mode.is_input() && new_mode.is_input() {
            self.input_cursor = self.input.len();
        }
    }

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
        let (prefix, token) = if let Some(t) = last_word.strip_prefix('@') {
            ('@', t)
        } else if let Some(t) = last_word.strip_prefix('~') {
            ('~', t)
        } else if let Some(t) = last_word.strip_prefix('*') {
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
        let bytes = self.input.as_bytes();
        // 若词首是非字母前缀（@/~/*），正文从其后开始；裸标签则从词首开始。
        let body_start = if bytes.get(start).is_some_and(|b| {
            let c = *b as char;
            c == '@' || c == '~' || c == '*'
        }) {
            start + 1
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
        // 词首是否已含前缀字符（@/~/*）：Tagging 裸标签词首是首字母，不插入前缀。
        let has_prefix = self
            .input
            .as_bytes()
            .get(start)
            .is_some_and(|b| *b as char == prefix_char);
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
        if let Ok(db_tags) = crate::repo::tags::list_tags(self.conn) {
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

    /// 执行用户触发的操作：失败时把错误写入状态栏并返回 `false`，供调用方据此
    /// 跳过后续成功提示（避免错误被成功文案覆盖），而不是静默吞掉。
    pub(crate) fn note<T>(&mut self, r: anyhow::Result<T>) -> bool {
        match r {
            Ok(_) => true,
            Err(e) => {
                self.status_message = format!("err: {}", e);
                false
            }
        }
    }

    pub(crate) fn switch_to_english_ime(&self) {
        // 探测结果缓存于 OnceLock：仅首次按 PATH 检测（不 spawn 进程），之后每次
        // 切换只执行命中的那一个 helper，避免反复 spawn 多个外部进程去探测。
        static DETECTED: std::sync::OnceLock<Option<&'static str>> = std::sync::OnceLock::new();
        let helper = *DETECTED.get_or_init(|| {
            ["fcitx5-remote", "fcitx-remote"]
                .into_iter()
                .find(|n| command_in_path(n))
                .or_else(|| {
                    ["ibus", "im-select"]
                        .into_iter()
                        .find(|n| command_in_path(n))
                })
        });
        match helper {
            Some(cmd @ ("fcitx5-remote" | "fcitx-remote")) => {
                let _ = std::process::Command::new(cmd).arg("-c").status();
            }
            Some("ibus") => {
                let _ = std::process::Command::new("ibus")
                    .args(["engine", "xkb:us::eng"])
                    .status();
            }
            Some("im-select") => {
                let _ = std::process::Command::new("im-select")
                    .arg("com.apple.keylayout.ABC")
                    .status();
                let _ = std::process::Command::new("im-select").arg("1033").status();
            }
            _ => {}
        }
    }

    pub(crate) fn update_visual_selection(&mut self) {
        if self.mode == Mode::Visual {
            if let Some(start) = self.visual_start_idx {
                self.selected_ids.clear();
                let min_idx = start.min(self.selected);
                let max_idx = start.max(self.selected);
                for i in min_idx..=max_idx {
                    if let Some(row) = self.items.get(i) {
                        self.selected_ids.insert(row.id.clone());
                    }
                }
            }
        }
    }

    /// 切换当前行是否在选择集内（普通模式下 Space 使用，支持非连续多选）。
    pub(crate) fn toggle_selected(&mut self) {
        if let Some(row) = self.items.get(self.selected).cloned() {
            if !self.selected_ids.remove(&row.id) {
                self.selected_ids.insert(row.id);
            }
        }
    }

    /// 全选当前视图所有行。
    pub(crate) fn select_all(&mut self) {
        self.selected_ids.clear();
        for row in &self.items {
            self.selected_ids.insert(row.id.clone());
        }
    }

    /// 反选当前视图所有行。
    pub(crate) fn invert_selection(&mut self) {
        for row in &self.items {
            if !self.selected_ids.remove(&row.id) {
                self.selected_ids.insert(row.id.clone());
            }
        }
    }

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

    /// 设置页：新建 profile（写入 config.json，不动任何数据库）。
    pub(crate) fn settings_new_profile(&mut self, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            self.status_message = crate::tr!(
                self.lang,
                "profile 名称不能为空",
                "profile name cannot be empty"
            )
            .into();
            return Ok(());
        }
        let mut config = crate::config::Config::load()?;
        if config.profile(name).is_some() {
            self.status_message = crate::tr!(
                self.lang,
                "profile 已存在: {}",
                "profile already exists: {}",
                name
            );
            return Ok(());
        }
        config.upsert_profile(
            name,
            crate::config::Profile {
                db: format!("profiles/{name}.db"),
                cloud: None,
            },
        );
        if config.save().is_err() {
            self.status_message = crate::tr!(
                self.lang,
                "保存 config.json 失败",
                "failed to save config.json"
            )
            .into();
            return Ok(());
        }
        self.status_message = crate::tr!(
            self.lang,
            "已创建 profile: {} (下次启动可用 --profile {})",
            "created profile: {} (use --profile {} next launch)",
            name,
            name
        );
        self.refresh()?;
        Ok(())
    }

    /// 设置页：重命名当前选中的 profile。
    pub(crate) fn settings_rename_profile(&mut self, new_name: &str) -> Result<()> {
        let Some(row) = self.items.get(self.selected).cloned() else {
            return Ok(());
        };
        let new_name = new_name.trim();
        if new_name.is_empty() || new_name == row.id {
            self.status_message =
                crate::tr!(self.lang, "profile 名称无效", "invalid profile name").into();
            return Ok(());
        }
        let mut config = crate::config::Config::load()?;
        if config.profile(new_name).is_some() {
            self.status_message = crate::tr!(
                self.lang,
                "profile 已存在: {}",
                "profile already exists: {}",
                new_name
            );
            return Ok(());
        }
        if config.rename_profile(&row.id, new_name).is_err() {
            self.status_message = crate::tr!(
                self.lang,
                "profile 不存在: {}",
                "profile not found: {}",
                row.id
            );
            return Ok(());
        }
        if config.save().is_err() {
            self.status_message = crate::tr!(
                self.lang,
                "保存 config.json 失败",
                "failed to save config.json"
            )
            .into();
            return Ok(());
        }
        if self.profile_name == row.id {
            self.profile_name = new_name.to_string();
        }
        self.status_message = crate::tr!(
            self.lang,
            "已重命名: {} -> {}",
            "renamed: {} -> {}",
            row.id,
            new_name
        );
        self.refresh()?;
        Ok(())
    }

    /// 设置页：删除当前选中的 profile（仅从 config.json 移除，db 文件保留）。
    pub(crate) fn settings_delete_profile(&mut self, name: &str) -> Result<()> {
        let mut config = crate::config::Config::load()?;
        if config.remove_profile(name).is_none() {
            self.status_message = crate::tr!(
                self.lang,
                "profile 不存在: {}",
                "profile not found: {}",
                name
            );
            return Ok(());
        }
        // 删除默认 profile 时把默认改派给剩余第一个。
        if config.default_profile == name {
            config.default_profile = config
                .profile_names()
                .first()
                .cloned()
                .unwrap_or_else(|| "default".to_string());
        }
        if config.save().is_err() {
            self.status_message = crate::tr!(
                self.lang,
                "保存 config.json 失败",
                "failed to save config.json"
            )
            .into();
            return Ok(());
        }
        self.status_message = crate::tr!(
            self.lang,
            "已删除 profile: {} (db 文件保留)",
            "deleted profile: {} (db file kept)",
            name
        );
        self.refresh()?;
        Ok(())
    }

    /// 设置页：把选中的 profile 设为默认（下次无 --profile 启动生效）。
    pub(crate) fn settings_set_default(&mut self) -> Result<()> {
        let Some(row) = self.items.get(self.selected).cloned() else {
            return Ok(());
        };
        let mut config = crate::config::Config::load()?;
        if config.set_default(&row.id).is_err() {
            self.status_message = crate::tr!(
                self.lang,
                "profile 不存在: {}",
                "profile not found: {}",
                row.id
            );
            return Ok(());
        }
        if config.save().is_err() {
            self.status_message = crate::tr!(
                self.lang,
                "保存 config.json 失败",
                "failed to save config.json"
            )
            .into();
            return Ok(());
        }
        self.status_message = crate::tr!(
            self.lang,
            "默认 profile 已设为 {} (下次启动生效)",
            "default profile set to {} (applies next launch)",
            row.id
        );
        self.refresh()?;
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

    pub(crate) fn set_view(&mut self, v: View) {
        self.view = v;
        self.selected = 0;
        self.status_message.clear();
        if let Err(e) = self.refresh() {
            self.status_message = format!("err: {}", e);
        }
        self.load_detail();
    }

    pub(crate) fn move_sel(&mut self, delta: isize) {
        if self.items.is_empty() {
            self.selected = 0;
            self.load_detail();
            return;
        }
        let n = self.items.len() as isize;
        let mut s = self.selected as isize + delta;
        if s < 0 {
            s = 0;
        }
        if s >= n {
            s = n - 1;
        }
        self.selected = s as usize;
        if self.mode == Mode::Visual {
            self.update_visual_selection();
        }
        self.load_detail();
    }

    pub(crate) fn next_view(&mut self, delta: isize) {
        // 方向键环与侧栏显示顺序完全一致（今日/明日也参与循环）。
        // 金句视图仅在其功能启用时参与循环。
        let mut views = vec![
            View::Today,
            View::Tomorrow,
            View::Inbox,
            View::Next,
            View::Waiting,
            View::Scheduled,
            View::Someday,
        ];
        if self.modules.reference {
            views.push(View::Reference);
        }
        if self.modules.done {
            views.push(View::Done);
        }
        if self.modules.archived {
            views.push(View::Archived);
        }
        if self.modules.tags {
            views.push(View::Tags);
        }
        if self.quotes.enabled {
            views.push(View::Quotes);
        }
        if self.modules.review {
            views.push(View::Review);
        }
        if self.modules.settings {
            views.push(View::Settings);
        }
        let idx = views.iter().position(|v| *v == self.view).unwrap_or(0) as isize;
        let next_idx = (idx + delta).rem_euclid(views.len() as isize);
        self.set_view(views[next_idx as usize]);
    }

    /// 回车进入组织/编辑模式：与 capture 同一个一句话编辑器，预填当前任务内容。
    pub(crate) fn open_organize(&mut self) -> Result<()> {
        if self.mode == Mode::Visual {
            self.set_mode(Mode::Normal);
            self.selected_ids.clear();
            self.visual_start_idx = None;
            self.status_message = crate::tr!(
                self.lang,
                "可视模式不支持编辑",
                "editing unavailable in visual mode"
            )
            .into();
            return Ok(());
        }
        if matches!(self.view, View::Tags | View::Archived | View::Settings) {
            return Ok(());
        }
        let Some(row) = self.items.get(self.selected).cloned() else {
            return Ok(());
        };
        let Ok(task) = tasks::get(self.conn, &row.id) else {
            return Ok(());
        };
        self.organizing_id = Some(task.id.clone());
        self.input = self.task_to_quick_add(&task);
        self.set_mode(Mode::Capturing);
        self.status_message = crate::tr!(
            self.lang,
            "组织: 编辑 @标签 ~时间 *周期 (空/Esc 跳过)",
            "organize: edit @tags ~time *rrule (empty/Esc to skip)"
        )
        .into();
        Ok(())
    }

    /// 把任务序列化成 quick-add 一句话（标题 @标签 ~时间 *周期），可解析回原字段。
    pub(crate) fn task_to_quick_add(&self, task: &Task) -> String {
        let row = crate::tui::row_from(task, 0, self.conn)
            .unwrap_or_else(|_| crate::tui::row_from_tags(task, 0, Vec::new()));
        let mut s = task.title.clone();
        for tag in &row.tags {
            s.push(' ');
            s.push('@');
            s.push_str(tag);
        }
        if let Some(start) = task.scheduled_start_at {
            s.push_str(" ~");
            s.push_str(&crate::time::format_quick_time(start));
        }
        if let Some(rr) = &task.rrule {
            s.push(' ');
            s.push('*');
            s.push_str(rr);
        }
        s
    }

    pub(crate) fn act_on_selected(&mut self, to: task::Status) -> Result<()> {
        let mut ids = vec![];
        if !self.selected_ids.is_empty() {
            ids.extend(self.selected_ids.iter().cloned());
        } else if let Some(row) = self.items.get(self.selected).cloned() {
            ids.push(row.id);
        }

        if ids.is_empty() {
            return Ok(());
        }

        if ids.len() == 1 {
            let id = &ids[0];
            if let Ok(task) = tasks::get(self.conn, id) {
                if task.status == to {
                    self.status_message = crate::tr!(
                        self.lang,
                        "已是 {} 状态",
                        "already {}",
                        crate::tui::status_cn(self.lang, task.status)
                    );
                    return Ok(());
                }
                // 习惯打卡一天一次：今日已打过卡则只提示，不重复推进排程。
                let already_checked_in = to == task::Status::Done
                    && task.rrule.is_some()
                    && crate::repo::tasks::checked_in_today(
                        self.conn,
                        crate::time::local_day_bounds(0).0,
                    )
                    .unwrap_or_default()
                    .iter()
                    .any(|tid| tid == id);
                if already_checked_in {
                    self.status_message = crate::tr!(
                        self.lang,
                        "{} 今日已打卡",
                        "{} already checked in today",
                        &id[..8]
                    );
                } else {
                    // 如果当前变动状态的任务正处于 Pomodoro 专注中，且新状态为 Done/Waiting，终止番茄钟
                    if let Ok(pomo) = crate::repo::pomodoro::get_state() {
                        if pomo.task_id.as_deref() == Some(id)
                            && matches!(
                                to,
                                task::Status::Done | task::Status::Waiting | task::Status::Someday
                            )
                        {
                            let _ = crate::commands::pomo::stop();
                        }
                    }
                    let t = tasks::transition(self.conn, id, to)?;
                    self.status_message = format!(
                        "{} -> {}",
                        &t.id[..8],
                        crate::tui::status_cn(self.lang, t.status)
                    );
                }
            }
        } else {
            let mut count = 0;
            for id in &ids {
                if let Ok(task) = tasks::get(self.conn, id) {
                    if task.status != to
                        && task.status != task::Status::Scheduled
                        && tasks::transition(self.conn, id, to).is_ok()
                    {
                        count += 1;
                        if let Ok(pomo) = crate::repo::pomodoro::get_state() {
                            if pomo.task_id.as_deref() == Some(id)
                                && matches!(
                                    to,
                                    task::Status::Done
                                        | task::Status::Waiting
                                        | task::Status::Someday
                                )
                            {
                                let _ = crate::commands::pomo::stop();
                            }
                        }
                    }
                }
            }
            self.status_message =
                crate::tr!(self.lang, "批量 {} {} 项", "Bulk {} {} items", to, count);
        }

        if !self.selected_ids.is_empty() {
            self.set_mode(Mode::Normal);
            self.selected_ids.clear();
            self.visual_start_idx = None;
        }

        if to == task::Status::Done {
            let _ = crate::commands::notify::completed_feedback(self.conn);
        }

        self.refresh()?;
        self.load_detail();
        Ok(())
    }

    /// 金句移入/移出（`"` 键）：加 `quote` 标签并把工作态任务流转为 reference，
    /// 使条目离开收件箱等行动流；已有该标签则摘除（移出金句视图）。支持多选。
    pub(crate) fn toggle_quotes(&mut self) -> Result<()> {
        let mut ids = vec![];
        if !self.selected_ids.is_empty() {
            ids.extend(self.selected_ids.iter().cloned());
        } else if let Some(row) = self.items.get(self.selected).cloned() {
            ids.push(row.id);
        }
        if ids.is_empty() {
            return Ok(());
        }
        let mut added = 0;
        let mut removed = 0;
        for id in &ids {
            match self.quotes.toggle_tag(self.conn, id) {
                Ok(Some(true)) => added += 1,
                Ok(Some(false)) => removed += 1,
                _ => {}
            }
        }
        self.status_message = if added > 0 && removed == 0 {
            crate::tr!(
                self.lang,
                "已加入金句 ({} 项)",
                "added to quotes ({} item(s))",
                added
            )
        } else if removed > 0 && added == 0 {
            crate::tr!(
                self.lang,
                "已移出金句 ({} 项)",
                "removed from quotes ({} item(s))",
                removed
            )
        } else {
            crate::tr!(
                self.lang,
                "金句: +{} / -{}",
                "quotes: +{} / -{}",
                added,
                removed
            )
        };
        if !self.selected_ids.is_empty() {
            self.set_mode(Mode::Normal);
            self.selected_ids.clear();
            self.visual_start_idx = None;
        }
        self.reload()?;
        Ok(())
    }
}
