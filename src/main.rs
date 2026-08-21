mod cli;
mod commands;
mod config;
mod db;
mod error;
mod i18n;
mod model;
mod parser;
mod repo;
#[cfg(test)]
mod testutil;
mod time;
mod tui;

use anyhow::Result;
use clap::Parser;

fn main() -> Result<()> {
    let cli = cli::Cli::parse();
    if let Some(cli::Command::Completions { shell }) = cli.command {
        cli::Cli::print_completions(shell);
        return Ok(());
    }
    if let Some(cli::Command::Profile { action }) = cli.command {
        return commands::profile::run(action);
    }
    let conn = db::conn::open(cli.profile.as_deref())?;
    commands::run(cli.command.unwrap_or(cli::Command::Tui), &conn)
}
