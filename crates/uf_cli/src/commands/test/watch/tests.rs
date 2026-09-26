//! Change detection and invalidation, without a file system or a clock.

use super::*;

fn file(path: &str, source: &str) -> ProjectFile {
    let absolute_path = camino::Utf8PathBuf::from(path);
    ProjectFile {
        kind: uf_project::SourceKind::from_path(&absolute_path)
            .unwrap_or(uf_project::SourceKind::JavaScript),
        absolute_path,
        relative_path: path.to_string(),
        source: source.to_string(),
    }
}

#[test]
fn an_unchanged_project_reports_no_moved_paths() {
    let before = vec![file("a.js", "1"), file("b.js", "2")];
    assert!(changed_paths(&before, &before.clone()).is_empty());
}

#[test]
fn a_content_change_is_a_moved_path() {
    let before = vec![file("a.js", "1")];
    let after = vec![file("a.js", "2")];
    assert_eq!(changed_paths(&before, &after), vec!["a.js".to_string()]);
}

#[test]
fn an_added_file_is_a_moved_path() {
    let before = vec![file("a.js", "1")];
    let after = vec![file("a.js", "1"), file("b.js", "2")];
    assert_eq!(changed_paths(&before, &after), vec!["b.js".to_string()]);
}

#[test]
fn a_removed_file_is_a_moved_path() {
    let before = vec![file("a.js", "1"), file("b.js", "2")];
    let after = vec![file("a.js", "1")];
    assert_eq!(changed_paths(&before, &after), vec!["b.js".to_string()]);
}

#[test]
fn a_file_added_before_every_other_is_a_moved_path() {
    let before = vec![file("b.js", "1")];
    let after = vec![file("a.js", "0"), file("b.js", "1")];
    assert_eq!(changed_paths(&before, &after), vec!["a.js".to_string()]);
}

#[test]
fn an_emptied_project_reports_every_file_as_moved() {
    let before = vec![file("a.js", "1"), file("b.js", "2")];
    assert_eq!(
        changed_paths(&before, &[]),
        vec!["a.js".to_string(), "b.js".to_string()]
    );
}

#[test]
fn a_removed_module_leaves_the_graph() {
    let files = vec![file("a.js", "export const a = 1;")];
    let mut graph = build_graph(&files);
    assert!(graph.contains("a.js"));

    refresh_graph(&mut graph, &[], &["a.js".to_string()]);
    assert!(!graph.contains("a.js"));
}

#[test]
fn a_changed_module_is_rescanned_into_the_graph() {
    let before = vec![
        file("shared.js", "export const s = 1;"),
        file("a.test.js", "it('a', () => {});"),
    ];
    let mut graph = build_graph(&before);
    assert!(
        graph
            .affected_tests(["shared.js"], |path| path.ends_with(".test.js"))
            .is_empty()
    );

    let after = vec![
        file("shared.js", "export const s = 1;"),
        file(
            "a.test.js",
            "import { s } from './shared.js';\nit('a', () => {});",
        ),
    ];
    refresh_graph(&mut graph, &after, &["a.test.js".to_string()]);
    assert_eq!(
        graph.affected_tests(["shared.js"], |path| path.ends_with(".test.js")),
        vec!["a.test.js".to_string()]
    );
}

#[test]
fn a_file_without_tests_is_never_in_the_rerun_set() {
    let files = vec![
        file("shared.js", "export const s = 1;"),
        file(
            "a.test.js",
            "import { s } from './shared.js';\nit('a', () => {});",
        ),
    ];
    let graph = build_graph(&files);
    let rerun = affected(
        &graph,
        &["shared.js".to_string()],
        &files,
        &TestFilter::new(),
    );

    assert_eq!(rerun, vec!["a.test.js".to_string()]);
}

#[test]
fn an_unrelated_change_produces_an_empty_rerun_set() {
    let files = vec![
        file("lonely.js", "export const l = 1;"),
        file("a.test.js", "it('a', () => {});"),
    ];
    let graph = build_graph(&files);

    assert!(
        affected(
            &graph,
            &["lonely.js".to_string()],
            &files,
            &TestFilter::new()
        )
        .is_empty()
    );
}

#[test]
fn the_path_filter_still_applies_in_watch_mode() {
    let files = vec![
        file("shared.js", "export const s = 1;"),
        file(
            "src/a.test.js",
            "import { s } from '../shared.js';\nit('a', () => {});",
        ),
        file(
            "other/b.test.js",
            "import { s } from '../shared.js';\nit('b', () => {});",
        ),
    ];
    let graph = build_graph(&files);
    let rerun = affected(
        &graph,
        &["shared.js".to_string()],
        &files,
        &TestFilter::new().with_path("src/"),
    );

    assert_eq!(rerun, vec!["src/a.test.js".to_string()]);
}

#[test]
fn a_burst_of_events_about_uf_s_own_files_calls_for_nothing() {
    // `uf test` writes `.uf/test-timings.json` after every run; an event
    // session that walked the project on its own writes would walk after every
    // run for nothing.
    let mut files = vec![file("a.js", "1")];
    let heard = [
        CompactString::from(".uf/test-timings.json"),
        CompactString::from("node_modules/react/index.js"),
        CompactString::from("a.js.swp"),
    ];
    assert_eq!(reread_heard(&mut files, &heard), Heard::Nothing);
}

#[test]
fn an_event_for_a_file_nobody_holds_that_could_be_source_calls_for_a_walk() {
    let mut files = vec![file("a.js", "1")];
    let heard = [CompactString::from("src/new.test.js")];
    assert_eq!(reread_heard(&mut files, &heard), Heard::Rescan);
}

#[test]
fn an_event_for_a_file_that_cannot_be_read_calls_for_a_walk() {
    // The file went between the event and the read: only the walk can say
    // whether it is gone or moved.
    let mut files = vec![file("/nonexistent/uf-watch/a.js", "1")];
    let heard = [CompactString::from("/nonexistent/uf-watch/a.js")];
    assert_eq!(reread_heard(&mut files, &heard), Heard::Rescan);
}

#[test]
fn an_event_for_a_held_file_reads_it_again_and_reports_only_a_real_change() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let path = camino::Utf8PathBuf::from_path_buf(directory.path().join("a.js")).expect("utf-8");
    std::fs::write(&path, "same").expect("written");
    let mut files = vec![ProjectFile {
        kind: uf_project::SourceKind::JavaScript,
        absolute_path: path.clone(),
        relative_path: String::from("a.js"),
        source: String::from("same"),
    }];
    let heard = [CompactString::from("a.js")];

    // A save of the same bytes is an event and not a change.
    assert_eq!(reread_heard(&mut files, &heard), Heard::Read(Vec::new()));

    std::fs::write(&path, "edited").expect("written");
    assert_eq!(
        reread_heard(&mut files, &heard),
        Heard::Read(vec![String::from("a.js")])
    );
    assert_eq!(files[0].source, "edited");
}

#[test]
fn only_what_could_be_a_source_file_is_worth_a_walk() {
    assert!(might_be_source("src/a.js"));
    assert!(might_be_source("package.json"));
    assert!(!might_be_source("src/.a.js.swp"));
    assert!(!might_be_source("README"));
    assert!(never_watched(".uf/test-timings.json"));
    assert!(never_watched("npm/ui/node_modules/x.js"));
    assert!(!never_watched("src/uf.js"));
}
