//! The `react/*` rules the React compiler decides.
//!
//! `react/hooks-rules` and `react/no-render-side-effects` are the same question
//! `uf_react_compiler`'s syntax mode asks: could this component have been
//! compiled? The predicate lives there, next to the rest of that answer, and
//! this runner maps what it found onto the severity the project configured so
//! the usual suppression handling applies — one predicate, one home, and no way
//! for `uf lint` and `uf build` to disagree about a component.

use uf_config::UniflowedConfig;
use uf_react_compiler::ReactCompilerRule;

use crate::scan::FileScan;
use crate::{Diagnostic, push, severity};

/// Report what the React compiler's syntax mode found.
///
/// A rule the validator knows about but the project has not configured is not
/// reported, which is how the compiler-only rules stay out of `uf lint` until
/// they are added to the rule table.
pub(crate) fn run_react_compiler_rules(
    scan: &FileScan<'_>,
    config: &UniflowedConfig,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if ReactCompilerRule::ALL
        .iter()
        .all(|rule| severity(config, rule.id()).is_none())
    {
        return;
    }
    // A module the validator refuses is one `flow/syntax` already reported on;
    // there is nothing useful to add about it here.
    let Ok(found) = uf_react_compiler::validate(&scan.file.source) else {
        return;
    };

    for entry in found {
        let rule = entry.rule();
        let Some(severity) = severity(config, rule) else {
            continue;
        };
        push(
            diagnostics,
            scan.file,
            rule,
            severity,
            entry.line,
            entry.column,
            entry.message(),
        );
    }
}
