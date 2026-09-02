//! Token-driven printer.
//!
//! The printer never reorders, drops or invents program tokens. It rewrites
//! trivia — indentation, spacing, blank lines — normalizes string quotes, and
//! adds or removes statement-terminating semicolons under a deliberately
//! conservative rule. Everything else in the output is the input.
//!
//! Layout is produced in two passes over the token stream:
//!
//! 1. [`analyze`] walks the tokens once with an explicit frame stack and records,
//!    for every token, whether a space precedes it, what indentation a line
//!    starting at it would use, and — for opening delimiters — where the group
//!    closes and how wide it would print flat.
//! 2. [`Emitter`] walks the tokens a second time and writes the output, using the
//!    recorded decisions plus the live column to explode groups that would
//!    overflow [`FmtConfig::line_width`].
//!
//! Both passes use explicit stacks, never recursion, so a source nested ten
//! thousand braces deep cannot overflow the native stack.

mod analyze;
mod angle;
mod emit;
mod frame;
mod quote;
mod spacing;
mod statement;

use uf_config::FmtConfig;

use crate::lexer::Token;

use analyze::analyze;
use emit::Emitter;

/// Indentation is capped so that pathological nesting cannot make the printer
/// allocate an unbounded amount of leading whitespace per line.
const MAX_INDENT_LEVELS: u16 = 256;

/// Sentinel for "this opening delimiter never closes".
const NO_MATCH: u32 = u32::MAX;

/// Lay `tokens` out as freshly formatted source text.
pub(crate) fn print(source: &str, tokens: &[Token], config: &FmtConfig) -> String {
    let analysis = analyze(source, tokens);
    Emitter::new(source, tokens, &analysis, config).run()
}
