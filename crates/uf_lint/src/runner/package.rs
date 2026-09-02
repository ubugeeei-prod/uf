//! `package/no-npm-scripts`: a uf project declares its tasks in `uf.config.js`,
//! so a `scripts` block in `package.json` is a task nothing will run.

use uf_config::UniflowedConfig;

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
    if !scan.file.path.ends_with("package.json") {
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
                "declare tasks in uf.config.js; npm scripts are not part of the uf toolchain",
            );
        }
    }
}
