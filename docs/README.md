# horae

GTD 终端任务管理器（Rust：SQLite 数据层 + CLI + ratatui TUI）。核心设计是**时间数据化（time-datafication）**：每次任务状态变更都打上 UTC 毫秒时间戳，追加到只写（append-only）的 `task_events` 时间线。

A GTD terminal task manager in Rust (SQLite data layer + CLI + ratatui TUI). Core idea: **time-datafication** — every state change is stamped with UTC-ms and appended to an append-only `task_events` timeline.

## 文档 / Documentation

- 中文手册：[README_cn.md](README_cn.md)
- English manual：[README_en.md](README_en.md)
- 贡献者指南（英文）：[AGENTS.md](AGENTS.md)
- 架构交接（中文）：[architecture-handoff.md](architecture-handoff.md)
- 领域词汇（中文）：[CONTEXT.md](CONTEXT.md)
- 专注推荐算法（中文）：[focus-algorithm.md](focus-algorithm.md)
- Profile 多库设计（中文）：[design-profiles-and-cloud.md](design-profiles-and-cloud.md)
- AI 技能包（教学 + 卖点 + 别名安装，中文）：[skills/horae/](skills/horae/SKILL.md)
- 更新日志（中文/English）：[CHANGELOG.md](CHANGELOG.md)

## 许可证 / License

GPL-3.0，见 [LICENSE](LICENSE)。
