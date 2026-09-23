//! `uf editor install` and `uf editor setup`, end to end.
//!
//! Nothing here touches a real editor or the network. The editors' launchers
//! are stand-ins on `PATH` — `code` and `cursor` shell scripts that record the
//! arguments they were started with — and the release is a directory served
//! over `file://` through `UF_EDITOR_RELEASE_BASE`, which curl reads like any
//! other URL. So the download, the checksum check and the hand-off to the
//! editor are the real ones, and what is asserted is what reached the editor:
//! which file, with which flags, and nothing at all when the checksum is
//! wrong. Every case has its own home, data and config directories, so the
//! suite cannot touch the developer's own editors.
#![cfg(unix)]

mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use support::uf;

/// A machine: a home, an empty `PATH` directory for editor launchers, and a
/// release directory.
struct Machine {
    root: tempfile::TempDir,
}

impl Machine {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        for dir in ["home", "bin", "release", "data", "config"] {
            fs::create_dir_all(root.path().join(dir)).unwrap();
        }
        Self { root }
    }

    fn path(&self, relative: &str) -> PathBuf {
        self.root.path().join(relative)
    }

    /// A launcher named `name` that records its arguments and exits `status`.
    fn launcher(&self, name: &str, status: i32) -> PathBuf {
        let log = self.path(&format!("{name}.log"));
        let script = self.path(&format!("bin/{name}"));
        fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" >> '{}'\nexit {status}\n",
                log.display()
            ),
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        log
    }

    /// Publish `uf-vscode-<version>.vsix` with `contents`, and a `.sha256`
    /// stating `digest` (the real one when `None`).
    fn publish(&self, version: &str, contents: &[u8], digest: Option<&str>) {
        let dir = self.path(&format!("release/{version}"));
        fs::create_dir_all(&dir).unwrap();
        let name = format!("uf-vscode-{version}.vsix");
        fs::write(dir.join(&name), contents).unwrap();
        let real = hex(&Sha256::digest(contents));
        fs::write(
            dir.join(format!("{name}.sha256")),
            format!("{}  {name}\n", digest.unwrap_or(&real)),
        )
        .unwrap();
    }

    fn uf(&self, cwd: &Path) -> assert_cmd::Command {
        let mut command = uf();
        command
            .current_dir(cwd)
            .env("HOME", self.path("home"))
            .env("XDG_DATA_HOME", self.path("data"))
            .env("XDG_CONFIG_HOME", self.path("config"))
            .env(
                "UF_EDITOR_RELEASE_BASE",
                format!("file://{}", self.path("release").display()),
            )
            // The launchers, then what curl and sh need.
            .env(
                "PATH",
                format!("{}:/usr/bin:/bin", self.path("bin").display()),
            );
        command
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// A uf project with nothing else in it.
fn project() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("uf.config.js"), "export default {};\n").unwrap();
    dir
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn install_vscode_downloads_checks_and_hands_the_vsix_to_code() {
    let machine = Machine::new();
    let log = machine.launcher("code", 0);
    machine.publish("0.2.0", b"a vsix", None);

    let output = machine
        .uf(machine.root.path())
        .args(["editor", "install", "vscode", "--version", "uf@0.2.0"])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        stdout(&output),
        stderr(&output)
    );

    let args = fs::read_to_string(&log).unwrap();
    let lines = args.lines().collect::<Vec<_>>();
    assert_eq!(lines[0], "--install-extension");
    assert!(lines[1].ends_with("uf-vscode-0.2.0.vsix"), "{args}");
    assert_eq!(lines[2], "--force");
    assert!(stdout(&output).contains("verified"), "{}", stdout(&output));
}

#[test]
fn a_vsix_that_fails_its_checksum_never_reaches_the_editor() {
    let machine = Machine::new();
    let log = machine.launcher("code", 0);
    machine.publish("0.2.0", b"a vsix", Some(&"0".repeat(64)));

    let output = machine
        .uf(machine.root.path())
        .args(["editor", "install", "vscode", "--version", "0.2.0"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("does not match the checksum"),
        "{}",
        stderr(&output)
    );
    assert!(
        !log.exists(),
        "code was run for a file that failed its check"
    );
}

#[test]
fn a_release_without_the_extension_says_what_to_do_instead() {
    let machine = Machine::new();
    machine.launcher("code", 0);
    let output = machine
        .uf(machine.root.path())
        .args(["editor", "install", "vscode", "--version", "0.1.0"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("uf@0.1.0"), "{message}");
    assert!(message.contains("--vsix"), "{message}");
}

#[test]
fn cursor_is_installed_with_cursor_and_a_local_vsix_skips_the_download() {
    let machine = Machine::new();
    let log = machine.launcher("cursor", 0);
    let vsix = machine.path("mine.vsix");
    fs::write(&vsix, b"built here").unwrap();

    let output = machine
        .uf(machine.root.path())
        .args(["editor", "install", "cursor", "--vsix"])
        .arg(&vsix)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let args = fs::read_to_string(&log).unwrap();
    assert!(args.contains(&vsix.display().to_string()), "{args}");
    assert!(
        stdout(&output).contains("not checked"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn no_launcher_on_path_names_the_command_and_how_to_get_it() {
    let machine = Machine::new();
    let output = machine
        .uf(machine.root.path())
        .args(["editor", "install", "vscode", "--version", "0.2.0"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("`code` is not on PATH"), "{message}");
    assert!(message.contains("Shell Command"), "{message}");
}

#[test]
fn a_failing_editor_install_is_a_failure() {
    let machine = Machine::new();
    machine.launcher("code", 3);
    machine.publish("0.2.0", b"a vsix", None);
    let output = machine
        .uf(machine.root.path())
        .args(["editor", "install", "vscode", "--version", "0.2.0"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("--install-extension failed"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn install_neovim_writes_uf_lua_and_leaves_someone_elses_alone() {
    let machine = Machine::new();
    let target = machine.path("config/nvim/lua/uf.lua");

    let output = machine
        .uf(machine.root.path())
        .args(["editor", "install", "neovim"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        fs::read_to_string(&target)
            .unwrap()
            .starts_with("-- uf for Neovim")
    );

    fs::write(&target, "-- my own module\n").unwrap();
    let refused = machine
        .uf(machine.root.path())
        .args(["editor", "install", "nvim"])
        .output()
        .unwrap();
    assert!(!refused.status.success());
    assert!(stderr(&refused).contains("--force"), "{}", stderr(&refused));
    assert_eq!(fs::read_to_string(&target).unwrap(), "-- my own module\n");

    let forced = machine
        .uf(machine.root.path())
        .args(["editor", "install", "neovim", "--force"])
        .output()
        .unwrap();
    assert!(forced.status.success());
    assert!(
        fs::read_to_string(&target)
            .unwrap()
            .starts_with("-- uf for Neovim")
    );
}

#[test]
fn install_zed_writes_the_extension_sources_to_point_zed_at() {
    let machine = Machine::new();
    let output = machine
        .uf(machine.root.path())
        .args(["editor", "install", "zed"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let dir = machine.path("data/uf/editors/zed");
    for file in [
        "extension.toml",
        "Cargo.toml",
        "Cargo.lock",
        "src/lib.rs",
        "src/command.rs",
    ] {
        assert!(dir.join(file).is_file(), "{file} missing");
    }
    assert!(
        stdout(&output).contains("install dev extension"),
        "{}",
        stdout(&output)
    );
}

#[test]
fn install_dry_run_writes_nothing() {
    let machine = Machine::new();
    let log = machine.launcher("code", 0);
    for editor in ["vscode", "neovim", "zed"] {
        let output = machine
            .uf(machine.root.path())
            .args([
                "editor",
                "install",
                editor,
                "--dry-run",
                "--version",
                "0.2.0",
            ])
            .output()
            .unwrap();
        assert!(output.status.success(), "{editor}: {}", stderr(&output));
    }
    assert!(!log.exists());
    assert!(!machine.path("config/nvim").exists());
    assert!(!machine.path("data/uf").exists());
}

#[test]
fn setup_vscode_writes_the_settings_then_finds_them_there() {
    let machine = Machine::new();
    let project = project();

    let output = machine
        .uf(project.path())
        .args(["editor", "setup", "vscode"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let settings: serde_json::Value =
        json5::from_str(&fs::read_to_string(project.path().join(".vscode/settings.json")).unwrap())
            .unwrap();
    assert_eq!(
        settings["javascript.validate.enable"],
        serde_json::json!(false)
    );
    assert_eq!(
        settings["[javascript]"]["js/ts.validate.enabled"],
        serde_json::json!(false)
    );
    assert_eq!(
        settings["[javascriptreact]"]["editor.defaultFormatter"],
        serde_json::json!("uniflowed.uf")
    );
    let extensions = fs::read_to_string(project.path().join(".vscode/extensions.json")).unwrap();
    assert!(extensions.contains("uniflowed.uf"));
    assert!(
        stdout(&output).contains("+ "),
        "shows what it added: {}",
        stdout(&output)
    );

    let before = fs::read_to_string(project.path().join(".vscode/settings.json")).unwrap();
    let check = machine
        .uf(project.path())
        .args(["editor", "setup", "vscode", "--check"])
        .output()
        .unwrap();
    assert!(check.status.success(), "{}", stderr(&check));
    let again = machine
        .uf(project.path())
        .args(["editor", "setup", "vscode"])
        .output()
        .unwrap();
    assert!(again.status.success());
    assert_eq!(
        fs::read_to_string(project.path().join(".vscode/settings.json")).unwrap(),
        before
    );
}

#[test]
fn setup_keeps_what_the_project_set_and_its_comments() {
    let machine = Machine::new();
    let project = project();
    fs::create_dir_all(project.path().join(".vscode")).unwrap();
    let mine = "{\n  // we want TypeScript's errors\n  \"javascript.validate.enable\": true\n}\n";
    fs::write(project.path().join(".vscode/settings.json"), mine).unwrap();

    let output = machine
        .uf(project.path())
        .args(["editor", "setup", "vscode"])
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", stderr(&output));
    let after = fs::read_to_string(project.path().join(".vscode/settings.json")).unwrap();
    assert!(after.contains("// we want TypeScript's errors"), "{after}");
    let value: serde_json::Value = json5::from_str(&after).unwrap();
    assert_eq!(value["javascript.validate.enable"], serde_json::json!(true));
    assert!(stdout(&output).contains("kept"), "{}", stdout(&output));
}

#[test]
fn setup_check_fails_until_the_settings_are_written_and_writes_nothing() {
    let machine = Machine::new();
    let project = project();
    let output = machine
        .uf(project.path())
        .args(["editor", "setup", "zed", "--check"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(!project.path().join(".zed").exists());
}

#[test]
fn setup_refuses_a_settings_file_it_cannot_read() {
    let machine = Machine::new();
    let project = project();
    fs::create_dir_all(project.path().join(".zed")).unwrap();
    fs::write(project.path().join(".zed/settings.json"), "{ not json").unwrap();
    let output = machine
        .uf(project.path())
        .args(["editor", "setup", "zed"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert_eq!(
        fs::read_to_string(project.path().join(".zed/settings.json")).unwrap(),
        "{ not json"
    );
}

#[test]
fn setup_needs_a_uf_project() {
    let machine = Machine::new();
    let output = machine
        .uf(machine.root.path())
        .args(["editor", "setup", "helix"])
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("uf.config.js"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn setup_helix_and_the_manual_editors() {
    let machine = Machine::new();
    let project = project();
    let helix = machine
        .uf(project.path())
        .args(["editor", "setup", "helix"])
        .output()
        .unwrap();
    assert!(helix.status.success(), "{}", stderr(&helix));
    assert!(
        fs::read_to_string(project.path().join(".helix/languages.toml"))
            .unwrap()
            .contains("language-servers = [\"uf\"]")
    );

    let jetbrains = machine
        .uf(project.path())
        .args(["editor", "setup", "jetbrains"])
        .output()
        .unwrap();
    assert!(jetbrains.status.success());
    assert!(
        stdout(&jetbrains).contains("TypeScript language service"),
        "{}",
        stdout(&jetbrains)
    );
}
