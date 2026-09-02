use crate::tui::app::{pad_right, App, Pane, View};
use crate::tui::icons::Icon;
use crate::tui::keys::{ctx_of, strip_keys};
use ratatui::{
    layout::Rect,
    style::{Color, Modifier, Style},
    text::{Line, Span},
};

impl<'a> App<'a> {
    /// 语法说明面板的行内容。
    pub(super) fn syntax_lines(&self) -> Vec<Line<'static>> {
        let s_hdr = Style::default()
            .fg(self.theme.accent)
            .add_modifier(Modifier::BOLD);
        let s_tok = Style::default()
            .fg(self.theme.text_success)
            .add_modifier(Modifier::BOLD);
        let s_dim = Style::default().fg(self.theme.text_dim);
        let s_hl = Style::default().fg(self.theme.hl_fg);
        let s_rrule = Style::default().fg(self.theme.rrule_fg);
        let s_urgent = Style::default()
            .fg(self.theme.text_urgent)
            .add_modifier(Modifier::BOLD);

        vec![
            // ── 1. 快速录入 ──
            Line::from(Span::styled(
                tr!(
                    self.lang,
                    "◈ 1. 快速录入核心语法 (按 a 录入 · 空格分词 · 顺序任意)",
                    "◈ 1. Quick Capture Syntax (Press a · Space-separated · Any order)"
                ),
                s_hdr,
            )),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<10}", tr!(self.lang, "@标签", "@tag")), s_tok),
                Span::raw(tr!(
                    self.lang,
                    "情境分类, 首次自动建档 (如 ",
                    "Context tag, auto-created (e.g. "
                )),
                Span::styled("@work", s_hl),
                Span::raw(" · "),
                Span::styled("@home", s_hl),
                Span::raw(" · "),
                Span::styled("@focus", s_hl),
                Span::raw(tr!(
                    self.lang,
                    " · Tab 补全高频优先)",
                    " · Tab completes hot tags first)"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<10}", tr!(self.lang, "!优先级", "!priority")),
                    s_tok,
                ),
                Span::raw(tr!(self.lang, "任务权重: ", "Task weight: ")),
                Span::styled("!high/!h/!1/!高", s_urgent),
                Span::raw(" · "),
                Span::styled(
                    "!medium/!m/!2/!中",
                    Style::default().fg(Color::Rgb(249, 226, 175)),
                ),
                Span::raw(" · "),
                Span::styled(
                    "!low/!l/!3/!低",
                    Style::default().fg(Color::Rgb(137, 180, 250)),
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<10}", tr!(self.lang, "~时间", "~time")), s_tok),
                Span::raw(tr!(
                    self.lang,
                    "排程起点 (进入已排程状态 · 软截止用 --due)",
                    "Scheduled start (enters Scheduled · soft deadline via --due)"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<10}", tr!(self.lang, "*循环", "*rrule")), s_tok),
                Span::raw(tr!(
                    self.lang,
                    "习惯与周期 (如 *2w[1,3] / *m[-1] / *weekday / *2d 等)",
                    "Recurrence (*2w[1,3] / *m[-1] / *weekday / *2d etc.)"
                )),
            ]),
            Line::from(vec![
                Span::styled(tr!(self.lang, "  示例: ", "  Examples: "), s_dim),
                Span::styled("买牛奶 @home ~tomorrow 18:00 !1", s_hl),
                Span::raw("  ·  "),
                Span::styled("站会 @work *weekday ~09:30 !h", s_hl),
            ]),
            Line::from(""),
            // ── 2. 时间排程 ──
            Line::from(Span::styled(
                tr!(
                    self.lang,
                    "◈ 2. 时间排程语法 (~ 排程起点 · 支持自然语言与拼音别名)",
                    "◈ 2. Time Syntax (~ Schedule start · Natural language & Aliases)"
                ),
                s_hdr,
            )),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<42}", "now / +15m / +30m / +1h / +1d / +1w"),
                    s_tok,
                ),
                Span::styled(tr!(self.lang, "相对时间偏移", "Relative offsets"), s_dim),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<42}", "+3d 15:30 / +1w 09:00"), s_tok),
                Span::styled(
                    tr!(self.lang, "相对偏移 + 指定时刻", "Relative offset + clock"),
                    s_dim,
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<42}", "today / tomorrow / 今天 / 明天 [HH:MM]"),
                    s_tok,
                ),
                Span::styled(
                    tr!(
                        self.lang,
                        "常用天词 (拼音 ~td/~tm 可补全)",
                        "Day words (~td/~tm to complete)"
                    ),
                    s_dim,
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<42}", "周一~周日 / 下周五 / mon~sun [HH:MM]"),
                    s_tok,
                ),
                Span::styled(
                    tr!(
                        self.lang,
                        "星期词汇 (拼音 ~zy/~mon 映射周一)",
                        "Weekdays (~zy/~mon maps Mon)"
                    ),
                    s_dim,
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<42}", "09:00 / 18:00 / HH:MM"), s_tok),
                Span::styled(
                    tr!(
                        self.lang,
                        "当日时刻 (已过则顺延至明日)",
                        "Same-day clock (next day if passed)"
                    ),
                    s_dim,
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    format!("{:<42}", "8/20 15:30 · 2026.8.20 · YYYY-MM-DD"),
                    s_tok,
                ),
                Span::styled(
                    tr!(
                        self.lang,
                        "灵活日期与绝对时刻",
                        "Flexible dates & absolute clock"
                    ),
                    s_dim,
                ),
            ]),
            Line::from(""),
            // ── 3. 周期规则 ──
            Line::from(Span::styled(
                tr!(
                    self.lang,
                    "◈ 3. 周期任务与循环规则 (Habit / RRULE · 支持数字动态推导)",
                    "◈ 3. Recurrence & Habit (RRULE · Dynamic inference)"
                ),
                s_hdr,
            )),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<38}", "*d / *w / *m / *y"), s_rrule),
                Span::styled(
                    tr!(
                        self.lang,
                        "基础周期: 每天 / 每周 / 每月 / 每年",
                        "Basic: daily / weekly / monthly / yearly"
                    ),
                    s_dim,
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<38}", "*weekday / *weekend"), s_rrule),
                Span::styled(
                    tr!(
                        self.lang,
                        "工作日 (周一至五) / 周末 (周六周日)",
                        "Workdays (Mon-Fri) / Weekend (Sat-Sun)"
                    ),
                    s_dim,
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<38}", "*2w[1,3] / *1w[mo,we]"), s_rrule),
                Span::styled(
                    tr!(
                        self.lang,
                        "每 2 周周一与周三 (1-7=周一至日, 0=周日)",
                        "Every 2 weeks Mon & Wed (1-7=Mon-Sun)"
                    ),
                    s_dim,
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<38}", "*m[1,15] / *m[1,-1] / *m[-1]"), s_rrule),
                Span::styled(
                    tr!(
                        self.lang,
                        "每月 1、15 号 / 每月 1 号与月末 / 月末最后一天",
                        "1st & 15th / 1st & last day / last day"
                    ),
                    s_dim,
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<38}", "*y[jan,jul] / *2y[6]"), s_rrule),
                Span::styled(
                    tr!(
                        self.lang,
                        "每年 1 月与 7 月 / 每两年 6 月",
                        "Yearly Jan & Jul / every 2 years in June"
                    ),
                    s_dim,
                ),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<38}", "*2 / *3 动态推导"), s_rrule),
                Span::styled(
                    tr!(
                        self.lang,
                        "输入数字动态推导对应周期 (*2d, *2w, *2m...)",
                        "Type number to infer *2d, *2w, *2m..."
                    ),
                    s_dim,
                ),
            ]),
            Line::from(""),
            // ── 4. 实时补全 ──
            Line::from(Span::styled(
                tr!(
                    self.lang,
                    "◈ 4. 自动补全与极速盲打 (Tab · 智能槽位引导)",
                    "◈ 4. Autocomplete & Fast Typing (Tab · Slot hints)"
                ),
                s_hdr,
            )),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<22}", "触发弹层"), s_tok),
                Span::raw(tr!(
                    self.lang,
                    "键入 @ / ~ / * / ! 自动唤出候选浮层 (大小写无关 & 拼音检索)",
                    "Type @ / ~ / * / ! to pop dropdown (case-insensitive & pinyin)"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<22}", "↓/↑ · Ctrl+N/P · Tab"), s_tok),
                Span::raw(tr!(
                    self.lang,
                    "上下循环切换候选；Tab / Ctrl+Y 采纳；Esc 取消",
                    "Cycle items; Tab to apply; Esc to dismiss"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<22}", "F7 风格切换"), s_tok),
                Span::raw(tr!(
                    self.lang,
                    "切换「语法参考模式」(丰富范式) 与「极速补全模式」(紧凑单列)",
                    "Toggle Reference Guide / Speed completion"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(format!("{:<22}", "智能槽位引导"), s_tok),
                Span::raw(tr!(
                    self.lang,
                    "输入标题后按空格，行末淡色显示未填写的 ",
                    "Type title + space to see ghost "
                )),
                Span::styled("[@标签] [~时间] [*周期] [!优先级]", s_dim),
            ]),
            Line::from(""),
            // ── 5. GTD 核心操作 ──
            Line::from(Span::styled(
                tr!(
                    self.lang,
                    "◈ 5. GTD 核心操作快捷键",
                    "◈ 5. GTD Core Actions & Keys"
                ),
                s_hdr,
            )),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("w", s_tok),
                Span::raw(tr!(self.lang, " 等待谁/何时  ·  ", " waiting for  ·  ")),
                Span::styled("C / Tab", s_tok),
                Span::raw(tr!(self.lang, " 检查单管理  ·  ", " checklist  ·  ")),
                Span::styled("/", s_tok),
                Span::raw(tr!(
                    self.lang,
                    " 搜索 (支持 4位日期如 0829)  ·  ",
                    " search (4-digit date e.g. 0829)  ·  "
                )),
                Span::styled("W", s_tok),
                Span::raw(tr!(self.lang, " GTD 决策树", " GTD workflow")),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("1~9", s_tok),
                Span::raw(tr!(self.lang, " 视图直达  ·  ", " views  ·  ")),
                Span::styled("J / K", s_tok),
                Span::raw(tr!(self.lang, " 今日/明日  ·  ", " today/tomorrow  ·  ")),
                Span::styled("r", s_tok),
                Span::raw(tr!(self.lang, " 周回顾  ·  ", " review  ·  ")),
                Span::styled("P", s_tok),
                Span::raw(tr!(self.lang, " 番茄专注  ·  ", " pomodoro  ·  ")),
                Span::styled("Ctrl+P", s_tok),
                Span::raw(tr!(self.lang, " 关闭本指南", " close guide")),
            ]),
        ]
    }

    /// 左侧引导栏内容：视图分组 + 动态快捷键（按剩余行数截断）。
    pub(super) fn guide_lines(&self, area: Rect) -> Vec<Line<'static>> {
        let mut lines: Vec<Line> = Vec::new();

        if self.total_count() == 0 {
            lines.push(Line::from(Span::styled(
                tr!(self.lang, " 欢迎使用 horae", "  Welcome to horae"),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )));
            lines.push(Line::from(""));
        }

        let cur = self.view;
        let is_left_pane = self.pane == Pane::Left;
        // 矮终端（如 80×24）收紧组间空行，把行数让给下方的 [Keys] 动态键。
        let spacious = area.height >= 30;

        let mut add_group = |views: &[(char, View)], title: String| {
            lines.push(Line::from(Span::styled(
                title,
                Style::default()
                    .fg(self.theme.text_dim)
                    .add_modifier(Modifier::BOLD),
            )));
            for (key, v) in views {
                let cnt = self.context_count(*v);
                let active = cur == *v;
                let (icon, label) = match v {
                    View::Inbox => (
                        self.icon(Icon::Inbox),
                        crate::tui::view_label(self.lang, View::Inbox),
                    ),
                    View::Today => (
                        self.icon(Icon::Today),
                        crate::tui::view_label(self.lang, View::Today),
                    ),
                    View::Tomorrow => (
                        self.icon(Icon::Tomorrow),
                        crate::tui::view_label(self.lang, View::Tomorrow),
                    ),
                    View::Next => (
                        self.icon(Icon::Next),
                        crate::tui::view_label(self.lang, View::Next),
                    ),
                    View::Waiting => (
                        self.icon(Icon::Waiting),
                        crate::tui::view_label(self.lang, View::Waiting),
                    ),
                    View::Scheduled => (
                        self.icon(Icon::Scheduled),
                        crate::tui::view_label(self.lang, View::Scheduled),
                    ),
                    View::Someday => (
                        self.icon(Icon::Someday),
                        crate::tui::view_label(self.lang, View::Someday),
                    ),
                    View::Reference => (
                        self.icon(Icon::Reference),
                        crate::tui::view_label(self.lang, View::Reference),
                    ),
                    View::Done => (
                        self.icon(Icon::Done),
                        crate::tui::view_label(self.lang, View::Done),
                    ),
                    View::Review => (
                        self.icon(Icon::Review),
                        crate::tui::view_label(self.lang, View::Review),
                    ),
                    View::Archived => (
                        self.icon(Icon::Archived),
                        crate::tui::view_label(self.lang, View::Archived),
                    ),
                    View::Tags => (
                        self.icon(Icon::Tags),
                        crate::tui::view_label(self.lang, View::Tags),
                    ),
                    View::Quotes => (
                        self.icon(Icon::Quotes),
                        crate::tui::view_label(self.lang, View::Quotes),
                    ),
                    View::Settings => (
                        self.icon(Icon::Settings),
                        crate::tui::view_label(self.lang, View::Settings),
                    ),
                    _ => ("", ""),
                };
                let padded_label = pad_right(label, 10);

                if active {
                    let mut style = Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD);
                    if is_left_pane {
                        style = style.add_modifier(Modifier::REVERSED);
                    }
                    lines.push(Line::from(Span::styled(
                        format!(
                            " {} {} {} {} {:>3} ",
                            self.icon(Icon::Active),
                            key,
                            icon,
                            padded_label,
                            cnt
                        ),
                        style,
                    )));
                } else {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("   {} ", key),
                            Style::default().fg(self.theme.text_dim),
                        ),
                        Span::styled(
                            format!("{} ", icon),
                            Style::default().add_modifier(Modifier::BOLD),
                        ),
                        Span::raw(format!("{} {:>3} ", padded_label, cnt)),
                    ]));
                }
            }
            if spacious {
                lines.push(Line::from(""));
            }
        };

        add_group(
            &[('J', View::Today), ('K', View::Tomorrow)],
            format!(" {} [Day] ⇧+", self.icon(Icon::GroupDay)),
        );
        add_group(
            &[('1', View::Inbox), ('2', View::Next)],
            format!(" {} [Active]", self.icon(Icon::GroupActive)),
        );
        add_group(
            &[
                ('3', View::Waiting),
                ('4', View::Scheduled),
                ('5', View::Someday),
            ],
            format!(" {} [Waiting]", self.icon(Icon::GroupWaiting)),
        );
        let mut archive_group = vec![];
        if self.modules.reference {
            archive_group.push(('6', View::Reference));
        }
        if self.modules.done {
            archive_group.push(('7', View::Done));
        }
        if !archive_group.is_empty() {
            add_group(
                &archive_group,
                format!(" {} [Archive]", self.icon(Icon::GroupArchive)),
            );
        }

        lines.push(Line::from(Span::styled(
            format!(" {} [Modules]", self.icon(Icon::GroupModules)),
            Style::default()
                .fg(self.theme.text_dim)
                .add_modifier(Modifier::BOLD),
        )));
        let mut mod_group = vec![];
        if self.modules.archived {
            mod_group.push(("8", View::Archived));
        }
        if self.modules.tags {
            mod_group.push(("9", View::Tags));
        }
        if self.quotes.enabled {
            mod_group.push(("0", View::Quotes));
        }
        if self.modules.review {
            mod_group.push(("r", View::Review));
        }
        if self.modules.settings {
            mod_group.push(("M", View::Settings));
        }
        mod_group.push(("W", View::Workflow));

        for (key, v) in mod_group {
            let active = cur == v;
            let (icon, label) = match v {
                View::Review => (
                    self.icon(Icon::Review),
                    crate::tui::view_label(self.lang, View::Review),
                ),
                View::Archived => (
                    self.icon(Icon::Archived),
                    crate::tui::view_label(self.lang, View::Archived),
                ),
                View::Tags => (
                    self.icon(Icon::Tags),
                    crate::tui::view_label(self.lang, View::Tags),
                ),
                View::Settings => (
                    self.icon(Icon::Settings),
                    crate::tui::view_label(self.lang, View::Settings),
                ),
                View::Quotes => (
                    self.icon(Icon::Quotes),
                    crate::tui::view_label(self.lang, View::Quotes),
                ),
                View::Workflow => (
                    self.icon(Icon::Workflow),
                    crate::tui::view_label(self.lang, View::Workflow),
                ),
                _ => ("", ""),
            };
            let padded_label = pad_right(label, 10);

            let cnt_str = if v == View::Quotes {
                format!("{:>3} ", self.context_count(View::Quotes))
            } else {
                "    ".to_string()
            };

            if active {
                let mut style = Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD);
                if is_left_pane {
                    style = style.add_modifier(Modifier::REVERSED);
                }
                lines.push(Line::from(Span::styled(
                    format!(
                        " {} {:>1} {} {} {}",
                        self.icon(Icon::Active),
                        key,
                        icon,
                        padded_label,
                        cnt_str
                    ),
                    style,
                )));
            } else {
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("   {:>1} ", key),
                        Style::default().fg(self.theme.text_dim),
                    ),
                    Span::styled(
                        format!("{} ", icon),
                        Style::default().add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!("{} {}", padded_label, cnt_str)),
                ]));
            }
        }
        if spacious {
            lines.push(Line::from(""));
        }

        // 动态快捷键：按当前视图/选择态/模式过滤，并严格按剩余行数截断。
        let rows_used = lines.len() as isize;
        let avail = area.height as isize - 2 - rows_used;
        let ctx = ctx_of(self);
        let keys = strip_keys(&ctx, self.lang);
        if avail >= 1 && !keys.is_empty() {
            lines.push(Line::from(Span::styled(
                format!(" {} [Keys]", self.icon(Icon::GroupKeys)),
                Style::default()
                    .fg(self.theme.text_dim)
                    .add_modifier(Modifier::BOLD),
            )));
            let mut budget = avail - 1;
            let mut shown = 0;
            for (k, desc) in &keys {
                if budget <= 0 {
                    break;
                }
                lines.push(Line::from(vec![
                    Span::styled(
                        format!("   {:>6} ", k),
                        Style::default().fg(self.theme.text_dim),
                    ),
                    Span::styled(*desc, Style::default().fg(self.theme.fg)),
                ]));
                shown += 1;
                budget -= 1;
            }
            if shown < keys.len() && budget >= 1 {
                lines.push(Line::from(Span::styled(
                    format!("   … {} 更多 (F1)", keys.len() - shown),
                    Style::default().fg(self.theme.text_dim),
                )));
            }
        }

        lines
    }
}
