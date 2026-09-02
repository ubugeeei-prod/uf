//! The `uf` command line: the root argument parser and the dispatch table.
//!
//! Everything a command actually does, including how it renders, lives in
//! [`commands`]. The output surface itself lives in [`ui`], and the terminal
//! primitives it draws with live in `uf_term`.

mod cli;
mod commands;
mod support;
mod ui;

use std::process::ExitCode;

use anyhow::{Result, anyhow};
use camino::Utf8PathBuf;
use clap::Parser;
use clap::error::ErrorKind;
use uf_term::ColorChoice;

use crate::cli::{ColorOption, Commands};
use crate::ui::{OutputMode, Ui};

#[derive(Debug, Parser)]
#[command(version, about = "Unified Toolchain for Flow (React)")]
struct Cli {
    #[arg(long, global = true, value_name = "DIR")]
    cwd: Option<Utf8PathBuf>,
    /// When to colourise output.
    #[arg(long, global = true, value_name = "WHEN", value_enum, default_value_t = ColorOption::Auto)]
    color: ColorOption,
    #[command(subcommand)]
    command: Commands,
}

pub fn main() -> ExitCode {
    let cli = match parse_cli() {
        Ok(cli) => cli,
        Err(error) => return report_startup_error(&error),
    };
    let mode = if cli.command.wants_json() || cli.command.owns_stdout() {
        OutputMode::Json
    } else {
        OutputMode::Human
    };
    let mut ui = Ui::new(cli.color.into(), mode);

    match run(cli, &mut ui) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            ui.error(&error);
            ExitCode::FAILURE
        }
    }
}

/// Report an error raised before the output surface exists, i.e. while parsing
/// arguments. clap renders its own help and version output.
fn report_startup_error(error: &anyhow::Error) -> ExitCode {
    if let Some(error) = error.downcast_ref::<clap::Error>() {
        let _ = error.print();
        return if matches!(
            error.kind(),
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
        ) {
            ExitCode::SUCCESS
        } else {
            ExitCode::FAILURE
        };
    }
    let mut ui = Ui::new(ColorChoice::Auto, OutputMode::Human);
    ui.error(error);
    ExitCode::FAILURE
}

fn run(cli: Cli, ui: &mut Ui) -> Result<()> {
    let cwd = resolve_cwd(cli.cwd)?;

    match cli.command {
        Commands::Build { size_report } => commands::build::build(&cwd, ui, size_report),
        Commands::Check { json } => {
            commands::lint::lint_command(&cwd, ui, commands::lint::LintCommand::Check, json)
        }
        Commands::Create { command } => commands::create::create(&cwd, ui, command),
        Commands::Dev { once } => commands::dev::dev(&cwd, ui, once),
        Commands::Env { command } => commands::env::env(&cwd, ui, command),
        Commands::Exec { package, args } => commands::task::exec_package(&cwd, ui, &package, &args),
        Commands::Fmt { check } => commands::fmt::fmt(&cwd, ui, check),
        Commands::Inspect { json } => commands::inspect::inspect(&cwd, ui, json),
        Commands::Install => commands::pm::install(&cwd, ui),
        Commands::Lint { json } => {
            commands::lint::lint_command(&cwd, ui, commands::lint::LintCommand::Lint, json)
        }
        Commands::Lsp => commands::dev::lsp(),
        Commands::Prepare => commands::release::prepare(&cwd, ui),
        Commands::Publish => commands::release::publish(&cwd, ui),
        Commands::Release { bump } => commands::release::release(&cwd, ui, bump),
        Commands::Run { script, args } => commands::task::run_task(&cwd, &script, &args),
        Commands::Test { list } => commands::test::test(&cwd, ui, list),
        Commands::Use { runtime } => commands::pm::use_runtime(&cwd, ui, &runtime),
        Commands::Upgrade => commands::pm::upgrade(&cwd, ui),
    }
}

/// Parse the process arguments, expanding the `ufr` and `ufx` aliases.
fn parse_cli() -> Result<Cli> {
    let mut args = std::env::args_os().collect::<Vec<_>>();
    let bin_name = args
        .first()
        .and_then(|arg| std::path::Path::new(arg).file_stem())
        .and_then(|stem| stem.to_str())
        .unwrap_or("uf");

    match bin_name {
        "ufr" => {
            args.insert(1, "run".into());
            Cli::try_parse_from(args).map_err(Into::into)
        }
        "ufx" => {
            args.insert(1, "exec".into());
            Cli::try_parse_from(args).map_err(Into::into)
        }
        _ => Cli::try_parse_from(args).map_err(Into::into),
    }
}

fn resolve_cwd(cwd: Option<Utf8PathBuf>) -> Result<Utf8PathBuf> {
    match cwd {
        Some(path) if path.is_absolute() => Ok(path),
        Some(path) => Ok(current_dir()?.join(path)),
        None => current_dir(),
    }
}

fn current_dir() -> Result<Utf8PathBuf> {
    Utf8PathBuf::from_path_buf(std::env::current_dir()?)
        .map_err(|path| anyhow!("current directory is not UTF-8: {}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_argument_parser_is_internally_consistent() {
        Cli::command().debug_assert();
    }

    #[test]
    fn the_color_flag_is_global_and_defaults_to_auto() {
        let cli = Cli::try_parse_from(["uf", "build"]).unwrap();
        assert_eq!(cli.color, ColorOption::Auto);

        let cli = Cli::try_parse_from(["uf", "build", "--color", "never"]).unwrap();
        assert_eq!(cli.color, ColorOption::Never);

        let cli = Cli::try_parse_from(["uf", "--color", "always", "lint"]).unwrap();
        assert_eq!(cli.color, ColorOption::Always);
    }

    #[test]
    fn an_unknown_color_value_is_rejected() {
        assert!(Cli::try_parse_from(["uf", "build", "--color", "beige"]).is_err());
    }

    #[test]
    fn an_absolute_working_directory_is_used_as_is() {
        let path = Utf8PathBuf::from("/tmp/demo");
        assert_eq!(resolve_cwd(Some(path.clone())).unwrap(), path);
    }

    #[test]
    fn a_relative_working_directory_resolves_against_the_process_directory() {
        let resolved = resolve_cwd(Some(Utf8PathBuf::from("demo"))).unwrap();
        assert!(resolved.is_absolute());
        assert!(resolved.ends_with("demo"));
    }

    #[test]
    fn no_working_directory_uses_the_process_directory() {
        assert_eq!(resolve_cwd(None).unwrap(), current_dir().unwrap());
    }
}
