use camino::Utf8PathBuf;

use super::{cache_key, memoise};

fn temp() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    (dir, path)
}

#[test]
fn the_second_call_does_no_work() {
    let (_guard, dir) = temp();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("out.bin"), b"x").unwrap();
    let key = cache_key("test", &[b"source"]);

    let mut ran = 0;
    for expected_hit in [false, true, true] {
        let cached = memoise::<Vec<String>, ()>(
            &dir,
            &key,
            |value| value.clone(),
            || {
                ran += 1;
                Ok(vec![String::from("out.bin")])
            },
        )
        .unwrap();
        assert_eq!(cached.hit, expected_hit);
        assert_eq!(cached.value, vec![String::from("out.bin")]);
    }
    assert_eq!(ran, 1);
}

#[test]
fn a_manifest_whose_files_are_gone_is_a_miss() {
    // `rm -rf dist` with `.uf/cache` left behind is the ordinary case, and a
    // hit that named files nobody could serve would be a broken build.
    let (_guard, dir) = temp();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("out.bin"), b"x").unwrap();
    let key = cache_key("test", &[b"source"]);
    let produce = || -> Result<Vec<String>, ()> { Ok(vec![String::from("out.bin")]) };

    assert!(!memoise(&dir, &key, Clone::clone, produce).unwrap().hit);
    assert!(memoise(&dir, &key, Clone::clone, produce).unwrap().hit);
    std::fs::remove_file(dir.join("out.bin")).unwrap();
    assert!(!memoise(&dir, &key, Clone::clone, produce).unwrap().hit);
}

#[test]
fn every_input_is_in_the_key() {
    // The whole of the invalidation. A parameter missing from the key is a
    // parameter a rebuild silently ignores.
    let base = cache_key("image", &[b"bytes", b"640", b"75"]);
    assert_ne!(base, cache_key("image", &[b"other", b"640", b"75"]));
    assert_ne!(base, cache_key("image", &[b"bytes", b"320", b"75"]));
    assert_ne!(base, cache_key("image", &[b"bytes", b"640", b"90"]));
    assert_ne!(base, cache_key("font", &[b"bytes", b"640", b"75"]));
    assert_eq!(base, cache_key("image", &[b"bytes", b"640", b"75"]));
    // Length-framed, so two parameters cannot be run together into one.
    assert_ne!(
        cache_key("image", &[b"ab", b"c"]),
        cache_key("image", &[b"a", b"bc"])
    );
}

#[test]
fn a_cache_that_cannot_be_written_is_a_slower_build_and_not_a_failed_one() {
    // The output directory is a container layer, or a sandbox, or read-only in
    // CI. A build that produced the right files has succeeded.
    let (_guard, dir) = temp();
    let unwritable = dir.join("no").join("such").join("place");
    let cached = memoise::<u32, ()>(
        &unwritable,
        &cache_key("test", &[]),
        |_| Vec::new(),
        || Ok(7),
    )
    .unwrap();
    assert_eq!(cached.value, 7);
    assert!(!cached.hit);
}

#[test]
fn a_corrupt_entry_is_a_miss_rather_than_an_error() {
    let (_guard, dir) = temp();
    let key = cache_key("test", &[b"x"]);
    std::fs::create_dir_all(dir.join(".manifests")).unwrap();
    std::fs::write(dir.join(".manifests").join(format!("{key}.json")), b"{").unwrap();
    let cached = memoise::<u32, ()>(&dir, &key, |_| Vec::new(), || Ok(3)).unwrap();
    assert_eq!(cached.value, 3);
    assert!(!cached.hit);
}
