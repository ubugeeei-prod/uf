//! The last mile: JSX, Fast Refresh and code generation through oxc.
//!
//! oxc is the compiler inside Vite and Rolldown, so what it does to JSX here
//! is what Vite would have done to a `.jsx` file: the automatic runtime
//! (`jsx`/`jsxs`, or `jsxDEV` in development) imported from the configured
//! source, and `react-refresh` registrations in development. Running it in
//! process means `uf test` and the Vite plugin produce byte-identical modules.
//!
//! The source map oxc produces points at the printed JavaScript; it is
//! composed with the printer's mappings so the final map points at the Flow
//! source.

use std::path::{Path, PathBuf};

use oxc_allocator::Allocator;
use oxc_codegen::{Codegen, CodegenOptions};
use oxc_parser::Parser;
use oxc_semantic::SemanticBuilder;
use oxc_sourcemap::SourceMapBuilder;
use oxc_span::SourceType;
use oxc_transformer::{
    EnvOptions, JsxOptions, JsxRuntime, ReactRefreshOptions, TransformOptions, Transformer,
};

use crate::print::{Mapping, Printed};
use crate::{TransformError, TransformOptions as UfOptions};

/// The final module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Emitted {
    /// JavaScript with JSX lowered.
    pub code: String,
    /// Source map JSON, when asked for.
    pub map: Option<String>,
}

/// Lower JSX, register components for Fast Refresh, and generate code.
///
/// # Errors
///
/// [`TransformError::Internal`] when the printed code does not parse, which
/// would be a printer bug, and [`TransformError::Lowering`] when the JSX
/// transform refuses something (a namespaced tag, say).
pub fn emit(
    printed: &Printed,
    source: &str,
    options: &UfOptions,
) -> Result<Emitted, TransformError> {
    let allocator = Allocator::default();
    let source_type = SourceType::mjs().with_jsx(true);
    let parsed = Parser::new(&allocator, &printed.code, source_type).parse();
    if let Some(error) = parsed.diagnostics.iter().next() {
        return Err(TransformError::Internal(format!(
            "printed module does not parse: {error}\n{}",
            printed.code
        )));
    }
    let mut program = parsed.program;

    let scoping = SemanticBuilder::new()
        .build(&program)
        .semantic
        .into_scoping();

    let mut jsx = JsxOptions {
        runtime: JsxRuntime::Automatic,
        development: options.development,
        import_source: Some(options.jsx_import_source.clone()),
        pure: !options.development,
        refresh: (options.development && options.refresh).then(ReactRefreshOptions::default),
        ..JsxOptions::enable()
    };
    jsx.conform();
    let transform_options = TransformOptions {
        // No down-levelling: the host runs modern JavaScript, and Vite applies
        // its own targets to the bundle.
        env: EnvOptions::from_target("esnext").map_err(|error| {
            TransformError::Internal(format!("oxc rejected the esnext target: {error}"))
        })?,
        jsx,
        ..TransformOptions::default()
    };

    let result = Transformer::new(&allocator, Path::new(&options.filename), &transform_options)
        .build_with_scoping(scoping, &mut program);
    if let Some(error) = result.diagnostics.iter().next() {
        return Err(TransformError::Lowering {
            message: error.to_string(),
            line: None,
            column: None,
        });
    }

    let codegen_options = CodegenOptions {
        source_map_path: options.source_map.then(|| PathBuf::from(&options.filename)),
        ..CodegenOptions::default()
    };
    let generated = Codegen::new().with_options(codegen_options).build(&program);

    let map = if options.source_map {
        generated
            .map
            .as_ref()
            .map(|map| compose(map, &printed.mappings, &options.filename, source))
    } else {
        None
    };

    Ok(Emitted {
        code: generated.code,
        map,
    })
}

/// Compose oxc's map (printed → final) with the printer's mappings
/// (source → printed) into one map (source → final).
fn compose(
    oxc_map: &oxc_sourcemap::SourceMap,
    printer_mappings: &[Mapping],
    filename: &str,
    source: &str,
) -> String {
    let mut builder = SourceMapBuilder::default();
    let source_id = builder.add_source_and_content(filename, source);
    for token in oxc_map.get_tokens() {
        let Some(original) = lookup(printer_mappings, token.get_src_line(), token.get_src_col())
        else {
            continue;
        };
        builder.add_token(
            token.get_dst_line(),
            token.get_dst_col(),
            original.original_line,
            original.original_column,
            Some(source_id),
            None,
        );
    }
    builder.into_sourcemap().to_json_string()
}

/// The printer mapping in effect at a printed position: the last one on the
/// same line at or before the column.
fn lookup(mappings: &[Mapping], line: u32, column: u32) -> Option<Mapping> {
    let index =
        mappings.partition_point(|m| (m.generated_line, m.generated_column) <= (line, column));
    let candidate = mappings.get(index.checked_sub(1)?)?;
    (candidate.generated_line == line).then_some(*candidate)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::print::print;
    use crate::{estree, lower};

    fn emitted(source: &str, options: &UfOptions) -> Emitted {
        let mut program = estree::parse(source).unwrap();
        lower::lower(&mut program, source).unwrap();
        let file = crate::babel::to_babel(program, source).unwrap();
        let printed = print(&file).unwrap();
        emit(&printed, source, options).unwrap()
    }

    #[test]
    fn jsx_becomes_the_automatic_runtime() {
        let out = emitted(
            "export const el = <p className=\"a\">hi</p>;\n",
            &UfOptions::new("a.js"),
        );
        assert!(
            out.code.contains("from \"react/jsx-runtime\""),
            "{}",
            out.code
        );
        assert!(out.code.contains("jsx(\"p\""), "{}", out.code);
        assert!(!out.code.contains("<p"), "{}", out.code);
    }

    #[test]
    fn development_uses_jsxdev_and_registers_refresh() {
        let options = UfOptions {
            development: true,
            refresh: true,
            ..UfOptions::new("App.js")
        };
        let out = emitted("export component App() { return <p>hi</p>; }\n", &options);
        assert!(out.code.contains("react/jsx-dev-runtime"), "{}", out.code);
        assert!(out.code.contains("$RefreshReg$"), "{}", out.code);
    }

    #[test]
    fn the_map_points_at_the_flow_source() {
        let source = "// @flow\nconst a: number = 1;\nexport function f(): number {\n  return <p>{a}</p>;\n}\n";
        let out = emitted(source, &UfOptions::new("f.js"));
        let map: serde_json::Value = serde_json::from_str(out.map.as_deref().unwrap()).unwrap();
        assert_eq!(map["sources"][0], "f.js");
        assert_eq!(map["sourcesContent"][0], source);
        assert!(!map["mappings"].as_str().unwrap().is_empty());
    }

    #[test]
    fn lookup_takes_the_last_mapping_on_the_line() {
        let mappings = [
            Mapping {
                generated_line: 0,
                generated_column: 0,
                original_line: 5,
                original_column: 0,
            },
            Mapping {
                generated_line: 0,
                generated_column: 8,
                original_line: 5,
                original_column: 10,
            },
            Mapping {
                generated_line: 2,
                generated_column: 0,
                original_line: 9,
                original_column: 0,
            },
        ];
        assert_eq!(lookup(&mappings, 0, 9).unwrap().original_column, 10);
        assert_eq!(lookup(&mappings, 0, 3).unwrap().original_column, 0);
        assert!(lookup(&mappings, 1, 0).is_none());
        assert_eq!(lookup(&mappings, 2, 4).unwrap().original_line, 9);
    }
}
