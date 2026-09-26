//! `uf env doctor`: whether this machine has what this project runs on.
//!
//! # What it looks at
//!
//! The tools the project itself names — `runtime`, `build.runtime`,
//! `test.runtime`, `packageManager`, `env.toolchain`, an exact
//! `package.json#engines` — and, where it names none, the runtime and package
//! manager uf falls back to. Each is one row: found or not, at which release,
//! and what it is for. Only a row with something wrong says what to do.
//!
//! It used to report `rustc`, `cargo`, `nix`, `git` and `bun` — the tools uf
//! is *built* with — on every machine and for every project, which told a
//! reader about uf's repository and nothing about theirs.
//!
//! # What it does not do
//!
//! It fetches nothing, installs nothing and writes nothing: it reads
//! `uf.lock` the way `uf env list` does, so a prefix nothing has locked is
//! reported as such rather than resolved. Where the store is, where the links
//! go and which key declared each tool are behind `--verbose`, and all of it
//! is in `--json`.
//!
//! It exits `0` whatever it finds, including over a `uf.config.js` it cannot
//! read: a doctor is what somebody runs when the project is the thing that is
//! broken, and it has to keep answering then. A script reads `ok` in `--json`.

use camino::{Utf8Path, Utf8PathBuf};
use serde::Serialize;
use serde_json::json;
use uf_config::{ResolvedConfig, load_config};
use uf_env::toolchain::{Declared, Lookup, Publishers, Resolution};
use uf_pm::{DetectionOptions, PackageManager, detect_package_manager_with};
use uf_term::{Status, StatusRow};

use crate::commands::vite::{find_program, resolve_host};
use crate::support::{command_output, plural, project_label};
use crate::ui::Ui;

/// What to do about a declared release the store does not have yet.
///
/// One sentence for "not locked" and "not downloaded" alike: the command that
/// answers both is the same, and the rows above it already say which is which.
const INSTALL_FIX: &str = "`uf env install` installs what is missing now; otherwise the first command that needs a \
     tool installs it";

/// The role of the row that stands in for a `uf.config.js` nobody could read.
const CONFIG_ROLE: &str = "configuration";

/// How one row came out.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
enum Health {
    /// Present, at the release the project wants.
    Ok,
    /// A command will still run, but something will happen first — a
    /// download, a lock — or something deserves a look.
    Warn,
    /// A command that needs this will fail.
    Error,
}

impl Health {
    fn status(self) -> Status {
        match self {
            Self::Ok => Status::Success,
            Self::Warn => Status::Warn,
            Self::Error => Status::Error,
        }
    }
}

/// One thing the doctor looked at.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct Finding {
    /// What was looked at, as the project writes it: `node@26`, `npm`.
    name: String,
    /// What it is for: `runtime, build runtime`.
    role: String,
    /// How it came out.
    status: Health,
    /// What was found: the release, or why there is none.
    found: String,
    /// Why this tool, when the project did not name it: `uf's default`.
    reason: Option<String>,
    /// What to do about it, only when something is wrong.
    fix: Option<String>,
    /// The keys that declared it: `build.runtime`, `package.json#engines.node`.
    declared_by: Vec<String>,
    /// Where it is: the executable on `PATH`, or the store entry.
    location: Option<Utf8PathBuf>,
}

/// Where uf keeps what it installs, for `--verbose` and `--json`.
#[derive(Debug, Default)]
struct Places {
    platform: Option<String>,
    store: Option<Utf8PathBuf>,
    bin: Option<Utf8PathBuf>,
    lock: Option<Utf8PathBuf>,
}

pub(super) fn doctor(cwd: &Utf8Path, ui: &mut Ui, json: bool, verbose: bool) -> anyhow::Result<()> {
    let (root, findings, places) = match load_config(cwd) {
        Ok(resolved) => {
            let (findings, places) = examine(&resolved);
            (resolved.root, findings, places)
        }
        // Reported as a row rather than raised: see the module documentation.
        Err(error) => (
            uf_config::discover_root(cwd),
            vec![Finding {
                name: "uf.config.js".to_owned(),
                role: CONFIG_ROLE.to_owned(),
                status: Health::Error,
                found: "could not be read".to_owned(),
                reason: None,
                fix: Some(uf_infra::into_string(uf_infra::cstr!("{error:#}"))),
                declared_by: Vec::new(),
                location: None,
            }],
            Places::default(),
        ),
    };

    if json {
        ui.json(&payload(&root, &findings, &places))?;
    } else {
        render(ui, &root, &findings, &places, verbose);
    }
    Ok(())
}

/// Every row for a project whose config could be read.
fn examine(resolved: &ResolvedConfig) -> (Vec<Finding>, Places) {
    let mut findings = Vec::new();
    let mut places = Places {
        lock: Some(resolved.root.join(resolved.config.pm.lockfile.as_str())),
        ..Places::default()
    };
    let platform = uf_env::Platform::current();
    places.platform = platform.map(uf_env::Platform::slug);
    let store = uf_env::Store::discover().ok();
    places.store = store.as_ref().map(|store| store.root().to_owned());
    places.bin = uf_env::project::Envs::discover()
        .ok()
        .map(|envs| envs.bin_dir(&resolved.root));

    // `LockOnly`: reads `uf.lock`, fetches no release list and writes nothing.
    match uf_env::toolchain::resolve(
        &resolved.root,
        &resolved.config,
        Lookup::LockOnly,
        &Publishers,
    ) {
        Ok(toolchain) => {
            for declared in &toolchain.tools {
                findings.push(declared_finding(declared, platform, store.as_ref()));
            }
        }
        Err(error) => findings.push(Finding {
            name: "toolchain".to_owned(),
            role: "declared tools".to_owned(),
            status: Health::Error,
            found: "could not be read".to_owned(),
            reason: None,
            fix: Some(error.to_string()),
            declared_by: Vec::new(),
            location: None,
        }),
    }

    // What runs when the project names no runtime: the host uf has always
    // found on `PATH`. Only then, because a declared runtime is already a row.
    if resolved.config.runtime_tool().is_none() {
        findings.insert(0, default_runtime(resolved));
    }
    // And the package manager, when nothing in the config pins one: the one
    // `uf install` would drive, chosen by the lockfile.
    if resolved.config.package_manager_tool().is_none() {
        findings.push(detected_manager(resolved));
    }
    (findings, places)
}

/// A tool the project declares.
fn declared_finding(
    declared: &Declared,
    platform: Option<uf_env::Platform>,
    store: Option<&uf_env::Store>,
) -> Finding {
    let name = declared.tool.name();
    let mut finding = Finding {
        name: declared.spec(),
        role: declared.roles(),
        status: Health::Ok,
        found: String::new(),
        reason: None,
        fix: None,
        declared_by: Vec::new(),
        location: None,
    };
    // Once each: `runtime` also answers for `build.runtime` and `test.runtime`
    // when those are absent, and naming the same key three times says nothing.
    for used in &declared.uses {
        if !finding.declared_by.contains(&used.key) {
            finding.declared_by.push(used.key.clone());
        }
    }
    match (&declared.resolution, platform) {
        (Resolution::OnPath, _) => on_path(&mut finding, name),
        (Resolution::Unlocked { .. }, _) => {
            finding.status = Health::Warn;
            finding.found = "not locked yet".to_owned();
            finding.fix = Some(INSTALL_FIX.to_owned());
        }
        (_, None) => {
            finding.status = Health::Error;
            finding.found = "no build for this machine".to_owned();
            finding.fix = Some(uf_infra::into_string(uf_infra::cstr!(
                "uf installs tools for macOS and Linux on arm64 and x64; install {name} yourself \
                 and write `{name}` without a version to use the one on PATH"
            )));
        }
        (resolution, Some(platform)) => {
            let version = resolution.version().unwrap_or_default().to_owned();
            let pin = declared.pin(platform);
            let stored = match (store, &pin) {
                (Some(store), Some(pin)) if store.has(pin) => Some(store.path(pin)),
                _ => None,
            };
            match stored {
                Some(path) => {
                    finding.found = version;
                    finding.location = Some(path);
                }
                None => {
                    finding.status = Health::Warn;
                    finding.found =
                        uf_infra::into_string(uf_infra::cstr!("{version} not installed"));
                    finding.fix = Some(INSTALL_FIX.to_owned());
                }
            }
        }
    }
    finding
}

/// A tool the project names without a version: whichever is on `PATH`.
fn on_path(finding: &mut Finding, program: &str) {
    match find_program(program) {
        Some(path) => {
            finding.found = probe_version(program);
            finding.reason = Some("on PATH".to_owned());
            finding.location = Some(path);
        }
        None => {
            finding.status = Health::Error;
            finding.found = "not on PATH".to_owned();
            finding.fix = Some(
                uf_infra::cstr!(
                    "install {program}, or write a version — `{program}@<major>` — and uf installs \
                 that release the first time a command needs it"
                )
                .into_string(),
            );
        }
    }
}

/// The runtime uf falls back to when the project declares none.
fn default_runtime(resolved: &ResolvedConfig) -> Finding {
    let mut finding = Finding {
        name: String::new(),
        role: "runtime".to_owned(),
        status: Health::Ok,
        found: String::new(),
        reason: Some("uf's default".to_owned()),
        fix: None,
        declared_by: Vec::new(),
        location: None,
    };
    match resolve_host(&resolved.config) {
        Ok(host) => {
            finding.name = host.name().to_owned();
            finding.found = probe_version(host.name());
            finding.reason = Some("uf's default, on PATH".to_owned());
            finding.location = Some(host.program);
        }
        Err(error) => {
            finding.name = "node".to_owned();
            finding.status = Health::Error;
            finding.found = "not on PATH".to_owned();
            finding.fix = Some(uf_infra::into_string(uf_infra::cstr!(
                "{error}; or write `runtime: \"node@<major>\"` in uf.config.js and uf installs it"
            )));
        }
    }
    finding
}

/// The package manager `uf install` would drive, when the config pins none.
fn detected_manager(resolved: &ResolvedConfig) -> Finding {
    let detection = detect_package_manager_with(
        &resolved.root,
        &DetectionOptions::from_config(&resolved.config),
    );
    let (manager, substituted) = uf_pm::run::installable(&detection);
    let program = program_of(manager);
    let mut finding = Finding {
        name: program.to_owned(),
        role: "package manager".to_owned(),
        status: Health::Ok,
        found: String::new(),
        reason: Some(manager_reason(&crate::commands::pm::chosen_by(
            &detection.source,
            substituted,
        ))),
        fix: None,
        declared_by: Vec::new(),
        location: None,
    };
    match find_program(program) {
        Some(path) => {
            finding.found = probe_version(program);
            finding.location = Some(path);
        }
        None => {
            finding.status = Health::Error;
            finding.found = "not on PATH".to_owned();
            finding.fix = Some(match manager {
                PackageManager::Npm => {
                    "npm comes with Node.js; install Node.js, or write `packageManager: \
                     \"npm@<version>\"` in uf.config.js and uf installs it"
                        .to_owned()
                }
                _ => uf_infra::cstr!(
                    "install {program}, or write `packageManager: \"{program}@<version>\"` in \
                     uf.config.js and uf installs it"
                )
                .into_string(),
            });
        }
    }
    finding
}

/// Why this manager: `chosen by package-lock.json`, or, when nothing chose
/// it, what was missing — "chosen by no lockfile" reads as a typo.
fn manager_reason(chosen_by: &str) -> String {
    if chosen_by.starts_with("no ") {
        uf_infra::into_string(uf_infra::cstr!("uf's default: {chosen_by}"))
    } else {
        uf_infra::into_string(uf_infra::cstr!("chosen by {chosen_by}"))
    }
}

/// The executable a package manager is run as.
fn program_of(manager: PackageManager) -> &'static str {
    match manager {
        PackageManager::Yarn(_) => "yarn",
        other => other.as_str(),
    }
}

/// `24.3.0` from `node --version`'s `v24.3.0`, or what the tool said instead.
fn probe_version(program: &str) -> String {
    match command_output(program, "--version") {
        // `v24.3.0`, `1.1.27` and `deno 2.1.4 (stable, …)` all carry a
        // version; the number is the part a reader compares, so it is the
        // first word that starts with a digit once a leading `v` is dropped.
        Ok(line) => line
            .split_whitespace()
            .map(|word| word.strip_prefix('v').unwrap_or(word))
            .find(|word| word.starts_with(|first: char| first.is_ascii_digit()))
            .unwrap_or(line.trim())
            .to_owned(),
        Err(_) => "found, but `--version` failed".to_owned(),
    }
}

fn payload(root: &Utf8Path, findings: &[Finding], places: &Places) -> serde_json::Value {
    json!({
        "command": "uf env doctor",
        "root": root,
        "ok": findings.iter().all(|finding| finding.status != Health::Error),
        "platform": places.platform,
        "store": places.store,
        "bin": places.bin,
        "lock": places.lock,
        "tools": findings,
    })
}

fn render(ui: &mut Ui, root: &Utf8Path, findings: &[Finding], places: &Places, verbose: bool) {
    let errors = findings
        .iter()
        .filter(|finding| finding.status == Health::Error)
        .count();
    let warnings = findings
        .iter()
        .filter(|finding| finding.status == Health::Warn)
        .count();
    let details: Vec<String> = findings
        .iter()
        .map(|finding| match &finding.reason {
            Some(reason) => uf_infra::into_string(uf_infra::cstr!("{} · {reason}", finding.role)),
            None => finding.role.clone(),
        })
        .collect();
    let (status, headline) = match (errors, warnings) {
        (0, 0) => (
            Status::Success,
            "everything this project runs on is here".to_owned(),
        ),
        (0, warnings) => (
            Status::Warn,
            uf_infra::into_string(uf_infra::cstr!(
                "{} not installed yet",
                plural(warnings, "tool")
            )),
        ),
        (errors, _) => (
            Status::Error,
            uf_infra::into_string(uf_infra::cstr!("{} to fix", plural(errors, "problem"))),
        ),
    };
    // Not over a config that could not be read: that row is not a tool, and
    // "1 tool checked" would count it as one.
    let checked = if findings.iter().any(|finding| finding.role == CONFIG_ROLE) {
        String::new()
    } else {
        uf_infra::into_string(uf_infra::cstr!(
            "{} checked",
            plural(findings.len(), "tool")
        ))
    };
    let verbose_rows = verbose.then(|| verbose_lines(findings, places));
    // A fix that several rows share is said once, after them, rather than
    // under each: three rows reading "`uf env install` installs it now" is one
    // instruction printed three times.
    let mut shared: Vec<&str> = Vec::new();
    for fix in findings.iter().filter_map(|finding| finding.fix.as_deref()) {
        let repeated = findings
            .iter()
            .filter(|finding| finding.fix.as_deref() == Some(fix))
            .count()
            > 1;
        if repeated && !shared.contains(&fix) {
            shared.push(fix);
        }
    }

    ui.render(|renderer, out| {
        renderer.banner(out, "uf env doctor", Some(project_label(root)));
        let rows: Vec<StatusRow<'_>> = findings
            .iter()
            .zip(&details)
            .map(|(finding, detail)| {
                let row = StatusRow::new(finding.status.status(), &finding.name, &finding.found)
                    .with_detail(detail);
                match finding.fix.as_deref() {
                    Some(fix) if !shared.contains(&fix) => row.with_hint(fix),
                    _ => row,
                }
            })
            .collect();
        renderer.status_rows(out, 2, &rows);
        for fix in &shared {
            renderer.hint(out, 2, fix);
        }
        if let Some(lines) = &verbose_rows {
            renderer.blank(out);
            let items: Vec<uf_term::KeyValue<'_>> = lines
                .iter()
                .map(|(key, value)| uf_term::KeyValue::toned(key, value, uf_term::Tone::Path))
                .collect();
            renderer.key_values(out, 2, &items);
        }
        renderer.blank(out);
        renderer.summary(out, status, &headline, &[&checked]);
    });
}

/// What `--verbose` adds: where each tool is and which key named it, and
/// where uf keeps what it installs.
fn verbose_lines(findings: &[Finding], places: &Places) -> Vec<(String, String)> {
    let mut lines = Vec::new();
    for finding in findings {
        let mut value = finding
            .location
            .as_ref()
            .map_or_else(|| "not installed".to_owned(), ToString::to_string);
        if !finding.declared_by.is_empty() {
            value.push_str(" · declared by ");
            value.push_str(&finding.declared_by.join(", "));
        }
        lines.push((finding.name.clone(), value));
    }
    for (key, value) in [
        ("platform", places.platform.clone()),
        ("store", places.store.as_ref().map(ToString::to_string)),
        ("links", places.bin.as_ref().map(ToString::to_string)),
        ("lock", places.lock.as_ref().map(ToString::to_string)),
    ] {
        if let Some(value) = value {
            lines.push((key.to_owned(), value));
        }
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(dir: &tempfile::TempDir, config: &str) -> Utf8PathBuf {
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        std::fs::write(root.join("package.json"), "{}\n").unwrap();
        std::fs::write(root.join("uf.config.js"), config).unwrap();
        root
    }

    /// uf's own build tools are nobody else's business.
    #[test]
    fn the_doctor_reports_the_project_s_tools_not_uf_s() {
        let dir = tempfile::tempdir().unwrap();
        let root = project(&dir, "export default {};\n");
        let resolved = load_config(&root).unwrap();

        let (findings, _) = examine(&resolved);
        let names: Vec<&str> = findings
            .iter()
            .map(|finding| finding.name.as_str())
            .collect();

        for internal in ["rustc", "cargo", "nix", "git"] {
            assert!(!names.contains(&internal), "{names:?}");
        }
        // A runtime and a package manager, whichever this machine has.
        assert!(
            findings.iter().any(|finding| finding.role == "runtime"),
            "{findings:?}"
        );
        assert!(
            findings
                .iter()
                .any(|finding| finding.role == "package manager"),
            "{findings:?}"
        );
    }

    /// A pinned release the store lacks is not an error: the first command
    /// that needs it installs it. It says so, and says how to do it now.
    #[test]
    fn a_pinned_release_not_yet_installed_is_a_warning_with_a_fix() {
        let dir = tempfile::tempdir().unwrap();
        let root = project(
            &dir,
            "export default { runtime: \"node@99.1.2\", packageManager: \"pnpm@99.0.0\" };\n",
        );
        let resolved = load_config(&root).unwrap();
        let (findings, _) = examine(&resolved);
        let Some(node) = findings
            .iter()
            .find(|finding| finding.name == "node@99.1.2")
        else {
            panic!("no row for the pinned runtime: {findings:?}");
        };
        if uf_env::Platform::current().is_none() {
            return;
        }

        assert_eq!(node.status, Health::Warn, "{node:?}");
        assert!(node.found.contains("99.1.2"), "{node:?}");
        assert!(
            node.fix
                .as_deref()
                .unwrap_or_default()
                .contains("uf env install"),
            "{node:?}"
        );
        assert_eq!(node.declared_by, ["runtime"]);
        // Declared, so uf's default is not a second runtime row.
        assert!(
            !findings
                .iter()
                .any(|finding| finding.reason.as_deref() == Some("uf's default, on PATH")),
            "{findings:?}"
        );
    }

    /// A tool named with no version that is not on `PATH` is the one case a
    /// command will fail on, so it is the one that is an error.
    #[test]
    fn a_tool_that_must_be_on_path_and_is_not_is_an_error() {
        let mut finding = Finding {
            name: "definitely-not-a-real-tool".to_owned(),
            role: "runtime".to_owned(),
            status: Health::Ok,
            found: String::new(),
            reason: None,
            fix: None,
            declared_by: Vec::new(),
            location: None,
        };
        on_path(&mut finding, "definitely-not-a-real-tool");

        assert_eq!(finding.status, Health::Error);
        assert_eq!(finding.found, "not on PATH");
        assert!(finding.fix.is_some());
    }

    #[test]
    fn the_json_payload_carries_every_place_and_row() {
        let findings = vec![Finding {
            name: "node".to_owned(),
            role: "runtime".to_owned(),
            status: Health::Ok,
            found: "24.3.0".to_owned(),
            reason: Some("uf's default, on PATH".to_owned()),
            fix: None,
            declared_by: Vec::new(),
            location: Some(Utf8PathBuf::from("/usr/bin/node")),
        }];
        let places = Places {
            platform: Some("darwin-arm64".to_owned()),
            store: Some(Utf8PathBuf::from("/store")),
            bin: Some(Utf8PathBuf::from("/envs/demo/bin")),
            lock: Some(Utf8PathBuf::from("/demo/uf.lock")),
        };

        let value = payload(Utf8Path::new("/demo"), &findings, &places);

        assert_eq!(value["ok"], json!(true));
        assert_eq!(value["store"], json!("/store"));
        assert_eq!(value["tools"][0]["status"], json!("ok"));
        assert_eq!(value["tools"][0]["location"], json!("/usr/bin/node"));
        assert_eq!(value["tools"][0]["declaredBy"], json!([]));
    }
}
