//! `uf env`: the per-repository toolchain.
//!
//! Nothing here reaches the network. Acquiring a real Node is exercised by
//! hand and by `uf_env`'s own tests over a staged directory; what these check
//! is the part a user meets — what the commands say, and that the store and
//! the roots are the ones the environment points them at rather than the
//! machine's.

mod support;

use std::fs;

use support::{assert_plain, uf};

/// A project with a toolchain, and a store and roots of its own.
fn project(dir: &std::path::Path, toolchain: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    fs::write(
        dir.join("uf.config.js"),
        format!(
            "// @flow\nimport {{ defineConfig }} from \"@uniflowed/config\";\n\n\
             export default defineConfig({{ env: {{ toolchain: {toolchain} }} }});\n"
        ),
    )
    .unwrap();
    (dir.join("store"), dir.join("roots"))
}

#[test]
fn env_list_reports_what_is_pinned_and_what_is_installed() {
    let dir = tempfile::tempdir().unwrap();
    let (store, roots) = project(dir.path(), r#"{ node: "24.14.0" }"#);

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["env", "list"])
        .env("UF_STORE", &store)
        .env("UF_ROOTS", &roots)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("node@24.14.0"), "{stdout}");
    assert!(stdout.contains("missing"), "{stdout}");
    assert!(stdout.contains("the store"), "{stdout}");
    assert_plain(&stdout);
}

/// A project that pins nothing is not an error. That is every project today,
/// and `uf env install` turning into a failure for them would be a change
/// nobody asked for.
#[test]
fn a_project_that_pins_nothing_is_told_so_and_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let (store, roots) = project(dir.path(), "{}");

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["env", "install"])
        .env("UF_STORE", &store)
        .env("UF_ROOTS", &roots)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("neither uf.config.js nor package.json engines declares"),
        "{stdout}"
    );
    assert!(!dir.path().join(".uniflowed/env/bin").exists());
}

/// The standard manifest field is enough to tell `uf env` what this project
/// wants, without repeating the same pin in uf.config.js.
#[test]
fn env_list_reports_exact_package_json_engines() {
    let dir = tempfile::tempdir().unwrap();
    let (store, roots) = project(dir.path(), "{}");
    fs::write(
        dir.path().join("package.json"),
        r#"{ "engines": { "node": "24.14.0", "npm": ">=10" } }"#,
    )
    .unwrap();

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["env", "list"])
        .env("UF_STORE", &store)
        .env("UF_ROOTS", &roots)
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("node@24.14.0"), "{stdout}");
    assert!(!stdout.contains("npm@"), "{stdout}");
    assert_plain(&stdout);
}

/// A range is refused, and the message says which tool and what was written.
#[test]
fn a_version_that_is_not_exact_is_refused_by_name() {
    let dir = tempfile::tempdir().unwrap();
    let (store, roots) = project(dir.path(), r#"{ node: "^24" }"#);

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["env", "install"])
        .env("UF_STORE", &store)
        .env("UF_ROOTS", &roots)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("node"), "{stderr}");
    assert!(stderr.contains("not an exact version"), "{stderr}");
}

/// A name uf does not install is refused with the list of names it does.
#[test]
fn a_tool_uf_does_not_install_is_refused_with_the_list() {
    let dir = tempfile::tempdir().unwrap();
    let (store, roots) = project(dir.path(), r#"{ cargo: "1.0.0" }"#);

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["env", "install"])
        .env("UF_STORE", &store)
        .env("UF_ROOTS", &roots)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains("node, bun, deno, npm, pnpm and yarn"),
        "{stderr}"
    );
}

/// Collection on an empty store says so rather than reporting a count of
/// nothing as if it had worked.
#[test]
fn gc_on_an_empty_store_says_there_is_nothing_to_collect() {
    let dir = tempfile::tempdir().unwrap();
    let (store, roots) = project(dir.path(), "{}");

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["env", "gc", "--dry-run"])
        .env("UF_STORE", &store)
        .env("UF_ROOTS", &roots)
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("nothing to collect"), "{stdout}");
    assert_plain(&stdout);
}

/// `uf env exec` before `uf env install` names the command that fixes it.
#[test]
fn exec_without_an_environment_names_the_command_that_makes_one() {
    let dir = tempfile::tempdir().unwrap();
    let (store, roots) = project(dir.path(), r#"{ node: "24.14.0" }"#);

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["env", "exec", "--", "node", "--version"])
        .env("UF_STORE", &store)
        .env("UF_ROOTS", &roots)
        .output()
        .unwrap();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("uf env install"), "{stderr}");
}

/// A project written with the keys ubugeeei-prod/uf#940 added.
fn declaring(dir: &std::path::Path, declarations: &str) {
    fs::write(
        dir.join("uf.config.js"),
        format!(
            "// @flow\nimport {{ defineConfig }} from \"@uniflowed/config\";\n\n\
             export default defineConfig({declarations});\n"
        ),
    )
    .unwrap();
}

/// `uf env list` names each tool with what it is for, and a prefix nothing has
/// resolved says so without fetching anything or writing `uf.lock`.
#[test]
fn env_list_names_each_tool_and_what_it_is_for() {
    let dir = tempfile::tempdir().unwrap();
    declaring(
        dir.path(),
        r#"{ runtime: "node@26.8.2", test: { runner: "bun@1.4" } }"#,
    );

    let output = uf()
        .arg("--cwd")
        .arg(dir.path())
        .args(["env", "list"])
        .env("UF_STORE", dir.path().join("store"))
        .env("UF_ROOTS", dir.path().join("roots"))
        // Nothing is served here, so a listing that fetched would fail.
        .env("UF_TOOL_INDEX_BASE", "file:///nothing/is/served/here")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("node@26.8.2  runtime, build runtime  missing"),
        "{stdout}"
    );
    assert!(
        stdout.contains("bun@1.4  test runtime  not locked yet"),
        "{stdout}"
    );
    assert!(!dir.path().join("uf.lock").exists());
    assert_plain(&stdout);
}

/// The publisher a fixture archive stands in for: an npm package whose one
/// executable prints its version.
///
/// Built here rather than checked in, and served over `file://`, so the whole
/// chain — resolve a prefix against a release list, lock it, fetch the archive,
/// check its digest, unpack, link, run — is exercised with no network, the way
/// `uf_env`'s own fixtures are.
fn publish_pnpm(root: &std::path::Path, version: &str) {
    use base64::Engine as _;
    use sha2::Digest as _;
    use std::os::unix::fs::PermissionsExt;

    let package = root.join("work/package");
    fs::create_dir_all(package.join("bin")).unwrap();
    fs::write(
        package.join("package.json"),
        format!(r#"{{ "name": "pnpm", "version": "{version}", "bin": {{ "pnpm": "bin/pnpm" }} }}"#),
    )
    .unwrap();
    let executable = package.join("bin/pnpm");
    fs::write(&executable, format!("#!/bin/sh\necho {version}\n")).unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o755)).unwrap();

    let archives = root.join("archives/pnpm/-");
    fs::create_dir_all(&archives).unwrap();
    let tarball = archives.join(format!("pnpm-{version}.tgz"));
    let packed = std::process::Command::new("tar")
        .arg("-czf")
        .arg(&tarball)
        .arg("-C")
        .arg(root.join("work"))
        .arg("package")
        .status()
        .unwrap();
    assert!(packed.success());
    let integrity = base64::engine::general_purpose::STANDARD
        .encode(sha2::Sha512::digest(fs::read(&tarball).unwrap()));
    fs::write(
        root.join(format!("archives/pnpm/{version}")),
        format!(r#"{{ "dist": {{ "integrity": "sha512-{integrity}" }} }}"#),
    )
    .unwrap();
}

/// A prefix resolves against the publisher's list and is locked in `uf.lock`;
/// `uf env install` installs what the lock says and links it; and the
/// toolchain-only `uf.lock` that leaves behind does not change which package
/// manager the project is detected as using.
#[test]
fn a_prefix_is_locked_installed_and_run_and_the_lock_does_not_vote() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    declaring(&project, r#"{ packageManager: "pnpm@12" }"#);
    fs::write(project.join("package.json"), r#"{ "name": "demo" }"#).unwrap();
    fs::write(project.join("package-lock.json"), "{}\n").unwrap();

    fs::create_dir_all(root.join("lists")).unwrap();
    fs::write(
        root.join("lists/pnpm"),
        r#"{ "versions": { "11.9.0": {}, "12.0.0": {}, "12.1.0": {}, "13.0.0-rc.1": {} } }"#,
    )
    .unwrap();
    publish_pnpm(root, "12.1.0");

    let run = |arguments: &[&str]| {
        uf().arg("--cwd")
            .arg(&project)
            .args(arguments)
            .env("UF_STORE", root.join("store"))
            .env("UF_ROOTS", root.join("roots"))
            .env("UF_ENVS", root.join("envs"))
            .env("UF_INDEX_CACHE", root.join("cache"))
            .env(
                "UF_TOOL_INDEX_BASE",
                format!("file://{}", root.join("lists").display()),
            )
            .env(
                "UF_TOOL_BASE",
                format!("file://{}", root.join("archives").display()),
            )
            .output()
            .unwrap()
    };

    let output = run(&["env", "update"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("pnpm@12  locked at 12.1.0"), "{stdout}");
    assert_eq!(
        fs::read_to_string(project.join("uf.lock")).unwrap(),
        "{\n  \"toolchain\": {\n    \"pnpm@12\": \"12.1.0\"\n  }\n}\n"
    );

    // `uf_env` wrote that file and `uf_pm` reads lockfiles to decide which
    // manager a project uses. They are held to one shape here: `packageManager`
    // decides, `package-lock.json` is the only other voice, and the toolchain
    // record in `uf.lock` is not a voice at all — neither an alternative nor a
    // reason to call the choice ambiguous.
    let output = run(&["inspect", "--json"]);
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    let detection = &value["engines"]["packageManagerDetection"];
    assert_eq!(detection["packageManager"], "pnpm", "{detection:#}");
    assert_eq!(
        detection["source"]["kind"], "config-override",
        "{detection:#}"
    );
    assert_eq!(detection["outcome"]["kind"], "unambiguous", "{detection:#}");
    let alternatives = detection["alternatives"]
        .as_array()
        .expect("alternatives is a list");
    assert_eq!(alternatives.len(), 1, "{detection:#}");
    assert_eq!(alternatives[0]["packageManager"], "npm", "{detection:#}");
    assert_eq!(
        alternatives[0]["source"]["lockfile"], "package-lock",
        "{detection:#}"
    );

    let output = run(&["env", "install"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("pnpm@12 (12.1.0)  package manager  installed"),
        "{stdout}"
    );

    let output = run(&["env", "exec", "--", "pnpm"]);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(String::from_utf8(output.stdout).unwrap(), "12.1.0\n");

    // And a second update, with nothing newer published, moves nothing.
    let output = run(&["env", "update"]);
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(
        stdout.contains("pnpm@12  12.1.0, already the newest"),
        "{stdout}"
    );
    assert!(stdout.contains("already locks the newest"), "{stdout}");
}

/// `packageManager` at a version is the manager uf starts: the release in the
/// store, found through the directory put in front of the manager's `PATH`,
/// and not whichever one the machine has. ubugeeei-prod/uf#940.
#[test]
fn a_versioned_package_manager_runs_from_the_store() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    declaring(&project, r#"{ packageManager: "pnpm@12.1.0" }"#);
    fs::write(project.join("package.json"), r#"{ "name": "demo" }"#).unwrap();
    let marks = fake_pnpm_in_store(root, "12.1.0");

    let output = uf()
        .arg("--cwd")
        .arg(&project)
        .arg("ls")
        .env("UF_STORE", root.join("store"))
        .env("UF_ENVS", root.join("envs"))
        .env("UF_ROOTS", root.join("roots"))
        .env("UF_INDEX_CACHE", root.join("cache"))
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    let marked = fs::read_to_string(&marks).unwrap_or_default();
    assert!(
        !marked.is_empty(),
        "the pnpm in the store did not run:\n{stderr}"
    );
    // Already in the store, so nothing was installed and nothing said.
    assert!(!stderr.contains("installing"), "{stderr}");
}

/// `uf exec --yes` fetches through the manager the config names, like every
/// other command that runs one: `packageManager` beats a `package-lock.json`,
/// and the pnpm that runs `dlx` is the release in the store.
#[test]
fn a_fetch_and_run_goes_through_the_package_manager_the_config_pins() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    declaring(&project, r#"{ packageManager: "pnpm@12.1.0" }"#);
    fs::write(project.join("package.json"), r#"{ "name": "demo" }"#).unwrap();
    fs::write(project.join("package-lock.json"), "{}\n").unwrap();
    let marks = fake_pnpm_in_store(root, "12.1.0");

    let output = uf()
        .arg("--cwd")
        .arg(&project)
        .args(["exec", "--yes", "cowsay", "hello"])
        .env("UF_STORE", root.join("store"))
        .env("UF_ENVS", root.join("envs"))
        .env("UF_ROOTS", root.join("roots"))
        .env("UF_INDEX_CACHE", root.join("cache"))
        .output()
        .unwrap();

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{stderr}");
    let marked = fs::read_to_string(&marks).unwrap_or_default();
    assert!(
        marked.contains("cowsay hello"),
        "the pnpm in the store did not fetch and run the package:\n{marked}\n{stderr}"
    );
}

/// A `pnpm` at `version` in the store under `root`, named the way the store
/// names an entry for this machine, that appends the arguments it was started
/// with to the file it returns.
fn fake_pnpm_in_store(root: &std::path::Path, version: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let os = match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    let bin = root.join(format!("store/pnpm-{version}-{os}-{arch}/bin"));
    fs::create_dir_all(&bin).unwrap();
    let marks = root.join("pnpm.log");
    fs::write(
        bin.join("pnpm"),
        format!("#!/bin/sh\necho \"$@\" >> '{}'\n", marks.display()),
    )
    .unwrap();
    fs::set_permissions(bin.join("pnpm"), fs::Permissions::from_mode(0o755)).unwrap();
    marks
}
