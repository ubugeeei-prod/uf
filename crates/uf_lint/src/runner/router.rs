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
        grammar(),
    );
}

/// The grammar, spelled from the enums that define it.
///
/// It used to be a string literal here, and it was wrong: it listed five roles
/// while `_uf.not-found` was a sixth the build router had reserved all along,
/// so a file the framework resolves was reported as a name it would not
/// recognize — and the message told the reader to rename it. A message that
/// lists what is allowed has to be generated from what is allowed.
fn grammar() -> String {
    let roles = uf_router::ReservedRole::all()
        .map(uf_router::ReservedRole::as_str)
        .join("|");
    let variants = uf_router::ReservedVariant::all()
        .iter()
        .filter_map(|variant| variant.as_str())
        .collect::<Vec<_>>()
        .join("|");
    format!("reserved file names are _uf.<{roles}>[.<{variants}>].js")
}
