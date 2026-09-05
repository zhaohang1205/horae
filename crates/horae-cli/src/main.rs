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
    let conn = horae_core::db::conn::open(cli.profile.as_deref())?;
    commands::run(
        cli.command.unwrap_or(cli::Command::Tui),
        &conn,
        cli.profile.as_deref(),
    )
}
