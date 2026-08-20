//! 快捷键单一数据源：F1 快捷键面板、状态栏全局键条、引导栏动态任务键都从这里渲染。
//!
//! 设计要点：
//! - 分为两层：**全局键**（状态栏常驻、压缩显示）与**任务操作键**（引导栏 [Keys] 上下文驱动）。
//! - 每一条 `KeyDef` 描述一个按键在某个上下文里的含义（含视图/选择/周回顾/番茄钟约束）。
//! - 同一个物理键可有多个 `KeyDef`（如 `a` 在 Tags 视图=新增标签，其余=捕获任务）。
//! - `hjkl` 压缩为一条，等价于上下左右方向键。

use super::app::{Mode, View};
use crate::i18n::Lang;

/// 快捷键分区：`Global` 显示在状态栏（或仅 F1 参考），`Task` 显示在引导栏动态条。
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
    /// 番茄钟活动时。
    PomoActive,
    /// 番茄钟空闲时。
    PomoIdle,
}

/// 非任务视图：行不是任务，任务操作键不适用。
pub(crate) const NON_TASK_VIEWS: &[View] = &[View::Tags, View::Archived];

pub(crate) struct KeyDef {
    pub keys: &'static str,
    pub zh: &'static str,
    pub en: &'static str,
    pub group: KeyGroup,
    /// 是否显示在状态栏全局键条（视图切换键已由侧栏展示，不重复进条）。
    pub status: bool,
    pub when: When,
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
            PomoActive => c.pomo_active,
            PomoIdle => !c.pomo_active,
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
}

pub(crate) fn ctx_of(app: &super::App) -> Ctx {
    let pomo_active = app.pomo.phase != crate::model::pomodoro::Phase::Idle;
    Ctx {
        view: app.view,
        mode: app.mode,
        has_selection: app.selected < app.items.len(),
        is_reviewing: app.is_reviewing,
        pomo_active,
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
    },
    KeyDef {
        keys: "v",
        zh: "多选",
        en: "multi",
        group: KeyGroup::Global,
        status: true,
        when: When::Always,
    },
    KeyDef {
        keys: "g/G",
        zh: "首尾",
        en: "top/bot",
        group: KeyGroup::Global,
        status: false,
        when: When::Always,
    },
    KeyDef {
        keys: "1-9",
        zh: "切视图",
        en: "view",
        group: KeyGroup::Global,
        status: false,
        when: When::Always,
    },
    KeyDef {
        keys: "J/K",
        zh: "今日/明日",
        en: "today/tmrw",
        group: KeyGroup::Global,
        status: false,
        when: When::Always,
    },
    KeyDef {
        keys: "r",
        zh: "周回顾",
        en: "review",
        group: KeyGroup::Global,
        status: false,
        when: When::Always,
    },
    KeyDef {
        keys: "/",
        zh: "搜索",
        en: "search",
        group: KeyGroup::Global,
        status: true,
        when: When::General,
    },
    KeyDef {
        keys: "f",
        zh: "过滤",
        en: "filter",
        group: KeyGroup::Global,
        status: true,
        when: When::General,
    },
    KeyDef {
        keys: "a",
        zh: "捕获",
        en: "capture",
        group: KeyGroup::Global,
        status: true,
        when: When::General,
    },
    KeyDef {
        keys: "Esc",
        zh: "取消",
        en: "cancel",
        group: KeyGroup::Global,
        status: true,
        when: When::Always,
    },
    KeyDef {
        keys: "F1/?",
        zh: "帮助",
        en: "help",
        group: KeyGroup::Global,
        status: true,
        when: When::Always,
    },
    KeyDef {
        keys: "F2",
        zh: "快捷键条开关",
        en: "shortcut bar",
        group: KeyGroup::Global,
        status: false,
        when: When::Always,
    },
    KeyDef {
        keys: "Ctrl+P",
        zh: "语法",
        en: "syntax",
        group: KeyGroup::Global,
        status: false,
        when: When::Always,
    },
    KeyDef {
        keys: "F5",
        zh: "主题",
        en: "theme",
        group: KeyGroup::Global,
        status: true,
        when: When::Always,
    },
    KeyDef {
        keys: "F6",
        zh: "语言",
        en: "lang",
        group: KeyGroup::Global,
        status: true,
        when: When::Always,
    },
    KeyDef {
        keys: "q",
        zh: "退出",
        en: "quit",
        group: KeyGroup::Global,
        status: true,
        when: When::Always,
    },
    // ── Task ──
    KeyDef {
        keys: "Enter",
        zh: "组织/编辑",
        en: "organize",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
    },
    KeyDef {
        keys: "x",
        zh: "完成",
        en: "done",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
    },
    KeyDef {
        keys: "w",
        zh: "等待",
        en: "waiting",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
    },
    KeyDef {
        keys: "s",
        zh: "将来",
        en: "someday",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
    },
    KeyDef {
        keys: "T",
        zh: "批量标签",
        en: "bulk tag",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
    },
    KeyDef {
        keys: "e",
        zh: "编辑",
        en: "edit",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
    },
    KeyDef {
        keys: "n",
        zh: "备注",
        en: "notes",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
    },
    KeyDef {
        keys: "C",
        zh: "检查单",
        en: "checklist",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
    },
    KeyDef {
        keys: "A/D",
        zh: "归档",
        en: "archive",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
    },
    KeyDef {
        keys: "u",
        zh: "恢复",
        en: "restore",
        group: KeyGroup::Task,
        status: false,
        when: When::ViewSel(View::Archived),
    },
    KeyDef {
        keys: "D",
        zh: "删除",
        en: "delete",
        group: KeyGroup::Task,
        status: false,
        when: When::ViewSel(View::Archived),
    },
    KeyDef {
        keys: "c",
        zh: "加标签",
        en: "add tag",
        group: KeyGroup::Task,
        status: false,
        when: When::View(View::Tags),
    },
    KeyDef {
        keys: "D",
        zh: "删标签",
        en: "delete tag",
        group: KeyGroup::Task,
        status: false,
        when: When::ViewSel(View::Tags),
    },
    KeyDef {
        keys: "R",
        zh: "下一步",
        en: "next step",
        group: KeyGroup::Task,
        status: false,
        when: When::Reviewing,
    },
    KeyDef {
        keys: "Space",
        zh: "切换选择",
        en: "toggle sel",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
    },
    KeyDef {
        keys: "Ctrl+a",
        zh: "全选",
        en: "select all",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
    },
    KeyDef {
        keys: "Ctrl+u",
        zh: "反选",
        en: "invert",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
    },
    KeyDef {
        keys: "=",
        zh: "勾选检查单",
        en: "tick checklist",
        group: KeyGroup::Task,
        status: false,
        when: When::SelectionNot(NON_TASK_VIEWS),
    },
    KeyDef {
        keys: "P",
        zh: "专注/续杯",
        en: "focus/continue",
        group: KeyGroup::Task,
        status: false,
        when: When::PomoActive,
    },
    KeyDef {
        keys: "P",
        zh: "专注",
        en: "focus",
        group: KeyGroup::Task,
        status: false,
        when: When::PomoIdle,
    },
    KeyDef {
        keys: "S",
        zh: "停止",
        en: "stop",
        group: KeyGroup::Task,
        status: false,
        when: When::PomoActive,
    },
    KeyDef {
        keys: "[",
        zh: "番茄设置",
        en: "pomo cfg",
        group: KeyGroup::Task,
        status: false,
        when: When::Always,
    },
];

/// 状态栏全局键条：Global 且 `status=true` 的条目，压缩展示。
pub(crate) fn status_strip(lang: Lang) -> Vec<(&'static str, &'static str)> {
    KEY_TABLE
        .iter()
        .filter(|k| k.group == KeyGroup::Global && k.status)
        .map(|k| (k.keys, k.desc(lang)))
        .collect()
}

/// 引导栏动态条：Normal/Visual 显示按视图精选的任务操作键；输入/确认模式显示模式键。
pub(crate) fn strip_keys(c: &Ctx, lang: Lang) -> Vec<(&'static str, &'static str)> {
    if c.mode != Mode::Normal && c.mode != Mode::Visual {
        return mode_keys(c.mode, lang);
    }
    view_task_keys(c, lang)
}

/// 按视图精选少量高相关任务操作键；desc 从 KEY_TABLE 的 Task 组按标签+上下文取回。
fn view_task_keys(c: &Ctx, lang: Lang) -> Vec<(&'static str, &'static str)> {
    // 周回顾向导进行中，只提示下一步。
    let curated: &[&str] = if c.is_reviewing {
        &["R"]
    } else {
        match c.view {
            View::Inbox => &["Enter", "x", "e", "Space", "T"],
            View::Today | View::Tomorrow => &["Enter", "x"],
            View::Next => &["Enter", "x"],
            View::Waiting => &["w", "x"],
            View::Scheduled => &["Enter", "x"],
            View::Someday => &["s", "x"],
            View::Reference => &["e", "n"],
            View::Done => &["A/D", "e", "n"],
            // 周回顾视图里 R 只在回顾向导进行中生效，但向导不会停留在此视图；可执行的是 r（开启回顾）。
            View::Review => &["r"],
            View::Archived => &["u", "D", "Space", "v"],
            View::Tags => &["c", "D"],
        }
    };
    curated
        .iter()
        .filter_map(|label| {
            // 优先任务操作键；v（多选）/ r（周回顾）等全局键在特定视图同样相关，兜底查找。
            KEY_TABLE
                .iter()
                .find(|k| k.group == KeyGroup::Task && k.keys == *label && k.applies(c))
                .or_else(|| {
                    KEY_TABLE
                        .iter()
                        .find(|k| k.group == KeyGroup::Global && k.keys == *label && k.applies(c))
                })
        })
        .map(|k| (k.keys, k.desc(lang)))
        .collect()
}

/// F1 面板全量行：按分组顺序，携带“当前是否可用”。
pub(crate) fn help_rows(c: &Ctx, lang: Lang) -> Vec<(KeyGroup, &'static str, &'static str, bool)> {
    GROUP_ORDER
        .iter()
        .flat_map(|g| {
            KEY_TABLE
                .iter()
                .filter(move |k| k.group == *g)
                .map(|k| (*g, k.keys, k.desc(lang), k.applies(c)))
        })
        .collect()
}

/// 输入/确认模式下的动态键（用于引导栏动态条）。
fn mode_keys(mode: Mode, lang: Lang) -> Vec<(&'static str, &'static str)> {
    use Mode::*;
    let mut v: Vec<(&'static str, &'static str)> = Vec::new();
    match mode {
        ConfirmArchive | ConfirmPurge => {
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
        _ => {
            v.push(("Enter", lang.tr("确认", "confirm")));
            v.push(("Esc", lang.tr("取消", "cancel")));
        }
    }
    v
}
