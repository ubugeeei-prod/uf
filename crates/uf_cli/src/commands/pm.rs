//! `uf install`, `uf upgrade`, and `uf use`: packages and runtimes.

use std::fs;

use anyhow::{Context, Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use serde_json::json;
use uf_config::load_config;
use uf_pm::{PackageManagerPlan, install_workspace};
use uf_rm::{RuntimeManagerPlan, RuntimeReference, RuntimeUsePlan, XdgEnv, XdgLayout};
use uf_term::{KeyValue, Status, Tone};

use crate::support::{enabled, plural, project_label, write_json_file};
use crate::ui::Ui;

pub(crate) fn install(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    let mut progress = ui.progress();
    progress.draw("resolving packages");
    let resolved = load_config(cwd)?;
    let plan = PackageManagerPlan::infer_from_config(&resolved.config);
    let report = install_workspace(&resolved.root, &resolved.config)?;
    progress.finish();
    drop(progress);

    let resolver = format!("{:?}", plan.resolver);
    let scripts = format!("{:?}", plan.scripts);
    let lockfile = report.lockfile.to_string();
    let store = report.store_manifest.to_string();
    let packages = report.packages.len().to_string();
    let entries = report.store_entries.len().to_string();
    let summary = format!("installed {}", plural(report.packages.len(), "package"));

    ui.render(|renderer, out| {
        renderer.banner(out, "uf install", Some(project_label(&resolved.root)));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::new("resolver", &resolver),
                KeyValue::new("scripts", &scripts),
                KeyValue::toned("lockfile", &lockfile, Tone::Path),
                KeyValue::toned("store", &store, Tone::Path),
                KeyValue::toned("packages", &packages, Tone::Number),
                KeyValue::toned("store entries", &entries, Tone::Number),
            ],
        );
        renderer.blank(out);
        renderer.status(out, Status::Success, &summary);
    });
    Ok(())
}

pub(crate) fn upgrade(cwd: &Utf8Path, ui: &mut Ui) -> Result<()> {
    let resolved = load_config(cwd)?;
    let package_manager = PackageManagerPlan::infer_from_config(&resolved.config);
    let runtime_manager = RuntimeManagerPlan::infer_from_config(&resolved.config);
    let install = install_workspace(&resolved.root, &resolved.config)?;
    let state_dir = resolved.root.join(".uf");
    fs::create_dir_all(&state_dir).with_context(|| format!("failed to create {state_dir}"))?;
    let manifest = state_dir.join("upgrade.json");
    write_json_file(
        &manifest,
        &json!({
            "version": 1,
            "packageManager": {
                "resolver": package_manager.resolver,
                "lockfile": install.lockfile.as_str(),
                "storeManifest": install.store_manifest.as_str(),
                "packages": install.packages.len(),
                "storeEntries": install.store_entries.len(),
            },
            "runtimeManager": {
                "engine": runtime_manager.engine,
                "acquisition": runtime_manager.acquisition,
                "hosts": &runtime_manager.hosts,
            },
        }),
    )?;

    let resolver = format!("{:?}", package_manager.resolver);
    let engine = format!("{:?}", runtime_manager.engine);
    let acquisition = format!("{:?}", runtime_manager.acquisition);
    let lockfile = install.lockfile.to_string();
    let manifest_path = manifest.to_string();
    let packages = install.packages.len().to_string();

    ui.render(|renderer, out| {
        renderer.banner(out, "uf upgrade", Some(project_label(&resolved.root)));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::new("package resolver", &resolver),
                KeyValue::new("runtime engine", &engine),
                KeyValue::new("acquisition", &acquisition),
                KeyValue::toned("packages", &packages, Tone::Number),
                KeyValue::toned("lockfile", &lockfile, Tone::Path),
                KeyValue::toned("manifest", &manifest_path, Tone::Path),
            ],
        );
        renderer.blank(out);
        renderer.status(out, Status::Success, "workspace upgraded");
    });
    Ok(())
}

pub(crate) fn use_runtime(cwd: &Utf8Path, ui: &mut Ui, runtime: &str) -> Result<()> {
    let resolved = load_config(cwd)?;
    let requested = RuntimeReference::parse(runtime)
        .ok_or_else(|| anyhow!("runtime must look like uf@0.1.0"))?;
    let plan = RuntimeUsePlan::new(
        requested,
        xdg_layout_from_process(),
        resolved.config.rm.auto_switch,
    );
    let report = apply_runtime_use_plan(&plan)?;

    let runtime_label = format!("{}@{}", plan.requested.name, plan.requested.version);
    let shim = report.shim.to_string();
    let state = report.active_runtime.to_string();
    let manifest = report.runtime_manifest.to_string();
    let binary = report.runtime_binary.to_string();
    let steps = plan
        .steps
        .iter()
        .map(|step| format!("{step:?}"))
        .collect::<Vec<_>>();
    let step_labels = steps.iter().map(String::as_str).collect::<Vec<_>>();
    let summary = format!("now using {runtime_label}");

    ui.render(|renderer, out| {
        renderer.banner(out, "uf use", Some(&runtime_label));
        renderer.blank(out);
        renderer.key_values(
            out,
            2,
            &[
                KeyValue::new("auto switch", enabled(plan.auto_switch)),
                KeyValue::toned("shim", &shim, Tone::Path),
                KeyValue::toned("state", &state, Tone::Path),
                KeyValue::toned("manifest", &manifest, Tone::Path),
                KeyValue::toned("binary", &binary, Tone::Path),
            ],
        );
        renderer.blank(out);
        renderer.heading(out, 2, "steps");
        renderer.bullet_list(out, 4, &step_labels);
        renderer.blank(out);
        renderer.status(out, Status::Success, &summary);
    });
    Ok(())
}

#[derive(Debug)]
struct RuntimeUseApplyReport {
    active_runtime: Utf8PathBuf,
    runtime_manifest: Utf8PathBuf,
    runtime_binary: Utf8PathBuf,
    shim: Utf8PathBuf,
}

fn apply_runtime_use_plan(plan: &RuntimeUsePlan) -> Result<RuntimeUseApplyReport> {
    let version_dir = Utf8PathBuf::from(plan.layout.versions_dir.as_str())
        .join(plan.requested.name.as_str())
        .join(plan.requested.version.as_str());
    let runtime_bin_dir = version_dir.join("bin");
    fs::create_dir_all(&runtime_bin_dir)
        .with_context(|| format!("failed to create {runtime_bin_dir}"))?;

    let runtime_binary = runtime_bin_dir.join(if cfg!(windows) { "uf.exe" } else { "uf" });
    let current_exe = std::env::current_exe().with_context(|| "failed to locate current uf")?;
    fs::copy(&current_exe, runtime_binary.as_std_path()).with_context(|| {
        format!(
            "failed to install runtime binary from {} to {runtime_binary}",
            current_exe.display()
        )
    })?;
    mark_executable(&runtime_binary)?;

    let runtime_manifest = version_dir.join("runtime.json");
    write_json_file(
        &runtime_manifest,
        &json!({
            "name": plan.requested.name.as_str(),
            "version": plan.requested.version.as_str(),
            "binary": runtime_binary.as_str(),
            "source": "current-exe",
            "autoSwitch": plan.auto_switch,
        }),
    )?;

    let state_dir = Utf8PathBuf::from(plan.layout.state_dir.as_str());
    fs::create_dir_all(&state_dir).with_context(|| format!("failed to create {state_dir}"))?;
    let active_runtime = state_dir.join("active-runtime.json");
    write_json_file(
        &active_runtime,
        &json!({
            "name": plan.requested.name.as_str(),
            "version": plan.requested.version.as_str(),
            "manifest": runtime_manifest.as_str(),
            "binary": runtime_binary.as_str(),
        }),
    )?;

    let shim = Utf8PathBuf::from(plan.layout.shim_path.as_str());
    if let Some(parent) = shim.parent() {
        fs::create_dir_all(parent).with_context(|| format!("failed to create {parent}"))?;
    }
    write_runtime_shim(&shim, &runtime_binary)?;
    mark_executable(&shim)?;

    Ok(RuntimeUseApplyReport {
        active_runtime,
        runtime_manifest,
        runtime_binary,
        shim,
    })
}

fn write_runtime_shim(shim: &Utf8Path, runtime_binary: &Utf8Path) -> Result<()> {
    #[cfg(windows)]
    let contents = format!("@echo off\r\n\"{}\" %*\r\n", runtime_binary);
    #[cfg(not(windows))]
    let contents = format!(
        "#!/usr/bin/env sh\nset -eu\nexec {} \"$@\"\n",
        shell_quote(runtime_binary)
    );

    fs::write(shim, contents).with_context(|| format!("failed to write runtime shim {shim}"))
}

#[cfg(unix)]
fn mark_executable(path: &Utf8Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .with_context(|| format!("failed to read permissions for {path}"))?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
        .with_context(|| format!("failed to update permissions for {path}"))
}

#[cfg(not(unix))]
fn mark_executable(_path: &Utf8Path) -> Result<()> {
    Ok(())
}

/// Quote a path for a POSIX shell, so a runtime directory containing a quote
/// cannot break out of the generated shim.
fn shell_quote(path: &Utf8Path) -> String {
    format!("'{}'", path.as_str().replace('\'', "'\\''"))
}

fn xdg_layout_from_process() -> XdgLayout {
    let home = std::env::var("HOME").unwrap_or_else(|_| "$HOME".to_string());
    let config_home = std::env::var("XDG_CONFIG_HOME").ok();
    let data_home = std::env::var("XDG_DATA_HOME").ok();
    let cache_home = std::env::var("XDG_CACHE_HOME").ok();
    let state_home = std::env::var("XDG_STATE_HOME").ok();
    let runtime_dir = std::env::var("XDG_RUNTIME_DIR").ok();

    XdgLayout::from_env(XdgEnv {
        home: &home,
        config_home: config_home.as_deref(),
        data_home: data_home.as_deref(),
        cache_home: cache_home.as_deref(),
        state_home: state_home.as_deref(),
        runtime_dir: runtime_dir.as_deref(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quoting_neutralises_embedded_quotes() {
        assert_eq!(shell_quote(Utf8Path::new("/tmp/uf")), "'/tmp/uf'");
        assert_eq!(
            shell_quote(Utf8Path::new("/tmp/it's here")),
            "'/tmp/it'\\''s here'"
        );
    }
}
