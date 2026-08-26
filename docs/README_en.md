# horae — GTD Terminal Task Manager

[![CI](https://github.com/zhaohang1205/horae/actions/workflows/ci.yml/badge.svg)](https://github.com/zhaohang1205/horae/actions/workflows/ci.yml)
[![Release](https://github.com/zhaohang1205/horae/actions/workflows/release.yml/badge.svg)](https://github.com/zhaohang1205/horae/actions/workflows/release.yml)
[![License: GPL-3.0](https://img.shields.io/badge/license-GPL--3.0-blue.svg)](LICENSE)

A GTD terminal task manager in Rust: SQLite data layer + CLI + ratatui TUI in one binary.

Core idea: **time-datafication** — every state change is stamped with UTC-ms and appended to an append-only `task_events` timeline, giving every task a full audit trail.

## Features

- Full GTD workflow with weekly review and today/tomorrow views
- Recurring tasks auto-reschedule on Done (RRULE + the `*2w[1,3]` shorthand)
- Tag system with auto-created custom tags
- Checklists with per-item tick/delete/reorder/rename and a progress badge in the list
- Quotes (opt-in): capture good sentences, ideas & knowledge into a dedicated view
- Pomodoro focus mode with progress ring, streaks and a waybar module
- Stats dashboard and TUI splash screen featuring Catppuccin ASCII art of the Goddess of Time and philosophical GTD slogans
- Bilingual UI (F6) and Catppuccin themes (F5)

## Installation

Requires Rust 1.89+; SQLite is bundled, no system dependencies.

```sh
cargo install --git https://github.com/zhaohang1205/horae
# or build from source:
git clone https://github.com/zhaohang1205/horae.git && cd horae
cargo build --release
```

Data lives in `~/.config/horae/` (`horae.db` + `config.json` (profile config) + `pomo.json`).

## Recommended environment

For the best experience:

- **A terminal with the Kitty graphics protocol**: the splash screen's "Goddess of Time" pixel art is rendered via the Kitty graphics protocol — Kitty, Ghostty, or WezTerm are recommended. Other terminals fall back to a plain ASCII splash automatically; nothing breaks.
- **Install a Nerd Font**: UI icons default to Nerd Font glyphs; when none is found they fall back to plain ASCII (no tofu characters). Pick any font from [Nerd Fonts](https://www.nerdfonts.com/) and set it as your terminal font.
- **Windows users**:
  - Build from source with Rust 1.89+ (`cargo build --release`); the data directory is `%APPDATA%\horae\`.
  - Desktop notifications rely on Linux's `notify-send`, so system pop-ups may not appear on Windows / macOS — but pomodoro timing and in-TUI reminders still work. Use a Kitty-protocol terminal such as WezTerm for the full splash.
  - The `HORAE_CONFIG_DIR` environment variable overrides the data directory.

## Quick start

```sh
horae                                # launch the TUI
horae capture "buy milk" --tag home  # capture into the inbox
horae list --status next             # list next actions
horae show <task-id>                 # full event timeline
```

Task refs accept a full id, a unique id-prefix, or an exact title.

## CLI

| Command | Description |
| --- | --- |
| `horae` | Launch the TUI |
| `horae capture <title> [--tag T]... [--due TIME] [--status S] [--p1\|--p2\|--p3] [--json]` | Capture (alias `c`) |
| `horae list [--status S] [--tag T]... [--due-before TIME] [--json]` | List tasks (alias `l`) |
| `horae show <id> [--json]` | Show with timeline (alias `s`) |
| `horae next\|wait\|someday\|done\|restore\|purge <id>` | Move / restore / hard-delete (alias `d` for `done`) |
| `horae schedule <id> [--start TIME] [--end TIME] [--rrule R]` | Schedule (+recurrence) |
| `horae archive <id>` / `horae restore <id>` / `horae purge <id>` | Soft delete / restore / hard delete |
| `horae tag <id> <name>` / `horae untag <id> <name>` | Manage tags |
| `horae focus [--start]` / `horae do [--start]` | Output the single top task now (can also start a pomodoro) |
| `horae log [message]` | Journal entry (no task) |
| `horae stats` | MOTD dashboard |
| `horae export [--file PATH]` | Full backup (tasks/events/tags/settings/pomodoro) |
| `horae import <FILE> [--replace]` | Merge / restore (`--replace` wipes then restores exactly) |
| `horae review` | Weekly review |
| `horae tags` | List tags |
| `horae pomo start <id> \| stop \| daemon \| waybar` | Pomodoro (alias `p`) |
| `horae alarm waybar [slot] \| next [slot] [--limit N] [--all]` | Upcoming-task reminders |
| `horae watch [--dir PATH] [--interval S] [--once]` | Phone bridge (Syncthing) |
| `horae profile <list\|new\|rename\|rm\|set-default> [--db PATH]` | Profile (data-set) management |
| `horae completions <shell>` | Generate shell completions |

Short aliases: `c`=capture, `l`=list, `s`=show, `d`=done, `p`=pomo, `do`=focus.

## TUI Keybindings

| Key | Action |
| --- | --- |
| `h` / `l` | switch pane |
| `j` / `k` | move up/down |
| `0`-`9` | switch view (8=archive, 9=tags, 0=quotes) |
| `⇧J` / `⇧K` | today / tomorrow |
| `/` | global search |
| `f` | tag filter |
| `a` | quick capture (any view) |
| `"` | add to / remove from quotes (work-status tasks become reference) |
| `Space` | toggle-select current row |
| `Ctrl+a` / `Ctrl+u` | select all / invert |
| `Enter` / `e` | full edit (title @tags ~time *rrule) |
| `x` / `w` / `s` | done / waiting / someday |
| `C` | add checklist item |
| `=` | tick next item (no auto-reset) |
| `Tab` | manage checklist items (`j/k` move, `Space` tick, `d` delete, `J/K` reorder, `e` rename; `Tab`/`Esc` exit) |
| `T` | bulk tag (visual multi-select) |
| `n` | edit notes (`$EDITOR`) |
| `P` / `S` / `[` | pomodoro start/continue/stop/config (format `work;short;long[;long-interval]`, e.g. `25;5;15;4`) |
| `A` / `D` | archive (y/n) |
| `u` | restore from archive (batch-capable) |
| `c` | add tag (Tags view) |
| `r` / `R` | weekly review (start/next) |
| `F5` / `F6` | theme / language |
| `F7` | module visibility (incl. quotes & icon style) |
| `M` | settings (profiles: new/rename/delete/set-default) |
| `F1` or `?` | shortcut help |
| `q` | quit |

## Quotes

Quotes is an **opt-in** feature (off by default; `F7` toggles it, persisted in `settings`). It's a notebook for quotes, inspirations and knowledge — not actionable tasks.

## Icon fallback

Icons default to Nerd Font glyphs. On startup horae auto-detects support (via `fc-list`) and falls back to plain ASCII when no Nerd font is found — no tofu characters. Override with `HORAE_ICONS=nerd|ascii`, or toggle the last entry of the `F7` module-visibility popup (persisted).

**How it works**

- A quote is a task tagged `@quote` with status `reference`, so it never surfaces in the inbox/action workflow.
- Quotes live **only** in the Quotes view — the Reference view (and its badge) filters out `@quote` tasks; turning the feature off restores them to Reference.
- After enabling via `F7`, the sidebar shows `[Library] 0 quotes`; press `0` to enter the Quotes view (newest first).
- **Inbox → quote**: select an item and press `"` to add `@quote` and transition it to reference, leaving the inbox.
- **Quick capture**: in the Quotes view press `a` and type a sentence to store it directly (auto `@quote`).
- **Auto-route**: capture with `@quote` anywhere (e.g. `a idea @quote`) → created as a quote and jumps to the Quotes view.
- In the Quotes view `"` removes the quote (drops the tag only). Enter/e edit, `n` notes, `T` tag, `A` archive — same as normal tasks.
- CLI: `horae capture "…" --tag quote --status reference` collects a quote from the command line.

## Journal

`horae log` reuses the append-only `task_events` timeline to record pure timestamped notes without creating a to-do — written to a dedicated system task `__journal__`, fully isolated from your tasks. `horae log "drink water"` adds one; `horae log` (without args) lists the last 50, newest first.

## Time & recurrence syntax

```
now  +2h  +30m  +1d  +1w          relative offsets
+3d 15:30                         offset + clock
today / tomorrow / day-after [HH:MM]
wed / next-fri [HH:MM]            weekday (+next week)
8/20 15:30 · 2026.8.20            slash & dot dates
HH:MM                             same-day time (past → tomorrow)
2026-07-24 [HH:MM]                absolute date & time
```

`~time` in a quick-add line sets the **schedule start** (`scheduled_start_at`, status becomes Scheduled; start only, no end); `--due` sets a soft deadline (`due_at`).

Recurrence RRULE (`*` shorthand in a quick-add line): `FREQ=DAILY|WEEKLY|MONTHLY`, `INTERVAL=2`, `BYDAY=SA,SU`, `BYMONTHDAY=1,-1` (-1 = last day of month), `COUNT=10` / `UNTIL=YYYY-MM-DD`. Shorthands: `*d`/`*w`/`*m` (daily/weekly/monthly), `*2w[1,3]` (every two weeks on Mon & Wed; 1-7 = Mon-Sun, 0 = Sun), `*m[1,-1]` (the 1st and last day of each month), `*m[1,15]` (the 1st and 15th), priority `!a`/`!b`/`!c`. Note: `FREQ=YEARLY` (`*y`) is not supported by the expansion engine and is rejected by both TUI and CLI; use a monthly recurrence instead.

## Backup

```sh
horae export                 # → horae-backup-2026-08-15.json (current dir)
horae export --file ~/gtd.json
horae import horae-backup-2026-08-15.json          # merge: existing ids skipped
horae import --replace ~/gtd.json                 # wipe then restore exactly
```

The backup is one self-contained JSON file with format/version fields, containing every task column, the `task_events` timeline, tags, settings and pomodoro state. Backup = copy this one file to git/cloud/cron for free redundancy. `--replace` is the true restore path; plain `import` merges.

## Phone sync (`horae watch`)

Bridge the phone–computer gap with Syncthing: sync `~/.config/horae/sync` to your phone and run `horae watch` on the computer. No server, no app.

```sh
horae watch                  # run forever (systemd/tmux/autostart)
horae watch --once           # single pass
horae watch --dir ~/gtd-sync # custom sync dir
```

Folder protocol (phone writes, computer consumes):

| File | Purpose |
| --- | --- |
| `capture.txt` | one quick-add line per capture: `title @tag ~time *rrule !p` |
| `actions.txt` | `done <id\|title>` · `set <id\|title> status next` · `set <id\|title> due <time>` |
| `today.md` | active-task snapshot (Next / Scheduled / Waiting / overdue) |
| `reminders/` | due/overdue task reminders (Syncthing pushes a file-change notice to the phone) |
| `*.done` | receipts of consumed lines |

Capture on the phone with any notes app pointed at this folder. Due reminders only fire while the computer is on (catch-up on boot after downtime). A later relay (`horae serve`) on any free PaaS unlocks real-time push — an optional upgrade path.

## Development

```sh
cargo test                     # tests
cargo clippy -- -D warnings    # lint (must stay clean)
cargo fmt --check              # formatting
```

See [AGENTS.md](AGENTS.md) for architecture and contributor rules.

## License

GPL-3.0, see [LICENSE](LICENSE). You are free to use, modify, and redistribute it, provided that any derivative work is released under the same GPL-3.0 license.
