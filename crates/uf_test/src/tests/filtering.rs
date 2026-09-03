//! What `-t` and a path filter accept, and what they store.
//!
//! These are the decisions the filter makes on its own. What a filter does to
//! a *run* — which cases are skipped and with which reason — is behaviour of
//! the whole runner, and is tested against a real host in
//! `crates/uf_cli/tests/testing.rs`.

use crate::{MAX_PATTERN_BYTES, TestFilter};

#[test]
fn an_empty_filter_excludes_nothing() {
    let filter = TestFilter::new();
    assert!(filter.is_empty());
    assert!(filter.matches_path("anything"));
    assert!(filter.matches_name("anything"));
}

#[test]
fn a_name_filter_matches_anywhere_in_the_fully_qualified_name() {
    let filter = TestFilter::new().with_name("adds");
    assert!(filter.matches_name("math > adds"));
    assert!(filter.matches_name("adds"));
    assert!(!filter.matches_name("math > subtracts"));
}

#[test]
fn a_name_filter_can_match_the_describe_alone() {
    let filter = TestFilter::new().with_name("button");
    assert!(filter.matches_name("button > renders"));
    assert!(!filter.matches_name("math > adds"));
}

#[test]
fn an_empty_name_pattern_is_ignored_rather_than_matching_nothing() {
    let filter = TestFilter::new().with_name("");
    assert!(filter.is_empty());
    assert!(filter.matches_name("anything at all"));
}

#[test]
fn a_whitespace_only_pattern_is_ignored() {
    assert!(TestFilter::new().with_name("   ").is_empty());
    assert!(TestFilter::new().with_path("\t\n").is_empty());
}

#[test]
fn a_path_filter_keeps_only_matching_files() {
    let filter = TestFilter::new().with_path("src/ui/");
    assert!(filter.matches_path("src/ui/button.test.js"));
    assert!(!filter.matches_path("src/math.test.js"));
}

#[test]
fn several_path_filters_widen_the_selection() {
    let filter = TestFilter::new().with_path("math").with_path("button");
    assert!(filter.matches_path("src/math.test.js"));
    assert!(filter.matches_path("src/ui/button.test.js"));
    assert!(!filter.matches_path("src/other.test.js"));
}

#[test]
fn path_filters_can_be_added_in_bulk() {
    let filter = TestFilter::new().with_paths(["math", "button"]);
    assert_eq!(filter.path_patterns().len(), 2);
}

#[test]
fn filtering_is_case_sensitive() {
    assert!(
        !TestFilter::new()
            .with_name("ADDS")
            .matches_name("math > adds")
    );
}

#[test]
fn a_pattern_is_reported_back_trimmed() {
    let filter = TestFilter::new().with_name("  adds  ");
    assert_eq!(filter.name_pattern(), Some("adds"));
}

#[test]
fn an_over_long_pattern_is_truncated_rather_than_rejected() {
    let pattern = "x".repeat(MAX_PATTERN_BYTES * 4);
    let filter = TestFilter::new().with_name(&pattern);

    assert_eq!(filter.name_pattern().unwrap().len(), MAX_PATTERN_BYTES);
}

#[test]
fn truncating_a_pattern_respects_character_boundaries() {
    let pattern = "日".repeat(MAX_PATTERN_BYTES);
    let filter = TestFilter::new().with_name(&pattern);
    // Would panic on a byte slice through the middle of a character.
    assert!(filter.name_pattern().unwrap().len() <= MAX_PATTERN_BYTES);
}
