//! `.uf/cache/lint`: what the React tree rules worked out, kept between runs.
//!
//! Two halves, and both matter. A warm run has to report exactly what a cold
//! one did — a cache that changes a report is a linter that lies. And the
//! record has to be *believed* only for the question it answered: a record is
//! tampered with here, so that a run that reads it says so, and every change
//! that must invalidate it — an edit, a configuration that asks the compiler
//! something else, a rebuilt `uf` — is shown to get the real answer instead.

use std::path::{Path, PathBuf};

use super::*;

const HOOKS: &str = "react-compiler/hooks";
const EXHAUSTIVE_EFFECT_DEPENDENCIES: &str = "react-compiler/exhaustive-effect-dependencies";
const REDUNDANT_MEMO: &str = "react/no-redundant-memo";

/// The identity a test's "current build of uf" is filed under.
const BUILD: &str = "uf-under-test\0size=100\0mtime=200";

const CONDITIONAL_HOOK: &str = "// @flow\nimport {useState} from \"react\";\n\ncomponent Toggle(flag: boolean) {\n  if (flag) {\n    const [on] = useState(false);\n  }\n  return null;\n}\n";

const REDUNDANT: &str = "// @flow\nimport {useMemo} from \"react\";\n\ncomponent List(items: Array<string>) {\n  const sorted = useMemo(() => items.slice(), [items]);\n  return <ul>{sorted}</ul>;\n}\n";

/// A message no compiler writes, so a report that carries it was read from
/// the record this test planted.
const PLANTED: &str = "planted in .uf/cache/lint by this test";

fn config(rules: &[(&str, RuleLevel)]) -> UniflowedConfig {
    let mut config = UniflowedConfig::default();
    config.lint.rules.clear();
    for (rule, level) in rules {
        config.lint.rules.insert(CompactString::from(*rule), *level);
    }
    config
}

fn hooks() -> UniflowedConfig {
    config(&[(HOOKS, RuleLevel::Error), (REDUNDANT_MEMO, RuleLevel::Warn)])
}

fn run(
    files: &[SourceFile],
    config: &UniflowedConfig,
    cache: Option<&LintCache>,
) -> Vec<Diagnostic> {
    lint_sources_cached(files, files, config, cache)
        .expect("lint")
        .diagnostics
}

fn records(root: &Path) -> Vec<PathBuf> {
    let Ok(listing) = std::fs::read_dir(root.join(".uf/cache/lint")) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = listing
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    found.sort();
    found
}

/// Lint `CONDITIONAL_HOOK` once, cold, and replace the message of the finding
/// that run filed with [`PLANTED`]. Returns the record's path.
fn plant(root: &Path, file: &SourceFile) -> PathBuf {
    let cache = LintCache::for_identity(root, BUILD);
    let cold = run(std::slice::from_ref(file), &hooks(), Some(&cache));
    assert_eq!(cold.len(), 1, "{cold:?}");
    assert_ne!(cold[0].message, PLANTED);

    let filed = records(root);
    assert_eq!(filed.len(), 1, "one record per module: {filed:?}");
    let record = &filed[0];
    let mut document: serde_json::Value =
        serde_json::from_slice(&std::fs::read(record).expect("the record reads")).expect("JSON");
    document["answer"]["compiler"][0]["message"] = serde_json::Value::from(PLANTED);
    std::fs::write(record, serde_json::to_vec(&document).expect("serializes"))
        .expect("the record is rewritten");
    record.clone()
}

fn messages(diagnostics: &[Diagnostic]) -> Vec<&str> {
    diagnostics
        .iter()
        .map(|diagnostic| diagnostic.message.as_str())
        .collect()
}

#[test]
fn a_warm_run_reports_exactly_what_a_cold_one_did() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let files = [
        at("app/toggle.js", CONDITIONAL_HOOK),
        at("app/list.js", REDUNDANT),
    ];
    let cache = LintCache::for_identity(root.path(), BUILD);

    let uncached = run(&files, &hooks(), None);
    assert!(fired(&uncached, HOOKS), "{uncached:?}");
    assert!(fired(&uncached, REDUNDANT_MEMO), "{uncached:?}");

    let cold = run(&files, &hooks(), Some(&cache));
    assert_eq!(records(root.path()).len(), 2, "one record per module");
    let warm = run(&files, &hooks(), Some(&cache));

    assert_eq!(cold, uncached);
    assert_eq!(warm, cold);
}

#[test]
fn a_warm_run_reads_the_record_instead_of_asking_the_compiler() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let file = at("app/toggle.js", CONDITIONAL_HOOK);
    plant(root.path(), &file);

    let warm = run(
        std::slice::from_ref(&file),
        &hooks(),
        Some(&LintCache::for_identity(root.path(), BUILD)),
    );
    assert_eq!(messages(&warm), [PLANTED]);
    // Placed and filed as a fresh finding is: under its rule, at its level.
    assert_eq!(warm[0].rule, HOOKS);
    assert_eq!(warm[0].severity, Severity::Error);
    assert_eq!((warm[0].line, warm[0].column), (6, 18));
}

#[test]
fn an_edit_is_not_served_the_previous_texts_answer() {
    let root = tempfile::tempdir().expect("a temporary directory");
    plant(root.path(), &at("app/toggle.js", CONDITIONAL_HOOK));

    let edited = at("app/toggle.js", &CONDITIONAL_HOOK.replace("false", "true"));
    let warm = run(
        std::slice::from_ref(&edited),
        &hooks(),
        Some(&LintCache::for_identity(root.path(), BUILD)),
    );
    assert_eq!(warm.len(), 1, "{warm:?}");
    assert_ne!(warm[0].message, PLANTED);
    // The same text at another path is another question too.
    let moved = at("app/moved.js", CONDITIONAL_HOOK);
    let warm = run(
        std::slice::from_ref(&moved),
        &hooks(),
        Some(&LintCache::for_identity(root.path(), BUILD)),
    );
    assert_eq!(warm.len(), 1, "{warm:?}");
    assert_ne!(warm[0].message, PLANTED);
}

#[test]
fn a_config_that_asks_the_compiler_something_else_is_not_served_the_old_answer() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let file = at("app/toggle.js", CONDITIONAL_HOOK);
    plant(root.path(), &file);
    let cache = LintCache::for_identity(root.path(), BUILD);

    // Switching on the validation the compiler has to be asked for.
    let switched = config(&[
        (HOOKS, RuleLevel::Error),
        (REDUNDANT_MEMO, RuleLevel::Warn),
        (EXHAUSTIVE_EFFECT_DEPENDENCIES, RuleLevel::Error),
    ]);
    let warm = run(std::slice::from_ref(&file), &switched, Some(&cache));
    assert!(fired(&warm, HOOKS), "{warm:?}");
    assert!(!messages(&warm).contains(&PLANTED), "{warm:?}");
}

/// The opposite guarantee, on purpose: a rule's level is applied to a record
/// on the way out, as it is to a fresh answer, so it is not in the key.
#[test]
fn a_rule_level_is_applied_to_the_record_rather_than_keyed_on() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let file = at("app/toggle.js", CONDITIONAL_HOOK);
    plant(root.path(), &file);

    let warned = config(&[(HOOKS, RuleLevel::Warn), (REDUNDANT_MEMO, RuleLevel::Warn)]);
    let warm = run(
        std::slice::from_ref(&file),
        &warned,
        Some(&LintCache::for_identity(root.path(), BUILD)),
    );
    assert_eq!(messages(&warm), [PLANTED]);
    assert_eq!(warm[0].severity, Severity::Warn);
}

#[test]
fn a_rebuilt_uf_is_not_served_what_the_previous_one_decided() {
    let root = tempfile::tempdir().expect("a temporary directory");
    let file = at("app/toggle.js", CONDITIONAL_HOOK);
    plant(root.path(), &file);

    let rebuilt = LintCache::for_identity(root.path(), "uf-under-test\0size=100\0mtime=201");
    let warm = run(std::slice::from_ref(&file), &hooks(), Some(&rebuilt));
    assert_eq!(warm.len(), 1, "{warm:?}");
    assert_ne!(warm[0].message, PLANTED);
    // And it files its own answer beside the first build's rather than over
    // it, so a bisect that comes back to that build finds it warm.
    assert_eq!(records(root.path()).len(), 2);
}

#[test]
fn an_unreadable_record_is_a_miss() {
    let file = at("app/toggle.js", CONDITIONAL_HOOK);
    for damage in [
        &b""[..],
        b"{\"version\":1,\"path\":\"app/toggle.js\",\"answer\":",
        b"not json",
        b"{\"version\":99,\"path\":\"app/toggle.js\",\"answer\":{\"compiler\":[],\"memo\":[]}}",
        b"{\"version\":1,\"path\":\"app/other.js\",\"answer\":{\"compiler\":[],\"memo\":[]}}",
        b"{\"version\":1,\"path\":\"app/toggle.js\",\"answer\":{\"compiler\":\"x\"}}",
    ] {
        let root = tempfile::tempdir().expect("a temporary directory");
        let record = plant(root.path(), &file);
        std::fs::write(&record, damage).expect("the record is damaged");

        let warm = run(
            std::slice::from_ref(&file),
            &hooks(),
            Some(&LintCache::for_identity(root.path(), BUILD)),
        );
        assert!(
            fired(&warm, HOOKS),
            "{}: {warm:?}",
            String::from_utf8_lossy(damage)
        );
        assert!(!messages(&warm).contains(&PLANTED));
    }
}
