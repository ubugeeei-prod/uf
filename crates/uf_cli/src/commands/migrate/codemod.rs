//! Versioned configuration migrations. No configuration code is executed.
use super::{Plan, source, ui_namespaces};
use anyhow::{Result, ensure};
use camino::Utf8Path;
use serde_json::{Value, json};

pub(super) const TOOL_DECLARATIONS: &str = "tool-declarations-940";
/// The release that moved the tool declarations out of `env.toolchain`.
const TOOL_DECLARATIONS_SINCE: Release = Release::alpha(41);
/// The release whose `@uniflowed/ui` exports each component with parts as a
/// namespace only, without the prefixed part names (ubugeeei-prod/uf#1453).
pub(super) const UI_NAMESPACES_SINCE: Release = Release::minor(3);

pub(super) const UNREAD_KEYS: &str = "unread-config-keys-1387";
/// The release that removed the configuration keys nothing read
/// (ubugeeei-prod/uf#1387).
pub(super) const UNREAD_KEYS_SINCE: Release = Release::minor(10);

/// Every key ubugeeei-prod/uf#1387 removed, as a path into `uf.config.js`.
///
/// Each was declared by `@uniflowed/config`, accepted by the loader, and read
/// by nothing — or only printed, or typed with the one value it could hold —
/// so taking it out changes no behaviour. A whole section is one entry where
/// every key in it went.
const UNREAD_KEYS_REMOVED: &[&[&str]] = &[
    &["app", "componentDefault"],
    &["app", "orm"],
    &["app", "react", "version"],
    &["app", "react", "asyncReact"],
    &["app", "react", "suspense"],
    &["app", "react", "useHook"],
    &["app", "router", "convention"],
    &["app", "rendering", "cache", "actions"],
    &["app", "builtins", "cell"],
    &["app", "builtins", "data"],
    &["app", "builtins", "effect"],
    &["app", "builtins", "fetch"],
    &["app", "builtins", "frameworkLints"],
    &["app", "builtins", "graphql"],
    &["app", "builtins", "loader"],
    &["app", "builtins", "motion"],
    &["app", "builtins", "nativeTestRunner"],
    &["app", "builtins", "pwa"],
    &["app", "builtins", "reactTestingLibrary"],
    &["app", "builtins", "temporal"],
    &["app", "builtins", "tui"],
    &["app", "builtins", "web"],
    &["app", "builtins", "markdown", "module"],
    &["app", "builtins", "markdown", "engine"],
    &["app", "builtins", "markdown", "cache"],
    &["app", "builtins", "markdown", "mdx", "extensions"],
    &["app", "builtins", "markdown", "mdx", "pipelinePlugin"],
    &["app", "builtins", "reactCompiler", "implementation"],
    &["build", "hooks"],
    &["dev", "allowedOrigins"],
    &["docs", "enabled"],
    &["docs", "app"],
    &["docs", "source"],
    &["docs", "staticBuild"],
    &["docs", "deploy"],
    &["fmt", "maxBlankLines"],
    &["fmt", "flow"],
    &["lint", "engine"],
    &["lint", "files"],
    &["lint", "flow"],
    &["package"],
    &["pm", "module"],
    &["pm", "resolver"],
    &["publish", "dryRun"],
    &["publish", "firstPublish"],
    &["publish", "trustedPublish"],
    &["release", "command"],
    &["release", "publish"],
    &["rm"],
    &["server"],
    &["std"],
    &["story"],
    &["taskRunner", "engine"],
    &["test", "reactTestingLibraryNative"],
    &["test", "runner", "runtime"],
    &["test", "runner", "jsHosts"],
    &["test", "runner", "scheduler"],
    &["test", "runner", "performanceTarget"],
    &["test", "runner", "officialFlowParser"],
    &["vrt", "enabled"],
    &["vrt", "module"],
];

/// Every migration between two releases, planned against the project at
/// `root`, for the uf binary running it.
pub(super) fn plan(root: &Utf8Path, from: Option<&str>, to: &str) -> Result<Plan> {
    plan_for(root, from, to, env!("CARGO_PKG_VERSION"))
}

/// [`plan`], with the running binary's version given rather than read.
///
/// A migration is registered for the release that introduces it, which is
/// newer than the binary on `main` until that release is cut. The tests plan a
/// migration as the binary that will carry it would.
pub(super) fn plan_for(
    root: &Utf8Path,
    from: Option<&str>,
    to: &str,
    binary: &str,
) -> Result<Plan> {
    let installed = root.join("node_modules/@uniflowed/core/package.json");
    let installed: Option<Value> = std::fs::read_to_string(installed)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let from = from.or_else(|| installed.as_ref()?.get("version")?.as_str())
        .ok_or_else(|| anyhow::anyhow!(uf_infra::cstr!("installed @uniflowed/core version unavailable; pass --from with the project's previous uf version")))?;
    let from_number = Release::parse(from)?;
    let to_number = Release::parse(to)?;
    ensure!(
        from_number <= to_number,
        uf_infra::cstr!("codemod does not downgrade a project")
    );
    ensure!(
        to_number <= Release::parse(binary)?,
        uf_infra::cstr!("target is newer than this uf binary; install the target uf first")
    );
    let mut plan = Plan::new("codemod");
    plan.from = Some(from.to_owned());
    plan.to = Some(to.to_owned());
    let file = root.join("uf.config.js");
    let before = std::fs::read_to_string(&file)?;
    let mut after = before.clone();
    source::object(&after)?;
    if from_number < TOOL_DECLARATIONS_SINCE && to_number >= TOOL_DECLARATIONS_SINCE {
        for (name, migration) in [
            ("env.toolchain", toolchain as fn(&mut String) -> Result<()>),
            ("builder.module", builder),
            ("pm.packageManager", package_manager),
            ("test.runner", runner),
        ] {
            let mut candidate = after.clone();
            match migration(&mut candidate) {
                Ok(()) => after = candidate,
                Err(error) => plan
                    .unmapped
                    .push(uf_infra::cstr!("{name}: {error}").into_string()),
            }
        }
        plan.migrations.push(TOOL_DECLARATIONS.to_owned());
    }
    if from_number < UNREAD_KEYS_SINCE && to_number >= UNREAD_KEYS_SINCE {
        unread_keys(&mut after, &mut plan.unmapped);
        plan.migrations.push(UNREAD_KEYS.to_owned());
    }
    if after != before {
        plan.write("uf.config.js", before, after);
    }
    if from_number < UI_NAMESPACES_SINCE && to_number >= UI_NAMESPACES_SINCE {
        // Both of #1453's migrations in one pass, because a copy `uf ui add`
        // wrote is rewritten by both and has to come out as one change.
        let steps = ui_namespaces::Steps {
            package: true,
            copies: true,
        };
        ui_namespaces::plan_project(root, &mut plan, steps)?;
        plan.migrations
            .push(ui_namespaces::UI_NAMESPACES.to_owned());
        plan.migrations.push(super::ui_copies::UI_COPIES.to_owned());
    }
    Ok(plan)
}

/// A uf release, ordered the way SemVer orders them: `0.0.0-alpha.48` below
/// `0.1.0`, a prerelease below the release it precedes. The catalog used to
/// read only `0.0.0-alpha.N`, so the first `0.x.0` binary refused every
/// codemod, including the default `--to`, which is its own version.
///
/// The shapes are the ones `tools/release/policy.cjs` lets a release have:
/// `major.minor.patch`, optionally `-alpha.N`, `-beta.N` or `-rc.N`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Release {
    core: [u64; 3],
    /// `alpha`, `beta`, `rc` as 0, 1, 2 with their number; a release is
    /// `(3, 0)`, above every prerelease of the same core.
    stage: (u8, u64),
}

impl Release {
    const fn alpha(number: u64) -> Self {
        Self {
            core: [0, 0, 0],
            stage: (0, number),
        }
    }

    /// `0.<minor>.0`, the release itself rather than a prerelease of it.
    const fn minor(minor: u64) -> Self {
        Self {
            core: [0, minor, 0],
            stage: (3, 0),
        }
    }

    pub(super) fn parse(version: &str) -> Result<Self> {
        let unsupported = || {
            anyhow::anyhow!(uf_infra::cstr!(
                "unsupported migration version {version}; expected a uf release such as 0.1.0 or 0.0.0-alpha.41"
            ))
        };
        let number = |text: &str| -> Option<u64> {
            let canonical = !text.is_empty()
                && text.bytes().all(|byte| byte.is_ascii_digit())
                && (text == "0" || !text.starts_with('0'));
            if canonical { text.parse().ok() } else { None }
        };
        let (core, prerelease) = match version.split_once('-') {
            Some((core, prerelease)) => (core, Some(prerelease)),
            None => (version, None),
        };
        let parts: Vec<u64> = core
            .split('.')
            .map(number)
            .collect::<Option<_>>()
            .ok_or_else(unsupported)?;
        let core: [u64; 3] = parts.try_into().map_err(|_| unsupported())?;
        let stage = match prerelease {
            None => (3, 0),
            Some(prerelease) => {
                let (word, count) = prerelease.split_once('.').ok_or_else(unsupported)?;
                let rank = match word {
                    "alpha" => 0,
                    "beta" => 1,
                    "rc" => 2,
                    _ => return Err(unsupported()),
                };
                (rank, number(count).ok_or_else(unsupported)?)
            }
        };
        Ok(Self { core, stage })
    }
}

/// Take out every key ubugeeei-prod/uf#1387 removed, and a section left empty
/// by it.
///
/// A key that is not a static value — a spread, a computed key — is reported
/// and left for a person, like every other migration here. Two keys it reads
/// the value of rather than removing blindly:
///
/// * `mdx.jsxImportSource` is read now, and `"@uniflowed/jsx-runtime"` — its
///   old default and the only value the type allowed — is refused, so that
///   value is removed and any other is kept;
/// * `taskRunner.allowPackageScripts: false` is enforced now, which is a
///   change of behaviour for a project that wrote it and relied on a
///   command-less task, so it is kept and reported.
fn unread_keys(source: &mut String, unmapped: &mut Vec<String>) {
    let mut emptied: Vec<Vec<&'static str>> = Vec::new();
    let mut remove = |source: &mut String, path: &[&'static str], unmapped: &mut Vec<String>| {
        let mut candidate = source.clone();
        let removed = source::get(&candidate, path).and_then(|found| {
            if found.is_some() {
                source::remove(&mut candidate, path)?;
            }
            Ok(found.is_some())
        });
        match removed {
            Ok(true) => {
                *source = candidate;
                for depth in 1..path.len() {
                    emptied.push(path[..depth].to_vec());
                }
            }
            Ok(false) => {}
            Err(error) => {
                unmapped.push(uf_infra::cstr!("{}: {error}", path.join(".")).into_string())
            }
        }
    };
    for path in UNREAD_KEYS_REMOVED {
        remove(source, path, unmapped);
    }
    let jsx_import_source = ["app", "builtins", "markdown", "mdx", "jsxImportSource"];
    if matches!(source::get(source, &jsx_import_source), Ok(Some(Value::String(value))) if value == "@uniflowed/jsx-runtime")
    {
        remove(source, &jsx_import_source, unmapped);
    }
    if matches!(
        source::get(source, &["taskRunner", "allowPackageScripts"]),
        Ok(Some(Value::Bool(false)))
    ) {
        unmapped.push(
            "taskRunner.allowPackageScripts: false is enforced from this release — a task \
             with no `command` is refused rather than handed to `vp run`; delete the key to \
             keep handing it over"
                .to_owned(),
        );
    }
    // Deepest first, so `app.builtins` emptied by its last key goes before
    // `app` is looked at.
    emptied.sort_by(|a, b| b.len().cmp(&a.len()).then_with(|| a.cmp(b)));
    emptied.dedup();
    for path in emptied {
        if matches!(source::get(source, &path), Ok(Some(Value::Object(object))) if object.is_empty())
        {
            let _ = source::remove(source, &path);
        }
    }
}

fn toolchain(source: &mut String) -> Result<()> {
    let Some(value) = source::get(source, &["env", "toolchain"])? else {
        return Ok(());
    };
    let tools = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!(uf_infra::cstr!("expected a static tool map")))?;
    let mut runtime = Vec::new();
    let mut managers = Vec::new();
    for (name, version) in tools {
        let version = version
            .as_str()
            .ok_or_else(|| anyhow::anyhow!(uf_infra::cstr!("{name} version is not a string")))?;
        match name.as_str() {
            "node" | "bun" | "deno" => {
                runtime.push(uf_infra::cstr!("{name}@{version}").into_string())
            }
            "npm" | "pnpm" | "yarn" => {
                managers.push(uf_infra::cstr!("{name}@{version}").into_string())
            }
            _ => anyhow::bail!(uf_infra::cstr!("unknown tool {name}")),
        }
    }
    ensure!(
        runtime.len() <= 1 && managers.len() <= 1,
        uf_infra::cstr!("multiple runtimes or package managers require explicit role selection")
    );
    if let Some(spec) = runtime.first() {
        source::merge(source, &["runtime"], &json!(spec))?;
    }
    if let Some(spec) = managers.first() {
        source::merge(source, &["packageManager"], &json!(spec))?;
    }
    source::remove(source, &["env", "toolchain"])
}

fn builder(source: &mut String) -> Result<()> {
    let Some(value) = source::get(source, &["builder", "module"])? else {
        return Ok(());
    };
    let value = if value == "@uniflowed/vite" {
        json!("vite")
    } else {
        value
    };
    source::merge(source, &["build", "builder"], &value)?;
    source::remove(source, &["builder", "module"])
}

fn package_manager(source: &mut String) -> Result<()> {
    let Some(value) = source::get(source, &["pm", "packageManager"])? else {
        return Ok(());
    };
    let name = value
        .as_str()
        .ok_or_else(|| anyhow::anyhow!(uf_infra::cstr!("expected a package manager name")))?;
    let spec = match name {
        "auto" => {
            source::remove(source, &["pm", "packageManager"])?;
            return Ok(());
        }
        "yarn-classic" => "yarn@1",
        "yarn-berry" => "yarn",
        "npm" | "pnpm" | "yarn" | "bun" => name,
        _ => anyhow::bail!(uf_infra::cstr!(
            "package manager {name} has no safe replacement"
        )),
    };
    let existing = source::get(source, &["packageManager"])?;
    if !existing.as_ref().and_then(Value::as_str).is_some_and(|v| {
        v == spec || (v.starts_with(uf_infra::cstr!("{spec}@").as_str()) && !spec.contains('@'))
    }) {
        source::merge(source, &["packageManager"], &json!(spec))?;
    }
    source::remove(source, &["pm", "packageManager"])
}

fn runner(source: &mut String) -> Result<()> {
    let Some(Value::Object(settings)) = source::get(source, &["test", "runner"])? else {
        return Ok(());
    };
    let defaults = json!({"runtime":"capability-js-host", "scheduler":"native-work-stealing", "performanceTarget":"faster-than-bun", "jsHosts":["node","deno","bun"], "officialFlowParser":true});
    for (key, value) in &settings {
        if key == "applicationTarget" {
            continue;
        }
        ensure!(
            defaults.get(key) == Some(value),
            uf_infra::cstr!("runner.{key} is non-default or unknown and needs manual migration")
        );
    }
    if let Some(target) = settings.get("applicationTarget").filter(|v| *v != "auto") {
        ensure!(
            matches!(target.as_str(), Some("web" | "react-native")),
            uf_infra::cstr!("unknown applicationTarget")
        );
        source::merge(source, &["test", "target"], target)?;
    }
    source::put(source, &["test", "runner"], &json!("uf"))
}
