# 架构工作交接文档

> 用途：开新任务窗口讨论后续架构候选时，无需重复完整上下文。本文件记录已完成工作、剩余候选与关键设计约束。
> 基线提交：`c3d0cc6`（repo::mutate seam）

## 已完成（4/5 候选）

### 1. 排程/循环模块 — `d7a56b5`
- 新模块 `src/schedule.rs`（顶层，in-process 纯计算，与 `time.rs` 平级）。
- 接口 5 函数：`occurrences(rrule, anchor)`（366 视野藏内部）、`effective_due(task)`、`effective_due_from(occ, now)`（now 显式注入）、`next_window(rrule, anchor, end)`、`display_due(task, cached)`。
- 吸收：`time.rs` 的 `rrule_occurrences` + 私有辅助（`step`/`add_months`/`days_in_month`/`month_day_matches`/`parse_until`）、`commands/mod.rs` 的 `effective_due*`、`tasks.rs::transition` 的 reschedule 纯部分、TUI `day_lists_from`/`row_due`/`row_from_tags` 的锚点选择+366+窗口匹配+展示阶梯。
- `time.rs` 退化为纯时间原语（`parse_time`/`now_ms`/`local_day_bounds`/`format_local`/`relative_*`）。
- 顺带修：**BYDAY 之前用 UTC 星期匹配**（BYMONTHDAY 已是本地历日），已对齐为本地历日（真实 bug）。
- `rrule_valid` 拒绝 `*y`/`4y`/`yearly`/`FREQ=YEARLY`（解析层拒绝，引擎支持 DAILY|WEEKLY|MONTHLY）。
- DB 写入留在调用方（`tasks.rs::transition` 调 `next_window` 后自己写库）。

### 2. JSON 状态文件合并 — `36695f0`
- 新模块 `src/repo/state.rs`：泛型 `JsonStateStore<T>`（路径解析、原子 tmp+rename 写、缺失文件→`Default`、测试隔离）。
- `repo/alarm.rs`、`repo/notify.rs`、`repo/pomodoro.rs` 变为薄 adapter（公共接口不变）。
- 修：pomodoro 之前用普通 `fs::write`（非原子），统一为原子写。
- 测试隔离：`state::set_test_override()` sticky、构造时采样（取代逐模块 `POMO_IDLE_OVERRIDE`）。测试用 `JsonStateStore::at(path)` 显式路径构造绕开全局开关。附带修掉 TUI 测试读真实 `notify.json` 的隐患。
- 注意：测试全局开关在并行测试里要小心——`at()` 是绕过它的唯一测试构造路径。

### 3. 变更+审计 seam — `c3d0cc6`
- 新 seam `repo::mutate(conn, |tx, now| ...)`（`src/repo/mod.rs:26`）：打开事务、计算唯一 `now`、跑闭包、commit/rollback。调用方描述变更，不碰事务生命周期。
- 11 个调用点全部迁移：`create_capture`/`transition`/`set_due`/`set_rrule`/`schedule`/`archive`/`unarchive`/`purge`（tasks.rs）、`add_tag_to_task`/`remove_tag_from_task`/`delete_tag`（tags.rs）、`import_all`（backup.rs）。
- `add_tag_to_task_inner` 增加 `now` 参数，嵌套标签事件共享外层事务时间戳。
- 修：`pomo.rs` 之前无事务写 `EV_POMODORO`；`delete_tag` 之前吞 `log_event` 错误。
- **重要修正**：报告原称 "unchecked_transaction 的 Drop 不回滚" 是**错的**——rusqlite `Transaction` 默认 `DropBehavior::Rollback`。此候选价值是**审计不变式的 locality**，不是回滚安全。
- 全仓仅剩一处 `unchecked_transaction`（在 `mutate` 内部）。

### 4. tasks 上帝模块拆分 + 死 schema — done
- `repo/tasks.rs`（964 行、26 fn）按职责拆为目录模块 `repo/tasks/`：`mod.rs`（共享 `TASK_COLUMNS`/`row_to_task` + `pub use` 再导出，`crate::repo::tasks::*` API 不变，所有消费方零改动）、`transition.rs`（11 个变更函数）、`query.rs`（只读查询）、`quotes.rs`（金句视图）。
- 新增迁移 `0011_drop_dead_columns.sql`（v10）删死列：`kind`、`parent_id`、`organized_at`、`started_at`、`project_type`。`parent_id` 有自引用 FK + 索引，迁移内先 `DROP INDEX idx_tasks_parent` 再 `PRAGMA foreign_keys=OFF` 后 DROP（SQLite 拒绝删除 FK/索引引用的列）。
- **修正原文档**：`delegated_to` 不是死列——capture 写入、TUI 详情显示（`render.rs`），**保留**。
- 同步：`Task` 模型删 `started_at`；`model/backup.rs::BackupTask` 删 5 个死列，`BACKUP_VERSION` 1→2（备份格式变更，旧版本备份拒绝导入）；`backup.rs` export/import 对应更新。
- 全测试（112）通过；改动文件 clippy 零警告。

## 剩余候选

### 5. 金句（quotes）功能单一归属 — Done
- 现状：功能开关 `quotes_enabled` 散在 7 个文件：`repo/tasks/quotes.rs`（`QUOTE_TAG` + 4 fn）、`tui/app.rs`、`tui/handlers.rs`、`tui/keys.rs`、`tui/ui.rs`、`tui/render.rs`、`migrations/0010`。
- 方案：`repo::quotes` 模块持有查询 + 门控面（`enabled`/`list`/`count`/`exclude_ids`/`toggle_tag`），TUI 各层消费同一接口。
- 收益：locality（功能可一处删除，deletion test 通过）、leverage（7 个消费方）。
- 注：查询层已并入 `repo/tasks/quotes.rs`（候选 4 的一部分），剩余是门控面收拢到一处。

## 关键设计约束（不要违反）

- 时间戳一律 UTC ms INTEGER，所有时间数学走 `time.rs`，展示走 `format_local`。
- 迁移：不改已有 `migrations/*.sql`，新增文件 + `migrate.rs` 新版本块（当前 v1~v9）。
- 新事件类型：`model/event.rs` 加 const + 同步 `migrations/0001_init.sql` 注释。
- DB 路径全部由 `dirs::config_dir()` 派生，绝不硬编码；`HORAE_CONFIG_DIR` 覆盖（测试用）。
- 命令层返回 `anyhow::Result`，域错误用 `crate::error::Error`。
- 架构词汇（/codebase-design）：module / interface / depth / seam / adapter / leverage / locality。领域词汇见 `CONTEXT.md`。
- 无 ADR 目录；唯一设计文档 `docs/design-profiles-and-cloud.md`（profile 多库 + 云，未实现；`config.rs` 已落地 Phase 1）。
## 最新会话已完成 (Latest Session Accomplishments)

### 6. 通知守护进程解耦 (Notification Engine Seam)
- **问题**：原 `App::tick` (在 `tui/app.rs`) 中混合了通知时间窗计算与 OS 弹窗，职责不清晰且存在反向依赖。
- **解决**：抽取纯后台计算引擎 `src/notification.rs::NotificationEngine`。它接管了所有的任务到期探测，吐出抽象的 `NotificationEvent::InOneHour | InTenMins | Now`。
- **修正**：消除了对 `chrono` 的直接依赖，全面切换回 `crate::time::now_ms()`。解决了循环依赖（通知引擎不依赖 `commands` 层，而是把 `commands::notify::check` 推回给 UI 适配器执行）。

### 7. 加深领域 API，消除 TUI 层逻辑泄漏 (Deepen Domain APIs)
- **问题**：`src/tui/handlers.rs` 在处理番茄钟打卡 (`P`) 和检查单勾选 (`=`) 时，直接在 UI 层遍历任务、修改状态并校验前置条件。
- **解决**：在 `repo::tasks::transition` 下新增 `toggle_next_checklist_item` 和 `ensure_ready_for_pomodoro`。TUI 退化为纯粹的事件路由与适配器（按键触发深层 API，根据 `ToggleResult` 显示提示信息），成功通过 Deletion Test。

### 8. 全局更名与 CLI 极简优化 (Project Rename & CLI UX)
- **更名**：项目从 `gtp` 正式更名为 **`horae`**（希腊神话掌管时间与秩序的女神）。涵盖 `Cargo.toml`、配置路径 `~/.config/horae/`、环境变量 `HORAE_*` 及所有相关文档注释。
- **短别名**：利用 `clap` 增加了高频单字母别名：`horae c` (capture), `l` (list), `d` (done), `s` (show), `p` (pomo)。
- **无引号输入**：将 `Capture` 的 `title` 改为 `Vec<String>`（Var-args），允许用户**完全不带引号**执行极速捕获：`horae c 给花浇水 @home ~today`。

### 9. 终端看板与极客美学 (`horae stats` & TUI Splash Screen)
- **看板设计**：实现了独立的 `horae stats` 控制台看板（MOTD 风格）。
- **色彩与图形**：编写了跨界脚本，将基于图片生成的 ASCII 艺术（时间女神像）注入 Rust 源码，并采用纯正的 Catppuccin Mocha（摩卡）渐变色彩，搭配当日番茄钟进度与任务统计。
- **UI 融合**：在 TUI 层提取了 `stats` 模块的核心渲染逻辑 `get_stats_lines`，将其转化为了交互式的 TUI 专属开屏页（Splash Screen），并在下方附加了每日随机的 GTD 哲学标语。

### 10. tui/mod.rs 瘦身（开屏与测试外迁）
- **问题**：`tui/mod.rs` 膨胀到 3156 行，其中 ~330 行是 PNG/Kitty 开屏机制、~2500 行是测试，生产逻辑与测试混杂。
- **解决**：拆出 `tui/splash.rs`（PNG 加载、Kitty 图形协议、base64、按键等待 + 自带 splash_tests；仅 `show_splash` 为 `pub(super)`）与 `tui/tests.rs`（作为 `tui` 的子模块声明，`use super::*` 对父模块私有项的可见性不变，迁移零改动）。`mod.rs` 收敛为 ~210 行：模块声明、label/row 辅助、`run`/`run_app`。顺带把悬空在开屏函数上的 `run` 文档注释归位。
- **遗留**：TUI 剩余大文件为 `render.rs`(2116) / `app.rs`(1500) / `handlers.rs`(1437)，是下一个拆分候选。

## 下一步建议方向 (Proposed Next Steps)

1. **`horae focus` (或 `horae do`) — Done**
   - **目标**：终结选择困难症。直接计算并输出**目前最应该做的一件事**（综合考虑 p1/p2、有效截止期、当前上下文时间），甚至支持附加 `--start` 直接起番茄钟。
2. **`horae log` (无任务碎碎念) — Done**
   - **目标**：复用底层强大的 `task_events` append-only 时间线，支持纯粹记录带时间戳的事件/日记，不产生待办任务。