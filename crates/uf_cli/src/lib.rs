//! The `uf` command line: the root argument parser and the dispatch table.
//!
//! Everything a command actually does, including how it renders, lives in
//! [`commands`]. The output surface itself lives in [`ui`], and the terminal
//! primitives it draws with live in `uf_term`.

mod brand;
mod changelog;
mod cli;
mod commands;
mod fix;
mod menu;
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

// The root parser. A `///` comment here would become clap's `long_about` and
// print in `uf --help`, so this one is deliberately not one.
//
// `name = "uf"` because clap otherwise takes the *crate* name, and
// `uf --version` — the first thing anyone runs after installing — answered
// `uf_cli 0.0.0-alpha.2`, naming something no user has heard of. The usage
// line still comes from `argv[0]`, so `ufr --help` keeps saying `ufr`.
#[derive(Debug, Parser)]
#[command(name = "uf", version, about = "Unified Toolchain for Flow (React)")]
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
        Err(error) if is_bare_uf(&error) => match ask_what_to_run() {
            Asked::Run(parsed) => *parsed,
            Asked::Help => return report_startup_error(&error),
            // The reader opened the menu and closed it. Nothing happened, and
            // saying so would be one more line to dismiss.
            Asked::Nothing => return ExitCode::SUCCESS,
        },
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

/// What asking the reader came back with.
///
/// The parsed command line is boxed because it is two orders of magnitude
/// larger than the other two answers — `Commands` carries every flag of every
/// subcommand — and this enum is returned by value on a path that mostly
/// answers "help" or "nothing".
enum Asked {
    /// Run this, as if it had been typed.
    Run(Box<(Cli, Option<String>)>),
    /// Print the help, which is what `uf` alone did before there was a menu.
    Help,
    /// The reader changed their mind.
    Nothing,
}

/// Whether this is `uf` typed on its own, with nothing after it.
///
/// Nothing after it, and not merely "no subcommand": `uf --cwd elsewhere` is
/// also missing a subcommand, and a menu built for one directory that then ran
/// a command in another would be answering a question it had not been asked.
/// The flag is rare and the help is a fine answer to it.
fn is_bare_uf(error: &anyhow::Error) -> bool {
    let Some(error) = error.downcast_ref::<clap::Error>() else {
        return false;
    };
    if !matches!(
        error.kind(),
        ErrorKind::MissingSubcommand | ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    ) {
        return false;
    }
    std::env::args_os().count() == 1
}

/// Offer the menu, and turn what was chosen back into parsed arguments.
fn ask_what_to_run() -> Asked {
    let Ok(cwd) = std::env::current_dir() else {
        return Asked::Help;
    };
    let Ok(cwd) = Utf8PathBuf::from_path_buf(cwd) else {
        return Asked::Help;
    };

    let words = match menu::choose(&cwd) {
        menu::Chosen::Run(words) => words,
        menu::Chosen::Help => return Asked::Help,
        menu::Chosen::Nothing => return Asked::Nothing,
    };

    // Back through the same parser the typed form goes through, so a menu
    // entry cannot mean something the command line could not have said.
    let mut args = vec!["uf".to_owned()];
    args.extend(words);
    match Cli::try_parse_from(&args) {
        Ok(cli) => Asked::Run(Box::new((cli, None))),
        // Unreachable while the menu's entries are checked against the parser
        // in `menu::tests`, and the help is the honest answer if that ever
        // stops being true.
        Err(_) => Asked::Help,
    }
}

/// The exit code for "uf could not run the command at all".
///
/// Distinct from [`ExitCode::FAILURE`], which means the command ran and found
/// a problem — a failing test, a lint error, a file that needs formatting. A
/// script that wants to tell "uf is unhappy with your code" from "uf never
/// started" has nothing else to look at, and both answering `1` made the two
/// indistinguishable. See `docs/app/reference/cli/$page.mdx`.
const COULD_NOT_RUN: u8 = 2;

/// Commands uf used to have, and what to run instead of each.
///
/// A retired name is *removed* from the parser and answered here rather than
/// kept as an alias, and the difference matters for the one entry in the
/// table. Every description `uf upgrade` ever had promised either a new uf
/// binary or newer dependencies, and it did neither — it ran the workspace
/// half of an install and wrote a JSON plan (ubugeeei-prod/uf#424). So nobody
/// who typed it got what they were after, and aliasing it to the command that
/// does what it *did* would preserve the misreading for exactly the people who
/// had it. Three sentences and three names is the migration; a silent
/// redirect is not one.
const RETIRED: &[(&str, &str)] = &[(
    "upgrade",
    "`uf upgrade` has been retired. It re-read the workspace and wrote a plan \
     into `.uf/`; it never fetched anything and never replaced the uf binary, \
     whatever its name suggested.\n\n  \
     to replace uf with the newest release   uf self-update\n  \
     to move this project's dependencies     uf update\n  \
     to re-read the workspace and install    uf install\n\n  \
     see ubugeeei-prod/uf#424 and ubugeeei-prod/uf#499",
)];

/// A name the parser no longer has, carried as its own error type.
///
/// Its own type rather than an `anyhow!` string because the exit code turns on
/// it: a command uf does not have is "uf could not run the command at all",
/// the documented `2`, and not the `1` that means uf ran and disliked what it
/// found. A script that told those apart before must still tell them apart.
#[derive(Debug)]
struct Retired(&'static str);

impl std::fmt::Display for Retired {
    fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        out.write_str(self.0)
    }
}

impl std::error::Error for Retired {}

/// The retired command on this command line, if it names one.
fn retired_command(args: &[std::ffi::OsString]) -> Option<&'static str> {
    let at = subcommand_index(args)?;
    let name = args[at].to_str()?;
    RETIRED
        .iter()
        .find_map(|(retired, message)| (*retired == name).then_some(*message))
}

/// Report an error raised before the output surface exists, i.e. while parsing
/// arguments. clap renders its own help and version output.
fn report_startup_error(error: &anyhow::Error) -> ExitCode {
    if error.downcast_ref::<Retired>().is_some() {
        let mut ui = Ui::new(ColorChoice::Auto, OutputMode::Human);
        ui.error(error);
        return ExitCode::from(COULD_NOT_RUN);
    }
    if let Some(error) = error.downcast_ref::<clap::Error>() {
        let _ = error.print();
        return if matches!(
            error.kind(),
            ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
        ) {
            ExitCode::SUCCESS
        } else {
            // An argument uf could not parse is the documented `2`: nothing
            // ran, so there is no result to report a problem about.
            ExitCode::from(COULD_NOT_RUN)
        };
    }
    let mut ui = Ui::new(ColorChoice::Auto, OutputMode::Human);
    ui.error(error);
    ExitCode::FAILURE
}

/// The two `--fix` flags as the one thing they mean.
///
/// Clap gives the flags; the tier they select is a fact about the catalogue,
/// so the enum lives with it. `--fix-unsafe` conflicts with `--fix` at the
/// parser, so both being set cannot happen — and if it ever could, the wider
/// tier is the one somebody asked for out loud.
const fn fix_mode(fix: bool, fix_unsafe: bool) -> crate::fix::files::FixMode {
    use crate::fix::files::FixMode;
    match (fix, fix_unsafe) {
        (_, true) => FixMode::Unsafe,
        (true, false) => FixMode::Safe,
        (false, false) => FixMode::Report,
    }
}

fn run(cli: Cli, target: Option<&str>, ui: &mut Ui) -> Result<()> {
    let cwd = resolve_cwd(cli.cwd)?;
    let cwd = match target {
        Some(target) => enter_workspace(&cwd, target)?,
        None => cwd,
    };

    match cli.command {
        Commands::Add {
            dev,
            optional,
            peer,
            specs,
        } => commands::pm::add(
            &cwd,
            ui,
            &specs,
            crate::cli::AddTarget {
                dev,
                optional,
                peer,
            }
            .into(),
        ),
        Commands::Build {
            size_report,
            mode,
            compile,
            target,
            adapter,
        } => commands::build::build(
            &cwd,
            ui,
            size_report,
            mode.as_deref(),
            compile,
            target.as_deref(),
            adapter.map(Into::into),
        ),
        Commands::Check {
            json,
            fix,
            fix_unsafe,
            paths,
        } => commands::check::check(&cwd, ui, json, fix_mode(fix, fix_unsafe), &paths),
        Commands::Completion { shell } => {
            commands::completion::completion(ui, shell);
            Ok(())
        }
        Commands::Complete { words } => commands::completion::complete(&cwd, ui, &words),
        Commands::Init {
            template,
            lib,
            name,
            force,
        } => commands::create::scaffold(&cwd, ui, None, template, lib, name, force),
        Commands::New {
            path,
            template,
            lib,
            name,
            force,
        } => commands::create::scaffold(&cwd, ui, Some(path), template, lib, name, force),
        Commands::Create { command } => commands::create::create(&cwd, ui, command),
        Commands::Dev { host, port, mode } => {
            commands::dev::dev(&cwd, ui, commands::dev::DevArgs { host, port, mode })
        }
        Commands::Doc { out_dir, json } => commands::doc::doc(&cwd, ui, &out_dir, json),
        Commands::Env { command } => commands::env::env(&cwd, ui, command),
        Commands::Exec { yes, package, args } => {
            commands::task::exec_package(&cwd, ui, &package, &args, yes)
        }
        Commands::Fmt { check, paths } => commands::fmt::fmt(&cwd, ui, check, &paths),
        Commands::I18n { command } => commands::i18n::i18n(&cwd, ui, command),
        Commands::Info => commands::info::info(&cwd, ui),
        Commands::Explain { command, json } => commands::explain::explain(&cwd, ui, &command, json),
        Commands::Inspect { json } => commands::inspect::inspect(&cwd, ui, json),
        Commands::Transform => commands::transform::transform_service(&cwd),
        Commands::Assets => commands::assets::assets_service(&cwd),
        Commands::Install { frozen_lockfile } => commands::pm::install(&cwd, ui, frozen_lockfile),
        Commands::Lint {
            json,
            fix,
            fix_unsafe,
            paths,
        } => commands::lint::lint_command(
            &cwd,
            ui,
            commands::lint::LintCommand::Lint,
            json,
            fix_mode(fix, fix_unsafe),
            &paths,
        ),
        Commands::Clean { deps, dry_run } => commands::clean::clean(&cwd, ui, deps, dry_run),
        Commands::Lsp => commands::dev::lsp(&cwd),
        Commands::Mcp => commands::mcp::mcp(&cwd),
        Commands::Preview { host, port, mode } => {
            commands::serve::preview(&cwd, ui, commands::serve::ServeArgs { host, port, mode })
        }
        Commands::Start { host, port, mode } => {
            commands::serve::start(&cwd, ui, commands::serve::ServeArgs { host, port, mode })
        }
        Commands::Prepare { fix } => commands::prepare::prepare(&cwd, ui, fix),
        Commands::Publish => commands::release::publish(&cwd, ui),
        Commands::Release { bump, force } => commands::release::release(&cwd, ui, bump, force),
        Commands::Remove { names } => commands::pm::remove(&cwd, ui, &names),
        Commands::Routes { command } => commands::routes::routes(&cwd, ui, command),
        Commands::Run {
            mode,
            concurrency,
            force,
            why,
            script,
            args,
        } => match script {
            Some(script) => commands::task::run_task(
                &cwd,
                mode.as_deref(),
                &script,
                &args,
                commands::task::RunArgs {
                    concurrency,
                    force,
                    why,
                },
            ),
            None => commands::task::list_tasks(&cwd, ui),
        },
        Commands::Test {
            list,
            mode,
            watch,
            json,
            filter,
            bail,
            retry,
            update_snapshots,
            browser,
            threads,
            watch_interval,
            coverage,
            coverage_reporters,
            coverage_dir,
            reporter,
            reporter_outfile,
            paths,
        } => commands::test::test(
            &cwd,
            ui,
            commands::test::TestArgs {
                list,
                mode,
                watch,
                json,
                filter,
                bail,
                retry,
                update_snapshots,
                browser,
                threads,
                watch_interval,
                coverage,
                coverage_reporters,
                coverage_dir,
                reporter,
                reporter_outfile,
                paths,
            },
        ),
        Commands::Pm { command } => match command {
            cli::PmCommand::ApproveBuilds { names, dry_run } => {
                commands::pm::approve_builds(&cwd, ui, &names, dry_run)
            }
        },
        Commands::Patch { target, commit } => commands::pm::patch(&cwd, ui, &target, commit),
        Commands::Catalog { command } => match command {
            None => commands::pm::catalog(&cwd, ui),
            Some(cli::CatalogCommand::Set {
                name,
                range,
                dry_run,
            }) => commands::pm::catalog_set(&cwd, ui, &name, &range, dry_run),
        },
        Commands::Update {
            packages,
            latest,
            minor,
            patch,
            dry_run,
        } => {
            // clap's group makes at most one of the three true.
            let level = match (latest, minor, patch) {
                (true, _, _) => Some(uf_pm::ranges::Level::Major),
                (_, true, _) => Some(uf_pm::ranges::Level::Minor),
                (_, _, true) => Some(uf_pm::ranges::Level::Patch),
                _ => None,
            };
            commands::pm::update(&cwd, ui, &packages, level, dry_run)
        }
        Commands::Use { runtime } => commands::toolchain::use_runtime(&cwd, ui, &runtime),
        Commands::SelfUpdate => commands::toolchain::self_update(ui),
        Commands::Why { package } => commands::pm::why(&cwd, ui, &package),
        Commands::Ls { args } => {
            commands::pm::query(&cwd, ui, "uf ls", uf_pm::Operation::List, &args)
        }
        Commands::Audit { args } => {
            commands::pm::query(&cwd, ui, "uf audit", uf_pm::Operation::Audit, &args)
        }
        Commands::Search { terms } => {
            commands::pm::query(&cwd, ui, "uf search", uf_pm::Operation::Search, &terms)
        }
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
    // Before clap, so the answer is uf's three sentences rather than clap's
    // "unrecognized subcommand" and whatever its edit distance suggests.
    if let Some(message) = retired_command(&args) {
        return Err(Retired(message).into());
    }
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
             directory with its own uf.config.js or a package named by package.json#workspaces"
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

/// Whether `uf <name>` parses as a command at all.
///
/// For the menu's own tests: an entry that is not a command is one that fails
/// in front of whoever chose it.
#[cfg(test)]
fn parses_as_command(name: &str) -> bool {
    !matches!(
        Cli::try_parse_from(["uf", name]).err().map(|e| e.kind()),
        Some(ErrorKind::InvalidSubcommand | ErrorKind::UnknownArgument)
    )
}

/// Whether `uf <name>` needs nothing else to run.
#[cfg(test)]
fn runs_with_no_arguments(name: &str) -> bool {
    Cli::try_parse_from(["uf", name]).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn the_argument_parser_is_internally_consistent() {
        Cli::command().debug_assert();
    }

    /// Every `uf <command>` the changelog names is a command `uf` has.
    ///
    /// `uf@0.0.0-alpha.16`'s notes said "`uf profile` is a profiler rather than
    /// a wall-clock number". There is no `uf profile`: #660 shipped
    /// `crates/uf_profiler`, a library the benches and the `alloc_report`
    /// examples link against, and running the command a release note told a
    /// reader to run answers `unrecognized subcommand 'profile'`.
    ///
    /// The changelog rather than the whole repository, and the reason is what
    /// each kind of prose is for. `docs/roadmap.md` names commands on purpose
    /// that do not exist yet; a *release note* is a statement about what
    /// shipped, and a command in one is a thing a reader will type.
    #[test]
    fn the_changelog_names_no_command_uf_does_not_have() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let changelog = std::fs::read_to_string(root.join("CHANGELOG.md"))
            .expect("the repository has a changelog");
        let parser = Cli::command();
        let mut unknown: Vec<String> = Vec::new();
        for span in backticked(&changelog) {
            let Some(rest) = span.strip_prefix("uf ") else {
                continue;
            };
            if let Some(named) = unnameable_command(&parser, rest)
                && !unknown.contains(&named)
            {
                unknown.push(named);
            }
        }

        assert!(
            unknown.is_empty(),
            "CHANGELOG.md names {} command(s) uf does not have: {}",
            unknown.len(),
            unknown.join(", ")
        );
    }

    /// What the walk above accepts and refuses, spelled out.
    ///
    /// The changelog is the input this test really has, and it is a poor place
    /// to prove a negative from: a case it happens not to contain looks the
    /// same as a case the walk cannot see. These are the cases.
    #[test]
    fn a_command_path_is_walked_to_the_end_and_no_further() {
        let parser = Cli::command();
        let unnameable = |rest: &str| unnameable_command(&parser, rest);

        // Real, at both depths.
        assert_eq!(unnameable("check"), None);
        assert_eq!(unnameable("pm approve-builds"), None);
        assert_eq!(unnameable("i"), None, "an alias is a name");

        // Not real, at both depths. The second is what taking only the first
        // word could not see.
        assert_eq!(unnameable("profile"), Some("`uf profile`".to_owned()));
        assert_eq!(
            unnameable("pm approve-build"),
            Some("`uf pm approve-build`".to_owned())
        );

        // Arguments are not command names. `run` takes a task, `build` takes
        // flags, and `test` takes a suite selector after a `#`.
        assert_eq!(unnameable("run ci --why"), None);
        assert_eq!(unnameable("build --adapter static"), None);
        assert_eq!(unnameable("test#library"), None);
        assert_eq!(unnameable("--version"), None);
    }

    /// The `uf …` path in `rest`, when the parser has no such command.
    ///
    /// Walks as deep as the parser goes, so `uf pm approve-builds` is checked
    /// against `pm`'s subcommands and not merely against the top level. Taking
    /// the first word alone would have called `uf pm anything-at-all` a
    /// command, and the mistake this test exists for — `uf approve-builds` for
    /// `uf pm approve-builds` — is exactly a nested path written short.
    ///
    /// It stops where the parser stops. A command with no subcommands takes
    /// arguments, so `uf run ci --why` names the task `ci`, which is
    /// `uf.config.js`'s business and not the parser's; a flag ends the path for
    /// the same reason. That is what keeps this a check on command *names*
    /// rather than a spellchecker for everything a release note quotes.
    fn unnameable_command(parser: &clap::Command, rest: &str) -> Option<String> {
        let mut current = parser;
        let mut path: Vec<&str> = Vec::new();
        for word in rest.split_whitespace() {
            if word.starts_with('-') {
                break;
            }
            // `uf test#library` names `test`; the rest is a suite selector.
            let end = word
                .find(|character: char| !(character.is_ascii_lowercase() || character == '-'))
                .unwrap_or(word.len());
            let name = &word[..end];
            if name.is_empty() {
                break;
            }
            match current.get_subcommands().find(|command| {
                command.get_name() == name || command.get_all_aliases().any(|alias| alias == name)
            }) {
                Some(next) => {
                    path.push(name);
                    current = next;
                }
                // A word in a *command position* is a claim about the parser.
                // The same word after a command that takes no subcommands is an
                // argument, and this test has nothing to say about it.
                None if current.get_subcommands().next().is_some() => {
                    path.push(name);
                    return Some(format!("`uf {}`", path.join(" ")));
                }
                None => break,
            }
        }
        None
    }

    /// Every span between a pair of backticks on one line.
    ///
    /// Line by line, because an unpaired backtick in prose would otherwise
    /// swallow the rest of the file into one "span" and the scan above would
    /// read a single `uf ` out of a hundred kilobytes.
    fn backticked(text: &str) -> Vec<&str> {
        let mut spans = Vec::new();
        for line in text.lines() {
            let mut rest = line;
            while let Some(open) = rest.find('`') {
                let after = &rest[open + 1..];
                let Some(close) = after.find('`') else {
                    break;
                };
                spans.push(&after[..close]);
                rest = &after[close + 1..];
            }
        }
        spans
    }

    /// A retired name has to be gone from the parser, or the message naming
    /// its replacements is dead code and the command it names is whatever clap
    /// still has under that spelling.
    #[test]
    fn a_retired_name_is_not_still_a_command() {
        for (name, message) in RETIRED {
            assert!(
                !parses_as_command(name),
                "`uf {name}` is retired and the parser still accepts it"
            );
            assert!(
                retired_command(&args(&["uf", name])) == Some(*message),
                "`uf {name}` is in the table and is not answered from it"
            );
        }
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
