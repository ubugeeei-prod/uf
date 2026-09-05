//! The formatter's guarantees, over Flow that nobody here wrote.
//!
//! `guarantees.rs` checks the same three invariants against this
//! repository's own `@uniflowed/*` packages, the project templates, and a
//! hand-written corpus of about thirty snippets. All three are useful and
//! all three share a blind spot: they were written by people who knew what
//! the printer does.
//!
//! The repositories under `tests/fixtures/git` were not. Fifteen of them —
//! React, Metro, Relay, React Native, Recoil, Flux, Parcel, Yarn, Prepack,
//! StyleX, fbt, react-native-web, react-motion, DataLoader and redux-form —
//! come to about 8,100 Flow modules of production code, and they use the
//! parts of the grammar that a hand-written corpus reaches for last —
//! `(x as any).path` in a test helper, `{a, ...rest} = parse(url)` spread
//! over four lines, an object literal on the left of `as` in an arrow body,
//! `type` used as an ordinary identifier, `function f(): %checks`.
//!
//! `tools/corpus/repos.txt` is the list. Adding a line to it is the whole
//! edit: the fixtures here are read from the directory.
//!
//! # Why this skips rather than fails
//!
//! The fixtures are ~1 GB of other people's code and most work in this
//! repository does not need them. A test that fails on a fresh clone
//! teaches people to ignore failures. `uf run fmt:corpus` checks them out
//! and runs this; on a checkout without them it says so and passes.

mod support;

use std::fs;
use std::path::{Path, PathBuf};

use uf_config::FmtConfig;
use uf_fmt::format_source;

/// Every corpus repository that is checked out, in directory order.
///
/// Read from the filesystem rather than listed here, so that adding a line
/// to `tools/corpus/repos.txt` is the whole edit. A list in two places is a
/// list that disagrees with itself.
fn fixtures() -> Vec<String> {
    let Ok(entries) = fs::read_dir(corpus_root()) else {
        return Vec::new();
    };
    let mut found: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    found
}

/// Modules the printer cannot format in reasonable time.
///
/// Empty, and the goal is that it stays that way. It held
/// `react-native-compatibility-check`'s `VersionDiffing-test.js` until
/// ubugeeei-prod/uf#125 was fixed: `expect.objectContaining` nested
/// nineteen deep, printed twice per level, so the document was 2^depth.
///
/// Named rather than skipped by a heuristic, so that the day a bug is fixed
/// this list is what fails and gets deleted. `no_stale_exclusions` keeps it
/// from rotting the other way.
const KNOWN_SLOW: [&str; 0] = [];

/// Modules the printer gets wrong, each with the issue that says how.
///
/// Separate from {@link KNOWN_SLOW} because they are different problems and
/// a single list of excuses hides that. Four bugs, eleven modules:
///
/// * **#133** — parentheses around a same-precedence right operand are
///   dropped, so `a && (b && c)` becomes `a && b && c` and the tree
///   re-associates. Eight of these; three fail as a changed program and
///   five as non-idempotence, which is the same bug seen on the second
///   pass.
/// * **#134** — `function f(): %checks` loses its colon and the output does
///   not parse.
/// * **#126** — Flow's comment types are rewritten into real syntax, so a
///   script written to run under bare `node` stops doing so.
const KNOWN_BROKEN: [&str; 10] = [
    // #126
    "react-native/packages/react-native/scripts/spm/generate-spm-xcodeproj.js",
    // #133
    "fbt/runtime/nonfb/FbtNumber/IntlCLDRNumberType19.js",
    "fbt/runtime/nonfb/FbtNumber/IntlCLDRNumberType31.js",
    "fbt/runtime/nonfb/FbtNumber/IntlCLDRNumberType46.js",
    "prepack/src/react/elements.js",
    "prepack/src/serializer/ResidualFunctions.js",
    "prepack/src/serializer/ResidualHeapSerializer.js",
    "prepack/src/serializer/ResidualHeapVisitor.js",
    "yarn/src/package-request.js",
    // #134
    "fbt/packages/babel-plugin-fbt/src/FbtUtil.js",
];

/// The fixtures this run should look at.
///
/// `UF_CORPUS=metro,react` narrows it. Whole repositories rather than a file
/// count, because a failure is reported as a path and the first thing anyone
/// does with one is re-run that repository on its own.
fn wanted() -> Vec<String> {
    match std::env::var("UF_CORPUS") {
        Ok(list) => fixtures()
            .into_iter()
            .filter(|fixture| list.split(',').any(|want| want.trim() == fixture))
            .collect(),
        Err(_) => fixtures(),
    }
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/git")
}

/// Every Flow module in the checked-out fixtures.
///
/// A file counts as Flow when `@flow` appears in its first few hundred
/// bytes, which is where the pragma lives and where Flow itself looks.
/// Reading the whole of 40,000 files to find 5,800 is the difference
/// between this test taking seconds and taking minutes.
fn flow_modules() -> Vec<PathBuf> {
    let mut found = Vec::new();
    for fixture in wanted() {
        let root = corpus_root().join(&fixture);
        if root.is_dir() {
            collect(&root, &mut found);
        }
    }
    found.sort();
    found
}

fn collect(path: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(path) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if path.is_dir() {
            // `node_modules` is somebody else's dependency tree, and the
            // fixtures' own test fixtures are deliberately malformed.
            if matches!(name.as_ref(), "node_modules" | ".git" | "__fixtures__") {
                continue;
            }
            collect(&path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("js") {
            let Ok(source) = fs::read_to_string(&path) else {
                continue;
            };
            if source.get(..400).unwrap_or(&source).contains("@flow") {
                out.push(path);
            }
        }
    }
}

/// The three invariants, over every upstream module that parses.
///
/// A file the parser rejects is skipped rather than failed: these are other
/// people's repositories, they contain deliberately broken fixtures, and
/// whether Meta's parser accepts a given file is not this crate's business.
/// What *is* this crate's business is that anything it does format comes
/// back parseable, unchanged in meaning, and settled.
#[test]
fn upstream_flow_survives_formatting() {
    let modules = flow_modules();
    if modules.is_empty() {
        eprintln!(
            "upstream corpus not checked out — skipping.\n\
             `uf run fmt:corpus` fetches it."
        );
        return;
    }

    let config = FmtConfig::default();
    let trace = std::env::var_os("UF_CORPUS_TRACE").is_some();
    let mut formatted = 0usize;
    let mut skipped = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for (index, module) in modules.iter().enumerate() {
        // Progress, because 5,800 modules is minutes and a test that prints
        // nothing for minutes is a test people assume has hung. It also names
        // the file a crash died on, which no summary at the end can do.
        if index % 500 == 0 {
            eprintln!("  {index}/{} …", modules.len());
        }
        // `UF_CORPUS_TRACE=1` names every module before it is touched, so a
        // file that hangs or is killed is identified by the last line rather
        // than by bisecting a directory of two thousand.
        if trace {
            eprintln!("  -> {}", module.display());
        }
        let label = module
            .strip_prefix(corpus_root())
            .unwrap_or(module)
            .display()
            .to_string();
        if KNOWN_SLOW.contains(&label.as_str()) || KNOWN_BROKEN.contains(&label.as_str()) {
            skipped += 1;
            continue;
        }
        let Ok(source) = fs::read_to_string(module) else {
            continue;
        };

        let Ok(once) = format_source(&source, &config) else {
            skipped += 1;
            continue;
        };
        let once = once.output;

        // 1. The output parses. Anything else means `uf fmt` writes a file
        //    that no longer builds, which is the one failure a formatter
        //    must not have.
        let twice = match format_source(&once, &config) {
            Ok(twice) => twice.output,
            Err(error) => {
                failures.push(format!("{label}: output does not format again: {error}"));
                continue;
            }
        };

        // 2. It settles.
        if once != twice {
            failures.push(format!("{label}: not idempotent"));
            continue;
        }

        // 3. It means the same thing, and says the same things.
        let before = support::structure(&source);
        let after = support::structure(&once);
        if before != after {
            // The first place the trees part, rather than "the program
            // changed". A verdict with no evidence sends whoever reads it
            // back to reproduce the run by hand, and the run takes minutes.
            failures.push(format!(
                "{label}: the program changed\n{}",
                first_difference(&before, &after)
            ));
            continue;
        }
        let before = support::comment_multiset(&source);
        let after = support::comment_multiset(&once);
        if before != after {
            let mut changed: Vec<String> = Vec::new();
            for (comment, count) in &before {
                let now = after.get(comment).copied().unwrap_or(0);
                if now != *count {
                    changed.push(format!("  - {count}x {:?} -> {now}x", comment.1));
                }
            }
            for (comment, count) in &after {
                if !before.contains_key(comment) {
                    changed.push(format!("  + {count}x {:?}", comment.1));
                }
            }
            changed.truncate(6);
            failures.push(format!(
                "{label}: a comment was lost, gained or rewritten\n{}",
                changed.join("\n")
            ));
            continue;
        }

        formatted += 1;
    }

    eprintln!(
        "upstream corpus: {formatted} formatted, {skipped} unparseable, {} failed",
        failures.len()
    );

    assert!(
        failures.is_empty(),
        "{} of {} upstream modules broke an invariant:\n{}",
        failures.len(),
        modules.len(),
        // The first twenty. A list of nine hundred is not a bug report.
        failures
            .iter()
            .take(20)
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join("\n")
    );
}

/// Every excluded name is a module that is actually there.
///
/// An exclusion that no longer matches anything is an exclusion nobody will
/// ever delete, because nothing goes wrong when they don't.
#[test]
fn no_stale_exclusions() {
    let modules = flow_modules();
    if modules.is_empty() {
        return;
    }
    let labels: Vec<String> = modules
        .iter()
        .map(|module| {
            module
                .strip_prefix(corpus_root())
                .unwrap_or(module)
                .display()
                .to_string()
        })
        .collect();

    for slow in KNOWN_SLOW.iter().chain(KNOWN_BROKEN.iter()).copied() {
        // Only when its own fixture is checked out: `UF_CORPUS=metro` must
        // not fail because a React Native path is not there.
        let fixture = slow.split('/').next().unwrap_or_default();
        if !corpus_root().join(fixture).is_dir() || !wanted().iter().any(|want| want == fixture) {
            continue;
        }
        assert!(
            labels.iter().any(|label| label == slow),
            "{slow} is excluded but no longer in the corpus — delete the entry"
        );
    }
}

/// The first line at which two structural renderings differ, with a little
/// of the tree around it.
///
/// `similar_asserts` would print the whole diff, and the whole diff of two
/// serialized React modules is tens of thousands of lines.
fn first_difference(before: &str, after: &str) -> String {
    let mut before = before.lines();
    let mut after = after.lines();
    let mut context: Vec<&str> = Vec::new();
    let mut line = 0usize;

    loop {
        line += 1;
        match (before.next(), after.next()) {
            (None, None) => return "  (identical)".to_owned(),
            (a, b) if a == b => {
                if let Some(a) = a {
                    context.push(a);
                    if context.len() > 6 {
                        context.remove(0);
                    }
                }
            }
            (a, b) => {
                let mut out = String::new();
                for line in &context {
                    out.push_str(&format!("    {}\n", line.trim()));
                }
                out.push_str(&format!("  line {line}\n"));
                out.push_str(&format!("  - {}\n", a.unwrap_or("<end>").trim()));
                out.push_str(&format!("  + {}", b.unwrap_or("<end>").trim()));
                return out;
            }
        }
    }
}
