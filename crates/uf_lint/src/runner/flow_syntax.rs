//! `flow/syntax`, the one rule that hands the file to the Flow parser instead of
//! reading it as text, and the extension test that decides which files it claims.

use uf_config::UniflowedConfig;
use uf_flow::FlowParser;

use crate::scan::FileScan;
use crate::{Diagnostic, LintError, push, severity};

pub(crate) fn run_flow_syntax(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) -> Result<(), LintError> {
    let Some(severity) = severity(config, "flow/syntax") else {
        return Ok(());
    };
    if !is_flow_syntax_target(&scan.file.path) {
        return Ok(());
    }

    let parser = FlowParser;
    let outcome = parser.validate_source(&scan.file.source)?;
    for diagnostic in outcome.diagnostics {
        push(
            diagnostics,
            scan.file,
            "flow/syntax",
            severity,
            diagnostic.line.unwrap_or(1) as usize,
            diagnostic.column.unwrap_or(0) as usize + 1,
            diagnostic.message,
        );
    }

    Ok(())
}

/// Whether `path` is Flow source uf should parse.
///
/// Flow lives in `.js` files here; `.flow` declaration sidecars are not part of
/// the product, so a file ending in `.flow` is someone else's convention and
/// parsing it would report syntax errors against declaration syntax uf never
/// emits.
fn is_flow_syntax_target(path: &str) -> bool {
    path.ends_with(".js")
        || path.ends_with(".jsx")
        || path.ends_with(".mjs")
        || path.ends_with(".cjs")
}
