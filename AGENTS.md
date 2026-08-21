# AGENTS.md

GTD terminal task manager (`gtp`) — a Rust binary (edition 2021, **no lib target**) combining a SQLite data layer, a CLI, and a ratatui TUI. Core design idea: **time-datafication** — every task state change is stamped with a UTC-ms timestamp and appended to an append-only `task_events` timeline.

## Commands

- Build/run: `cargo run -- capture "buy milk" --tag home` — note the `--` before subcommand args.
- Test: `cargo test` — unit tests live in `src/tui/mod.rs`, `src/parser.rs`, and `src/commands/alarm.rs`; run one with `cargo test <name>`.
- Lint/format: `cargo clippy`, `cargo fmt`. Clippy must stay clean with `-- -D warnings`.
- CI: GitHub Actions (`.github/workflows/ci.yml`) runs fmt, clippy `-D warnings`, tests, and an MSRV job; `release.yml` builds tagged releases (`v*`). `tempfile` is available for test DBs.

## Architecture (layers depend only inward)

- `cli.rs` — clap `Command` enum, one variant per CLI action. Default command (no args) is `Tui`. `gtp completions <shell>` is intercepted in `main.rs` before the DB opens (zero side effects); the `commands::run` arm is intentionally unreachable.
- `config.rs` — profile config (`~/.config/gtp/config.json`): named profiles each map to a SQLite file (`default` → legacy `gtp.db`), `default_profile` selector. `gtp profile` subcommands are intercepted in `main.rs` before the DB opens (like completions); the `commands::run` arm is intentionally unreachable.
- `commands/` — thin handlers; `mod.rs::run` dispatches. `profile.rs` edits config.json only (never opens a DB). `pomo.rs` handles the daemon/waybar logic. `watch.rs` is the phone bridge: polls a Syncthing-shared folder (default `~/.config/gtp/sync`) every few seconds, ingests `capture.txt` / executes `actions.txt`, rewrites `today.md`, drops `reminders/*.md` when tasks come due. It polls instead of using inotify because Syncthing writes via atomic tmp-file renames; the `.processing`/`.done` file dance gives crash-safe, dedup'd consumption (identical duplicate lines are deduped — a known trade-off).
- `repo/` — rusqlite data access; `tasks.rs` holds most domain logic (`create_capture`, `transition`, `schedule`, `resolve_project`, `list`). `mod.rs::log_event` writes the audit timeline.
- `model/` — plain structs + enums; `event.rs` holds event-type string consts.
- `db/` — `conn.rs::open(name)` resolves a profile's SQLite file via `config.rs` (relative paths under `~/.config/gtp`), then runs migrations keyed off SQLite `user_version`. `None` → the configured default profile.
- `time.rs` — `parse_time` (human input: `now`, `+2h`, `today`, `2026-07-24 14:30` → UTC ms), self-contained `rrule_occurrences` (no external crate), `format_local`.
- `parser.rs` — `parse_quick_add`: splits input into `@tag` words and `~time` words.
- `tui/` — `app.rs`, `handlers.rs` (key handling), `render.rs`, `ui.rs`, `calendar.rs`, `theme.rs` (Catppuccin), `i18n.rs`. **UI strings are Chinese by default and localized via `crate::tr!` / `Lang` (F6 toggles to English, F5 toggles theme); never hardcode UI text.**

## Non-obvious rules (violating these breaks things)

- **Timestamps**: store UTC ms INTEGER, never formatted strings. All time math goes through `time.rs`; display via `format_local`.
- **Migrations**: never edit existing `migrations/*.sql`. Add a new file plus a new version block in `migrate.rs` (currently v1 = 0001+0002, v2 = +0003, v3 = +0004, v4 = +0005, v5 = +0006, v6 = +0007, v7 = +0008, v8 = +0009, v9 = +0010). Migration SQL is idempotent (IF NOT EXISTS / INSERT OR IGNORE).
- **New event types**: add a const in `model/event.rs` AND keep the `task_events` comment in `migrations/0001_init.sql` in sync.
- **DB paths**: `~/.config/gtp/gtp.db`, `~/.config/gtp/config.json` and `~/.config/gtp/pomo.json` all derive from `dirs::config_dir()` — never hardcode.
- **ID resolution**: commands accept a task id, a unique id-prefix, or an exact title (`resolve_project`).
- **Archive is soft-delete**: sets `archived_at` and `archive_reason` (`completed` when the task was Done at archive time, else `deleted`); list queries filter `archived_at IS NULL`. `Restore` clears both. The Archived view shows reason + archive time, never the old status or overdue.
- **Purge is a hard delete**: `gtp purge` / TUI `D` in the Archived view only works on archived tasks (non-archived → `Error::NotArchived`). It's a plain `DELETE FROM tasks` — `task_events`, `task_tags`, and child tasks cascade via `ON DELETE CASCADE`, and there is deliberately no purge event logged (the whole timeline is deleted with it). Supports batch: `v` toggles visual mode (selection range), then `D` + y purges all selected rows; visual mode makes `j`/`k` move the selection even when the left pane is active.
- **Recurring tasks**: a task with `rrule` reschedules to its next occurrence on `Done` instead of completing. Sorting/filtering uses `effective_due` (`commands/mod.rs`), not the raw due column.
- **Status lifecycle**: `Inbox → Next / Scheduled / Waiting / Someday / Reference → Done`; `transition` in `repo/tasks.rs` sets the matching `*_at` timestamp.
- **Pomodoro**: `pomo start` spawns a background `gtp pomo daemon` (ticks every second, writes `pomo.json`, sends `notify-send`). `pomo waybar` emits JSON for a waybar module. `kill_daemon` (pomo.rs) deliberately waits for the old process to exit — concurrent daemons corrupt `pomo.json`; don't "optimize" that away.
- **Tags**: system presets seeded in migrations (`home`, `work`, `learning`, `errands`, `calls`, `computer`, `quick`, `focus`; priorities `p1`/`p2`/`p3`). Custom tags auto-create on first use (`find_or_create_tag`).
- **金句 (Quotes) feature**: a quote is a task tagged `@quote` (system tag, seeded in `0010`) with status `reference`, so it never surfaces in the inbox/action workflow. The Quotes view (`0` key, sidebar `[Library]`) lists them via `repo/tasks::list_quotes` (newest first; display due = `created_at`). **Quotes live only in the Quotes view**: when the feature is enabled, the Reference view and its sidebar badge exclude `@quote` tasks (`tasks::quote_task_ids` + `count_quotes_in_status`); when disabled, `@quote` is a plain tag again. Feature is gated by settings key `quotes` (`"1"`/`"0"`, TUI `F7` toggle, default off) — gate the `0` digit, the `"` shortcut, the sidebar group, and the `KeyDef`s on `app.quotes_enabled`. `"` toggles the tag in/out (work-status tasks are transitioned to `reference` when quoted); capturing with `@quote` in the input auto-creates as reference and jumps to the Quotes view. See `QUOTE_TAG` in `repo/tasks.rs`.

## Errors

Domain errors use `crate::error::Error` (thiserror); command handlers return `anyhow::Result`.

## Existing docs

`README.md` is the bilingual user manual (中文/English). This file is the authoritative contributor guide.
