//! Applying the catalogue to files on disk.
//!
//! The catalogue in [`super`] answers "what is the edit". This answers "what
//! is written", which is a different question with three guarantees attached
//! to it, because a batch fixer that gets any of them wrong destroys work:
//!
//! 1. **What is written parses.** Every candidate goes through
//!    `uf_fmt::format_source`, which prints from the syntax tree and therefore
//!    fails on text the parser refuses. A file whose candidate does not parse
//!    is left exactly as it was and named in the report.
//! 2. **What is written is what `uf fmt` would keep.** A file that passed
//!    `uf fmt --check` before the fix passes it after. Four bytes is enough to
//!    push a line past the print width, so a file the formatter was content
//!    with is reprinted around the edit — by the formatter, from the syntax
//!    tree, which is the only thing entitled to an opinion about where that
//!    line breaks. A file `uf fmt` already wanted to change is written as the
//!    edit left it: imposing a layout nobody asked for is a different command.
//!    Either way `uf lint --fix` is never the reason `uf fmt --check` starts
//!    failing.
//! 3. **Running it twice is running it once.** Each file is linted, fixed and
//!    linted again until the text stops changing, so the second run of
//!    `uf lint --fix` finds the fixpoint the first one left. Text that was
//!    never linted again is not written at all: if the linter stops being able
//!    to run, the run stops with it and the file keeps what it had.
//!
//! The loop is also how two fixes over the same bytes are handled:
//! [`super::plan`] applies one of an overlapping pair and drops the other, and
//! the next turn of this loop lints the result and plans the dropped one
//! again against text that now exists.
//!
//! Nothing here touches a file the catalogue has no fix for, and nothing here
//! reformats: the answer to `uniflowed/no-tabs` is `uf fmt`, which is its own
//! command with its own `--check`, and a lint fixer that silently reprinted
//! every file with a tab in it would be doing that command's job on files it
//! was not pointed at. Findings whose answer is the formatter are counted and
//! said out loud instead.

use anyhow::{Context, Result};
use camino::Utf8Path;
use uf_config::{ResolvedConfig, load_config};
use uf_lint::{LintError, LintReport, SourceFile, lint_source};
use uf_project::{ProjectFile, scan_selected_source_files};

use super::{FORMATTED_AWAY, Safety, apply, fix_for, plan};
use crate::support::selects;

/// How many times one file is linted, fixed and linted again before the fixer
/// gives up on it.
///
/// A fix removes the diagnostic that asked for it, so one turn is the normal
/// case and a second is only needed when an overlapping pair was split across
/// turns. The bound is here so that a future fix that somehow re-reports
/// itself costs a message rather than a hung command.
const MAX_ROUNDS: usize = 8;

/// Which fixes a run is allowed to write.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FixMode {
    /// Write nothing. The default, and what a `--check`-like run does.
    Report,
    /// Apply every [`Safety::Safe`] fix.
    Safe,
    /// Apply every fix, including the ones whose correctness rests on
    /// something the rule could not check.
    Unsafe,
}

impl FixMode {
    /// Whether this mode writes files at all.
    pub(crate) const fn writes(self) -> bool {
        matches!(self, Self::Safe | Self::Unsafe)
    }

    /// Whether [`Safety::Unsafe`] fixes are included.
    const fn allows_unsafe(self) -> bool {
        matches!(self, Self::Unsafe)
    }
}

/// What one fixing pass over a project did.
#[derive(Debug, Default)]
pub(crate) struct FixSummary {
    /// How many individual edits were written.
    pub(crate) applied: usize,
    /// The files whose text changed, by project-relative path.
    pub(crate) changed: Vec<String>,
    /// Files a fix was planned for and refused, as `path: reason`.
    ///
    /// A refusal is a bug in a fix rather than a problem with the file, so it
    /// is reported rather than counted: the file is untouched either way.
    pub(crate) refused: Vec<String>,
    /// Findings left over that have an [`Safety::Unsafe`] fix this run did not
    /// apply, which is what `--fix-unsafe` would have written.
    pub(crate) needs_unsafe: usize,
    /// Findings left over whose answer is `uf fmt` rather than an edit.
    pub(crate) needs_fmt: usize,
}

/// Apply `mode`'s fixes to every Flow file in the project that `paths` selects.
///
/// The report the caller prints afterwards comes from linting the project
/// again rather than from anything counted here, so what a reader is shown is
/// what `uf lint` would say if they ran it themselves.
pub(crate) fn fix_project(cwd: &Utf8Path, paths: &[String], mode: FixMode) -> Result<FixSummary> {
    let resolved = load_config(cwd)?;
    // The same discovery `uf lint` runs, told the same paths: a `.gitignore`d
    // file that was named is reported by the report this run prints
    // afterwards, so a fix pass that could not see it would print a diagnostic
    // it had silently declined to fix.
    let scan = scan_selected_source_files(&resolved.root, &resolved.config, paths)?;
    let mut files: Vec<ProjectFile> = scan
        .files
        .into_iter()
        .filter(|file| selects(paths, &file.relative_path))
        .collect();
    fix_files(&resolved, &mut files, mode)
}

/// Apply `mode`'s fixes to the Flow files among `files`, in place.
///
/// Each file's [`ProjectFile::source`] is updated to the text that was
/// written, so a caller that already holds the project's sources — `uf
/// prepare` does — does not have to read them again to see what it now has.
///
/// Non-Flow files are ignored rather than rejected: the catalogue's rules are
/// Flow rules, and the verification step prints from a Flow syntax tree, so
/// handing it `package.json` would be asking a Flow formatter what a JSON file
/// should look like.
pub(crate) fn fix_files(
    resolved: &ResolvedConfig,
    files: &mut [ProjectFile],
    mode: FixMode,
) -> Result<FixSummary> {
    let mut summary = FixSummary::default();
    for file in files.iter_mut().filter(|file| file.kind.is_flow()) {
        fix_file(resolved, file, mode, &mut summary)?;
    }
    summary.changed.sort_unstable();
    Ok(summary)
}

/// Lint, fix and re-lint one file until its text stops changing.
fn fix_file(
    resolved: &ResolvedConfig,
    file: &mut ProjectFile,
    mode: FixMode,
    summary: &mut FixSummary,
) -> Result<()> {
    fix_file_with(
        &|source| lint_source(source, &resolved.config),
        resolved,
        file,
        mode,
        summary,
    )
}

/// [`fix_file`], with the linter each round asks handed in.
///
/// A parameter for one reason. The failure this loop has to get right is the
/// Flow parser refusing to run *at all* — a file it refuses is a `flow/syntax`
/// diagnostic and not an error — and that cannot be arranged from any input,
/// so without a seam here the handling below is code no test can reach. A
/// guarantee nothing can check is one nobody can keep.
fn fix_file_with(
    lint: &dyn Fn(&SourceFile) -> Result<LintReport, LintError>,
    resolved: &ResolvedConfig,
    file: &mut ProjectFile,
    mode: FixMode,
    summary: &mut FixSummary,
) -> Result<()> {
    let mut text = file.source.clone();
    // Asked once, of the file as it arrived: a file `uf fmt` already wants to
    // change is not one this pass can be blamed for, but a file it was happy
    // with has to stay that way.
    let was_formatted = uf_fmt::format_source(&text, &resolved.config.fmt)
        .is_ok_and(|formatted| !formatted.changed);
    let mut applied = 0;
    let mut leftover = Leftover::default();

    for _ in 0..MAX_ROUNDS {
        let source = SourceFile {
            path: file.relative_path.clone(),
            source: text.clone(),
        };
        // A linter that could not run is not a file with nothing left to fix.
        // This used to break out of the loop, and the text the earlier rounds
        // had accumulated — which nothing has linted since — was written below
        // anyway: a file left short of the fixpoint the module header promises,
        // with no one told. The Flow parser refusing a *file* is a
        // `flow/syntax` diagnostic and arrives as a report; the only failures
        // that arrive here are the parser refusing to run at all, and there is
        // no answer to that but to stop. The original text stays on disk
        // because nothing is written before the loop ends.
        let report = lint(&source)
            .with_context(|| format!("failed to lint {} while fixing it", file.relative_path))?;
        leftover = Leftover::count(&report.diagnostics, &text, mode);

        let fixes = plan(&text, &report.diagnostics, mode.allows_unsafe());
        if fixes.is_empty() {
            break;
        }
        let candidate = apply(&text, &fixes);
        if candidate == text {
            break;
        }
        let settled = match settle(candidate, was_formatted, &resolved.config.fmt) {
            Ok(settled) => settled,
            Err(reason) => {
                summary
                    .refused
                    .push(format!("{}: {reason}", file.relative_path));
                break;
            }
        };
        applied += fixes.len();
        text = settled;
    }

    summary.needs_unsafe += leftover.needs_unsafe;
    summary.needs_fmt += leftover.needs_fmt;
    if text != file.source {
        std::fs::write(&file.absolute_path, &text)
            .with_context(|| format!("failed to write {}", file.absolute_path))?;
        summary.applied += applied;
        summary.changed.push(file.relative_path.clone());
        file.source = text;
    }
    Ok(())
}

/// Findings a pass is going to leave behind, and why.
#[derive(Debug, Default)]
struct Leftover {
    /// Has an unsafe fix this mode did not ask for.
    needs_unsafe: usize,
    /// Is answered by `uf fmt` rather than by an edit.
    needs_fmt: usize,
}

impl Leftover {
    fn count(diagnostics: &[uf_lint::Diagnostic], text: &str, mode: FixMode) -> Self {
        let lines: Vec<&str> = text.lines().collect();
        let mut leftover = Self::default();
        for diagnostic in diagnostics {
            if FORMATTED_AWAY.contains(&diagnostic.rule) {
                leftover.needs_fmt += 1;
                continue;
            }
            if mode.allows_unsafe() {
                continue;
            }
            let has_unsafe = lines
                .get(diagnostic.line.saturating_sub(1))
                .and_then(|line| fix_for(diagnostic, line))
                .is_some_and(|fix| fix.safety == Safety::Unsafe);
            if has_unsafe {
                leftover.needs_unsafe += 1;
            }
        }
        leftover
    }
}

/// The text uf is willing to write, or why there is none.
///
/// The two guarantees the module header makes, in the order they matter. The
/// candidate has to parse, which `uf_fmt::format_source` answers by failing
/// when it does not — it prints from a syntax tree and there is no tree to
/// print. And it has to be text `uf fmt` is content with when the file it
/// replaces was, which the same call answers by handing back the printed form.
fn settle(
    candidate: String,
    was_formatted: bool,
    config: &uf_config::FmtConfig,
) -> Result<String, &'static str> {
    let formatted = uf_fmt::format_source(&candidate, config)
        .map_err(|_| "the fixed file did not parse, so it was not written")?;
    Ok(if was_formatted {
        formatted.output
    } else {
        candidate
    })
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use camino::Utf8PathBuf;
    use uf_lint::{Diagnostic, Severity};
    use uf_project::SourceKind;

    use super::*;

    /// A file with one safe fix in it, already formatted.
    const HAS_A_SAFE_FIX: &str = "// @flow\n\nexport type Flag = bool;\n";

    /// The same file with that fix applied.
    const FIXED: &str = "// @flow\n\nexport type Flag = boolean;\n";

    /// A project holding `HAS_A_SAFE_FIX` as `app.js`, and the file to fix.
    fn a_file_to_fix(root: &Utf8Path) -> ProjectFile {
        std::fs::write(root.join("app.js"), HAS_A_SAFE_FIX).expect("the file is written");
        ProjectFile {
            absolute_path: root.join("app.js"),
            relative_path: String::from("app.js"),
            source: String::from(HAS_A_SAFE_FIX),
            kind: SourceKind::JavaScript,
        }
    }

    /// The one finding `HAS_A_SAFE_FIX` has, where it has it.
    fn deprecated_bool(path: &str) -> LintReport {
        LintReport {
            diagnostics: vec![Diagnostic {
                rule: "flow/deprecated-type",
                severity: Severity::Error,
                path: Some(path.to_owned()),
                line: 3,
                column: 20,
                message: String::new(),
            }],
            files_checked: 1,
            unavailable: Vec::new(),
        }
    }

    /// A `ResolvedConfig` for a project at `root` with nothing configured.
    fn config_at(root: &Utf8Path) -> ResolvedConfig {
        ResolvedConfig {
            root: root.to_owned(),
            config_path: None,
            config: uf_config::UniflowedConfig::default(),
        }
    }

    /// A linter that stops working after the first round writes nothing.
    ///
    /// The round that ran produced a fix; nothing has linted it since, so it
    /// has not reached the fixpoint the module header promises and it is not
    /// uf's to write. It used to be written, and the failure was not reported
    /// at all — the run went on to say it had changed the file. See the review
    /// on #455.
    #[test]
    fn a_linter_that_stops_working_mid_loop_writes_nothing_and_says_so() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
        let resolved = config_at(&root);
        let mut file = a_file_to_fix(&root);
        let rounds = Cell::new(0usize);
        let lint = |source: &SourceFile| {
            if rounds.replace(rounds.get() + 1) > 0 {
                // What the parser thread panicking looks like from here.
                return Err(LintError::Flow(uf_flow::FlowError::Runtime(String::from(
                    "the Flow parser thread panicked",
                ))));
            }
            Ok(deprecated_bool(&source.path))
        };
        let mut summary = FixSummary::default();

        let outcome = fix_file_with(&lint, &resolved, &mut file, FixMode::Safe, &mut summary);

        let error = outcome.expect_err("a linter that could not run passed for a clean file");
        assert!(
            error.to_string().contains("app.js"),
            "the failure does not name the file: {error}"
        );
        assert_eq!(rounds.get(), 2, "the second round never ran");
        assert_eq!(
            std::fs::read_to_string(root.join("app.js")).expect("the file is there"),
            HAS_A_SAFE_FIX,
            "a fix nothing had linted since was written to disk"
        );
        assert!(summary.changed.is_empty(), "{:?}", summary.changed);
        assert_eq!(file.source, HAS_A_SAFE_FIX);
    }

    /// The control: the same file and the same first round, linted to the end.
    ///
    /// Without this the test above would pass on a loop that never fixed
    /// anything, which is the wrong reason to leave a file alone.
    #[test]
    fn a_linter_that_keeps_working_writes_the_fix_it_confirmed() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
        let resolved = config_at(&root);
        let mut file = a_file_to_fix(&root);
        let rounds = Cell::new(0usize);
        let lint = |source: &SourceFile| {
            if rounds.replace(rounds.get() + 1) > 0 {
                return Ok(LintReport::default());
            }
            Ok(deprecated_bool(&source.path))
        };
        let mut summary = FixSummary::default();

        fix_file_with(&lint, &resolved, &mut file, FixMode::Safe, &mut summary)
            .expect("the linter kept working");

        assert_eq!(
            std::fs::read_to_string(root.join("app.js")).expect("the file is there"),
            FIXED
        );
        assert_eq!(summary.changed, vec![String::from("app.js")]);
        assert_eq!(file.source, FIXED);
    }

    #[test]
    fn only_the_writing_modes_write() {
        assert!(!FixMode::Report.writes());
        assert!(FixMode::Safe.writes());
        assert!(FixMode::Unsafe.writes());
    }

    #[test]
    fn only_the_unsafe_mode_admits_unsafe_fixes() {
        assert!(!FixMode::Report.allows_unsafe());
        assert!(!FixMode::Safe.allows_unsafe());
        assert!(FixMode::Unsafe.allows_unsafe());
    }

    #[test]
    fn text_that_does_not_parse_is_refused() {
        let config = uf_config::FmtConfig::default();
        assert!(settle(String::from("const x = (;"), false, &config).is_err());
    }

    /// The edit is allowed to disturb the layout; what is written is not.
    #[test]
    fn a_file_the_formatter_was_content_with_comes_back_formatted() {
        let config = uf_config::FmtConfig::default();
        let disturbed = String::from("const   x = 1;\n");

        assert_eq!(
            settle(disturbed.clone(), true, &config).expect("it parses"),
            "const x = 1;\n",
            "a file `uf fmt --check` accepted must not come back one it rejects"
        );
        assert_eq!(
            settle(disturbed.clone(), false, &config).expect("it parses"),
            disturbed,
            "a file `uf fmt` already wanted to change is not this pass's to lay out"
        );
    }
}
