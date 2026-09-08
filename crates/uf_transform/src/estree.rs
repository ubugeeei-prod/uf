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
    // Through `uf_flow::module` rather than the port directly, so a module
    // that awaits at its top level transforms here for the same reason it
    // formats and checks: one file, one reading, whichever command asked. The
    // options are that call's to make, not this one's — see its documentation
    // and ubugeeei-prod/uf#430.
    let (ast, errors): (_, Vec<(Loc, ParseError)>) = uf_flow::module::parse(source, None);

    if let Some((loc, error)) = errors.first() {
        // Through `uf_flow` so a module that fails to transform is refused in
        // the same words `uf check` and `uf lint` refuse it. A construct the
        // parser does not implement described three ways is three bugs to
        // report.
        let (message, start) = uf_flow::explain::explained(source, loc, error.to_string());
        let position = offsets
            .convert_flow_position_to_js_position(start)
            .unwrap_or(start);
        return Err(TransformError::Syntax {
            message,
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
    fn renders_a_module_that_awaits_at_its_top_level() {
        // A module — it exports — so `await` is the operator ES2022 says it
        // is, and the transform is handed the same tree `uf fmt` and
        // `uf check` are handed.
        let program = parse("// @flow\nexport const value = await load();\n").unwrap();
        let body = program["body"].as_array().unwrap();
        let declaration = &body[0]["declaration"]["declarations"][0]["init"];
        assert_eq!(declaration["type"], "AwaitExpression");
        assert_eq!(declaration["argument"]["type"], "CallExpression");
    }

    #[test]
    fn renders_a_top_level_for_await() {
        let program =
            parse("// @flow\nexport const rows = [];\nfor await (const row of stream()) {}\n")
                .unwrap();
        let statement = &program["body"].as_array().unwrap()[1];
        assert_eq!(statement["type"], "ForOfStatement");
        assert_eq!(statement["await"], true);
    }

    /// The parser's half of ubugeeei-prod/uf#432.
    ///
    /// `for await` is `for-await-of` and nothing else, and Flow's AST says so
    /// in its shape rather than in a check: `ForOf` carries `await`, `ForIn`
    /// carries only `each` — the E4X-era flag — and there is no field for a
    /// `ForInStatement` to answer `await` with. So no tree this parser
    /// produces can ask a printer for `for await (… in …)`.
    ///
    /// Pinned here because it is a guarantee the printers lean on and nothing
    /// else checks. `uf_fmt` prints from the typed AST, where the shape makes
    /// it unrepresentable; `uf_transform::print` works on JSON and has to
    /// refuse the node itself, because it also prints trees uf did not parse.
    #[test]
    fn a_for_in_has_no_await_to_carry() {
        let program = parse("// @flow\nfor (const k in o) {}\n").unwrap();
        let statement = &program["body"].as_array().unwrap()[0];
        assert_eq!(statement["type"], "ForInStatement");
        assert!(statement.get("await").is_none(), "{statement}");
        assert_eq!(statement["each"], false);
    }

    /// And the syntax is a parse error rather than something the parser bends
    /// into a `ForInStatement` with a flag set.
    #[test]
    fn for_await_over_in_does_not_parse() {
        let source = "// @flow\nexport const rows = [];\nfor await (const k in o) {}\n";
        let error = parse(source).unwrap_err();
        assert!(matches!(error, TransformError::Syntax { .. }), "{error:?}");
    }

    #[test]
    fn refuses_await_outside_async_in_the_same_words_as_the_linter() {
        // A script, which is where `await` is still an identifier: the same
        // file, the same sentence, whichever command reached it.
        let source = "// @flow\nconst value = await load();\n";
        let TransformError::Syntax { message, line, .. } = parse(source).unwrap_err() else {
            panic!("expected a syntax error");
        };
        let expected = uf_flow::validate_source(source).unwrap().diagnostics[0].clone();
        assert_eq!(message, expected.message);
        assert_eq!(line, expected.line.unwrap());
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
