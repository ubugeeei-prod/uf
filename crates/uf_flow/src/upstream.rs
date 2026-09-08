//! Meta's official Flow Rust port (`upstream/flow/rust_port`).
//!
//! It understands modern Flow syntax (`component`, `hook`, `renders`, `match`,
//! enums) natively, so nothing rewrites the source before parsing and every
//! diagnostic points at the location the user actually wrote.
//!
//! # The ceilings apply here too
//!
//! This is the entry point the linter uses, and for a long time it was the
//! careless one: it handed the source straight to the parser with no size
//! limit, no depth limits, no `catch_unwind`, and on whatever thread the caller
//! was on. [`parse`](crate::parse) has had all four since ubugeeei-prod/uf#136,
//! twenty lines away, with comments explaining that a tool running over a whole
//! project must see one refused file rather than its own crash.
//!
//! So `uf fmt --check` said
//!
//! ```text
//! - deep.js: operators chain 600001 deep, over the 10000 level ceiling
//! ```
//!
//! and `uf lint`, on the same file, said `fatal runtime error: stack overflow,
//! aborting` and took the process with it. A minified bundle in `node_modules`
//! is exactly that shape. See ubugeeei-prod/uf#231.
//!
//! The difference from [`parse`](crate::parse) is what a refusal *is*. A
//! formatter that cannot parse a file must not rewrite it, so it gets a typed
//! error and stops. A linter that cannot parse a file has something to say
//! about it and thirty thousand other files to get through, so a refusal is a
//! diagnostic like any other and the run continues.

use std::panic::{AssertUnwindSafe, catch_unwind};

use flow_parser::loc::Loc;
use flow_parser::parse_error::ParseError;

use crate::parse::{
    MAX_CHAIN_DEPTH, MAX_NESTING_DEPTH, MAX_PARSE_BYTES, PARSE_STACK_BYTES, ParseFailure, depths,
};
use crate::{FlowError, ParseDiagnostic, ParseOutcome};

pub(crate) fn validate_source(source: &str) -> Result<ParseOutcome, FlowError> {
    if let Some(refusal) = refuses(source) {
        return Ok(ParseOutcome {
            diagnostics: vec![ParseDiagnostic {
                message: refusal.to_string(),
                line: None,
                column: None,
            }],
        });
    }

    // On a thread of its own, sized like the one `uf fmt` uses. The ceilings
    // above bound the *tree*, and the frames the port spends getting there are
    // large — an object literal costs about 150 KiB a level — so a source well
    // inside every ceiling can still want more stack than a linter thread has.
    // The tree is dropped inside the closure, so it is freed here too, which is
    // the other half of the same hazard (ubugeeei-prod/uf#155).
    //
    // A thread per file, unconditionally, rather than one only for deep
    // sources: a threshold is a number somebody has to justify and keep true,
    // and this costs nothing worth having one for. `uf lint` over this
    // repository's 280 files, five runs each after a warm-up: 400, 414, 416,
    // 426 ms without the thread and 412, 416, 420, 421, 424 ms with it — one
    // binary's spread against itself is wider than the gap.
    let diagnostics = std::thread::scope(|scope| {
        std::thread::Builder::new()
            .stack_size(PARSE_STACK_BYTES)
            .spawn_scoped(scope, || parse_for_diagnostics(source))
            .map_err(|error| FlowError::Initialize(error.to_string()))?
            .join()
            .map_err(|_| FlowError::Runtime("the Flow parser thread panicked".to_owned()))
    })?;

    Ok(ParseOutcome { diagnostics })
}

/// The reason this source is not parsed at all, or [`None`] to go ahead.
///
/// Every one of these is decided from the text before the parser runs, which is
/// the point: the tree that would overflow the stack is never built.
fn refuses(source: &str) -> Option<ParseFailure> {
    if source.len() > MAX_PARSE_BYTES {
        return Some(ParseFailure::SourceTooLarge {
            bytes: source.len(),
            limit: MAX_PARSE_BYTES,
        });
    }
    let depths = depths(source);
    if depths.brackets > MAX_NESTING_DEPTH {
        return Some(ParseFailure::TooDeeplyNested {
            depth: depths.brackets,
            limit: MAX_NESTING_DEPTH,
        });
    }
    if depths.chain > MAX_CHAIN_DEPTH {
        return Some(ParseFailure::TooDeeplyChained {
            depth: depths.chain,
            limit: MAX_CHAIN_DEPTH,
        });
    }
    None
}

/// Parse and keep only what the linter asked for.
///
/// The tree is built and dropped here rather than returned: `ParseOutcome`
/// carries diagnostics, and handing the tree back across the thread boundary
/// would move the free onto the caller's stack, which is where it does not fit.
fn parse_for_diagnostics(source: &str) -> Vec<ParseDiagnostic> {
    let parsed = catch_unwind(AssertUnwindSafe(|| {
        let (_program, errors): (_, Vec<(Loc, ParseError)>) = crate::module::parse(source, None);
        errors
            .iter()
            .map(|error| crate::diagnostic_from_error(source, error))
            .collect::<Vec<_>>()
    }));

    // A panic is not expected for any input, and a parser is the part of a
    // toolchain most exposed to hostile bytes. `uf fmt` reports it as a refused
    // file; here it is one more thing to say about one file.
    parsed.unwrap_or_else(|_| {
        vec![ParseDiagnostic {
            message: ParseFailure::ParserPanicked.to_string(),
            line: None,
            column: None,
        }]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::validate_source;

    /// A source each ceiling refuses, named by the ceiling it trips.
    ///
    /// A table over both entry points, because that is the bug: two ways into
    /// the same parser, one careful and one not, and nothing that compared
    /// them. A ceiling added to [`parse`](crate::parse) and not here fails
    /// below by name.
    fn refused() -> [(&'static str, String); 3] {
        [
            ("bytes", "x".repeat(MAX_PARSE_BYTES + 1)),
            (
                "brackets",
                format!(
                    "x = {}1{};",
                    "(".repeat(MAX_NESTING_DEPTH + 1),
                    ")".repeat(MAX_NESTING_DEPTH + 1)
                ),
            ),
            ("chain", format!("x = a{};", ".f()".repeat(MAX_CHAIN_DEPTH))),
        ]
    }

    #[test]
    fn every_ceiling_refuses_the_same_source_the_formatter_refuses() {
        for (what, source) in refused() {
            let failure = crate::parse(&source)
                .err()
                .unwrap_or_else(|| panic!("`parse` accepted a source over the {what} ceiling"));

            let outcome = validate_source(&source).expect("the parser backend is always available");
            let diagnostics = outcome.diagnostics;

            assert_eq!(
                diagnostics.len(),
                1,
                "the {what} ceiling should be one diagnostic: {diagnostics:?}"
            );
            assert_eq!(
                diagnostics[0].message,
                failure.to_string(),
                "`uf lint` and `uf fmt` describe the {what} ceiling differently"
            );
        }
    }

    #[test]
    fn a_refused_source_is_a_diagnostic_rather_than_an_error() {
        // The whole difference between this entry point and `parse`. A
        // formatter that cannot parse a file must not rewrite it and stops; a
        // linter has something to say about the file and thirty thousand more
        // to get through.
        let source = format!("x = a{};", ".f()".repeat(MAX_CHAIN_DEPTH));
        let outcome = validate_source(&source).expect("a refusal is not a backend error");
        assert!(!outcome.is_ok());
        assert!(outcome.diagnostics[0].message.contains("ceiling"));
    }

    #[test]
    fn a_source_inside_the_ceilings_parses_from_a_small_thread() {
        // Nesting at the ceiling costs about 150 KiB of stack a level in the
        // port, so this is some 45 MiB of frames on a thread that has 2 MiB —
        // which is what a linter thread gets. `validate_source` runs the parser
        // on a thread of `PARSE_STACK_BYTES` and the tree is freed there too.
        //
        // Without that, this does not fail: it *aborts*, taking the test binary
        // with it, because a stack overflow cannot be caught. That is the shape
        // of the bug rather than a shortcoming of the test — `uf lint` exited
        // 134 on a file `uf fmt` refused politely. A named failure would need a
        // child process, which ubugeeei-prod/uf#230 builds for the same hazard
        // one function over.
        let source = format!(
            "x = {}1{};",
            "(".repeat(MAX_NESTING_DEPTH),
            ")".repeat(MAX_NESTING_DEPTH)
        );
        let outcome = std::thread::Builder::new()
            .stack_size(2 * 1024 * 1024)
            .spawn(move || validate_source(&source))
            .expect("a thread")
            .join()
            .expect("the caller's thread survives")
            .expect("the parser backend is always available");

        assert!(outcome.is_ok(), "{:?}", outcome.diagnostics);
    }
}
