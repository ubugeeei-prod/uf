//! Reading one module's imports, exports and rewrite spans.

mod effects;
mod exporting;
mod importing;

use super::*;

fn record(source: &str) -> ModuleRecord {
    scan_module(source)
}

/// The module with its recorded rewrites applied, for readability.
fn rewritten(source: &str) -> String {
    crate::emit::apply_patches(source, &record(source).patches)
}
