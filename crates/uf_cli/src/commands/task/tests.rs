//! What `uf exec` and `uf run` hand to `Command`, and on which platform's
//! rules.
//!
//! Two questions, one arrangement. `uf exec` picks a file out of
//! `node_modules/.bin`; `uf run` turns a task's command string into a program,
//! or into the shell that has to run it instead. Both answers differ between
//! Windows and everywhere else, and neither is asked with a `#[cfg]`.
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

// --- `uf run`: the program a task names, and the shell it does not need ---

/// `sh` where it is part of the platform, a real lookup where it is not.
///
/// On Unix the answer is the bare name, so the spawn is byte for byte the one
/// `uf run` has always done. On Windows it is a search, and the `None` it can
/// return is what turns "os error 2" into a sentence about the task.
#[test]
fn a_posix_shell_is_looked_for_only_where_it_may_be_missing() {
    let installed = tempfile::tempdir().expect("a temporary directory");
    fs::write(installed.path().join("sh.exe"), "").expect("a shell");
    let path = std::ffi::OsString::from(installed.path().as_os_str());
    let empty = tempfile::tempdir().expect("a temporary directory");

    assert_eq!(
        posix_shell(ShellLookup::OnThePath, None),
        Some(Utf8PathBuf::from("sh")),
        "a platform that has a shell does not have to go looking for one"
    );
    assert_eq!(
        posix_shell(ShellLookup::Searched, Some(&path)),
        Some(Utf8PathBuf::from_path_buf(installed.path().join("sh.exe")).expect("utf-8"))
    );
    assert_eq!(
        posix_shell(
            ShellLookup::Searched,
            Some(&std::ffi::OsString::from(empty.path().as_os_str()))
        ),
        None
    );
    assert_eq!(posix_shell(ShellLookup::Searched, None), None);
}

/// A machine with no `sh` is told which construct needed one.
///
/// Which is the whole of the Windows half of ubugeeei-prod/uf#272: `uf run`
/// used to spawn `sh` there too, and the failure was `CreateProcess` not
/// finding a program nobody had written down.
#[test]
fn a_command_that_needs_a_missing_shell_names_the_construct() {
    let refused = TaskSpawner::shell(
        "uf lint && uf test",
        uf_task::ShellSyntax::Operator("&&"),
        None,
    )
    .expect_err("no shell, so no command");
    let said = refused.to_string();

    assert!(said.contains("the shell operator `&&`"), "{said}");
    assert!(said.contains("uf lint && uf test"), "{said}");
    // The runner puts `task "check" could not be started:` in front of this,
    // so the task is named once rather than twice.
    assert!(said.starts_with("it needs a shell"), "{said}");
}

/// And where there is one, it gets `-c` and the whole string, as before.
#[test]
fn a_command_that_needs_a_shell_is_handed_all_of_it() {
    let process = TaskSpawner::shell(
        "echo a; echo b",
        uf_task::ShellSyntax::Operator(";"),
        Some(Utf8Path::new("sh")),
    )
    .expect("a shell was found");

    assert_eq!(process.get_program(), "sh");
    assert_eq!(
        process.get_args().collect::<Vec<_>>(),
        ["-c", "echo a; echo b"]
    );
}

/// A program written as a path is resolved against the task's directory
/// before it is spawned.
///
/// std calls a relative program beside `current_dir` "platform specific and
/// unstable": Unix resolves it against the child's directory and Windows
/// against the parent's. `tools/upstream/sync.sh` is how most of this
/// repository's tasks are written, and it has to mean one file on both.
#[test]
fn a_program_written_as_a_path_is_resolved_before_it_is_spawned() {
    let uf_task::Command::Direct(direct) = uf_task::parse("tools/upstream/sync.sh --integrations")
    else {
        panic!("a program and one argument");
    };

    let process = TaskSpawner::started(&direct, Utf8Path::new("/project"));
    assert_eq!(process.get_program(), "/project/tools/upstream/sync.sh");
    assert_eq!(process.get_args().collect::<Vec<_>>(), ["--integrations"]);
}

/// A bare name is left to the platform's own lookup, as a shell would leave
/// it.
#[test]
fn a_program_written_as_a_name_is_left_to_the_path() {
    let uf_task::Command::Direct(direct) =
        uf_task::parse("cargo clippy --workspace -- -D warnings")
    else {
        panic!("a program and its arguments");
    };

    let process = TaskSpawner::started(&direct, Utf8Path::new("/project"));
    assert_eq!(process.get_program(), "cargo");
    assert_eq!(
        process.get_args().collect::<Vec<_>>(),
        ["clippy", "--workspace", "--", "-D", "warnings"]
    );
}

#[test]
fn a_program_is_a_path_when_it_carries_either_separator() {
    assert!(is_path("./target/release/uf"));
    assert!(is_path("tools/ci/publishable.sh"));
    assert!(is_path(r"tools\ci\publishable.cmd"));
    assert!(!is_path("cargo"));
    assert!(!is_path("uf"));
}
