//! The poll watcher: what it sees, what it refuses to see, and its bounds.

use super::*;

struct Project {
    _dir: tempfile::TempDir,
    root: Utf8PathBuf,
}

impl Project {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let root = Utf8PathBuf::from_path_buf(dir.path().canonicalize().expect("canonical"))
            .expect("utf-8 temp dir");
        Self { _dir: dir, root }
    }

    fn write(&self, relative: &str, contents: &str) {
        let path = self.root.join(relative);
        std::fs::create_dir_all(path.parent().expect("a parent")).expect("directories");
        std::fs::write(&path, contents).expect("write");
    }

    fn remove(&self, relative: &str) {
        std::fs::remove_file(self.root.join(relative)).expect("remove");
    }

    fn watcher(&self) -> PollWatcher {
        PollWatcher::with_default_interval(&self.root)
    }
}

/// Force a stamp change even when the filesystem's mtime resolution is coarse:
/// the length is part of the stamp, so a different body is always visible.
fn rewrite(project: &Project, relative: &str, extra: &str) {
    project.write(relative, &format!("export const a = 1;\n{extra}"));
}

fn changed_paths(changes: &[FileChange]) -> Vec<&str> {
    changes.iter().map(|change| change.path.as_str()).collect()
}

#[test]
fn the_first_poll_seeds_without_reporting_the_whole_project() {
    let project = Project::new();
    project.write("app/page.js", "export const a = 1;\n");
    let mut watcher = project.watcher();

    assert!(!watcher.is_seeded());
    let changes = watcher.poll().expect("polls");

    assert!(changes.is_empty());
    assert!(watcher.is_seeded());
    assert_eq!(watcher.tracked(), 1);
}

#[test]
fn a_rewritten_file_is_reported_as_modified() {
    let project = Project::new();
    project.write("app/page.js", "export const a = 1;\n");
    let mut watcher = project.watcher();
    watcher.poll().expect("seeds");

    rewrite(&project, "app/page.js", "export const b = 2;\n");
    let changes = watcher.poll().expect("polls");

    assert_eq!(changed_paths(&changes), ["app/page.js"]);
    assert_eq!(changes[0].change, ChangeKind::Modified);
}

#[test]
fn a_new_file_is_reported_as_created() {
    let project = Project::new();
    project.write("app/page.js", "export const a = 1;\n");
    let mut watcher = project.watcher();
    watcher.poll().expect("seeds");

    project.write("app/Counter.js", "export const b = 2;\n");
    let changes = watcher.poll().expect("polls");

    assert_eq!(changed_paths(&changes), ["app/Counter.js"]);
    assert_eq!(changes[0].change, ChangeKind::Created);
}

#[test]
fn a_removed_file_is_reported_as_deleted() {
    let project = Project::new();
    project.write("app/page.js", "export const a = 1;\n");
    let mut watcher = project.watcher();
    watcher.poll().expect("seeds");

    project.remove("app/page.js");
    let changes = watcher.poll().expect("polls");

    assert_eq!(changed_paths(&changes), ["app/page.js"]);
    assert_eq!(changes[0].change, ChangeKind::Deleted);
}

#[test]
fn an_unchanged_tree_reports_nothing() {
    let project = Project::new();
    project.write("app/page.js", "export const a = 1;\n");
    let mut watcher = project.watcher();
    watcher.poll().expect("seeds");

    for _ in 0..4 {
        assert!(watcher.poll().expect("polls").is_empty());
    }
}

#[test]
fn changes_are_reported_in_a_deterministic_order() {
    let project = Project::new();
    project.write("app/a.js", "export const a = 1;\n");
    let mut watcher = project.watcher();
    watcher.poll().expect("seeds");

    project.write("app/z.js", "export const z = 1;\n");
    project.write("app/b.js", "export const b = 1;\n");
    project.write("app/m.js", "export const m = 1;\n");
    let changes = watcher.poll().expect("polls");

    assert_eq!(
        changed_paths(&changes),
        ["app/b.js", "app/m.js", "app/z.js"]
    );
}

#[test]
fn only_js_files_are_watched() {
    let project = Project::new();
    project.write("app/page.js", "export const a = 1;\n");
    project.write("app/styles.css", "body {}\n");
    project.write("package.json", "{}\n");
    project.write("README.md", "# hi\n");
    let mut watcher = project.watcher();

    watcher.poll().expect("seeds");

    assert_eq!(watcher.tracked(), 1);
}

#[test]
fn skipped_directories_are_never_descended_into() {
    let project = Project::new();
    project.write("app/page.js", "export const a = 1;\n");
    for directory in SKIPPED_DIRECTORIES {
        project.write(&format!("{directory}/inner.js"), "export const x = 1;\n");
    }
    let mut watcher = project.watcher();

    watcher.poll().expect("seeds");

    assert_eq!(watcher.tracked(), 1);
}

#[test]
fn dot_directories_are_never_descended_into() {
    let project = Project::new();
    project.write("app/page.js", "export const a = 1;\n");
    project.write(".hidden/secret.js", "export const x = 1;\n");
    let mut watcher = project.watcher();

    watcher.poll().expect("seeds");

    assert_eq!(watcher.tracked(), 1);
}

#[test]
fn a_denied_path_is_never_watched() {
    let project = Project::new();
    project.write("app/page.js", "export const a = 1;\n");
    project.write("secrets/private.key.js", "export const x = 1;\n");
    project.write("app/vendor.pem.js", "export const y = 1;\n");
    let policy = crate::policy::FsPolicy::new(
        &project.root,
        Vec::<&str>::new(),
        vec!["*.key.js", "*.pem.js"],
    )
    .expect("policy");
    let mut watcher = PollWatcher::with_default_interval(&project.root).with_policy(policy);

    watcher.poll().expect("seeds");

    assert_eq!(watcher.tracked(), 1);
}

#[cfg(unix)]
#[test]
fn a_symlink_is_never_followed_or_watched() {
    let project = Project::new();
    project.write("app/page.js", "export const a = 1;\n");
    std::os::unix::fs::symlink("/etc", project.root.join("etc")).expect("symlink");
    std::os::unix::fs::symlink("app/page.js", project.root.join("alias.js")).expect("symlink");
    let mut watcher = project.watcher();

    watcher.poll().expect("seeds");

    assert_eq!(watcher.tracked(), 1);
}

#[test]
fn a_nested_tree_is_walked_without_recursion() {
    let project = Project::new();
    let mut path = String::new();
    for depth in 0..(MAX_WATCH_DEPTH - 1) {
        path.push_str(&format!("d{depth}/"));
        project.write(&format!("{path}m.js"), "export const a = 1;\n");
    }
    let mut watcher = project.watcher();

    watcher.poll().expect("polls a deep tree");

    assert_eq!(watcher.tracked(), MAX_WATCH_DEPTH - 1);
}

#[test]
fn a_tree_deeper_than_the_bound_is_a_typed_error() {
    let project = Project::new();
    let mut path = String::new();
    for depth in 0..=(MAX_WATCH_DEPTH + 1) {
        path.push_str(&format!("d{depth}/"));
    }
    project.write(&format!("{path}m.js"), "export const a = 1;\n");
    let mut watcher = project.watcher();

    assert!(matches!(
        watcher.poll().unwrap_err(),
        WatchError::TooDeep { .. }
    ));
}

#[test]
fn a_missing_root_is_a_typed_io_error() {
    let mut watcher = PollWatcher::with_default_interval(Utf8Path::new(
        "/definitely/not/a/directory/on/this/machine",
    ));

    assert!(matches!(watcher.poll().unwrap_err(), WatchError::Io { .. }));
}

#[test]
fn the_poll_interval_is_clamped_into_the_accepted_range() {
    let root = Utf8Path::new(".");

    assert_eq!(
        PollWatcher::new(root, Duration::from_nanos(1)).interval(),
        MIN_POLL_INTERVAL
    );
    assert_eq!(
        PollWatcher::new(root, Duration::from_secs(3_600)).interval(),
        MAX_POLL_INTERVAL
    );
    assert_eq!(
        PollWatcher::new(root, Duration::from_millis(250)).interval(),
        Duration::from_millis(250)
    );
}

#[test]
fn the_default_interval_sits_inside_the_accepted_range() {
    assert!(MIN_POLL_INTERVAL <= DEFAULT_POLL_INTERVAL);
    assert!(DEFAULT_POLL_INTERVAL <= MAX_POLL_INTERVAL);
    assert_eq!(
        PollWatcher::with_default_interval(Utf8Path::new(".")).interval(),
        DEFAULT_POLL_INTERVAL
    );
}

#[test]
fn the_watcher_reports_the_root_it_was_built_with() {
    let project = Project::new();

    assert_eq!(project.watcher().root(), project.root);
}

#[test]
fn watched_files_lists_the_tree_in_sorted_order() {
    let project = Project::new();
    project.write("app/z.js", "export const z = 1;\n");
    project.write("app/a.js", "export const a = 1;\n");
    project.write("lib/m.js", "export const m = 1;\n");
    let watcher = project.watcher();

    let files = watched_files(&watcher).expect("lists");

    assert_eq!(
        files.iter().map(|path| path.as_str()).collect::<Vec<_>>(),
        ["app/a.js", "app/z.js", "lib/m.js"]
    );
}

#[test]
fn watched_files_does_not_seed_the_watcher() {
    let project = Project::new();
    project.write("app/a.js", "export const a = 1;\n");
    let watcher = project.watcher();

    watched_files(&watcher).expect("lists");

    assert!(!watcher.is_seeded());
    assert_eq!(watcher.tracked(), 0);
}

#[test]
fn an_empty_project_polls_cleanly() {
    let project = Project::new();
    let mut watcher = project.watcher();

    assert!(watcher.poll().expect("seeds").is_empty());
    assert!(watcher.poll().expect("polls").is_empty());
    assert_eq!(watcher.tracked(), 0);
}

#[test]
fn a_file_that_becomes_a_directory_is_reported_as_deleted() {
    let project = Project::new();
    project.write("app/page.js", "export const a = 1;\n");
    let mut watcher = project.watcher();
    watcher.poll().expect("seeds");

    project.remove("app/page.js");
    std::fs::create_dir(project.root.join("app/page.js")).expect("directory");
    let changes = watcher.poll().expect("polls");

    assert_eq!(changed_paths(&changes), ["app/page.js"]);
    assert_eq!(changes[0].change, ChangeKind::Deleted);
}

#[test]
fn a_created_and_deleted_file_in_one_interval_reports_both() {
    let project = Project::new();
    project.write("app/a.js", "export const a = 1;\n");
    let mut watcher = project.watcher();
    watcher.poll().expect("seeds");

    project.write("app/b.js", "export const b = 1;\n");
    project.remove("app/a.js");
    let changes = watcher.poll().expect("polls");

    assert_eq!(changed_paths(&changes), ["app/a.js", "app/b.js"]);
    assert_eq!(changes[0].change, ChangeKind::Deleted);
    assert_eq!(changes[1].change, ChangeKind::Created);
}

#[test]
fn a_non_ascii_file_name_is_watched() {
    let project = Project::new();
    project.write("app/café.js", "export const a = 1;\n");
    let mut watcher = project.watcher();

    watcher.poll().expect("seeds");

    assert_eq!(watcher.tracked(), 1);
}
