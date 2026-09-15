//! `uf use` and `uf self-update`, against a release store on disk.
//!
//! What `uf use uf@0.9.9` used to do was copy the running binary into a
//! directory called `0.9.9` and write three files saying that is what it was
//! (ubugeeei-prod/uf#534). Nothing here mocks that away: the acquisition runs
//! the real installer — the same `infra/cloudflare/setup-assets/install.sh`
//! that `curl -fsSL https://setup.uniflowed.dev | sh` runs, embedded in the
//! binary — with `UF_RELEASE_BASE` pointing at a directory of release assets
//! this file builds, `file://` and all. curl reads `file://`, so the download,
//! the checksum and the tar guard are the real ones and no server is needed.
//!
//! The kills are real too. A switch has to leave every name pointing at a
//! complete runtime whenever the process dies, and the only honest way to
//! check that is to kill it: the installer runs with stand-ins for `mv`, `ln`,
//! `rm`, `mkdir` and `tar` on `PATH` that do the real thing and then `SIGKILL`
//! the whole process group after the n-th of them, for every n. The switch uf
//! makes in Rust is stopped after each of its steps by the unit tests in
//! `src/commands/toolchain/tests.rs`.
//!
//! Every case installs into its own `UF_INSTALL_ROOT` and `UF_BIN_DIR`, so the
//! suite cannot touch the developer's own `~/.local`.
#![cfg(unix)]

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use assert_cmd::Command as TestCommand;
use support::{uf, uf_path};

/// A release the installer will accept: the archive, its digest, and a
/// `VERSION` file for the `latest` channel.
///
/// The archive holds `bin/uf`, `bin/ufr` and `bin/ufx`, because the installer
/// refuses a build that is missing any of the three. They are copies of
/// `sleep` rather than shell scripts, and that is not laziness in the other
/// direction: `sleep` is a real executable, so a test can have one *running*
/// while the installer unpacks over its path. A shell script would prove
/// nothing there — the kernel hands a script to `sh` and holds no write lock
/// on it, which is the whole difference `ETXTBSY` turns on.
fn publish(base: &Path, version: &str) {
    publish_with(base, version, a_real_executable());
}

/// [`publish`], with `executable` as all three binaries.
fn publish_with(base: &Path, version: &str, executable: &Path) {
    let target = target_triple();
    let staging = base.join(format!("staging-{version}"));
    let bin = staging.join("bin");
    fs::create_dir_all(&bin).unwrap();
    for name in ["uf", "ufr", "ufx"] {
        let path = bin.join(name);
        fs::copy(executable, &path).unwrap();
        make_executable(&path);
    }

    let channel = base.join(version);
    fs::create_dir_all(&channel).unwrap();
    let archive = channel.join(format!("uf-{target}.tar.gz"));
    let tar = Command::new("tar")
        .arg("-czf")
        .arg(&archive)
        .arg("-C")
        .arg(&staging)
        .arg("bin")
        .status()
        .unwrap();
    assert!(tar.success(), "tar could not package the fixture release");

    let digest = sha256(&archive);
    fs::write(
        channel.join(format!("uf-{target}.tar.gz.sha256")),
        format!("{digest}  uf-{target}.tar.gz\n"),
    )
    .unwrap();
    fs::write(channel.join("VERSION"), format!("{version}\n")).unwrap();
}

/// Publish `version` and make it what `latest` resolves to.
///
/// The mirror layout the installer reads for `UF_RELEASE_BASE`: a `latest/`
/// directory holding a `VERSION` naming the real version, beside a copy of
/// that version's assets.
fn publish_as_latest(base: &Path, version: &str) {
    publish(base, version);
    let target = target_triple();
    let latest = base.join("latest");
    fs::create_dir_all(&latest).unwrap();
    for name in [
        format!("uf-{target}.tar.gz"),
        format!("uf-{target}.tar.gz.sha256"),
        "VERSION".to_owned(),
    ] {
        fs::copy(base.join(version).join(&name), latest.join(&name)).unwrap();
    }
}

/// The target the installer will ask for, which is this build's own host.
fn target_triple() -> String {
    // The installer spells it `<arch>-<os>` from `uname`, which is the same
    // pair Rust names its host triple with for the four platforms uf ships.
    let arch = if cfg!(target_arch = "aarch64") {
        "aarch64"
    } else {
        "x86_64"
    };
    let os = if cfg!(target_os = "macos") {
        "apple-darwin"
    } else {
        "unknown-linux-gnu"
    };
    format!("{arch}-{os}")
}

fn sha256(path: &Path) -> String {
    for program in ["sha256sum", "shasum"] {
        let mut command = Command::new(program);
        if program == "shasum" {
            command.args(["-a", "256"]);
        }
        let Ok(output) = command.arg(path).output() else {
            continue;
        };
        if output.status.success() {
            return String::from_utf8(output.stdout)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .to_owned();
        }
    }
    panic!("neither sha256sum nor shasum is available, and the installer needs one");
}

/// A small program that stays alive when told to, wherever this platform keeps
/// it.
fn a_real_executable() -> &'static Path {
    for candidate in ["/bin/sleep", "/usr/bin/sleep"] {
        let path = Path::new(candidate);
        if path.exists() {
            return path;
        }
    }
    panic!("no sleep on this machine, and the fixture release needs a real binary");
}

/// Another real program, of a different size, so that a rebuilt release of the
/// same version can be told from the first one byte count alone.
fn another_real_executable() -> &'static Path {
    let first = fs::metadata(a_real_executable()).unwrap().len();
    for candidate in ["/bin/ls", "/usr/bin/ls", "/bin/cat", "/usr/bin/cat"] {
        let path = Path::new(candidate);
        if fs::metadata(path).is_ok_and(|metadata| metadata.len() != first) {
            return path;
        }
    }
    panic!("no second executable of another size on this machine");
}

fn make_executable(path: &Path) {
    let mut permissions = fs::metadata(path).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).unwrap();
}

/// A `uf` pointed at a private store, a private link directory and a release
/// base that is a directory rather than a host.
fn uf_in(root: &Path, release_base: Option<&Path>) -> TestCommand {
    let mut command = uf();
    command
        .env("UF_INSTALL_ROOT", root.join("share/uf"))
        .env("UF_BIN_DIR", root.join("bin"))
        .env("HOME", root.join("home"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env_remove("UF_VERSION")
        .env_remove("UF_REPO");
    match release_base {
        Some(base) => command.env("UF_RELEASE_BASE", format!("file://{}", base.display())),
        None => command.env_remove("UF_RELEASE_BASE"),
    };
    command
}

/// Where the installer unpacks a version, which is where `uf use` looks.
fn runtime_dir(root: &Path, version: &str) -> PathBuf {
    root.join("share/uf/runtimes").join(format!("uf@{version}"))
}

/// Run `uf` in `root` against `release`, and insist that it succeeded.
fn succeeds(root: &Path, release: &Path, args: &[&str]) -> String {
    let output = uf_in(root, Some(release)).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "uf {} failed:\n{}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Whether all three names point at `version`'s runtime.
fn linked_at(root: &Path, version: &str) -> bool {
    ["uf", "ufr", "ufx"].iter().all(|name| {
        fs::read_link(root.join("bin").join(name))
            .is_ok_and(|target| target == runtime_dir(root, version).join("bin").join(name))
    })
}

/// What `--rollback` would return to.
fn previous_version(root: &Path) -> Option<String> {
    fs::read_to_string(root.join("share/uf/previous-version")).ok()
}

/// The installer in this checkout, run directly, the way `curl … | sh` runs it:
/// every step, links included.
fn installer(root: &Path, release: &Path, version: &str) -> Command {
    fs::create_dir_all(root.join("tmp")).unwrap();
    let mut command = Command::new("sh");
    command
        .arg(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../infra/cloudflare/setup-assets/install.sh"),
        )
        .env("UF_INSTALL_ROOT", root.join("share/uf"))
        .env("UF_BIN_DIR", root.join("bin"))
        .env("UF_RELEASE_BASE", format!("file://{}", release.display()))
        .env("UF_VERSION", version)
        .env("HOME", root.join("home"))
        // A killed installer never runs its trap, so its download directory is
        // left where it was made: in the case's own directory, not in `/tmp`.
        .env("TMPDIR", root.join("tmp"))
        .env("NO_COLOR", "1")
        .env_remove("UF_STOP_AFTER")
        .env_remove("UF_REPO")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// `uf` as a plain process, for a case that has to put it in a process group
/// of its own.
fn uf_process(root: &Path, release: &Path) -> Command {
    fs::create_dir_all(root.join("tmp")).unwrap();
    let mut command = Command::new(uf_path());
    command
        .env("UF_INSTALL_ROOT", root.join("share/uf"))
        .env("UF_BIN_DIR", root.join("bin"))
        .env("UF_RELEASE_BASE", format!("file://{}", release.display()))
        .env("HOME", root.join("home"))
        .env("XDG_STATE_HOME", root.join("state"))
        .env("TMPDIR", root.join("tmp"))
        .env("NO_COLOR", "1")
        .env_remove("UF_VERSION")
        .env_remove("UF_REPO")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    command
}

/// Stand-ins for every command the installer changes the filesystem with. Each
/// runs the real command, and the `kill_after`-th of them then kills its whole
/// process group — `sh`, and `uf` when `uf` started it — with `SIGKILL`, so no
/// trap runs and nothing is tidied: what is left is what a closed laptop or a
/// timed-out CI job would leave.
fn killing_path(dir: &Path, kill_after: usize) -> String {
    let shims = dir.join("shims");
    fs::create_dir_all(&shims).unwrap();
    let count = dir.join("count");
    let path = std::env::var("PATH").unwrap();
    for command in ["mv", "ln", "rm", "mkdir", "tar"] {
        let real = std::env::split_paths(&path)
            .map(|directory| directory.join(command))
            .find(|candidate| candidate.is_file())
            .unwrap_or_else(|| panic!("no {command} on PATH"));
        let real = real.display();
        // Listing an archive changes nothing, so it is not a step.
        let listing = if command == "tar" {
            format!("case \"${{1:-}}\" in -*x*) ;; *) exec {real} \"$@\" ;; esac\n")
        } else {
            String::new()
        };
        let shim = shims.join(command);
        fs::write(
            &shim,
            format!(
                "#!/bin/sh\n{listing}{real} \"$@\"\nstatus=$?\n\
                 count=$(( $(cat '{count}' 2>/dev/null || echo 0) + 1 ))\n\
                 echo \"$count\" > '{count}'\n\
                 if [ \"$count\" -eq {kill_after} ]; then kill -9 0; fi\n\
                 exit \"$status\"\n",
                count = count.display(),
            ),
        )
        .unwrap();
        make_executable(&shim);
    }
    format!("{}:{path}", shims.display())
}

/// Run `command` in a process group of its own, so that the stand-ins kill it
/// and never the test.
fn in_own_group(command: &mut Command) -> ExitStatus {
    command.process_group(0).status().unwrap()
}

/// Every name still runs a whole binary, every runtime in the store is whole,
/// and the rollback record, when there is one, names a runtime that is there.
fn assert_nothing_half_done(root: &Path, context: &str) {
    let whole = [
        fs::metadata(a_real_executable()).unwrap().len(),
        fs::metadata(another_real_executable()).unwrap().len(),
    ];
    for name in ["uf", "ufr", "ufx"] {
        let link = root.join("bin").join(name);
        let metadata = fs::metadata(&link)
            .unwrap_or_else(|error| panic!("{context}: {name} leads nowhere: {error}"));
        assert!(
            metadata.is_file() && metadata.permissions().mode() & 0o111 != 0,
            "{context}: {name} does not run"
        );
        assert!(
            whole.contains(&metadata.len()),
            "{context}: {name} is a binary cut short at {} bytes",
            metadata.len()
        );
    }
    for entry in fs::read_dir(root.join("share/uf/runtimes")).unwrap() {
        let entry = entry.unwrap();
        // A staged directory is where a half-written binary is allowed to be,
        // because nothing links into it.
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.contains(".incoming.") {
            continue;
        }
        for name in ["uf", "ufr", "ufx"] {
            let binary = entry.path().join("bin").join(name);
            let length = fs::metadata(&binary)
                .unwrap_or_else(|error| {
                    panic!("{context}: {} is missing: {error}", binary.display())
                })
                .len();
            assert!(
                whole.contains(&length),
                "{context}: {} is cut short",
                binary.display()
            );
        }
    }
    if let Some(record) = previous_version(root) {
        assert!(record.ends_with('\n'), "{context}: the record is cut short");
        assert!(
            runtime_dir(root, record.trim()).join("bin/uf").exists(),
            "{context}: the record names {record:?}, which is not in the store"
        );
    }
}

/// Nothing named `.incoming.` anywhere uf writes: every leftover of a killed
/// process has been swept.
fn assert_no_leftovers(root: &Path, context: &str) {
    for dir in ["bin", "share/uf", "share/uf/runtimes"] {
        let Ok(entries) = fs::read_dir(root.join(dir)) else {
            continue;
        };
        let leftovers = entries
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|name| name.contains(".incoming."))
            .collect::<Vec<_>>();
        assert!(leftovers.is_empty(), "{context}: {dir} kept {leftovers:?}");
    }
}

/// The test #534 asks for by name: a version that does not exist must fail,
/// where it used to succeed.
#[test]
fn use_refuses_a_version_that_cannot_be_acquired() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let release = root.join("releases");
    fs::create_dir_all(&release).unwrap();

    let output = uf_in(root, Some(&release))
        .arg("--cwd")
        .arg(root)
        .args(["use", "uf@0.9.9"])
        .output()
        .unwrap();

    assert!(
        !output.status.success(),
        "uf use invented a version that was never published:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    assert!(
        !runtime_dir(root, "0.9.9").exists(),
        "a failed acquisition left a directory named after a version that does not exist"
    );
    assert!(
        !root.join("bin/uf").exists(),
        "a failed acquisition relinked uf anyway"
    );
}

/// And the acquisition is real: a published release is downloaded, checked and
/// unpacked, and the three names are linked at it together.
#[test]
fn use_acquires_a_published_version_and_links_all_three_binaries() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let release = root.join("releases");
    publish(&release, "0.9.9");

    let output = uf_in(root, Some(&release))
        .arg("--cwd")
        .arg(root)
        .args(["use", "uf@0.9.9"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let installed = runtime_dir(root, "0.9.9");
    assert!(installed.join("bin/uf").exists());

    for name in ["uf", "ufr", "ufx"] {
        let link = root.join("bin").join(name);
        assert_eq!(
            fs::read_link(&link).unwrap(),
            installed.join("bin").join(name),
            "{name} is not linked at the version that was just installed"
        );
    }

    // The archive came from a `file://` base, so the manifest must say mirror
    // rather than release: which of the two it was is the thing #534 asked
    // `runtime.json` to stop guessing about.
    let manifest = fs::read_to_string(installed.join("runtime.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(manifest["version"], "0.9.9");
    assert_eq!(manifest["source"], "mirror");

    let active = fs::read_to_string(root.join("state/uniflowed/active-runtime.json")).unwrap();
    let active: serde_json::Value = serde_json::from_str(&active).unwrap();
    assert_eq!(active["version"], "0.9.9");

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("✓ now using uf@0.9.9"), "{stdout}");
    assert!(stdout.contains("VerifyChecksum"), "{stdout}");
}

/// A version already on the machine is activated without reaching anywhere: an
/// unreachable release base proves nothing was fetched.
#[test]
fn use_activates_an_installed_version_without_fetching() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let release = root.join("releases");
    publish(&release, "0.9.9");

    assert!(
        uf_in(root, Some(&release))
            .arg("--cwd")
            .arg(root)
            .args(["use", "uf@0.9.9"])
            .output()
            .unwrap()
            .status
            .success()
    );

    let output = uf_in(root, Some(Path::new("/nowhere-at-all")))
        .arg("--cwd")
        .arg(root)
        .args(["use", "uf@0.9.9"])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "an installed version needed the network:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        !stdout.contains("DownloadRuntime"),
        "an activation claimed to have downloaded something:\n{stdout}"
    );
    // Where it came from was settled when it arrived; activating it again does
    // not turn a mirror into "it was already there".
    let manifest = fs::read_to_string(runtime_dir(root, "0.9.9").join("runtime.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(manifest["source"], "mirror");
}

/// `uf use node@22` used to produce `node/22/bin/uf` holding a copy of the uf
/// binary. The host a project runs on is not this command's business.
#[test]
fn use_refuses_a_runtime_that_is_not_uf() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();

    let output = uf_in(root, None)
        .arg("--cwd")
        .arg(root)
        .args(["use", "node@22"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("host.runtime"), "{stderr}");
    assert!(
        !root.join("share/uf/runtimes/node@22").exists()
            && !root.join("share/uf/runtimes/uf@22").exists()
    );
}

/// The one case where copying the running binary is honest: the version asked
/// for is the version that is running.
#[test]
fn use_installs_the_running_binary_only_as_its_own_version() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let version = env!("CARGO_PKG_VERSION");

    let output = uf_in(root, None)
        .arg("--cwd")
        .arg(root)
        .args(["use", &format!("uf@{version}")])
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let manifest = fs::read_to_string(runtime_dir(root, version).join("runtime.json")).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&manifest).unwrap();
    assert_eq!(manifest["source"], "running-binary");
}

/// `uf self-update` resolves `latest` through the installer and switches to it,
/// which is the command `uf upgrade` was named after and never was.
#[test]
fn self_update_resolves_the_newest_release_and_switches_to_it() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let release = root.join("releases");
    publish_as_latest(&release, "9.9.9");

    let output = uf_in(root, Some(&release))
        .arg("self-update")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(runtime_dir(root, "9.9.9").join("bin/uf").exists());
    assert!(linked_at(root, "9.9.9"));

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("✓ now using uf@9.9.9"), "{stdout}");
    assert!(stdout.contains(env!("CARGO_PKG_VERSION")), "{stdout}");
}

/// It reads no project, so a config uf cannot parse is not a reason to be
/// unable to install a uf that can.
#[test]
fn self_update_does_not_need_a_readable_project() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let release = root.join("releases");
    publish_as_latest(&release, "9.9.9");
    fs::write(root.join("uf.config.js"), "this is not javascript {{{").unwrap();

    let output = uf_in(root, Some(&release))
        .arg("--cwd")
        .arg(root)
        .arg("self-update")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// A release base with nothing at it fails, and leaves the machine as it was.
#[test]
fn self_update_reports_a_release_it_cannot_resolve() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let release = root.join("releases");
    fs::create_dir_all(&release).unwrap();

    let output = uf_in(root, Some(&release))
        .arg("self-update")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert!(!root.join("bin/uf").exists());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("VERSION"), "{stderr}");
}

/// A version named on the command line is the one installed, in either
/// spelling, and asking for the active one again changes and fetches nothing.
#[test]
fn self_update_installs_the_version_it_is_given() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let release = root.join("releases");
    publish(&release, "1.0.0");
    publish_as_latest(&release, "2.0.0");

    succeeds(root, &release, &["self-update", "uf@1.0.0"]);
    assert!(linked_at(root, "1.0.0"));
    assert!(!runtime_dir(root, "2.0.0").exists());

    let output = uf_in(root, Some(Path::new("/nowhere-at-all")))
        .args(["self-update", "1.0.0"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "switching to the active version needed the network:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("already active"), "{stdout}");
    assert_eq!(previous_version(root), None, "nothing was replaced");
}

/// `--check` answers the question and changes nothing: no download, no link,
/// no record.
#[test]
fn self_update_check_says_whether_a_newer_release_exists_and_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let release = root.join("releases");
    publish(&release, "1.0.0");
    publish_as_latest(&release, "2.0.0");
    succeeds(root, &release, &["self-update", "1.0.0"]);

    let stdout = succeeds(root, &release, &["self-update", "--check"]);
    assert!(stdout.contains("uf@1.0.0"), "{stdout}");
    assert!(stdout.contains("uf@2.0.0 is newer"), "{stdout}");
    assert!(linked_at(root, "1.0.0"));
    assert!(
        !runtime_dir(root, "2.0.0").exists(),
        "--check downloaded the release it was only asked about"
    );
    assert_eq!(previous_version(root), None);

    succeeds(root, &release, &["self-update"]);
    let stdout = succeeds(root, &release, &["self-update", "--check"]);
    assert!(
        stdout.contains("uf@2.0.0 is the newest release"),
        "{stdout}"
    );
}

/// The rollback the issue asks for: back to the version the update replaced,
/// from the store, with no release host to reach — and back again.
#[test]
fn self_update_rollback_returns_to_the_replaced_version_without_the_network() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let release = root.join("releases");
    publish(&release, "1.0.0");
    publish_as_latest(&release, "2.0.0");
    succeeds(root, &release, &["self-update", "1.0.0"]);

    let stdout = succeeds(root, &release, &["self-update"]);
    assert!(linked_at(root, "2.0.0"));
    assert!(
        stdout.contains("uf@1.0.0"),
        "the update did not say what it kept:\n{stdout}"
    );
    assert_eq!(previous_version(root).as_deref(), Some("1.0.0\n"));

    let nowhere = Path::new("/nowhere-at-all");
    let stdout = succeeds(root, nowhere, &["self-update", "--rollback"]);
    assert!(stdout.contains("rolled back to uf@1.0.0"), "{stdout}");
    assert!(linked_at(root, "1.0.0"));
    let active = fs::read_to_string(root.join("state/uniflowed/active-runtime.json")).unwrap();
    let active: serde_json::Value = serde_json::from_str(&active).unwrap();
    assert_eq!(active["version"], "1.0.0");

    // Rolling back records what it rolled back from, so a second rollback
    // undoes the first.
    succeeds(root, nowhere, &["self-update", "--rollback"]);
    assert!(linked_at(root, "2.0.0"));
    assert_eq!(previous_version(root).as_deref(), Some("1.0.0\n"));
}

/// With nothing recorded there is nothing to return to, and saying so changes
/// nothing.
#[test]
fn self_update_rollback_refuses_when_nothing_was_replaced() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let release = root.join("releases");
    publish(&release, "1.0.0");
    succeeds(root, &release, &["self-update", "1.0.0"]);

    let output = uf_in(root, Some(&release))
        .args(["self-update", "--rollback"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("no version to roll back to"), "{stderr}");
    assert!(linked_at(root, "1.0.0"));
}

/// A rollback that would need the network is not a rollback: when the replaced
/// runtime is gone from the store, it refuses and names the command that
/// downloads it.
#[test]
fn self_update_rollback_refuses_a_version_the_store_no_longer_holds() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let release = root.join("releases");
    publish(&release, "1.0.0");
    publish(&release, "2.0.0");
    succeeds(root, &release, &["self-update", "1.0.0"]);
    succeeds(root, &release, &["self-update", "2.0.0"]);
    fs::remove_dir_all(runtime_dir(root, "1.0.0")).unwrap();

    let output = uf_in(root, Some(&release))
        .args(["self-update", "--rollback"])
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("uf self-update 1.0.0"), "{stderr}");
    assert!(linked_at(root, "2.0.0"));
}

/// An archive that does not match its checksum installs nothing: the active
/// version stays active, nothing is recorded, and no staged half of it is left.
#[test]
fn self_update_refuses_an_archive_whose_checksum_does_not_match() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let release = root.join("releases");
    publish(&release, "1.0.0");
    publish(&release, "2.0.0");
    succeeds(root, &release, &["self-update", "1.0.0"]);
    let target = target_triple();
    fs::write(
        release.join(format!("2.0.0/uf-{target}.tar.gz.sha256")),
        format!("{}  uf-{target}.tar.gz\n", "0".repeat(64)),
    )
    .unwrap();

    let output = uf_in(root, Some(&release))
        .args(["self-update", "2.0.0"])
        .output()
        .unwrap();

    assert!(!output.status.success(), "a tampered archive was installed");
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("checksum mismatch"), "{stderr}");
    assert!(linked_at(root, "1.0.0"));
    assert!(!runtime_dir(root, "2.0.0").exists());
    assert_eq!(previous_version(root), None);
    assert_no_leftovers(root, "after a checksum mismatch");
}

/// `curl … | sh` switches versions too, so it records what it replaced, and
/// `uf self-update --rollback` reads that record.
#[test]
fn the_installer_records_what_it_replaced_for_a_rollback() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let release = root.join("releases");
    publish(&release, "1.0.0");
    publish(&release, "2.0.0");

    assert!(
        installer(root, &release, "1.0.0")
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(previous_version(root), None);
    assert!(
        installer(root, &release, "2.0.0")
            .status()
            .unwrap()
            .success()
    );
    assert!(linked_at(root, "2.0.0"));
    assert_eq!(previous_version(root).as_deref(), Some("1.0.0\n"));

    succeeds(
        root,
        Path::new("/nowhere-at-all"),
        &["self-update", "--rollback"],
    );
    assert!(linked_at(root, "1.0.0"));
}

/// The installer killed after every command that changes the filesystem, in
/// both of the switches it makes: to a version that is not installed, and over
/// the version that is active. Each time, every name must still run a whole
/// binary, and running the installer again must finish the job and sweep what
/// the killed one left.
///
/// The second switch is the one the installer used to break: it removed the
/// active runtime and then renamed the new one in, so a kill between the two
/// left `uf`, `ufr` and `ufx` pointing at nothing.
#[test]
fn an_installer_killed_at_any_step_leaves_every_name_running() {
    let dir = tempfile::tempdir().unwrap();
    let releases = dir.path().join("releases");
    let rebuilt = dir.path().join("rebuilt");
    publish(&releases, "1.0.0");
    publish(&releases, "2.0.0");
    publish_with(&rebuilt, "1.0.0", another_real_executable());

    for (switch, release, version) in [
        ("a reinstall of the active version", &rebuilt, "1.0.0"),
        ("an upgrade", &releases, "2.0.0"),
    ] {
        let mut kills = 0;
        for kill_after in 1.. {
            assert!(kill_after < 100, "{switch}: the installer never finished");
            let case = tempfile::tempdir().unwrap();
            let root = case.path();
            assert!(
                installer(root, &releases, "1.0.0")
                    .status()
                    .unwrap()
                    .success()
            );

            let path = killing_path(&root.join("kill"), kill_after);
            let status = in_own_group(installer(root, release, version).env("PATH", path));
            if status.success() {
                break;
            }
            kills += 1;
            let context = format!("{switch}, killed after step {kill_after}");
            assert_nothing_half_done(root, &context);

            assert!(
                installer(root, release, version)
                    .status()
                    .unwrap()
                    .success(),
                "{context}: installing again failed"
            );
            assert!(
                linked_at(root, version),
                "{context}: installing again did not finish"
            );
            assert_nothing_half_done(root, &context);
            assert_no_leftovers(root, &context);
            let expected = (version == "2.0.0").then(|| "1.0.0\n".to_owned());
            assert_eq!(previous_version(root), expected, "{context}");
        }
        assert!(kills > 5, "{switch}: only {kills} kills landed");
    }
}

/// `uf self-update` killed while its installer runs — `uf` and `sh` together,
/// the way Ctrl-C or a closed terminal takes a process group — has not switched
/// anything yet, so every name still runs the version that was active, and the
/// same command finishes the update.
#[test]
fn self_update_killed_while_it_installs_leaves_the_active_version() {
    let dir = tempfile::tempdir().unwrap();
    let release = dir.path().join("releases");
    publish(&release, "1.0.0");
    publish(&release, "2.0.0");

    let mut kills = 0;
    for kill_after in 1.. {
        assert!(kill_after < 100, "the update never finished");
        let case = tempfile::tempdir().unwrap();
        let root = case.path();
        succeeds(root, &release, &["self-update", "1.0.0"]);

        let path = killing_path(&root.join("kill"), kill_after);
        let status = in_own_group(
            uf_process(root, &release)
                .args(["self-update", "2.0.0"])
                .env("PATH", path),
        );
        if status.success() {
            break;
        }
        kills += 1;
        let context = format!("uf self-update killed after step {kill_after}");
        assert_nothing_half_done(root, &context);
        assert!(
            linked_at(root, "1.0.0"),
            "{context}: uf switched before it had a runtime"
        );

        succeeds(root, &release, &["self-update", "2.0.0"]);
        assert!(linked_at(root, "2.0.0"), "{context}");
        assert_eq!(
            previous_version(root).as_deref(),
            Some("1.0.0\n"),
            "{context}"
        );
        assert_no_leftovers(root, &context);
    }
    assert!(kills > 3, "only {kills} kills landed");
}

/// Reinstalling the runtime that is running must work, because repairing the
/// active version is exactly what `uf self-update` does when part of it has
/// gone missing.
///
/// On Linux, writing to a file that is being executed is `ETXTBSY` and GNU tar
/// does not recover from it, so an installer that unpacked over the runtime
/// directory would fail here with `Cannot open: Text file busy`. macOS unlinks
/// first and would pass either way, which is exactly why this is a test rather
/// than a thing anyone would have noticed by hand.
#[test]
fn self_update_over_a_runtime_that_is_running() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let release = root.join("releases");
    publish_as_latest(&release, "9.9.9");
    succeeds(root, &release, &["self-update"]);

    // The installed `uf` is a copy of `sleep`, so this is a real process
    // executing the exact file the next install is about to replace.
    let mut running = Command::new(runtime_dir(root, "9.9.9").join("bin/uf"))
        .arg("30")
        .spawn()
        .unwrap();
    // A runtime missing a binary is not an installed one, so the update below
    // reinstalls it rather than reporting it already active.
    fs::remove_file(runtime_dir(root, "9.9.9").join("bin/ufx")).unwrap();

    let output = uf_in(root, Some(&release))
        .arg("self-update")
        .output()
        .unwrap();
    running.kill().ok();
    running.wait().ok();

    assert!(
        output.status.success(),
        "reinstalling over a running runtime failed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(runtime_dir(root, "9.9.9").join("bin/ufx").exists());
    assert!(linked_at(root, "9.9.9"));
}
