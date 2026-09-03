# horae — GTD 终端任务管理器

[![CI](https://github.com/zhaohang1205/horae/actions/workflows/ci.yml/badge.svg)](https://github.com/zhaohang1205/horae/actions/workflows/ci.yml)
[![Release](https://github.com/zhaohang1205/horae/actions/workflows/release.yml/badge.svg)](https://github.com/zhaohang1205/horae/actions/workflows/release.yml)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)

用 Rust 写成的 GTD 终端任务管理器：SQLite 数据层 + CLI + ratatui TUI 三合一。

核心设计是 **时间数据化（time-datafication）**：每次任务状态变更都打上 UTC 毫秒时间戳，追加到只写（append-only）的 `task_events` 时间线，每个任务都带完整的履历。

## 功能特性

- 完整 GTD 流程：收件箱 → 下一步/已排程/等待中/将来也许/参考资料 → 已完成，含周回顾向导与今日/明日视图、GTD 工作流心法导引
- 循环任务：全面支持 RRULE（日/周/月/年）+ 快捷简写（`*2w[1,3]`、`*m[1,-1]`、`*y[jan,jul]`），完成后自动重排
- 标签与优先级：情境标签 + `high/medium/low` 独立优先级字段（`!high`/`!medium`/`!low` 简写），自定义标签自动建档
- 实时补全与双模式：输入 `@/~/*/!` 即时弹出智能补全，支持「语法参考卡片」与「极速盲打」双模式切换
- 检查单：逐项勾选/删除/排序/改名，列表行显示 n/total 进度徽标
- 金句（可选功能）：随心记录好句子/灵感/知识，一键入库，独立视图管理
- 番茄钟专注模式：全屏倒计时环、连击、桌面通知、waybar 模块
- 控制台看板与开屏页：`horae stats` 或进入 TUI 时，展示极具极客美学与 Catppuccin 配色的“时间女神” ASCII 艺术与哲学标语，极具仪式感
- 中英双语界面（`F6` 切换）、Catppuccin 深/浅主题（`F5` 切换）

## 安装

需要 Rust 1.89+，SQLite 已内置，无系统依赖。

```sh
cargo install --git https://github.com/zhaohang1205/horae
# 或本地构建（从源码编译）：
git clone https://github.com/zhaohang1205/horae.git && cd horae
cargo build --release
```

数据目录：`~/.config/horae/`（`horae.db` + `config.json`（Profile 配置）+ `pomo.json`）。

## 推荐环境

为了获得最佳体验，建议：

- **支持 Kitty 图形协议的终端**：开屏页（Splash Screen）的“时间女神”像素艺术通过 Kitty 图形协议渲染，推荐使用 Kitty、Ghostty 或 WezTerm。其它终端会自动回退为纯 ASCII 文字版开屏，功能不受影响。
- **安装 Nerd Font**：界面图标默认使用 Nerd Font 字形；未安装时自动回退为纯 ASCII 字符（不会出现“豆腐块”）。可从 [Nerd Fonts](https://www.nerdfonts.com/) 任选一款并设为终端字体。
- **Windows 用户**：
  - 用 Rust 1.89+ 从源码构建（`cargo build --release`）；数据目录位于 `%APPDATA%\horae\`。
  - 桌面通知依赖 Linux 的 `notify-send`，在 Windows / macOS 上系统级弹窗可能不触发，但番茄钟计时与 TUI 内提醒正常工作；用 WezTerm 等支持 Kitty 协议的终端可获完整开屏。
  - 可用 `HORAE_CONFIG_DIR` 环境变量自定义数据目录。

## 快速开始

```sh
horae                                # 启动 TUI
horae capture "买牛奶" --tag home     # 捕获进收件箱
horae list --status next             # 列出下一步
horae show <task-id>                 # 查看完整时间线
```

任务引用支持完整 id、唯一 id 前缀（类似 git）、或精确标题。

## CLI

| 命令 / Command | 说明 |
| --- | --- |
| `horae` | 启动 TUI |
| `horae capture [title] [--clip] [--notes N] [--tag T]... [--due TIME] [--status S] [--high\|--medium\|--low] [--json]` | 捕获新任务（别名 `c`，支持 `--clip` 瞬时入库） |
| `horae list [--status S] [--tag T]... [--date MMDD] [--due-before TIME] [--json]` | 列出任务（别名 `l`）；日期搜索统一用四位数字，如 `0829` |
| `horae show <id> [--json]` | 任务详情 + 时间线（别名 `s`） |
| `horae modify <id> [text] [--title T] [--due TIME] [--notes N] [--edit-notes] [--tag T]... [--untag T]... [--high\|--medium\|--low\|--clear-priority] [--status S] [--json]` | 修改任务（别名 `m` / `mod` / `edit`，支持一句话 quick-add 语法） |
| `horae next\|wait\|someday\|done\|restore\|purge <id>` | 状态流转 / 恢复 / 永久删除（别名 `d` 对应 `done`） |
| `horae schedule <id> [--start TIME] [--end TIME] [--rrule R]` | 排期（可加循环） |
| `horae archive <id>` / `horae restore <id>` / `horae purge <id>` | 软删除 / 恢复 / 永久删除归档（`archive` 别名 `rm` / `delete`） |
| `horae tag <id> <name>` / `horae untag <id> <name>` | 增删标签 |
| `horae focus [--start]` / `horae do [--start]` | 推荐当前最该做的一件事（可顺带起番茄） |
| `horae log [message]` | 记录时间戳日志（不建任务） |
| `horae stats` | 控制台看板（Catppuccin 时间女神） |
| `horae export [--file PATH]` | 备份到 JSON（任务/事件/标签/设置/番茄钟） |
| `horae import <FILE> [--replace]` | 合并还原；`--replace` 清空后精确还原 |
| `horae review` | 周回顾 |
| `horae tags` | 标签库 |
| `horae pomo start <id> \| stop \| daemon \| waybar` | 番茄钟（别名 `p`） |
| `horae alarm waybar [slot] \| next [slot] [--limit N] [--all]` | 到期提醒 |
| `horae watch [--dir PATH] [--interval S] [--once]` | 手机同步桥（Syncthing）+ ntfy 提醒推送 |
| `horae ntfy test` | 发送一条 ntfy 测试推送，验证手机收到 |
| `horae profile <list\|new\|rename\|rm\|set-default> [--db PATH]` | 数据集（Profile）管理 |
| `horae completions <shell>` | 生成 shell 补全 |

> 帮助语言：CLI 的 `--help` 默认英文，加 `--lang zh`（或环境变量 `HORAE_LANG=zh`）
> 可切换为中文，例如 `horae --lang zh --help`、`HORAE_LANG=zh horae capture --help`。

高频单字母别名：`c`=capture, `l`=list, `s`=show, `m`=modify/edit, `d`=done, `rm`=archive, `p`=pomo, `do`=focus。

## TUI 快捷键

| 键 / Key | 说明 |
| --- | --- |
| `h` / `l` | 切换面板（引导/列表/详情） |
| `j` / `k` | 上下移动 |
| `0`-`9` | 切换视图（8=归档，9=标签库，0=金句） |
| `⇧J` / `⇧K` | 今日 / 明日 |
| `/` | 全局搜索 |
| `f` | 情境过滤 |
| `a` | 快速捕获（任意视图） |
| `"` | 加入 / 移出金句（工作态任务自动转参考资料） |
| `Space` | 切换选择当前行（非连续多选） |
| `Ctrl+a` / `Ctrl+u` | 全选 / 反选 |
| `Enter` / `e` | 全量编辑：一句话补全标题 @标签 ~时间 *周期 !优先级（即时触发补全卡片） |
| `x` / `w` / `s` | 已完成 / 等待中 / 将来也许 |
| `C` | 新增检查项 |
| `=` | 勾选下一项（不自动重置） |
| `Tab` | 进入检查单逐项管理（`j/k` 移动，`Space` 勾选，`d` 删除，`J/K` 排序，`e` 改名；`Tab`/`Esc` 退出） |
| `T` | 批量打标签（可视模式多选） |
| `n` | 编辑长备注（`$EDITOR`） |
| `P` / `S` / `[` | 开始/续杯 / 停止番茄 / 番茄时长配置（格式 `工作;短休;长休[;长休周期]`，如 `25;5;15;4`） |
| `A` / `D` | 归档（y 确认 / n 取消） |
| `u` | 恢复归档（支持批量） |
| `c` | 标签库视图新增标签 |
| `r` / `R` | 周回顾（开始 / 下一步） |
| `F5` / `F6` | 主题 / 语言 |
| `F7` | 模块显示设置（含金句、图标风格、启动快速录入、补全风格等 11 项开关） |
| `M` | 设置页（管理 Profile 数据集：新建/重命名/删除/设默认） |
| `F1` 或 `?` | 快捷键帮助 |
| `q` | 退出 |

**今日 / 明日视图的口径**：今日 = 今日窗口内的任务（含循环任务今日的发生点）+ 逾期任务（循环任务今天没有发生点时，展示最近一次已错过的发生点并标为逾期）；明日 = **仅**明日窗口内的任务（含循环任务明日的发生点），逾期与今日未完成的任务不结转。两个视图都只收**下一步 / 已排程**状态，等待中、将来也许、参考资料与收件箱里带日期的任务留在各自的状态视图。

## 金句

金句是一个**可选功能**（默认关闭，`F7` 开启，状态持久化到 `settings`）。用于随心收藏好句子、灵感与知识碎片——它们不是任务，不该占用收件箱/今日等行动流。

## 图标回退

界面图标默认使用 Nerd Font 字形。启动时自动探测（`fc-list` 是否含 Nerd 字体），未安装或无法探测时自动回退为纯 ASCII 字符，不会出现“豆腐块”。可用环境变量 `HORAE_ICONS=nerd|ascii` 强制指定，或在 TUI 里按 `F7` 打开模块显示设置，选到对应项按空格切换（持久化）。

**工作方式**

- 金句 = 带 `@quote` 系统标签、状态为 `参考资料(reference)` 的任务。
- **金句只出现在金句视图**：参考资料视图会过滤掉 `@quote` 任务（侧栏徽标同步）；功能关闭后 `@quote` 回归普通标签，这些任务重新出现在参考资料视图。
- `F7` 开启后侧栏出现 `[Library] 0 金句`，按 `0` 进入金句视图（按创建时间倒序，新的在前）。
- **收件箱 → 金句**：选中条目按 `"`，自动加 `@quote` 并流转为参考资料，离开收件箱（留在当前视图）。
- **随心记录**：金句视图内按 `a` 输入句子即直接入库（自动 `@quote`）。
- **自动路由**：任何视图捕获时输入 `@quote`（如 `a灵感 @quote`）→ 直接创建为金句并跳转金句视图。
- 金句视图内按 `"` = 移出金句（仅摘除标签）。回车/e 编辑、`n` 备注、`T` 打标签、`A` 归档等与普通任务一致。
- CLI：`horae capture "…" --tag quote --status reference` 可直接从命令行收藏金句。

## 日志

`horae log` 复用底层 `task_events` 只增时间线，记录纯时间戳事件/碎碎念，**不创建待办任务**——它写入一个特殊的系统任务 `__journal__`，与你的任务数据完全隔离。

`horae log "喝点水"` 记录一条；`horae log`（不带参数）倒序列出最近 50 条。

## 时间与循环语法

```
now  +15m  +30m  +1h  +2h  +4h  +1d  +3d  +1w   相对偏移
+3d 15:30 · +1w 09:00                           相对偏移 + 指定时刻
today / tomorrow / 今天 / 明天 / 后天 [HH:MM]   天词（支持拼音 ~td/~tm 补全）
周三 / 下周五 / mon / tue / next fri [HH:MM]    中英文星期词（拼音 ~zy/~mon 映射周一）
09:00 · 18:00 · HH:MM                           当日时刻（已过则自动顺延至明日）
8/20 15:30 · 2026.8.20                          斜杠/点日期
2026-07-24 [HH:MM]                              绝对日期时间
```

一句话里的 `~time` 设**排程起点**（`scheduled_start_at`，状态进入已排程，只设起点不设终点）；`--due` 设软截止（`due_at`）。

循环 RRULE（一句话里 `*` 简写）：`FREQ=DAILY|WEEKLY|MONTHLY|YEARLY`、`INTERVAL=2`、`BYDAY=SA,SU`、`BYMONTHDAY=1,-1`（-1=月末最后一天）、`BYMONTH=1,7`（按月份数字或名称，如 `jan,jul`）、`COUNT=10` / `UNTIL=YYYY-MM-DD`。快速简写：`*d`/`*w`/`*m`/`*y`（每天/周/月/年）、`*weekday`/`*weekend`（工作日/周末）、`*2w[1,3]`（每两周周一、周三，1-7=周一至周日，0=周日）、`*m[1,-1]`（每月 1 号和最后一天，负数=月末倒数）、`*m[1,15]`（每月 1 号、15 号）、`*y[jan,jul]`（每年 1 月与 7 月）、`*2y[6]`（每两年 6 月），输入 `*2`/`*3` 动态推导 `*2d`/`*2w` 等。优先级支持 `!high`/`!h`/`!1`/`!高`、`!medium`/`!m`/`!2`/`!中`、`!low`/`!l`/`!3`/`!低`（大小写均可）。补全支持缺失槽位引导与即时语法参考。

## 备份

```sh
horae export                 # → horae-backup-2026-08-15.json（当前目录）
horae export --file ~/gtd.json
horae import horae-backup-2026-08-15.json          # 合并：已存在 id 整行跳过
horae import --replace ~/gtd.json                # 清空当前数据，精确还原
```

导出文件是一个自包含 JSON（带格式/版本字段），包含全部任务列、`task_events` 时间线、标签、设置与番茄钟状态。备份即“拷贝这一个文件”，可放进 git、网盘或 cron 定时导出。`--replace` 是真正的还原路径；普通 `import` 为合并。

## 手机同步（`horae watch`）

用 Syncthing（或任意双向同步云盘）把 `~/.config/horae/sync` 同步到手机，然后在电脑上常驻运行 `horae watch`，即可在手机上采集、查看与完成任务的闭环——零服务器、零 App。

```sh
horae watch                  # 常驻对账（systemd/tmux/autostart 后台运行）
horae watch --once           # 手动跑一轮
horae watch --dir ~/gtd-sync # 自定义同步目录
```

文件夹协议（手机写，电脑执行）：

| 文件 / File | 用途 |
| --- | --- |
| `capture.txt` | 每行一条采集，quick-add 语法 `标题 @tag ~time *rrule !p` |
| `actions.txt` | `done <id\|标题>` · `set <id\|标题> status next` · `set <id\|标题> due <time>` |
| `today.md` | 电脑生成的活动任务快照（Next / Scheduled / Waiting / 逾期） |
| `reminders/` | 电脑生成的任务到期提醒（同步时手机 App 会收到文件变更通知） |
| `*.done` | 已处理回执（去重依据） |

采集用手机上的任意笔记 App（Obsidian / Markor 等）指向该目录，写一行存盘即采集；任务到期提醒仅在电脑开机期间触发——关机时到期，开机后补发。用任一免费 PaaS 部署 `horae serve`（中继）可获得真正实时的推送，此为可选升级路径。

## 手机提醒（`horae ntfy`）

如果你只想要「任务到点时手机弹通知」（而不是在手机上维护日历），最省事的方式是用 [ntfy](https://ntfy.sh)：桌面端 `watch` 守护进程在定时任务到点前（默认 10 分钟）向 ntfy 主题 POST 一条消息，手机上订阅该主题的 ntfy App 即收原生推送。**零自建应用、零服务器**，比自建日历 API 轻得多。

1. 手机装 ntfy（[ntfy.sh](https://ntfy.sh) 或 F-Droid / App Store），订阅一个随机主题（如 `horae-<uuid>`）。

> ⚠️ **隐私提示**：公共 ntfy.sh 上的主题名就是唯一凭据——任何知道（或猜到）主题名的人都能**订阅读取你的提醒内容**，也能**向你的手机伪造推送**。务必使用随机长主题名；更进一步可在 ntfy Web 界面给该主题设置访问令牌，并把令牌放进环境变量（见下文 `token_env`），绝不写进 config.json。
2. 在 `~/.config/horae/config.json` 的对应 profile 下加 `ntfy` 块：

   ```json
   {
     "default_profile": "default",
     "profiles": {
       "default": {
         "db": "horae.db",
         "ntfy": {
           "url": "https://ntfy.sh",
           "topic": "horae-你的随机主题",
           "token_env": "HORAE_NTFY_TOKEN",
           "priority": 5,
           "lead_minutes": 10,
           "tags": "alarm"
         }
       }
     }
   }
   ```

   - `token_env`：读取 Bearer token 的环境变量名（令牌本身**绝不落盘**，只放环境变量）；不用令牌可省略。
   - `priority`：1–5，默认 5（强制提醒）；`lead_minutes`：提前多少分钟推送；`tags`：ntfy 的 emoji 短码（`Tags` 头），可选。

3. 发送一条测试推送，确认手机收到：

   ```sh
   export HORAE_NTFY_TOKEN=你的令牌   # 若设了 token_env
   horae ntfy test
   ```

4. 常驻 `horae watch`，到点任务的手机提醒即自动推送（仅带排程/截止时间的任务会推送；无时间的纯收件箱任务不推送）。ntfy 未配置时 `watch` 的该 stage 为空操作，对老用户零影响；单条推送失败不影响其它阶段，下一轮自动重试。

> 提醒仅在电脑开机期间触发——关机时到期的任务，开机后补发。需要全程实时请参考 `horae serve` 中继（可选升级路径）。

## 开发

```sh
cargo test                     # 测试
cargo clippy -- -D warnings    # 静态检查（须零警告）
cargo fmt --check              # 格式
```

架构说明见 [AGENTS.md](AGENTS.md)。

## 许可证

GPL-3.0，见 [LICENSE](LICENSE)。你可自由使用、修改与再分发，但衍生作品须以相同的 GPL-3.0 协议开源。
