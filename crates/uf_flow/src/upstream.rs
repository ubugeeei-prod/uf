//! Meta's official Flow Rust port (`upstream/flow/rust_port`).
//!
//! It understands modern Flow syntax (`component`, `hook`, `renders`, `match`,
//! enums) natively, so nothing rewrites the source before parsing and every
//! diagnostic points at the location the user actually wrote.

use flow_parser::loc::Loc;
use flow_parser::parse_error::ParseError;

use crate::parse::PARSE_OPTIONS;
use crate::{FlowError, ParseDiagnostic, ParseOutcome};

pub(crate) fn validate_source(source: &str) -> Result<ParseOutcome, FlowError> {
    let (_program, errors): (_, Vec<(Loc, ParseError)>) =
        flow_parser::parse_program_without_file(false, None, Some(PARSE_OPTIONS), Ok(source));

    Ok(ParseOutcome {
        diagnostics: errors.iter().map(diagnostic_from_error).collect(),
    })
}

fn diagnostic_from_error((loc, error): &(Loc, ParseError)) -> ParseDiagnostic {
    ParseDiagnostic {
        message: error.to_string(),
        line: u32::try_from(loc.start.line).ok(),
        column: u32::try_from(loc.start.column).ok(),
    }
}
