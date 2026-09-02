//! Reading a file anything on the machine could have written.

use camino::{Utf8Path, Utf8PathBuf};

use crate::{
    MAX_TIMING_ENTRIES, MAX_TIMING_MICROS, TIMINGS_VERSION, TestTimings, TimingsError,
    load_timings, save_timings, timings_path,
};

fn parse(text: &str) -> Result<(TestTimings, crate::TimingsAudit), TimingsError> {
    TestTimings::from_json(Utf8Path::new(".uf/test-timings.json"), text)
}

fn root(dir: &tempfile::TempDir) -> Utf8PathBuf {
    Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap()
}

#[test]
fn a_valid_document_is_read() {
    let (timings, audit) = parse(r#"{"version": 1, "files": {"src/a.test.js": 1234}}"#).unwrap();

    assert_eq!(timings.get("src/a.test.js"), Some(1234));
    assert!(audit.is_clean());
    assert_eq!(timings.len(), 1);
}

#[test]
fn an_unknown_version_is_rejected_whole() {
    let error = parse(r#"{"version": 99, "files": {}}"#).unwrap_err();
    assert!(matches!(
        error,
        TimingsError::UnsupportedVersion { found: 99, .. }
    ));
}

#[test]
fn a_document_that_is_not_json_is_rejected_whole() {
    let error = parse("not json at all").unwrap_err();
    assert!(matches!(error, TimingsError::Malformed { .. }));
}

#[test]
fn a_document_that_is_a_json_array_is_rejected_whole() {
    assert!(matches!(
        parse("[1, 2, 3]").unwrap_err(),
        TimingsError::Malformed { .. }
    ));
}

#[test]
fn a_document_without_a_version_is_rejected_whole() {
    assert!(matches!(
        parse(r#"{"files": {}}"#).unwrap_err(),
        TimingsError::Malformed { .. }
    ));
}

#[test]
fn a_document_without_files_is_an_empty_record() {
    let (timings, audit) = parse(r#"{"version": 1}"#).unwrap();
    assert!(timings.is_empty());
    assert!(audit.is_clean());
}

#[test]
fn a_negative_duration_is_dropped_rather_than_trusted() {
    let (timings, audit) = parse(r#"{"version": 1, "files": {"a.js": -5}}"#).unwrap();

    assert_eq!(timings.get("a.js"), None);
    assert_eq!(audit.rejected_durations, 1);
}

#[test]
fn a_fractional_duration_is_dropped() {
    let (timings, audit) = parse(r#"{"version": 1, "files": {"a.js": 1.5}}"#).unwrap();
    assert_eq!(timings.get("a.js"), None);
    assert_eq!(audit.rejected_durations, 1);
}

#[test]
fn a_string_duration_is_dropped() {
    let (timings, audit) = parse(r#"{"version": 1, "files": {"a.js": "fast"}}"#).unwrap();
    assert_eq!(timings.get("a.js"), None);
    assert_eq!(audit.rejected_durations, 1);
}

#[test]
fn a_null_duration_is_dropped() {
    let (_, audit) = parse(r#"{"version": 1, "files": {"a.js": null}}"#).unwrap();
    assert_eq!(audit.rejected_durations, 1);
}

#[test]
fn an_object_duration_is_dropped() {
    let (_, audit) = parse(r#"{"version": 1, "files": {"a.js": {"micros": 5}}}"#).unwrap();
    assert_eq!(audit.rejected_durations, 1);
}

#[test]
fn an_absurd_duration_is_dropped() {
    let text = format!(
        r#"{{"version": 1, "files": {{"a.js": {}}}}}"#,
        MAX_TIMING_MICROS + 1
    );
    let (timings, audit) = parse(&text).unwrap();

    assert_eq!(timings.get("a.js"), None);
    assert_eq!(audit.rejected_durations, 1);
}

#[test]
fn a_duration_exactly_at_the_limit_is_kept() {
    let text = format!(r#"{{"version": 1, "files": {{"a.js": {MAX_TIMING_MICROS}}}}}"#);
    let (timings, _) = parse(&text).unwrap();
    assert_eq!(timings.get("a.js"), Some(MAX_TIMING_MICROS));
}

#[test]
fn a_traversing_path_key_is_dropped() {
    let (timings, audit) =
        parse(r#"{"version": 1, "files": {"../../../../etc/passwd": 1}}"#).unwrap();

    assert!(timings.is_empty());
    assert_eq!(audit.rejected_paths, 1);
}

#[test]
fn an_absolute_path_key_is_dropped() {
    let (_, audit) = parse(r#"{"version": 1, "files": {"/etc/passwd": 1}}"#).unwrap();
    assert_eq!(audit.rejected_paths, 1);
}

#[test]
fn a_windows_drive_key_is_dropped() {
    let (_, audit) = parse(r#"{"version": 1, "files": {"C:/windows/system32": 1}}"#).unwrap();
    assert_eq!(audit.rejected_paths, 1);
}

#[test]
fn a_backslash_key_is_dropped() {
    let (_, audit) = parse(r#"{"version": 1, "files": {"src\\a.js": 1}}"#).unwrap();
    assert_eq!(audit.rejected_paths, 1);
}

#[test]
fn an_empty_key_is_dropped() {
    let (_, audit) = parse(r#"{"version": 1, "files": {"": 1}}"#).unwrap();
    assert_eq!(audit.rejected_paths, 1);
}

#[test]
fn a_dot_segment_key_is_dropped() {
    let (_, audit) = parse(r#"{"version": 1, "files": {"src/./a.js": 1}}"#).unwrap();
    assert_eq!(audit.rejected_paths, 1);
}

#[test]
fn a_double_slash_key_is_dropped() {
    let (_, audit) = parse(r#"{"version": 1, "files": {"src//a.js": 1}}"#).unwrap();
    assert_eq!(audit.rejected_paths, 1);
}

#[test]
fn one_bad_entry_does_not_lose_the_good_ones() {
    let (timings, audit) =
        parse(r#"{"version": 1, "files": {"a.js": 10, "../b.js": 20, "c.js": -1}}"#).unwrap();

    assert_eq!(timings.get("a.js"), Some(10));
    assert_eq!(timings.len(), 1);
    assert_eq!(audit.rejected(), 2);
    assert!(!audit.is_clean());
}

#[test]
fn too_many_entries_is_rejected_whole() {
    let mut text = String::from(r#"{"version": 1, "files": {"#);
    for index in 0..(MAX_TIMING_ENTRIES + 1) {
        if index > 0 {
            text.push(',');
        }
        text.push_str(&format!(r#""f{index}.js": 1"#));
    }
    text.push_str("}}");

    assert!(matches!(
        parse(&text).unwrap_err(),
        TimingsError::TooManyEntries { .. }
    ));
}

#[test]
fn recording_clamps_an_absurd_duration() {
    let mut timings = TestTimings::new();
    timings.record("a.js", u64::MAX);
    assert_eq!(timings.get("a.js"), Some(MAX_TIMING_MICROS));
}

#[test]
fn recording_refuses_an_unsafe_path() {
    let mut timings = TestTimings::new();
    timings.record("../escape.js", 5);
    assert!(timings.is_empty());
}

#[test]
fn recording_updates_an_existing_entry() {
    let mut timings = TestTimings::new();
    timings.record("a.js", 5);
    timings.record("a.js", 9);
    assert_eq!(timings.get("a.js"), Some(9));
    assert_eq!(timings.len(), 1);
}

#[test]
fn stale_entries_can_be_pruned() {
    let mut timings = TestTimings::new();
    timings.record("a.js", 1);
    timings.record("b.js", 2);
    timings.retain_files(|file| file == "a.js");

    assert_eq!(timings.len(), 1);
    assert_eq!(timings.get("b.js"), None);
}

#[test]
fn the_document_is_written_with_sorted_keys() {
    let mut timings = TestTimings::new();
    timings.record("c.js", 3);
    timings.record("a.js", 1);
    timings.record("b.js", 2);

    let json = timings.to_json();
    let a = json.find("a.js").unwrap();
    let b = json.find("b.js").unwrap();
    let c = json.find("c.js").unwrap();
    assert!(a < b && b < c, "keys must be sorted:\n{json}");
}

#[test]
fn writing_and_reading_a_document_round_trips() {
    let mut timings = TestTimings::new();
    timings.record("src/a.test.js", 1_234);
    timings.record("src/b.test.js", 5_678);

    let (back, audit) = parse(&timings.to_json()).unwrap();
    assert_eq!(back, timings);
    assert!(audit.is_clean());
}

#[test]
fn an_empty_document_round_trips() {
    let timings = TestTimings::new();
    let (back, _) = parse(&timings.to_json()).unwrap();
    assert!(back.is_empty());
}

#[test]
fn the_written_document_declares_the_current_version() {
    let json = TestTimings::new().to_json();
    assert!(json.contains(&format!("\"version\": {TIMINGS_VERSION}")));
}

#[test]
fn two_writes_of_one_record_are_byte_identical() {
    let mut timings = TestTimings::new();
    for index in 0..64 {
        timings.record(&format!("f{index}.js"), index as u64);
    }
    assert_eq!(timings.to_json(), timings.to_json());
}

#[test]
fn a_missing_file_is_a_cold_run_rather_than_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let (timings, audit) = load_timings(&root(&dir)).unwrap();

    assert!(timings.is_empty());
    assert!(audit.is_clean());
}

#[test]
fn saving_creates_the_cache_directory() {
    let dir = tempfile::tempdir().unwrap();
    let root = root(&dir);
    let mut timings = TestTimings::new();
    timings.record("src/a.test.js", 42);

    save_timings(&root, &timings).unwrap();
    assert!(timings_path(&root).exists());

    let (loaded, audit) = load_timings(&root).unwrap();
    assert_eq!(loaded.get("src/a.test.js"), Some(42));
    assert!(audit.is_clean());
}

#[test]
fn saving_twice_overwrites_rather_than_appending() {
    let dir = tempfile::tempdir().unwrap();
    let root = root(&dir);

    let mut first = TestTimings::new();
    first.record("a.js", 1);
    save_timings(&root, &first).unwrap();

    let mut second = TestTimings::new();
    second.record("b.js", 2);
    save_timings(&root, &second).unwrap();

    let (loaded, _) = load_timings(&root).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded.get("b.js"), Some(2));
}

#[test]
fn a_corrupt_file_on_disk_is_an_error_the_caller_can_fall_back_from() {
    let dir = tempfile::tempdir().unwrap();
    let root = root(&dir);
    std::fs::create_dir_all(root.join(crate::CACHE_DIRECTORY)).unwrap();
    std::fs::write(timings_path(&root), "{ this is not json").unwrap();

    assert!(matches!(
        load_timings(&root).unwrap_err(),
        TimingsError::Malformed { .. }
    ));
}

#[test]
fn the_timings_path_lives_under_the_cache_directory() {
    let path = timings_path(Utf8Path::new("/project"));
    assert!(path.as_str().ends_with(".uf/test-timings.json"));
}

#[test]
fn an_audit_counts_both_rejection_classes() {
    let audit = crate::TimingsAudit {
        rejected_paths: 2,
        rejected_durations: 3,
    };
    assert_eq!(audit.rejected(), 5);
    assert!(!audit.is_clean());
}
