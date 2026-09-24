//! Following `uf: "<version>"`: the uf a project pins runs its commands.
//!
//! `rustup`'s proxies read `rust-toolchain.toml` and hand `cargo` to the
//! toolchain it names; this is the same move for uf, made by uf itself rather
//! than by a proxy, because every uf on the machine is already the thing on
//! `PATH`. Before a command line is parsed — a newer release may have commands
//! this one has never heard of — [`follow`] reads the one key that matters
//! from the project's `uf.config.js`, and when it names another release, it
//! replaces this process with that release's binary from the installer's
//! store, installing it first when the machine does not have it.
//!
//! Nothing global moves. `~/.local/bin/uf` keeps pointing where
//! `uf self-update` last pointed it, `previous-version` is untouched, and a
//! directory outside the project runs the machine's own uf. Installing is the
//! installer's, exactly as it is for `uf self-update`: see the parent module.
//!
//! # What does not follow
//!
//! - `uf self-update`, `uf self-uninstall` and `uf use`, which manage the
//!   machine's uf. Handing `uf self-update` to the pinned release would update
//!   the machine with whatever the project happened to pin.
//! - `UF_TOOLCHAIN=current`, for one command: the escape hatch for a pin that
//!   cannot be installed here, and for working on uf itself.
//!
//! `UF_TOOLCHAIN=<version>` follows that version instead of the pin, the way
//! `RUSTUP_TOOLCHAIN` does.
//!
//! # Never twice
//!
//! The followed process is told which version it was started as, in
//! [`FOLLOWED`]. A store whose `uf@0.3.0` is some other build — renamed by
//! hand, or half of a failed install — would otherwise follow the pin again,
//! into itself, forever; the second attempt is refused instead, naming the
//! directory. A nested `uf` in a project pinning a *different* release still
//! follows its own pin, because the marker names the version, not the fact.

use std::ffi::OsString;
use std::process::{Command, ExitCode};

use anyhow::{Context as _, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_term::Status;

use super::{BINARIES, OWN_VERSION, Store, acquire, binary_file, is_version, variable};
use crate::ui::Ui;

/// Overrides the pin for one command: a version, or `current`.
const OVERRIDE: &str = "UF_TOOLCHAIN";

/// Set on the process a pin was followed into, to the version it was
/// followed to. See the module documentation.
const FOLLOWED: &str = "UF_TOOLCHAIN_FOLLOWED";

/// The commands that manage the machine's uf, and so never follow a pin.
const MACHINE_COMMANDS: &[&str] = &["self-update", "self-uninstall", "use"];

/// Where a version to follow came from, for the messages.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Source {
    /// `UF_TOOLCHAIN`.
    Override,
    /// `uf` in the project's `uf.config.js`.
    Config,
}

impl Source {
    const fn describe(&self) -> &'static str {
        match self {
            Self::Override => "UF_TOOLCHAIN",
            Self::Config => "uf.config.js",
        }
    }
}

/// What this process should do with its command line.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Decision {
    /// Run it: nothing is pinned, or what is pinned is this release.
    RunHere,
    /// Hand it to another release.
    Follow { version: String, source: Source },
}

/// Hand this command line to the release the project pins, if it pins one
/// other than this.
///
/// `Ok(None)` is "run it here". On Unix a followed command replaces this
/// process and never returns; elsewhere its exit code comes back as
/// `Ok(Some(_))`.
///
/// # Errors
///
/// When the pin is not a release, when it cannot be installed, or when the
/// store's copy of it is not what it says it is.
pub(crate) fn follow() -> Result<Option<ExitCode>> {
    let args = std::env::args_os().collect::<Vec<_>>();
    let binary = invoked_as();
    let decision = decide(&args, binary, variable(OVERRIDE).as_deref(), || {
        pinned_near(&args)
    })?;
    let Decision::Follow { version, source } = decision else {
        return Ok(None);
    };
    let store = Store::from_process();
    if variable(FOLLOWED).as_deref() == Some(version.as_str()) {
        bail!(
            "{} pins uf@{version}, and {} runs as uf@{OWN_VERSION}\n\n  \
             the store's copy is not the release it is named for; \
             `uf self-update {version}` installs it again, and \
             `UF_TOOLCHAIN=current` runs the uf that was started",
            source.describe(),
            store.version_dir(&version)
        );
    }

    if !store.has_complete(&version) {
        announce_install(&version, &source);
        acquire(&store, &version).with_context(|| {
            format!(
                "{} pins uf@{version}, which is not installed and could not be\n\n  \
                 `UF_TOOLCHAIN=current` runs the uf that was started instead",
                source.describe()
            )
        })?;
    }
    let target = store
        .version_dir(&version)
        .join("bin")
        .join(binary_file(binary));
    run(&target, &args[1..], &version).map(Some)
}

/// Decide, from everything [`follow`] reads, whether to follow and where to.
///
/// `pinned` is only asked when neither the command nor the environment has
/// already answered, so a machine command reads no project at all — a broken
/// `uf.config.js` is a reason to want a different uf, not a reason to be
/// unable to install one.
fn decide(
    args: &[OsString],
    binary: &str,
    override_: Option<&str>,
    pinned: impl FnOnce() -> Result<Option<String>>,
) -> Result<Decision> {
    if binary == "uf" && subcommand(args).is_some_and(|command| MACHINE_COMMANDS.contains(&command))
    {
        return Ok(Decision::RunHere);
    }
    let (version, source) = match override_ {
        Some("current") => return Ok(Decision::RunHere),
        Some(version) => {
            let version = version.strip_prefix("uf@").unwrap_or(version);
            if !is_version(version) {
                bail!(
                    "{OVERRIDE} is {version:?}, which is not a uf version: \
                     write a release such as 0.3.0, or `current`"
                );
            }
            (version.to_owned(), Source::Override)
        }
        None => match pinned()? {
            Some(version) => (version, Source::Config),
            None => return Ok(Decision::RunHere),
        },
    };
    if version == OWN_VERSION {
        return Ok(Decision::RunHere);
    }
    Ok(Decision::Follow { version, source })
}

/// The release the project this command runs in pins, if any.
fn pinned_near(args: &[OsString]) -> Result<Option<String>> {
    let Some(start) = working_directory(args) else {
        return Ok(None);
    };
    Ok(uf_config::pinned_uf(&start)?.map(Into::into))
}

/// Where the command will run: `--cwd`, when it is given, against the process's
/// own directory.
fn working_directory(args: &[OsString]) -> Option<Utf8PathBuf> {
    let current = Utf8PathBuf::from_path_buf(std::env::current_dir().ok()?).ok()?;
    Some(match cwd_flag(args) {
        Some(cwd) => current.join(cwd),
        None => current,
    })
}

/// The value of `--cwd`, written `--cwd <dir>` or `--cwd=<dir>`, before the
/// subcommand.
fn cwd_flag(args: &[OsString]) -> Option<&Utf8Path> {
    let mut index = 1;
    while index < args.len() {
        let argument = args[index].to_str()?;
        if argument == "--cwd" {
            return args.get(index + 1)?.to_str().map(Utf8Path::new);
        }
        if let Some(value) = argument.strip_prefix("--cwd=") {
            return Some(Utf8Path::new(value));
        }
        if argument == "--color" {
            index += 2;
            continue;
        }
        if !argument.starts_with('-') {
            return None;
        }
        index += 1;
    }
    None
}

/// The subcommand, skipping the global flags and their values.
fn subcommand(args: &[OsString]) -> Option<&str> {
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
        return Some(
            argument
                .split_once('#')
                .map_or(argument, |(command, _)| command),
        );
    }
    None
}

/// Which of the three binaries this is, so the pinned release runs the same
/// one: `ufr` stays `ufr`.
fn invoked_as() -> &'static str {
    let stem = std::env::args_os()
        .next()
        .and_then(|arg| {
            std::path::Path::new(&arg)
                .file_stem()
                .and_then(|stem| stem.to_str().map(ToOwned::to_owned))
        })
        .unwrap_or_default();
    BINARIES
        .iter()
        .copied()
        .find(|name| *name == stem)
        .unwrap_or("uf")
}

/// Say why a download is about to start, before the installer draws it.
fn announce_install(version: &str, source: &Source) {
    let mut ui = Ui::new(uf_term::ColorChoice::Auto, crate::ui::OutputMode::Human);
    let line = format!(
        "{} pins uf@{version}, which is not installed; installing it",
        source.describe()
    );
    ui.render_err(|renderer, out| renderer.status(out, Status::Info, &line));
}

/// Replace this process with `target`, run with `args`.
#[cfg(unix)]
fn run(target: &Utf8Path, args: &[OsString], version: &str) -> Result<ExitCode> {
    use std::os::unix::process::CommandExt as _;

    let error = Command::new(target)
        .args(args)
        .env(FOLLOWED, version)
        .exec();
    Err(error).with_context(|| format!("failed to run {target}"))
}

/// Run `target` with `args`, and answer with its exit code.
#[cfg(not(unix))]
fn run(target: &Utf8Path, args: &[OsString], version: &str) -> Result<ExitCode> {
    let status = Command::new(target)
        .args(args)
        .env(FOLLOWED, version)
        .status()
        .with_context(|| format!("failed to run {target}"))?;
    Ok(status
        .code()
        .and_then(|code| u8::try_from(code).ok())
        .map_or(ExitCode::FAILURE, ExitCode::from))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(line: &[&str]) -> Vec<OsString> {
        std::iter::once("uf")
            .chain(line.iter().copied())
            .map(OsString::from)
            .collect()
    }

    fn pinned(version: &str) -> impl FnOnce() -> Result<Option<String>> + '_ {
        move || Ok(Some(version.to_owned()))
    }

    fn unread() -> Result<Option<String>> {
        panic!("the project was read, and nothing needed it to be")
    }

    #[test]
    fn a_pin_on_another_release_is_followed() {
        let decision = decide(&args(&["build"]), "uf", None, pinned("0.0.1")).unwrap();
        assert_eq!(
            decision,
            Decision::Follow {
                version: "0.0.1".into(),
                source: Source::Config
            }
        );
    }

    #[test]
    fn a_pin_on_this_release_runs_here() {
        let decision = decide(&args(&["build"]), "uf", None, pinned(OWN_VERSION)).unwrap();
        assert_eq!(decision, Decision::RunHere);
    }

    #[test]
    fn no_pin_runs_here() {
        let decision = decide(&args(&["build"]), "uf", None, || Ok(None)).unwrap();
        assert_eq!(decision, Decision::RunHere);
    }

    #[test]
    fn the_commands_that_manage_the_machine_never_read_the_project() {
        for line in [
            &["self-update"][..],
            &["--color", "never", "self-update", "--check"],
            &["--cwd", "elsewhere", "use", "uf@0.0.1"],
            &["self-uninstall", "--dry-run"],
        ] {
            let decision = decide(&args(line), "uf", Some("0.0.1"), unread).unwrap();
            assert_eq!(decision, Decision::RunHere, "{line:?}");
        }
    }

    #[test]
    fn a_task_named_like_a_machine_command_still_follows() {
        let decision = decide(&args(&["use"]), "ufr", None, pinned("0.0.1")).unwrap();
        assert!(matches!(decision, Decision::Follow { .. }));
    }

    #[test]
    fn the_override_wins_and_current_runs_here() {
        let decision = decide(&args(&["build"]), "uf", Some("uf@0.0.2"), unread).unwrap();
        assert_eq!(
            decision,
            Decision::Follow {
                version: "0.0.2".into(),
                source: Source::Override
            }
        );
        let decision = decide(&args(&["build"]), "uf", Some("current"), unread).unwrap();
        assert_eq!(decision, Decision::RunHere);
    }

    #[test]
    fn an_override_that_is_not_a_version_is_refused() {
        let error = decide(&args(&["build"]), "uf", Some("../bin"), unread).unwrap_err();
        assert!(error.to_string().contains("UF_TOOLCHAIN"), "{error}");
    }

    #[test]
    fn cwd_is_read_in_both_spellings_and_only_before_the_subcommand() {
        assert_eq!(
            cwd_flag(&args(&["--cwd", "app", "build"])),
            Some(Utf8Path::new("app"))
        );
        assert_eq!(
            cwd_flag(&args(&["--color", "never", "--cwd=app", "build"])),
            Some(Utf8Path::new("app"))
        );
        assert_eq!(cwd_flag(&args(&["run", "--cwd", "app"])), None);
    }

    #[test]
    fn the_subcommand_loses_its_member_selector() {
        assert_eq!(subcommand(&args(&["--cwd", "x", "use#docs"])), Some("use"));
        assert_eq!(subcommand(&args(&["--version"])), None);
    }
}
