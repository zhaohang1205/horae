use anyhow::Result;
use ratatui::widgets::ListState;
use rusqlite::Connection;

use horae_core::model::event::TaskEvent;
use horae_core::model::pomodoro::PomoState;
use horae_core::model::tag::Tag;
use horae_core::model::task::Task;

pub(crate) mod completion;
mod data;
mod ops;
mod profiles;

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
    /// GTD 工作流说明视图（纯展示，无任务主体）。
    Workflow,
}

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
            | View::Settings
            | View::Workflow => None,
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
    /// 检查单逐项管理：光标在清单项间移动，可单独勾选/删除/排序/改名。
    ChecklistFocus,
    /// 检查单项改名：输入新标题。
    RenamingChecklist,
}

impl Mode {
    pub(crate) fn is_input(&self) -> bool {
        !matches!(self, Mode::Normal | Mode::Visual | Mode::ChecklistFocus)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Pane {
    Left,
    Center,
    Right,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum Popup {
    /// Prompt to enter Pomodoro mode for a scheduled task
    TaskDueNow(String, String), // task_id, task_title
    /// Feature toggles modal (current selected index)
    ModuleToggles(usize),
}

#[derive(Clone, Debug)]
pub(crate) struct Toast {
    pub message: String,
    pub created_at_ms: i64,
    pub duration_ms: i64,
    pub is_success: bool,
}

#[derive(Clone, Debug)]
pub(crate) enum UndoAction {
    StatusChange {
        task_id: String,
        from: horae_core::model::task::Status,
        to: horae_core::model::task::Status,
        title: String,
    },
    BulkStatusChange {
        records: Vec<(
            String,
            horae_core::model::task::Status,
            horae_core::model::task::Status,
        )>,
    },
    Archive {
        task_id: String,
        from_status: horae_core::model::task::Status,
        title: String,
    },
    BulkArchive {
        records: Vec<(String, horae_core::model::task::Status)>,
    },
    Unarchive {
        task_id: String,
        title: String,
    },
    Created {
        task_id: String,
        title: String,
    },
    ChecklistToggled {
        task_id: String,
        item_id: String,
        item_title: String,
    },
}

#[derive(Clone)]
pub(crate) struct Row {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) status: String,
    pub(crate) due: Option<i64>,
    pub(crate) tags: Vec<String>,
    pub(crate) priority: Option<String>,
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
    /// 瞬时操作提示浮层（Toast）。
    pub(crate) toast: Option<Toast>,
    /// 操作撤销栈（最多记录 50 步操作）。
    pub(crate) undo_stack: Vec<UndoAction>,
    /// 操作重做栈。
    pub(crate) redo_stack: Vec<UndoAction>,
    pub(crate) lang: horae_core::i18n::Lang,
    pub(crate) show_help: bool,
    pub(crate) show_syntax: bool,
    pub(crate) syntax_scroll: usize,
    pub(crate) show_shortcut_bar: bool,
    pub(crate) help_scroll: usize,
    /// GTD 工作流视图（中心决策树）滚动偏移。
    pub(crate) workflow_scroll: usize,
    /// GTD 工作流视图（右侧哲学与人物）滚动偏移。
    pub(crate) workflow_side_scroll: usize,
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
    pub(crate) notification_engine: horae_core::notification::NotificationEngine,
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
    pub(crate) quotes: horae_core::repo::quotes::Quotes,
    /// 模块显示门控 (Splash, Reference, Done, etc.)
    pub(crate) modules: horae_core::repo::modules::ModuleVisibility,
    /// 图标风格（Nerd Font / ASCII 回退）。
    pub(crate) icon_style: crate::tui::icons::IconStyle,
    /// 当前会话使用的 profile 名（用于设置页标记与显示）。
    pub(crate) profile_name: String,
    /// 设置页待删除的 profile 名。
    pub(crate) pending_profile_delete: Option<String>,
    /// 检查单管理（ChecklistFocus）模式下的光标位置（指向当前任务的某一项）。
    pub(crate) checklist_cursor: Option<usize>,
    /// 启动即进入快速录入（settings 键 `start_capture`，默认开启）。
    pub(crate) start_in_capture: bool,
    /// 自动补全模式：语法参考（Reference，默认）vs 极速补全（Speed）。
    pub(crate) completion_style: crate::tui::app::completion::CompletionStyle,
    /// 纯净录入无干扰（settings 键 `zen_capture`，默认开启）。
    pub(crate) zen_capture: bool,
    /// 农历与节气提醒开关（settings 键 `lunar_reminder`，默认开启）。
    pub(crate) lunar_enabled: bool,
    /// 今日聚合历法与节气信息。
    pub(crate) calendar_info: Option<horae_core::lunar::CalendarDayInfo>,
}

impl<'a> App<'a> {
    pub(crate) fn new(conn: &'a Connection) -> Result<Self> {
        // 从 settings 表恢复语言与主题。
        let lang = match horae_core::repo::settings::get(conn, "lang")
            .ok()
            .flatten()
            .as_deref()
        {
            Some("en") => horae_core::i18n::Lang::En,
            _ => horae_core::i18n::Lang::Zh,
        };
        let theme = match horae_core::repo::settings::get(conn, "theme")
            .ok()
            .flatten()
            .as_deref()
        {
            Some("latte") => crate::tui::theme::Theme::catppuccin_latte(),
            _ => crate::tui::theme::Theme::catppuccin_mocha(),
        };
        let quotes = horae_core::repo::quotes::Quotes::load(conn);
        let modules = horae_core::repo::modules::ModuleVisibility::load(conn);
        let icon_style = crate::tui::icons::IconStyle::load(conn);
        // 启动即快速录入：缺省视为开启（settings 显式写 "0" 才关闭）。
        let start_in_capture = !matches!(
            horae_core::repo::settings::get(conn, "start_capture")
                .ok()
                .flatten()
                .as_deref(),
            Some("0")
        );
        let completion_style = horae_core::repo::settings::get(conn, "completion_style")
            .ok()
            .flatten()
            .map(|s| crate::tui::app::completion::CompletionStyle::from_key(&s))
            .unwrap_or_default();
        // 纯净录入无干扰：缺省视为开启（settings 显式写 "0" 才关闭）。
        let zen_capture = !matches!(
            horae_core::repo::settings::get(conn, "zen_capture")
                .ok()
                .flatten()
                .as_deref(),
            Some("0")
        );
        // 农历与节气提醒：缺省视为开启（settings 显式写 "0" 才关闭）。
        let lunar_enabled = !matches!(
            horae_core::repo::settings::get(conn, "lunar_reminder")
                .ok()
                .flatten()
                .as_deref(),
            Some("0")
        );
        let calendar_info = if lunar_enabled {
            let today = chrono::Local::now().naive_local().date();
            horae_core::lunar::day_calendar_info(today)
        } else {
            None
        };
        let mut app = App {
            conn,
            view: View::Inbox,
            items: Vec::new(),
            selected: 0,
            list_state: ListState::default(),
            detail: None,
            mode: if start_in_capture {
                Mode::Capturing
            } else {
                Mode::Normal
            },
            pane: Pane::Center,
            input: String::new(),
            input_cursor: 0,
            organizing_id: None,
            status_message: String::new(),
            toast: None,
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            lang,
            show_help: false,
            show_syntax: false,
            syntax_scroll: 0,
            show_shortcut_bar: true,
            help_scroll: 0,
            workflow_scroll: 0,
            workflow_side_scroll: 0,
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
            notification_engine: horae_core::notification::NotificationEngine::new(),
            last_notify_check_ms: 0,
            counts: std::collections::HashMap::new(),
            rrule_cache: std::collections::HashMap::new(),
            pomo: horae_core::repo::pomodoro::get_state().unwrap_or_default(),
            pomo_loaded_ms: 0,
            completion_candidates: Vec::new(),
            completion_index: 0,
            completion_range: None,
            completion_prefix: '@',
            quotes,
            modules,
            icon_style,
            profile_name: String::new(),
            pending_profile_delete: None,
            checklist_cursor: None,
            start_in_capture,
            completion_style,
            zen_capture,
            lunar_enabled,
            calendar_info,
        };
        app.refresh()?;

        app.load_detail();
        if start_in_capture {
            // 启动即快速录入：空输入，光标就位。
            app.input.clear();
            app.set_mode(Mode::Capturing);
            app.status_message = tr!(
                lang,
                "快速录入: @标签 ~时间 *周期 (Esc 返回列表)",
                "Quick capture: @tag ~time *rrule (Esc for list)"
            )
            .into();
        }
        app.switch_to_english_ime();
        Ok(app)
    }

    /// 当前是否处于纯净无干扰快速录入模式（启用 zen_capture 且为全新录入，非编辑组织模式）。
    pub(crate) fn is_zen_capturing(&self) -> bool {
        self.zen_capture && self.mode == Mode::Capturing && self.organizing_id.is_none()
    }

    pub(crate) fn check_notifications(&mut self) {
        let now_ms = horae_core::time::now_ms();
        // Bug4 修复：每日摘要检查每 60s 至多一次，避免每帧都读 notify.json。
        if now_ms - self.last_notify_check_ms >= 60_000 {
            let _ = horae_core::notify::check(self.conn);
            self.last_notify_check_ms = now_ms;
        }

        let events = self.notification_engine.tick(self.conn);
        for event in events {
            match event {
                // Bug5 修复：InOneHour/InTenMins 只有 title 一个字段，去掉多余的 ..
                horae_core::notification::NotificationEvent::InOneHour { title } => {
                    horae_core::notify::desktop("任务即将在1小时后开始", &title);
                }
                horae_core::notification::NotificationEvent::InTenMins { title } => {
                    horae_core::notify::desktop("任务即将在10分钟后开始", &title);
                }
                horae_core::notification::NotificationEvent::Now { id, title } => {
                    horae_core::notify::desktop("任务现在开始!", &title);
                    self.popup = Some(Popup::TaskDueNow(id, title));
                    self.needs_clear = true; // force redraw to show popup
                }
            }
        }
    }

    /// 按需刷新番茄钟状态快照（每 ~500ms 至多重读一次 `pomo.json`），供渲染帧内
    /// 复用，避免每帧多次磁盘读 + JSON 解析。
    pub(crate) fn refresh_pomo(&mut self) {
        let now = horae_core::time::now_ms();
        if now - self.pomo_loaded_ms < 500 {
            return;
        }
        self.pomo_loaded_ms = now;
        self.pomo = horae_core::repo::pomodoro::get_state().unwrap_or_default();
    }

    /// 强制立即重新加载番茄钟状态（用于启动/停止/完成番茄钟操作后瞬时刷新 UI）。
    pub(crate) fn force_reload_pomo(&mut self) {
        self.pomo_loaded_ms = horae_core::time::now_ms();
        self.pomo = horae_core::repo::pomodoro::get_state().unwrap_or_default();
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

    /// 按当前图标风格取字形（Nerd Font 或 ASCII 回退）。
    pub(crate) fn icon(&self, kind: crate::tui::icons::Icon) -> &'static str {
        crate::tui::icons::glyph(kind, self.icon_style)
    }

    pub(crate) fn set_view(&mut self, v: View) {
        self.view = v;
        self.selected = 0;
        self.workflow_scroll = 0;
        self.workflow_side_scroll = 0;
        self.status_message.clear();
        if let Err(e) = self.refresh() {
            self.status_message = format!("err: {}", e);
        }
        self.load_detail();
    }

    pub(crate) fn move_sel(&mut self, delta: isize) {
        if self.view == View::Workflow {
            if delta <= -10000 {
                if self.pane == Pane::Right {
                    self.workflow_side_scroll = 0;
                } else {
                    self.workflow_scroll = 0;
                }
            } else if self.pane == Pane::Right {
                let s = self.workflow_side_scroll as isize + delta;
                self.workflow_side_scroll = s.max(0) as usize;
            } else {
                let s = self.workflow_scroll as isize + delta;
                self.workflow_scroll = s.max(0) as usize;
            }
            return;
        }
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
        views.push(View::Workflow);
        let idx = views.iter().position(|v| *v == self.view).unwrap_or(0) as isize;
        let next_idx = (idx + delta).rem_euclid(views.len() as isize);
        self.set_view(views[next_idx as usize]);
    }
}
