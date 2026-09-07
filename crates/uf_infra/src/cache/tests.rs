//! What the bound promises, and the three ways it could quietly not.
//!
//! Every test here writes a real directory and takes real timestamps back off
//! it. The timestamps are *set* rather than waited for: a sweep is about days
//! of accumulated entries, and a test that slept long enough to tell two of
//! them apart would be a test nobody runs.

use std::fs::{self, File, FileTimes};
use std::io::Write as _;
use std::time::{Duration, SystemTime};

use super::{CacheBound, Swept, sweep};

/// A bound with no grace period, for the tests that are about eviction rather
/// than about concurrency. The grace period has tests of its own below.
fn bound(max_bytes: u64, target_bytes: u64) -> CacheBound {
    CacheBound {
        max_bytes,
        target_bytes,
        grace: Duration::ZERO,
    }
}

/// How long ago, as a time.
fn ago(seconds: u64) -> SystemTime {
    SystemTime::now()
        .checked_sub(Duration::from_secs(seconds))
        .expect("a time that far back exists")
}

/// Write `bytes` bytes to `name` and say it was last used `used` seconds ago.
fn entry(directory: &std::path::Path, name: &str, bytes: usize, used: u64) {
    let path = directory.join(name);
    let mut file = File::create(&path).unwrap();
    file.write_all(&vec![b'x'; bytes]).unwrap();
    // Both times, so a filesystem that reports either one gives the same
    // answer: `used_at` takes the later of the two.
    let when = ago(used);
    file.set_times(FileTimes::new().set_accessed(when).set_modified(when))
        .unwrap();
}

/// The entry names still in `directory`, sorted.
fn remaining(directory: &std::path::Path) -> Vec<String> {
    let mut names = fs::read_dir(directory)
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    names.sort();
    names
}

#[test]
fn a_directory_that_does_not_exist_is_not_an_error() {
    let root = tempfile::tempdir().unwrap();
    let swept = sweep(&root.path().join("never-built"), CacheBound::default());
    assert_eq!(swept, Swept::default());
}

#[test]
fn a_directory_under_the_cap_is_left_exactly_as_it_was() {
    let root = tempfile::tempdir().unwrap();
    for (age, name) in [(0, "a"), (1000, "b"), (2000, "c")] {
        entry(root.path(), name, 100, age);
    }

    let swept = sweep(root.path(), bound(1000, 500));

    // The common path: nothing is removed, and the directory is reported at
    // its real size so a caller can say what it found.
    assert_eq!(swept.removed, 0);
    assert_eq!(swept.kept, 3);
    assert_eq!(swept.bytes_before, 300);
    assert_eq!(swept.bytes_after, 300);
    assert_eq!(remaining(root.path()), ["a", "b", "c"]);
}

#[test]
fn a_directory_over_the_cap_comes_back_to_the_target() {
    let root = tempfile::tempdir().unwrap();
    for index in 0..10u64 {
        entry(root.path(), &format!("e{index}"), 100, index * 1000);
    }

    // 1000 bytes held, 500 allowed, back to 300.
    let swept = sweep(root.path(), bound(500, 300));

    assert_eq!(swept.bytes_before, 1000);
    assert!(
        swept.bytes_after <= 300,
        "swept to {} bytes, which is over the target",
        swept.bytes_after
    );
    assert_eq!(swept.removed, 7);
    assert_eq!(swept.kept, 3);
}

#[test]
fn the_coldest_entries_are_the_ones_that_go() {
    let root = tempfile::tempdir().unwrap();
    // Named for their age so the assertion reads as the policy does.
    entry(root.path(), "yesterday", 100, 86_400);
    entry(root.path(), "an-hour-ago", 100, 3_600);
    entry(root.path(), "a-minute-ago", 100, 90);

    // 300 held, 250 allowed, and 200 is enough — so exactly one entry has to
    // go and the test is about *which* one.
    let swept = sweep(root.path(), bound(250, 200));

    assert_eq!(swept.removed, 1);
    // The oldest one, and only it. This is the whole reason the policy is
    // least-recently-used rather than "whatever `read_dir` hands back first",
    // which on most filesystems is an order nobody can predict.
    assert_eq!(remaining(root.path()), ["a-minute-ago", "an-hour-ago"]);
}

#[test]
fn an_entry_read_recently_outlives_one_written_after_it() {
    let root = tempfile::tempdir().unwrap();
    let old_but_read = root.path().join("old-but-read");
    let newer_but_cold = root.path().join("newer-but-cold");

    let mut file = File::create(&old_but_read).unwrap();
    file.write_all(&[b'x'; 100]).unwrap();
    // Written a week ago, read a minute ago: a module that has not changed in
    // a week and was compiled from cache on the last build. It is the entry
    // the next build wants most and the one an eviction by age alone would
    // take first.
    file.set_times(
        FileTimes::new()
            .set_modified(ago(604_800))
            .set_accessed(ago(60)),
    )
    .unwrap();

    entry(root.path(), "newer-but-cold", 100, 3_600);
    assert!(old_but_read.exists() && newer_but_cold.exists());

    sweep(root.path(), bound(150, 100));

    assert_eq!(remaining(root.path()), ["old-but-read"]);
}

#[test]
fn a_file_used_within_the_grace_period_is_never_removed() {
    let root = tempfile::tempdir().unwrap();
    // Four entries, all of them written moments ago — which is what a
    // temporary file another process is about to rename into place looks like,
    // and what this run's own writes look like.
    for index in 0..4u64 {
        entry(root.path(), &format!("e{index}"), 100, 0);
    }

    let swept = sweep(
        root.path(),
        CacheBound {
            max_bytes: 100,
            target_bytes: 50,
            grace: Duration::from_secs(60),
        },
    );

    // Over budget and nothing taken. A cache briefly above its cap is the
    // price of never unlinking a file a concurrent run is in the middle of;
    // the next sweep, a minute later, brings it down.
    assert_eq!(swept.removed, 0);
    assert_eq!(swept.bytes_after, 400);
    assert_eq!(remaining(root.path()).len(), 4);
}

#[test]
fn a_subdirectory_is_not_swept_and_is_not_counted() {
    let root = tempfile::tempdir().unwrap();
    // `.uf/cache/task/last/` is this, and losing it costs `uf run --why` the
    // ability to say which file changed.
    let notes = root.path().join("last");
    fs::create_dir(&notes).unwrap();
    entry(&notes, "note", 10_000, 86_400);
    entry(root.path(), "cold", 100, 86_400);
    entry(root.path(), "warm", 100, 10);

    let swept = sweep(root.path(), bound(150, 100));

    // The notes are neither counted towards the budget nor eligible for it.
    assert_eq!(swept.bytes_before, 200);
    assert_eq!(remaining(root.path()), ["last", "warm"]);
    assert_eq!(remaining(&notes), ["note"]);
}

#[cfg(unix)]
#[test]
fn a_symlink_is_not_followed() {
    let root = tempfile::tempdir().unwrap();
    let outside = root.path().join("outside.txt");
    fs::write(&outside, "not the cache's to delete").unwrap();

    let cache = root.path().join("cache");
    fs::create_dir(&cache).unwrap();
    entry(&cache, "cold", 100, 86_400);
    std::os::unix::fs::symlink(&outside, cache.join("link")).unwrap();

    let swept = sweep(&cache, bound(1, 0));

    // The link is not a file as far as the sweep is concerned, so it is
    // neither counted nor removed — and, the part that matters, the file it
    // points at is untouched. A cache that could be made to delete an
    // arbitrary path by planting a link in it would be a far worse bug than
    // the one this module fixes.
    assert_eq!(swept.bytes_before, 100);
    assert_eq!(swept.removed, 1);
    assert!(outside.exists());
    assert_eq!(remaining(&cache), ["link"]);
}

#[test]
fn a_file_stamped_in_the_future_is_protected_rather_than_evicted_first() {
    let root = tempfile::tempdir().unwrap();
    let ahead = root.path().join("ahead");
    let mut file = File::create(&ahead).unwrap();
    file.write_all(&[b'x'; 100]).unwrap();
    // A clock that moved, or a tree copied with its timestamps. `used_at` is
    // then not a duration in the past at all, and the arithmetic that answers
    // "how old is this" fails rather than returning something enormous.
    let later = SystemTime::now()
        .checked_add(Duration::from_secs(86_400))
        .unwrap();
    file.set_times(FileTimes::new().set_accessed(later).set_modified(later))
        .unwrap();

    entry(root.path(), "ordinary", 100, 86_400);

    sweep(root.path(), bound(150, 100));

    // Protected, not taken first: a timestamp nobody can reason about is not
    // permission to delete the file it is on.
    assert_eq!(remaining(root.path()), ["ahead"]);
}
