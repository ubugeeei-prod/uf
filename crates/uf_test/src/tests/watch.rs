//! Noticing a change, and turning it into a rerun set.

use std::time::Duration;

use camino::Utf8PathBuf;

use crate::{
    ChangeSet, DEFAULT_POLL_INTERVAL, ImportGraph, MAX_POLL_INTERVAL, MIN_POLL_INTERVAL,
    WatchOptions, Watcher,
};

struct Project {
    _dir: tempfile::TempDir,
    root: Utf8PathBuf,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        Self { _dir: dir, root }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, contents).unwrap();
    }

    fn remove(&self, relative: &str) {
        std::fs::remove_file(self.root.join(relative)).unwrap();
    }

    fn watcher(&self) -> Watcher {
        Watcher::new(&self.root, WatchOptions::default())
    }
}

#[test]
fn the_poll_interval_is_clamped_at_both_ends() {
    assert_eq!(
        WatchOptions::with_interval(Duration::from_micros(1)).interval(),
        MIN_POLL_INTERVAL
    );
    assert_eq!(
        WatchOptions::with_interval(Duration::from_secs(60 * 60)).interval(),
        MAX_POLL_INTERVAL
    );
    assert_eq!(WatchOptions::default().interval(), DEFAULT_POLL_INTERVAL);
}

#[test]
fn a_watcher_reports_its_clamped_interval() {
    let project = Project::new();
    let watcher = Watcher::new(&project.root, WatchOptions::with_interval(Duration::ZERO));
    assert_eq!(watcher.interval(), MIN_POLL_INTERVAL);
}

#[test]
fn priming_reports_nothing() {
    let project = Project::new();
    project.write("src/a.js", "export const a = 1;\n");

    let mut watcher = project.watcher();
    watcher.prime(["src/a.js"]);

    assert_eq!(watcher.len(), 1);
    assert!(!watcher.is_empty());
    assert!(watcher.poll(["src/a.js"]).is_empty());
}

#[test]
fn a_first_poll_without_priming_reports_every_file_as_added() {
    let project = Project::new();
    project.write("src/a.js", "a\n");
    project.write("src/b.js", "b\n");

    let mut watcher = project.watcher();
    let changes = watcher.poll(["src/a.js", "src/b.js"]);

    similar_asserts::assert_eq!(
        changes.added.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
        vec!["src/a.js", "src/b.js"]
    );
}

#[test]
fn a_content_change_is_noticed() {
    let project = Project::new();
    project.write("src/a.js", "export const a = 1;\n");
    let mut watcher = project.watcher();
    watcher.prime(["src/a.js"]);

    project.write("src/a.js", "export const a = 2; // longer now\n");
    let changes = watcher.poll(["src/a.js"]);

    similar_asserts::assert_eq!(
        changes
            .modified
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>(),
        vec!["src/a.js"]
    );
}

#[test]
fn a_new_file_is_reported_as_added() {
    let project = Project::new();
    project.write("src/a.js", "a\n");
    let mut watcher = project.watcher();
    watcher.prime(["src/a.js"]);

    project.write("src/b.js", "b\n");
    let changes = watcher.poll(["src/a.js", "src/b.js"]);

    similar_asserts::assert_eq!(
        changes.added.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
        vec!["src/b.js"]
    );
    assert!(changes.modified.is_empty());
}

#[test]
fn a_deleted_file_is_reported_as_removed() {
    let project = Project::new();
    project.write("src/a.js", "a\n");
    project.write("src/b.js", "b\n");
    let mut watcher = project.watcher();
    watcher.prime(["src/a.js", "src/b.js"]);

    project.remove("src/b.js");
    let changes = watcher.poll(["src/a.js", "src/b.js"]);

    similar_asserts::assert_eq!(
        changes
            .removed
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>(),
        vec!["src/b.js"]
    );
}

#[test]
fn a_file_dropped_from_the_watched_list_is_reported_as_removed() {
    let project = Project::new();
    project.write("src/a.js", "a\n");
    project.write("src/b.js", "b\n");
    let mut watcher = project.watcher();
    watcher.prime(["src/a.js", "src/b.js"]);

    let changes = watcher.poll(["src/a.js"]);
    assert_eq!(changes.removed.len(), 1);
    assert_eq!(watcher.len(), 1);
}

#[test]
fn a_second_poll_after_a_change_reports_nothing_new() {
    let project = Project::new();
    project.write("src/a.js", "a\n");
    let mut watcher = project.watcher();
    watcher.prime(["src/a.js"]);

    project.write("src/a.js", "a much longer body\n");
    assert!(!watcher.poll(["src/a.js"]).is_empty());
    assert!(watcher.poll(["src/a.js"]).is_empty());
}

#[test]
fn a_missing_file_that_was_never_seen_is_not_a_change() {
    let project = Project::new();
    let mut watcher = project.watcher();

    assert!(watcher.poll(["src/never.js"]).is_empty());
}

#[test]
fn a_directory_is_not_watched_as_a_file() {
    let project = Project::new();
    project.write("src/a.js", "a\n");
    let mut watcher = project.watcher();

    assert!(watcher.poll(["src"]).is_empty());
}

#[test]
fn a_change_set_lists_every_moved_path_once_and_sorted() {
    let changes = ChangeSet {
        added: vec!["b.js".into()],
        modified: vec!["a.js".into()],
        removed: vec!["c.js".into()],
    };

    assert_eq!(changes.len(), 3);
    similar_asserts::assert_eq!(
        changes
            .paths()
            .iter()
            .map(|p| p.as_str())
            .collect::<Vec<_>>(),
        vec!["a.js", "b.js", "c.js"]
    );
}

#[test]
fn an_empty_change_set_is_empty() {
    let changes = ChangeSet::default();
    assert!(changes.is_empty());
    assert_eq!(changes.len(), 0);
    assert!(changes.paths().is_empty());
}

#[test]
fn an_unrelated_edit_reruns_nothing_end_to_end() {
    let project = Project::new();
    project.write("src/shared.js", "export const s = 1;\n");
    project.write("src/lonely.js", "export const l = 1;\n");
    project.write(
        "src/a.test.js",
        "import { s } from './shared.js';\nit('a', () => {});\n",
    );
    project.write(
        "src/b.test.js",
        "import { s } from './shared.js';\nit('b', () => {});\n",
    );

    let files = [
        "src/shared.js",
        "src/lonely.js",
        "src/a.test.js",
        "src/b.test.js",
    ];
    let sources: Vec<(String, String)> = files
        .iter()
        .map(|file| {
            (
                (*file).to_string(),
                std::fs::read_to_string(project.root.join(file)).unwrap(),
            )
        })
        .collect();
    let graph = ImportGraph::build(
        sources
            .iter()
            .map(|(file, source)| (file.as_str(), source.as_str())),
    );

    let mut watcher = project.watcher();
    watcher.prime(files);

    project.write("src/lonely.js", "export const l = 2; // edited\n");
    let changes = watcher.poll(files);
    let changed: Vec<String> = changes.paths().iter().map(|p| p.to_string()).collect();
    let rerun = graph.affected_tests(changed.iter().map(String::as_str), |path| {
        path.ends_with(".test.js")
    });

    assert!(rerun.is_empty(), "unrelated edit reran {rerun:?}");
}

#[test]
fn a_shared_dependency_edit_reruns_both_dependents_end_to_end() {
    let project = Project::new();
    project.write("src/shared.js", "export const s = 1;\n");
    project.write("src/lonely.js", "export const l = 1;\n");
    project.write(
        "src/a.test.js",
        "import { s } from './shared.js';\nit('a', () => {});\n",
    );
    project.write(
        "src/b.test.js",
        "import { s } from './shared.js';\nit('b', () => {});\n",
    );

    let files = [
        "src/shared.js",
        "src/lonely.js",
        "src/a.test.js",
        "src/b.test.js",
    ];
    let sources: Vec<(String, String)> = files
        .iter()
        .map(|file| {
            (
                (*file).to_string(),
                std::fs::read_to_string(project.root.join(file)).unwrap(),
            )
        })
        .collect();
    let graph = ImportGraph::build(
        sources
            .iter()
            .map(|(file, source)| (file.as_str(), source.as_str())),
    );

    let mut watcher = project.watcher();
    watcher.prime(files);

    project.write("src/shared.js", "export const s = 2; // edited longer\n");
    let changes = watcher.poll(files);
    let changed: Vec<String> = changes.paths().iter().map(|p| p.to_string()).collect();
    let rerun = graph.affected_tests(changed.iter().map(String::as_str), |path| {
        path.ends_with(".test.js")
    });

    similar_asserts::assert_eq!(
        rerun.iter().map(|p| p.as_str()).collect::<Vec<_>>(),
        vec!["src/a.test.js", "src/b.test.js"]
    );
}

#[test]
fn the_next_poll_instant_is_one_interval_away() {
    let now = std::time::SystemTime::UNIX_EPOCH;
    let options = WatchOptions::with_interval(Duration::from_millis(500));
    assert_eq!(
        crate::next_poll_at(now, options),
        now + Duration::from_millis(500)
    );
}
