//! Parsing with the official Flow parser, rendered as ESTree.
//!
//! `flow_parser` is Meta's Rust port of Flow's parser and it ships its own
//! ESTree translator — the same rendering `flow-parser` on npm produces. uf
//! takes that rendering as a `serde_json::Value` tree and works on it
//! directly: every later stage in this crate is a rewrite of that tree, and
//! the lowering rules it applies were written against exactly this shape.
//!
//! Offsets (`range`) are JavaScript string indices, in UTF-16 code units,
//! which is what source maps and editors expect. The translator's `loc`
//! columns count code points instead; [`crate::babel`] recomputes them from
//! the offsets so every position downstream agrees.

use flow_parser::ParseOptions;
use flow_parser::estree_translator::{self, Config, OffsetStyle};
use flow_parser::loc::Loc;
use flow_parser::offset_utils::{OffsetKind, OffsetTable};
use flow_parser::parse_error::ParseError;
use serde_json::Value;

use crate::TransformError;

/// Longest source the transform will read, in bytes.
///
/// A dependency can put a generated or hostile file in `node_modules`; every
/// scan in uf has an explicit ceiling rather than trusting its input.
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;

/// Parse options aligned with the `uf` project defaults.
///
/// Every syntax uf documents is on: components, hooks, enums, pattern
/// matching, records. Decorators stay off because generated projects never
/// emit them.
pub const PARSE_OPTIONS: ParseOptions = ParseOptions {
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

/// Parse `source` and render it as an ESTree `Program`.
///
/// Comments come back twice: on the program (`comments`) and attached to the
/// nodes they sit beside. The React Compiler reads the program's list; the
/// attached copies are dropped when the tree is converted for it.
///
/// # Errors
///
/// [`TransformError::SourceTooLarge`] over the ceiling, and
/// [`TransformError::Syntax`] with the first error the parser reported.
pub fn parse(source: &str) -> Result<Value, TransformError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(TransformError::SourceTooLarge {
            bytes: source.len(),
            limit: MAX_SOURCE_BYTES,
        });
    }

    // JavaScript columns: UTF-16 code units, which is what source maps count.
    let offsets = OffsetTable::make_with_kind(OffsetKind::JavaScript, source);
    let (ast, errors): (_, Vec<(Loc, ParseError)>) =
        flow_parser::parse_program_without_file(false, None, Some(PARSE_OPTIONS), Ok(source));

    if let Some((loc, error)) = errors.first() {
        let position = offsets
            .convert_flow_position_to_js_position(loc.start)
            .unwrap_or(loc.start);
        return Err(TransformError::Syntax {
            message: error.to_string(),
            line: u32::try_from(position.line).unwrap_or(u32::MAX),
            column: u32::try_from(position.column).unwrap_or(u32::MAX),
        });
    }

    let config = Config {
        include_locs: true,
        include_filename: false,
        offset_style: OffsetStyle::JsIndices,
    };
    Ok(estree_translator::program(&offsets, &config, &ast))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_component_syntax_as_estree() {
        let program = parse("// @flow\ncomponent A(x: string) { return null; }\n").unwrap();
        let body = program["body"].as_array().unwrap();
        assert_eq!(body[0]["type"], "ComponentDeclaration");
        assert_eq!(body[0]["params"][0]["type"], "ComponentParameter");
        assert_eq!(program["comments"][0]["type"], "Line");
    }

    #[test]
    fn reports_the_first_syntax_error_with_a_position() {
        let error = parse("const a = ;\n").unwrap_err();
        match error {
            TransformError::Syntax { line, .. } => assert_eq!(line, 1),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn offsets_count_utf16_code_units() {
        let program = parse("const s = \"😀\"; const t = 1;\n").unwrap();
        let second = &program["body"][1];
        // `const s = "😀"; ` is 16 code units (the emoji is two) and 19 bytes.
        assert_eq!(second["range"][0], 16);
    }

    #[test]
    fn refuses_oversized_input() {
        let big = "a;".repeat(MAX_SOURCE_BYTES / 2 + 1);
        assert!(matches!(
            parse(&big),
            Err(TransformError::SourceTooLarge { .. })
        ));
    }
}
