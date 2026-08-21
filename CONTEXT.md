# CONTEXT.md — gtp 领域词汇表

GTD 终端任务管理器（`gtp`）。核心设计理念是**时间数据化**：每个任务状态变更都盖 UTC-ms 时间戳并追加进只增的 `task_events` 时间线。

## 领域术语

- **任务 (Task)**：领域的基本单元，一个待办事项。由 `model/task.rs` 建模。
- **收件箱 (Inbox)**：任务的状态之一，捕获后的初始落点，未澄清。
- **澄清 (Clarify)**：从 Inbox 进入后续动作流，设置 `clarified_at`。
- **状态生命周期 (Status lifecycle)**：`Inbox → Next / Scheduled / Waiting / Someday / Reference → Done`；每次转换盖对应 `*_at` 时间戳。
- **排程 (Scheduling)**：给任务设定计划起点/终点 + 可选循环规则（rrule），并移入 `Scheduled` 状态。`scheduled_start_at` 是起点，`scheduled_end_at` 是终点，`rrule` 是周期规则。快速录入的 `~time` 表示排程起点。
- **循环任务 / 习惯 (Recurring task / Habit)**：带 `rrule` 的任务。完成后不结束，而是**推进锚点**重新排程到下一次发生。一天只允许**打卡**一次。
- **打卡 (Check-in)**：循环任务完成时记录 `habit_completed` 事件；同日重复打卡被拒绝。
- **有效截止 (effective_due)**：任务当前「真正关心的那个槽位」。对循环任务 = 最近一次已错过（逾期）的发生点，否则下一个发生点；对普通任务 = `due_at` 或 `scheduled_start_at`。用于排序/过滤/闹钟窗口/每日摘要。
- **发生点 (Occurrence)**：循环规则展开后的时间槽位序列中的一个元素。
- **锚点 (Anchor)**：循环任务展开的基准时间，即 `scheduled_start_at`，缺省回退 `due_at`。
- **金句 (Quote)**：一种特殊的 `reference` 任务，带 `@quote` 标签，只活在金句视图。
- **归档 (Archive)**：软删除，置 `archived_at` 与 `archive_reason`；**清除 (Purge)** 是硬删除。
- **打卡视野 / 时间视野 (Time perspective)**：TUI 的今日/明日视图，按本地日界展开循环任务。