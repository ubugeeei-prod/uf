//! The two router rules: `router/reserved-files`, which keeps the file names
//! the router gives meaning to from being spelled in ways the router will not
//! recognize, and `router/unsupported-segment`, which keeps a directory from
//! being served as a URL when it is spelled the way a feature uf does not have
//! would be.

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

/// `router/unsupported-segment`: `@slot` and `(.)segment` directories.
///
/// A file-scan rule for something that is not about the file, and that is the
/// shape the linter has: a directory is only ever seen through the files under
/// it, and the path of a file is where its directories are written down. The
/// diagnostic lands on the file because the file is what `uf lint` can point
/// at; the message names the directory.
///
/// Only inside the router root, because that is the only place these spellings
/// mean anything — `packages/@scope/…` is a directory, not a route. And only
/// on the way down through public directories: a leading `.` or `_` is a
/// subtree both routers skip, so a slot inside one is not a route uf would
/// have served.
pub(crate) fn run_router_unsupported_segment(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let Some(severity) = severity(config, "router/unsupported-segment") else {
        return;
    };

    let path = scan.file.path.replace('\\', "/");
    let Some(under_root) = path
        .strip_prefix(config.app.router.root.as_str())
        .and_then(|rest| rest.strip_prefix('/'))
    else {
        return;
    };

    // Every component but the last, which is the file name.
    let mut directories: Vec<&str> = under_root.split('/').collect();
    directories.pop();

    for directory in directories {
        if directory.starts_with('.') || directory.starts_with('_') {
            return;
        }
        // `uf_router::reserved` owns this grammar, for the reason
        // `router/reserved-files` above reads the file names from there: a
        // linter with its own copy is how a linter comes to disagree with the
        // build about what is wrong.
        let Some(reason) =
            uf_router::classify_route_segment(directory).unsupported_reason(directory)
        else {
            continue;
        };
        push(
            diagnostics,
            scan.file,
            "router/unsupported-segment",
            severity,
            1,
            1,
            reason,
        );
        return;
    }
}
