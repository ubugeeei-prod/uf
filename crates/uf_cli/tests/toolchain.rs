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
//! Every case installs into its own `UF_INSTALL_ROOT` and `UF_BIN_DIR`, so the
//! suite cannot touch the developer's own `~/.local`.
#![cfg(unix)]

mod support;

use std::fs;
use std::path::Path;
use std::process::Command;

use assert_cmd::Command as TestCommand;
use support::uf;

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
    let target = target_triple();
    let staging = base.join(format!("staging-{version}"));
    let bin = staging.join("bin");
    fs::create_dir_all(&bin).unwrap();
    for name in ["uf", "ufr", "ufx"] {
        let path = bin.join(name);
        fs::copy(a_real_executable(), &path).unwrap();
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

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;

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
fn runtime_dir(root: &Path, version: &str) -> std::path::PathBuf {
    root.join("share/uf/runtimes").join(format!("uf@{version}"))
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
    assert_eq!(
        fs::read_link(root.join("bin/uf")).unwrap(),
        runtime_dir(root, "9.9.9").join("bin/uf")
    );

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

/// Reinstalling the version that is currently running must work, because that
/// is what `uf self-update` does on a machine that is already up to date.
///
/// On Linux, writing to a file that is being executed is `ETXTBSY` and GNU tar
/// does not recover from it, so an installer that unpacked over the runtime
/// directory would fail here with `Cannot open: Text file busy` — in the most
/// ordinary invocation there is. macOS unlinks first and would pass either
/// way, which is exactly why this is a test rather than a thing anyone would
/// have noticed by hand.
#[test]
fn self_update_over_a_runtime_that_is_running() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let release = root.join("releases");
    publish_as_latest(&release, "9.9.9");

    assert!(
        uf_in(root, Some(&release))
            .arg("self-update")
            .output()
            .unwrap()
            .status
            .success()
    );

    // The installed `uf` is a copy of `sleep`, so this is a real process
    // executing the exact file the next install is about to write.
    let mut running = Command::new(runtime_dir(root, "9.9.9").join("bin/uf"))
        .arg("30")
        .spawn()
        .unwrap();

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
    assert!(runtime_dir(root, "9.9.9").join("bin/uf").exists());
    assert_eq!(
        fs::read_link(root.join("bin/uf")).unwrap(),
        runtime_dir(root, "9.9.9").join("bin/uf")
    );
}
