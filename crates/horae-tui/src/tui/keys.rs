//! 快捷键单一数据源：F1 快捷键面板、状态栏快捷键条、引导栏动态任务键都从这里渲染。
//!
//! 设计要点：
//! - 分为两层：**状态栏快捷键条**（常驻展示 a/x/s/w/f/F1/F7）与**任务操作键**（引导栏 [Keys] 上下文驱动）。
//! - 每一条 `KeyDef` 描述一个按键在某个上下文里的含义（含视图/选择/周回顾/番茄钟约束）。
//! - 同一个物理键可有多个 `KeyDef`（如 `a` 在 Tags 视图=新增标签，其余=捕获任务）。
//! - `hjkl` 压缩为一条，等价于上下左右方向键。
//! - 每条带 `heat`（预设热度）：引导栏 / 状态栏 / F1 面板统一按热度降序展示。

use super::app::{Mode, View};
use horae_core::i18n::Lang;
use horae_core::model::task::Status;

/// 快捷键分区：`Global` 用于全局操作，`Task` 用于任务操作（F1 按此分区展示）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum KeyGroup {
    Global,
    Task,
}

pub(crate) const GROUP_ORDER: [KeyGroup; 2] = [KeyGroup::Global, KeyGroup::Task];

impl KeyGroup {
    pub(crate) fn title(self, lang: Lang) -> &'static str {
        match self {
            KeyGroup::Global => lang.tr(" 全局 ", " Global "),
            KeyGroup::Task => lang.tr(" 任务 ", " Tasks "),
        }
    }
}

/// 适用条件。`applies` 只在 Normal/Visual 模式下被调用。
#[derive(Clone, Copy)]
pub(crate) enum When {
    /// 任何 Normal/Visual 上下文都可用。
    Always,
    /// 无需选中即可用的通用操作（捕获/搜索/过滤等）。
    General,
    /// 需要选中行，且不在排除视图内（Tags/Archived 是标签行与归档行，任务键无意义）。
    SelectionNot(&'static [View]),
    /// 仅指定视图（不要求选中）。
    View(View),
    /// 指定视图且有选中行。
    ViewSel(View),
    /// 仅周回顾进行中。
    Reviewing,
    /// 番茄钟活动时（需选中任务）。
    PomoActive,
    /// 需要选中任务行，且该任务状态不在列内。
    StatusNot(&'static [Status]),
    /// 需要选中任务行，且该任务含检查单。
    HasChecklist,
    /// 金句功能启用且选中任务行（非 Tags/Archived/Quotes 视图）：加入金句。
    QuoteAdd,
    /// 金句功能启用且选中金句视图行：移出金句。
    QuoteRemove,
}

/// 非任务视图：行不是任务，任务操作键不适用。
pub(crate) const NON_TASK_VIEWS: &[View] = &[View::Tags, View::Archived, View::Settings];

/// `~` 时间补全候选（中文模式：首屏覆盖自然语言天词、常用英文词、相对偏移、星期词汇、跨周表达与整点时刻等多样表达）。
pub(crate) const TIME_CANDIDATES_ZH: &[&str] = &[
    "today",
    "tomorrow",
    "今天",
    "明天",
    "+1h",
    "+1d",
    "周五",
    "下周一",
    "18:00",
    "now",
    "8/20",
    "后天",
    "+30m",
    "+15m",
    "+2h",
    "+3h",
    "+4h",
    "+2d",
    "+3d",
    "+1w",
    "周一",
    "周二",
    "周三",
    "周四",
    "周六",
    "周日",
    "周末",
    "09:00",
];

/// `~` 时间补全候选（英文模式：纯英文词汇，首屏涵盖天词、相对偏移、星期、整点时刻等多样表达）。
pub(crate) const TIME_CANDIDATES_EN: &[&str] = &[
    "today", "tomorrow", "+1h", "+1d", "fri", "18:00", "now", "8/20", "+30m", "+15m", "+2h", "+3h",
    "+4h", "+2d", "+3d", "+1w", "mon", "tue", "wed", "thu", "sat", "sun", "weekend", "09:00",
];

pub(crate) fn time_candidates(lang: Lang) -> &'static [&'static str] {
    match lang {
        Lang::Zh => TIME_CANDIDATES_ZH,
        Lang::En => TIME_CANDIDATES_EN,
    }
}

/// `*` 循环简写补全候选（全景覆盖基础周期、工作日/周末、复合周几、月末倒数、月首月末与年度月份）。
pub(crate) const RRULE_CANDIDATES: &[&str] = &[
    "d",
    "w",
    "m",
    "y",
    "weekday",
    "weekend",
    "2w[1,3]",
    "m[-1]",
    "m[1,-1]",
    "y[jan,jul]",
    "1w[mo,we]",
    "m[1,2,-2,-1]",
    "2d",
];

/// `!` 优先级补全候选（输入即弹，按前缀过滤）。
pub(crate) const PRIORITY_CANDIDATES: &[&str] = &["high", "medium", "low"];

pub(crate) struct KeyDef {
    pub keys: &'static str,
    pub zh: &'static str,
    pub en: &'static str,
    pub group: KeyGroup,
    /// 是否显示在底部状态栏快捷键条。
    pub status: bool,
    pub when: When,
    /// 预设热度（越大越常用），引导栏 / 状态栏 / F1 面板均按热度降序展示。
    pub heat: u8,
}

/// 当前选中行是否为任务行（有选中且视图非 Tags/Archived）。
fn sel_task(c: &Ctx) -> bool {
    c.has_selection && !NON_TASK_VIEWS.contains(&c.view)
}

impl KeyDef {
    pub(crate) fn applies(&self, c: &Ctx) -> bool {
        use When::*;
        match self.when {
            Always => true,
            General => true,
            SelectionNot(vs) => c.has_selection && !vs.contains(&c.view),
            View(v) => c.view == v,
            ViewSel(v) => c.view == v && c.has_selection,
            Reviewing => c.is_reviewing,
            PomoActive => c.pomo_active && sel_task(c),
            StatusNot(ss) => sel_task(c) && c.task_status.is_some_and(|s| !ss.contains(&s)),
            HasChecklist => sel_task(c) && c.has_checklist,
            QuoteAdd => {
                c.quotes_enabled
                    && c.has_selection
                    && !NON_TASK_VIEWS.contains(&c.view)
                    && c.view != crate::tui::View::Quotes
            }
            QuoteRemove => {
                c.quotes_enabled && c.view == crate::tui::View::Quotes && c.has_selection
            }
        }
    }

    pub(crate) fn desc(&self, lang: Lang) -> &'static str {
        lang.tr(self.zh, self.en)
    }
}

/// 渲染时刻的应用状态快照。
#[derive(Clone, Copy)]
pub(crate) struct Ctx {
    pub view: View,
    pub mode: Mode,
    pub has_selection: bool,
    pub is_reviewing: bool,
    pub pomo_active: bool,
    /// 当前选中任务的状态（非任务行 / 无法解析时为 None）。
    pub task_status: Option<Status>,
    /// 当前选中任务是否含检查单。
    pub has_checklist: bool,
    /// 金句视图功能是否启用（F7 开关）。
    pub quotes_enabled: bool,
}

pub(crate) fn ctx_of(app: &super::App) -> Ctx {
    let pomo_active = app.pomo.phase != horae_core::model::pomodoro::Phase::Idle;
    let (task_status, has_checklist) = app
        .items
        .get(app.selected)
        .map(|r| (r.status.parse().ok(), r.done.is_some()))
        .unwrap_or((None, false));
    Ctx {
        view: app.view,
        mode: app.mode,
        has_selection: app.selected < app.items.len(),
        is_reviewing: app.is_reviewing,
        pomo_active,
        task_status,
        has_checklist,
        quotes_enabled: app.quotes.enabled,
    }
}

pub(crate) const KEY_TABLE: &[KeyDef] = &[
    // ── Global ──
    KeyDef {
        keys: "hjkl",
        zh: "方向键",
        en: "arrows",
        group: KeyGroup::Global,
        status: true,
        when: When::Always,
        heat: 100,
    },
    KeyDef {
        keys: "v",
        zh: "多选",
        en: "multi",
        group: KeyGroup::Global,
        status: false,
        when: When::Always,
        heat: 96,
    },
    KeyDef {
        keys: "g/G",
        zh: "首尾",
        en: "top/bot",
        group: KeyGroup::Global,
        status: false,
        when: When::Always,
        heat: 70,
    },
    KeyDef {
        keys: "0-9",
        zh: "切视图",
        en: "view",
        group: KeyGroup::Global,
        status: false,
        when: When::Always,
        heat: 90,
    },
    KeyDef {
        keys: "J/K",
        zh: "今日/明日",
        en: "today/tmrw",
        group: KeyGroup::Global,
        status: false,
        when: When::Always,
        heat: 85,
    },
    KeyDef {
        keys: "r",
        zh: "周回顾",
        en: "review",
        group: KeyGroup::Global,
        status: false,
        when: When::Always,
        heat: 80,
    },
    KeyDef {
        keys: "/",
        zh: "搜索",
        en: "search",
        group: KeyGroup::Global,
        status: true,
        when: When::General,
        heat: 95,
    },
    KeyDef {
        keys: "f",
        zh: "过滤",
        en: "filter",
        group: KeyGroup::Global,
        status: true,
        when: When::General,
        heat: 92,
    },
    KeyDef {
        keys: "a",
        zh: "捕获",
        en: "capture",
        group: KeyGroup::Global,
        status: true,
        when: When::General,
        heat: 100,
    },
    KeyDef {
        keys: "Esc",
        zh: "取消",
        en: "cancel",
        group: KeyGroup::Global,
        status: false,
        when: When::Always,
        heat: 88,
    },
    KeyDef {
        keys: "F1",
        zh: "帮助",
        en: "help",
        group: KeyGroup::Global,
        status: true,
        when: When::Always,
        heat: 75,
    },
    KeyDef {
        keys: "F2",
        zh: "快捷键条开关",
        en: "shortcut bar",
        group: KeyGroup::Global,
        status: false,
        when: When::Always,
        heat: 55,
    },
    KeyDef {
        keys: "Ctrl+P",
        zh: "语法",
        en: "syntax",
        group: KeyGroup::Global,
        status: true,
        when: When::Always,
        heat: 45,
    },
    KeyDef {
        keys: "F5",
        zh: "主题",
        en: "theme",
        group: KeyGroup::Global,
        status: true,
        when: When::Always,
        heat: 60,
    },
    KeyDef {
        keys: "F6",
        zh: "语言",
        en: "lang",
        group: KeyGroup::Global,
        status: true,
        when: When::Always,
        heat: 58,
    },
    KeyDef {
        keys: "F7",
        zh: "功能开关",
        en: "toggles",
        group: KeyGroup::Global,
        status: true,
        when: When::Always,
        heat: 56,
    },
    KeyDef {
        keys: "M",
        zh: "设置",
        en: "settings",
        group: KeyGroup::Global,
        status: false,
        when: When::Always,
        heat: 50,
    },
    KeyDef {
        keys: "W",
        zh: "工作流",
        en: "workflow",
        group: KeyGroup::Global,
        status: false,
        when: When::Always,
        heat: 50,
    },
    KeyDef {
        keys: "q",
        zh: "退出",
        en: "quit",
        group: KeyGroup::Global,
        status: false,
        when: When::Always,
        heat: 82,
    },
    // ── 设置页（Settings 视图内）──
    KeyDef {
        keys: "n",
        zh: "新建 profile",
        en: "new profile",
        group: KeyGroup::Global,
        status: false,
        when: When::View(View::Settings),
        heat: 60,
    },
    KeyDef {
        keys: "r",
        zh: "重命名",
        en: "rename",
        group: KeyGroup::Global,
        status: false,
        when: When::ViewSel(View::Settings),
        heat: 58,
    },
    KeyDef {
        keys: "d",
        zh: "删除",
        en: "delete",
        group: KeyGroup::Global,
        status: false,
        when: When::ViewSel(View::Settings),
        heat: 56,
    },
    KeyDef {
        keys: "s",
        zh: "设为默认",
        en: "set default",
        group: KeyGroup::Global,
        status: false,
        when: When::ViewSel(View::Settings),
        heat: 54,
    },
    // ── Task ──
    KeyDef {
        keys: "Enter",
        zh: "组织/编辑",
        en: "organize",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
        heat: 100,
    },
    KeyDef {
        keys: "x",
        zh: "完成",
        en: "done",
        group: KeyGroup::Task,
        status: true,
        when: When::StatusNot(&[Status::Done]),
        heat: 98,
    },
    KeyDef {
        keys: "s",
        zh: "将来",
        en: "someday",
        group: KeyGroup::Task,
        status: true,
        when: When::StatusNot(&[Status::Someday, Status::Done]),
        heat: 96,
    },
    KeyDef {
        keys: "w",
        zh: "等待",
        en: "waiting",
        group: KeyGroup::Task,
        status: true,
        when: When::StatusNot(&[Status::Waiting, Status::Done]),
        heat: 94,
    },
    KeyDef {
        keys: "T",
        zh: "批量标签",
        en: "bulk tag",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
        heat: 76,
    },
    KeyDef {
        keys: "e",
        zh: "编辑",
        en: "edit",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
        heat: 92,
    },
    KeyDef {
        keys: "n",
        zh: "备注",
        en: "notes",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
        heat: 88,
    },
    KeyDef {
        keys: "C",
        zh: "检查单",
        en: "checklist",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
        heat: 86,
    },
    KeyDef {
        keys: "A/D",
        zh: "归档",
        en: "archive",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
        heat: 88,
    },
    KeyDef {
        keys: "u",
        zh: "恢复",
        en: "restore",
        group: KeyGroup::Task,
        status: false,
        when: When::ViewSel(View::Archived),
        heat: 82,
    },
    KeyDef {
        keys: "D",
        zh: "删除",
        en: "delete",
        group: KeyGroup::Task,
        status: false,
        when: When::ViewSel(View::Archived),
        heat: 78,
    },
    KeyDef {
        keys: "c",
        zh: "加标签",
        en: "add tag",
        group: KeyGroup::Task,
        status: false,
        when: When::View(View::Tags),
        heat: 90,
    },
    KeyDef {
        keys: "D",
        zh: "删标签",
        en: "delete tag",
        group: KeyGroup::Task,
        status: false,
        when: When::ViewSel(View::Tags),
        heat: 74,
    },
    KeyDef {
        keys: "R",
        zh: "下一步",
        en: "next step",
        group: KeyGroup::Task,
        status: false,
        when: When::Reviewing,
        heat: 96,
    },
    KeyDef {
        keys: "Space",
        zh: "切换选择",
        en: "toggle sel",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
        heat: 70,
    },
    KeyDef {
        keys: "Ctrl+a",
        zh: "全选",
        en: "select all",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
        heat: 60,
    },
    KeyDef {
        keys: "Ctrl+u",
        zh: "反选",
        en: "invert",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
        heat: 58,
    },
    KeyDef {
        keys: "=",
        zh: "勾选检查单",
        en: "tick checklist",
        group: KeyGroup::Task,
        status: false,
        when: When::HasChecklist,
        heat: 64,
    },
    KeyDef {
        keys: "Tab",
        zh: "检查单管理",
        en: "manage checklist",
        group: KeyGroup::Task,
        status: false,
        when: When::HasChecklist,
        heat: 62,
    },
    KeyDef {
        keys: "P",
        zh: "专注/续杯",
        en: "focus/continue",
        group: KeyGroup::Task,
        status: true,
        when: When::SelectionNot(NON_TASK_VIEWS),
        heat: 66,
    },
    KeyDef {
        keys: "S",
        zh: "停止",
        en: "stop",
        group: KeyGroup::Task,
        status: false,
        when: When::PomoActive,
        heat: 62,
    },
    KeyDef {
        keys: "[",
        zh: "番茄设置",
        en: "pomo cfg",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
        heat: 50,
    },
    KeyDef {
        keys: "\"",
        zh: "加入金句",
        en: "to quote",
        group: KeyGroup::Task,
        status: false,
        when: When::QuoteAdd,
        heat: 64,
    },
    KeyDef {
        keys: "\"",
        zh: "移出金句",
        en: "unquote",
        group: KeyGroup::Task,
        status: false,
        when: When::QuoteRemove,
        heat: 64,
    },
];

/// 状态栏快捷键条：`status=true` 的条目，压缩展示，按热度降序。
pub(crate) fn status_strip(lang: Lang) -> Vec<(&'static str, &'static str)> {
    let mut v: Vec<(&KeyDef, u8)> = KEY_TABLE
        .iter()
        .filter(|k| k.status)
        .map(|k| (k, k.heat))
        .collect();
    v.sort_by_key(|b| std::cmp::Reverse(b.1));
    v.into_iter().map(|(k, _)| (k.keys, k.desc(lang))).collect()
}

/// 引导栏动态条：Normal/Visual 显示按视图精选的任务操作键；输入/确认模式显示模式键。
pub(crate) fn strip_keys(c: &Ctx, lang: Lang) -> Vec<(&'static str, &'static str)> {
    if c.mode != Mode::Normal && c.mode != Mode::Visual {
        return mode_keys(c.mode, lang);
    }
    view_task_keys(c, lang)
}

/// 引导栏动态任务键：枚举全部对当前活动任务可用的 Task 组快捷键，
/// 追加少量上下文相关的全局键（多选 v、周回顾 r），按热度降序展示。
fn view_task_keys(c: &Ctx, lang: Lang) -> Vec<(&'static str, &'static str)> {
    // 周回顾向导进行中，只提示下一步。
    if c.is_reviewing {
        return vec![("R", lang.tr("下一步", "next step"))];
    }

    let mut picked: Vec<&KeyDef> = KEY_TABLE
        .iter()
        .filter(|k| k.group == KeyGroup::Task && k.applies(c))
        .collect();

    // 上下文相关的全局键：v（多选）在归档箱或任务视图有选中时提示；r（周回顾）仅 Review 视图。
    if c.view == View::Archived || (c.has_selection && !NON_TASK_VIEWS.contains(&c.view)) {
        if let Some(k) = KEY_TABLE
            .iter()
            .find(|k| k.group == KeyGroup::Global && k.keys == "v" && k.applies(c))
        {
            picked.push(k);
        }
    }
    if c.view == View::Review {
        if let Some(k) = KEY_TABLE
            .iter()
            .find(|k| k.group == KeyGroup::Global && k.keys == "r" && k.applies(c))
        {
            picked.push(k);
        }
    }
    // 捕获键 a：任何视图都可新建任务，常驻提示。
    if let Some(k) = KEY_TABLE
        .iter()
        .find(|k| k.group == KeyGroup::Global && k.keys == "a" && k.applies(c))
    {
        picked.push(k);
    }

    // 按热度降序；同一物理键只保留最高热度的定义。
    picked.sort_by_key(|b| std::cmp::Reverse(b.heat));
    picked.dedup_by(|a, b| a.keys == b.keys);

    picked.iter().map(|k| (k.keys, k.desc(lang))).collect()
}

/// F1 面板全量行：按分组顺序（全局→任务），组内按热度降序，携带“当前是否可用”。
pub(crate) fn help_rows(c: &Ctx, lang: Lang) -> Vec<(KeyGroup, &'static str, &'static str, bool)> {
    GROUP_ORDER
        .iter()
        .flat_map(|g| {
            let mut v: Vec<&KeyDef> = KEY_TABLE.iter().filter(|k| k.group == *g).collect();
            v.sort_by_key(|b| std::cmp::Reverse(b.heat));
            v.into_iter()
                .map(|k| (*g, k.keys, k.desc(lang), k.applies(c)))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// 输入/确认模式下的动态键（用于引导栏动态条）。
/// 检查单逐项管理（ChecklistFocus）的键也在这里：该模式下 strip_keys 不走 KEY_TABLE。
fn mode_keys(mode: Mode, lang: Lang) -> Vec<(&'static str, &'static str)> {
    use Mode::*;
    let mut v: Vec<(&'static str, &'static str)> = Vec::new();
    match mode {
        ConfirmArchive | ConfirmPurge | ConfirmProfileDelete => {
            v.push(("y/Enter", lang.tr("确认", "confirm")));
            v.push(("n/Esc", lang.tr("取消", "cancel")));
        }
        Capturing | Tagging | FilteringTag => {
            v.push(("Enter", lang.tr("保存", "save")));
            v.push(("Esc", lang.tr("取消", "cancel")));
            v.push(("Tab", lang.tr("补全", "complete")));
        }
        Search => {
            v.push(("Enter", lang.tr("搜索", "search")));
            v.push(("Esc", lang.tr("清除", "clear")));
        }
        ChecklistFocus => {
            v.push(("j/k", lang.tr("移动光标", "move cursor")));
            v.push(("Space", lang.tr("勾选/取消", "tick / untick")));
            v.push(("d", lang.tr("删除项", "delete item")));
            v.push(("J/K", lang.tr("上下排序", "reorder")));
            v.push(("e", lang.tr("改名", "rename")));
            v.push(("Tab/Esc", lang.tr("退出管理", "exit manage")));
        }
        _ => {
            v.push(("Enter", lang.tr("确认", "confirm")));
            v.push(("Esc", lang.tr("取消", "cancel")));
        }
    }
    v
}
