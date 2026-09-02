use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "horae",
    version,
    about = "GTD terminal task manager",
    long_about = "A GTD terminal task manager in Rust: SQLite data layer + CLI + ratatui TUI in one binary.\n\
    Every task state change is stamped with UTC-ms and appended to an append-only task_events timeline.",
    after_help = "Examples:\n  horae                       launch the TUI\n  horae capture \"buy milk\" --tag home --high\n  horae list --status next\n  horae show <id>\n  horae completions bash\n\nTime syntax: now, +2h, +30m, +1d, today, tomorrow, 2026-07-24 14:30\nDate search: four digits MMDD, for example 0829\nTask refs: full id, unique id-prefix, or exact title."
)]
pub struct Cli {
    /// Profile (data set) to use; defaults to the configured default profile.
    #[arg(long, value_name = "NAME", global = true)]
    pub profile: Option<String>,

    /// Output language for help text: `en` (default) or `zh` (中文).
    /// Also configurable via the `HORAE_LANG` environment variable.
    #[arg(long, value_name = "LANG", global = true)]
    pub lang: Option<String>,

    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Cli {
    /// Print a shell completion script to stdout. Handled before the database
    /// is opened so `horae completions bash` has no side effects.
    pub fn print_completions(shell: Shell) {
        let mut cmd = Self::command();
        let name = cmd.get_name().to_string();
        let mut buf = Vec::new();
        clap_complete::generate(shell, &mut cmd, name, &mut buf);
        if let Err(e) = std::io::Write::write_all(&mut std::io::stdout(), &buf) {
            if e.kind() != std::io::ErrorKind::BrokenPipe {
                eprintln!("failed to write completions: {e}");
            }
        }
    }
}

#[derive(Subcommand)]
pub enum Command {
    /// Capture a new item into the inbox
    #[command(long_about = "Capture a new item into the inbox. Tags auto-create on first use; \
        quick-add syntax (@tag ~time *rrule !priority) is parsed in the title.",
        after_help = "Examples:\n  horae capture \"buy milk\" --tag home\n  horae capture \"call mom\" --high --due tomorrow\n  horae capture \"submit report\" --status scheduled --due +1d\n  horae capture \"email boss ~today @work !high\"",
        visible_alias = "c",
        group = clap::ArgGroup::new("priority").args(["high", "medium", "low"]))]
    Capture {
        #[arg(num_args = 0.., help = "Task title (quotes optional)")]
        title: Vec<String>,
        #[arg(long = "clip", help = "Capture content from system clipboard")]
        clip: bool,
        #[arg(long = "tag", value_name = "TAG", help = "Tag to apply (repeatable)")]
        tag: Vec<String>,
        #[arg(long, help = "Set priority high")]
        high: bool,
        #[arg(long, help = "Set priority medium")]
        medium: bool,
        #[arg(long, help = "Set priority low")]
        low: bool,
        #[arg(
            long,
            value_name = "TIME",
            help = "Due time (now, +2h, today, 2026-07-24 14:30)"
        )]
        due: Option<String>,
        #[arg(
            long,
            value_name = "STATUS",
            help = "Initial status (inbox, next, waiting, scheduled, someday, reference)"
        )]
        status: Option<String>,
        #[arg(
            short = 'n',
            long = "notes",
            value_name = "NOTES",
            help = "Notes / description for the task"
        )]
        notes: Option<String>,
        #[arg(long, help = "Print the created task as JSON")]
        json: bool,
    },
    /// List tasks (optional filters)
    #[command(
        long_about = "List tasks with optional filters. Sorting uses the effective due: \
        for recurring tasks that is the next occurrence on or after now.",
        after_help = "Examples:\n  horae list\n  horae list --status next\n  horae list --status scheduled --tag work\n  horae list --date 0829 --json\n  horae list --due-before +1d --json",
        visible_alias = "l"
    )]
    List {
        #[arg(long, value_name = "STATUS", help = "Filter by status")]
        status: Option<String>,
        #[arg(long = "tag", value_name = "TAG", help = "Filter by tag (repeatable)")]
        tag: Vec<String>,
        #[arg(long, value_name = "TIME", help = "Only tasks due before this time")]
        due_before: Option<String>,
        #[arg(
            long,
            value_name = "MMDD",
            help = "Only tasks due on this date, e.g. 0829"
        )]
        date: Option<String>,
        #[arg(long, help = "Print rows as JSON")]
        json: bool,
    },
    /// Show a task with its full event timeline
    #[command(
        long_about = "Show a task's details plus its full append-only event timeline.",
        visible_alias = "s"
    )]
    Show {
        id: String,
        #[arg(long, help = "Print the task as JSON")]
        json: bool,
    },
    /// Modify an existing task (title, tags, due, schedule, status, notes)
    #[command(
        long_about = "Modify an existing task. Supports one-sentence quick-add syntax \
        (@tag ~time *rrule !priority) in the title/tokens and explicit flags.",
        after_help = "Examples:\n  horae modify <id> \"buy organic milk @groceries\"\n  horae modify <id> --due tomorrow\n  horae modify <id> --tag home --untag work\n  horae modify <id> --clear-due\n  horae modify <id> --clear-schedule\n  horae modify <id> --notes \"call at 3pm\"\n  horae modify <id> --edit-notes\n  horae modify <id> --status next",
        visible_aliases = ["m", "mod", "edit"],
        group = clap::ArgGroup::new("priority").args(["high", "medium", "low", "clear_priority"])
    )]
    Modify {
        #[arg(help = "Task ID, unique prefix, or exact title")]
        id: String,
        #[arg(num_args = 0.., help = "New title or quick-add update text")]
        text: Vec<String>,
        #[arg(long, help = "Explicitly set title (overrides quick-add title)")]
        title: Option<String>,
        #[arg(
            long = "tag",
            value_name = "TAG",
            help = "Add tag to task (repeatable)"
        )]
        tag: Vec<String>,
        #[arg(
            long = "untag",
            value_name = "TAG",
            help = "Remove tag from task (repeatable)"
        )]
        untag: Vec<String>,
        #[arg(long, help = "Clear all tags from task")]
        clear_tags: bool,
        #[arg(long, help = "Set priority high")]
        high: bool,
        #[arg(long, help = "Set priority medium")]
        medium: bool,
        #[arg(long, help = "Set priority low")]
        low: bool,
        #[arg(long, help = "Clear priority")]
        clear_priority: bool,
        #[arg(
            long,
            value_name = "TIME",
            help = "Due time (now, +2h, today, 2026-07-24 14:30, or 'none')"
        )]
        due: Option<String>,
        #[arg(long, help = "Clear due date")]
        clear_due: bool,
        #[arg(long, value_name = "TIME", help = "Scheduled start time (or 'none')")]
        start: Option<String>,
        #[arg(long, value_name = "TIME", help = "Scheduled end time (or 'none')")]
        end: Option<String>,
        #[arg(
            long,
            value_name = "RRULE",
            help = "Recurrence rule (FREQ=DAILY|WEEKLY|MONTHLY|YEARLY, or 'none')"
        )]
        rrule: Option<String>,
        #[arg(long, help = "Clear schedule (start, end, and rrule)")]
        clear_schedule: bool,
        #[arg(
            long,
            value_name = "STATUS",
            help = "Change status (inbox, next, waiting, scheduled, someday, reference, done)"
        )]
        status: Option<String>,
        #[arg(
            short = 'n',
            long = "notes",
            value_name = "NOTES",
            help = "Notes / description for the task"
        )]
        notes: Option<String>,
        #[arg(
            short = 'e',
            long = "edit-notes",
            help = "Open $EDITOR to edit notes interactively"
        )]
        edit_notes: bool,
        #[arg(long, help = "Print the modified task as JSON")]
        json: bool,
    },
    /// Mark actionable (next)
    Next { id: String },
    /// Mark waiting-for
    Wait { id: String },
    /// Schedule with a planned start (and optional --rrule)
    #[command(
        long_about = "Schedule a task with a planned start/end and optional recurrence (RRULE).",
        after_help = "Examples:\n  horae schedule <id> --start tomorrow\n  horae schedule <id> --start +1d --end +1d 14:00\n  horae schedule <id> --start +1w --rrule 'FREQ=WEEKLY;BYDAY=MO,WE'"
    )]
    Schedule {
        id: String,
        #[arg(long, value_name = "TIME", help = "Planned start time")]
        start: Option<String>,
        #[arg(long, value_name = "TIME", help = "Planned end time")]
        end: Option<String>,
        #[arg(
            long,
            value_name = "RRULE",
            help = "Recurrence rule (FREQ=DAILY|WEEKLY|MONTHLY|YEARLY;INTERVAL=..;BYDAY=..;BYMONTH=..;COUNT=..|UNTIL=..)"
        )]
        rrule: Option<String>,
    },
    /// Move to someday/maybe
    Someday { id: String },
    /// Mark done (reschedules recurring tasks to the next occurrence)
    #[command(visible_alias = "d")]
    Done { id: String },
    /// Archive (soft delete) a task
    #[command(visible_aliases = ["rm", "delete"])]
    Archive { id: String },
    /// Restore a previously archived (soft-deleted) task
    Restore { id: String },
    /// Permanently delete an archived task (cannot be undone)
    Purge { id: String },
    /// Add a tag to a task (preset or custom)
    Tag { id: String, name: String },
    /// Remove a tag from a task
    Untag { id: String, name: String },
    /// Weekly review helper
    Review,
    /// List all tags grouped by category
    Tags,
    /// Calculate and output the single most important task right now
    #[command(
        long_about = "Calculate and output the single most important task right now, ending decision fatigue. Considers priority (high/medium/low), effective due time, and context.",
        visible_alias = "do"
    )]
    Focus {
        #[arg(long, help = "Immediately start a pomodoro for this task")]
        start: bool,
    },
    /// Record a timestamped journal entry/event without creating a task
    #[command(
        long_about = "Record a pure timestamped event/journal entry into the timeline without creating a to-do task.",
        after_help = "Examples:\n  horae log \"Drank 3 cups of water\"\n  horae log"
    )]
    Log {
        #[arg(help = "The message to log (if omitted, lists recent logs)")]
        message: Vec<String>,
    },
    /// Pomodoro commands (start, stop, daemon, waybar)
    #[command(
        long_about = "Pomodoro focus mode. `start` spawns a background daemon that ticks \
        every second, writes pomo.json and sends desktop notifications.",
        after_help = "Examples:\n  horae pomo start <task-id>\n  horae pomo stop\n  horae pomo daemon\n  horae pomo waybar",
        visible_alias = "p"
    )]
    Pomo {
        #[arg(value_name = "ACTION", help = "start, stop, daemon, or waybar")]
        action: String,
        #[arg(value_name = "TASK_ID", help = "Task to focus on (required for start)")]
        task_id: Option<String>,
    },
    /// Upcoming-task alarm reminders (waybar, next)
    #[command(
        long_about = "Upcoming-task alarm reminders: `waybar` emits JSON for a waybar \
        module, `next` prints the nearest upcoming alarms."
    )]
    Alarm {
        #[arg(value_name = "ACTION", help = "waybar or next")]
        action: String,
        /// Slot 1/2: positions multiple waybar alarm modules; `next` skips that many
        slot: Option<usize>,
        #[arg(
            long,
            value_name = "N",
            help = "How many alarm tasks the window shows (default 2)"
        )]
        limit: Option<usize>,
        #[arg(
            long,
            help = "waybar: emit the whole window as a JSON array instead of a single slot"
        )]
        all: bool,
    },
    /// Launch the interactive TUI
    Tui,
    /// Push mobile reminders via ntfy (requires `ntfy` config in the profile)
    #[command(
        long_about = "Send task reminders to your phone via ntfy (https://ntfy.sh). \
        Configure the `ntfy` block in your profile (url, topic, optional token_env, priority, lead_minutes). \
        The `watch` daemon pushes a native notification when a timed task comes due; \
        `ntfy test` fires a sample push so you can confirm your phone receives it.",
        after_help = "Examples:\n  horae ntfy test\n  horae --profile work ntfy test"
    )]
    Ntfy {
        #[arg(value_name = "ACTION", help = "test (send a sample push)")]
        action: String,
    },
    /// Watch a Syncthing-shared folder (phone <-> computer bridge)
    #[command(
        long_about = "Watch a folder synced with the phone (e.g. via Syncthing) and reconcile \
        it against the local database. Every few seconds it: ingests new lines from \
        capture.txt into the inbox (quick-add syntax), executes action lines from \
        actions.txt, rewrites today.md with the active task list, and drops reminder \
        files into reminders/ when tasks come due.\n\
        Run it in the background (systemd / tmux / autostart); pass --once to do a single pass.",
        after_help = "Folder protocol (phone writes, computer consumes):\n\
            capture.txt   one quick-add line per capture: title @tag ~time *rrule !p\n\
            actions.txt   done <id|title> | set <id|title> status next | set <id|title> due <time>\n\
        Computer writes back:\n\
            today.md          active-task snapshot (Next / Scheduled / Waiting / overdue)\n\
            reminders/*.md    due/overdue task reminders (Syncthing pushes a file-change notice to the phone)\n\
            *.done            receipts of consumed lines"
    )]
    Watch {
        #[arg(
            long,
            value_name = "PATH",
            help = "Synced folder to watch (default: ~/.config/horae/sync)"
        )]
        dir: Option<PathBuf>,
        #[arg(
            long,
            value_name = "SECS",
            help = "Poll interval in seconds (default: 5)"
        )]
        interval: Option<u64>,
        #[arg(long, help = "Process once and exit instead of running forever")]
        once: bool,
    },
    /// Export a full backup (tasks, events, tags, settings, pomodoro) to JSON
    #[command(
        long_about = "Export every task, event, tag, setting and the pomodoro state \
        to a single JSON file — a complete restore point for the database.",
        after_help = "Examples:\n  horae export\n  horae export --file ~/backups/horae.json"
    )]
    Export {
        #[arg(
            long,
            value_name = "PATH",
            help = "Output path (default: horae-backup-<date>.json)"
        )]
        file: Option<String>,
    },
    /// Import a backup, merging (or replacing) the database
    #[command(
        long_about = "Import a backup created by `horae export`. By default it merges: \
        tasks whose id already exists are left untouched, everything else is added. \
        Pass --replace to wipe the current task data and restore the backup exactly.",
        after_help = "Examples:\n  horae import horae-backup-2026-08-15.json\n  horae import --replace ~/backups/horae.json"
    )]
    Import {
        #[arg(value_name = "FILE", help = "Path to a backup JSON file")]
        file: String,
        #[arg(long, help = "Wipe current data and restore the backup exactly")]
        replace: bool,
    },
    /// Show a terminal dashboard summary (MOTD style)
    #[command(
        long_about = "Show a terminal dashboard summary (MOTD style) with today's completed pomodoros, burndown, and pending tasks."
    )]
    Stats,
    /// Generate shell completion scripts (bash, elvish, fish, powershell, zsh)
    #[command(
        after_help = "Usage:\n  horae completions bash\n  horae completions fish\n\nInstall into ~/.bashrc or ~/.config/fish/completions/"
    )]
    Completions { shell: Shell },
    /// Manage data-set profiles (list, create, delete, rename, set default)
    #[command(
        long_about = "Profiles let you keep separate data sets (e.g. work / personal / prod1) \
        each in its own SQLite file, switched via `horae --profile <name>` or the TUI settings view. \
        This command edits the profile config (~/.config/horae/config.json) without touching any data.",
        after_help = "Examples:\n  horae profile list\n  horae profile new work\n  horae profile new prod1 --db prod1.db\n  horae profile rename work work2\n  horae profile rm prod1\n  horae profile set-default work\n  horae --profile work capture \"buy milk\""
    )]
    Profile {
        #[command(subcommand)]
        action: ProfileAction,
    },
}

#[derive(Subcommand)]
pub enum ProfileAction {
    /// List all profiles and mark the default
    List,
    /// Create a new profile (data set)
    New {
        name: String,
        #[arg(
            long,
            value_name = "PATH",
            help = "Database file (default: profiles/<name>.db)"
        )]
        db: Option<String>,
    },
    /// Rename a profile
    Rename { from: String, to: String },
    /// Delete a profile from the config (its database file is kept)
    Rm { name: String },
    /// Set the default profile used when --profile is not given
    SetDefault { name: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Cli, clap::Error> {
        Cli::try_parse_from(std::iter::once("horae").chain(args.iter().copied()))
    }

    #[test]
    fn bare_invocation_launches_tui_with_no_command() {
        let cli = parse(&[]).unwrap();
        assert!(cli.command.is_none(), "无子命令 = 启动 TUI");
        assert!(cli.profile.is_none());
    }

    #[test]
    fn capture_joins_title_words_and_flags() {
        let cli = parse(&["capture", "buy", "milk", "--tag", "home", "--medium"]).unwrap();
        match cli.command.unwrap() {
            Command::Capture {
                title,
                clip,
                tag,
                high,
                medium,
                low,
                due,
                status,
                notes,
                json,
            } => {
                assert_eq!(title.join(" "), "buy milk");
                assert!(!clip);
                assert_eq!(tag, vec!["home".to_string()]);
                assert!(!high && medium && !low);
                assert!(due.is_none() && status.is_none() && notes.is_none() && !json);
            }
            _ => panic!("应为 Capture"),
        }
    }

    #[test]
    fn capture_rejects_conflicting_priorities() {
        // ArgGroup(priority) 互斥
        assert!(parse(&["capture", "x", "--high", "--medium"]).is_err());
    }

    #[test]
    fn capture_parses_clip_flag() {
        let cli = parse(&["capture", "--clip"]).unwrap();
        match cli.command.unwrap() {
            Command::Capture { clip, title, .. } => {
                assert!(clip);
                assert!(title.is_empty());
            }
            _ => panic!("应为 Capture"),
        }
    }

    #[test]
    fn list_parses_filters() {
        let cli = parse(&[
            "list", "--status", "next", "--tag", "work", "--date", "0829", "--json",
        ])
        .unwrap();
        match cli.command.unwrap() {
            Command::List {
                status,
                tag,
                due_before,
                date,
                json,
            } => {
                assert_eq!(status.as_deref(), Some("next"));
                assert_eq!(tag, vec!["work".to_string()]);
                assert!(due_before.is_none());
                assert_eq!(date.as_deref(), Some("0829"));
                assert!(json);
            }
            _ => panic!("应为 List"),
        }
    }

    #[test]
    fn global_profile_flag_applies_to_subcommands() {
        let cli = parse(&["--profile", "work", "list"]).unwrap();
        assert_eq!(cli.profile.as_deref(), Some("work"));
        assert!(matches!(cli.command, Some(Command::List { .. })));
    }

    #[test]
    fn unknown_status_string_is_accepted_by_cli_and_validated_later() {
        // CLI 层不做枚举校验（status 是 Option<String>），由命令层报错
        let cli = parse(&["list", "--status", "bogus"]).unwrap();
        assert!(matches!(
            cli.command,
            Some(Command::List { ref status, .. }) if status == &Some("bogus".into())
        ));
    }

    #[test]
    fn modify_parses_tokens_and_flags_and_aliases() {
        let cli = parse(&[
            "modify", "1234", "new", "title", "--tag", "work", "--untag", "home", "--due",
            "tomorrow", "--high",
        ])
        .unwrap();
        match cli.command.unwrap() {
            Command::Modify {
                id,
                text,
                tag,
                untag,
                high,
                due,
                ..
            } => {
                assert_eq!(id, "1234");
                assert_eq!(text, vec!["new".to_string(), "title".to_string()]);
                assert_eq!(tag, vec!["work".to_string()]);
                assert_eq!(untag, vec!["home".to_string()]);
                assert!(high);
                assert_eq!(due.as_deref(), Some("tomorrow"));
            }
            _ => panic!("应为 Modify"),
        }

        // test aliases: m, mod, edit
        assert!(matches!(
            parse(&["m", "abc", "text"]).unwrap().command,
            Some(Command::Modify { .. })
        ));
        assert!(matches!(
            parse(&["mod", "abc", "text"]).unwrap().command,
            Some(Command::Modify { .. })
        ));
        assert!(matches!(
            parse(&["edit", "abc", "text"]).unwrap().command,
            Some(Command::Modify { .. })
        ));

        // test rm/delete aliases for Archive
        assert!(matches!(
            parse(&["rm", "abc"]).unwrap().command,
            Some(Command::Archive { .. })
        ));
        assert!(matches!(
            parse(&["delete", "abc"]).unwrap().command,
            Some(Command::Archive { .. })
        ));
    }
}
