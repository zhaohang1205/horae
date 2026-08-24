# horae — GTD 终端任务管理器 / GTD Terminal Task Manager

[![CI](https://github.com/zhaohang1205/horae/actions/workflows/ci.yml/badge.svg)](https://github.com/zhaohang1205/horae/actions/workflows/ci.yml)
[![Release](https://github.com/zhaohang1205/horae/actions/workflows/release.yml/badge.svg)](https://github.com/zhaohang1205/horae/actions/workflows/release.yml)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)

用 Rust 写成的 GTD 终端任务管理器：SQLite 数据层 + CLI + ratatui TUI 三合一。
A GTD terminal task manager in Rust: SQLite data layer + CLI + ratatui TUI in one binary.

核心设计是 **时间数据化（time-datafication）**：每次任务状态变更都打上 UTC 毫秒时间戳，
追加到只写(append-only)的 `task_events` 时间线，每个任务都带完整的履历。
Core idea: **time-datafication** — every state change is stamped with UTC-ms and appended to
an append-only `task_events` timeline, giving every task a full audit trail.

## 功能特性 / Features

- 完整 GTD 流程：收件箱 → 下一步/已排程/等待中/将来也许/参考资料 → 已完成，含周回顾向导与今日/明日视图 · Full GTD workflow with weekly review and today/tomorrow views
- 循环任务：RRULE 支持 + 快捷简写（`*2w[1,3]`），完成后自动重排 · Recurring tasks auto-reschedule on Done
- 标签系统：情境标签 + `p1/p2/p3` 优先级，自定义标签自动创建 · Tag system with auto-created custom tags
- 检查单：一键勾选，全部完成自动重置 · Checklists with one-key tick and auto-reset
- 金句（可选功能）：随心记录好句子/灵感/知识，一键入库，独立视图管理 · Quotes (opt-in): capture good sentences, ideas & knowledge into a dedicated view
- 番茄钟专注模式：全屏倒计时环、连击、桌面通知、waybar 模块 · Pomodoro focus mode with progress ring, streaks and a waybar module
- 控制台看板与开屏页：`horae stats` 或进入 TUI 时，展示极具极客美学与 Catppuccin 配色的“时间女神” ASCII 艺术与哲学标语，极具仪式感 · Stats dashboard and TUI splash screen featuring Catppuccin ASCII art of the Goddess of Time and philosophical GTD slogans
- 中英双语界面（`F6` 切换）、Catppuccin 深/浅主题（`F5` 切换） · Bilingual UI (F6) and Catppuccin themes (F5)

## 安装 / Installation

需要 Rust 1.89+，SQLite 已内置，无系统依赖。Requires Rust 1.89+; SQLite is bundled.

```sh
cargo install --git https://github.com/zhaohang1205/horae
# 或本地构建 / or build from source:
git clone https://github.com/zhaohang1205/horae.git && cd horae
cargo build --release
```

数据目录：`~/.config/horae/`（`horae.db` + `pomo.json`）。Data lives in `~/.config/horae/`.

## 快速开始 / Quick start

```sh
horae                                # 启动 TUI / launch the TUI
horae capture "买牛奶" --tag home     # 捕获进收件箱 / capture into the inbox
horae list --status next             # 列出下一步 / list next actions
horae show <task-id>                 # 查看完整时间线 / full event timeline
```

任务引用支持完整 id、唯一 id 前缀（类似 git）、或精确标题。Task refs accept a full id, a unique
id-prefix, or an exact title.

## CLI

| 命令 / Command | 说明 / Description |
| --- | --- |
| `horae` | 启动 TUI / Launch the TUI |
| `horae capture <title> [--tag T]... [--due TIME] [--status S] [--p1\|--p2\|--p3] [--json]` | 捕获新任务 / Capture |
| `horae list [--status S] [--tag T]... [--due-before TIME] [--json]` | 列出任务 / List tasks |
| `horae show <id> [--json]` | 任务详情 + 时间线 / Show with timeline |
| `horae next\|wait\|someday\|done <id>` | 流转状态 / Move between statuses |
| `horae schedule <id> [--start TIME] [--end TIME] [--rrule R]` | 排期（可加循环）/ Schedule (+recurrence) |
| `horae archive <id>` / `horae restore <id>` | 软删除 / 恢复 / Soft delete / restore |
| `horae tag <id> <name>` / `horae untag <id> <name>` | 增删标签 / Manage tags |
| `horae export [--file PATH]` | 备份到 JSON（任务/事件/标签/设置/番茄钟）· full backup |
| `horae import <FILE> [--replace]` | 合并还原；`--replace` 清空后精确还原 · merge / restore |
| `horae review` | 周回顾 / Weekly review |
| `horae tags` | 标签库 / List tags |
| `horae pomo start <id> \| stop \| daemon \| waybar` | 番茄钟 / Pomodoro |
| `horae alarm waybar [slot] \| next [slot]` | 到期提醒 / Upcoming-task reminders |

## TUI 快捷键 / Keybindings

| 键 / Key | 说明 / Action |
| --- | --- |
| `h` / `l` | 切换面板（引导/列表/详情）· switch pane |
| `j` / `k` | 上下移动 · move up/down |
| `0`-`9` | 切换视图（8=归档，9=标签库，0=金句）· switch view |
| `⇧J` / `⇧K` | 今日 / 明日 · today / tomorrow |
| `/` | 全局搜索 · global search |
| `f` | 情境过滤 · tag filter |
| `a` | 快速捕获（任意视图）· quick capture (any view) |
| `"` | 加入 / 移出金句（工作态任务自动转参考资料）· add to / remove from quotes |
| `Space` | 切换选择当前行（非连续多选）· toggle-select current row |
| `Ctrl+a` / `Ctrl+u` | 全选 / 反选 · select all / invert |
| `Enter` / `e` | 全量编辑：一句话补全标题 @标签 ~时间 *周期 · full edit (title @tags ~time *rrule) |
| `x` / `w` / `s` | 已完成 / 等待中 / 将来也许 · done / waiting / someday |
| `C` | 新增检查单 · add checklist item |
| `=` | 勾选检查单 / 重置 · tick / reset checklist |
| `T` | 批量打标签（可视模式多选）· bulk tag (visual multi-select) |
| `n` | 编辑长备注（`$EDITOR`）· edit notes |
| `P` / `S` / `[` | 开始/续杯 / 停止番茄 / 番茄时长配置 · pomodoro start/continue/stop/config |
| `A` / `D` | 归档（y 确认 / n 取消）· archive (y/n) |
| `u` | 恢复归档（支持批量）· restore from archive (batch-capable) |
| `c` | 标签库视图新增标签 · add tag (Tags view) |
| `r` / `R` | 周回顾（开始 / 下一步）· weekly review (start/next) |
| `F5` / `F6` | 主题 / 语言 · theme / language |
| `F7` | 金句功能开关（默认关闭）· toggle quotes feature (default off) |
| `F1` 或 `?` | 快捷键帮助 · shortcut help |
| `q` | 退出 · quit |

## 金句 / Quotes

金句是一个**可选功能**（默认关闭，`F7` 开启，状态持久化到 `settings`）。用于随心收藏好句子、灵感与知识碎片——它们不是任务，不该占用收件箱/今日等行动流。

Quotes is an **opt-in** feature (off by default; `F7` toggles it, persisted in `settings`). It's a notebook for quotes, inspirations and knowledge — not actionable tasks.

**工作方式 / How it works**

- 金句 = 带 `@quote` 系统标签、状态为 `参考资料(reference)` 的任务。A quote is a task tagged `@quote` with status `reference`, so it never surfaces in the inbox/action workflow.
- **金句只出现在金句视图**：参考资料视图会过滤掉 `@quote` 任务（侧栏徽标同步）；功能关闭后 `@quote` 回归普通标签，这些任务重新出现在参考资料视图。Quotes live **only** in the Quotes view — the Reference view (and its badge) filters out `@quote` tasks; turning the feature off restores them to Reference.
- `F7` 开启后侧栏出现 `[Library] 0 金句`，按 `0` 进入金句视图（按创建时间倒序，新的在前）。
- **收件箱 → 金句**：选中条目按 `"`，自动加 `@quote` 并流转为参考资料，离开收件箱（留在当前视图）。
- **随心记录**：金句视图内按 `a` 输入句子即直接入库（自动 `@quote`）。
- **自动路由**：任何视图捕获时输入 `@quote`（如 `a灵感 @quote`）→ 直接创建为金句并跳转金句视图。
- 金句视图内按 `"` = 移出金句（仅摘除标签）。回车/e 编辑、`n` 备注、`T` 打标签、`A` 归档等与普通任务一致。
- CLI：`horae capture "…" --tag quote --status reference` 可直接从命令行收藏金句。

## 时间与循环语法 / Time & recurrence syntax

```
now  +2h  +30m  +1d  +1w          相对偏移 / relative offsets
+3d 15:30                         相对偏移 + 时刻 / offset + clock
今天 / 明天 / 后天 [HH:MM]         中文天词 / Chinese day words
周三 / 下周五 [HH:MM]              星期几（可带"下周"）/ weekday (+next week)
8/20 15:30 · 2026.8.20            斜杠/点日期 / slash & dot dates
HH:MM                             当日时刻（已过则视为明日）/ same-day time
2026-07-24 [HH:MM]                绝对日期时间 / absolute date & time
```

一句话里的 `~time` 设**排程起点**（`scheduled_start_at`，状态进入已排程，只设起点不设终点）；`--due` 设软截止（`due_at`）。

循环 RRULE（一句话里 `*` 简写）：`FREQ=DAILY|WEEKLY|MONTHLY`、`INTERVAL=2`、
`BYDAY=SA,SU`、`BYMONTHDAY=1,-1`（-1=月末最后一天）、`COUNT=10` / `UNTIL=YYYY-MM-DD`。
快速简写：`*d`/`*w`/`*m`/`*y`（每天/周/月/年）、`*2w[1,3]`（每两周周一、周三，1-7=周一至周日，0=周日）、`*m[1,-1]`（每月 1 号和最后一天，负数=月末倒数）、`*m[1,15]`（每月 1 号、15 号），优先级 `!a`/`!b`/`!c`。

## 备份 / Backup

```sh
horae export                 # → horae-backup-2026-08-15.json（当前目录）
horae export --file ~/gtd.json
horae import horae-backup-2026-08-15.json          # 合并：已存在 id 整行跳过
horae import --replace ~/gtd.json                # 清空当前数据，精确还原
```

导出文件是一个自包含 JSON（带格式/版本字段），包含全部任务列、`task_events` 时间线、
标签、设置与番茄钟状态。备份即"拷贝这一个文件"，可放进 git、网盘或 cron 定时导出。
The backup is one self-contained JSON file — copy it to git/cloud/cron for free
redundancy. `--replace` is the true restore path; plain `import` merges.

## 手机同步 / Phone sync (`horae watch`)

用 Syncthing（或任意双向同步云盘）把 `~/.config/horae/sync` 同步到手机，然后在电脑上
常驻运行 `horae watch`，即可在手机上采集、查看与完成任务的闭环——零服务器、零 App。

Bridge the phone–computer gap with Syncthing: sync `~/.config/horae/sync` to your phone
and run `horae watch` on the computer. No server, no app.

```sh
horae watch                  # 常驻对账（systemd/tmux/autostart 后台运行）
horae watch --once           # 手动跑一轮
horae watch --dir ~/gtd-sync # 自定义同步目录
```

文件夹协议 / Folder protocol（手机写 / phone writes，电脑执行 / computer consumes）:

| 文件 / File | 用途 / Purpose |
| --- | --- |
| `capture.txt` | 每行一条采集，quick-add 语法 `标题 @tag ~time *rrule !p` / one capture per line |
| `actions.txt` | `done <id\|标题>` · `set <id\|标题> status next` · `set <id\|标题> due <time>` |
| `today.md` | 电脑生成的活动任务快照（Next / Scheduled / Waiting / 逾期）/ active snapshot |
| `reminders/` | 电脑生成的任务到期提醒（同步时手机 App 会收到文件变更通知）/ due reminders |
| `*.done` | 已处理回执（去重依据）/ receipts of consumed lines |

采集用手机上的任意笔记 App（Obsidian / Markor 等）指向该目录，写一行存盘即采集；
任务到期提醒仅在电脑开机期间触发——关机时到期，开机后补发。用任一免费 PaaS 部署
`horae serve`（中继）可获得真正实时的推送，此为可选升级路径。

Capture on the phone with any notes app pointed at this folder. Due reminders only fire
while the computer is on (catch-up on boot after downtime). A later relay (`horae serve`)
on any free PaaS unlocks real-time push — an optional upgrade path.

## 开发 / Development

```sh
cargo test                     # 测试 / tests
cargo clippy -- -D warnings    # 静态检查（须零警告）/ lint (must stay clean)
cargo fmt --check              # 格式 / formatting
```

架构说明见 [AGENTS.md](AGENTS.md)。See AGENTS.md for architecture and contributor rules.

## 许可证 / License

GPL-3.0，见 [LICENSE](LICENSE)。衍生作品须以同协议开源——拿代码可以，闭源白嫖不行。
GPL-3.0, see [LICENSE](LICENSE). Derivatives must be licensed alike — use it freely, but share alike.
