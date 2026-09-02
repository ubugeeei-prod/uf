//! Backend hosting the reference Flow parser inside QuickJS.
//!
//! This backend predates Flow component syntax, so sources are normalized
//! through [`crate::normalize_modern_flow_for_parser`] before parsing.

use std::cell::RefCell;

use crate::{
    FlowError, ParseDiagnostic, ParseOutcome, ParserKind, normalize_modern_flow_for_parser,
};

pub(crate) fn validate_source(source: &str) -> Result<ParseOutcome, FlowError> {
    thread_local! {
        static PARSER: RefCell<Option<flowjs_parser::FlowParser>> = const { RefCell::new(None) };
    }

    let parser_source = normalize_modern_flow_for_parser(source);
    let diagnostics = PARSER.with(|slot| {
        if slot.borrow().is_none() {
            let parser = flowjs_parser::FlowParser::new()
                .map_err(|error| FlowError::Initialize(error.to_string()))?;
            *slot.borrow_mut() = Some(parser);
        }

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
