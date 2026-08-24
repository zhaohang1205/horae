# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `horae stats` 控制台看板，使用 Catppuccin 摩卡配色的“时间女神” ASCII 像素艺术展示今日番茄钟进度与任务统计。
- TUI 启动专属“开屏页”（Splash Screen）：复用 `stats` 绝美画面与统计数据，并伴随随机 GTD 哲思标语（“大脑是用来思考的，记忆交给 HORAE”等），增强操作仪式感。
- 完整备份与还原：`horae export [--file PATH]` 把全部任务（含所有列）、
  `task_events` 时间线、标签、设置与番茄钟状态打包成一个 JSON 文件
  （默认 `horae-backup-<日期>.json`）；`horae import <FILE> [--replace]` 默认按 id
  合并（已存在整行跳过），`--replace` 清空任务数据后精确还原。备份是纯文件，
  可随 git/网盘同步；导入为原始 INSERT，不伪造时间线事件。
- Permanent delete for archived tasks: `horae purge <id>` (CLI) and `D` in the
  Archived view (TUI, y/n confirm). Purge only works on archived tasks and
  cascade-deletes their events/tags via `ON DELETE CASCADE`; no event is logged.
- Batch purge in the Archived view: enter visual mode with `v`, move with
  `j`/`k` to range-select, then `D` + y to permanently delete the whole
  selection at once. Visual mode now takes priority over the left pane so
  `j`/`k` always move the selection. ConfirmArchive also clears the visual
  selection after confirming/cancelling (was silently kept before).
- Open-source release: MIT `LICENSE`, `README.md`, `CHANGELOG.md`, Cargo
  metadata (`license`, `repository`, `rust-version`), and GitHub Actions CI +
  release workflows.
- Count caching for the guide sidebar (`App::counts`) so rendering performs
  zero database queries per frame; one-pass today/tomorrow list computation
  (`day_lists`) with a single RRULE expansion per recurring task; batched tag
  fetch (`get_tags_for_tasks`) replacing per-row queries.
- `horae completions <bash|zsh|fish|...>` generates shell completion scripts via
  `clap_complete`, handled before the database is opened so it has no side
  effects.
- Richer `--help` output: top-level `long_about` + usage examples, per-command
  examples for `capture`/`list`/`schedule`/`pomo`/`alarm`, and value-name/help
  hints for flags. `--p1`/`--p2`/`--p3` are now mutually exclusive.

### Changed

- RRULE 单字母简写：`*d`/`*w`/`*m`/`*y` 现在解析为完整的
  `FREQ=DAILY|WEEKLY|MONTHLY|YEARLY`，不再把裸 `"d"` 存进数据库。`horae capture`
  也会保留 quick-add 里的 `*rrule` 令牌（此前只有 TUI 生效）。
- 新增迁移 v6，把历史遗留的裸 `"d"`/`"w"`/`"m"`/`"y"` 循环规则规范化为完整 RRULE。

- `README.md` is now a bilingual (中文/English) user manual.
- Removed the stale `CODEBUDDY.md`; `AGENTS.md` is the single authoritative
  contributor guide, with test locations and CI usage corrected.
- List-row status now follows the human's perspective for habits/scheduled
  tasks: a recurring task checked in today is marked `✓` and shows
  `已打卡·下次:<time>` (its next occurrence), while a missed slot is treated as
  overdue and reported uniformly (`逾期X分钟/小时/天`). `effective_due` now
  returns the most recent missed occurrence for recurring tasks, so the alarm
  window, `horae list --due-before`, and the daily digest all count a missed
  today's slot as overdue.

### Fixed

- Checklist-adding keybinding was unreachable (`Shift+K` shadowed by the
  Tomorrow view). `Shift+C` now adds checklist items, and the pomodoro-length
  configuration moved to `[`; help/syntax panels updated.
- Due-notification checks mixed milliseconds and seconds, so 1h/10m/due-now
  desktop notifications never fired. The check now uses a consistent seconds
  scale and queries only tasks within the relevant window (`due_in_range`).
- TUI tests read the live `pomo.json`, so a running `horae pomo daemon` made the
  rendering tests fail. Added a test-only idle override
  (`set_pomo_idle_for_tests`).
- `relative_due` / `relative_past` built strings via repeated `replace`;
  switched to a single `format!`-style substitution.

### Performance

- `check_notifications` no longer scans the full task table on every tick;
  it selects only tasks with `due_at` in the ±1h window.
- `refresh()` loads the visible list and all its tag names in two queries
  instead of one query per row.

## [0.1.0] - unreleased

Initial development build of the GTD terminal task manager (see git history for
the full list of `feat`/`fix`/`refactor` commits).
