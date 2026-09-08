use camino::Utf8PathBuf;

use super::*;

fn scratch() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    (dir, path)
}

/// The store is the installer's, so a version the installer unpacked is a
/// version `uf use` finds without downloading anything.
#[test]
fn the_store_addresses_a_version_the_way_the_installer_writes_it() {
    let store = Store {
        runtimes: Utf8PathBuf::from("/share/uf/runtimes"),
        bin_dir: Utf8PathBuf::from("/bin"),
        state_dir: Utf8PathBuf::from("/state"),
    };

    assert_eq!(
        store.version_dir("0.0.0-alpha.13"),
        "/share/uf/runtimes/uf@0.0.0-alpha.13"
    );
    assert!(
        store
            .binary("0.0.0-alpha.13")
            .as_str()
            .starts_with("/share/uf/runtimes/uf@0.0.0-alpha.13/bin/uf")
    );
}

/// `UF_INSTALL_ROOT=` in a CI environment means the variable was never set, and
/// reading it as a path would resolve the store to `/runtimes`.
#[test]
fn an_empty_variable_is_not_a_value() {
    assert_eq!(set_to_something(String::new()), None);
    assert_eq!(
        set_to_something("/opt/uf".to_owned()),
        Some("/opt/uf".to_owned())
    );
}

/// What the installer resolved `latest` to, read from the link it wrote rather
/// than from its prose.
#[cfg(unix)]
#[test]
fn the_installed_version_is_read_from_the_link_the_installer_wrote() {
    let (_guard, root) = scratch();
    let store = Store {
        runtimes: root.join("share/uf/runtimes"),
        bin_dir: root.join("bin"),
        state_dir: root.join("state"),
    };
    let binary = store.binary("0.0.0-alpha.13");
    std::fs::create_dir_all(binary.parent().unwrap()).unwrap();
    std::fs::write(&binary, "#!/bin/sh\n").unwrap();
    std::fs::create_dir_all(&store.bin_dir).unwrap();
    std::os::unix::fs::symlink(binary.as_std_path(), store.bin_dir.join("uf").as_std_path())
        .unwrap();

    assert_eq!(installed_version(&store).unwrap(), "0.0.0-alpha.13");
}

/// A link that points somewhere else is not an answer to "which version".
#[cfg(unix)]
#[test]
fn a_link_outside_the_store_is_not_a_version() {
    let (_guard, root) = scratch();
    let store = Store {
        runtimes: root.join("share/uf/runtimes"),
        bin_dir: root.join("bin"),
        state_dir: root.join("state"),
    };
    std::fs::create_dir_all(&store.bin_dir).unwrap();
    std::fs::write(root.join("elsewhere"), "#!/bin/sh\n").unwrap();
    std::os::unix::fs::symlink(
        root.join("elsewhere").as_std_path(),
        store.bin_dir.join("uf").as_std_path(),
    )
    .unwrap();

    assert!(installed_version(&store).is_err());
}

/// Activating a runtime a second time must not overwrite where it came from
/// with the fact that it was already there.
#[test]
fn an_existing_origin_survives_a_second_activation() {
    let (_guard, root) = scratch();
    let manifest = root.join("runtime.json");
    std::fs::write(&manifest, r#"{"source": "release"}"#).unwrap();

    assert_eq!(recorded_origin(&manifest), Some(Origin::Release));
}

/// And a manifest uf did not write is not a fact to preserve: `already-installed`
/// is what uf knows, and inventing `release` for it would be the #534 mistake
/// in a smaller field.
#[test]
fn an_unreadable_or_unknown_origin_is_not_invented() {
    let (_guard, root) = scratch();
    let missing = root.join("absent.json");
    assert_eq!(recorded_origin(&missing), None);

    let garbage = root.join("garbage.json");
    std::fs::write(&garbage, "not json").unwrap();
    assert_eq!(recorded_origin(&garbage), None);

    let unknown = root.join("unknown.json");
    std::fs::write(&unknown, r#"{"source": "current-exe"}"#).unwrap();
    assert_eq!(recorded_origin(&unknown), None);
}

/// A version already on the machine downloads nothing, and `uf use` must not
/// claim it did.
#[test]
fn only_a_download_counts_as_one() {
    assert!(Origin::Release.downloaded());
    assert!(Origin::Mirror.downloaded());
    assert!(!Origin::RunningBinary.downloaded());
    assert!(!Origin::AlreadyInstalled.downloaded());
}

/// `ln -sfn`: a name already linked to the previous version is the normal case.
#[cfg(unix)]
#[test]
fn linking_replaces_whatever_was_there() {
    let (_guard, root) = scratch();
    let old = root.join("old-uf");
    let new = root.join("new-uf");
    std::fs::write(&old, "old").unwrap();
    std::fs::write(&new, "new").unwrap();
    let path = root.join("uf");

    link(&old, &path).unwrap();
    link(&new, &path).unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    assert_eq!(std::fs::read_link(path.as_std_path()).unwrap(), new);
}

/// The embedded installer has to be the installer, not a file that happens to
/// be at that path: `include_str!` is resolved at build time and would happily
/// carry a truncated or renamed script into the binary.
#[test]
fn the_embedded_installer_is_the_one_that_acquires_a_release() {
    assert!(INSTALLER.starts_with("#!/bin/sh\n"));
    for marker in [
        "UF_RELEASE_BASE",
        "UF_INSTALL_ROOT",
        "UF_BIN_DIR",
        "UF_VERSION",
        "checksum mismatch",
        "writes outside its own directory",
    ] {
        assert!(
            INSTALLER.contains(marker),
            "the embedded installer does not mention {marker}"
        );
    }
}
