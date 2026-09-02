//! Backend hosting the reference Flow parser inside QuickJS.
//!
//! This backend predates Flow component syntax, so sources are normalized
//! through [`crate::normalize_modern_flow_for_parser`] before parsing.
//!
//! # Stack budget
//!
//! QuickJS records the stack pointer when the runtime is created and enforces a
//! fixed 256 KB budget measured from that point (`JS_DEFAULT_STACK_SIZE`), which
//! it reports as a `SyntaxError: stack overflow`. That budget is relative, not
//! absolute, so *where* the runtime is created decides how much of it the
//! embedder has already spent.
//!
//! Creating it lazily inside a work-stealing job is therefore a trap: a later
//! job on the same worker can run several frames deeper than the one that
//! created the runtime, so the parser starts with much of its 256 KB already
//! consumed and Flow's recursive-descent parser trips the limit on ordinary
//! files. It looks like a syntax error in the user's code, and it scales with
//! parallelism rather than with input size.
//!
//! [`prepare_thread`] exists so callers can create the runtime from a shallow
//! frame, before any nested work begins.

use std::cell::RefCell;

use crate::{
    FlowError, ParseDiagnostic, ParseOutcome, ParserKind, normalize_modern_flow_for_parser,
};

thread_local! {
    static PARSER: RefCell<Option<flowjs_parser::FlowParser>> = const { RefCell::new(None) };
}

/// Create this thread's QuickJS runtime now, at the current stack depth.
///
/// Idempotent, and cheap after the first call.
pub(crate) fn prepare_thread() -> Result<(), FlowError> {
    PARSER.with(|slot| {
        if slot.borrow().is_none() {
            let parser = flowjs_parser::FlowParser::new()
                .map_err(|error| FlowError::Initialize(error.to_string()))?;
            *slot.borrow_mut() = Some(parser);
        }
        Ok(())
    })
}

pub(crate) fn validate_source(source: &str) -> Result<ParseOutcome, FlowError> {
    let parser_source = normalize_modern_flow_for_parser(source);
    prepare_thread()?;
    let diagnostics = PARSER.with(|slot| {
        let parser = slot.borrow();
        let parser = parser.as_ref().expect("parser initialized");
        parser
            .diagnostics(parser_source.as_deref().unwrap_or(source))
            .map_err(|error| FlowError::Runtime(error.to_string()))
    })?;
    let diagnostics = diagnostics
        .into_iter()
        .map(|diagnostic| ParseDiagnostic {
            message: diagnostic.message,
            line: diagnostic.loc.as_ref().map(|loc| loc.start.line),
            column: diagnostic.loc.as_ref().map(|loc| loc.start.column),
        })
        .collect();

    Ok(ParseOutcome {
        diagnostics,
        parser: ParserKind::OfficialFlowParser,
    })
}
