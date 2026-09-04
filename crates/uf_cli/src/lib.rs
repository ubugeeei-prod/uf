//! The `uf` command line: the root argument parser and the dispatch table.
//!
//! Everything a command actually does, including how it renders, lives in
//! [`commands`]. The output surface itself lives in [`ui`], and the terminal
//! primitives it draws with live in `uf_term`.

mod brand;
mod cli;
mod commands;
mod suggest;
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
    /// Run as if uf had been started in DIR instead of the current directory.
    #[arg(long, global = true, value_name = "DIR")]
    cwd: Option<Utf8PathBuf>,
    /// When to colourise output.
    #[arg(long, global = true, value_name = "WHEN", value_enum, default_value_t = ColorOption::Auto)]
    color: ColorOption,
    #[command(subcommand)]
    command: Commands,
}

pub fn main() -> ExitCode {
    let (cli, target) = match parse_cli() {
        Ok(parsed) => parsed,
        Err(error) => return report_startup_error(&error),
    };
    let mode = if cli.command.wants_json() || cli.command.owns_stdout() {
        OutputMode::Json
    } else {
        OutputMode::Human
    };
    let mut ui = Ui::new(cli.color.into(), mode);

    match run(cli, target.as_deref(), &mut ui) {
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

fn run(cli: Cli, target: Option<&str>, ui: &mut Ui) -> Result<()> {
    let cwd = resolve_cwd(cli.cwd)?;
    let cwd = match target {
        Some(target) => enter_workspace(&cwd, target)?,
        None => cwd,
    };

    match cli.command {
        Commands::Build { size_report } => commands::build::build(&cwd, ui, size_report),
        Commands::Check { json } => commands::check::check(&cwd, ui, json),
        Commands::Completion { shell } => {
            commands::completion::completion(ui, shell);
            Ok(())
        }
        Commands::Complete { words } => commands::completion::complete(&cwd, ui, &words),
        Commands::Create { command } => commands::create::create(&cwd, ui, command),
        Commands::Dev { host, port } => {
            commands::dev::dev(&cwd, ui, commands::dev::DevArgs { host, port })
        }
        Commands::Env { command } => commands::env::env(&cwd, ui, command),
        Commands::Exec { package, args } => commands::task::exec_package(&cwd, ui, &package, &args),
        Commands::Fmt { check } => commands::fmt::fmt(&cwd, ui, check),
        Commands::Info => commands::info::info(&cwd, ui),
        Commands::Explain { command, json } => commands::explain::explain(&cwd, ui, &command, json),
        Commands::Inspect { json } => commands::inspect::inspect(&cwd, ui, json),
        Commands::Transform => commands::transform::transform_service(&cwd),
        Commands::Install => commands::pm::install(&cwd, ui),
        Commands::Lint { json } => {
            commands::lint::lint_command(&cwd, ui, commands::lint::LintCommand::Lint, json)
        }
        Commands::Lsp => commands::dev::lsp(),
        Commands::Prepare => commands::release::prepare(&cwd, ui),
        Commands::Publish => commands::release::publish(&cwd, ui),
        Commands::Release { bump } => commands::release::release(&cwd, ui, bump),
        Commands::Run { script, args } => match script {
            Some(script) => commands::task::run_task(&cwd, &script, &args),
            None => commands::task::list_tasks(&cwd, ui),
        },
        Commands::Test {
            list,
            watch,
            json,
            filter,
            bail,
            retry,
            threads,
            watch_interval,
            paths,
        } => commands::test::test(
            &cwd,
            ui,
            commands::test::TestArgs {
                list,
                watch,
                json,
                filter,
                bail,
                retry,
                threads,
                watch_interval,
                paths,
            },
        ),
        Commands::Use { runtime } => commands::pm::use_runtime(&cwd, ui, &runtime),
        Commands::Upgrade => commands::pm::upgrade(&cwd, ui),
    }
}

/// Parse the process arguments, expanding the `ufr` and `ufx` aliases.
///
/// Alias binaries behave like their longhand commands for real work, but keep
/// the root command's version surface. Release smoke tests exercise the binary
/// after installation, before any project or script exists, so `ufr --version`
/// and `ufx --version` must never be interpreted as `uf run --version` or
/// `uf exec --version`.
fn parse_cli() -> Result<(Cli, Option<String>)> {
    let mut args = std::env::args_os().collect::<Vec<_>>();
    let bin_name = args
        .first()
        .and_then(|arg| std::path::Path::new(arg).file_stem())
        .and_then(|stem| stem.to_str())
        .unwrap_or("uf");

    match bin_name {
        "ufr" if !args_request_root_version(&args) => args.insert(1, "run".into()),
        "ufx" if !args_request_root_version(&args) => args.insert(1, "exec".into()),
        _ => {}
    }

    let target = take_workspace_selector(&mut args);
    Ok((Cli::try_parse_from(args)?, target))
}

/// Split a `#member` selector off the subcommand, if there is one.
///
/// `uf dev#docs` runs `uf dev` in the `docs` member. The selector is written on
/// the command rather than as a flag because it changes *where* the command
/// runs rather than how, and because it reads in the order it happens — which
/// is also why it is stripped here, before clap sees an argument it would
/// otherwise reject as an unknown subcommand.
///
/// Only the subcommand carries one. A `#` anywhere else belongs to whatever
/// argument it is part of: a task name, a filter, a path.
fn take_workspace_selector(args: &mut [std::ffi::OsString]) -> Option<String> {
    let at = subcommand_index(args)?;
    let (command, target) = args[at].to_str()?.split_once('#')?;
    if target.is_empty() {
        return None;
    }
    let target = target.to_owned();
    args[at] = command.into();
    Some(target)
}

/// Where the subcommand is, skipping the global flags and their values.
///
/// `--cwd` and `--color` take a value, and that value is not the subcommand —
/// which is what `uf --cwd /tmp dev#docs` gets wrong if the first non-flag
/// argument is taken to be one.
fn subcommand_index(args: &[std::ffi::OsString]) -> Option<usize> {
    let mut index = 1;
    while index < args.len() {
        let argument = args[index].to_str()?;
        if argument == "--cwd" || argument == "--color" {
            index += 2;
            continue;
        }
        if argument.starts_with('-') || argument.is_empty() {
            index += 1;
            continue;
        }
        return Some(index);
    }
    None
}

fn args_request_root_version(args: &[std::ffi::OsString]) -> bool {
    let mut args = args.iter().skip(1);
    while let Some(arg) = args.next().and_then(|arg| arg.to_str()) {
        match arg {
            "--version" | "-V" => return true,
            "--cwd" | "--color" => {
                let _ = args.next();
            }
            arg if arg.starts_with("--cwd=") || arg.starts_with("--color=") => {}
            _ => return false,
        }
    }
    false
}

/// The directory the `#member` selector names.
///
/// # Errors
///
/// Names the members that do exist, and the closest spellings of the one that
/// does not, because "no such workspace" on its own is the least useful thing
/// this could say.
fn enter_workspace(cwd: &Utf8PathBuf, target: &str) -> Result<Utf8PathBuf> {
    let resolved = uf_config::load_config(cwd)?;
    let workspaces = uf_project::discover_workspaces(&resolved.root, &resolved.config);

    match uf_project::resolve_workspace(&workspaces, target) {
        Ok(workspace) => Ok(resolved.root.join(&workspace.path)),
        Err(available) if available.is_empty() => Err(anyhow!(
            "no workspace named {target:?}\n\n  this project has no members; a member is a \
             directory with its own uf.config.js"
        )),
        Err(available) => {
            let names = available.iter().map(compact_str::CompactString::as_str);
            let suggestions = crate::suggest::closest(target, names.clone());
            let mut message = format!("no workspace named {target:?}");
            if !suggestions.is_empty() {
                message.push_str("\n\n  did you mean: ");
                message.push_str(&suggestions.join(", "));
            }
            message.push_str("\n\n  workspaces: ");
            message.push_str(&names.collect::<Vec<_>>().join(", "));
            Err(anyhow!(message))
        }
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

    fn args(line: &[&str]) -> Vec<std::ffi::OsString> {
        line.iter().map(Into::into).collect()
    }

    #[test]
    fn a_selector_is_taken_off_the_subcommand() {
        let mut line = args(&["uf", "dev#docs"]);

        assert_eq!(take_workspace_selector(&mut line), Some("docs".to_owned()));
        assert_eq!(line, args(&["uf", "dev"]));
    }

    /// `--cwd` takes a value, and that value is not the subcommand.
    #[test]
    fn global_flags_before_the_subcommand_do_not_hide_it() {
        let mut line = args(&["uf", "--cwd", "/tmp", "--color", "never", "build#site"]);

        assert_eq!(take_workspace_selector(&mut line), Some("site".to_owned()));
        assert_eq!(
            line,
            args(&["uf", "--cwd", "/tmp", "--color", "never", "build"])
        );
    }

    /// Only the subcommand carries a selector. A `#` in a task name, a filter
    /// or a path belongs to that argument.
    #[test]
    fn a_hash_after_the_subcommand_is_left_alone() {
        let mut line = args(&["uf", "run", "build#2"]);

        assert_eq!(take_workspace_selector(&mut line), None);
        assert_eq!(line, args(&["uf", "run", "build#2"]));
    }

    #[test]
    fn a_command_with_no_selector_is_untouched() {
        let mut line = args(&["uf", "dev"]);

        assert_eq!(take_workspace_selector(&mut line), None);
        assert_eq!(line, args(&["uf", "dev"]));
    }

    /// An empty selector is a typo, not a request for the root; leaving the
    /// `#` on makes clap say so rather than silently running somewhere.
    #[test]
    fn an_empty_selector_is_not_a_selector() {
        let mut line = args(&["uf", "dev#"]);

        assert_eq!(take_workspace_selector(&mut line), None);
        assert_eq!(line, args(&["uf", "dev#"]));
    }

    #[test]
    fn a_line_with_no_subcommand_has_no_selector() {
        assert_eq!(take_workspace_selector(&mut args(&["uf"])), None);
        assert_eq!(take_workspace_selector(&mut args(&["uf", "--help"])), None);
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
