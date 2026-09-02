//! `router/reserved-files`, which keeps the file names the router gives meaning
//! to from being spelled in ways the router will not recognize.

use uf_config::UniflowedConfig;

use crate::scan::FileScan;
use crate::{Diagnostic, push, severity};

pub(crate) fn run_router_reserved_files(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "router/reserved-files") else {
        return;
    };

    let Some(file_name) = scan.file.path.rsplit('/').next() else {
        return;
    };
    // `uf_router::reserved` owns this grammar; duplicating it here is how the
    // scaffold, the router, and the linter drifted apart in the first place.
    if !uf_router::classify_reserved_file(file_name).is_unknown() {
        return;
    }

    push(
        diagnostics,
        scan.file,
        "router/reserved-files",
        severity,
        1,
        1,
        "reserved router file names are _uf.<layout|page|middleware>[.<native|ios|android|web|test>].js",
    );
}
