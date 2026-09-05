//! `fetch/no-global-override`: reassigning `globalThis.fetch` replaces the
//! instrumented client the rest of the toolchain relies on.

use uf_config::UniflowedConfig;

use crate::scan::{FileScan, in_string};
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
        // Not in a string: a sentence that names `globalThis.fetch` overrides
        // nothing, and this package's own tests are full of such sentences.
        let at = ["globalThis.fetch", "window.fetch", "global.fetch"]
            .into_iter()
            .filter_map(|needle| code.find(needle))
            .find(|&at| !in_string(code, at));
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
