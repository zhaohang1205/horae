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
    if let Some(ref t) = cli.theme {
        std::env::set_var("HORAE_THEME", t);
    }
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

    if horae_core::password::is_password_enabled(&conn)? {
        let is_tui = matches!(
            cli.command,
            None | Some(cli::Command::Tui { .. }) | Some(cli::Command::Flash)
        );
        let is_bg_status = matches!(
            &cli.command,
            Some(cli::Command::Pomo { action, .. }) if action == "daemon" || action == "waybar"
        ) || matches!(
            &cli.command,
            Some(cli::Command::Alarm { action, .. }) if action == "waybar"
        );
        let cli_password = cli
            .password
            .or_else(|| std::env::var("HORAE_PASSWORD").ok());
        if let Some(ref pass) = cli_password {
            if horae_core::password::verify_password(&conn, pass)? {
                std::env::set_var("HORAE_UNLOCKED", "1");
            } else {
                eprintln!("Error: Incorrect password for horae.");
                std::process::exit(1);
            }
        } else if horae_core::password::session_key().is_some() {
            std::env::set_var("HORAE_UNLOCKED", "1");
        } else if !is_tui
            && !is_bg_status
            && !matches!(cli.command, Some(cli::Command::Password { .. }))
        {
            use std::io::IsTerminal;
            if std::io::stdin().is_terminal() {
                let pass = commands::password::prompt_password("Enter horae password: ")?;
                if !horae_core::password::verify_password(&conn, &pass)? {
                    eprintln!("Error: Incorrect password.");
                    std::process::exit(1);
                }
            } else {
                eprintln!(
                    "Error: horae is password protected. Provide password via --password or HORAE_PASSWORD."
                );
                std::process::exit(1);
            }
        }
    }

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
