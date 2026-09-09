//! `flow/syntax`, the one rule that hands the file to the Flow parser instead of
//! reading it as text, and the extension test that decides which files it claims.

use uf_config::UniflowedConfig;
use uf_flow::ParseDiagnostic;
use uf_profiler::profile_span;

use crate::scan::FileScan;
use crate::{Diagnostic, Severity, push, severity};

/// Whether `flow/syntax` wants this module read, and at what severity.
///
/// Split from the report so that one parse can serve every runner that needs
/// the module's tree — see [`super::module_tree`], which owns that parse.
/// `flow/syntax` is the reason a module is read even when no other rule wants
/// it, and it is the only runner whose answer is the parser's own output
/// rather than a walk over the tree.
pub(super) fn wanted(scan: &FileScan<'_>, config: &UniflowedConfig) -> Option<Severity> {
    let severity = severity(config, "flow/syntax")?;
    is_flow_syntax_target(&scan.file.path).then_some(severity)
}

/// Report what the parser said.
///
/// `parsed` is `uf_flow::Parsed::diagnostics`, or the one diagnostic a refusal
/// becomes: a linter that cannot parse a file has something to say about it
/// and thirty thousand other files to get through, so a refusal is a
/// diagnostic like any other and the run continues. `uf_flow::upstream` argues
/// that at length.
pub(super) fn report(
    scan: &FileScan<'_>,
    severity: Severity,
    parsed: &[ParseDiagnostic],
    diagnostics: &mut Vec<Diagnostic>,
) {
    profile_span!("run_flow_syntax");
    for diagnostic in parsed {
        push(
            diagnostics,
            scan.file,
            "flow/syntax",
            severity,
            diagnostic.line.unwrap_or(1) as usize,
            diagnostic.column.unwrap_or(0) as usize + 1,
            diagnostic.message.clone(),
        );
    }
}

/// Whether `path` is Flow source uf should parse.
///
/// Flow lives in `.js` files here; `.flow` declaration sidecars are not part of
/// the product, so a file ending in `.flow` is someone else's convention and
/// parsing it would report syntax errors against declaration syntax uf never
/// emits.
pub(super) fn is_flow_syntax_target(path: &str) -> bool {
    path.ends_with(".js")
        || path.ends_with(".jsx")
        || path.ends_with(".mjs")
        || path.ends_with(".cjs")
}
