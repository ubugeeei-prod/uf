#![allow(clippy::disallowed_macros)]

//! uf's compile path, measured against the React Compiler's own fixtures.
//!
//! uf runs the official compiler — the `react_compiler` crate, published from
//! facebook/react's `compiler/crates/` — but the crate has no front end. It
//! reads Babel's AST and a `ScopeInfo` that whoever parsed the file built;
//! upstream those come from `@babel/parser` and `@babel/traverse`, and in uf
//! from the Flow parser, `babel.rs` and `scope.rs`. Wherever those differ from
//! Babel, the official compiler behaves differently inside uf than inside
//! `babel-plugin-react-compiler`. This test measures exactly that difference.
//!
//! # What runs
//!
//! Every fixture in `babel-plugin-react-compiler/src/__tests__/fixtures/compiler`
//! at the commit `tools/react-compiler/pin.txt` names — the commit the crate
//! was published from — goes through [`pipeline::compile`] with the options
//! its first line asks for ([`pragma`]), and the result is compared with the
//! `.expect.md` snapshot the TypeScript plugin wrote ([`expect`]), in the form
//! [`normalize`] describes. No Babel runs anywhere in this.
//!
//! `tools/react-compiler/sync.sh` checks the fixtures out; `uf run
//! react-compiler:conformance` syncs and runs this with the report printed.
//!
//! # The baseline
//!
//! `baseline.tsv` records, for every fixture, what happened (`class`) and whose
//! it is (`bucket`):
//!
//! | bucket       | meaning                                                          |
//! |--------------|------------------------------------------------------------------|
//! | `pass`       | uf's output is the snapshot's                                    |
//! | `syntax`     | TypeScript the Flow parser rejects, or reads other than Babel    |
//! | `conversion` | the lowering rules or `babel.rs` give the compiler another tree  |
//! | `scope`      | `scope.rs`, or what uf does with the crate's renames, differs    |
//! | `printer`    | `print.rs` prints something other than the compiled tree         |
//! | `compiler`   | the crate itself disagrees with the snapshot                     |
//! | `snapshot`   | the snapshot also holds `babel-plugin-fbt`/`babel-plugin-idx` output |
//! | `harness`    | this test could not build the options or read the snapshot       |
//!
//! The test fails when any fixture's class differs from the baseline, so the
//! pass count can only go down in a diff that says so, and a fix has to add
//! the fixtures it fixed. `UF_REACT_COMPILER_CONFORMANCE=update` rewrites the
//! file, keeping the bucket and note of every fixture whose class did not
//! change; a failure whose owner is not known yet is `untriaged`, which the
//! test refuses.

mod expect;
mod normalize;
mod pipeline;
mod pragma;

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Instant;

use pipeline::{Compiled, Refused};

const WORKER: &str = "react-compiler-fixture";

/// What happened to one fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Class {
    Pass,
    Unparsed,
    ConversionError,
    SchemaError,
    PrintError,
    OptionsError,
    Panic,
    NormalizerError,
    CodeMismatch,
    ErrorMismatch,
    LogsMismatch,
    ExpectedCodeGotError,
    ExpectedErrorGotCode,
    NoSnapshot,
}

impl Class {
    const ALL: [Self; 14] = [
        Self::Pass,
        Self::Unparsed,
        Self::ConversionError,
        Self::SchemaError,
        Self::PrintError,
        Self::OptionsError,
        Self::Panic,
        Self::NormalizerError,
        Self::CodeMismatch,
        Self::ErrorMismatch,
        Self::LogsMismatch,
        Self::ExpectedCodeGotError,
        Self::ExpectedErrorGotCode,
        Self::NoSnapshot,
    ];

    const fn id(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Unparsed => "unparsed",
            Self::ConversionError => "conversion-error",
            Self::SchemaError => "schema-error",
            Self::PrintError => "print-error",
            Self::OptionsError => "options-error",
            Self::Panic => "panic",
            Self::NormalizerError => "normalizer-error",
            Self::CodeMismatch => "code-mismatch",
            Self::ErrorMismatch => "error-mismatch",
            Self::LogsMismatch => "logs-mismatch",
            Self::ExpectedCodeGotError => "expected-code-got-error",
            Self::ExpectedErrorGotCode => "expected-error-got-code",
            Self::NoSnapshot => "no-snapshot",
        }
    }

    fn parse(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|class| class.id() == id)
    }

    /// The bucket a class settles on its own, without anyone reading the
    /// difference.
    const fn bucket(self) -> Option<&'static str> {
        match self {
            Self::Pass => Some("pass"),
            Self::Unparsed => Some("syntax"),
            Self::ConversionError | Self::SchemaError => Some("conversion"),
            Self::PrintError => Some("printer"),
            Self::OptionsError | Self::NormalizerError | Self::NoSnapshot => Some("harness"),
            _ => None,
        }
    }
}

const BUCKETS: [(&str, &str); 9] = [
    ("pass", "uf's output is the snapshot's"),
    (
        "syntax",
        "TypeScript that uf's Flow parser rejects, or reads differently from Babel's parser",
    ),
    (
        "conversion",
        "the lowering rules or babel.rs give the compiler another tree",
    ),
    (
        "scope",
        "scope.rs, or uf's use of the compiler's scope output, differs from Babel's",
    ),
    (
        "printer",
        "print.rs prints something other than the compiled tree",
    ),
    (
        "compiler",
        "the react_compiler crate itself disagrees with the snapshot",
    ),
    (
        "snapshot",
        "the snapshot records babel-plugin-fbt or babel-plugin-idx output as well as the compiler's",
    ),
    (
        "harness",
        "this test could not build the options or read the snapshot",
    ),
    ("untriaged", "not attributed yet"),
];

#[derive(Debug, Clone)]
struct Judged {
    class: Class,
    detail: String,
}

#[derive(Debug, Clone)]
struct Recorded {
    class: String,
    bucket: String,
    note: String,
}

struct Pin {
    version: String,
    commit: String,
    fixtures: String,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn pin() -> Pin {
    let text = fs::read_to_string(repo_root().join("tools/react-compiler/pin.txt"))
        .expect("tools/react-compiler/pin.txt");
    let value = |key: &str| {
        text.lines()
            .filter(|line| !line.starts_with('#'))
            .find_map(|line| {
                let mut fields = line.split_whitespace();
                (fields.next() == Some(key)).then(|| fields.next().unwrap_or_default().to_owned())
            })
            .unwrap_or_else(|| panic!("tools/react-compiler/pin.txt names no `{key}`"))
    };
    Pin {
        version: value("version"),
        commit: value("commit"),
        fixtures: value("fixtures"),
    }
}

fn baseline_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/react_compiler_conformance/baseline.tsv")
}

/// The fixtures are measured against the compiler that wrote their snapshots.
///
/// `Cargo.lock` must resolve `react_compiler` to the version the pin names, and
/// when the crate's registry source is on this machine — it is wherever the
/// crate was built — its `.cargo_vcs_info.json` must name the pinned commit.
#[test]
fn the_pin_is_the_commit_react_compiler_was_published_from() {
    let pin = pin();
    let lock = fs::read_to_string(repo_root().join("Cargo.lock")).expect("Cargo.lock");
    let locked = lock
        .split("[[package]]")
        .find(|package| package.contains("\nname = \"react_compiler\"\n"))
        .and_then(|package| {
            package
                .lines()
                .find_map(|line| line.strip_prefix("version = \""))
                .map(|version| version.trim_end_matches('"').to_owned())
        })
        .expect("react_compiler in Cargo.lock");
    assert_eq!(
        locked, pin.version,
        "Cargo.lock resolves react_compiler {locked}, but tools/react-compiler/pin.txt pins the \
         fixtures of {}; move the pin to the commit the new version was published from",
        pin.version
    );

    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")));
    let Some(registry) = cargo_home.map(|home| home.join("registry/src")) else {
        return;
    };
    let Ok(indexes) = fs::read_dir(&registry) else {
        return;
    };
    for index in indexes.flatten() {
        let info = index.path().join(format!(
            "react_compiler-{}/.cargo_vcs_info.json",
            pin.version
        ));
        if let Ok(text) = fs::read_to_string(&info) {
            assert!(
                text.contains(&pin.commit),
                "{} does not name the pinned commit {}:\n{text}",
                info.display(),
                pin.commit
            );
        }
    }
}

#[test]
fn uf_compiles_the_react_compiler_fixtures_as_babel_plugin_react_compiler_does() {
    let pin = pin();
    let corpus = repo_root()
        .join("tests/fixtures/react-compiler")
        .join(&pin.commit)
        .join(&pin.fixtures);
    if !corpus.is_dir() {
        assert!(
            std::env::var_os("UF_ALLOW_FIXTURE_SKIP").is_some(),
            "the React Compiler fixtures are not checked out at {}; run \
             tools/react-compiler/sync.sh (or set UF_ALLOW_FIXTURE_SKIP=1 to skip)",
            corpus.display()
        );
        eprintln!(
            "skipped: no React Compiler fixtures at {}",
            corpus.display()
        );
        return;
    }

    let mut fixtures = Vec::new();
    collect(&corpus, &mut fixtures);
    fixtures.sort();
    let only = std::env::var("UF_REACT_COMPILER_FIXTURE").ok();
    if let Some(only) = &only {
        fixtures.retain(|fixture| {
            let name = relative(&corpus, fixture);
            only.split(',').any(|wanted| name.contains(wanted.trim()))
        });
    }

    let started = Instant::now();
    let judged = judge_all(&corpus, &fixtures);
    let elapsed = started.elapsed();

    if only.is_some() {
        explain_all(&corpus, &fixtures, &judged);
        return;
    }

    let observed: BTreeMap<String, Judged> = fixtures
        .iter()
        .zip(judged)
        .map(|(fixture, judged)| (relative(&corpus, fixture), judged))
        .collect();
    let recorded = read_baseline();
    let update = std::env::var("UF_REACT_COMPILER_CONFORMANCE").as_deref() == Ok("update");
    let merged = merge(&observed, &recorded, &read_triage());

    let workers = worker_count();
    println!("{}", report(&pin, &observed, &merged, elapsed, workers));

    if update {
        write_baseline(&observed, &merged);
        println!("wrote {}", baseline_path().display());
        return;
    }

    assert!(
        !recorded.is_empty(),
        "{} is missing; run with UF_REACT_COMPILER_CONFORMANCE=update to write it",
        baseline_path().display()
    );
    let mut changed = Vec::new();
    for (fixture, judged) in &observed {
        match recorded.get(fixture) {
            Some(entry) if entry.class == judged.class.id() => {}
            Some(entry) => changed.push(format!(
                "{fixture}: {} -> {} ({})",
                entry.class,
                judged.class.id(),
                judged.detail
            )),
            None => changed.push(format!("{fixture}: not in the baseline")),
        }
    }
    for fixture in recorded.keys() {
        if !observed.contains_key(fixture) {
            changed.push(format!("{fixture}: in the baseline but not in the corpus"));
        }
    }
    let floor = recorded
        .values()
        .filter(|entry| entry.class == "pass")
        .count();
    let passed = observed
        .values()
        .filter(|judged| judged.class == Class::Pass)
        .count();
    let untriaged: Vec<&String> = merged
        .iter()
        .filter(|(_, entry)| entry.bucket == "untriaged")
        .map(|(fixture, _)| fixture)
        .collect();
    assert!(
        changed.is_empty(),
        "{} fixture(s) changed against {}; a fix records what it fixed and a regression is \
         not merged. Rerun with UF_REACT_COMPILER_CONFORMANCE=update to record:\n{}",
        changed.len(),
        baseline_path().display(),
        changed
            .iter()
            .take(60)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(
        passed >= floor,
        "{passed} fixtures pass, below the floor of {floor}"
    );
    assert!(
        untriaged.is_empty(),
        "{} failure(s) are not attributed to a bucket: {untriaged:?}",
        untriaged.len()
    );
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| matches!(extension, "js" | "jsx" | "ts" | "tsx"))
        {
            out.push(path);
        }
    }
}

fn relative(corpus: &Path, fixture: &Path) -> String {
    fixture
        .strip_prefix(corpus)
        .unwrap_or(fixture)
        .to_string_lossy()
        .replace('\\', "/")
}

fn worker_count() -> usize {
    std::thread::available_parallelism().map_or(4, std::num::NonZero::get)
}

/// Every fixture, on threads with the Flow parser's stack.
fn judge_all(corpus: &Path, fixtures: &[PathBuf]) -> Vec<Judged> {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(move |info| {
        if std::thread::current().name() != Some(WORKER) {
            previous(info);
        }
    }));

    let next = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<Judged>>> = Mutex::new(vec![None; fixtures.len()]);
    std::thread::scope(|scope| {
        for _ in 0..worker_count() {
            std::thread::Builder::new()
                .name(WORKER.to_owned())
                .stack_size(uf_flow::PARSE_STACK_BYTES)
                .spawn_scoped(scope, || {
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(fixture) = fixtures.get(index) else {
                            break;
                        };
                        let judged = judge(corpus, fixture);
                        results.lock().expect("results")[index] = Some(judged);
                    }
                })
                .expect("a worker thread");
        }
    });
    results
        .into_inner()
        .expect("results")
        .into_iter()
        .map(|judged| judged.expect("every fixture judged"))
        .collect()
}

fn judge(corpus: &Path, fixture: &Path) -> Judged {
    let judged = |class, detail: String| Judged { class, detail };
    let Ok(source) = fs::read_to_string(fixture) else {
        return judged(Class::NoSnapshot, "the input is not UTF-8".to_owned());
    };
    let snapshot_path = fixture.with_extension("expect.md");
    let Ok(snapshot) = fs::read_to_string(&snapshot_path) else {
        return judged(
            Class::NoSnapshot,
            format!("no {}", relative(corpus, &snapshot_path)),
        );
    };
    let expected = expect::parse(&snapshot);

    let first_line = pragma::first_line(&source);
    let language = pragma::language(first_line);
    let stem = fixture
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let filename = pragma::filename(stem, language);

    let compiled = match panic::catch_unwind(AssertUnwindSafe(|| {
        pipeline::compile(&source, first_line, &filename)
    })) {
        Ok(Ok(compiled)) => compiled,
        Ok(Err(refused)) => {
            return match refused {
                Refused::Unparsed(detail) => judged(Class::Unparsed, detail),
                Refused::Conversion(detail) => judged(Class::ConversionError, detail),
                Refused::Options(detail) => judged(Class::OptionsError, detail),
                Refused::Schema(detail) => judged(Class::SchemaError, detail),
                Refused::Print(detail) => judged(Class::PrintError, detail),
            };
        }
        Err(payload) => {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| {
                    payload
                        .downcast_ref::<&str>()
                        .map(|text| (*text).to_owned())
                })
                .unwrap_or_default();
            return judged(Class::Panic, message);
        }
    };
    let compiled = snap_checks(compiled, first_line);

    match (compiled, &expected.code, &expected.error) {
        (Compiled::Thrown { message, .. }, _, Some(expected_error)) => {
            let expected_error = normalize::error(expected_error);
            let message = normalize::error(&message);
            if message == expected_error {
                judged(Class::Pass, String::new())
            } else {
                judged(
                    Class::ErrorMismatch,
                    first_differing_line(&expected_error, &message),
                )
            }
        }
        (Compiled::Code { .. }, _, Some(expected_error)) => {
            judged(Class::ExpectedErrorGotCode, heading(expected_error))
        }
        (Compiled::Thrown { message, .. }, Some(_), None) => {
            judged(Class::ExpectedCodeGotError, heading(&message))
        }
        (Compiled::Code { code, events }, Some(expected_code), None) => {
            let expected_tree = match normalize::code(expected_code) {
                Ok(tree) => tree,
                Err(error) => {
                    return judged(Class::NormalizerError, format!("the snapshot: {error}"));
                }
            };
            let actual_tree = match normalize::code(&code) {
                Ok(tree) => tree,
                Err(error) => {
                    return judged(
                        Class::PrintError,
                        format!("uf's output does not parse: {error}"),
                    );
                }
            };
            if expected_tree != actual_tree {
                return judged(
                    Class::CodeMismatch,
                    normalize::first_difference(&expected_tree, &actual_tree),
                );
            }
            logs(first_line, expected.logs.as_deref(), &events)
        }
        (Compiled::Code { .. } | Compiled::Thrown { .. }, None, None) => judged(
            Class::NoSnapshot,
            "the snapshot has neither code nor an error".to_owned(),
        ),
    }
}

/// Both sides of each fixture `UF_REACT_COMPILER_FIXTURE` selected, in full,
/// for reading a failure rather than counting it.
fn explain_all(corpus: &Path, fixtures: &[PathBuf], judged: &[Judged]) {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(uf_flow::PARSE_STACK_BYTES)
            .spawn_scoped(scope, || {
                for (fixture, judged) in fixtures.iter().zip(judged) {
                    println!("{}", explain(corpus, fixture, judged));
                }
            })
            .expect("a thread for the Flow parser");
    });
}

fn explain(corpus: &Path, fixture: &Path, judged: &Judged) -> String {
    let mut out = format!(
        "== {} ({})\n{}\n",
        relative(corpus, fixture),
        judged.class.id(),
        judged.detail
    );
    let source = fs::read_to_string(fixture).unwrap_or_default();
    let snapshot = fs::read_to_string(fixture.with_extension("expect.md")).unwrap_or_default();
    let expected = expect::parse(&snapshot);
    let first_line = pragma::first_line(&source);
    let stem = fixture
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or_default();
    let filename = pragma::filename(stem, pragma::language(first_line));
    match panic::catch_unwind(AssertUnwindSafe(|| {
        pipeline::compile(&source, first_line, &filename)
    })) {
        Ok(Ok(Compiled::Code { code, events })) => {
            let _ = write!(out, "-- uf's code\n{code}\n-- uf's events\n");
            for event in &events {
                let _ = writeln!(out, "{event}");
            }
        }
        Ok(Ok(Compiled::Thrown { message })) => {
            let _ = write!(out, "-- uf's error\n{message}\n");
        }
        Ok(Err(_)) | Err(_) => out.push_str("-- uf produced nothing to show\n"),
    }
    if let Some(code) = &expected.code {
        let _ = write!(out, "-- expected code\n{code}\n");
    }
    if let Some(error) = &expected.error {
        let _ = write!(out, "-- expected error\n{error}\n");
    }
    if let Some(logs) = &expected.logs {
        let _ = write!(out, "-- expected logs\n{logs}\n");
    }
    out
}

/// `## Logs`, written for a `@loggerTestOnly` fixture that logged anything.
fn logs(first_line: &str, expected: Option<&str>, events: &[serde_json::Value]) -> Judged {
    let actual = if first_line.contains("@loggerTestOnly") && !events.is_empty() {
        Some(normalize::events(events))
    } else {
        None
    };
    let expected = match expected.map(normalize::logs).transpose() {
        Ok(expected) => expected,
        Err(error) => {
            return Judged {
                class: Class::NormalizerError,
                detail: format!("the snapshot's logs: {error}"),
            };
        }
    };
    if expected == actual {
        return Judged {
            class: Class::Pass,
            detail: String::new(),
        };
    }
    let detail = match (&expected, &actual) {
        (Some(expected), Some(actual)) => normalize::first_difference(
            &serde_json::Value::Array(expected.clone()),
            &serde_json::Value::Array(actual.clone()),
        ),
        (Some(_), None) => "the snapshot logs events and uf logged none".to_owned(),
        (None, _) => "uf logged events the snapshot does not have".to_owned(),
    };
    Judged {
        class: Class::LogsMismatch,
        detail,
    }
}

/// `transformFixtureInput`'s checks after a successful transform, which turn
/// some successes into the error the snapshot records.
fn snap_checks(compiled: Compiled, first_line: &str) -> Compiled {
    let Compiled::Code { code, events } = compiled else {
        return compiled;
    };
    let expect_nothing = first_line.contains("@expectNothingCompiled");
    let outcomes = events
        .iter()
        .filter(|event| {
            matches!(
                event["kind"].as_str(),
                Some("CompileSuccess" | "CompileError")
            )
        })
        .count();
    let message = if outcomes == 0 && !expect_nothing {
        Some(
            "No success/failure events, add `// @expectNothingCompiled` to the first line if \
             this is expected"
                .to_owned(),
        )
    } else if outcomes != 0 && expect_nothing {
        Some(
            "Expected nothing to be compiled (from `// @expectNothingCompiled`), but some \
             functions compiled or errored"
                .to_owned(),
        )
    } else {
        let throws: Vec<&str> = events
            .iter()
            .filter(|event| event["kind"] == "CompileUnexpectedThrow")
            .map(|event| event["data"].as_str().unwrap_or_default())
            .collect();
        (!throws.is_empty()).then(|| {
            format!(
                "Compiler pass(es) threw instead of recording errors:\n{}",
                throws.join("\n")
            )
        })
    };
    match message {
        Some(message) => Compiled::Thrown { message },
        None => Compiled::Code { code, events },
    }
}

/// The line of an error that says what it is, past `Found N errors:`.
fn heading(message: &str) -> String {
    clip(
        message
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with("Found "))
            .unwrap_or_default(),
    )
}

fn first_differing_line(expected: &str, actual: &str) -> String {
    let mut expected_lines = expected.lines();
    let mut actual_lines = actual.lines();
    let mut number = 1;
    loop {
        match (expected_lines.next(), actual_lines.next()) {
            (Some(left), Some(right)) if left == right => number += 1,
            (None, None) => return "the messages differ only in blank lines".to_owned(),
            (left, right) => {
                return format!(
                    "line {number}: expected {:?}, uf has {:?}",
                    left.map(clip).unwrap_or_default(),
                    right.map(clip).unwrap_or_default()
                );
            }
        }
    }
}

fn clip(text: &str) -> String {
    text.chars().take(160).collect()
}

fn read_baseline() -> BTreeMap<String, Recorded> {
    let Ok(text) = fs::read_to_string(baseline_path()) else {
        return BTreeMap::new();
    };
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.splitn(4, '\t');
            let fixture = fields.next().unwrap_or_default().to_owned();
            let class = fields.next().unwrap_or_default().to_owned();
            let bucket = fields.next().unwrap_or_default().to_owned();
            let note = fields.next().unwrap_or_default().to_owned();
            assert!(
                Class::parse(&class).is_some(),
                "baseline.tsv: `{class}` is not a class ({fixture})"
            );
            assert!(
                BUCKETS.iter().any(|(name, _)| *name == bucket),
                "baseline.tsv: `{bucket}` is not a bucket ({fixture})"
            );
            (
                fixture,
                Recorded {
                    class,
                    bucket,
                    note,
                },
            )
        })
        .collect()
}

/// Attributions to apply while updating, from the file
/// `UF_REACT_COMPILER_TRIAGE` names: `fixture | bucket | note` per line, the
/// last line for a fixture winning. Only failures take one; a fixture that
/// passes is `pass` whatever the file says.
fn read_triage() -> BTreeMap<String, (String, String)> {
    let Ok(path) = std::env::var("UF_REACT_COMPILER_TRIAGE") else {
        return BTreeMap::new();
    };
    let text = fs::read_to_string(&path).unwrap_or_else(|error| panic!("{path}: {error}"));
    text.lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .map(|line| {
            let mut fields = line.splitn(3, " | ");
            let fixture = fields.next().unwrap_or_default().to_owned();
            let bucket = fields.next().unwrap_or_default().to_owned();
            let note = fields.next().unwrap_or_default().to_owned();
            assert!(
                BUCKETS
                    .iter()
                    .any(|(name, _)| *name == bucket && bucket != "pass"),
                "{path}: `{bucket}` is not a failure bucket ({fixture})"
            );
            (fixture, (bucket, note))
        })
        .collect()
}

/// The baseline as it would be written now: observed classes, with the bucket
/// and note of every fixture whose class did not change.
fn merge(
    observed: &BTreeMap<String, Judged>,
    recorded: &BTreeMap<String, Recorded>,
    triage: &BTreeMap<String, (String, String)>,
) -> BTreeMap<String, Recorded> {
    observed
        .iter()
        .map(|(fixture, judged)| {
            // What somebody attributed stays attributed while the failure is
            // the same failure. Everything else is this run's: the bucket a
            // class settles on its own, or `untriaged`, with the run's detail.
            let kept = recorded
                .get(fixture)
                .filter(|entry| entry.class == judged.class.id() && entry.bucket != "untriaged");
            let mut recorded = match kept {
                Some(entry) => entry.clone(),
                None => Recorded {
                    class: judged.class.id().to_owned(),
                    bucket: judged.class.bucket().unwrap_or("untriaged").to_owned(),
                    note: String::new(),
                },
            };
            if judged.class != Class::Pass
                && let Some((bucket, note)) = triage.get(fixture)
            {
                recorded.bucket.clone_from(bucket);
                recorded.note.clone_from(note);
            }
            (fixture.clone(), recorded)
        })
        .collect()
}

fn write_baseline(observed: &BTreeMap<String, Judged>, merged: &BTreeMap<String, Recorded>) {
    let mut text = String::from(
        "# The React Compiler conformance baseline: one line per fixture, as\n\
         # fixture<TAB>class<TAB>bucket<TAB>note. See main.rs beside this file.\n\
         # Written by UF_REACT_COMPILER_CONFORMANCE=update; buckets and notes of\n\
         # failures are kept across updates and edited by hand when attributed.\n",
    );
    for (fixture, entry) in merged {
        let note = if entry.note.is_empty() && entry.class != "pass" {
            observed
                .get(fixture)
                .map(|judged| judged.detail.replace(['\t', '\n'], " "))
                .unwrap_or_default()
        } else {
            entry.note.clone()
        };
        let _ = writeln!(text, "{fixture}\t{}\t{}\t{note}", entry.class, entry.bucket);
    }
    fs::write(baseline_path(), text).expect("baseline.tsv");
}

fn report(
    pin: &Pin,
    observed: &BTreeMap<String, Judged>,
    merged: &BTreeMap<String, Recorded>,
    elapsed: std::time::Duration,
    workers: usize,
) -> String {
    let total = observed.len();
    let passed = merged
        .values()
        .filter(|entry| entry.bucket == "pass")
        .count();
    let mut out = String::new();
    let _ = writeln!(
        out,
        "React Compiler conformance: uf's compile path on facebook/react@{} (react_compiler {})",
        &pin.commit[..12],
        pin.version
    );
    #[expect(
        clippy::cast_precision_loss,
        reason = "a percentage of under two thousand"
    )]
    let percent = if total == 0 {
        0.0
    } else {
        passed as f64 * 100.0 / total as f64
    };
    let _ = writeln!(out, "  {total} fixtures, {passed} pass ({percent:.1}%)");
    let _ = writeln!(out, "  by bucket:");
    for (bucket, meaning) in BUCKETS {
        let members: Vec<&String> = merged
            .iter()
            .filter(|(_, entry)| entry.bucket == bucket)
            .map(|(fixture, _)| fixture)
            .collect();
        if members.is_empty() {
            continue;
        }
        let examples = if bucket == "pass" {
            String::new()
        } else {
            format!(
                "  e.g. {}",
                members
                    .iter()
                    .take(3)
                    .map(|fixture| fixture.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        };
        let _ = writeln!(
            out,
            "    {bucket:<11}{:>5}  {meaning}{examples}",
            members.len()
        );
    }
    let _ = writeln!(out, "  by class:");
    for class in Class::ALL {
        let count = observed
            .values()
            .filter(|judged| judged.class == class)
            .count();
        if count > 0 {
            let _ = writeln!(out, "    {:<24}{count:>5}", class.id());
        }
    }
    let _ = write!(out, "  {:.1} s on {workers} threads", elapsed.as_secs_f64());
    out
}
