use crate::tui::app::App;
use crate::tui::icons::Icon;
use horae_core::model::pomodoro::Phase;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    symbols::Marker,
    text::{Line, Span},
    widgets::{
        canvas::{Canvas, Points},
        Block, Paragraph,
    },
    Frame,
};

/// 专注模式响应式三档断点
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ZenTier {
    /// 宽屏/全屏大尺寸：沉浸式悬浮居中卡片，带圆角边框与呼吸光泽
    Expanded,
    /// 标准尺寸（如 80x24）：紧凑全屏流式布局，动态平衡圆环与倒计时
    Balanced,
    /// 极小/分屏尺寸（矮窗口或极窄窗口）：高精度平滑进度条，精炼倒计时与操作栏
    Compact,
}

impl ZenTier {
    pub(crate) fn from_area(area: Rect) -> Self {
        if area.width >= 75 && area.height >= 28 {
            ZenTier::Expanded
        } else if area.width >= 50 && area.height >= 18 {
            ZenTier::Balanced
        } else {
            ZenTier::Compact
        }
    }
}

impl<'a> App<'a> {
    /// 番茄钟专注模式全屏渲染入口（支持全分辨率响应式适配）
    pub(crate) fn render_focus_view(&self, f: &mut Frame, area: Rect) {
        let pomo = &self.pomo;
        let now = horae_core::time::now_ms();

        // ── 1. 基础时间计算 ──
        let start_ts = pomo.start_ts.unwrap_or(now);
        let end_ts = pomo.end_ts.unwrap_or(now);
        let total_ms = (end_ts - start_ts).max(1) as f64;
        let elapsed_fraction = ((now - start_ts) as f64 / total_ms).clamp(0.0, 1.0);

        let diff_secs = ((end_ts - now) / 1000).max(0);
        let mins = diff_secs / 60;
        let secs = diff_secs % 60;
        let time_str = format!("{:02}:{:02}", mins, secs);

        // ── 2. 阶段与主题配色（遵循 Catppuccin 调性） ──
        let (phase_badge, ring_color, dim_color, bg_color) = match &pomo.phase {
            Phase::Work => {
                let color = if self.theme.is_dark {
                    Color::Rgb(243, 139, 168) // Red
                } else {
                    Color::Rgb(210, 15, 57)
                };
                (
                    tr!(self.lang, "🍅 专注中", "🍅 Focus"),
                    color,
                    mix_toward(self.theme.bg, color, 0.45),
                    self.theme.bg,
                )
            }
            Phase::ShortBreak => {
                let color = if self.theme.is_dark {
                    Color::Rgb(166, 227, 161) // Green
                } else {
                    Color::Rgb(64, 160, 43)
                };
                (
                    tr!(self.lang, "☕ 小休时间", "☕ Short break"),
                    color,
                    mix_toward(self.theme.bg, color, 0.45),
                    self.theme.bg,
                )
            }
            Phase::LongBreak => {
                let color = if self.theme.is_dark {
                    Color::Rgb(148, 226, 213) // Teal
                } else {
                    Color::Rgb(23, 146, 153)
                };
                (
                    tr!(self.lang, "🌿 长休放松", "🌿 Long break"),
                    color,
                    mix_toward(self.theme.bg, color, 0.45),
                    self.theme.bg,
                )
            }
            Phase::Idle => return,
        };

        // ── 3. 关联任务与检查单状态 ──
        let active_task = pomo
            .task_id
            .as_deref()
            .and_then(|tid| horae_core::repo::tasks::get(self.conn, tid).ok());

        let raw_title =
            pomo.task_title
                .as_deref()
                .unwrap_or(tr!(self.lang, "专注无标题", "Untitled focus"));

        let has_checklist = active_task
            .as_ref()
            .map(|t| !t.checklist.is_empty())
            .unwrap_or(false);

        let tier = ZenTier::from_area(area);

        match tier {
            ZenTier::Expanded => self.render_focus_expanded(
                f,
                area,
                phase_badge,
                raw_title,
                &active_task,
                has_checklist,
                elapsed_fraction,
                &time_str,
                secs,
                ring_color,
                dim_color,
                bg_color,
            ),
            ZenTier::Balanced => self.render_focus_balanced(
                f,
                area,
                phase_badge,
                raw_title,
                &active_task,
                has_checklist,
                elapsed_fraction,
                &time_str,
                secs,
                ring_color,
                dim_color,
                bg_color,
            ),
            ZenTier::Compact => self.render_focus_compact(
                f,
                area,
                phase_badge,
                raw_title,
                &active_task,
                has_checklist,
                elapsed_fraction,
                &time_str,
                secs,
                ring_color,
                dim_color,
                bg_color,
            ),
        }
    }

    /// Tier 1: 沉浸式大屏悬浮卡片模式
    #[allow(clippy::too_many_arguments)]
    fn render_focus_expanded(
        &self,
        f: &mut Frame,
        area: Rect,
        phase_badge: &str,
        raw_title: &str,
        active_task: &Option<horae_core::model::task::Task>,
        has_checklist: bool,
        elapsed_fraction: f64,
        time_str: &str,
        secs: i64,
        ring_color: Color,
        dim_color: Color,
        bg_color: Color,
    ) {
        // 背景全屏清除与极简底色
        f.render_widget(ratatui::widgets::Clear, area);
        f.render_widget(Block::default().style(Style::default().bg(bg_color)), area);

        // 黄金比例居中视窗容器（无边框纯净设计，聚焦居中）
        let card_w = (area.width * 78 / 100)
            .clamp(72, 88)
            .min(area.width.saturating_sub(4));
        let card_h = (area.height * 78 / 100)
            .clamp(24, 32)
            .min(area.height.saturating_sub(2));
        let inner = center_rect_exact(card_w, card_h, area);

        let checklist_height = if has_checklist { 2 } else { 0 };

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2),                // 标题栏与阶段徽章
                Constraint::Min(8),                   // Canvas 圆环
                Constraint::Length(6),                // 5 行大数字 + 1 行间隔
                Constraint::Length(checklist_height), // 检查单卡片
                Constraint::Length(1),                // 统计
                Constraint::Length(1),                // 快捷键提示
            ])
            .split(inner);

        // 1. 顶部标题与阶段徽章
        self.render_header(f, rows[0], phase_badge, raw_title, ring_color, bg_color);

        // 2. 正圆 Braille 进度环
        render_braille_ring(
            f,
            rows[1],
            elapsed_fraction,
            ring_color,
            dim_color,
            bg_color,
            self.theme.text_dim,
        );

        // 3. 5行大数字倒计时
        let blink = secs % 2 == 0;
        let big_lines = build_big_time(time_str, ring_color, bg_color, blink);
        f.render_widget(
            Paragraph::new(big_lines)
                .alignment(Alignment::Center)
                .style(Style::default().bg(bg_color)),
            rows[2],
        );

        // 4. 检查单卡片
        if has_checklist {
            self.render_checklist_card(f, rows[3], active_task, ring_color, bg_color);
        }

        // 5. 统计栏与底部快捷键
        self.render_stats(f, rows[4], bg_color);
        self.render_hints(f, rows[5], has_checklist, bg_color);
    }

    /// Tier 2: 标准均衡模式（中屏 / 80x24 经典尺寸）
    #[allow(clippy::too_many_arguments)]
    fn render_focus_balanced(
        &self,
        f: &mut Frame,
        area: Rect,
        phase_badge: &str,
        raw_title: &str,
        active_task: &Option<horae_core::model::task::Task>,
        has_checklist: bool,
        elapsed_fraction: f64,
        time_str: &str,
        secs: i64,
        ring_color: Color,
        dim_color: Color,
        bg_color: Color,
    ) {
        f.render_widget(ratatui::widgets::Clear, area);
        f.render_widget(Block::default().style(Style::default().bg(bg_color)), area);

        // 根据高度动态适配倒计时字体与检查单高度
        let use_medium_digits = area.height < 24;
        let digits_height = if use_medium_digits { 4 } else { 6 };
        let show_checklist = has_checklist && area.height >= 20;
        let cl_height = if show_checklist { 1 } else { 0 };

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),             // 顶部留白或轻量垫高
                Constraint::Length(2),             // 任务标题栏
                Constraint::Min(6),                // Canvas 正圆环
                Constraint::Length(digits_height), // 倒计时（中字体或大字体）
                Constraint::Length(cl_height),     // 检查单卡片
                Constraint::Length(1),             // 统计
                Constraint::Length(1),             // 操作提示
            ])
            .split(area);

        // 1. 任务标题栏
        self.render_header(f, rows[1], phase_badge, raw_title, ring_color, bg_color);

        // 2. 正圆 Braille 进度环
        render_braille_ring(
            f,
            rows[2],
            elapsed_fraction,
            ring_color,
            dim_color,
            bg_color,
            self.theme.text_dim,
        );

        // 3. 倒计时数字
        let blink = secs % 2 == 0;
        if use_medium_digits {
            let med_lines = build_medium_time(time_str, ring_color, bg_color, blink);
            f.render_widget(
                Paragraph::new(med_lines)
                    .alignment(Alignment::Center)
                    .style(Style::default().bg(bg_color)),
                rows[3],
            );
        } else {
            let big_lines = build_big_time(time_str, ring_color, bg_color, blink);
            f.render_widget(
                Paragraph::new(big_lines)
                    .alignment(Alignment::Center)
                    .style(Style::default().bg(bg_color)),
                rows[3],
            );
        }

        // 4. 检查单行
        if show_checklist {
            self.render_checklist_card(f, rows[4], active_task, ring_color, bg_color);
        }

        // 5. 统计与操作提示
        self.render_stats(f, rows[5], bg_color);
        self.render_hints(f, rows[6], has_checklist, bg_color);
    }

    /// Tier 3: 极简分屏模式（矮终端 / 窄终端平滑降级）
    #[allow(clippy::too_many_arguments)]
    fn render_focus_compact(
        &self,
        f: &mut Frame,
        area: Rect,
        phase_badge: &str,
        raw_title: &str,
        active_task: &Option<horae_core::model::task::Task>,
        has_checklist: bool,
        elapsed_fraction: f64,
        time_str: &str,
        secs: i64,
        ring_color: Color,
        _dim_color: Color,
        bg_color: Color,
    ) {
        f.render_widget(ratatui::widgets::Clear, area);
        f.render_widget(Block::default().style(Style::default().bg(bg_color)), area);

        let blink = secs % 2 == 0;
        let show_medium_digits = area.height >= 12 && area.width >= 35;
        let show_checklist = has_checklist && area.height >= 10;

        let digits_h = if show_medium_digits { 3 } else { 1 };
        let cl_h = if show_checklist { 1 } else { 0 };

        let rows = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(1),        // 标题行
                Constraint::Length(digits_h), // 倒计时
                Constraint::Length(1),        // 高精度平滑进度条
                Constraint::Length(cl_h),     // 检查单
                Constraint::Length(1),        // 极简操作按键
            ])
            .split(area);

        // 1. 标题行
        let max_title_len = (area.width.saturating_sub(18) as usize).max(8);
        let truncated_title = truncate_str(raw_title, max_title_len);
        let title_line = Line::from(vec![
            Span::styled(
                format!(" {} ", phase_badge),
                Style::default().fg(ring_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(truncated_title, Style::default().fg(self.theme.fg)),
        ]);
        f.render_widget(
            Paragraph::new(title_line)
                .alignment(Alignment::Center)
                .style(Style::default().bg(bg_color)),
            rows[0],
        );

        // 2. 倒计时数字
        if show_medium_digits {
            let med_lines = build_medium_time(time_str, ring_color, bg_color, blink);
            f.render_widget(
                Paragraph::new(med_lines)
                    .alignment(Alignment::Center)
                    .style(Style::default().bg(bg_color)),
                rows[1],
            );
        } else {
            let single_time = Line::from(Span::styled(
                format!("── {} ──", time_str),
                Style::default().fg(ring_color).add_modifier(Modifier::BOLD),
            ));
            f.render_widget(
                Paragraph::new(single_time)
                    .alignment(Alignment::Center)
                    .style(Style::default().bg(bg_color)),
                rows[1],
            );
        }

        // 3. 高精度平滑进度条
        let bar_width = (area.width.saturating_sub(12) as usize).clamp(10, 48);
        let percent = (elapsed_fraction * 100.0).round() as u32;
        let (filled_bar, empty_bar) = build_smooth_bar(elapsed_fraction, bar_width);

        let progress_line = Line::from(vec![
            Span::styled(
                filled_bar,
                Style::default().fg(ring_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(empty_bar, Style::default().fg(self.theme.border_inactive)),
            Span::styled(
                format!(" {:2}%", percent),
                Style::default().fg(self.theme.text_dim),
            ),
        ]);
        f.render_widget(
            Paragraph::new(progress_line)
                .alignment(Alignment::Center)
                .style(Style::default().bg(bg_color)),
            rows[2],
        );

        // 4. 检查单子项
        if show_checklist {
            self.render_checklist_card(f, rows[3], active_task, ring_color, bg_color);
        }

        // 5. 极简快捷键提示（确保即使在分屏下也绝不丢失退出或完成键）
        let compact_hints = if matches!(self.pomo.phase, Phase::ShortBreak | Phase::LongBreak) {
            tr!(
                self.lang,
                "[Space] 下一轮  |  [S] 停止",
                "[Space] next round  |  [S] stop"
            )
        } else {
            tr!(self.lang, "[x] 完成  |  [S] 停止", "[x] done  |  [S] stop")
        };
        f.render_widget(
            Paragraph::new(compact_hints)
                .alignment(Alignment::Center)
                .style(Style::default().fg(self.theme.border_inactive).bg(bg_color)),
            rows[4],
        );
    }

    /// 渲染顶部标题与徽章
    fn render_header(
        &self,
        f: &mut Frame,
        area: Rect,
        phase_badge: &str,
        raw_title: &str,
        ring_color: Color,
        bg_color: Color,
    ) {
        let max_title_w = (area.width.saturating_sub(24) as usize).max(10);
        let display_title = truncate_str(raw_title, max_title_w);

        let title_line = Line::from(vec![
            Span::styled(
                format!(" {} ", phase_badge),
                Style::default().fg(ring_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(" │ ", Style::default().fg(self.theme.border_inactive)),
            Span::styled(
                display_title,
                Style::default()
                    .fg(self.theme.fg)
                    .add_modifier(Modifier::BOLD),
            ),
        ]);

        f.render_widget(
            Paragraph::new(title_line)
                .alignment(Alignment::Center)
                .style(Style::default().bg(bg_color)),
            area,
        );
    }

    /// 渲染检查单步骤卡片
    fn render_checklist_card(
        &self,
        f: &mut Frame,
        area: Rect,
        active_task: &Option<horae_core::model::task::Task>,
        ring_color: Color,
        bg_color: Color,
    ) {
        if let Some(ref task) = active_task {
            let total = task.checklist.len();
            if total == 0 {
                return;
            }
            let done = task.checklist.iter().filter(|i| i.done).count();
            let next_item = task.checklist.iter().find(|i| !i.done);

            let filled = (done * 5).checked_div(total).unwrap_or(0).min(5);
            let empty = 5 - filled;
            let bar = format!(
                "[{}{}] {}/{}",
                "■".repeat(filled),
                "□".repeat(empty),
                done,
                total
            );

            let max_item_w = (area.width.saturating_sub(bar.len() as u16 + 10) as usize).max(6);
            let item_text = if let Some(item) = next_item {
                format!("▶ [ ] {}", truncate_str(&item.title, max_item_w))
            } else {
                tr!(self.lang, "✓ 所有子项已达成", "✓ All steps completed").to_string()
            };

            let cl_line = Line::from(vec![
                Span::styled(
                    bar,
                    Style::default()
                        .fg(if done == total {
                            self.theme.text_success
                        } else {
                            ring_color
                        })
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
                Span::styled(
                    item_text,
                    Style::default()
                        .fg(self.theme.fg)
                        .add_modifier(Modifier::BOLD),
                ),
            ]);

            f.render_widget(
                Paragraph::new(cl_line)
                    .alignment(Alignment::Center)
                    .style(Style::default().bg(bg_color)),
                area,
            );
        }
    }

    /// 渲染统计信息
    fn render_stats(&self, f: &mut Frame, area: Rect, bg_color: Color) {
        let stats_line = Line::from(vec![
            Span::styled(
                tr!(
                    self.lang,
                    " 🏆 今日完成: {} ",
                    " 🏆 Today: {} ",
                    self.pomo.today_count
                ),
                Style::default().fg(self.theme.text_dim),
            ),
            Span::styled(" • ", Style::default().fg(self.theme.border_inactive)),
            Span::styled(
                tr!(
                    self.lang,
                    " 🔥 连击: {} ",
                    " 🔥 Streak: {} ",
                    self.pomo.streak
                ),
                Style::default().fg(self.theme.text_dim),
            ),
        ]);

        f.render_widget(
            Paragraph::new(stats_line)
                .alignment(Alignment::Center)
                .style(Style::default().bg(bg_color)),
            area,
        );
    }

    /// 渲染操作按键提示
    fn render_hints(&self, f: &mut Frame, area: Rect, has_checklist: bool, bg_color: Color) {
        let hints = if matches!(self.pomo.phase, Phase::ShortBreak | Phase::LongBreak) {
            tr!(
                self.lang,
                "{} [Space/P] 下一轮  |  [S] 结束专注",
                "{} [Space/P] next round  |  [S] end focus",
                self.icon(Icon::Active)
            )
        } else if has_checklist {
            tr!(
                self.lang,
                "{} [Space/=] 打卡子项  |  [x] 完成任务  |  [S] 停止番茄钟",
                "{} [Space/=] Tick step  |  [x] Complete task  |  [S] Stop pomodoro",
                self.icon(Icon::Active)
            )
        } else {
            tr!(
                self.lang,
                "{} [x] 完成任务  |  [S] 停止番茄钟",
                "{} [x] Complete task  |  [S] Stop pomodoro",
                self.icon(Icon::Active)
            )
        };

        f.render_widget(
            Paragraph::new(hints)
                .alignment(Alignment::Center)
                .style(Style::default().fg(self.theme.border_inactive).bg(bg_color)),
            area,
        );
    }
}

/// 几何纠偏后的正圆 Braille Canvas 进度环
fn render_braille_ring(
    f: &mut Frame,
    area: Rect,
    elapsed_fraction: f64,
    ring_color: Color,
    dim_color: Color,
    bg_color: Color,
    tick_color: Color,
) {
    if area.width < 4 || area.height < 3 {
        return;
    }

    let cw = area.width as f64;
    let ch = area.height as f64;

    // Braille 点阵分辨率：每个字符宽 2 点，高 4 点。
    // 终端字符物理像素比例通常为 1宽 : 2高。
    // 因此在 dot 坐标系中，1水平点物理尺寸约为 1垂直点物理尺寸，即物理点阵为正方形。
    let w_dots = cw * 2.0;
    let h_dots = ch * 4.0;

    let cx = w_dots / 2.0;
    let cy = h_dots / 2.0;

    // 最大正圆物理半径（单位：dot），保留 10% 边距留给外围刻度
    let max_radius = (cx.min(cy) * 0.86).max(2.0);
    let outer_r = max_radius;
    let inner_r = (max_radius * 0.72).max(1.0);

    let ef = elapsed_fraction;

    let canvas = Canvas::default()
        .marker(Marker::Braille)
        .x_bounds([0.0, w_dots])
        .y_bounds([0.0, h_dots])
        .background_color(bg_color)
        .paint(move |ctx| {
            let steps = 720_usize;
            let ring_steps = ((outer_r - inner_r) * 2.0) as usize + 1;

            let mut rem_pts = Vec::with_capacity(steps * (ring_steps + 1));
            let mut ela_pts = Vec::with_capacity(steps * (ring_steps + 1));

            for i in 0..steps {
                let angle_deg = i as f64 * 360.0 / steps as f64;
                // 从 12 点钟顺时针旋转
                let angle_rad = (90.0_f64 - angle_deg).to_radians();
                let frac = angle_deg / 360.0;

                let cos_val = angle_rad.cos();
                let sin_val = angle_rad.sin();

                for ri in 0..=ring_steps {
                    let r = inner_r + (ri as f64 * (outer_r - inner_r) / ring_steps as f64);
                    let x = cx + r * cos_val;
                    let y = cy + r * sin_val;

                    if x < 0.0 || x >= w_dots || y < 0.0 || y >= h_dots {
                        continue;
                    }

                    if frac >= ef {
                        rem_pts.push((x, y));
                    } else {
                        ela_pts.push((x, y));
                    }
                }
            }

            // 绘制已过去时间（浅暗色）
            ctx.draw(&Points {
                coords: &ela_pts,
                color: dim_color,
            });
            // 绘制剩余时间（主题明亮色）
            ctx.draw(&Points {
                coords: &rem_pts,
                color: ring_color,
            });

            // 12 点钟对称刻度
            let mut tick_pts = Vec::with_capacity(48);
            for t in 0..12 {
                let deg = t as f64 * 30.0;
                let rad = (90.0_f64 - deg).to_radians();
                let cos_t = rad.cos();
                let sin_t = rad.sin();
                for ri in 1..=3 {
                    let r = outer_r + 1.2 + (ri as f64 * 0.9);
                    let x = cx + r * cos_t;
                    let y = cy + r * sin_t;
                    if (0.0..w_dots).contains(&x) && (0.0..h_dots).contains(&y) {
                        tick_pts.push((x, y));
                    }
                }
            }
            ctx.draw(&Points {
                coords: &tick_pts,
                color: tick_color,
            });
        });

    f.render_widget(canvas, area);
}

/// 构建高精度平滑进度条（利用 ▏▎▍▌▋▊▉█ 8 级平滑字符）
pub(crate) fn build_smooth_bar(fraction: f64, width: usize) -> (String, String) {
    if width == 0 {
        return (String::new(), String::new());
    }

    let smooth_chars = [' ', '▏', '▎', '▍', '▌', '▋', '▊', '▉', '█'];
    let total_eighths = (fraction.clamp(0.0, 1.0) * (width as f64) * 8.0).round() as usize;
    let full_blocks = (total_eighths / 8).min(width);
    let remainder = total_eighths % 8;

    let mut filled = "█".repeat(full_blocks);
    let mut empty = String::new();

    if full_blocks < width {
        if remainder > 0 {
            filled.push(smooth_chars[remainder]);
            empty.push_str(&"░".repeat(width - full_blocks - 1));
        } else {
            empty.push_str(&"░".repeat(width - full_blocks));
        }
    }

    (filled, empty)
}

/// 3 行精致中型时间数字字模 (3x3 blocks + 1 space = 20 cols)
fn medium_digit_rows(c: char, blink: bool) -> [&'static str; 3] {
    match c {
        '0' => ["█▀█", "█ █", "█▄█"],
        '1' => [" ▄█", "  █", "  ▀"],
        '2' => ["▀▀█", "█▀▀", "███"],
        '3' => ["▀▀█", " ▀█", "██▀"],
        '4' => ["█ █", "▀▀█", "  ▀"],
        '5' => ["█▀▀", "▀▀█", "██▀"],
        '6' => ["█▀▀", "█▀█", "▀██"],
        '7' => ["▀▀█", "  █", "  ▀"],
        '8' => ["█▀█", "█▀█", "█▄█"],
        '9' => ["█▀█", "▀▀█", "  ▀"],
        ':' if blink => [" · ", "   ", " · "],
        ':' => ["   ", "   ", "   "],
        _ => ["   ", "   ", "   "],
    }
}

pub(crate) fn build_medium_time(
    s: &str,
    color: Color,
    bg: Color,
    blink: bool,
) -> Vec<Line<'static>> {
    let chars: Vec<char> = s.chars().collect();
    let mut rows: [String; 3] = Default::default();
    for &c in &chars {
        let digit = medium_digit_rows(c, blink);
        for (i, part) in digit.iter().enumerate() {
            rows[i].push_str(part);
            rows[i].push(' ');
        }
    }
    rows.into_iter()
        .map(|row| {
            Line::from(Span::styled(
                row,
                Style::default()
                    .fg(color)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ))
        })
        .collect()
}

/// 5 行标准饱满时间数字字模
fn big_digit_rows(c: char, blink: bool) -> [&'static str; 5] {
    match c {
        '0' => [" ██ ", "█  █", "█  █", "█  █", " ██ "],
        '1' => [" ▐█ ", " ██ ", "  █ ", "  █ ", " ███"],
        '2' => [" ██ ", "   █", " ██ ", "█   ", "████"],
        '3' => ["███ ", "   █", " ██ ", "   █", "███ "],
        '4' => ["█  █", "█  █", "████", "   █", "   █"],
        '5' => ["████", "█   ", "███ ", "   █", "███ "],
        '6' => [" ██ ", "█   ", "███ ", "█  █", " ██ "],
        '7' => ["████", "   █", "  █ ", " █  ", " █  "],
        '8' => [" ██ ", "█  █", " ██ ", "█  █", " ██ "],
        '9' => [" ██ ", "█  █", " ███", "   █", " ██ "],
        ':' if blink => ["    ", " ██ ", "    ", " ██ ", "    "],
        ':' => ["    ", "    ", "    ", "    ", "    "],
        _ => ["    ", "    ", "    ", "    ", "    "],
    }
}

pub(crate) fn build_big_time(s: &str, color: Color, bg: Color, blink: bool) -> Vec<Line<'static>> {
    let chars: Vec<char> = s.chars().collect();
    let mut rows: [String; 5] = Default::default();
    for &c in &chars {
        let digit = big_digit_rows(c, blink);
        for (i, part) in digit.iter().enumerate() {
            rows[i].push_str(part);
            rows[i].push(' ');
        }
    }
    rows.into_iter()
        .map(|row| {
            Line::from(Span::styled(
                row,
                Style::default()
                    .fg(color)
                    .bg(bg)
                    .add_modifier(Modifier::BOLD),
            ))
        })
        .collect()
}

/// 精确居中计算
fn center_rect_exact(w: u16, h: u16, r: Rect) -> Rect {
    let actual_w = w.min(r.width);
    let actual_h = h.min(r.height);
    let x = r.x + (r.width.saturating_sub(actual_w)) / 2;
    let y = r.y + (r.height.saturating_sub(actual_h)) / 2;
    Rect {
        x,
        y,
        width: actual_w,
        height: actual_h,
    }
}

/// 字符串带 Unicode 宽度的安全截断
fn truncate_str(s: &str, max_width: usize) -> String {
    use unicode_width::UnicodeWidthStr;
    if s.width() <= max_width {
        return s.to_string();
    }
    let mut cur_w = 0;
    let mut res = String::new();
    let target_w = max_width.saturating_sub(1);
    for c in s.chars() {
        let cw = unicode_width::UnicodeWidthChar::width(c).unwrap_or(0);
        if cur_w + cw > target_w {
            break;
        }
        cur_w += cw;
        res.push(c);
    }
    res.push('…');
    res
}

fn mix_toward(bg: Color, fg: Color, factor: f32) -> Color {
    let factor = factor.clamp(0.0, 1.0);
    match (bg, fg) {
        (Color::Rgb(br, bg_g, bb), Color::Rgb(fr, fg_g, fb)) => {
            let r = (br as f32 + (fr as f32 - br as f32) * factor).round() as u8;
            let g = (bg_g as f32 + (fg_g as f32 - bg_g as f32) * factor).round() as u8;
            let b = (bb as f32 + (fb as f32 - bb as f32) * factor).round() as u8;
            Color::Rgb(r, g, b)
        }
        _ => fg,
    }
}
