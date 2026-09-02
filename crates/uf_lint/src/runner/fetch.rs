//! `fetch/no-global-override`: reassigning `globalThis.fetch` replaces the
//! instrumented client the rest of the toolchain relies on.

use uf_config::UniflowedConfig;

use crate::scan::FileScan;
use crate::{Diagnostic, push_in_code, severity};

pub(crate) fn run_fetch_no_global_override(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "fetch/no-global-override") else {
        return;
    };

    for (position, line) in scan.lines.iter().enumerate() {
        let code = line.code();
        let at = code
            .find("globalThis.fetch")
            .or_else(|| code.find("window.fetch"))
            .or_else(|| code.find("global.fetch"));
        if let Some(at) = at {
            push_in_code(
                diagnostics,
                scan,
                "fetch/no-global-override",
                severity,
                position,
                at,
                "do not override global fetch; use @uniflowed/fetch explicit clients",
            );
        }
    }
}
