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
        vec![
            Line::from(Span::styled(
                tr!(
                    self.lang,
                    "快速录入语法 (按 a 捕获)",
                    "Quick capture syntax (press a)"
                ),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    tr!(self.lang, "@标签", "@tag"),
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(tr!(
                    self.lang,
                    "    添加情境, 如 ",
                    "    add context, e.g. "
                )),
                Span::styled("@work", Style::default().fg(self.theme.accent)),
                Span::raw(tr!(self.lang, " (支持 Tab 补全)", " (Tab to complete)")),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    tr!(self.lang, "!优先级", "!priority"),
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(tr!(self.lang, "    设置优先级: ", "    set priority: ")),
                Span::styled("!a", Style::default().fg(self.theme.text_urgent)),
                Span::raw(tr!(self.lang, "(高) / ", "(high) / ")),
                Span::styled("!b", Style::default().fg(Color::Rgb(249, 226, 175))),
                Span::raw(tr!(self.lang, "(中) / ", "(medium) / ")),
                Span::styled("!c", Style::default().fg(Color::Rgb(137, 180, 250))),
                Span::raw(tr!(self.lang, "(低)", "(low)")),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    tr!(self.lang, "~时间", "~time"),
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(tr!(
                    self.lang,
                    "    设置截止时间, 见下方时间语法",
                    "    set due time, see below"
                )),
            ]),
            Line::from(vec![
                Span::raw(tr!(self.lang, "  例: ", "  examples: ")),
                Span::styled(
                    "a买牛奶 @home ~tomorrow",
                    Style::default().fg(self.theme.accent),
                ),
                Span::raw(" / "),
                Span::styled(
                    "a写周报 @work !a ~+3d",
                    Style::default().fg(self.theme.accent),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                tr!(
                    self.lang,
                    "时间语法 (~ 排程起点；日期搜索用 MMDD，如 0829)",
                    "Time syntax (~ schedule start; date search uses MMDD, e.g. 0829)"
                ),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "now / +2h +30m +1d +1w",
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(tr!(self.lang, "    相对时间偏移", "    relative offsets")),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "today / tomorrow [HH:MM]",
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(tr!(
                    self.lang,
                    "  今天/明天指定时刻",
                    "  today/tomorrow at a time"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("HH:MM", Style::default().fg(self.theme.text_success)),
                Span::raw(tr!(
                    self.lang,
                    "                     当天指定时刻, 如 18:00",
                    "                     same-day time, e.g. 18:00"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "YYYY-MM-DD [HH:MM]",
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(tr!(
                    self.lang,
                    "        绝对日期与时间",
                    "        absolute date & time"
                )),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                tr!(
                    self.lang,
                    "周期 / 循环任务 (Habit / RRULE)",
                    "Recurring / habit tasks (RRULE)"
                ),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw(tr!(self.lang, "  一句话排程: ", "  one-line schedule: ")),
                Span::styled("~明天 15:30", Style::default().fg(self.theme.accent)),
                Span::raw(tr!(
                    self.lang,
                    " 即可设排程起点, 循环任务再补 *rrule",
                    " sets the start time; append *rrule for habits"
                )),
            ]),
            Line::from(vec![
                Span::raw(tr!(self.lang, "  快速录入简写: ", "  quick shorthand: ")),
                Span::styled("*2w[1,3]", Style::default().fg(self.theme.rrule_fg)),
                Span::raw(tr!(
                    self.lang,
                    " = 每2周周一、周三  (星期用 1-7, 0=周日; 也可写 *mo,we)",
                    " = every 2 weeks Mon,Wed  (days 1-7, 0=Sun; or *mo,we)"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "FREQ=DAILY|WEEKLY|MONTHLY",
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(tr!(self.lang, "   循环频率", "   frequency")),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("INTERVAL=2", Style::default().fg(self.theme.text_success)),
                Span::raw(tr!(
                    self.lang,
                    "                  循环间隔 (如每 2 周)",
                    "                  interval (e.g. every 2 weeks)"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled("BYDAY=SA,SU", Style::default().fg(self.theme.text_success)),
                Span::raw(tr!(
                    self.lang,
                    "                 指定周几 (MO TU WE TH FR SA SU)",
                    "                 days of week (MO TU WE TH FR SA SU)"
                )),
            ]),
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    "COUNT=10 / UNTIL=YYYY-MM-DD",
                    Style::default().fg(self.theme.text_success),
                ),
                Span::raw(tr!(self.lang, " 终止条件", " end conditions")),
            ]),
            Line::from(vec![
                Span::raw(tr!(self.lang, "  例: ", "  examples: ")),
                Span::styled(
                    ";FREQ=WEEKLY;BYDAY=SA,SU",
                    Style::default().fg(self.theme.rrule_fg),
                ),
                Span::raw("    "),
                Span::styled(
                    ";FREQ=DAILY;COUNT=30",
                    Style::default().fg(self.theme.rrule_fg),
                ),
            ]),
            Line::from(""),
            Line::from(Span::styled(
                tr!(self.lang, "其他操作说明", "Other tips"),
                Style::default()
                    .fg(self.theme.accent)
                    .add_modifier(Modifier::BOLD),
            )),
            Line::from(vec![
                Span::raw(tr!(self.lang, "  等待 ", "  waiting ")),
                Span::styled(
                    "w",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(tr!(
                    self.lang,
                    " 后可填写 [谁/何时], 如 ",
                    " then set [who/when], e.g. "
                )),
                Span::styled("w → Alice → +1d", Style::default().fg(self.theme.accent)),
            ]),
            Line::from(vec![
                Span::raw(tr!(self.lang, "  检查单 ", "  checklist ")),
                Span::styled(
                    "C",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(tr!(self.lang, " 新增; ", " add; ")),
                Span::styled(
                    "Tab",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(tr!(
                    self.lang,
                    " 逐项管理: j/k 移动, Space 勾选, d 删除, J/K 排序, e 改名",
                    " manage: j/k move, Space tick, d delete, J/K reorder, e rename"
                )),
            ]),
            Line::from(vec![
                Span::raw(tr!(
                    self.lang,
                    "  标签库 (视图9): 按 ",
                    "  Tags (view 9): press "
                )),
                Span::styled(
                    "a",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(tr!(self.lang, " 动态新增, 按 ", " to add, ")),
                Span::styled(
                    "D",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(tr!(self.lang, " 删除", " to delete")),
            ]),
            Line::from(vec![
                Span::raw(tr!(self.lang, "  按 ", "  press ")),
                Span::styled(
                    "Ctrl+P",
                    Style::default()
                        .fg(self.theme.accent)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(tr!(
                    self.lang,
                    " 弹出/关闭本语法说明指南",
                    " to toggle this guide"
                )),
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
