//! uf's compile path, stage by stage, with the fixture's options.
//!
//! This is `uf_transform::transform` up to and including the printer — the
//! Flow parser, the lowering rules, `babel.rs`, `scope.rs`, the official
//! `react_compiler` crate and `print.rs` — with two differences, both forced by
//! what is being measured:
//!
//! * the options are the fixture's, from [`crate::pragma`], where
//!   `compiler::compile` builds uf's own;
//! * nothing after the printer runs. `emit.rs` hands the output to oxc for
//!   JSX and source maps, and the snapshots are JSX.
//!
//! Each stage's refusal is kept apart, because each names a different owner.

use react_compiler::entrypoint::PluginOptions;
use serde_json::Value;
use uf_transform::TransformError;
use uf_transform::compiler::{Compiled as Outcome, compile_with_options};
use uf_transform::print::{PrintOptions, print};

use crate::pragma;

/// What the plugin would have done with the module.
pub enum Compiled {
    /// Printed output, and the logger events on the way.
    Code { code: String, events: Vec<Value> },
    /// The message the plugin would have thrown. The test runner writes no
    /// `## Logs` for a transform that threw, so the events stop mattering.
    Thrown { message: String },
}

/// A stage that refused the module before a result existed.
pub enum Refused {
    /// The Flow parser rejected the input.
    Unparsed(String),
    /// The lowering rules or the Babel conversion rejected the tree.
    Conversion(String),
    /// The fixture's options could not be built.
    Options(String),
    /// The crate's AST types rejected uf's tree.
    Schema(String),
    /// The printer rejected the compiled tree.
    Print(String),
}

/// Run one fixture through uf's compile path.
pub fn compile(source: &str, first_line: &str, filename: &str) -> Result<Compiled, Refused> {
    let (program, _) = uf_transform::lowered_ast(source).map_err(|error| match error {
        TransformError::Syntax {
            message,
            line,
            column,
        } => Refused::Unparsed(format!("{line}:{column}: {message}")),
        TransformError::SourceTooLarge { .. } => Refused::Unparsed(error.to_string()),
        other => Refused::Conversion(format!("lowering: {other}")),
    })?;
    let file = uf_transform::babel_from_lowered(program, source)
        .map_err(|error| Refused::Conversion(format!("babel.rs: {error}")))?;

    let options =
        pragma::plugin_options(first_line, filename, source, &file).map_err(Refused::Options)?;
    let options: PluginOptions =
        serde_json::from_value(options).map_err(|error| Refused::Options(error.to_string()))?;

    let scope = uf_transform::scope::analyze(&file);

    // Through `uf_transform`'s one compile entry, so that what this measures is
    // what `uf build` and `uf lint` run rather than a second arrangement of the
    // same calls, which could drift from them and report conformance for a
    // pipeline nobody ships.
    match compile_with_options(&file, scope, options)
        .map_err(|error| Refused::Schema(error.to_string()))?
    {
        Outcome::Ran { ast, events, .. } => {
            let events = events_as_json(&events);
            let compiled = match ast {
                Some(ast) => serde_json::to_value(ast)
                    .map_err(|error| Refused::Schema(format!("compiled AST: {error}")))?,
                None => file,
            };
            let printed = print(&compiled, PrintOptions::default())
                .map_err(|error| Refused::Print(error.to_string()))?;
            Ok(Compiled::Code {
                code: printed.code,
                events,
            })
        }
        Outcome::Fatal { error, .. } => Ok(Compiled::Thrown {
            // `BabelPlugin.ts`: the raw message when there is one, else the
            // pre-formatted one.
            message: error
                .raw_message
                .clone()
                .or_else(|| error.formatted_message.clone())
                .unwrap_or_else(|| "Unexpected compiler error".to_owned()),
        }),
    }
}

fn events_as_json<T: serde::Serialize>(events: &[T]) -> Vec<Value> {
    events
        .iter()
        .map(|event| serde_json::to_value(event).unwrap_or(Value::Null))
        .collect()
}
