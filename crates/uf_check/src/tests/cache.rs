//! What the check cache promises, and every way it has to stop believing.
//!
//! A cache that is merely fast is not the feature: a stale diagnostic, or a
//! missing one, is a checker that lies, and it lies quietly. So most of what is
//! here is about *invalidation* — one test per input the key claims to cover,
//! each of which fails if that input is taken back out of it.
//!
//! The other half is the invariant a user actually holds the checker to: **the
//! same tree reports the same diagnostics, in the same order, whatever ran
//! before it.** A cache is allowed to decide how long a run takes and nothing
//! else, so the tests at the end of this file check the orderings a person
//! really produces — whole project, one path, an edit, an edit reverted — and
//! demand the same bytes from every one of them.

use std::fs;
use std::time::Duration;

use tempfile::TempDir;

use crate::cache::{CachedAnswer, CheckCache, MAX_RECORD_ANSWERS, Record};
use crate::{CheckLimits, CheckReport, Source, check_sources_cached};

/// The limits `uf check` runs under, which bound the file and never the clock.
///
/// `without_timeout` is redundant against today's default and stated anyway:
/// it is the thing these tests depend on, and a test that raced a wall clock
/// would pass on an idle laptop and fail on a loaded CI box, which is the
/// failure ubugeeei-prod/uf#565 was about.
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

/// A batch with something to disagree about.
///
/// A type error in the file the reader asked about and another two modules
/// away, an import that resolves to nothing, a file that opted out, and a file
/// that reaches none of it. Enough that a run which lost or duplicated one
/// file's answers reports a different number from one that did not.
const NOISY: [(&str, &str); 5] = [
    (
        "app.js",
        "// @flow\nimport type { Mode } from \"./mode.js\";\nimport { thing } from \"some-package\";\nexport const mode: Mode = \"never\";\nexport const used: mixed = thing;\n",
    ),
    (
        "mode.js",
        "// @flow\nimport type { Name } from \"./names.js\";\nexport type Mode = Name;\n",
    ),
    (
        "names.js",
        "// @flow\nexport type Name = \"onSubmit\" | \"onChange\";\nexport const wrong: number = \"no\";\n",
    ),
    ("opted.js", "// @noflow\nconst n: number = \"no\";\n"),
    ("alone.js", "// @flow\nexport const one: number = 1;\n"),
];

/// `NOISY`'s first file on its own: the same bytes, a different batch.
///
/// `./mode.js` resolves to nothing here, so `Mode` is not a type this batch
/// has and the diagnostics are not the ones the whole-project run computes.
/// That is the point — it is what `uf check app.js` after `uf check` really is.
const NOISY_SCOPED: [(&str, &str); 1] = [NOISY[0]];

/// Every diagnostic in a report, as the bytes a reader would be shown.
fn rendered(report: &CheckReport) -> String {
    serde_json::to_string(&report.diagnostics).expect("diagnostics serialize")
}

/// Every record on disk that is about `path`.
///
/// There is at most one: a record is filed under the file's own text, and these
/// tests never change it while asking.
fn records_about(project: &TempDir, path: &str) -> Vec<Record> {
    let directory = project.path().join(".uf").join("cache").join("check");
    let Ok(entries) = fs::read_dir(&directory) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| fs::read_to_string(entry.path()).ok())
        .filter_map(|document| serde_json::from_str::<Record>(&document).ok())
        .filter(|record| record.path == path)
        .collect()
}

#[test]
fn a_path_scoped_run_and_a_whole_project_run_do_not_take_back_each_others_entries() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).unwrap();

    check(&cache, &limits(), &NOISY);
    let scoped = check(&cache, &limits(), &NOISY_SCOPED);
    let whole = check(&cache, &limits(), &NOISY);
    let scoped_again = check(&cache, &limits(), &NOISY_SCOPED);

    assert_eq!(
        scoped.files_from_cache, 0,
        "a batch of one is a batch this file has not been checked in"
    );
    // ubugeeei-prod/uf#406: with one digest per record these two runs took each
    // other's entry back, every file they both named, and never settled.
    assert_eq!(
        whole.files_from_cache, whole.files_checked,
        "the scoped run must not have taken the project's answers back"
    );
    assert_eq!(
        scoped_again.files_from_cache,
        NOISY_SCOPED.len(),
        "nor the project run the scoped one's"
    );
}

#[test]
fn the_same_batch_reports_the_same_diagnostics_whatever_ran_before_it() {
    if !crate::is_available() {
        return;
    }

    // Cold, with nothing at all before it. This is the answer every other
    // ordering has to agree with, because it is the only one a reader can
    // reproduce from the source alone.
    let cold = TempDir::new().unwrap();
    let first = check(&CheckCache::open(cold.path()).unwrap(), &limits(), &NOISY);

    // Warm: the same batch again, through the cache it just filled.
    let warm = TempDir::new().unwrap();
    let warm_cache = CheckCache::open(warm.path()).unwrap();
    check(&warm_cache, &limits(), &NOISY);
    let warm_again = check(&warm_cache, &limits(), &NOISY);

    // The two orders a user actually produces: `uf check <path>` before
    // `uf check`, and after it.
    let scoped_first = TempDir::new().unwrap();
    let scoped_first_cache = CheckCache::open(scoped_first.path()).unwrap();
    check(&scoped_first_cache, &limits(), &NOISY_SCOPED);
    let after_scoped = check(&scoped_first_cache, &limits(), &NOISY);

    let scoped_between = TempDir::new().unwrap();
    let scoped_between_cache = CheckCache::open(scoped_between.path()).unwrap();
    check(&scoped_between_cache, &limits(), &NOISY);
    check(&scoped_between_cache, &limits(), &NOISY_SCOPED);
    let settled = check(&scoped_between_cache, &limits(), &NOISY);

    for (ordering, report) in [
        ("a second run through a warm cache", &warm_again),
        ("a run after a path-scoped one", &after_scoped),
        ("a run with a path-scoped one in the middle", &settled),
    ] {
        // ubugeeei-prod/uf#564: the number `uf check` reports has to be a fact
        // about the code, not about what the cache happened to hold.
        assert_eq!(
            rendered(report),
            rendered(&first),
            "{ordering} reported different diagnostics"
        );
        assert_eq!(report.untyped_modules, first.untyped_modules, "{ordering}");
        assert_eq!(report.files_checked, first.files_checked, "{ordering}");
        assert_eq!(report.files_skipped, first.files_skipped, "{ordering}");
    }
    assert_eq!(
        settled.files_from_cache, settled.files_checked,
        "and the project's own run is free again afterwards"
    );
}

#[test]
fn a_wall_clock_budget_is_not_part_of_what_a_file_is_filed_under() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).unwrap();

    let cold = check(&cache, &limits(), &NOISY);
    // A budget decides whether a check *finishes*, never what it says, so an
    // embedder that bounds its own latency reads what `uf check` wrote and
    // reports the same thing. ubugeeei-prod/uf#565.
    let budgeted = check(
        &cache,
        &limits().with_file_timeout(Duration::from_secs(600)),
        &NOISY,
    );

    assert_eq!(budgeted.files_from_cache, cold.files_checked);
    assert_eq!(rendered(&budgeted), rendered(&cold));
}

#[test]
fn a_file_checked_many_ways_keeps_a_bounded_number_of_answers() {
    if !crate::is_available() {
        return;
    }
    // Six batches, one per spelling of the type `app.js` checks against: the
    // same file, the same key, six dependency digests.
    const MODES: [&str; 6] = [
        "// @flow\nexport type Mode = \"onSubmit\";\n",
        "// @flow\nexport type Mode = \"onSubmit\" | \"a\";\n",
        "// @flow\nexport type Mode = \"onSubmit\" | \"b\";\n",
        "// @flow\nexport type Mode = \"onSubmit\" | \"c\";\n",
        "// @flow\nexport type Mode = \"onSubmit\" | \"d\";\n",
        "// @flow\nexport type Mode = \"onSubmit\" | \"e\";\n",
    ];
    let app = (
        "app.js",
        "// @flow\nimport type { Mode } from \"./mode.js\";\nexport const mode: Mode = \"onSubmit\";\n",
    );
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).unwrap();
    for mode in MODES {
        check(&cache, &limits(), &[app, ("mode.js", mode)]);
    }

    let held = records_about(&project, "app.js");

    assert_eq!(held.len(), 1, "one record, six answers asked of it");
    assert_eq!(
        held[0].answers.len(),
        MAX_RECORD_ANSWERS,
        "a project checked twenty ways must not grow twenty copies of every file"
    );
    // The four that are kept are the four most recently used, so the batch a
    // project is really checked in stays and the one-off leaves.
    let newest = check(&cache, &limits(), &[app, ("mode.js", MODES[5])]);
    let oldest = check(&cache, &limits(), &[app, ("mode.js", MODES[0])]);

    assert_eq!(newest.files_from_cache, 2, "both files were still known");
    assert_eq!(
        oldest.files_from_cache, 1,
        "`mode.js` is filed under its own text and still hits; `app.js`'s \
         answer for this batch was evicted"
    );
}

#[test]
fn a_record_claiming_more_answers_than_the_bound_is_refused_whole() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).unwrap();
    let first = check(&cache, &limits(), &NOISY);

    // A record is a file anything can write, and a document claiming more
    // answers than `MAX_RECORD_ANSWERS` is not one this build wrote. It is
    // refused whole rather than trusted as far as the bound: reading the first
    // four answers out of a document somebody else authored is still reading
    // it, and the bound exists to cap what a read costs, not to repair a file.
    let directory = project.path().join(".uf").join("cache").join("check");
    let mut inflated = 0;
    for entry in fs::read_dir(&directory).unwrap() {
        let path = entry.unwrap().path();
        let mut record: Record =
            serde_json::from_str(&fs::read_to_string(&path).unwrap()).expect("a record we wrote");
        while record.answers.len() <= MAX_RECORD_ANSWERS {
            record.answers.push(CachedAnswer {
                dependencies: format!("padding-{}", record.answers.len()),
                diagnostics: Vec::new(),
            });
        }
        fs::write(&path, serde_json::to_string(&record).unwrap()).unwrap();
        inflated += 1;
    }
    assert_eq!(inflated, NOISY.len(), "one record per file was written");

    let second = check(&cache, &limits(), &NOISY);

    assert_eq!(
        second.files_from_cache, 0,
        "a record over its bound is a miss, not a partial read"
    );
    assert_eq!(rendered(&second), rendered(&first));
}

/// [`NOISY`] with one file's text replaced.
fn noisy_with(path: &str, source: &'static str) -> Vec<(&'static str, &'static str)> {
    NOISY
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
fn a_dependency_flipped_back_is_answered_from_the_cache_it_already_filled() {
    if !crate::is_available() {
        return;
    }
    let project = TempDir::new().unwrap();
    let cache = CheckCache::open(project.path()).unwrap();
    let edited = noisy_with(
        "names.js",
        "// @flow\nexport type Name = \"onSubmit\";\nexport const wrong: number = \"no\";\n",
    );

    check(&cache, &limits(), &NOISY);
    let changed = check(&cache, &limits(), &edited);
    let back = check(&cache, &limits(), &NOISY);

    assert!(
        changed.files_from_cache < changed.files_checked,
        "the edit has to reach something for the flip back to mean anything"
    );
    // ubugeeei-prod/uf#406's smaller case, and the one a rebase or a bisect
    // produces every day: with one digest per record the answer computed
    // before the edit was gone, so flipping back cost the same inference twice.
    assert_eq!(
        back.files_from_cache, back.files_checked,
        "flipping a dependency back must not re-infer what was already computed"
    );
}

#[test]
fn a_batch_only_half_of_which_was_re_inferred_reports_what_a_cold_run_does() {
    if !crate::is_available() {
        return;
    }
    // `alone.js` is imported by nothing, so editing it re-infers exactly one
    // file and leaves the rest to be replayed. That mixture is where an
    // inference that depended on which files were already resolved would show
    // up as a diagnostic appearing twice, or not at all.
    let edited = noisy_with("alone.js", "// @flow\nexport const one: number = 2;\n");

    let cold = TempDir::new().unwrap();
    let fresh = check(&CheckCache::open(cold.path()).unwrap(), &limits(), &edited);

    let warm = TempDir::new().unwrap();
    let cache = CheckCache::open(warm.path()).unwrap();
    check(&cache, &limits(), &NOISY);
    let partial = check(&cache, &limits(), &edited);

    assert_eq!(
        partial.files_from_cache,
        partial.files_checked - 1,
        "only the edited file should have been inferred again"
    );
    assert_eq!(rendered(&partial), rendered(&fresh));
    assert_eq!(partial.untyped_modules, fresh.untyped_modules);
}
