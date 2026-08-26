# 工作 / 生活任务管理工作流

把 horae 的功能映射到 GTD（Getting Things Done）五步法，形成可持续的日常系统。
本文件是展开版实践手册；精要版见 SKILL.md。

## 一、GTD 五步 ↔ horae 功能映射

| GTD 步骤 | horae 做法 |
| --- | --- |
| 收集 Capture | `horae c "..."` / TUI `a` / 手机 `capture.txt`——先入库再想 |
| 厘清 Clarify | TUI 选中条目 `Enter`：一句话补成可执行格式；`x/w/s/e` 分流 |
| 组织 Organize | 状态视图（1-7）+ 情境标签 `@work @home...` + 优先级 `!a/b/c` + 排期/循环 |
| 回顾 Reflect | 每周 `horae review` 或 TUI `r` 向导；随时 `horae stats` 看板 |
| 执行 Do | `horae do` 终结选择困难；TUI `P` 起番茄钟 |

## 二、每日闭环（约 15 分钟固定开销）

**早上（3 分钟）**

```sh
horae stats          # 昨晚完成了几个番茄、收件箱堆积多少
horae do             # 算法告诉你第一件事是什么，别犹豫
```

**白天（随手）**

- 想到任何事 → `hc "事 @情境"`，2 秒钟，不打断当前心流。
- 别人拜托的事 → `hc "等 X 反馈合同 @work"` 完成后转等待：
  `horae wait <id>`（逾期会自动浮出提醒你去催）。

**晚上（10 分钟，清空收件箱）**

对收件箱每一条做出决定（TUI 里逐条过最快）：

| 决定 | 键 | CLI |
| --- | --- | --- |
| 2 分钟能做完 | 直接做完 | — |
| 是行动 → 补成 Next | `Enter` 编辑 | `horae next <id>` |
| 有明确时间 → 排期 | `~明天 14:00` | `horae schedule` |
| 等别人 → 等待中 | `w` | `horae wait <id>` |
| 也许以后 → 将来 | `s` | `horae someday <id>` |
| 不是任务 → 归档 | `A` | `horae archive <id>` |

收件箱清零不是强迫症——它是"第二天早上 `do` 推荐质量"的保障。

## 三、每周回顾（周日晚上，15 分钟）

```sh
horae review         # 各状态桶计数 + 未来 3 天临期任务
```

对照检查：

1. **收件箱**是否又堆了？（平时没清干净的部分）
2. **等待中**有没有该催的？（逾期任务会加分浮出到 `do` 推荐）
3. **已排程**下周的循环任务槽位是否合理？
4. **将来也许**里捞一两件升格为 Next（很多"也许"其实是想做的心愿）。
5. **归档箱**扫一眼，确认没有误删；确认后可以放心留着或 purge。

## 四、工作与生活分库（Profile）

```sh
horae profile new work        # 建工作库
alias hw='horae --profile work'   # 加进 rc 文件
```

- 工作时间用 `hw ...` / `hw` 进工作库 TUI；下班回默认库存生活事务。
- 两库完全独立（各自的 SQLite 文件），`stats`/`review`/`do` 都互不干扰。
- 备份也独立：`hw export --file ~/backups/work.json`。
- TUI 里按 `M` 可视化管理 Profile（切换在下次启动生效）。

## 五、情境标签体系（建议起步集）

直接用系统预设，不要自创太多：

| 标签 | 用法 |
| --- | --- |
| `@work` / `@home` | 大场景二分 |
| `@errands` | 出门顺路办（配合"今日"视图批量处理） |
| `@calls` | 打电话类（碎片时间批量清） |
| `@computer` | 要坐电脑前做的 |
| `@learning` | 学习类（可配番茄钟专注） |
| `@quick` | 5 分钟内的小事（排队时过滤出来清掉） |
| `@focus` | 需要整块深度时间的 |
| `!a !b !c` | 每天 `!a` 尽量不超过 3 件，否则优先级失效 |

自定义标签（如 `@老板名字`、`@项目代号`）随用随建，但每月回顾时清理孤儿标签。

## 六、番茄钟实战

- 开工仪式：`hfp`（= `horae do --start`）→ 推荐任务 + 计时同时启动。
- 手动指定：`horae pomo start <id|前缀|精确标题>` 或 TUI 选中按 `P`。
- 时长配置 `[`：默认节奏改不了效率就换节奏，如 `50;10;25;2`（深工作型）。
- waybar 用户：模块配置指向 `horae pomo waybar`，状态栏常驻 🍅 与倒计时；
  到期提醒可用第二个模块跑 `horae alarm waybar`。
- 今日战绩看 `horae stats` 的番茄格子。

## 七、手机采集桥（零服务器）

一次性设置：

1. 安装 Syncthing（未装见 [install.md](install.md) 可选项表；Arch: `sudo pacman -S syncthing`），
   手机装 Syncthing App，两端配对。
2. 共享目录 `~/.config/horae/sync` ↔ 手机任意目录。
3. 电脑常驻：systemd user service 或 tmux 跑 `horae watch`。

日常使用（手机装 Obsidian/Markor 等笔记 App 指向同步目录）：

- 手机记一行到 `capture.txt`：`给房东发消息 @calls ~今天` → 几秒后进电脑收件箱。
- 手机打开 `today.md` 看今日快照（Next/已排程/等待/逾期）。
- 到期任务电脑会往 `reminders/` 写 markdown，Syncthing 推送文件变更通知到手机。
- 注意：提醒只在**电脑开机期间**触发；关机期间到期，开机会补发。

### 补强：ntfy 实时推送提醒（推荐）

`reminders/*.md` 只是文件变更通知，不是真正的系统推送。要「到点手机弹原生通知」，
加配 ntfy（零自建应用、零服务器）：

1. 手机装 ntfy App，订阅一个**随机主题**（如 `horae-<uuid>`——主题名即凭据，
   可猜的主题任何人都能读取你的提醒或伪造推送；要更稳就在 ntfy Web 给主题设令牌）。
2. profile 的 config.json 加：
   ```json
   "ntfy": { "url": "https://ntfy.sh", "topic": "horae-<随机串>",
             "priority": 5, "lead_minutes": 10 }
   ```
   设了令牌再加 `"token_env": "HORAE_NTFY_TOKEN"` 并 `export` 该变量。
3. `horae ntfy test` 发样例推送验证；常驻的 `horae watch` 会自动在定时任务
   到点前（默认 10 分钟）推送。仅带排程/截止时间的任务会推送；未配置时该
   功能完全静默。

## 八、灵感与知识：金句库

- F7 开启金句功能（持久化），侧栏出现 `[Library]`。
- 读到好句子：手机上照样走 capture.txt：`句子原文 @quote` → 电脑端自动路由进金句库。
- 金句不进收件箱、不出现在 `do` 推荐——它们是资产，不是待办。
- CLI 直录：`horae capture "句子" --tag quote --status reference`。

## 九、备份策略

```sh
# crontab：每晚 23:30 导出并保留最近 30 份
30 23 * * * horae export --file ~/backups/horae/$(date +\%F).json && ls -t ~/backups/horae/*.json | tail -n +31 | xargs -r rm
```

- 数据本质是单 SQLite 文件（`~/.config/horae/horae.db`），停机拷贝也可。
- 恢复演练：`horae import --replace xxx.json`（⚠️ 清空还原，先 export 当前状态更稳妥的做法是合并导入）。
- 把备份目录放进 git/private 网盘 = 异地容灾。

## 十、给不同人群的起手式

| 人群 | 最小可用组合 |
| --- | --- |
| 极简党 | 只用收件箱+下一步两个视图 + `hc/hd/hf` 三个别名 |
| 学生党 | `@learning` 标签 + 循环任务排课程复习 + 番茄钟 |
| 职场多线程 | work profile 分库 + `w` 等待跟踪 + 周回顾 |
| 双机党 | Syncthing 手机桥 + `today.md` 移动端查看 |
| 数据控 | 全程 quick-add 时间戳 + 定期 export 存档 + `show` 时间线复盘 |
