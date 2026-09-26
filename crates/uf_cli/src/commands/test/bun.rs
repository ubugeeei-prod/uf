//! `bun test` behind `uf test`, when `test.runner` names Bun.
//!
//! uf's own runner is a default, not the only implementation (red line 3). A
//! project that writes `test: { runner: "bun" }` has its suite run by
//! `bun test`, on the Bun that spec names, and `uf test` is the same command
//! it always was: it discovers the files, loads the project's `.env`, starts
//! the runner, and says whether the suite passed.
//!
//! # How a Flow suite reaches `bun test`
//!
//! Three things have to be true of the Bun process, and each is one argument:
//!
//! * **It can read Flow.** `--preload @uniflowed/host/bun-preload` transforms
//!   every module uf is responsible for through `uf transform`, exactly as a
//!   Bun host running uf's own runner does.
//! * **`@uniflowed/test` means `bun:test`.** `--conditions=uniflowed-bun-test`
//!   makes the package's `exports` answer with `bun/index.js`, which maps the
//!   API onto `bun:test` and refuses by name what Bun has no equivalent for. A
//!   preload plugin cannot do this: Bun does not ask a plugin about a bare
//!   specifier that resolves to an installed package, and the test file then
//!   registers its cases with uf's registry, where `bun test` cannot see them,
//!   and exits 0 over a suite that ran nothing. See ubugeeei-prod/uf#942.
//! * **It runs the files uf would.** The files are uf's discovery, passed as
//!   paths, rather than Bun's own filename patterns — so the two runners run
//!   the same files.
//!
//! # Why the JUnit report is always asked for
//!
//! `bun test`'s console output is for a person, and its exit status cannot
//! tell a suite that passed from a suite that ran nothing: a file whose
//! registrations went somewhere Bun could not see reports "0 tests" and exits
//! 0. So the run always writes a JUnit report — to the `--reporter-outfile`
//! the caller named, or into `.uf` — and `uf test` reads it back. A file uf's
//! discovery says declares tests, for which Bun reports no case at all, fails
//! the run by name: the same rule `FileStatus::RegisteredNothing` enforces for
//! uf's own runner, for the same reason.
//!
//! # Flags
//!
//! A flag with the same meaning to `bun test` is passed through; a flag
//! without one is refused by name, before Bun starts. A flag silently dropped
//! is a run that is not the run a person asked for, and a flag translated into
//! something with a different meaning is worse.

use std::io::Read;
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use uf_config::env_files::ProjectEnv;
use uf_config::{CoverageThresholdConfig, ResolvedConfig};
use uf_project::ProjectFile;

use super::TestArgs;
use crate::cli::{CoverageReporterArg, ResultReporterArg};
use crate::commands::builder::uniflowed_package;
use crate::commands::runtimes;
use crate::support::plural;
use crate::ui::Ui;

pub(crate) mod junit;

/// The export condition under which `@uniflowed/test` is `bun:test`.
pub(crate) const CONDITION: &str = "uniflowed-bun-test";

/// Where the JUnit report goes when the caller did not name a file.
const REPORT: &str = ".uf/bun-test/junit.xml";

/// What a module has to say to hold an in-source test.
const IN_SOURCE_MARKER: &str = "import.meta.uf.test";

/// A `uf test` flag `bun test` has no meaning for, and what to do instead.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RefusedFlag {
    /// The flag as a person typed it.
    pub(crate) flag: &'static str,
    /// Why it cannot be passed on, and what to use.
    pub(crate) reason: &'static str,
}

/// Every flag in `args` that `bun test` cannot honour.
pub(crate) fn refused_flags(args: &TestArgs) -> Vec<RefusedFlag> {
    let mut refused = Vec::new();
    let mut refuse = |present: bool, flag: &'static str, reason: &'static str| {
        if present {
            refused.push(RefusedFlag { flag, reason });
        }
    };
    refuse(
        args.json,
        "--json",
        "`bun test` writes no JSON report; use `--reporter junit --reporter-outfile <file>`",
    );
    refuse(
        args.list,
        "--list",
        "`bun test` cannot list what it would run without running it",
    );
    refuse(
        args.browser,
        "--browser",
        "browser mode is uf's own runner; `bun test` runs on Bun",
    );
    refuse(
        args.watch_interval.is_some(),
        "--watch-interval",
        "`bun test --watch` follows file events and takes no interval",
    );
    refuse(
        args.update_snapshots,
        "--update-snapshots",
        "uf and Bun key and serialise snapshots differently, and the runner refuses snapshot \
         matchers rather than rewrite snapshots uf did not write; update them with \
         `runner: \"uf\"`",
    );
    refuse(
        args.coverage_reporters
            .iter()
            .any(|reporter| matches!(reporter, CoverageReporterArg::Cobertura)),
        "--coverage-reporter cobertura",
        "`bun test` writes `text` and `lcov` coverage only",
    );
    refuse(
        args.shard.is_some(),
        "--shard",
        "a shard is cut from uf's own schedule and records uf's own report, and `bun test` has \
         neither; split the suite with `runner: \"uf\"`",
    );
    refuse(
        args.bench,
        "--bench",
        "`bun:test` has no benchmarks, and `bench()` refuses under Bun; run them with \
         `runner: \"uf\"`",
    );
    refused
}

/// The test-bearing files whose tests are in-source blocks.
///
/// `import.meta.uf.test` is compiled to uf's test API only for uf's own runner;
/// for any other it is `void 0`, and a block behind it registers nothing. So a
/// file relying on one is refused by name before Bun starts, rather than being
/// reported afterwards as a file that ran nothing.
pub(crate) fn in_source_files(files: &[ProjectFile]) -> Vec<&str> {
    files
        .iter()
        .filter(|file| file.source.contains(IN_SOURCE_MARKER))
        .map(|file| file.relative_path.as_str())
        .collect()
}

/// The arguments `bun` is started with, after the program itself.
pub(crate) fn arguments(
    preload: &Utf8Path,
    report: &Utf8Path,
    args: &TestArgs,
    files: &[&str],
) -> Vec<String> {
    let mut out = vec![
        uf_infra::into_string(uf_infra::cstr!("--conditions={CONDITION}")),
        String::from("test"),
        String::from("--preload"),
        preload.to_string(),
    ];
    if let Some(pattern) = args.filter.as_deref() {
        // uf's `-t` is a substring of the full name; Bun's is a regular
        // expression. Escaping makes every character literal, so the pattern
        // means what it meant to uf.
        out.push(uf_infra::into_string(uf_infra::cstr!(
            "--test-name-pattern={}",
            escape_pattern(pattern)
        )));
    }
    if args.watch {
        out.push(String::from("--watch"));
    }
    if args.coverage {
        out.push(String::from("--coverage"));
    }
    for reporter in &args.coverage_reporters {
        match reporter {
            CoverageReporterArg::Text => out.push(String::from("--coverage-reporter=text")),
            CoverageReporterArg::Lcov => out.push(String::from("--coverage-reporter=lcov")),
            CoverageReporterArg::Cobertura => {}
        }
    }
    if let Some(directory) = args.coverage_dir.as_deref() {
        out.push(uf_infra::into_string(uf_infra::cstr!(
            "--coverage-dir={directory}"
        )));
    }
    if let Some(failures) = args.bail {
        out.push(uf_infra::into_string(uf_infra::cstr!("--bail={failures}")));
    }
    if args.retry > 0 {
        out.push(uf_infra::into_string(uf_infra::cstr!(
            "--retry={}",
            args.retry
        )));
    }
    if let Some(threads) = args.threads {
        out.push(uf_infra::into_string(uf_infra::cstr!(
            "--parallel={threads}"
        )));
    }
    out.push(String::from("--reporter=junit"));
    out.push(uf_infra::into_string(uf_infra::cstr!(
        "--reporter-outfile={report}"
    )));
    // As paths rather than filters: `./` is what makes Bun run a file by name
    // whatever its filename patterns say.
    out.extend(
        files
            .iter()
            .map(|file| uf_infra::into_string(uf_infra::cstr!("./{file}"))),
    );
    out
}

/// Where the JUnit report is written: the caller's file, or uf's own.
pub(crate) fn report_path(root: &Utf8Path, args: &TestArgs) -> Utf8PathBuf {
    match (args.reporter.as_ref(), args.reporter_outfile.as_deref()) {
        (Some(ResultReporterArg::Junit), Some(outfile)) => {
            let outfile = Utf8Path::new(outfile);
            if outfile.is_absolute() {
                outfile.to_path_buf()
            } else {
                root.join(outfile)
            }
        }
        _ => root.join(REPORT),
    }
}

/// `pattern` with every regular-expression metacharacter escaped.
pub(crate) fn escape_pattern(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    for character in pattern.chars() {
        if matches!(
            character,
            '\\' | '^'
                | '$'
                | '.'
                | '|'
                | '?'
                | '*'
                | '+'
                | '('
                | ')'
                | '['
                | ']'
                | '{'
                | '}'
                | '/'
        ) {
            out.push('\\');
        }
        out.push(character);
    }
    out
}

/// Run the suite with `bun test`, and say whether it passed.
///
/// `files` are the test-bearing files uf's discovery found, already narrowed by
/// any path arguments — the files uf's own runner would have run.
///
/// # Which Bun
///
/// The one [`runtimes::resolve`] settles for `uf test`, the same resolution uf's
/// own runner starts its workers from: `test.runtime`, then the runtime the
/// runner brings, then `runtime`. So `runner: "bun"` is the `bun` on `PATH`,
/// and `runner: "bun@1.4"` is locked in `uf.lock` and installed into the store
/// the first time a run needs it, exactly as `test: { runtime: "bun@1.4" }` is.
/// It is resolved after every refusal below, so a run that was never going to
/// start downloads nothing.
pub(crate) fn run(
    ui: &mut Ui,
    resolved: &ResolvedConfig,
    env: ProjectEnv,
    files: &[ProjectFile],
    args: &TestArgs,
) -> Result<()> {
    let root = &resolved.root;
    let config = &resolved.config;

    // Each refusal below is a promise uf keeps for its own runner and cannot
    // keep through Bun. Running anyway would be a green run over a check that
    // did not happen.
    if config.permissions.is_some() {
        bail!(uf_infra::cstr!(
            "`test.runner` is `bun`, and this project declares `permissions`. Bun has no \
             permission model, so the set would not be enforced; a set that is written down and \
             silently ignored is worse than none. Run the suite with `runner: \"uf\"` on Node or \
             Deno, which enforce it."
        ));
    }
    let coverage = &config.test.coverage;
    if declares_thresholds(&coverage.thresholds)
        || declares_thresholds(&coverage.per_file_thresholds)
    {
        bail!(uf_infra::cstr!(
            "`test.runner` is `bun`, and `test.coverage` declares thresholds. uf checks those \
             against the coverage its own runner maps back to your Flow source; `bun test` \
             measures in its own terms and nothing would check the numbers. Remove the \
             thresholds, or run with `runner: \"uf\"`."
        ));
    }
    let in_source = in_source_files(files);
    if !in_source.is_empty() {
        bail!(uf_infra::cstr!(
            "`test.runner` is `bun`, and {} in-source tests (`import.meta.uf.test`), which only \
             uf's own runner runs: {}. Move them into a `.test.js` file, or run with \
             `runner: \"uf\"`.",
            plural(in_source.len(), "file holds"),
            in_source.join(", ")
        ));
    }

    let declared: Vec<&str> = files
        .iter()
        .map(|file| file.relative_path.as_str())
        .collect();
    if declared.is_empty() {
        // What uf's own runner answers for a project with no tests yet.
        ui.plain("no test files\n");
        return Ok(());
    }

    // Always a Bun: loading the config already refused a `test.runtime` that
    // names anything else beside a Bun runner.
    let runtime = runtimes::resolve(resolved, runtimes::Role::Test, &mut |message| {
        ui.render_err(|renderer, out| renderer.status(out, uf_term::Status::Info, message));
    })?;
    let env = runtime.environment(env);
    let program = runtime.host.program;
    let preload = uniflowed_package(root, "host", "bun-preload.js")?.join("bun-preload.js");
    let report = report_path(root, args);
    if let Some(parent) = report.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| uf_infra::cstr!("could not create {parent}"))?;
    }
    // A report an earlier run left must not be read back as this run's.
    let _ = std::fs::remove_file(&report);

    let status = Command::new(program.as_std_path())
        .args(arguments(&preload, &report, args, &declared))
        .current_dir(root.as_std_path())
        .envs(env.exported())
        .env("UF_PROJECT_ROOT", root.as_str())
        // The preload transforms through the binary that started it, never a
        // different `uf` on PATH — the same promise uf's own runner makes.
        .env("UF_BINARY", super::uf_binary()?.as_str())
        .status()
        .with_context(|| uf_infra::cstr!("could not start `{program}`"))?;

    if args.watch {
        // `bun test --watch` reports as it goes and ends when the person ends
        // it, so there is no single report to read back.
        return if status.success() {
            Ok(())
        } else {
            bail!(uf_infra::cstr!("`bun test --watch` exited ({status})"))
        };
    }

    let file = std::fs::File::open(&report).with_context(|| {
        uf_infra::into_string(uf_infra::cstr!(
            "`bun test` exited ({status}) and wrote no report to {report}, so uf cannot say which \
             cases ran"
        ))
    })?;
    let document = read_report(file, junit::MAX_JUNIT_BYTES).with_context(|| {
        uf_infra::into_string(uf_infra::cstr!(
            "could not read the report `bun test` wrote to {report}"
        ))
    })?;
    let cases =
        junit::read_cases(&document).map_err(|error| anyhow!(uf_infra::cstr!("{error}")))?;
    let count = |outcome| cases.iter().filter(|case| case.outcome == outcome).count();
    let (passed, failed, skipped) = (
        count(junit::BunOutcome::Passed),
        count(junit::BunOutcome::Failed),
        count(junit::BunOutcome::Skipped),
    );
    ui.plain(&uf_infra::into_string(uf_infra::cstr!(
        "\nrunner  bun test ({program}) · {passed} passed, {failed} failed, {skipped} skipped\n"
    )));

    // Before the exit status: Bun exits 0 when a file's registrations went
    // somewhere it could not see, and that is the run most in need of failing.
    let silent = silent_files(&declared, &cases);
    if !silent.is_empty() {
        bail!(uf_infra::cstr!(
            "`bun test` ran no case from {}, though uf's discovery found tests there: {}. Their \
             registrations went somewhere `bun test` could not see — usually a test API imported \
             from somewhere other than `@uniflowed/test` or `bun:test`.",
            plural(silent.len(), "file"),
            silent.join(", ")
        ));
    }
    if failed > 0 {
        bail!(uf_infra::cstr!(
            "bun test failed with {}",
            plural(failed, "failure")
        ));
    }
    if !status.success() {
        bail!(uf_infra::cstr!(
            "`bun test` exited ({status}) with no failing case in its report"
        ));
    }
    Ok(())
}

/// Whether any threshold in `thresholds` is set.
fn declares_thresholds(thresholds: &CoverageThresholdConfig) -> bool {
    thresholds.lines.is_some() || thresholds.functions.is_some() || thresholds.branches.is_some()
}

/// Everything in `file`, the report `bun test` wrote, when it holds no more
/// than `limit` bytes.
///
/// [`junit::read_cases`] refuses a document past [`junit::MAX_JUNIT_BYTES`],
/// but only once it holds the document, and reading the file whole to get it
/// would hold a report of any size in memory to learn that it is too large.
/// The report comes from a process uf does not control, so nothing past one
/// byte beyond the limit is read: that byte is what says the report is too
/// large, and the size the refusal names is the file's, from its metadata.
fn read_report(file: std::fs::File, limit: usize) -> Result<String> {
    let size = file.metadata()?.len();
    let mut bytes = Vec::new();
    file.take(u64::try_from(limit).map_or(u64::MAX, |limit| limit.saturating_add(1)))
        .read_to_end(&mut bytes)?;
    if bytes.len() > limit {
        let error = junit::JunitError::TooLarge(usize::try_from(size).unwrap_or(usize::MAX));
        bail!(uf_infra::cstr!("{error}"));
    }
    Ok(String::from_utf8(bytes)?)
}

/// The files uf's discovery says declare tests that Bun reported no case for.
pub(crate) fn silent_files<'a>(declared: &[&'a str], cases: &[junit::BunCase]) -> Vec<&'a str> {
    declared
        .iter()
        .copied()
        .filter(|file| {
            !cases
                .iter()
                .any(|case| case.file.trim_start_matches("./") == *file)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uf_project::SourceKind;

    fn file(path: &str, source: &str) -> ProjectFile {
        ProjectFile {
            relative_path: path.to_owned(),
            absolute_path: Utf8PathBuf::from(path),
            source: source.to_owned(),
            kind: SourceKind::JavaScript,
        }
    }

    #[test]
    fn a_substring_filter_stays_a_substring_under_a_regex() {
        assert_eq!(escape_pattern("a.b (c)"), "a\\.b \\(c\\)");
        assert_eq!(escape_pattern("math > adds"), "math > adds");
    }

    #[test]
    fn a_report_past_the_limit_is_refused_from_the_byte_past_it() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("junit.xml");
        // ASCII up to and including the byte past the limit, and not UTF-8 at
        // all after it. Reading the whole file first, as this used to, fails on
        // the encoding before the size is ever looked at; a read that stops at
        // that byte never meets the bytes that are not UTF-8.
        let mut bytes = vec![b' '; 17];
        bytes.extend([0xFF; 64]);
        std::fs::write(&path, &bytes).expect("write the report");
        let file = std::fs::File::open(&path).expect("open the report");

        let error = read_report(file, 16).expect_err("a report past the limit is refused");

        assert!(
            error
                .to_string()
                .contains(uf_infra::cstr!("is {} bytes, past the", bytes.len()).as_str()),
            "{error:#}"
        );
    }

    #[test]
    fn a_report_at_the_limit_is_read_whole() {
        let directory = tempfile::tempdir().expect("a temporary directory");
        let path = directory.path().join("junit.xml");
        let document = "<testsuites></testsuites>";
        std::fs::write(&path, document).expect("write the report");
        let file = std::fs::File::open(&path).expect("open the report");

        assert_eq!(
            read_report(file, document.len()).expect("a report at the limit is read"),
            document
        );
    }

    #[test]
    fn the_condition_preload_report_and_files_are_always_passed() {
        let args = TestArgs::default();
        let arguments = arguments(
            Utf8Path::new("/p/node_modules/@uniflowed/host/bun-preload.js"),
            Utf8Path::new("/p/.uf/bun-test/junit.xml"),
            &args,
            &["src/a.test.js"],
        );

        assert_eq!(
            arguments,
            vec![
                "--conditions=uniflowed-bun-test",
                "test",
                "--preload",
                "/p/node_modules/@uniflowed/host/bun-preload.js",
                "--reporter=junit",
                "--reporter-outfile=/p/.uf/bun-test/junit.xml",
                "./src/a.test.js",
            ]
        );
    }

    #[test]
    fn flags_with_a_bun_meaning_pass_through() {
        let args = TestArgs {
            filter: Some(String::from("adds")),
            watch: true,
            coverage: true,
            bail: Some(2),
            retry: 3,
            threads: Some(4),
            ..TestArgs::default()
        };
        let arguments = arguments(
            Utf8Path::new("/p/bun-preload.js"),
            Utf8Path::new("/p/r.xml"),
            &args,
            &[],
        );

        for expected in [
            "--test-name-pattern=adds",
            "--watch",
            "--coverage",
            "--bail=2",
            "--retry=3",
            "--parallel=4",
        ] {
            assert!(
                arguments.iter().any(|argument| argument == expected),
                "{expected}: {arguments:?}"
            );
        }
    }

    #[test]
    fn flags_without_one_are_refused_by_name() {
        let args = TestArgs {
            json: true,
            list: true,
            browser: true,
            watch_interval: Some(100),
            update_snapshots: true,
            ..TestArgs::default()
        };

        let refused: Vec<&str> = refused_flags(&args)
            .iter()
            .map(|refusal| refusal.flag)
            .collect();

        assert_eq!(
            refused,
            vec![
                "--json",
                "--list",
                "--browser",
                "--watch-interval",
                "--update-snapshots"
            ]
        );
        assert!(refused_flags(&TestArgs::default()).is_empty());
    }

    #[test]
    fn a_shard_is_refused_because_bun_test_has_no_schedule_to_cut() {
        let args = TestArgs {
            shard: Some(uf_test::Shard::new(1, 2).expect("a shard")),
            ..TestArgs::default()
        };

        let refused: Vec<&str> = refused_flags(&args)
            .iter()
            .map(|refusal| refusal.flag)
            .collect();

        assert_eq!(refused, vec!["--shard"]);
    }

    #[test]
    fn an_in_source_test_is_named_before_bun_starts() {
        let files = [
            file("src/a.test.js", "it('a', () => {});\n"),
            file(
                "src/slug.js",
                "if (import.meta.uf.test) { it('b', () => {}); }\n",
            ),
        ];

        assert_eq!(in_source_files(&files), vec!["src/slug.js"]);
    }

    #[test]
    fn a_file_bun_ran_nothing_from_is_named() {
        let cases = vec![junit::BunCase {
            file: String::from("src/a.test.js"),
            name: String::from("a > b"),
            outcome: junit::BunOutcome::Passed,
        }];

        assert_eq!(
            silent_files(&["src/a.test.js", "src/b.test.js"], &cases),
            vec!["src/b.test.js"]
        );
    }
}
