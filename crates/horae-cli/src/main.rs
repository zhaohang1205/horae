mod cli;
mod cli_i18n;
mod commands;

use anyhow::Result;
use clap::{CommandFactory, FromArgMatches};

fn main() -> Result<()> {
    horae_core::time::mark_boot();
    let lang = cli_i18n::detect_lang();
    let mut cmd = cli::Cli::command();
    cli_i18n::localize(&mut cmd, lang);
    let matches = cmd.get_matches();
    let cli = cli::Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
    if let Some(cli::Command::Completions { shell }) = cli.command {
        cli::Cli::print_completions(shell);
        return Ok(());
    }
    if let Some(cli::Command::Profile { action }) = cli.command {
        return commands::profile::run(action);
    }
    let launch_mode = if cli.flash
        || matches!(cli.command, Some(cli::Command::Flash))
        || matches!(cli.command, Some(cli::Command::Tui { flash: true, .. }))
    {
        horae_tui::LaunchMode::Flash
    } else if cli.normal || matches!(cli.command, Some(cli::Command::Tui { normal: true, .. })) {
        horae_tui::LaunchMode::Normal
    } else {
        horae_tui::LaunchMode::Default
    };
    let conn = horae_core::db::conn::open(cli.profile.as_deref())?;
    commands::run_with_mode(
        cli.command.unwrap_or(cli::Command::Tui {
            flash: false,
            normal: false,
        }),
        &conn,
        cli.profile.as_deref(),
        launch_mode,
    )
}
