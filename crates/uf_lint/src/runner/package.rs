//! `package/no-npm-scripts`: the same install-hook policy `uf install` uses.

use uf_config::{INSTALL_LIFECYCLE_SCRIPTS, UniflowedConfig};

use crate::scan::FileScan;
use crate::{Diagnostic, push_at, severity};

pub(crate) fn run_package_no_npm_scripts(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "package/no-npm-scripts") else {
        return;
    };
    if !scan.file.path.ends_with("package.json") || config.pm.allow_lifecycle_scripts {
        return;
    }
    let Ok(manifest) = serde_json::from_str::<serde_json::Value>(&scan.file.source) else {
        return;
    };
    let Some(scripts) = manifest
        .get("scripts")
        .and_then(serde_json::Value::as_object)
    else {
        return;
    };
    let forbidden: Vec<_> = scripts
        .keys()
        .filter(|name| INSTALL_LIFECYCLE_SCRIPTS.contains(&name.as_str()))
        .cloned()
        .collect();
    if forbidden.is_empty() {
        return;
    }

    for (position, line) in scan.lines.iter().enumerate() {
        if let Some(at) = line.text.find(r#""scripts""#) {
            push_at(
                diagnostics,
                scan,
                "package/no-npm-scripts",
                severity,
                position,
                at,
                uf_infra::cstr!(
                    "install-time lifecycle scripts ({}) are disabled; move the automation to uf tasks or explicitly allow lifecycle scripts",
                    forbidden.join(", ")
                ).into_string(),
            );
        }
    }
}
