use camino::Utf8PathBuf;

use super::switch::{link_atomically, place};
use super::*;

fn scratch() -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
    (dir, path)
}

fn store_in(root: &Utf8Path) -> Store {
    Store {
        runtimes: root.join("share/uf/runtimes"),
        bin_dir: root.join("bin"),
        state_dir: root.join("state"),
    }
}

/// A file that runs, holding `contents`.
#[cfg(unix)]
fn executable(path: &Utf8Path, contents: &str) {
    use std::os::unix::fs::PermissionsExt;

    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, contents).unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A complete runtime in the store, whose binaries say which version they are.
#[cfg(unix)]
fn runtime(store: &Store, version: &str) {
    for name in BINARIES {
        executable(
            &store.version_dir(version).join("bin").join(name),
            &format!("#!/bin/sh\necho {name} {version}\n"),
        );
    }
}

/// A checkpoint that never stops a switch.
fn never(_: Step) -> Result<()> {
    Ok(())
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
    // Beside the runtimes, where the installer writes it too.
    assert_eq!(store.previous_record(), "/share/uf/previous-version");
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

/// A version becomes a directory name and a URL segment, so nothing that
/// would leave the store is one.
#[test]
fn a_version_is_what_a_release_tag_carries() {
    for version in ["0.0.0-alpha.35", "1.2.3", "1.2.3+build.5"] {
        assert!(is_version(version), "{version} was refused");
    }
    for text in [
        "",
        "../bin",
        "a/b",
        ".hidden",
        "-rf",
        "0.0.0 alpha",
        "9.9.9\n",
    ] {
        assert!(!is_version(text), "{text:?} was accepted");
    }
}

/// Which version is active is read from the link the switch commits with.
#[cfg(unix)]
#[test]
fn the_active_version_is_read_from_the_link() {
    let (_guard, root) = scratch();
    let store = store_in(&root);
    runtime(&store, "0.0.0-alpha.13");
    std::fs::create_dir_all(&store.bin_dir).unwrap();
    std::os::unix::fs::symlink(
        store.binary("0.0.0-alpha.13").as_std_path(),
        store.bin_dir.join("uf").as_std_path(),
    )
    .unwrap();

    assert_eq!(active_version(&store).as_deref(), Some("0.0.0-alpha.13"));
}

/// A link that points somewhere else is not an answer to "which version".
#[cfg(unix)]
#[test]
fn a_link_outside_the_store_is_not_a_version() {
    let (_guard, root) = scratch();
    let store = store_in(&root);
    executable(&root.join("elsewhere/uf@1.0.0/bin/uf"), "#!/bin/sh\n");
    std::fs::create_dir_all(&store.bin_dir).unwrap();
    std::os::unix::fs::symlink(
        root.join("elsewhere/uf@1.0.0/bin/uf").as_std_path(),
        store.bin_dir.join("uf").as_std_path(),
    )
    .unwrap();

    assert_eq!(active_version(&store), None);
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

/// A name already linked to the previous version is the normal case, and so
/// is a file that is not a link at all.
#[cfg(unix)]
#[test]
fn linking_replaces_whatever_was_there_and_leaves_nothing_beside_it() {
    let (_guard, root) = scratch();
    let old = root.join("old-uf");
    let new = root.join("new-uf");
    executable(&old, "old");
    executable(&new, "new");
    let path = root.join("uf");
    std::fs::write(&path, "a file that was here first").unwrap();

    link_atomically(&old, &path).unwrap();
    link_atomically(&new, &path).unwrap();

    assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
    assert_eq!(std::fs::read_link(path.as_std_path()).unwrap(), new);
    assert_eq!(std::fs::read_dir(&root).unwrap().count(), 3);
}

/// A link to a binary that does not run is the dangling link a switch exists
/// not to make.
#[cfg(unix)]
#[test]
fn a_runtime_missing_a_binary_is_not_switched_to() {
    let (_guard, root) = scratch();
    let store = store_in(&root);
    runtime(&store, "1.0.0");
    runtime(&store, "2.0.0");
    switch_to(&store, "1.0.0", Origin::Release, &mut never).unwrap();
    std::fs::remove_file(store.version_dir("2.0.0").join("bin/ufx")).unwrap();

    assert!(switch_to(&store, "2.0.0", Origin::Release, &mut never).is_err());
    assert!(links_agree(&store, "1.0.0"));
    assert_eq!(recorded_previous(&store), None);
}

/// What `--rollback` returns to is the version a switch replaced — never the
/// one it switched to, and not forgotten by switching to the active one again.
#[cfg(unix)]
#[test]
fn a_switch_records_the_version_it_replaced_and_never_itself() {
    let (_guard, root) = scratch();
    let store = store_in(&root);
    runtime(&store, "1.0.0");
    runtime(&store, "2.0.0");

    let first = switch_to(&store, "1.0.0", Origin::Release, &mut never).unwrap();
    assert_eq!(first.replaced, None);
    assert_eq!(recorded_previous(&store), None);

    let second = switch_to(&store, "2.0.0", Origin::Release, &mut never).unwrap();
    assert_eq!(second.replaced.as_deref(), Some("1.0.0"));
    assert_eq!(recorded_previous(&store).as_deref(), Some("1.0.0"));

    switch_to(&store, "2.0.0", Origin::Release, &mut never).unwrap();
    assert_eq!(recorded_previous(&store).as_deref(), Some("1.0.0"));

    switch_to(&store, "1.0.0", Origin::Release, &mut never).unwrap();
    assert_eq!(recorded_previous(&store).as_deref(), Some("2.0.0"));
}

/// The kill a switch is built for, at every point it can happen: stop after
/// each step with nothing undone, and every name must still run a complete
/// runtime, every record must still parse, and running the switch again must
/// finish it with the version it replaced on record.
#[cfg(unix)]
#[test]
fn a_switch_killed_after_any_step_leaves_every_name_on_a_complete_runtime() {
    for stop in Step::ALL {
        let (_guard, root) = scratch();
        let store = store_in(&root);
        runtime(&store, "1.0.0");
        runtime(&store, "2.0.0");
        switch_to(&store, "1.0.0", Origin::Release, &mut never).unwrap();

        let killed = switch_to(&store, "2.0.0", Origin::Release, &mut |step| {
            if step == stop {
                Err(anyhow!("killed after {step:?}"))
            } else {
                Ok(())
            }
        });
        assert!(killed.is_err(), "the switch did not stop after {stop:?}");

        for name in BINARIES {
            let target = std::fs::read_link(store.bin_dir.join(name))
                .unwrap_or_else(|error| panic!("killed after {stop:?}, {name} is gone: {error}"));
            let target = Utf8PathBuf::from_path_buf(target).unwrap();
            assert!(
                is_executable_file(&target),
                "killed after {stop:?}, {name} points at {target}, which does not run"
            );
        }
        let uf = active_version(&store)
            .unwrap_or_else(|| panic!("killed after {stop:?}, uf runs nothing in the store"));
        assert!(
            ["1.0.0", "2.0.0"].contains(&uf.as_str()),
            "killed after {stop:?}, uf runs {uf}"
        );
        for record in [
            store.version_dir("2.0.0").join("runtime.json"),
            store.state_dir.join("active-runtime.json"),
        ] {
            if let Ok(contents) = std::fs::read_to_string(&record) {
                serde_json::from_str::<serde_json::Value>(&contents).unwrap_or_else(|error| {
                    panic!("killed after {stop:?}, {record} is not whole: {error}")
                });
            }
        }
        if let Some(previous) = recorded_previous(&store) {
            assert!(
                store.has_complete(&previous),
                "killed after {stop:?}, the rollback target uf@{previous} is not in the store"
            );
        }

        switch_to(&store, "2.0.0", Origin::Release, &mut never).unwrap();
        assert!(
            links_agree(&store, "2.0.0"),
            "killed after {stop:?}, running the switch again did not finish it"
        );
        assert_eq!(
            recorded_previous(&store).as_deref(),
            Some("1.0.0"),
            "killed after {stop:?}, the version the switch replaced was lost"
        );
    }
}

/// A kill in the middle of a write leaves the file under its `.incoming` name,
/// and the next switch neither reads it nor leaves it behind.
#[cfg(unix)]
#[test]
fn what_a_killed_write_left_is_neither_read_nor_kept() {
    let (_guard, root) = scratch();
    let store = store_in(&root);
    runtime(&store, "1.0.0");
    runtime(&store, "2.0.0");
    switch_to(&store, "1.0.0", Origin::Release, &mut never).unwrap();

    let pid = std::process::id();
    std::fs::write(
        store
            .root()
            .join(format!(".previous-version.incoming.{pid}")),
        "9.9",
    )
    .unwrap();
    std::os::unix::fs::symlink(
        root.join("nowhere").as_std_path(),
        store
            .bin_dir
            .join(format!(".uf.incoming.{pid}"))
            .as_std_path(),
    )
    .unwrap();

    switch_to(&store, "2.0.0", Origin::Release, &mut never).unwrap();

    assert_eq!(recorded_previous(&store).as_deref(), Some("1.0.0"));
    assert!(links_agree(&store, "2.0.0"));
    for dir in [store.bin_dir.clone(), store.root().to_owned()] {
        let leftovers = std::fs::read_dir(&dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".incoming."))
            .collect::<Vec<_>>();
        assert!(leftovers.is_empty(), "{dir} kept {leftovers:?}");
    }
}

/// A record that does not hold a version is not one.
#[test]
fn a_record_that_is_not_a_version_is_not_a_rollback_target() {
    let (_guard, root) = scratch();
    let store = store_in(&root);
    std::fs::create_dir_all(store.root()).unwrap();
    for contents in ["", "../../bin\n", "not a version\n"] {
        std::fs::write(store.previous_record(), contents).unwrap();
        assert_eq!(recorded_previous(&store), None, "{contents:?}");
    }
    std::fs::write(store.previous_record(), "0.0.0-alpha.34\n").unwrap();
    assert_eq!(recorded_previous(&store).as_deref(), Some("0.0.0-alpha.34"));
}

/// A version that is not there yet arrives in one rename.
#[cfg(unix)]
#[test]
fn placing_where_nothing_is_renames_the_staged_directory() {
    let (_guard, root) = scratch();
    let store = store_in(&root);
    let staged = store.runtimes.join(".uf@1.0.0.incoming.1");
    for name in BINARIES {
        executable(&staged.join("bin").join(name), "staged");
    }

    place(&staged, &store.version_dir("1.0.0")).unwrap();

    assert!(store.has_complete("1.0.0"));
    assert!(!staged.exists());
}

/// A version that is there — possibly running — has its files replaced, and
/// the directory the links point into is never the thing that moves.
#[cfg(unix)]
#[test]
fn placing_over_a_runtime_replaces_its_files_and_keeps_its_directory() {
    let (_guard, root) = scratch();
    let store = store_in(&root);
    runtime(&store, "1.0.0");
    let staged = store.runtimes.join(".uf@1.0.0.incoming.1");
    for name in BINARIES {
        executable(&staged.join("bin").join(name), "rebuilt");
    }
    std::fs::write(staged.join("README.md"), "readme").unwrap();

    place(&staged, &store.version_dir("1.0.0")).unwrap();

    for name in BINARIES {
        let binary = store.version_dir("1.0.0").join("bin").join(name);
        assert_eq!(std::fs::read_to_string(&binary).unwrap(), "rebuilt");
    }
    assert!(store.version_dir("1.0.0").join("README.md").exists());
    assert!(store.has_complete("1.0.0"));
    assert!(!staged.exists());
}

/// The embedded installer has to be the installer, not a file that happens to
/// be at that path: `include_str!` is resolved at build time and would happily
/// carry a truncated or renamed script into the binary.
#[test]
fn the_embedded_installer_is_the_one_that_acquires_a_release() {
    assert!(INSTALLER_SH.starts_with("#!/bin/sh\n"));
    for marker in [
        "UF_RELEASE_BASE",
        "UF_INSTALL_ROOT",
        "UF_BIN_DIR",
        "UF_VERSION",
        "UF_STOP_AFTER",
        "previous-version",
        "checksum mismatch",
        "writes outside its own directory",
    ] {
        assert!(
            INSTALLER_SH.contains(marker),
            "the embedded installer does not mention {marker}"
        );
    }
}
