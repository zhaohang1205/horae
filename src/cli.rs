use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::Shell;
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "gtp",
    version,
    about = "GTD terminal task manager",
    long_about = "A GTD terminal task manager in Rust: SQLite data layer + CLI + ratatui TUI in one binary.\n\
    Every task state change is stamped with UTC-ms and appended to an append-only task_events timeline.",
    after_help = "Examples:\n  gtp                       launch the TUI\n  gtp capture \"buy milk\" --tag home --p2\n  gtp list --status next\n  gtp show <id>\n  gtp completions bash\n\nTime syntax: now, +2h, +30m, +1d, today, tomorrow, 2026-07-24 14:30\nTask refs: full id, unique id-prefix, or exact title."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,
}

impl Cli {
    /// Print a shell completion script to stdout. Handled before the database
    /// is opened so `gtp completions bash` has no side effects.
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
        after_help = "Examples:\n  gtp capture \"buy milk\" --tag home\n  gtp capture \"call mom\" --p2 --due tomorrow\n  gtp capture \"submit report\" --status scheduled --due +1d\n  gtp capture \"email boss ~today @work !a\"",
        group = clap::ArgGroup::new("priority").args(["p1", "p2", "p3"]))]
    Capture {
        title: String,
        #[arg(long = "tag", value_name = "TAG", help = "Tag to apply (repeatable)")]
        tag: Vec<String>,
        #[arg(long, help = "Priority 1 (high)")]
        p1: bool,
        #[arg(long, help = "Priority 2 (medium)")]
        p2: bool,
        #[arg(long, help = "Priority 3 (low)")]
        p3: bool,
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
        #[arg(long, help = "Print the created task as JSON")]
        json: bool,
    },
    /// List tasks (optional filters)
    #[command(
        long_about = "List tasks with optional filters. Sorting uses the effective due: \
        for recurring tasks that is the next occurrence on or after now.",
        after_help = "Examples:\n  gtp list\n  gtp list --status next\n  gtp list --status scheduled --tag work\n  gtp list --due-before +1d --json"
    )]
    List {
        #[arg(long, value_name = "STATUS", help = "Filter by status")]
        status: Option<String>,
        #[arg(long = "tag", value_name = "TAG", help = "Filter by tag (repeatable)")]
        tag: Vec<String>,
        #[arg(long, value_name = "TIME", help = "Only tasks due before this time")]
        due_before: Option<String>,
        #[arg(long, help = "Print rows as JSON")]
        json: bool,
    },
    /// Show a task with its full event timeline
    #[command(long_about = "Show a task's details plus its full append-only event timeline.")]
    Show {
        id: String,
        #[arg(long, help = "Print the task as JSON")]
        json: bool,
    },
    /// Mark actionable (next)
    Next { id: String },
    /// Mark waiting-for
    Wait { id: String },
    /// Schedule with a planned start (and optional --rrule)
    #[command(
        long_about = "Schedule a task with a planned start/end and optional recurrence (RRULE).",
        after_help = "Examples:\n  gtp schedule <id> --start tomorrow\n  gtp schedule <id> --start +1d --end +1d 14:00\n  gtp schedule <id> --start +1w --rrule 'FREQ=WEEKLY;BYDAY=MO,WE'"
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
            help = "Recurrence rule (FREQ=DAILY|WEEKLY|MONTHLY;INTERVAL=..;BYDAY=..;COUNT=..|UNTIL=..)"
        )]
        rrule: Option<String>,
    },
    /// Move to someday/maybe
    Someday { id: String },
    /// Mark done (reschedules recurring tasks to the next occurrence)
    Done { id: String },
    /// Archive (soft delete) a task
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
    /// Pomodoro commands (start, stop, daemon, waybar)
    #[command(
        long_about = "Pomodoro focus mode. `start` spawns a background daemon that ticks \
        every second, writes pomo.json and sends desktop notifications.",
        after_help = "Examples:\n  gtp pomo start <task-id>\n  gtp pomo stop\n  gtp pomo daemon\n  gtp pomo waybar"
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
            help = "Synced folder to watch (default: ~/.config/gtp/sync)"
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
        after_help = "Examples:\n  gtp export\n  gtp export --file ~/backups/gtp.json"
    )]
    Export {
        #[arg(
            long,
            value_name = "PATH",
            help = "Output path (default: gtp-backup-<date>.json)"
        )]
        file: Option<String>,
    },
    /// Import a backup, merging (or replacing) the database
    #[command(
        long_about = "Import a backup created by `gtp export`. By default it merges: \
        tasks whose id already exists are left untouched, everything else is added. \
        Pass --replace to wipe the current task data and restore the backup exactly.",
        after_help = "Examples:\n  gtp import gtp-backup-2026-08-15.json\n  gtp import --replace ~/backups/gtp.json"
    )]
    Import {
        #[arg(value_name = "FILE", help = "Path to a backup JSON file")]
        file: String,
        #[arg(long, help = "Wipe current data and restore the backup exactly")]
        replace: bool,
    },
    /// Generate shell completion scripts (bash, elvish, fish, powershell, zsh)
    #[command(
        after_help = "Usage:\n  gtp completions bash\n  gtp completions fish\n\nInstall into ~/.bashrc or ~/.config/fish/completions/"
    )]
    Completions { shell: Shell },
}
