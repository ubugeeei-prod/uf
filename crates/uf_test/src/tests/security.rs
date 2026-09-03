//! The guards on everything this crate reads from disk or from source text.
//!
//! Two untrusted inputs reach path handling: the recorded timings document, and
//! import specifiers. Both decide which files get opened, stat-ed and watched,
//! so both are held to the same rule — a path that is not project-relative is
//! not a path this crate acts on.

use crate::{MAX_RELATIVE_PATH_BYTES, is_safe_relative, normalize_relative};

#[test]
fn a_plain_relative_path_is_safe() {
    assert!(is_safe_relative("src/a.test.js"));
    assert!(is_safe_relative("a.js"));
    assert!(is_safe_relative("src/nested/deep/a.js"));
}

#[test]
fn an_empty_path_is_not_safe() {
    assert!(!is_safe_relative(""));
}

#[test]
fn an_absolute_path_is_not_safe() {
    assert!(!is_safe_relative("/etc/passwd"));
    assert!(!is_safe_relative("/"));
}

#[test]
fn a_traversing_path_is_not_safe() {
    assert!(!is_safe_relative("../etc/passwd"));
    assert!(!is_safe_relative("src/../../etc/passwd"));
    assert!(!is_safe_relative("src/.."));
    assert!(!is_safe_relative(".."));
}

#[test]
fn a_dot_segment_is_not_safe() {
    assert!(!is_safe_relative("./a.js"));
    assert!(!is_safe_relative("src/./a.js"));
}

#[test]
fn an_empty_segment_is_not_safe() {
    assert!(!is_safe_relative("src//a.js"));
    assert!(!is_safe_relative("src/"));
}

#[test]
fn a_backslash_is_not_safe() {
    assert!(!is_safe_relative("src\\a.js"));
    assert!(!is_safe_relative("..\\..\\windows\\system32"));
}

#[test]
fn a_windows_drive_is_not_safe() {
    assert!(!is_safe_relative("C:/windows"));
    assert!(!is_safe_relative("c:a.js"));
}

#[test]
fn an_embedded_nul_is_not_safe() {
    assert!(!is_safe_relative("src/a.js\0.txt"));
}

#[test]
fn an_over_long_path_is_not_safe() {
    let path = "a/".repeat(MAX_RELATIVE_PATH_BYTES);
    assert!(!is_safe_relative(&path));
}

#[test]
fn a_path_exactly_at_the_limit_is_safe() {
    let path = "a".repeat(MAX_RELATIVE_PATH_BYTES);
    assert!(is_safe_relative(&path));
}

#[test]
fn a_non_ascii_path_is_safe() {
    assert!(is_safe_relative("src/日本語.test.js"));
}

#[test]
fn a_relative_specifier_resolves_against_its_own_directory() {
    assert_eq!(
        normalize_relative("src/ui/a.test.js", "./button.js").as_deref(),
        Some("src/ui/button.js")
    );
}

#[test]
fn a_parent_specifier_climbs_one_directory() {
    assert_eq!(
        normalize_relative("src/ui/a.test.js", "../shared.js").as_deref(),
        Some("src/shared.js")
    );
}

#[test]
fn a_specifier_that_climbs_out_of_the_project_resolves_to_nothing() {
    assert_eq!(
        normalize_relative("src/a.test.js", "../../../../etc/passwd"),
        None
    );
    assert_eq!(normalize_relative("a.test.js", "../outside.js"), None);
}

#[test]
fn climbing_is_refused_rather_than_clamped() {
    // Clamping `../../x` to `x` is how a traversal turns into reading the wrong
    // file; the resolution must fail instead.
    assert_eq!(normalize_relative("src/a.js", "../../x.js"), None);
}

#[test]
fn a_bare_specifier_is_not_a_project_path() {
    assert_eq!(normalize_relative("src/a.test.js", "react"), None);
    assert_eq!(normalize_relative("src/a.test.js", "@uniflowed/test"), None);
}

#[test]
fn an_absolute_specifier_is_not_a_project_path() {
    assert_eq!(normalize_relative("src/a.test.js", "/etc/passwd"), None);
}

#[test]
fn a_backslash_specifier_is_refused() {
    assert_eq!(normalize_relative("src/a.test.js", ".\\evil.js"), None);
    assert_eq!(normalize_relative("src/a.test.js", "./a\\b.js"), None);
}

#[test]
fn a_nul_specifier_is_refused() {
    assert_eq!(normalize_relative("src/a.test.js", "./a\0.js"), None);
}

#[test]
fn redundant_dot_segments_collapse() {
    assert_eq!(
        normalize_relative("src/a.test.js", "./././util.js").as_deref(),
        Some("src/util.js")
    );
}

#[test]
fn a_specifier_resolving_to_the_root_itself_is_refused() {
    assert_eq!(normalize_relative("a.js", "."), None);
}

#[test]
fn a_specifier_pointing_at_its_own_directory_stays_inside_it() {
    assert_eq!(
        normalize_relative("src/ui/a.js", "../ui/b.js").as_deref(),
        Some("src/ui/b.js")
    );
}

#[test]
fn a_prototype_pollution_style_key_is_still_only_a_path() {
    // `__proto__` is a perfectly legal directory name; the guard that matters is
    // that it stays project-relative, not that the word is banned.
    assert!(is_safe_relative("src/__proto__/a.js"));
    assert_eq!(
        normalize_relative("src/a.js", "./__proto__/b.js").as_deref(),
        Some("src/__proto__/b.js")
    );
}

#[test]
fn a_hostile_timings_key_never_becomes_a_read() {
    let document = r#"{"version": 1, "files": {
        "../../../../etc/passwd": 1,
        "/etc/shadow": 2,
        "C:/windows/system32/config/sam": 3,
        "src/a.test.js": 4
    }}"#;
    let (timings, audit) =
        crate::TestTimings::from_json(camino::Utf8Path::new(".uf/test-timings.json"), document)
            .unwrap();

    assert_eq!(timings.len(), 1);
    assert_eq!(timings.get("src/a.test.js"), Some(4));
    assert_eq!(audit.rejected_paths, 3);
}

#[test]
fn a_watcher_refuses_to_stat_outside_its_root() {
    let dir = tempfile::tempdir().unwrap();
    let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    let mut watcher = crate::Watcher::new(&root, crate::WatchOptions::default());

    watcher.prime(["../../../../etc/passwd", "/etc/passwd"]);
    assert!(watcher.is_empty());
    assert!(watcher.poll(["../../../../etc/passwd"]).is_empty());
}

#[test]
fn a_deeply_nested_source_does_not_overflow_the_stack() {
    // The scanners walk delimiters iteratively; a generated file of ten
    // thousand nested describes must not blow the stack.
    let mut source = String::new();
    for index in 0..10_000 {
        source.push_str("describe('d");
        source.push_str(&index.to_string());
        source.push_str("', () => {");
    }
    source.push_str("it('leaf', () => {});");
    for _ in 0..10_000 {
        source.push_str("});");
    }

    let plan = crate::discover_tests("deep.test.js", &source);
    assert!(plan.runnable_count() >= 1);
}

#[test]
fn a_source_of_only_open_delimiters_terminates() {
    let source = "it('a', () => {".repeat(10_000);
    let plan = crate::discover_tests("a.test.js", &source);
    assert!(plan.runnable_count() <= 10_000);
}

#[test]
fn a_source_of_only_quotes_terminates() {
    let source = "\"".repeat(100_000);
    let plan = crate::discover_tests("a.test.js", &source);
    assert!(plan.is_empty());
}

#[test]
fn a_pathological_nested_call_terminates() {
    let mut source = String::from("it('a', () => { expect(");
    source.push_str(&"(".repeat(5_000));
    source.push_str("); });");

    let plan = crate::discover_tests("a.test.js", &source);
    assert_eq!(plan.runnable_count(), 1);
}
