//! `uf add`, `uf remove`, `uf dedupe` and `uf link`, against every package
//! manager uf drives rather than only npm.
//!
//! `dependencies.rs` runs npm, because npm is wherever Node is. The table in
//! `crates/uf_pm/src/command.rs` has more rows than that, and a row nobody ran
//! is a row nobody knows is right: pnpm, bun and Yarn 4 had been checked by
//! hand, and Yarn 1 had unit tests and spot checks. So each test here runs one
//! command against every row — npm, pnpm 10, pnpm 12, Yarn 1, Yarn 4 and bun —
//! on a fresh project each time, and asserts on the files the manager wrote
//! rather than on what either program printed. A failure names every row that
//! failed, not only the first.
//!
//! # Where the managers come from
//!
//! npm comes with Node, and bun is on `PATH` for the Bun host tests. pnpm and
//! both Yarns are installed by `tools/ci/install-package-managers.sh DIR`, one
//! release per directory, and `UF_TEST_PACKAGE_MANAGERS=DIR` says where. Yarn 1
//! and Yarn 4 are both `yarn`, so each row puts its own directory first on
//! `PATH` for that row alone.
//!
//! Without the variable these tests fail rather than skip, like the Bun and
//! Deno ones: CI installs the managers in every job that runs the suite, and
//! `tools/ci/workspace-suite-runtimes.sh` fails the build when a job does not.
//! `UF_ALLOW_FIXTURE_SKIP=1` runs the npm and bun rows only.
//!
//! # Nothing reaches the machine's own state
//!
//! Every manager keeps something global, and `uf link` writes to it: npm's
//! prefix, pnpm's home, Yarn 1's link folder under `XDG_DATA_HOME`, bun's
//! install directory. Each project gets a home of its own inside the test's
//! temporary directory, and every variable those managers read is pointed into
//! it. `CI` and `GITHUB_ACTIONS` are removed too: pnpm and Yarn 4 each refuse
//! to write a lockfile when they think they are in CI, which is a setting of
//! the job and not a behaviour of uf.
//!
//! None of this needs the network. Every dependency is a directory beside the
//! project.

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Output;

use serde_json::Value;
use support::{bun_ready, uf_with_tools};

/// A release uf can be pointed at, one per row of the table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Manager {
    Npm,
    Pnpm10,
    Pnpm12,
    Yarn1,
    Yarn4,
    Bun,
}

impl Manager {
    const ALL: [Self; 6] = [
        Self::Npm,
        Self::Pnpm10,
        Self::Pnpm12,
        Self::Yarn1,
        Self::Yarn4,
        Self::Bun,
    ];

    /// What a failure calls it.
    const fn name(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm10 => "pnpm 10",
            Self::Pnpm12 => "pnpm 12",
            Self::Yarn1 => "Yarn 1",
            Self::Yarn4 => "Yarn 4",
            Self::Bun => "bun",
        }
    }

    /// The `packageManager` field that makes uf choose it.
    ///
    /// For npm and bun the version is only the field's syntax: the one that
    /// runs is the one on `PATH`, and neither checks the field against itself.
    const fn pin(self) -> &'static str {
        match self {
            Self::Npm => "npm@11.0.0",
            Self::Pnpm10 => "pnpm@10.34.5",
            Self::Pnpm12 => "pnpm@12.4.2",
            Self::Yarn1 => "yarn@1.22.22",
            Self::Yarn4 => "yarn@4.18.0",
            Self::Bun => "bun@1.3.0",
        }
    }

    /// What uf's `manager` row calls it.
    const fn label(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm10 | Self::Pnpm12 => "pnpm",
            Self::Yarn1 => "yarn-classic",
            Self::Yarn4 => "yarn-berry",
            Self::Bun => "bun",
        }
    }

    /// The program uf spawns for it.
    const fn program(self) -> &'static str {
        match self {
            Self::Npm => "npm",
            Self::Pnpm10 | Self::Pnpm12 => "pnpm",
            Self::Yarn1 | Self::Yarn4 => "yarn",
            Self::Bun => "bun",
        }
    }

    /// The lockfile it writes.
    const fn lockfile(self) -> &'static str {
        match self {
            Self::Npm => "package-lock.json",
            Self::Pnpm10 | Self::Pnpm12 => "pnpm-lock.yaml",
            Self::Yarn1 | Self::Yarn4 => "yarn.lock",
            Self::Bun => "bun.lock",
        }
    }

    /// Its directory under `UF_TEST_PACKAGE_MANAGERS`, or `None` for the two
    /// that are already on `PATH`.
    const fn installed_as(self) -> Option<&'static str> {
        match self {
            Self::Npm | Self::Bun => None,
            Self::Pnpm10 => Some("pnpm-10"),
            Self::Pnpm12 => Some("pnpm-12"),
            Self::Yarn1 => Some("yarn-1"),
            Self::Yarn4 => Some("yarn-4"),
        }
    }
}

/// A manager, and the directory to put in front of `PATH` to get it.
struct Row {
    manager: Manager,
    bin: Option<PathBuf>,
}

/// Every row this machine can run, failing when it should be able to run more.
fn rows() -> Vec<Row> {
    let installed = std::env::var_os("UF_TEST_PACKAGE_MANAGERS").map(PathBuf::from);
    if installed.is_none() {
        assert!(
            std::env::var_os("UF_ALLOW_FIXTURE_SKIP").is_some(),
            "these tests need pnpm and both Yarns and UF_TEST_PACKAGE_MANAGERS is not set: run \
             `tools/ci/install-package-managers.sh DIR` and set UF_TEST_PACKAGE_MANAGERS=DIR"
        );
        eprintln!("skipping the pnpm and Yarn rows: UF_TEST_PACKAGE_MANAGERS is not set");
    }
    let bun = bun_ready();
    Manager::ALL
        .into_iter()
        .filter_map(|manager| match manager.installed_as() {
            None if manager == Manager::Bun && !bun => None,
            None => Some(Row { manager, bin: None }),
            Some(directory) => {
                let installed = installed.as_ref()?;
                // The script's link to the program itself, not npm's
                // `node_modules/.bin`: pnpm 12's entry there is a placeholder
                // only a shell can start.
                let bin = installed.join(directory).join("bin");
                assert!(
                    bin.join(manager.program()).exists(),
                    "{} is not in {}: run `tools/ci/install-package-managers.sh {}`",
                    manager.name(),
                    bin.display(),
                    installed.display()
                );
                Some(Row {
                    manager,
                    bin: Some(bin),
                })
            }
        })
        .collect()
}

/// Run `test` on a fresh fixture for every row, and fail once, naming every
/// row that failed and why.
fn each_row(test: impl Fn(&Fixture) -> Result<(), String>) {
    let mut failures = Vec::new();
    for row in rows() {
        let fixture = Fixture::new(row);
        if let Err(why) = test(&fixture) {
            failures.push(format!("── {} ──\n{why}", fixture.manager.name()));
        }
    }
    assert!(failures.is_empty(), "\n{}", failures.join("\n\n"));
}

const CONFIG: &str = "// @flow\nimport { defineConfig } from \"@uniflowed/config\";\n\
                      export default defineConfig({ app: { router: { enabled: false } } });\n";

/// Everything Yarn 4 is told, in both projects: install into `node_modules`
/// rather than Plug'n'Play, keep the cache in the project, and let an install
/// write the lockfile.
const YARNRC: &str = "nodeLinker: node-modules\nenableGlobalCache: false\n\
                      enableTelemetry: false\nenableImmutableInstalls: false\n";

/// Two projects for one manager, and a home for that manager.
///
/// ```text
/// app/                  the project the commands run in
///   vendor/tiny/        a package to add and remove
///   vendor/managers-lib/  managers-lib 1.0.0, the release the app declares
/// lib/                  managers-lib 2.0.0, the checkout that gets linked
/// home/                 HOME and every manager's global directory
/// ```
struct Fixture {
    dir: tempfile::TempDir,
    manager: Manager,
    bin: Option<PathBuf>,
}

impl Fixture {
    fn new(row: Row) -> Self {
        let fixture = Self {
            dir: tempfile::tempdir().unwrap(),
            manager: row.manager,
            bin: row.bin,
        };
        let app = fixture.app();
        let lib = fixture.lib();
        fixture.project(&app, "managers-app", "1.0.0");
        fixture.project(&lib, "managers-lib", "2.0.0");
        package(&app.join("vendor/tiny"), "tiny", "1.2.3");
        package(&app.join("vendor/managers-lib"), "managers-lib", "1.0.0");
        let home = fixture.home();
        for directory in ["npm-global/lib/node_modules", "npm-global/bin", "pnpm/bin"] {
            fs::create_dir_all(home.join(directory)).unwrap();
        }
        fs::create_dir_all(fixture.root().join("tools/nothing")).unwrap();
        fixture
    }

    /// The temporary directory, as the filesystem itself names it.
    ///
    /// Canonical, because on macOS the temporary directory is reached through
    /// `/var`, which is a link to `/private/var`, and Yarn 1 writes a registry
    /// link as a relative path worked out from `XDG_DATA_HOME` as it was
    /// given. A link written from the `/var` spelling resolves one directory
    /// too high, and `yarn link NAME` then finds nothing registered.
    ///
    /// Under a UUID, because npm masks one wherever it prints it: with the
    /// fixture's `npm_config_prefix` under this directory, `npm root --global`
    /// answers `…/***/home/npm-global/lib/node_modules`, and uf has to put the
    /// segment back to find what `uf link` registered. A temporary directory
    /// alone never holds a UUID, so nothing here met npm's real answer before
    /// (ubugeeei-prod/uf#976).
    fn root(&self) -> PathBuf {
        fs::canonicalize(self.dir.path())
            .unwrap()
            .join("6d0b7f14-9c2a-4e35-8b7d-1f4a6c8e23b9")
    }

    fn app(&self) -> PathBuf {
        self.root().join("app")
    }

    fn lib(&self) -> PathBuf {
        self.root().join("lib")
    }

    fn home(&self) -> PathBuf {
        self.root().join("home")
    }

    /// A project this manager drives: a manifest that pins it, a uf config,
    /// and for Yarn 4 the two files that make a directory a Yarn project.
    ///
    /// Not `"private"`: Yarn 4 will not link a private package without a flag
    /// of its own, and a package somebody is developing to link elsewhere is
    /// one they mean to publish.
    fn project(&self, dir: &Path, name: &str, version: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("package.json"),
            format!(
                "{{\n  \"name\": \"{name}\",\n  \"version\": \"{version}\",\n  \
                 \"packageManager\": \"{}\"\n}}\n",
                self.manager.pin()
            ),
        )
        .unwrap();
        fs::write(dir.join("index.js"), "module.exports = 1;\n").unwrap();
        fs::write(dir.join("uf.config.js"), CONFIG).unwrap();
        if self.manager == Manager::Yarn4 {
            fs::write(dir.join(".yarnrc.yml"), YARNRC).unwrap();
            fs::write(dir.join("yarn.lock"), "").unwrap();
        }
    }

    /// `uf ARGS` in `cwd`, with this row's manager first on `PATH` and every
    /// global directory inside the fixture.
    fn uf(&self, cwd: &Path, args: &[&str]) -> Output {
        let home = self.home();
        let mut path: Vec<PathBuf> = self.bin.iter().cloned().collect();
        // pnpm 10 links into `PNPM_HOME` and pnpm 12 into `PNPM_HOME/bin`, and
        // both refuse to when that directory is not on `PATH`.
        path.push(home.join("pnpm"));
        path.push(home.join("pnpm/bin"));
        path.extend(std::env::split_paths(
            &std::env::var_os("PATH").unwrap_or_default(),
        ));
        let mut command = uf_with_tools(&self.root().join("tools"));
        if self.manager == Manager::Yarn1 {
            // Yarn 1's update check. Yarn 4 reads every `YARN_*` variable as a
            // setting of its own and stops at one it does not know — "Usage
            // Error: Unrecognized or legacy configuration settings found:
            // disableSelfUpdateCheck" — so it is set for Yarn 1 alone.
            command.env("YARN_DISABLE_SELF_UPDATE_CHECK", "true");
        }
        command
            .arg("--cwd")
            .arg(cwd)
            .args(["--color", "never"])
            .args(args)
            .env("PATH", std::env::join_paths(path).unwrap())
            .env("HOME", &home)
            .env("XDG_CONFIG_HOME", home.join(".config"))
            .env("XDG_DATA_HOME", home.join(".local/share"))
            .env("XDG_CACHE_HOME", home.join(".cache"))
            .env("XDG_STATE_HOME", home.join(".local/state"))
            .env("npm_config_cache", home.join("npm-cache"))
            .env("npm_config_prefix", home.join("npm-global"))
            .env("npm_config_update_notifier", "false")
            .env("PNPM_HOME", home.join("pnpm"))
            .env("YARN_CACHE_FOLDER", home.join("yarn-cache"))
            .env("YARN_GLOBAL_FOLDER", home.join("yarn-global"))
            .env("YARN_ENABLE_TELEMETRY", "0")
            .env("BUN_INSTALL", home.join("bun"))
            .env("BUN_INSTALL_CACHE_DIR", home.join("bun-cache"))
            .env_remove("CI")
            .env_remove("GITHUB_ACTIONS")
            .output()
            .unwrap()
    }
}

/// A package with nothing in it but a manifest and an entry point.
fn package(dir: &Path, name: &str, version: &str) {
    fs::create_dir_all(dir).unwrap();
    fs::write(
        dir.join("package.json"),
        format!("{{ \"name\": \"{name}\", \"version\": \"{version}\" }}\n"),
    )
    .unwrap();
    fs::write(dir.join("index.js"), "module.exports = 1;\n").unwrap();
}

/// What `uf` printed, both streams, for a failure message.
fn printed(output: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

/// Its stdout, when it succeeded.
fn succeeded(output: &Output, command: &str) -> Result<String, String> {
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    } else {
        Err(format!("`uf {command}` failed:\n{}", printed(output)))
    }
}

/// Its stderr, when it failed.
fn refused(output: &Output, command: &str) -> Result<String, String> {
    if output.status.success() {
        Err(format!(
            "`uf {command}` succeeded where it should have been refused:\n{}",
            printed(output)
        ))
    } else {
        Ok(String::from_utf8_lossy(&output.stderr).into_owned())
    }
}

fn ensure(holds: bool, why: impl FnOnce() -> String) -> Result<(), String> {
    if holds { Ok(()) } else { Err(why()) }
}

/// The value of one row of the report: `manager`, `command`.
fn report_row(stdout: &str, key: &str) -> String {
    stdout
        .lines()
        .map(str::trim_start)
        .find_map(|line| line.strip_prefix(key).filter(|rest| rest.starts_with(' ')))
        .map(|rest| rest.trim().to_owned())
        .unwrap_or_default()
}

fn manifest(dir: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(dir.join("package.json")).unwrap()).unwrap()
}

/// The version of `name` that `node_modules` holds, links followed.
fn installed(dir: &Path, name: &str) -> Option<String> {
    let source =
        fs::read_to_string(dir.join("node_modules").join(name).join("package.json")).ok()?;
    let manifest: Value = serde_json::from_str(&source).ok()?;
    manifest["version"].as_str().map(ToOwned::to_owned)
}

/// Where `node_modules/NAME` leads, when it is a link rather than a directory.
fn linked(dir: &Path, name: &str) -> Option<PathBuf> {
    let entry = dir.join("node_modules").join(name);
    let metadata = fs::symlink_metadata(&entry).ok()?;
    metadata
        .file_type()
        .is_symlink()
        .then(|| fs::canonicalize(&entry).ok())
        .flatten()
}

/// `uf add` records the package, writes the manager's own lockfile and
/// installs it; `uf remove` takes it back out of both the manifest and
/// `node_modules`. And the report names the manager that did it.
#[test]
fn add_and_remove_change_the_manifest_the_lockfile_and_node_modules() {
    each_row(|fixture| {
        let app = fixture.app();
        let added = succeeded(
            &fixture.uf(&app, &["add", "./vendor/tiny"]),
            "add ./vendor/tiny",
        )?;
        ensure(
            report_row(&added, "manager") == fixture.manager.label(),
            || {
                format!(
                    "the report names another manager than {}:\n{added}",
                    fixture.manager.label()
                )
            },
        )?;
        ensure(manifest(&app)["dependencies"]["tiny"].is_string(), || {
            format!("tiny is not in dependencies:\n{:#}", manifest(&app))
        })?;
        ensure(app.join(fixture.manager.lockfile()).is_file(), || {
            format!("no {} was written:\n{added}", fixture.manager.lockfile())
        })?;
        ensure(installed(&app, "tiny").as_deref() == Some("1.2.3"), || {
            format!("node_modules/tiny is not tiny 1.2.3:\n{added}")
        })?;

        let removed = succeeded(&fixture.uf(&app, &["remove", "tiny"]), "remove tiny")?;
        ensure(manifest(&app)["dependencies"].get("tiny").is_none(), || {
            format!("tiny is still declared:\n{:#}", manifest(&app))
        })?;
        ensure(
            fs::symlink_metadata(app.join("node_modules/tiny")).is_err(),
            || format!("node_modules/tiny is still there:\n{removed}"),
        )
    });
}

/// `uf dedupe` is the manager's own dedupe where it has one. Yarn 1 and bun
/// have none, and the refusal names the manager and the command.
#[test]
fn dedupe_is_the_managers_own_or_a_refusal_that_names_it() {
    each_row(|fixture| {
        let app = fixture.app();
        succeeded(
            &fixture.uf(&app, &["add", "./vendor/tiny"]),
            "add ./vendor/tiny",
        )?;
        let output = fixture.uf(&app, &["dedupe"]);
        match fixture.manager {
            Manager::Yarn1 | Manager::Bun => {
                let stderr = refused(&output, "dedupe")?;
                ensure(
                    stderr.contains(&format!("{} has no `dedupe`", fixture.manager.label())),
                    || format!("the refusal does not name the manager:\n{stderr}"),
                )
            }
            _ => {
                let stdout = succeeded(&output, "dedupe")?;
                let command = report_row(&stdout, "command");
                ensure(
                    command.contains(&format!("{} dedupe", fixture.manager.program())),
                    || format!("uf ran something other than the manager's dedupe:\n{stdout}"),
                )?;
                ensure(installed(&app, "tiny").as_deref() == Some("1.2.3"), || {
                    format!("the dedupe took tiny out of node_modules:\n{stdout}")
                })
            }
        }
    });
}

/// `uf link ../lib` puts a link to the checkout where the declared release
/// was, on the managers that link by path. Yarn 1 and bun link by name only,
/// and are refused before anything runs.
#[test]
fn link_a_directory_replaces_the_declared_release_with_a_link() {
    each_row(|fixture| {
        let app = fixture.app();
        succeeded(
            &fixture.uf(&app, &["add", "./vendor/managers-lib"]),
            "add ./vendor/managers-lib",
        )?;
        let output = fixture.uf(&app, &["link", "../lib"]);
        let lib = fs::canonicalize(fixture.lib()).unwrap();
        match fixture.manager {
            Manager::Yarn1 | Manager::Bun => {
                let stderr = refused(&output, "link ../lib")?;
                ensure(stderr.contains("link <dir>"), || {
                    format!("the refusal does not name the form:\n{stderr}")
                })?;
                ensure(linked(&app, "managers-lib").is_none(), || {
                    "a refused link still linked".to_owned()
                })
            }
            _ => {
                let stdout = succeeded(&output, "link ../lib")?;
                ensure(linked(&app, "managers-lib") == Some(lib.clone()), || {
                    format!(
                        "node_modules/managers-lib does not lead to {}:\n{stdout}",
                        lib.display()
                    )
                })?;
                // And the report says what happened, rather than that nothing did.
                ensure(
                    stdout.contains("linked managers-lib in")
                        && !stdout.contains("already up to date"),
                    || format!("the report does not say managers-lib was linked:\n{stdout}"),
                )
            }
        }
    });
}

/// `uf link` in a package registers it, and `uf link NAME` in a project links
/// what was registered — on the managers that keep a registry. Yarn 4 keeps
/// none and uf refuses both forms; pnpm 12 keeps none either, and refuses them
/// itself.
#[test]
fn link_by_name_links_the_package_uf_link_registered() {
    each_row(|fixture| {
        let app = fixture.app();
        succeeded(
            &fixture.uf(&app, &["add", "./vendor/managers-lib"]),
            "add ./vendor/managers-lib",
        )?;
        let registered = fixture.uf(&fixture.lib(), &["link"]);
        let by_name = fixture.uf(&app, &["link", "managers-lib"]);
        let lib = fs::canonicalize(fixture.lib()).unwrap();
        match fixture.manager {
            Manager::Yarn4 => {
                let stderr = refused(&registered, "link")?;
                ensure(stderr.contains("yarn 2+ links by path"), || {
                    format!("the refusal does not say how Yarn 4 links:\n{stderr}")
                })?;
                let stderr = refused(&by_name, "link managers-lib")?;
                ensure(stderr.contains("link <name>"), || {
                    format!("the refusal does not name the form:\n{stderr}")
                })?;
            }
            Manager::Pnpm12 => {
                // pnpm refuses both itself, and uf adds what to run instead.
                for (output, command) in [(&registered, "link"), (&by_name, "link managers-lib")] {
                    let stderr = refused(output, command)?;
                    ensure(stderr.contains("uf link <path to the package>"), || {
                        format!("the refusal does not say what to run instead:\n{stderr}")
                    })?;
                    ensure(!stderr.contains(&format!("`uf {command}` again")), || {
                        format!("the refusal also says to run the refused form again:\n{stderr}")
                    })?;
                }
            }
            _ => {
                succeeded(&registered, "link")?;
                let stdout = succeeded(&by_name, "link managers-lib").map_err(|why| {
                    format!(
                        "{why}\n…after `uf link` in the package printed:\n{}",
                        printed(&registered)
                    )
                })?;
                ensure(
                    stdout.contains("linked managers-lib in")
                        && !stdout.contains("already up to date"),
                    || format!("the report does not say managers-lib was linked:\n{stdout}"),
                )?;
            }
        }
        // Where it was refused, whatever the app declared is still installed —
        // which on pnpm is itself a link, to `vendor/managers-lib` — so the
        // assertion is about the checkout, not about links in general.
        let found = linked(&app, "managers-lib");
        let reaches_the_checkout = found.as_deref() == Some(lib.as_path());
        let should = !matches!(fixture.manager, Manager::Yarn4 | Manager::Pnpm12);
        ensure(reaches_the_checkout == should, || {
            format!(
                "node_modules/managers-lib leads to {found:?}, and {} lead to {}:\n{}{}",
                if should { "should" } else { "should not" },
                lib.display(),
                printed(&registered),
                printed(&by_name)
            )
        })
    });
}

/// `uf unlink NAME` takes the link out and puts back the release the manifest
/// declares, on every manager: including pnpm 10, whose own `pnpm unlink`
/// answers "Nothing to unlink" about a link its `pnpm link` made, and bun,
/// which has no `unlink <name>`. A second `uf unlink` finds nothing to do and
/// says so.
#[test]
fn unlink_takes_the_link_out_and_puts_back_what_the_manifest_declares() {
    each_row(|fixture| {
        let app = fixture.app();
        let lib = fs::canonicalize(fixture.lib()).unwrap();
        succeeded(
            &fixture.uf(&app, &["add", "./vendor/managers-lib"]),
            "add ./vendor/managers-lib",
        )?;
        // Linked the way each manager links: by name where it keeps a
        // registry and links by nothing else, by path everywhere else.
        if matches!(fixture.manager, Manager::Yarn1 | Manager::Bun) {
            succeeded(&fixture.uf(&fixture.lib(), &["link"]), "link")?;
            succeeded(
                &fixture.uf(&app, &["link", "managers-lib"]),
                "link managers-lib",
            )?;
        } else {
            succeeded(&fixture.uf(&app, &["link", "../lib"]), "link ../lib")?;
        }
        ensure(linked(&app, "managers-lib") == Some(lib.clone()), || {
            "the link there was to take out was never made".to_owned()
        })?;

        let unlinked = succeeded(
            &fixture.uf(&app, &["unlink", "managers-lib"]),
            "unlink managers-lib",
        )?;
        ensure(linked(&app, "managers-lib") != Some(lib.clone()), || {
            format!("node_modules/managers-lib still leads to the checkout:\n{unlinked}")
        })?;
        ensure(
            installed(&app, "managers-lib").as_deref() == Some("1.0.0"),
            || format!("node_modules/managers-lib is not the declared 1.0.0:\n{unlinked}"),
        )?;
        ensure(
            unlinked.contains("unlinked managers-lib in")
                && unlinked.contains("managers-lib 1.0.0 again"),
            || format!("the report does not say what it did:\n{unlinked}"),
        )?;

        let again = succeeded(
            &fixture.uf(&app, &["unlink", "managers-lib"]),
            "unlink managers-lib, a second time",
        )?;
        ensure(again.contains("nothing to unlink"), || {
            format!("a second unlink did not say there was nothing to do:\n{again}")
        })
    });
}

/// `uf unlink` in a package removes what `uf link` registered for it, on every
/// manager that keeps a registry, and a second one finds nothing registered.
/// pnpm 12 keeps no registry, so there is never anything to remove; Yarn 4
/// keeps none either, and uf refuses before running anything.
#[test]
fn unlink_with_nothing_named_unregisters_what_uf_link_registered() {
    each_row(|fixture| {
        let lib = fixture.lib();
        match fixture.manager {
            Manager::Yarn4 => {
                let stderr = refused(&fixture.uf(&lib, &["unlink"]), "unlink")?;
                ensure(stderr.contains("keeps no registry"), || {
                    format!("the refusal does not say why:\n{stderr}")
                })
            }
            Manager::Pnpm12 => {
                let stdout = succeeded(&fixture.uf(&lib, &["unlink"]), "unlink")?;
                ensure(stdout.contains("nothing to unlink"), || {
                    format!("pnpm 12 registered nothing, and uf did not say so:\n{stdout}")
                })
            }
            _ => {
                succeeded(&fixture.uf(&lib, &["link"]), "link")?;
                let stdout = succeeded(&fixture.uf(&lib, &["unlink"]), "unlink")?;
                ensure(stdout.contains("unregistered managers-lib in"), || {
                    format!("the report does not say what it removed:\n{stdout}")
                })?;
                let again = succeeded(&fixture.uf(&lib, &["unlink"]), "unlink, a second time")?;
                ensure(
                    again.contains("nothing to unlink") && again.contains("not registered"),
                    || format!("a second unlink did not find the registration gone:\n{again}"),
                )
            }
        }
    });
}
