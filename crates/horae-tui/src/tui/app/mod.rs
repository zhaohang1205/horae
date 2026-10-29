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
pub(crate) mod voice;
pub(crate) use voice::VoiceRecording;

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
pub(crate) fn command_in_path(name: &str) -> bool {
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

/// TUI 启动模式选择（正常主页面 vs 闪念录入即退出）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LaunchMode {
    #[default]
    Default,
    Normal,
    Flash,
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
    /// Password setup modal (first password input or confirm input)
    PasswordSet {
        is_confirm: bool,
        input: String,
        first_pass: String,
        error: Option<String>,
    },
    /// Password disable modal (enter current password to disable)
    PasswordDisable {
        input: String,
        error: Option<String>,
    },
    /// Password change modal
    PasswordChange {
        step: u8, // 0 = old pass, 1 = new pass, 2 = confirm pass
        old_pass: String,
        new_pass: String,
        input: String,
        error: Option<String>,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ToastKind {
    Success,
    Info,
    Warning,
    Error,
}

#[derive(Clone, Debug)]
pub(crate) struct Toast {
    pub message: String,
    pub created_at_ms: i64,
    pub duration_ms: i64,
    pub kind: ToastKind,
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
    /// 是否为真正的截止时间（如带有 --due）
    pub(crate) is_deadline: bool,
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
    pub(crate) omarchy_auto: bool,
    pub(crate) last_theme_check: std::time::Instant,
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
    /// 闪念录入即退出（settings 键 `flash_mode`，默认关闭）。
    pub(crate) flash_mode: bool,
    /// 语音录入后台句柄与状态。
    pub(crate) voice_recording: Option<VoiceRecording>,
    /// 是否正在调用 voxtype 转写中。
    pub(crate) voice_transcribing: bool,
    /// 后台 voxtype 转写结果接收通道。
    pub(crate) voice_transcribe_rx:
        Option<std::sync::mpsc::Receiver<std::io::Result<std::process::Output>>>,
    /// 农历与节气提醒开关（settings 键 `lunar_reminder`，默认开启）。
    pub(crate) lunar_enabled: bool,
    /// 今日聚合历法与节气信息。
    pub(crate) calendar_info: Option<horae_core::lunar::CalendarDayInfo>,
    /// 密码保护模式开关（settings 键 `password_enabled`，默认关闭）。
    pub(crate) password_enabled: bool,
    /// 提示音效开关（settings 键 `sound_enabled`，默认开启）。
    pub(crate) sound_enabled: bool,
    /// 是否处于密码锁定状态（保护数据安全，解锁前不呈现数据与操作）。
    pub(crate) locked: bool,
    /// 锁定界面已输入的密码缓冲。
    pub(crate) lock_input: String,
    /// 锁定界面错误提示信息。
    pub(crate) lock_error: Option<String>,
}

impl<'a> App<'a> {
    #[allow(dead_code)]
    pub(crate) fn new(conn: &'a Connection) -> Result<Self> {
        Self::new_with_mode(conn, LaunchMode::Default)
    }

    pub(crate) fn new_with_mode(conn: &'a Connection, launch_mode: LaunchMode) -> Result<Self> {
        // 一次性批量加载 settings 表，避免 10+ 次独立 SQL 往返开销。
        let settings_map = horae_core::repo::settings::get_all(conn).unwrap_or_default();
        let lang = match settings_map.get("lang").map(|s| s.as_str()) {
            Some("en") => horae_core::i18n::Lang::En,
            _ => horae_core::i18n::Lang::Zh,
        };
        let (theme, omarchy_auto) = {
            let env_theme = std::env::var("HORAE_THEME")
                .ok()
                .and_then(|s| crate::tui::theme::Theme::from_name(&s));
            if let Some(t) = env_theme {
                (t, false)
            } else if let Some(key) = settings_map.get("theme") {
                if key == "auto" {
                    let detected = crate::tui::theme::detect_omarchy_theme()
                        .unwrap_or_else(crate::tui::theme::Theme::catppuccin_mocha);
                    (detected, true)
                } else {
                    let loaded = crate::tui::theme::Theme::from_name(key)
                        .unwrap_or_else(crate::tui::theme::Theme::catppuccin_mocha);
                    (loaded, false)
                }
            } else {
                let detected = crate::tui::theme::detect_omarchy_theme();
                let is_omarchy = crate::tui::theme::is_omarchy_system();
                (
                    detected.unwrap_or_else(crate::tui::theme::Theme::catppuccin_mocha),
                    is_omarchy,
                )
            }
        };
        let quotes = horae_core::repo::quotes::Quotes {
            enabled: matches!(settings_map.get("quotes").map(|s| s.as_str()), Some("1")),
        };
        let modules = horae_core::repo::modules::ModuleVisibility::from_map(&settings_map);
        let icon_style = crate::tui::icons::IconStyle::from_map(&settings_map, conn);
        // 启动即快速录入：缺省视为关闭（settings 显式写 "1" 才开启）。
        let start_in_capture = match launch_mode {
            LaunchMode::Flash => true,
            LaunchMode::Normal => false,
            LaunchMode::Default => matches!(
                settings_map.get("start_capture").map(|s| s.as_str()),
                Some("1")
            ),
        };
        let completion_style = settings_map
            .get("completion_style")
            .map(|s| crate::tui::app::completion::CompletionStyle::from_key(s))
            .unwrap_or_default();
        // 纯净录入无干扰：缺省视为开启（settings 显式写 "0" 才关闭）。
        let zen_capture = !matches!(
            settings_map.get("zen_capture").map(|s| s.as_str()),
            Some("0")
        );
        // 闪念录入即退出：缺省视为关闭（settings 显式写 "1" 才开启）。
        let flash_mode = match launch_mode {
            LaunchMode::Flash => true,
            LaunchMode::Normal => false,
            LaunchMode::Default => matches!(
                settings_map.get("flash_mode").map(|s| s.as_str()),
                Some("1")
            ),
        };
        // 农历与节气提醒：缺省视为开启（settings 显式写 "0" 才关闭）。
        let lunar_enabled = !matches!(
            settings_map.get("lunar_reminder").map(|s| s.as_str()),
            Some("0")
        );
        // 密码保护模式：缺省视为关闭（settings 显式写 "1" 才开启）。
        let password_enabled = matches!(
            settings_map.get("password_enabled").map(|s| s.as_str()),
            Some("1")
        );
        let locked = password_enabled && std::env::var("HORAE_UNLOCKED").is_err();
        // 提示音效开关：缺省视为开启（settings 显式写 "0" 才关闭）。
        let sound_enabled = !matches!(
            settings_map.get("sound_enabled").map(|s| s.as_str()),
            Some("0")
        );
        // 农历卡片由 [`App::refresh`] 统一计算（构造末尾必跑一次），此处不重复算。
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
            omarchy_auto,
            last_theme_check: std::time::Instant::now(),
            popup: None,
            notification_engine: horae_core::notification::NotificationEngine::new(),
            last_notify_check_ms: 0,
            counts: std::collections::HashMap::new(),
            rrule_cache: std::collections::HashMap::new(),
            pomo: horae_core::repo::pomodoro::get_state().unwrap_or_default(),
            // 上一行刚读完 pomo.json：打上时间戳，避免主循环首帧又重复读一次。
            pomo_loaded_ms: horae_core::time::now_ms(),
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
            flash_mode,
            voice_recording: None,
            voice_transcribing: false,
            voice_transcribe_rx: None,
            lunar_enabled,
            calendar_info: None,
            password_enabled,
            sound_enabled,
            locked,
            lock_input: String::new(),
            lock_error: None,
        };
        app.refresh()?;

        app.load_detail();
        if start_in_capture {
            // 启动即快速录入：空输入，光标就位。
            app.input.clear();
            app.set_mode(Mode::Capturing);
            app.status_message = if flash_mode {
                tr!(
                    lang,
                    "闪念录入: 回车录入并退出 (Esc 返回列表)",
                    "Flash capture: Enter to save & exit (Esc for list)"
                )
                .into()
            } else {
                tr!(
                    lang,
                    "快速录入: @标签 ~时间 *周期 (Esc 返回列表)",
                    "Quick capture: @tag ~time *rrule (Esc for list)"
                )
                .into()
            };
        }
        app.switch_to_english_ime();
        Ok(app)
    }

    /// 当前是否处于纯净无干扰快速录入模式（启用 zen_capture 且为全新录入，非编辑组织模式）。
    pub(crate) fn is_zen_capturing(&self) -> bool {
        self.zen_capture && self.mode == Mode::Capturing && self.organizing_id.is_none()
    }

    pub(crate) fn check_notifications(&mut self) {
        // 快速录入模式（以及各录入/编辑子模式）下保持纯净无干扰：
        // 暂停检查与发送桌面通知、不弹窗到期提醒，避免打断用户的输入流与按键焦点。
        // 待退出录入模式回到普通视图后，下一帧即可恢复正常检查。
        if self.mode.is_input() {
            return;
        }

        let now_ms = horae_core::time::now_ms();
        // Bug4 修复：每日摘要检查每 60s 至多一次，避免每帧都读 notify.json。
        if now_ms - self.last_notify_check_ms >= 60_000 {
            let _ = horae_core::notify::check(self.conn);
            self.last_notify_check_ms = now_ms;
        }

        let events = self.notification_engine.tick(self.conn);
        for event in events {
            let summary = event.summary(self.lang);
            let body = event.body(self.lang);
            horae_core::notify::desktop_notify(&summary, &body, event.urgency(), event.icon());

            if let horae_core::notification::NotificationEvent::Now { id, title, .. } = event {
                horae_core::sound::play(horae_core::sound::SoundKind::TaskDue);
                self.popup = Some(Popup::TaskDueNow(id, title));
                self.needs_clear = true; // force redraw to show popup
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

    /// 弹出带分级生命周期的提示：
    /// - `Success` / `Info`：3 秒后淡出；
    /// - `Warning`：5 秒后淡出；
    /// - `Error`：保留到用户下一次按键，绝不自动消失。
    pub(crate) fn flash(&mut self, kind: ToastKind, message: impl Into<String>) {
        let msg = message.into();
        self.status_message = msg.clone();
        let duration_ms = match kind {
            ToastKind::Success | ToastKind::Info => 3000,
            ToastKind::Warning => 5000,
            ToastKind::Error => i64::MAX,
        };
        self.toast = Some(Toast {
            message: msg,
            created_at_ms: horae_core::time::now_ms(),
            duration_ms,
            kind,
        });
    }

    /// 是否存在需要随时间自动淡出的 Toast（供事件循环决定是否持续重绘）。
    pub(crate) fn has_expiring_toast(&self) -> bool {
        self.toast
            .as_ref()
            .is_some_and(|t| t.kind != ToastKind::Error)
    }

    /// 持久化一项设置到 `settings` 表；若写入失败则发出 Warning 提示。
    pub(crate) fn save_setting(&mut self, key: &str, value: &str) {
        if let Err(e) = horae_core::repo::settings::set(self.conn, key, value) {
            self.flash(
                ToastKind::Warning,
                tr!(
                    self.lang,
                    "设置未能保存，下次启动将恢复原值: {}",
                    "Failed to save setting (will reset next launch): {}",
                    e
                ),
            );
        }
    }

    /// 执行用户触发的操作：失败时把错误写入状态栏并返回 `false`，供调用方据此
    /// 跳过后续成功提示（避免错误被成功文案覆盖），而不是静默吞掉。
    pub(crate) fn note<T>(&mut self, r: anyhow::Result<T>) -> bool {
        match r {
            Ok(_) => true,
            Err(e) => {
                self.flash(
                    ToastKind::Error,
                    tr!(self.lang, "操作失败: {}", "Operation failed: {}", e),
                );
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
        if let Some(cmd) = helper {
            // 在后台线程执行外部输入法切换指令，避免冷启动及模式切换阻塞主线程毫秒级延迟
            std::thread::spawn(move || match cmd {
                "fcitx5-remote" | "fcitx-remote" => {
                    let _ = std::process::Command::new(cmd).arg("-c").status();
                }
                "ibus" => {
                    let _ = std::process::Command::new("ibus")
                        .args(["engine", "xkb:us::eng"])
                        .status();
                }
                "im-select" => {
                    let _ = std::process::Command::new("im-select")
                        .arg("com.apple.keylayout.ABC")
                        .status();
                    let _ = std::process::Command::new("im-select").arg("1033").status();
                }
                _ => {}
            });
        }
    }

    /// 若处于自动跟随 Omarchy 主题模式，定期探测系统主题变化并热重载。
    pub(crate) fn check_omarchy_theme_sync(&mut self) {
        if !self.omarchy_auto {
            return;
        }
        if self.last_theme_check.elapsed() < std::time::Duration::from_millis(1000) {
            return;
        }
        self.last_theme_check = std::time::Instant::now();
        if let Some(detected) = crate::tui::theme::detect_omarchy_theme() {
            if detected.id != self.theme.id {
                self.theme = detected;
                self.needs_clear = true;
            }
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
        self.toast = None;
        self.status_message.clear();
        if let Err(e) = self.refresh() {
            self.flash(
                ToastKind::Error,
                tr!(
                    self.lang,
                    "刷新视图失败: {}",
                    "Failed to refresh view: {}",
                    e
                ),
            );
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
