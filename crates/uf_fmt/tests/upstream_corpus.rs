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

/// The manifest, as `(name, url, commit)` — every line that is not blank or a
/// comment.
fn manifest() -> Vec<(String, String, String)> {
    let source = fs::read_to_string(manifest_path()).expect("tools/corpus/repos.txt");
    source
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| {
            let mut fields = line.split_whitespace();
            let name = fields.next().expect("a name").to_owned();
            let url = fields.next().unwrap_or_default().to_owned();
            let commit = fields.next().unwrap_or_default().to_owned();
            assert!(
                fields.next().is_none(),
                "{name}: a manifest line is `name url commit` and nothing else"
            );
            (name, url, commit)
        })
        .collect()
}

fn manifest_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/corpus/repos.txt")
}

/// Adding a corpus repository is one line in one file.
///
/// It used to be one line in `tools/corpus/repos.txt` *or* a `.gitmodules`
/// entry plus a gitlink, depending on which half of the corpus you were
/// looking at, and `tools/corpus/sync.sh` had to run both mechanisms.
/// ubugeeei-prod/uf#137 converged them on the manifest, and this is what
/// keeps the second mechanism from growing back: a corpus fixture must never
/// be a submodule again.
///
/// `upstream/flow` is the one submodule this repository has left, and it has
/// to stay one for a reason that does not apply to the corpus: it is a cargo
/// *path dependency*, so nothing in the workspace resolves without it. That
/// is why this asserts the whole list rather than only that the corpus is
/// absent from it.
#[test]
fn the_corpus_is_a_manifest_and_not_a_set_of_submodules() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let declared = fs::read_to_string(root.join(".gitmodules")).expect(".gitmodules");
    let paths: Vec<&str> = declared
        .lines()
        .filter_map(|line| line.trim().strip_prefix("path"))
        .filter_map(|rest| rest.trim_start().strip_prefix('='))
        .map(str::trim)
        .collect();

    assert_eq!(
        paths,
        ["upstream/flow"],
        "the corpus belongs in tools/corpus/repos.txt, not in .gitmodules"
    );

    let names: Vec<String> = manifest().into_iter().map(|(name, ..)| name).collect();
    for converged in ["react", "metro", "relay", "react-native"] {
        assert!(
            names.iter().any(|name| name == converged),
            "{converged} was a submodule and its pin belongs in the manifest now"
        );
    }
}

/// Every manifest line is a name, a URL and a full commit.
///
/// A pin is what makes the corpus reproducible, and the three ways to lose
/// that quietly are an abbreviated sha, a branch name where a sha belongs,
/// and the same repository listed twice under different names. `sync.sh`
/// would fetch all three without complaining.
#[test]
fn every_pin_is_a_full_commit() {
    let entries = manifest();
    assert!(!entries.is_empty(), "the manifest is empty");

    let mut seen: Vec<&str> = Vec::new();
    for (name, url, commit) in &entries {
        assert!(
            url.starts_with("https://") && url.ends_with(".git"),
            "{name}: {url} is not an https git URL"
        );
        assert!(
            commit.len() == 40 && commit.chars().all(|c| c.is_ascii_hexdigit()),
            "{name}: {commit} is not a full commit"
        );
        assert!(!seen.contains(&name.as_str()), "{name} is listed twice");
        seen.push(name);
    }
}

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
/// Named rather than skipped by a heuristic, so that a slow module is a
/// decision somebody wrote down. `no_stale_exclusions` checks that each name
/// is still a file in the corpus; it cannot check that the module is still
/// slow, because finding that out means formatting it, which is the thing
/// the exclusion exists to avoid.
const KNOWN_SLOW: [&str; 0] = [];

/// Modules the printer gets wrong, each with the issue that says how.
///
/// Separate from {@link KNOWN_SLOW} because they are different problems and
/// a single list of excuses hides that.
///
/// Empty. It held nine modules across two bugs — ubugeeei-prod/uf#133, a
/// right-nested logical chain re-associating, and ubugeeei-prod/uf#134, a
/// predicate function losing its colon — and both are fixed, so all 8,119 of
/// the Flow modules Meta's parser reads come back parseable, settled, with
/// the same tree and the same comments. (One of the 8,120 it does not read,
/// which is not this crate's business.)
///
/// It stayed at nine for a while after they were fixed, which is why
/// `no_stale_exclusions` now formats what is listed here rather than taking
/// the list's word for it.
const KNOWN_BROKEN: [&str; 0] = [];

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

/// What the three guarantees had to say about one module.
#[derive(Debug)]
enum Verdict {
    /// Meta's parser would not read it. Not this crate's business: the corpus
    /// is other people's repositories and some of their fixtures are
    /// deliberately malformed.
    Unparseable,
    /// It came back parseable, settled, and saying the same things.
    Held,
    /// It did not, and this is the first way it did not.
    Broke(String),
}

/// Run the three guarantees over one module.
///
/// Shared by [`upstream_flow_survives_formatting`], which needs it to hold,
/// and [`no_stale_exclusions`], which needs it *not* to for anything still
/// named in {@link KNOWN_BROKEN}. One implementation, so the two can never
/// disagree about what "broken" means.
fn guarantees(source: &str, config: &FmtConfig) -> Verdict {
    let Ok(once) = format_source(source, config) else {
        return Verdict::Unparseable;
    };
    let once = once.output;

    // 1. The output parses. Anything else means `uf fmt` writes a file that no
    //    longer builds, which is the one failure a formatter must not have.
    let twice = match format_source(&once, config) {
        Ok(twice) => twice.output,
        Err(error) => return Verdict::Broke(format!("output does not format again: {error}")),
    };

    // 2. It settles.
    if once != twice {
        return Verdict::Broke("not idempotent".to_owned());
    }

    // 3. It means the same thing, and says the same things.
    let before = support::structure(source);
    let after = support::structure(&once);
    if before != after {
        // The first place the trees part, rather than "the program changed". A
        // verdict with no evidence sends whoever reads it back to reproduce
        // the run by hand, and the run takes minutes.
        return Verdict::Broke(format!(
            "the program changed\n{}",
            first_difference(&before, &after)
        ));
    }
    let before = support::comment_multiset(source);
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
        return Verdict::Broke(format!(
            "a comment was lost, gained or rewritten\n{}",
            changed.join("\n")
        ));
    }

    Verdict::Held
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

        match guarantees(&source, &config) {
            Verdict::Unparseable => skipped += 1,
            Verdict::Held => formatted += 1,
            Verdict::Broke(how) => failures.push(format!("{label}: {how}")),
        }
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

/// Every exclusion is a module that is there, and — where it can be checked
/// — a module that is still broken.
///
/// A list of excuses rots two ways. A name that no longer matches a file is
/// an exclusion nobody will delete, because nothing goes wrong when they
/// don't. A name whose bug has been *fixed* is worse: the module goes on
/// being skipped, silently, and the corpus quietly stops covering it.
///
/// That second one happened. ubugeeei-prod/uf#133 and ubugeeei-prod/uf#134
/// were both closed with all nine of their modules still listed, and the
/// only thing that noticed was someone emptying the list by hand to see what
/// would happen. So this formats what {@link KNOWN_BROKEN} names rather than
/// taking the list's word for it.
///
/// {@link KNOWN_SLOW} gets the existence check and not the other one:
/// finding out whether a module is still slow means formatting it, and not
/// formatting it is the entire point of the exclusion.
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

    for excluded in KNOWN_SLOW.iter().chain(KNOWN_BROKEN.iter()).copied() {
        // Only when its own fixture is checked out: `UF_CORPUS=metro` must
        // not fail because a React Native path is not there.
        if !checked_out(excluded) {
            continue;
        }
        assert!(
            labels.iter().any(|label| label == excluded),
            "{excluded} is excluded but no longer in the corpus — delete the entry"
        );
    }

    let config = FmtConfig::default();
    let mut fixed: Vec<&str> = Vec::new();
    for excluded in KNOWN_BROKEN {
        if !checked_out(excluded) {
            continue;
        }
        let Ok(source) = fs::read_to_string(corpus_root().join(excluded)) else {
            continue;
        };
        if matches!(guarantees(&source, &config), Verdict::Held) {
            fixed.push(excluded);
        }
    }

    assert!(
        fixed.is_empty(),
        "{} excluded module(s) now hold all three guarantees — take them out of \
         KNOWN_BROKEN, and close the issue if it was the last one:\n  {}",
        fixed.len(),
        fixed.join("\n  ")
    );
}

/// Whether the repository an excluded path belongs to is part of this run.
///
/// A path is `<fixture>/<rest>`, and `UF_CORPUS=metro` narrows the run to
/// one fixture, so an exclusion under another must not be judged.
fn checked_out(excluded: &str) -> bool {
    let fixture = excluded.split('/').next().unwrap_or_default();
    corpus_root().join(fixture).is_dir() && wanted().iter().any(|want| want == fixture)
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
