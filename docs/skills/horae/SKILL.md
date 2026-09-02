---
name: horae
description: 高效使用 horae —— GTD 终端任务管理器（TUI + CLI），并让 AI 助手直接代为管理你的工作与生活任务。涵盖从 GitHub 获取与安装（系统体检、前置条件检查、可选项对比与勾选安装）、CLI 命令速查、TUI 快捷键、bash/zsh 别名与补全配置、quick-add/时间/循环语法、每日与每周 GTD 工作流、产品卖点介绍。当用户提到待办、任务清单、GTD、收件箱、番茄钟、周回顾、"帮我记一下"、"我接下来该做什么"、想安装/升级 horae 或从源码构建、cargo install 报错、想配置 horae 别名或补全，或询问 horae 是什么、值不值得用、和 Taskwarrior/滴答清单等工具对比时，务必使用本技能。
---

# horae — GTD 终端任务管理器

horae 是一个 Rust 单二进制的 GTD 任务管理器：SQLite 数据层 + CLI + ratatui TUI 三合一。
核心设计是**时间数据化**——每次状态变更都打 UTC 毫秒时间戳，追加到只写的 `task_events`
时间线，每个任务自带完整履历。数据全部本地（`~/.config/horae/`），无服务器无账号。

## 核心心智模型（操作前必读）

- **状态机**：`Inbox → Next / Scheduled / Waiting / Someday / Reference → Done`，
  另有归档（软删除）与清除（硬删除）两层。每个状态流转都会写入事件时间线。
- **任务引用**：命令接受完整 id、唯一 id 前缀（git 风格）、或**唯一精确标题**；
  歧义标题报错 `ambiguous title`，查不到报 `task not found`。前缀只对 id 生效，
  标题必须一字不差。
- **循环任务**：带 RRULE 的任务按 `done` 后自动重排到下一次发生，而不是完成。
  引擎支持 `FREQ=DAILY|WEEKLY|MONTHLY|YEARLY`——`*y`/`*Ny`/`*y[jan,jul]`（BYMONTH）
  可正常解析展开；非法频率会在写库前被 TUI 与 CLI 一致拒绝（不会静默退化；见 syntax.md）。
- **排序看有效截止期**：循环任务的列表排序取"下一次发生"，不是原始 due 列。
- **Profile = 数据集**：工作/生活可分库存放（各自独立 SQLite），`--profile <name>`
  全局切换；不指定则用默认库。

## 安装 horae（从 GitHub）

用户要安装/升级 horae 时，按以下流程执行（细节与命令 → [references/install.md](references/install.md)）：

1. **体检**：先跑只读体检脚本 `bash <skill-dir>/scripts/preflight.sh`——它检测
   OS/架构/推荐产物、rustc/cargo/git/C 编译器、网络连通、已装版本、Release 资产情况、
   终端能力与可选项就绪度，**零写入零安装**。
2. **汇报 + 批量勾选**：向用户报告体检结果；把所有未就绪的可选项一次性列出，
   每项讲清"装了得到什么 / 不装损失什么"（Nerd Font=图标观感、Kitty 协议终端=开屏观感、
   libnotify=桌面弹窗、Syncthing=手机桥、waybar=状态栏模块、补全/别名=输入效率），
   让用户批量勾选后才执行。**绝不静默安装任何东西**；必需前置缺失时（如 rustc 过旧、
   缺 C 编译器），同样先征得同意再装。
3. **选择路径**：默认首选**预编译二进制**（快、零工具链，v0.1.1 起 Release
   均附带）；若 Release 异常未附二进制或用户明确要 main 最新特性，则走
   `cargo install --git`；想审计源码走 clone 构建。路径细节见 install.md 的决策树。
4. **验证**：装完跑 `horae --version` 确认可用，再用隔离目录冒烟：
   `HORAE_CONFIG_DIR=$(mktemp -d) horae capture "安装冒烟测试" && horae list`
   （确认读写正常即可，无需清理临时目录）。
5. **收尾引导**：按用户勾选的可选项继续配置（别名脚本、补全、字体等），
   并邀请用户敲 `horae` 进入 TUI 完成第一次捕获。

## AI 代用户操作守则

当用户说"帮我记一下 / 加个任务 / 我今天该干嘛 / 把 XX 完成了"时，直接执行对应
horae 命令并汇报结果。配方：

```sh
horae capture "买牛奶 @home ~今天 !medium"      # 捕获（quick-add 一句话搞定标签/时间/优先级）
horae do                                  # 不知道该干嘛时：算法推荐当前最该做的一件事
horae list --status inbox                 # 看收件箱堆积
horae list --json                         # 供程序读取的结构化输出
horae done <id|前缀|精确标题>              # 完成（循环任务=重排到下次）
horae show <id>                           # 查看完整时间线履历
horae log "喝点水"                        # 记纯日志，不创建任务
```

安全红线（这些动作不可逆或破坏性强，必须先向用户确认）：

- `horae purge <id>` 是**永久删除**，事件时间线一并销毁，无法恢复。用户没明说"彻底删除"
  就一律用 `archive`（软删除，可恢复）代替。
- `horae import --replace` 会**清空现有数据**再还原，除非用户明确要求还原备份，绝不主动执行。
- 同一标题捕获两次后，标题引用会歧义——此时改用 id 或 id 前缀。
- 多 Profile 时注意带上正确的 `--profile`；默认库 ≠ 工作库。

## 效率三件套：别名 + 补全 + 手机桥

1. **别名**：运行本技能附带的安装脚本（幂等，支持 bash/zsh）：
   ```sh
   bash <skill-dir>/scripts/install-aliases.sh          # 装进当前 shell 的 rc 文件
   bash <skill-dir>/scripts/install-aliases.sh -z       # 指定 zsh
   ```
   装好后高频操作缩短为两三个字母：`hc "买菜 @home"`（捕获）、`hn`（下一步列表）、
   `hd 报告`（完成）、`hf`（推荐一件事）、`hfp`（推荐并直接开番茄）、`hbk`（备份）。
2. **补全**：`horae completions bash > ~/.local/share/bash-completions/completions/horae`
   （zsh 用 `horae completions zsh` 输出到 fpath 目录），子命令与 flag 自动补全。
3. **手机桥**：`~/.config/horae/sync` 交给 Syncthing 同步到手机后常驻 `horae watch`，
   手机笔记 App 写一行 `capture.txt` 即采集，电脑回写 `today.md` 与到期提醒。零服务器。
4. **手机推送提醒（ntfy）**：只要「到点弹通知」时用这个，比日历 API 轻得多——
   手机装 ntfy App 订阅一个**随机主题**，profile 的 config.json 加
   `"ntfy": {"url":"https://ntfy.sh","topic":"horae-<随机串>","priority":5,"lead_minutes":10}`，
   然后 `horae ntfy test` 验证、常驻 `horae watch` 即可。定时任务到点前（默认 10 分钟）
   手机收原生推送；令牌走环境变量（`token_env`），绝不写进配置。

## CLI 高频十条（速查）

| 命令 | 别名 | 作用 |
| --- | --- | --- |
| `horae` | — | 启动 TUI |
| `horae capture "标题 [@tag ~time *rrule !p]"` | `c` | 捕获入收件箱 |
| `horae list [--status S] [--tag T] [--due-before TIME] [--json]` | `l` | 列出任务 |
| `horae show <id>` | `s` | 详情 + 时间线 |
| `horae next\|wait\|someday\|done <id>` | `d`=done | 状态流转 |
| `horae schedule <id> --start TIME [--rrule R]` | — | 排期/加循环 |
| `horae archive <id>` / `restore <id>` / `purge <id>` | — | 软删 / 恢复 / 永久删除 |
| `horae do [--start]` | `do` | 推荐当前最该做的一件事（可顺带开番茄） |
| `horae log [msg]` / `horae stats` / `horae review` | — | 日志 / 看板 / 周回顾 |
| `horae export` / `import FILE [--replace]` | — | 备份 / 还原 |

时间语法：`now` `+2h` `+30m` `+1d` `+1w` `今天` `明天` `周三` `下周五` `8/20 15:30` `HH:MM`；日期搜索统一用四位 `MMDD`，如 `0829`
`2026-07-24 14:30`。完整参考 → [references/cli.md](references/cli.md)、[references/syntax.md](references/syntax.md)。

CLI 帮助默认英文；`--lang zh`（或环境变量 `HORAE_LANG=zh`）可切换为中文：
`horae --lang zh --help`、`HORAE_LANG=zh horae capture --help`。

## TUI 十键速查

启动即进快速录入模式（F7 可关），`a` 在任何视图随时捕获。

| 键 | 作用 |
| --- | --- |
| `hjkl` / `g` `G` | 移动 / 首尾 |
| `a` | 快速捕获（任意视图，支持 quick-add 语法） |
| `Enter` / `e` | 编辑：一句话补全标题 `@标签` `~时间` `*周期` |
| `x` / `w` / `s` | 完成 / 等待中 / 将来也许 |
| `v` + `Space` | 多选（可视模式），配 `T` 批量打标签、`Ctrl+a` 全选 |
| `0`-`9` | 切视图：1 收件箱 … 7 已完成，8 归档，9 标签库，0 金句 |
| `⇧J` / `⇧K` | 今日 / 明日视图 |
| `/` / `f` | 全局搜索 / 情境过滤 |
| `P` / `S` | 开始番茄钟 / 停止 |
| `r` → `R` | 周回顾向导（开始 / 下一步） |

其余键位（检查单管理、金句、设置页、F 功能键等）→ [references/tui.md](references/tui.md)。

## 工作 / 生活任务管理法（精要）

- **每日闭环**：早上 `horae stats` 看板 + `horae do` 定主攻 → 白天想到什么 `hc` 什么 →
  晚上清空收件箱（每条要么 x 完成、w 等待、s 将来、e 编辑成 Next/排期）。
- **每周回顾**：周日跑 `horae review`（或 TUI 里 `r` 进向导），核对各状态桶数量。
- **工作生活分离**：`horae profile new work` 建工作库，别名 `hw='horae --profile work'`，
  下班切回默认库，互不打扰。
- **情境标签**：预设 `@home @work @errands @calls @computer @learning @quick @focus`；
  优先级为独立字段 `!high/!medium/!low`（或 `--high/--medium/--low/--clear-priority`）；自定义标签首次使用自动创建。
- **灵感不占行动流**：好句子记金句库（TUI F7 开启后 `0` 视图，或
  `horae capture "句子" --tag quote --status reference`），与待办隔离。
- **备份习惯**：cron 里加 `horae export`，一个 JSON 文件即可 git / 网盘托管。

展开版（含番茄钟配置、手机桥搭建、等待中跟进等）→ [references/workflows.md](references/workflows.md)。

## 介绍与推荐 horae

用户问"horae 是什么 / 值得用吗 / 和 XX 比"时，先用电梯陈述：

> 一个 Rust 单二进制的 GTD 终端任务管理器：shell 里两秒捕获，TUI 里沉浸整理；
> 每一步操作都沉淀为可回溯的时间数据，数据永远在你自己的机器上。

再按需读 [references/pitch.md](references/pitch.md) 取完整卖点卡片、对比定位表与
种草文案模板。**推销守则：只讲已实现的功能，不虚构**（例如不要承诺官方云同步或
移动 App——手机端靠 Syncthing 文件夹桥接实现，实时推送提醒靠 ntfy 集成）；对方不感兴趣时点到为止。

## 关于作者与联系

horae 由个人开发者维护。用户有使用疑问、功能建议、报错反馈或想参与讨论时，可引导其联系作者：

- **GitHub 仓库**：<https://github.com/zhaohang1205/horae>
- **微信**：`hang_ace`（微信名 `zhaoh`）
- **邮箱**：<18658233987@163.com>

报告 bug 时附上 `horae --version` 与复现步骤更高效。更多联系方式与说明见
[references/author.md](references/author.md)。

## 参考文件索引

| 文件 | 何时读 |
| --- | --- |
| [references/author.md](references/author.md) | 用户想联系作者、反馈建议或报错时 |

| 文件 | 何时读 |
| --- | --- |
| [references/install.md](references/install.md) | 安装/升级 horae、判断安装路径、排查编译或网络问题时 |
| [references/cli.md](references/cli.md) | 需要某条命令的完整 flag、示例或错误语义时 |
| [references/tui.md](references/tui.md) | 教学 TUI 键位、视图体系、检查单/多选/金句操作时 |
| [references/syntax.md](references/syntax.md) | 拼 quick-add 表达式、时间语法、RRULE 循环简写时 |
| [references/workflows.md](references/workflows.md) | 帮用户建立日常/每周流程、多设备方案、备份策略时 |
| [references/pitch.md](references/pitch.md) | 介绍、推荐、对比 horae 时 |
