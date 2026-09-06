//! Formatting the files uf's own printer has no business touching.
//!
//! `uf_fmt` prints Flow from the official Flow parser's syntax tree. A project
//! also holds JSON, CSS and TypeScript, and running a Flow printer over any of
//! them produces a file that no longer parses. So they go to a formatter that
//! understands them — and uf does not write that formatter, it runs one.
//!
//! That is the whole of red line 3 in one place. The provider is a choice a
//! project makes, uf ships a default because a default is a convenience, and
//! the default being replaceable is what keeps it from becoming an
//! architecture. Until this existed, `NonFlowFormatter` was an enumeration with
//! one variant that nothing read: the shape of replaceability with none of it.
//!
//! # Why a subprocess
//!
//! Linking Biome in would make uf's release depend on Biome's, which is red
//! line 5 — one package serialising the upgrade path of everything inside it.
//! Running the binary a project already has means a project can upgrade its
//! formatter without waiting for uf, and can point uf at one uf has never heard
//! of by naming its command.

use std::process::{Command, Stdio};

use camino::Utf8Path;
use compact_str::CompactString;
use uf_config::{FmtConfig, NonFlowFormatter, QuoteStyle};

#[cfg(test)]
mod tests;

/// Why a non-Flow file could not be formatted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum NonFlowError {
    /// The formatter the *project* named is not on `PATH`.
    ///
    /// Its own message is "No such file or directory", which sends a reader
    /// looking for the *source* file rather than the formatter.
    ///
    /// This is an error only because `fmt.nonFlow.formatter` says so in the
    /// project's own `uf.config.js`. The same binary missing when uf picked it
    /// is [`NonFlowOutcome::Skipped`] — see the note there.
    #[error(
        "uf.config.js asks for {formatter}, which is not installed, so {} skipped. \
         Install it, or set `fmt.nonFlow.formatter` to \"none\".",
        files_were(*count)
    )]
    NotInstalled {
        /// The command uf tried to run.
        formatter: CompactString,
        /// How many files were waiting for it.
        count: usize,
    },
    /// The formatter ran and reported a failure.
    #[error("{formatter} failed: {detail}")]
    Failed {
        /// The command uf ran.
        formatter: CompactString,
        /// What it said, trimmed to something a terminal can hold.
        detail: CompactString,
    },
}

/// What a run over the non-Flow files produced.
///
/// Three answers rather than two, because "uf could not look at these" is not
/// the same answer as "these are formatted" *or* as "these are not". Folding
/// it into either one is what made `uf fmt` fail in a project uf had just
/// scaffolded: nothing installs the default formatter there, so the first
/// `uf fmt` a reader ever ran exited 1 over the lockfile `uf install` had
/// written a moment earlier — a file uf produced itself, that nothing was
/// wrong with. See ubugeeei-prod/uf#441.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NonFlowOutcome {
    /// Everything handed over is formatted, or there was nothing to hand over.
    Formatted,
    /// Under `check`, the formatter reported that something needs formatting.
    Unformatted,
    /// The formatter uf chose is not installed, so these files are exactly as
    /// they were and nothing is known about them.
    ///
    /// A warning rather than an error, and the distinction is where the choice
    /// came from rather than what happened. uf's default is a convenience: a
    /// project that never mentioned a formatter did not ask for one, and
    /// failing its `uf fmt` over a tool it never named reports uf's own
    /// default as the project's mistake. A project that wrote
    /// `fmt.nonFlow.formatter` down stated a requirement, and an unmet
    /// requirement is [`NonFlowError::NotInstalled`].
    ///
    /// The exit code then means what it can honestly mean: every file uf was
    /// able to format is formatted. `uf fmt --check` in CI still fails on an
    /// unformatted Flow file, and a project that wants CI to insist on the
    /// non-Flow half says so by naming the formatter.
    Skipped {
        /// The command that is not installed.
        formatter: CompactString,
        /// The files nobody looked at, so the reader can see which.
        paths: Vec<String>,
    },
}

impl NonFlowOutcome {
    /// Whether every file handed over came back formatted.
    ///
    /// Skipping is not formatted and not unformatted, and a caller asking this
    /// question wants "is there anything to report", so a skip answers `false`
    /// and reports itself separately.
    #[must_use]
    pub const fn is_formatted(&self) -> bool {
        matches!(self, Self::Formatted)
    }
}

/// "1 non-Flow file was" / "2 non-Flow files were".
///
/// The count and its verb agree. "1 non-Flow files were left alone" was the
/// first sentence a new project ever saw from uf, and a tool that cannot count
/// to one is a tool a reader stops trusting about the rest of the sentence.
fn files_were(count: usize) -> String {
    if count == 1 {
        "1 non-Flow file was".to_string()
    } else {
        format!("{count} non-Flow files were")
    }
}

/// What to print when the formatter uf chose is not installed.
///
/// Says which files, in the caller's own list, and both ways out: install it,
/// or say that this project does not want one.
#[must_use]
pub fn skipped_message(formatter: &str, count: usize) -> String {
    format!(
        "{formatter} is not installed, so {} skipped. Install it, or set \
         `fmt.nonFlow.formatter` to \"none\" in uf.config.js.",
        files_were(count)
    )
}

/// How many bytes of a formatter's complaint are worth repeating.
///
/// A formatter that fails on a hundred files prints a hundred blocks, and the
/// first one is the one that explains the rest.
const MAX_DETAIL_BYTES: usize = 2_000;

/// What a provider needs to be run.
///
/// Deliberately not a trait. There is exactly one operation — hand a list of
/// paths to a command — and the difference between providers is which command
/// and which arguments. A trait here would be a vocabulary for a variation that
/// does not exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invocation {
    /// The binary to run.
    pub program: CompactString,
    /// Arguments before the paths.
    pub arguments: Vec<CompactString>,
}

/// How to run `formatter` over some files, or [`None`] to run nothing.
///
/// `check` decides whether the formatter rewrites the files or only reports
/// that it would: CI wants the report and a developer wants the rewrite, and
/// every formatter spells that differently.
///
/// `config` is uf's formatting settings, translated into whatever the provider
/// calls them. This is the part that makes the seam a seam rather than a shell
/// escape: `fmt.indentWidth` is a setting of *uf's*, and a provider that
/// ignored it would reformat every JSON file against the rule uf's own printer
/// follows. Biome indents with tabs by default and uf does not, so without this
/// the two formatters in one project disagree on every run — which is exactly
/// what happened the first time this ran over uf's own repository.
#[must_use]
pub fn invocation(
    formatter: NonFlowFormatter,
    check: bool,
    config: &FmtConfig,
) -> Option<Invocation> {
    let mut arguments: Vec<CompactString> = Vec::new();
    let program: &str = match formatter {
        NonFlowFormatter::None => return None,
        NonFlowFormatter::Biome => {
            arguments.push("format".into());
            // `--write` is the rewrite; without it `format` reports.
            if !check {
                arguments.push("--write".into());
            }
            // uf indents with spaces — `uniflowed/no-tabs` is on by default —
            // so the style is not a setting, only the width is.
            arguments.push("--indent-style=space".into());
            arguments.push(format!("--indent-width={}", config.indent_width).into());
            arguments.push(format!("--line-width={}", config.line_width).into());
            "biome"
        }
        NonFlowFormatter::Prettier => {
            arguments.push(if check {
                "--check".into()
            } else {
                "--write".into()
            });
            // Prettier's default log level narrates every file it looked at.
            arguments.push("--log-level".into());
            arguments.push("warn".into());
            arguments.push(format!("--tab-width={}", config.indent_width).into());
            arguments.push(format!("--print-width={}", config.line_width).into());
            if config.quotes == QuoteStyle::Single {
                arguments.push("--single-quote".into());
            }
            "prettier"
        }
    };

    Some(Invocation {
        program: program.into(),
        arguments,
    })
}

/// Run the configured formatter over `paths`, from `root`.
///
/// Which formatter, and whether the project named it, are one decision and are
/// both read from `config` — they used to arrive separately, and a caller that
/// passed one from the config and the other from somewhere else would decide
/// "the project asked for this" about a formatter the project never chose.
///
/// Formatting nothing is [`NonFlowOutcome::Formatted`] without running
/// anything: a project with no JSON should not need a formatter installed to
/// run `uf fmt`.
///
/// # Errors
///
/// [`NonFlowError::NotInstalled`] when the binary the project named is not on
/// `PATH` — when uf named it instead, that is
/// [`NonFlowOutcome::Skipped`] and not an error — and
/// [`NonFlowError::Failed`] when the formatter ran and failed.
pub fn run(
    root: &Utf8Path,
    paths: &[String],
    check: bool,
    config: &FmtConfig,
) -> Result<NonFlowOutcome, NonFlowError> {
    let Some(invocation) = invocation(config.non_flow.formatter, check, config) else {
        return Ok(NonFlowOutcome::Formatted);
    };
    if paths.is_empty() {
        return Ok(NonFlowOutcome::Formatted);
    }

    let output = Command::new(program_path(root, &invocation.program).as_str())
        .args(invocation.arguments.iter().map(CompactString::as_str))
        .args(paths)
        .current_dir(root)
        .stdin(Stdio::null())
        .output();

    let output = match output {
        Ok(output) => output,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            // The one place the decision is made: a tool the project asked for
            // is a requirement, and a tool uf suggested is a suggestion.
            if config.non_flow.chosen_by_project {
                return Err(NonFlowError::NotInstalled {
                    formatter: invocation.program,
                    count: paths.len(),
                });
            }
            return Ok(NonFlowOutcome::Skipped {
                formatter: invocation.program,
                paths: paths.to_vec(),
            });
        }
        Err(error) => {
            return Err(NonFlowError::Failed {
                formatter: invocation.program,
                detail: error.to_string().into(),
            });
        }
    };

    if output.status.success() {
        return Ok(NonFlowOutcome::Formatted);
    }

    // Under `check`, a non-zero exit is the formatter saying a file is not
    // formatted — the answer uf asked for, not an error. Anything else is a
    // failure, and the two are told apart by whether uf asked a question.
    if check {
        return Ok(NonFlowOutcome::Unformatted);
    }

    Err(NonFlowError::Failed {
        formatter: invocation.program,
        detail: detail_of(&output.stderr, &output.stdout),
    })
}

/// Where to find `program`: the project's own copy, or whatever is on `PATH`.
///
/// A formatter is almost always a dependency rather than a global install, and
/// a dependency lives in `node_modules/.bin`. Looking there first is what every
/// tool in this ecosystem does, and it is also the honest thing: the version a
/// project pinned is the version its files were formatted with, and a different
/// global one would reformat them on every run.
///
/// Falls back to the bare name, so a globally installed formatter still works
/// and so the "not installed" error is raised by the process spawn rather than
/// by a path check that cannot know about `PATH`.
fn program_path(root: &Utf8Path, program: &str) -> CompactString {
    let local = root.join("node_modules/.bin").join(program);
    if local.exists() {
        return local.as_str().into();
    }
    program.into()
}

/// The most useful thing a failed run said, trimmed to a readable length.
fn detail_of(stderr: &[u8], stdout: &[u8]) -> CompactString {
    let source = if stderr.iter().any(|byte| !byte.is_ascii_whitespace()) {
        stderr
    } else {
        stdout
    };
    let text = String::from_utf8_lossy(source);
    let trimmed = text.trim();
    if trimmed.len() <= MAX_DETAIL_BYTES {
        return trimmed.into();
    }
    let cut = trimmed
        .char_indices()
        .map(|(at, _)| at)
        .take_while(|at| *at <= MAX_DETAIL_BYTES)
        .last()
        .unwrap_or(0);
    format!("{}…", &trimmed[..cut]).into()
}
