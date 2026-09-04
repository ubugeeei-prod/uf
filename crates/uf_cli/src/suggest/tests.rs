use super::*;

/// The tasks in this repository's own `uf.config.js`, which is the list a
/// contributor actually mistypes.
const TASKS: &[&str] = &[
    "build",
    "ci",
    "docs:build",
    "flow:clippy",
    "flow:test",
    "install:test",
    "manifests",
    "rust:bench",
    "rust:clippy",
    "rust:fmt",
    "rust:fmt:check",
    "rust:metadata",
    "rust:test",
    "test:lib",
];

#[test]
fn a_transposition_is_one_edit_not_two() {
    assert_eq!(damerau_levenshtein("build", "biuld"), 1);
    assert_eq!(damerau_levenshtein("test", "tset"), 1);
}

#[test]
fn the_usual_edits_cost_what_they_should() {
    assert_eq!(damerau_levenshtein("build", "build"), 0);
    assert_eq!(damerau_levenshtein("build", "buil"), 1, "a deletion");
    assert_eq!(damerau_levenshtein("build", "builds"), 1, "an insertion");
    assert_eq!(damerau_levenshtein("build", "bujld"), 1, "a substitution");
    assert_eq!(damerau_levenshtein("", "build"), 5);
    assert_eq!(damerau_levenshtein("build", ""), 5);
    assert_eq!(damerau_levenshtein("", ""), 0);
}

#[test]
fn distance_is_symmetric() {
    for (left, right) in [
        ("build", "biuld"),
        ("rust:test", "rust:tset"),
        ("ci", "cli"),
        ("", "x"),
    ] {
        assert_eq!(
            damerau_levenshtein(left, right),
            damerau_levenshtein(right, left),
            "{left} vs {right}"
        );
    }
}

#[test]
fn distance_counts_characters_rather_than_bytes() {
    // Two characters, six bytes. A byte-wise measure would call this distance 6.
    assert_eq!(damerau_levenshtein("日本", "日本"), 0);
    assert_eq!(damerau_levenshtein("日本", "日水"), 1);
}

#[test]
fn the_obvious_typo_is_the_first_suggestion() {
    assert_eq!(closest("biuld", TASKS.iter().copied()), vec!["build"]);
    assert_eq!(
        closest("rust:tset", TASKS.iter().copied()).first(),
        Some(&"rust:test")
    );
}

/// Not a typo — an ambiguity. Both answers are useful, so both are offered.
#[test]
fn a_prefix_offers_every_name_it_could_have_meant() {
    let out = closest("rust:fmt", TASKS.iter().copied());

    assert!(out.contains(&"rust:fmt"), "{out:?}");
    assert!(out.contains(&"rust:fmt:check"), "{out:?}");
}

#[test]
fn a_substring_match_beats_a_close_spelling() {
    // `test` is inside four task names and one edit from none of them.
    let out = closest("test", TASKS.iter().copied());

    assert!(!out.is_empty());
    assert!(
        out.iter().all(|name| name.contains("test")),
        "a containing name should outrank a merely similar one, got {out:?}"
    );
}

#[test]
fn matching_ignores_case() {
    // `docs:build` contains `build`, so it is offered too — the same rule that
    // makes `rust:fmt` offer `rust:fmt:check`. What matters is which comes
    // first, and an exact match sorts ahead of a name that merely contains it.
    assert_eq!(
        closest("BUILD", TASKS.iter().copied()),
        vec!["build", "docs:build"]
    );
    assert_eq!(
        closest("Rust:Test", TASKS.iter().copied()).first(),
        Some(&"rust:test")
    );
}

/// Within the containment rank, the shortest name is the closest thing to what
/// was typed, and an exact match is the shortest of all.
#[test]
fn an_exact_match_outranks_a_name_that_merely_contains_it() {
    assert_eq!(
        closest("build", ["docs:build", "rebuild", "build"]).first(),
        Some(&"build")
    );
}

#[test]
fn nothing_similar_suggests_nothing() {
    assert!(closest("wibble", TASKS.iter().copied()).is_empty());
    assert!(closest("zzzzzzzzzzzz", TASKS.iter().copied()).is_empty());
}

/// A very short word is close to everything, and a suggestion drawn from that
/// is noise rather than help.
#[test]
fn a_word_too_short_to_be_a_typo_gets_no_distance_budget() {
    assert_eq!(distance_budget("ci"), 0);
    assert_eq!(distance_budget("a"), 0);
    assert_eq!(distance_budget(""), 0);

    // `cli` is one edit from `ci`, and is not offered.
    assert!(closest("xy", ["ci", "build"]).is_empty());
    // An exact match still works, through the containment rule.
    assert_eq!(closest("ci", TASKS.iter().copied()).first(), Some(&"ci"));
}

#[test]
fn the_budget_grows_with_the_length_of_what_was_typed() {
    assert_eq!(distance_budget("abc"), 1);
    assert_eq!(distance_budget("abcdef"), 2);
    assert_eq!(distance_budget("abcdefghijk"), 3);
}

#[test]
fn no_more_than_three_suggestions_are_offered() {
    let many = [
        "aaaa", "aaab", "aaac", "aaad", "aaae", "aaaf", "aaag", "aaah",
    ];

    assert!(closest("aaaa", many).len() <= MAX_SUGGESTIONS);
}

/// The same input must produce the same list, or a test that asserts on the
/// message becomes flaky and a reader sees a different hint each time.
#[test]
fn suggestions_are_ordered_deterministically() {
    let first = closest("rust", TASKS.iter().copied());

    for _ in 0..16 {
        assert_eq!(closest("rust", TASKS.iter().copied()), first);
    }
}

#[test]
fn an_empty_candidate_list_suggests_nothing() {
    assert!(closest("build", std::iter::empty()).is_empty());
}
