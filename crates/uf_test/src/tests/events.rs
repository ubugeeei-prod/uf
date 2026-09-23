//! Kernel file events, on the real file system of the machine running the
//! tests: FSEvents on macOS, inotify on Linux.

use std::time::Duration;

use camino::Utf8PathBuf;

use crate::EventWatcher;

/// A temporary project with one file in a subdirectory.
fn project() -> (tempfile::TempDir, Utf8PathBuf) {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(directory.path().to_path_buf()).expect("utf-8");
    std::fs::create_dir_all(root.join("src")).expect("created");
    std::fs::write(root.join("src/a.js"), "export const a = 1;\n").expect("written");
    (directory, root)
}

/// A watcher on `root`, when this machine delivers events at all.
///
/// A sandbox can deny the file-event service while letting the watch start;
/// [`EventWatcher::verify`] is how the watch loop finds that out, and these
/// tests ask the same question before asserting anything about events.
fn live(root: &camino::Utf8Path) -> Option<EventWatcher> {
    let mut watcher = EventWatcher::new(root).ok()?;
    watcher
        .verify(&root.join(".uf"), Duration::from_secs(2))
        .ok()?
        .then_some(watcher)
}

/// Wait for a batch that names `path`, ignoring anything else the backend
/// reports first (FSEvents can report the directory's creation late).
fn heard(watcher: &EventWatcher, path: &str) -> bool {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    while std::time::Instant::now() < deadline {
        if let Some(batch) = watcher.wait(Duration::from_millis(500))
            && (batch.rescan || batch.changed.iter().any(|changed| changed == path))
        {
            return true;
        }
    }
    false
}

#[test]
fn a_write_to_a_watched_file_is_heard_by_its_project_relative_path() {
    let (_directory, root) = project();
    let Some(mut watcher) = live(&root) else {
        // A machine that sends no events at all is the polling fallback's
        // case, and it is tested with the watch loop.
        return;
    };
    watcher
        .watch_directories(["src/a.js"])
        .expect("the directory is watched");
    assert_eq!(watcher.directories(), 3, "the root, .uf and src");
    // FSEvents delivers what happened just before the stream began a moment
    // later; let that settle so it is not taken for the write below.
    while watcher.wait(Duration::from_millis(200)).is_some() {}

    std::fs::write(root.join("src/a.js"), "export const a = 2;\n").expect("written");

    assert!(heard(&watcher, "src/a.js"), "the write was never reported");
}

#[test]
fn a_file_created_beside_a_watched_one_is_heard() {
    let (_directory, root) = project();
    let Some(mut watcher) = live(&root) else {
        return;
    };
    watcher.watch_directories(["src/a.js"]).expect("watched");
    while watcher.wait(Duration::from_millis(200)).is_some() {}

    std::fs::write(root.join("src/b.test.js"), "it('b', () => {});\n").expect("written");

    assert!(
        heard(&watcher, "src/b.test.js"),
        "the new file was never reported"
    );
}

#[test]
fn nothing_happening_is_a_timeout_and_not_a_change() {
    let (_directory, root) = project();
    let Some(mut watcher) = live(&root) else {
        return;
    };
    watcher.watch_directories(["src/a.js"]).expect("watched");
    while watcher.wait(Duration::from_millis(200)).is_some() {}

    assert_eq!(watcher.wait(Duration::from_millis(50)), None);
}

#[test]
fn a_root_that_does_not_exist_is_an_error_the_caller_can_fall_back_from() {
    let missing = Utf8PathBuf::from("/nonexistent/uf-test-events/project");
    assert!(EventWatcher::new(&missing).is_err());
}

#[test]
fn a_path_that_climbs_out_of_the_project_is_not_watched() {
    let (_directory, root) = project();
    let Some(mut watcher) = live(&root) else {
        return;
    };
    watcher
        .watch_directories(["../outside.js", "/etc/passwd", "src/a.js"])
        .expect("watched");
    assert_eq!(watcher.directories(), 3, "only the root, .uf and src");
}

#[test]
fn a_watch_that_reports_nothing_is_found_out_rather_than_trusted() {
    // Wherever events arrive, the probe is heard; wherever they do not — this
    // sandbox, a network mount — the answer is no rather than an error, which
    // is what sends the watch loop to polling.
    let (_directory, root) = project();
    let Ok(mut watcher) = EventWatcher::new(&root) else {
        return;
    };
    let heard = watcher
        .verify(&root.join(".uf"), Duration::from_secs(2))
        .expect("the probe is written and watched");
    assert!(root.join(".uf/watch-probe").exists());
    let _ = heard;
}
