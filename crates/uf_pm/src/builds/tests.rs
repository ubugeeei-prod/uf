//! What is in the tree that would run code, and what the project has said
//! about it.

use super::*;
use crate::detect::YarnEdition;

const YARN_BERRY: PackageManager = PackageManager::Yarn(YarnEdition::Berry);
const YARN_CLASSIC: PackageManager = PackageManager::Yarn(YarnEdition::Classic);

fn project(files: &[(&str, &str)]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8Path::from_path(dir.path()).expect("a UTF-8 path");
    for (path, contents) in files {
        let file = root.join(path);
        fs::create_dir_all(file.parent().expect("a parent")).expect("a directory");
        fs::write(&file, contents).expect("a file");
    }
    dir
}

fn root_of(dir: &tempfile::TempDir) -> &Utf8Path {
    Utf8Path::from_path(dir.path()).expect("a UTF-8 path")
}

fn nothing() -> BTreeSet<CompactString> {
    BTreeSet::new()
}

const WITH_POSTINSTALL: &str =
    r#"{"name":"sharp","version":"0.33.5","scripts":{"postinstall":"node install.js"}}"#;
const HARMLESS: &str = r#"{"name":"left-pad","version":"1.3.0","scripts":{"test":"echo"}}"#;

#[test]
fn only_the_packages_that_would_run_code_are_listed() {
    let dir = project(&[
        ("node_modules/sharp/package.json", WITH_POSTINSTALL),
        ("node_modules/left-pad/package.json", HARMLESS),
        (
            "node_modules/@scope/native/package.json",
            r#"{"name":"@scope/native","version":"2.0.0","scripts":{"install":"make"}}"#,
        ),
    ]);

    let found = scan(root_of(&dir), &nothing()).expect("a scan");

    assert_eq!(
        found
            .iter()
            .map(|package| package.name.as_str())
            .collect::<Vec<_>>(),
        vec!["@scope/native", "sharp"],
        "left-pad has scripts, but none that run at install time"
    );
    assert_eq!(found[1].version, "0.33.5");
    assert_eq!(found[1].scripts, vec!["postinstall"]);
}

/// An empty script body is a package that declares the hook and does nothing
/// with it, which is not something to ask a person to approve.
#[test]
fn an_empty_script_is_not_a_build() {
    let dir = project(&[(
        "node_modules/quiet/package.json",
        r#"{"name":"quiet","scripts":{"postinstall":"   "}}"#,
    )]);

    assert!(scan(root_of(&dir), &nothing()).expect("a scan").is_empty());
}

/// The manager's own bookkeeping lives beside the packages and is not one.
#[test]
fn the_managers_own_directories_are_skipped() {
    let dir = project(&[
        (
            "node_modules/.pnpm/x/package.json",
            r#"{"name":"x","scripts":{"postinstall":"echo"}}"#,
        ),
        ("node_modules/sharp/package.json", WITH_POSTINSTALL),
    ]);

    let found = scan(root_of(&dir), &nothing()).expect("a scan");

    assert_eq!(found.len(), 1, "{found:?}");
    assert_eq!(found[0].name, "sharp");
}

/// A directory can be renamed; the name a manager approves is the one the
/// package claims for itself.
#[test]
fn the_name_comes_from_the_manifest_rather_than_the_directory() {
    let dir = project(&[(
        "node_modules/renamed/package.json",
        r#"{"name":"sharp","scripts":{"postinstall":"node install.js"}}"#,
    )]);

    let found = scan(root_of(&dir), &nothing()).expect("a scan");

    assert_eq!(found[0].name, "sharp");
}

#[test]
fn a_project_that_has_not_installed_yet_is_not_an_error() {
    let dir = project(&[("package.json", "{}")]);

    assert!(scan(root_of(&dir), &nothing()).expect("a scan").is_empty());
}

#[test]
fn approval_is_read_from_the_field_this_manager_reads() {
    let dir = project(&[(
        "package.json",
        r#"{
  "trustedDependencies": ["sharp"],
  "pnpm": { "onlyBuiltDependencies": ["esbuild"] },
  "dependenciesMeta": { "swc": { "built": true }, "other": { "built": false } }
}"#,
    )]);
    let root = root_of(&dir);

    assert_eq!(
        approved(root, PackageManager::Bun).expect("read"),
        ["sharp"].map(CompactString::from).into()
    );
    assert_eq!(
        approved(root, PackageManager::Pnpm).expect("read"),
        ["esbuild"].map(CompactString::from).into()
    );
    assert_eq!(
        approved(root, YARN_BERRY).expect("read"),
        ["swc"].map(CompactString::from).into(),
        "`built: false` is a package that was considered and refused"
    );
    // npm has no field, so nothing is approved and nothing can be.
    assert!(
        approved(root, PackageManager::Npm)
            .expect("read")
            .is_empty()
    );
}

#[test]
fn a_prototype_key_is_never_an_approved_package() {
    let dir = project(&[(
        "package.json",
        r#"{"trustedDependencies":["__proto__","sharp"],
            "dependenciesMeta":{"constructor":{"built":true}}}"#,
    )]);
    let root = root_of(&dir);

    assert_eq!(
        approved(root, PackageManager::Bun).expect("read"),
        ["sharp"].map(CompactString::from).into()
    );
    assert!(approved(root, YARN_BERRY).expect("read").is_empty());
}

#[test]
fn a_scan_marks_what_the_project_has_already_approved() {
    let dir = project(&[
        ("package.json", r#"{"trustedDependencies":["sharp"]}"#),
        ("node_modules/sharp/package.json", WITH_POSTINSTALL),
        (
            "node_modules/esbuild/package.json",
            r#"{"name":"esbuild","scripts":{"postinstall":"node install.js"}}"#,
        ),
    ]);
    let root = root_of(&dir);
    let approved = approved(root, PackageManager::Bun).expect("read");

    let found = scan(root, &approved).expect("a scan");

    assert_eq!(found[0].name, "esbuild");
    assert!(!found[0].approved);
    assert_eq!(found[1].name, "sharp");
    assert!(found[1].approved);
}

/// Three managers can be told about one package; two cannot, and saying so is
/// the whole of what uf can honestly offer them.
#[test]
fn npm_and_yarn_one_are_all_or_nothing() {
    assert!(approvals_for(PackageManager::Pnpm).is_per_package());
    assert!(approvals_for(PackageManager::Bun).is_per_package());
    assert!(approvals_for(YARN_BERRY).is_per_package());
    assert!(!approvals_for(PackageManager::Npm).is_per_package());
    assert!(!approvals_for(YARN_CLASSIC).is_per_package());
}

/// The rule the install turns on, stated as a test because getting it wrong
/// runs somebody's `postinstall`.
#[test]
fn scripts_are_allowed_only_when_the_manager_can_hold_the_line() {
    let empty = project(&[("package.json", "{}")]);
    let listed = project(&[(
        "package.json",
        r#"{"pnpm":{"onlyBuiltDependencies":["esbuild"]}}"#,
    )]);

    // The project said so outright: every manager, whatever is listed.
    for root in [root_of(&empty), root_of(&listed)] {
        for manager in [PackageManager::Npm, PackageManager::Pnpm] {
            assert!(
                scripts_allowed(root, manager, true).expect("read"),
                "{manager}"
            );
        }
    }

    // It did not, so the manager has to be able to enforce a list, and there
    // has to be one. An empty `onlyBuiltDependencies` is pnpm saying "none".
    assert!(!scripts_allowed(root_of(&empty), PackageManager::Pnpm, false).expect("read"));
    assert!(scripts_allowed(root_of(&listed), PackageManager::Pnpm, false).expect("read"));
    // npm cannot, at any length of list.
    assert!(!scripts_allowed(root_of(&listed), PackageManager::Npm, false).expect("read"));
    assert!(!scripts_allowed(root_of(&listed), YARN_CLASSIC, false).expect("read"));
}
