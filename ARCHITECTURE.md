# Architecture

`horae` is a Cargo workspace of three crates. The layering rule is one-directional: the CLI and TUI depend on the core, never on each other, and the core depends on nothing inside the workspace.

```text
crates/horae-cli  ─┐
                   ├─► crates/horae-core ─► rusqlite / SQLite
crates/horae-tui  ─┘
```

See [AGENTS.md](AGENTS.md) for the rules that must not be broken (timestamps, migrations, ID resolution, event types).

## Crate map

### horae-core

Everything that is not a user interface.

| Module | Responsibility |
| --- | --- |
| `time.rs` | pure time primitives: `parse_time` (human input → UTC ms), `now_ms`, `local_day_bounds`, `format_local` |
| `schedule.rs` | the recurrence engine, in-process and pure: `occurrences`, `occurrences_since`, `anchor_ms`, `effective_due`, `next_window`, `display_due` |
| `parser.rs` | `parse_quick_add` — splits one line into `@tag` / `~time` / `*rrule` / `!priority` tokens, plus `rrule_valid` and the RRULE shorthand codec |
| `notification.rs` | pure background notification engine; evaluates effective due windows and yields `NotificationEvent`s |
| `repo/` | all SQLite access; `tasks/` is the domain module (`query` for reads, `transition` for writes, `timeline`, `quotes`) |
| `repo/mod.rs` | the mutation seam: `mutate` and `log_event` |
| `repo/state.rs` | generic `JsonStateStore<T>` for JSON state files (atomic tmp + rename, missing file → `Default`) |
| `model/` | plain structs and enums; `event.rs` holds the event-type constants |
| `db/` | `conn.rs` resolves a profile's SQLite file through `config.rs`; `migrate.rs` runs migrations keyed off `user_version` |
| `lunar/` | built-in 1900–2100 lunar calendar and 24 solar terms |
| `feishu/`, `ntfy.rs`, `notify.rs`, `pomo.rs` | integration transports and adapters |

### horae-cli

`cli.rs` defines the clap `Command` enum (one variant per action; no subcommand at all means "launch the TUI"). `commands/` holds thin handlers dispatched by `commands::run`. Notable ones:

- `watch.rs` — the phone bridge daemon
- `pomo.rs` — pomodoro daemon / waybar
- `modify.rs`, `capture.rs` — task editing and capture
- `profile.rs` — edits `config.json` only, never opens a database

### horae-tui

`tui/app/` owns state (`mod.rs` holds the shared vocabulary and the `App` struct; `data.rs` the refresh pipeline; `completion.rs` input editing; `ops.rs` task actions; `profiles.rs` settings). `tui/handlers/` handles keys grouped by `Mode`, `tui/render/` draws views, `tui/keys.rs` is the single source of truth for keybindings and their help text, `tui/icons.rs` resolves every glyph, `tui/theme.rs` holds the theme palettes (24 official Omarchy themes including Catppuccin, Tokyo Night, Gruvbox, Retro 82, Nord, Kanagawa, Hackerman, Jade Horse, etc., with Omarchy auto-detection).

UI strings are Chinese by default and localized through `crate::tr!` / `Lang`; F6 toggles language. Never hardcode UI text.

## Data model

| Table | Contents |
| --- | --- |
| `tasks` | one row per task: title, status, `*_at` timestamps, `scheduled_start_at` / `scheduled_end_at`, `rrule`, `due_at`, `priority`, `archived_at`, `archive_reason` |
| `task_events` | append-only timeline; one row per state change, schedule change, check-in or note |
| `tags` / `task_tags` | tag catalogue and the many-to-many join |
| `settings` | key/value UI preferences (language, theme, feature toggles) |

Timestamps are always UTC milliseconds stored as INTEGER. There are no formatted date strings in the database; display goes through `format_local`.

## The mutation seam

Every write goes through `repo::mutate`, which opens a transaction, computes **one** `now`, runs the caller's closure, then commits or rolls back. The audit event written by `log_event` shares that same `now`. This is what makes time-datafication consistent rather than approximate: a task's state and its timeline entry can never disagree by a millisecond.

Callers describe the change and never touch the transaction lifecycle. `mutate` is the only place in the workspace that uses `unchecked_transaction`.

## Scheduling

`time.rs` knows nothing about recurrence; `schedule.rs` knows nothing about the database. Writes stay in callers: `repo/tasks/transition.rs` calls `next_window` and then persists the result.

Invariants worth knowing:

- `anchor_ms` is the single definition of a recurring task's anchor: start-first for recurring tasks, due-first otherwise.
- `next_window` takes the next occurrence after `max(anchor, now)`, so a habit missed for N periods jumps straight to the future instead of crawling forward one period at a time.
- `occurrences_since` rolls the expansion base forward when the 366-day horizon cannot reach the caller's window — a stale habit must never disappear. `COUNT` rules never roll.
- Sorting and filtering always use `effective_due`, never the raw `due_at` column.

## Domain vocabulary

| Term | Meaning |
| --- | --- |
| Clarify | moving a task out of `Inbox`, stamping `clarified_at` |
| Schedule | giving a task a planned start/end plus an optional `rrule`, moving it to `Scheduled` |
| Occurrence | one slot in the expansion of a recurrence rule |
| Anchor | the base time a recurrence expands from |
| Effective due | the slot a task currently cares about: the most recently missed occurrence, else the next one, else `due_at` or `scheduled_start_at` |
| Check-in | completing a recurring task for the day; a second one the same day is refused |
| Archive | soft delete — sets `archived_at` and `archive_reason` |
| Purge | hard delete — `DELETE` with cascades, no event logged |
| Quote | a `reference` task tagged `@quote`, visible only in the Quotes view |

## Profiles and configuration

A profile is an independent SQLite file. `config.rs` owns `~/.config/horae/config.json`:

```json
{
  "default_profile": "default",
  "profiles": {
    "default": { "db": "horae.db" },
    "work": { "db": "profiles/work.db", "ntfy": { "...": "..." } }
  }
}
```

Profiles affect only *which file is opened* — `repo`, `model` and the migrations know nothing about them, and migrations run against any profile file on open. Credentials are referenced by environment variable name (`token_env`, `secret_env`) and never written to disk.

## JSON state files

`repo/state.rs::JsonStateStore<T>` backs `pomo.json`, the alarm state and the notification state: load from `~/.config/horae/<file>`, atomic tmp + rename on save, `Default` when the file is missing. Tests override the directory with `state::set_test_override()`, which is sticky and sampled at store construction; `JsonStateStore::at(path)` is the only way to bypass it.

## Migrations

Migration files live in `crates/horae-core/migrations/` and are applied by `db/migrate.rs` based on SQLite `user_version`. The current mapping:

| `user_version` | Files applied |
| --- | --- |
| 1 | 0001, 0002 |
| 2 | +0003 |
| 3 | +0004 |
| 4 | +0005 |
| 5 | +0006 |
| 6 | +0007 |
| 7 | +0008 |
| 8 | +0009 |
| 9 | +0010 |
| 10 | +0011 |
| 11 | +0012 |
| 12 | +0013 |
| 13 | +0014 |

Never edit an existing migration file. Add a new one plus a version block in `migrate.rs`. Migration SQL is idempotent where possible (`IF NOT EXISTS`, `INSERT OR IGNORE`); 0011's `DROP COLUMN` relies on the version guard instead. Adding an event type means adding a constant in `model/event.rs` **and** keeping the `task_events` comment in `0001_init.sql` in sync.

## Not implemented

Cloud sync is designed but not built. The intended shape is a `cloud` field per profile backed by Turso/libSQL as a read replica — the local file stays the primary database so the tool keeps working offline, and sync is manual or timer-driven alongside the existing Syncthing folder bridge. No decision has been made to start it.
