# horae CLI 完整参考

约定：

- **任务引用 `<id>`**：完整 UUID、唯一 id 前缀（git 语义，精确 id > 前缀 > 标题）、
  或唯一精确标题。歧义标题报 `ambiguous title`；不存在报 `task not found`。
  已归档任务只能用 id 寻址。
- **时间语法** 见 [syntax.md](syntax.md)，所有命令的时间参数共用一套。
- 全局 flag：`horae --profile <NAME> <子命令>` 对任何子命令生效。
- 帮助语言：CLI 的 `--help` 默认英文，可通过 `horae --lang zh <子命令> --help`
  或环境变量 `HORAE_LANG=zh` 切换为中文（与 TUI 的 F6 语言切换一致）。
- 状态取值：`inbox | next | waiting | scheduled | someday | reference | done`。

## 高频单字母别名

`c`=capture, `l`=list, `s`=show, `d`=done, `p`=pomo, `do`=focus

## 任务生命周期

### capture（别名 c）

```sh
horae capture "买牛奶" --tag home --medium       # flag 风格
horae capture "提交报告 @work ~明天 14:00 !high"  # quick-add 内联风格（推荐）
horae capture "周报" --status scheduled --due +1d
horae capture "句子" --tag quote --status reference   # 直接收藏金句
```

- 标题可不加引号（多个词自动拼接）。
- `--tag` 可重复；自定义标签首次使用自动创建。
- `--high/--medium/--low` 互斥（对应独立 priority 字段）；`--clear-priority` 清除。
- 标题里的内联 token 同样生效：`@tag` `~time` `*rrule` `!high|!medium|!low` → 见 [syntax.md](syntax.md)。
- `--json` 输出新建任务的 JSON。

实测行为：`~今天` 会把任务直接置为 `scheduled`（设排程起点）；只有 RRULE 没有时间时
任务留在 `inbox`。

### list（别名 l）

```sh
horae list                          # 全部未归档任务，按有效截止期排序
horae list --status next            # 单状态过滤
horae list --status scheduled --tag work
horae list --tag home --tag errands # 多标签 = 同时满足
horae list --date 0829 --json         # 搜索当前年份 8 月 29 日的任务
horae list --due-before +1d --json  # 24 小时内到期，JSON 输出
```

排序用 `effective_due`（循环任务取下一次发生）。`--json` 行结构含
`id/title/status/rrule/due_at/scheduled_start_at/checklist/tags` 等字段，
适合脚本与 AI 解析。

日期搜索统一使用四位 `MMDD` 格式，例如 `0829` 表示当前年份的 8 月 29 日。

### show（别名 s）

```sh
horae show 501acf51        # 详情 + append-only 事件时间线
horae show "精确标题"
horae show <id> --json
```

时间线按本地时间展示事件类型（captured/clarified/completed/tag_added/scheduled…）。

### 状态流转

```sh
horae next <id>       # Inbox/其他 → Next
horae wait <id>       # → Waiting（等待他人）
horae schedule <id> [--start TIME] [--end TIME] [--rrule R]
horae someday <id>    # → Someday/Maybe
horae done <id>       # → Done；循环任务改为重排到下一次发生
```

- 循环任务 `done` 的输出仍是 `<前缀> -> done`，但任务并未终结——它被排到下一个槽位
  （可用 `horae show` 看到新的 scheduled 记录）。
- 已完成的任务再 `done` 报 `invalid status transition: done -> done`。

### 归档三层

```sh
horae archive <id>    # 软删除（可 restore），记录归档原因 completed/deleted
horae restore <id>    # 从归档恢复
horae purge <id>      # ⚠️ 永久删除：任务 + 时间线 + 标签 + 子任务级联清除，无事件可查
```

`purge` 只对已归档任务有效。除非用户明确要求"彻底删除"，一律用 `archive`。

### 标签

```sh
horae tag <id> focus      # 加标签（自动创建）
horae untag <id> focus
horae tags                # 标签库：context 组 / priority 组，(sys) 为系统预设
```

系统预设情境标签：`home work learning errands calls computer quick focus quote`；
优先级为独立字段（`high`/`medium`/`low`，经由 `!high`/`!medium`/`!low` 或 `--high`/`--medium`/`--low` 设置）。

## 执行与专注

### do（focus 的可见别名）

```sh
horae do              # 打印当前最该做的一件事 + 推荐分数
horae do --start      # 同时启动番茄钟
```

推荐算法（详见仓库 docs/focus-algorithm.md）：过滤掉 Someday/Reference/Done 及未到点的
Scheduled/Waiting 后打分——优先级 `high` +10000（统治级）、`medium` +5000、`low` +1000；
逾期 +2000 且每多逾期一天 +10（封顶 +500）、今天到期 +1000；Next 状态 +500；
同分比创建时间，越早越优先。

### log（时间戳日志）

```sh
horae log "喝点水"    # 记一条纯日志，写入特殊系统任务 __journal__，不产生待办
horae log             # 倒序列最近 50 条
```

适合"发生了什么"类记录；想留痕又不想污染收件箱时用它。

### 番茄钟（别名 p）

```sh
horae pomo start <id|前缀|精确标题>   # 与其他命令一致的 git 式解析
horae pomo stop               # 停止并杀掉后台 daemon
horae pomo daemon             # 后台进程本体（一般不手动跑）
horae pomo waybar             # 输出 waybar JSON：{"class":"work","text":"🍅 任务 MM:SS",...}
```

`start` 会拉起后台守护进程（每秒写 `pomo.json`、结束发桌面通知）。
不想找 id 就用 `horae do --start`。

### 到期提醒

```sh
horae alarm next [--limit N]     # 最近将到期的 N 个提醒
horae alarm waybar [slot]        # waybar 提醒模块 JSON（--all 输出整个窗口数组）
```

## 数据与维护

### stats / review

```sh
horae stats           # MOTD 风格看板：今日番茄、完成数、各状态桶计数
horae review          # 周回顾摘要：inbox/next/waiting/scheduled/someday 数量与临期提示
```

### export / import

```sh
horae export                          # → horae-backup-<日期>.json（当前目录）
horae export --file ~/backups/h.json  # 自定义路径
horae import h.json                   # 合并：已存在 id 整行跳过
horae import --replace h.json         # ⚠️ 清空后精确还原
```

导出是自包含 JSON（任务全列 + 事件线 + 标签 + 设置 + 番茄钟状态），可进 git/网盘/cron。

### profile（数据集管理）

```sh
horae profile list
horae profile new work                 # 默认 db=profiles/work.db
horae profile new prod1 --db prod1.db
horae profile rename work work2
horae profile rm prod1                 # 仅移出配置，数据库文件保留
horae profile set-default work
horae --profile work capture "写周报"   # 临时切库
```

配置存于 `~/.config/horae/config.json`；切换只影响后续命令，不影响已开会话。

### watch（手机同步桥）

```sh
horae watch                    # 常驻轮询（放 systemd/tmux/自启）
horae watch --once             # 手动跑一轮
horae watch --dir ~/gtd-sync --interval 10
```

协议：手机写 `capture.txt`（quick-add 一行一条）与 `actions.txt`
（`done <id|标题>` / `set <id|标题> status next` / `set <id|标题> due <time>`）；
电脑回写 `today.md` 快照、`reminders/*.md` 到期提醒和 `.done` 回执。

### completions

```sh
horae completions bash    # bash/zsh/fish/elvish/powershell
```

在 DB 打开之前拦截执行，零副作用。

## 错误速查

| 报错 | 含义 | 处理 |
| --- | --- | --- |
| `task not found: X` | 无匹配 id/前缀/标题 | 先 `list` 找准引用 |
| `ambiguous title: X` | 多条同名任务 | 改用 id 或 id 前缀 |
| `invalid status transition: a -> b` | 状态机不允许 | 如对 done 任务再 done |
| `Error: NotArchived` | purge 只作用于归档任务 | 先 archive 再 purge |
