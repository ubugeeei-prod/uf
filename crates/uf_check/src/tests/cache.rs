//! What the check cache promises, and every way it has to stop believing.
//!
//! A cache that is merely fast is not the feature: a stale diagnostic, or a
//! missing one, is a checker that lies, and it lies quietly. So most of what is
//! here is about *invalidation* — one test per input the key claims to cover,
//! each of which fails if that input is taken back out of it.

use std::fs;

use tempfile::TempDir;

use crate::cache::CheckCache;
use crate::{CheckLimits, CheckReport, Source, check_sources_cached};

/// Tests must not race the wall clock; a loaded CI box is not a type error.
fn limits() -> CheckLimits {
    CheckLimits::default().without_timeout()
}

/// Check `files` through `cache`, in the order given.
fn check(cache: &CheckCache, limits: &CheckLimits, files: &[(&str, &str)]) -> CheckReport {
    let sources: Vec<Source<'_>> = files
        .iter()
        .map(|(path, source)| Source::new(path, source))
        .collect();
    check_sources_cached(&sources, &[], limits, Some(cache)).expect("the checker runs")
}

/// The error codes a report carries, in order, without the parse errors.
///
/// `uf check` drops those too: a batch here may contain a `package.json`,
/// because that is how a package name is resolved, and the Flow parser has an
/// opinion about JSON that no test in this file is about.
fn codes(report: &CheckReport) -> Vec<&str> {
    report
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.kind != crate::DiagnosticKind::Parse)
        .map(|diagnostic| diagnostic.code.unwrap_or("<none>"))
        .collect()
}

/// A three-deep import chain and a file that is in none of it.
///
/// `app.js` reads a type out of `mode.js`, which reads one out of `names.js`;
/// `alone.js` imports nothing. Which is the point: it is what a run has to
/// *not* re-check.
const CHAIN: [(&str, &str); 4] = [
    (
        "app.js",
        "// @flow\nimport type { Mode } from \"./mode.js\";\nexport const mode: Mode = \"onSubmit\";\n",
    ),
    (
        "mode.js",
        "// @flow\nimport type { Name } from \"./names.js\";\nexport type Mode = Name;\n",
    ),
    (
        "names.js",
        "// @flow\nexport type Name = \"onSubmit\" | \"onChange\";\nexport function pick(): number {\n  return 1;\n}\n",
    ),
    ("alone.js", "// @flow\nexport const one: number = 1;\n"),
];

/// The batch above with one file's text replaced.
fn chain_with(path: &str, source: &'static str) -> Vec<(&'static str, &'static str)> {
    CHAIN
        .iter()
        .map(|(name, text)| {
            if *name == path {
                (*name, source)
            } else {
                (*name, *text)
            }
        })
        .collect()
}

#[test]
fn a_second_run_over_an_unchanged_batch_is_answered_entirely_from_the_cache() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).expect("this process can name its own binary");

    let first = check(&cache, &limits(), &CHAIN);
    let second = check(&cache, &limits(), &CHAIN);

    assert_eq!(first.files_from_cache, 0, "nothing was there to answer");
    assert_eq!(second.files_from_cache, CHAIN.len());
    assert_eq!(second.files_checked, first.files_checked);
}

#[test]
fn a_run_served_from_the_cache_reports_exactly_what_filled_it() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).unwrap();
    // A batch with something to say: a type error, a warning-free file, a hole
    // where an import resolves to nothing, and a file that opted out.
    let files = [
        (
            "app.js",
            "// @flow\nimport { thing } from \"some-package\";\nexport const total: number = \"twelve\";\nexport const used: mixed = thing;\n",
        ),
        ("opted.js", "// @noflow\nconst n: number = \"no\";\n"),
        ("clean.js", "// @flow\nexport const one: number = 1;\n"),
    ];

    let first = check(&cache, &limits(), &files);
    let second = check(&cache, &limits(), &files);

    assert_eq!(
        serde_json::to_string(&first.diagnostics).unwrap(),
        serde_json::to_string(&second.diagnostics).unwrap(),
        "a cached run must render the same bytes as the run that filled it"
    );
    assert_eq!(first.untyped_modules, second.untyped_modules);
    assert_eq!(first.files_checked, second.files_checked);
    assert_eq!(first.files_skipped, second.files_skipped);
    assert!(!first.diagnostics.is_empty(), "the batch has a type error");
    assert_eq!(first.untyped_modules, ["some-package"]);
    assert_eq!(first.files_skipped, 1, "`opted.js` said `@noflow`");
}

#[test]
fn editing_one_file_rechecks_it_and_the_files_that_reach_it_and_nothing_else() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).unwrap();
    check(&cache, &limits(), &CHAIN);

    // `names.js` declares one fewer name, which changes the type `mode.js`
    // exports and so the type `app.js` checks its constant against.
    let edited = chain_with(
        "names.js",
        "// @flow\nexport type Name = \"onChange\";\nexport function pick(): number {\n  return 1;\n}\n",
    );
    let second = check(&cache, &limits(), &edited);

    assert_eq!(
        second.files_from_cache, 1,
        "only `alone.js`, which reaches none of it"
    );
    assert_eq!(
        codes(&second),
        ["incompatible-type"],
        "`app.js` is now wrong about `\"onSubmit\"`"
    );
}

#[test]
fn editing_a_body_that_moves_no_declaration_rechecks_only_that_file() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).unwrap();
    check(&cache, &limits(), &CHAIN);

    // The same declarations in the same places, and a different number in a
    // function body. Nothing that crosses the module boundary moved, so the
    // two files that import this one are still answered from disk — which is
    // what a digest over the packed *signature*, rather than over the source
    // text, buys.
    let edited = chain_with(
        "names.js",
        "// @flow\nexport type Name = \"onSubmit\" | \"onChange\";\nexport function pick(): number {\n  return 2;\n}\n",
    );
    let second = check(&cache, &limits(), &edited);

    assert_eq!(second.files_from_cache, CHAIN.len() - 1);
    assert!(second.diagnostics.is_empty(), "{:?}", codes(&second));
}

#[test]
fn a_dependency_whose_signature_changed_is_not_still_believed() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).unwrap();
    let before = [
        (
            "app.js",
            "// @flow\nimport type { Mode } from \"./mode.js\";\nexport const mode: Mode = \"onSubmit\";\n",
        ),
        (
            "mode.js",
            "// @flow\nexport type Mode = \"onSubmit\" | \"onChange\";\n",
        ),
    ];
    let first = check(&cache, &limits(), &before);
    assert!(first.diagnostics.is_empty(), "{:?}", codes(&first));

    let after = [
        before[0],
        ("mode.js", "// @flow\nexport type Mode = \"onChange\";\n"),
    ];
    let second = check(&cache, &limits(), &after);

    assert_eq!(
        codes(&second),
        ["incompatible-type"],
        "`app.js` must be checked again against the type `mode.js` now exports"
    );
    assert_eq!(second.files_from_cache, 0);
}

#[test]
fn a_specifier_that_starts_resolving_is_not_still_typed_as_any() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).unwrap();
    let app = (
        "app.js",
        "// @flow\nimport type { Mode } from \"./mode.js\";\nexport const mode: Mode = \"onSubmit\";\n",
    );

    // No `mode.js` in the batch at all: the import is a hole, so `Mode` is not
    // a type this batch has, and the constant is never compared with anything.
    let alone = check(&cache, &limits(), &[app]);
    assert_eq!(alone.untyped_modules, ["./mode.js"]);
    assert_eq!(codes(&alone), ["value-as-type"]);

    // `app.js` has not changed one byte. What changed is the batch it is in.
    let together = check(
        &cache,
        &limits(),
        &[
            app,
            ("mode.js", "// @flow\nexport type Mode = \"onChange\";\n"),
        ],
    );

    assert!(together.untyped_modules.is_empty());
    assert_eq!(codes(&together), ["incompatible-type"]);
    assert_eq!(together.files_from_cache, 0);
}

#[test]
fn a_manifest_that_stops_publishing_a_name_unmakes_the_types_it_gave() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).unwrap();
    // `app.js` reaches `pkg/index.js` twice: once through the name the manifest
    // publishes, and once by relative path. So when the manifest stops
    // publishing that name, the *set* of modules `app.js` reaches does not
    // change and neither does any of their signatures — the only thing that
    // changes is what one specifier resolved to. Which is why the digest
    // records that, and not only what it reached.
    let app = (
        "app.js",
        "// @flow\nimport type { Mode } from \"@scope/pkg\";\nimport type { Count } from \"./pkg/index.js\";\nexport const mode: Mode = \"onSubmit\";\nexport const count: Count = 1;\n",
    );
    let index = (
        "pkg/index.js",
        "// @flow\nexport type Mode = \"onChange\";\nexport type Count = number;\n",
    );
    let published = check(
        &cache,
        &limits(),
        &[
            app,
            index,
            (
                "pkg/package.json",
                "{ \"name\": \"@scope/pkg\", \"exports\": { \".\": \"./index.js\" } }\n",
            ),
        ],
    );
    assert_eq!(
        codes(&published),
        ["incompatible-type"],
        "`Mode` is `\"onChange\"`, so `\"onSubmit\"` is wrong"
    );

    let renamed = check(
        &cache,
        &limits(),
        &[
            app,
            index,
            (
                "pkg/package.json",
                "{ \"name\": \"@scope/other\", \"exports\": { \".\": \"./index.js\" } }\n",
            ),
        ],
    );

    assert_eq!(renamed.untyped_modules, ["@scope/pkg"]);
    assert_eq!(
        codes(&renamed),
        ["value-as-type"],
        "the name resolves to nothing now, so `Mode` is not a type this batch has"
    );
}

#[test]
fn a_rebuilt_uf_is_not_served_what_the_previous_one_decided() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    // The shape `binary_identity` produces: a path, a size and a modification
    // time, the last of which is what a rebuild changes.
    let first_build = CheckCache::for_identity(project.path(), "/bin/uf\x00111\x00222");
    let second_build = CheckCache::for_identity(project.path(), "/bin/uf\x00111\x00333");

    check(&first_build, &limits(), &CHAIN);
    let rebuilt = check(&second_build, &limits(), &CHAIN);
    // The old build's entries are still *there*, keyed by the build that wrote
    // them: the key identifies a compiler rather than counting generations, so
    // checking out the previous binary is a warm run and not a cold one.
    let returned = check(&first_build, &limits(), &CHAIN);

    assert_eq!(
        rebuilt.files_from_cache, 0,
        "another `uf` compiled these answers"
    );
    assert_eq!(returned.files_from_cache, CHAIN.len());
}

#[test]
fn changing_the_limits_a_check_runs_under_is_a_different_check() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).unwrap();
    check(&cache, &limits(), &CHAIN);

    let mut deeper = limits();
    deeper.recursion_limit /= 2;
    let second = check(&cache, &deeper, &CHAIN);

    assert_eq!(
        second.files_from_cache, 0,
        "how far inference may recurse is not a property of any one file"
    );
}

#[test]
fn a_library_definition_the_project_added_is_a_different_check() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).unwrap();
    let sources = [Source::new(
        "src/kind.js",
        "// @flow\nexport const kind: UmlEdgeKind = \"assoc\";\n",
    )];
    let libs = [Source::new(
        "flow-typed/globals.js",
        "declare type UmlEdgeKind = string;\n",
    )];

    // Nothing about the file changed; what a global *means* did. A record
    // filed without the libdefs in its key would answer here with the run
    // that could not resolve the name.
    let cold =
        check_sources_cached(&sources, &[], &limits(), Some(&cache)).expect("the checker runs");
    assert_eq!(codes(&cold), ["cannot-resolve-name"]);

    let declared =
        check_sources_cached(&sources, &libs, &limits(), Some(&cache)).expect("the checker runs");

    assert_eq!(codes(&declared), Vec::<&str>::new());
    assert_eq!(
        declared.files_from_cache, 0,
        "what the library definitions declare is not a property of any one file"
    );
    // And back, from the record the first run left rather than by inferring
    // it again: the two keys are two entries and neither took the other back.
    let again =
        check_sources_cached(&sources, &[], &limits(), Some(&cache)).expect("the checker runs");
    assert_eq!(codes(&again), ["cannot-resolve-name"]);
    assert_eq!(again.files_from_cache, 1);
}

#[test]
fn a_limit_a_cached_run_would_never_reach_is_still_enforced() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).unwrap();
    check(&cache, &limits(), &CHAIN);

    // The source-size limit is checked before the parser sees the file, so a
    // run answered from the cache would never reach it — which is exactly why
    // the limits have to be in the key rather than consulted afterwards.
    let tiny = limits().with_max_source_bytes(8);
    let sources: Vec<Source<'_>> = CHAIN
        .iter()
        .map(|(path, source)| Source::new(path, source))
        .collect();
    let error = check_sources_cached(&sources, &[], &tiny, Some(&cache))
        .expect_err("every file in the batch is past the limit");

    assert!(matches!(error, crate::CheckError::SourceTooLarge { .. }));
}

#[test]
fn an_unreadable_entry_is_a_miss_rather_than_a_crash() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).unwrap();
    let first = check(&cache, &limits(), &CHAIN);

    let directory = project.path().join(".uf").join("cache").join("check");
    let mut damaged = 0;
    for entry in fs::read_dir(&directory).unwrap() {
        let entry = entry.unwrap().path();
        // Three ways a document can be wrong, so that no one of them is the
        // only one tested: truncated, well-formed JSON of the wrong shape, and
        // a record that answers for another file.
        let content = match damaged % 3 {
            0 => "{\"version\": 1, \"path\":".to_owned(),
            1 => "[]".to_owned(),
            _ => fs::read_to_string(&entry)
                .unwrap()
                .replace("\"path\":\"", "\"path\":\"somewhere/else/"),
        };
        fs::write(&entry, content).unwrap();
        damaged += 1;
    }
    assert_eq!(damaged, CHAIN.len(), "one entry per file was written");

    let second = check(&cache, &limits(), &CHAIN);

    assert_eq!(second.files_from_cache, 0);
    assert_eq!(
        serde_json::to_string(&second.diagnostics).unwrap(),
        serde_json::to_string(&first.diagnostics).unwrap()
    );
}

#[test]
fn a_cache_that_cannot_be_written_is_a_slower_run_and_not_a_failed_one() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    // A regular file where the cache wants a directory: every `create_dir_all`
    // under it fails, which is the shape a read-only checkout has too.
    fs::write(project.path().join(".uf"), "not a directory\n").unwrap();
    let cache = CheckCache::open(project.path()).unwrap();

    let first = check(&cache, &limits(), &CHAIN);
    let second = check(&cache, &limits(), &CHAIN);

    assert_eq!(first.files_from_cache, 0);
    assert_eq!(second.files_from_cache, 0);
    assert_eq!(second.files_checked, first.files_checked);
}
