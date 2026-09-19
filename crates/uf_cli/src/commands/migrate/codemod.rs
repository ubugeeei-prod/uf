//! Versioned configuration migrations. No configuration code is executed.
use super::{Plan, source};
use anyhow::{Result, ensure};
use camino::Utf8Path;
use serde_json::{Value, json};

pub(super) const TOOL_DECLARATIONS: &str = "tool-declarations-940";

pub(super) fn plan(root: &Utf8Path, from: Option<&str>, to: &str) -> Result<Plan> {
    let installed = root.join("node_modules/@uniflowed/core/package.json");
    let installed: Option<Value> = std::fs::read_to_string(installed)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok());
    let from = from.or_else(|| installed.as_ref()?.get("version")?.as_str())
        .ok_or_else(|| anyhow::anyhow!("installed @uniflowed/core version unavailable; pass --from with the project's previous uf version"))?;
    let from_number = alpha(from)?;
    let to_number = alpha(to)?;
    ensure!(
        from_number <= to_number,
        "codemod does not downgrade a project"
    );
    ensure!(
        to_number <= alpha(env!("CARGO_PKG_VERSION"))?,
        "target is newer than this uf binary; install the target uf first"
    );
    let mut plan = Plan::new("codemod");
    plan.from = Some(from.to_owned());
    plan.to = Some(to.to_owned());
    let file = root.join("uf.config.js");
    let before = std::fs::read_to_string(&file)?;
    let mut after = before.clone();
    source::object(&after)?;
    if from_number < 41 && to_number >= 41 {
        for (name, migration) in [
            ("env.toolchain", toolchain as fn(&mut String) -> Result<()>),
            ("builder.module", builder),
            ("pm.packageManager", package_manager),
            ("test.runner", runner),
        ] {
            let mut candidate = after.clone();
            match migration(&mut candidate) {
                Ok(()) => after = candidate,
                Err(error) => plan.unmapped.push(format!("{name}: {error}")),
            }
        }
        plan.migrations.push(TOOL_DECLARATIONS.to_owned());
    }
    if after != before {
        plan.write("uf.config.js", before, after);
    }
    Ok(plan)
}

fn alpha(version: &str) -> Result<u32> {
    version
        .strip_prefix("0.0.0-alpha.")
        .and_then(|v| v.parse().ok())
        .ok_or_else(|| {
            anyhow::anyhow!(
                "unsupported migration version {version}; this catalog covers 0.0.0-alpha releases"
            )
        })
}

fn toolchain(source: &mut String) -> Result<()> {
    let Some(value) = source::get(source, &["env", "toolchain"])? else {
        return Ok(());
    };
    let tools = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("expected a static tool map"))?;
    let mut runtime = Vec::new();
    let mut managers = Vec::new();
    for (name, version) in tools {
        let version = version
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("{name} version is not a string"))?;
        match name.as_str() {
            "node" | "bun" | "deno" => runtime.push(format!("{name}@{version}")),
            "npm" | "pnpm" | "yarn" => managers.push(format!("{name}@{version}")),
            _ => anyhow::bail!("unknown tool {name}"),
        }
    }
    ensure!(
        runtime.len() <= 1 && managers.len() <= 1,
        "multiple runtimes or package managers require explicit role selection"
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
        .ok_or_else(|| anyhow::anyhow!("expected a package manager name"))?;
    let spec = match name {
        "auto" => {
            source::remove(source, &["pm", "packageManager"])?;
            return Ok(());
        }
        "yarn-classic" => "yarn@1",
        "yarn-berry" => "yarn",
        "npm" | "pnpm" | "yarn" | "bun" => name,
        _ => anyhow::bail!("package manager {name} has no safe replacement"),
    };
    let existing = source::get(source, &["packageManager"])?;
    if !existing
        .as_ref()
        .and_then(Value::as_str)
        .is_some_and(|v| v == spec || (v.starts_with(&format!("{spec}@")) && !spec.contains('@')))
    {
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
            "runner.{key} is non-default or unknown and needs manual migration"
        );
    }
    if let Some(target) = settings.get("applicationTarget").filter(|v| *v != "auto") {
        ensure!(
            matches!(target.as_str(), Some("web" | "react-native")),
            "unknown applicationTarget"
        );
        source::merge(source, &["test", "target"], target)?;
    }
    source::put(source, &["test", "runner"], &json!("uf"))
}
