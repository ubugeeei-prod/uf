//! Which file in `node_modules/.bin` `uf exec` hands to `Command`.
//!
//! # What these prove, and what they cannot
//!
//! ubugeeei-prod/uf#390 is a Windows bug on a project that has no Windows
//! machine, no Windows release target (#309) and no Windows runner in
//! `ci.yml`. The issue's own conclusion was that a `#[cfg(windows)]` fix could
//! not be tested and so should not be written.
//!
//! [`BinPlatform`] is the way past that. The rules are a value rather than a
//! `#[cfg]`, so the directory below — the three files npm, pnpm and yarn
//! really write for one executable — can be built in a temporary directory on
//! the Ubuntu runner and asked which file each platform would run. That is the
//! whole of the decision #390 is about: an extensionless `sh` script that
//! `is_file()` reports as present, beside the `.cmd` that Windows can actually
//! start.
//!
//! What is *not* proven here, and is not provable here:
//!
//! * that `CreateProcess` accepts the chosen file. Nothing in this file
//!   spawns anything; it asserts which path is chosen.
//! * that the standard library's `.bat`/`.cmd` argument escaping
//!   (CVE-2024-24576) behaves on a hostile argument. That is std's code and it
//!   only runs on Windows. What uf can be held to is passing arguments through
//!   untouched, which `exec_runs_an_installed_binary_and_forwards_its_arguments_and_status`
//!   in `tests/cli.rs` asserts on a real spawn, with the metacharacters batch
//!   quoting cares about in them.
//! * that a Windows build of uf exists at all. It does not; #309 is that.

use std::fs;

use super::*;

/// Both sets of rules, so an assertion can be exhaustive over them.
const PLATFORMS: [BinPlatform; 2] = [BinPlatform::Unix, BinPlatform::Windows];

/// A project whose `node_modules/.bin` holds exactly `files`.
fn bin_with(files: &[&str]) -> (tempfile::TempDir, Utf8PathBuf) {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
    let bin = root.join("node_modules/.bin");
    fs::create_dir_all(&bin).expect("a .bin directory");
    for file in files {
        fs::write(bin.join(file), "").expect("a linked binary");
    }
    (dir, root)
}

/// The three files a package manager writes, and the one Windows can run.
///
/// This is #390 exactly. `node_modules/.bin/tool` exists on Windows — it is
/// the `sh` script for Git Bash — so the old lookup succeeded, handed
/// `CreateProcess` a file Windows cannot execute, and the failure surfaced as
/// "failed to execute …\node_modules\.bin\tool": an error naming a file that
/// plainly exists.
#[test]
fn windows_runs_the_cmd_shim_and_not_the_sh_script_beside_it() {
    let (_guard, root) = bin_with(&["tool", "tool.cmd", "tool.ps1"]);

    assert_eq!(
        installed_binary_in(&root, "tool", BinPlatform::Windows),
        Some(root.join("node_modules/.bin/tool.cmd"))
    );
    // And Unix runs the one with the executable bit, as it always did.
    assert_eq!(
        installed_binary_in(&root, "tool", BinPlatform::Unix),
        Some(root.join("node_modules/.bin/tool"))
    );
}

/// A package that ships a real executable gets the real executable.
///
/// `.exe` outranks `.cmd` because `PATHEXT` orders it first, and a package
/// with both means it: the `.cmd` is the shim, the `.exe` is the program.
#[test]
fn a_native_executable_outranks_the_shim_written_beside_it() {
    let (_guard, root) = bin_with(&["esbuild", "esbuild.cmd", "esbuild.exe"]);

    assert_eq!(
        installed_binary_in(&root, "esbuild", BinPlatform::Windows),
        Some(root.join("node_modules/.bin/esbuild.exe"))
    );
}

/// The candidates are tried in `PATHEXT`'s order, not in directory order.
#[test]
fn the_windows_candidates_are_pathexts_order() {
    assert_eq!(
        bin_candidates("tool", BinPlatform::Windows),
        ["tool.com", "tool.exe", "tool.bat", "tool.cmd"]
    );
    assert_eq!(bin_candidates("tool", BinPlatform::Unix), ["tool"]);

    // `.ps1` is never a candidate: `CreateProcess` cannot start one, so the
    // third file a package manager writes is no more runnable than the first.
    assert!(
        !bin_candidates("tool", BinPlatform::Windows)
            .iter()
            .any(|candidate| candidate.ends_with(".ps1"))
    );
}

/// On Windows the extensionless file is not a fallback either.
///
/// A `.bin` holding only the `sh` script is a `.bin` with nothing Windows can
/// run, and saying so is what sends `exec_package` on to the path it can
/// actually do something with. Choosing the `sh` script "because it is better
/// than nothing" is the bug.
#[test]
fn a_lone_sh_script_is_nothing_windows_can_run() {
    let (_guard, root) = bin_with(&["tool"]);

    assert_eq!(
        installed_binary_in(&root, "tool", BinPlatform::Windows),
        None
    );
    assert!(installed_binary_in(&root, "tool", BinPlatform::Unix).is_some());
}

/// A scoped package is linked under its bare binary name, on both platforms.
#[test]
fn a_scoped_package_resolves_to_its_bare_binary_name() {
    let (_guard, root) = bin_with(&["vite", "vite.cmd"]);

    assert_eq!(
        installed_binary_in(&root, "@uniflowed/vite", BinPlatform::Unix),
        Some(root.join("node_modules/.bin/vite"))
    );
    assert_eq!(
        installed_binary_in(&root, "@uniflowed/vite", BinPlatform::Windows),
        Some(root.join("node_modules/.bin/vite.cmd"))
    );
}

/// Nothing gets to leave `node_modules/.bin`, on any platform.
///
/// The extension list appends to the name, so it must not be possible to reach
/// out of the directory first and pick up a `.cmd` from somewhere else —
/// `..\\evil.cmd` is a real file name on Windows and would be a real spawn.
///
/// Two rules do that, and they are different rules. A separator or a leading
/// dot in the *last segment* is refused outright, because either would make
/// the join mean something other than one file in one directory. Everything
/// before the last separator is discarded rather than refused, because that is
/// how a scoped name resolves: `@uniflowed/vite` is `vite`. So `../evil` looks
/// alarming and is simply `evil`, which is a lookup in this project's own
/// `.bin` and nowhere else.
#[test]
fn a_name_that_could_climb_out_of_the_bin_directory_is_refused() {
    let (_guard, root) = bin_with(&["tool", "tool.cmd", "evil", "evil.cmd"]);
    let bin = root.join("node_modules/.bin");

    // Refused outright: the last segment is not a file name.
    for package in ["", ".", "..", "a/../..", "sub/", "..\\evil", ".hidden"] {
        assert_eq!(binary_name(package), None, "{package:?}");
        for platform in PLATFORMS {
            assert_eq!(
                installed_binary_in(&root, package, platform),
                None,
                "{package:?} resolved on {platform:?}"
            );
        }
    }

    // And the ones that do resolve resolve inside the directory, which is the
    // property that actually matters.
    for package in ["tool", "./tool", "../evil", "@scope/tool", "a/b/c/tool"] {
        for platform in PLATFORMS {
            let resolved = installed_binary_in(&root, package, platform)
                .unwrap_or_else(|| panic!("{package:?} on {platform:?}"));
            assert!(
                resolved.starts_with(&bin) && resolved.parent() == Some(bin.as_path()),
                "{resolved} is not one file in {bin}"
            );
        }
    }
}

/// A `.bin` entry that is a directory is not an executable.
#[test]
fn a_directory_named_like_a_binary_is_not_one() {
    let (_guard, root) = bin_with(&[]);
    let bin = root.join("node_modules/.bin");
    fs::create_dir(bin.join("tool")).expect("a directory in its place");
    fs::create_dir(bin.join("tool.cmd")).expect("a directory in its place");

    for platform in PLATFORMS {
        assert_eq!(installed_binary_in(&root, "tool", platform), None);
    }
}

/// A project with no `node_modules` at all resolves nothing, and does not
/// panic on the missing directory.
#[test]
fn a_project_without_node_modules_resolves_nothing() {
    let dir = tempfile::tempdir().expect("a temporary directory");
    let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");

    for platform in PLATFORMS {
        assert_eq!(installed_binary_in(&root, "tool", platform), None);
    }
}
