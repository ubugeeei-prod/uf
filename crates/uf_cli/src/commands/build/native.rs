//! Metro owns native bundles and asset density resolution. uf selects the
//! platform, writes the routes and records the output of the project's CLI.

use std::fs;
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::json;
use uf_config::ResolvedConfig;
use uf_router::RouteTarget;
use uf_router::native::{discover_native_route_tables, write_native_route_tables};
use uf_term::Status;
use walkdir::WalkDir;

use crate::commands::dev::native::{NativeServer, check_metro};
use crate::commands::task::adopt_exit_status;
use crate::support::{PRODUCTION, project_env, write_json_file};
use crate::ui::Ui;

pub(super) fn build(
    ui: &mut Ui,
    resolved: &ResolvedConfig,
    target: RouteTarget,
    mode: Option<&str>,
) -> Result<()> {
    let root = &resolved.root;
    let server = NativeServer::detect(root).ok_or_else(|| anyhow!(
        "`uf build --target {}` needs the project's Expo or React Native CLI; install expo or @react-native-community/cli and compose withUniflowedMetro() in metro.config.js",
        target.as_str()
    ))?;
    check_metro(&server, root)?;
    let tables = discover_native_route_tables(root, &resolved.config)?;
    let written = write_native_route_tables(root, &resolved.config, &tables)?;
    let env = project_env(resolved, mode, PRODUCTION)?;
    let binary = std::env::current_exe()?;
    let manifest: serde_json::Value = fs::read_to_string(root.join("package.json"))
        .ok()
        .map(|text| serde_json::from_str(&text))
        .transpose()?
        .unwrap_or_else(|| json!({}));
    let entry = manifest
        .get("main")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("index.js");
    let platforms: &[&str] = match target {
        RouteTarget::Ios => &["ios"],
        RouteTarget::Android => &["android"],
        RouteTarget::Native => &["ios", "android"],
        RouteTarget::Web => unreachable!("web builds use their builder"),
    };
    let command_name = match server {
        NativeServer::Expo { .. } => "export:embed",
        NativeServer::ReactNativeCli { .. } => "bundle",
    };
    let out = root
        .join(resolved.config.build.out_dir.as_str())
        .join("native");
    let mut outputs = Vec::new();
    for platform in platforms {
        let platform_out = out.join(platform);
        // Use a fresh staging directory so removed assets cannot appear in a
        // successful manifest and a failed bundle leaves the previous output.
        fs::create_dir_all(&out)?;
        let staged = tempfile::tempdir_in(&out)?;
        let bundle = staged.path().join("main.jsbundle");
        let assets = staged.path().join("assets");
        fs::create_dir_all(&assets)?;
        let mut command = Command::new(server.binary());
        env.apply(&mut command);
        command
            .current_dir(root)
            .env("UF_BINARY", &binary)
            .args([
                command_name,
                "--platform",
                platform,
                "--dev",
                "false",
                "--entry-file",
                entry,
                "--bundle-output",
            ])
            .arg(&bundle)
            .arg("--assets-dest")
            .arg(&assets)
            .arg("--sourcemap-output")
            .arg(staged.path().join("main.jsbundle.map"));
        let status = command
            .status()
            .context("failed to start the project's Metro bundler")?;
        if !status.success() {
            adopt_exit_status(ui, status, command_name);
        }
        if !bundle.is_file() {
            bail!("{command_name} exited successfully but wrote no Metro bundle for {platform}");
        }
        if platform_out.exists() {
            fs::remove_dir_all(&platform_out)?;
        }
        fs::rename(staged.path(), &platform_out)?;
        for file in WalkDir::new(&platform_out) {
            let file = file?;
            if file.file_type().is_file() {
                outputs.push(json!({
                    "platform": platform,
                    "path": file.path().strip_prefix(root)?.to_string_lossy(),
                    "bytes": file.metadata()?.len(),
                }));
            }
        }
    }
    outputs.sort_by(|a, b| a["path"].as_str().cmp(&b["path"].as_str()));
    let routes: Vec<_> = tables.iter().filter(|table| table.target == target).flat_map(|table| &table.routes)
        .map(|entry| json!({"path": entry.route.path, "page": entry.route.page.strip_prefix(root).unwrap_or(&entry.route.page)})).collect();
    fs::create_dir_all(root.join(".uf/build/meta"))?;
    write_json_file(
        &root.join(".uf/build/meta/uf-build-manifest.json"),
        &json!({
            "target": target.as_str(),
            "targetContract": super::target_contract(target),
            "provider": server.label(),
            "command": command_name,
            "entry": entry,
            "routes": routes,
            "routeModules": written.files,
            "outputs": outputs,
            "delegated": ["prebuild", "config plugins", "native compilation", "signing", "submission", "updates"],
        }),
    )?;
    ui.render(|renderer, out| {
        renderer.status(
            out,
            Status::Success,
            "Metro bundle and assets written; compile and sign with Expo/EAS, Xcode or Gradle",
        )
    });
    Ok(())
}
