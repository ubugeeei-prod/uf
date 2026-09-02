//! Backend built on Meta's official Flow Rust port (`upstream/flow/rust_port`).
//!
//! The upstream parser understands modern Flow syntax (`component`, `hook`,
//! `renders`, `match`, enums) natively, so this backend never rewrites the
//! source before parsing and reports diagnostics at their true locations.

use flow_parser::ParseOptions;
use flow_parser::loc::Loc;
use flow_parser::parse_error::ParseError;

use crate::{FlowError, ParseDiagnostic, ParseOutcome, ParserKind};

/// Parse options aligned with the `uf` project defaults.
///
/// Decorators stay off because generated projects never emit them, and every
/// syntax `uf` ships in templates or lints stays on.
const UF_PARSE_OPTIONS: ParseOptions = ParseOptions {
    components: true,
    enums: true,
    pattern_matching: true,
    records: true,
    esproposal_decorators: false,
    types: true,
    ambiguous_types: true,
    enable_types_in_comments: true,
    use_strict: false,
    assert_operator: false,
    module_ref_prefix: None,
    ambient: false,
    allow_return_outside_function: false,
};

pub(crate) fn validate_source(source: &str) -> Result<ParseOutcome, FlowError> {
    let (_program, errors): (_, Vec<(Loc, ParseError)>) =
        flow_parser::parse_program_without_file(false, None, Some(UF_PARSE_OPTIONS), Ok(source));

    Ok(ParseOutcome {
        diagnostics: errors.iter().map(diagnostic_from_error).collect(),
        parser: ParserKind::UpstreamRustPort,
    })
}

fn diagnostic_from_error((loc, error): &(Loc, ParseError)) -> ParseDiagnostic {
    ParseDiagnostic {
        message: error.to_string(),
        line: u32::try_from(loc.start.line).ok(),
        column: u32::try_from(loc.start.column).ok(),
    }
}
