use camino::Utf8PathBuf;

use super::*;

fn temp() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    (dir, path)
}

/// A project that has never resolved a prefix locks nothing, and that is not an
/// error.
#[test]
fn a_missing_lock_locks_nothing() {
    let (_guard, root) = temp();
    assert!(read(&root.join("uf.lock")).unwrap().is_empty());
}

/// The record round-trips, sorted by spec, written the way `uf.lock` is.
#[test]
fn a_record_is_written_the_way_uf_lock_is_written() {
    let (_guard, root) = temp();
    let path = root.join("uf.lock");
    let mut lock = ToolchainLock::default();
    assert_eq!(lock.insert(Tool::Node, "26", "26.8.2"), None);
    lock.insert(Tool::Bun, "1.4", "1.4.2");

    write(&path, &lock).unwrap();

    assert_eq!(
        fs::read_to_string(&path).unwrap(),
        "{\n  \"toolchain\": {\n    \"bun@1.4\": \"1.4.2\",\n    \"node@26\": \"26.8.2\"\n  }\n}\n"
    );
    let read_back = read(&path).unwrap();
    assert_eq!(read_back, lock);
    assert_eq!(read_back.get(Tool::Node, "26"), Some("26.8.2"));
    assert_eq!(read_back.get(Tool::Node, "24"), None);
    assert!(is_toolchain_only(&fs::read_to_string(&path).unwrap()));

    // Moving a prefix says where it was.
    assert_eq!(
        lock.insert(Tool::Node, "26", "26.9.0").as_deref(),
        Some("26.8.2")
    );
    assert_eq!(
        lock.entries().collect::<Vec<_>>(),
        [("bun@1.4", "1.4.2"), ("node@26", "26.9.0")]
    );
}

/// The resolver's half of the file is carried through untouched, in its own
/// order, and emptying the record takes only the record away.
#[test]
fn the_resolvers_half_of_the_file_is_left_as_it_was() {
    let (_guard, root) = temp();
    let path = root.join("uf.lock");
    let resolver =
        "{\n  \"lockfileVersion\": 1,\n  \"resolver\": \"uf-native\",\n  \"packages\": []\n}\n";
    fs::write(&path, resolver).unwrap();

    let mut lock = ToolchainLock::default();
    lock.insert(Tool::Node, "26", "26.8.2");
    write(&path, &lock).unwrap();

    let text = fs::read_to_string(&path).unwrap();
    assert!(
        text.starts_with(
            "{\n  \"lockfileVersion\": 1,\n  \"resolver\": \"uf-native\",\n  \"packages\": [],\n  \"toolchain\": {"
        ),
        "{text}"
    );
    assert!(!is_toolchain_only(&text));

    write(&path, &ToolchainLock::default()).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), resolver);
}

/// A file that held only the record goes when the record does, because `{}` is
/// what an empty resolver lock looks like and would vote for uf's resolver.
#[test]
fn a_file_that_only_held_the_record_goes_with_it() {
    let (_guard, root) = temp();
    let path = root.join("uf.lock");
    let mut lock = ToolchainLock::default();
    lock.insert(Tool::Deno, "2", "2.9.6");
    write(&path, &lock).unwrap();
    assert!(path.is_file());

    lock.retain(|spec| spec != "deno@2");
    assert!(lock.is_empty());
    write(&path, &lock).unwrap();
    assert!(!path.exists());

    // Writing nothing where there was nothing writes nothing.
    write(&path, &lock).unwrap();
    assert!(!path.exists());
}

/// A lock uf cannot read is refused by path, rather than read as empty and
/// every prefix silently re-resolved.
#[test]
fn a_lock_uf_cannot_read_is_refused_by_path() {
    let (_guard, root) = temp();
    let path = root.join("uf.lock");
    for (text, detail) in [
        ("not json", "expected"),
        ("[]", "not a JSON object"),
        (r#"{"toolchain":[]}"#, "`toolchain` is not an object"),
        (
            r#"{"toolchain":{"node@26":26}}"#,
            "`toolchain.node@26` is not a version string",
        ),
    ] {
        fs::write(&path, text).unwrap();
        let error = read(&path).unwrap_err();
        assert!(
            matches!(&error, EnvError::LockUnreadable { .. }),
            "{text}: {error:?}"
        );
        let message = error.to_string();
        assert!(message.contains(path.as_str()), "{message}");
        assert!(message.contains(detail), "{text}: {message}");
    }

    // And a write does not paper over a file it cannot read either.
    fs::write(&path, "not json").unwrap();
    assert!(write(&path, &ToolchainLock::default()).is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "not json");
}

/// Only a file whose one key is the record is toolchain-only; everything else
/// votes in package-manager detection as it always did.
#[test]
fn only_the_record_and_nothing_else_is_toolchain_only() {
    assert!(is_toolchain_only(r#"{"toolchain":{"node@26":"26.8.2"}}"#));
    assert!(is_toolchain_only(r#"{"toolchain":{}}"#));
    assert!(
        !is_toolchain_only("{}"),
        "an empty object is an empty resolver lock"
    );
    assert!(!is_toolchain_only(r#"{"packages":[],"toolchain":{}}"#));
    assert!(!is_toolchain_only(r#"{"lockfileVersion":1}"#));
    assert!(!is_toolchain_only("not json"));
    assert!(!is_toolchain_only(r#"["toolchain"]"#));
}
