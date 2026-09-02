//! Single-pass, lossless tokenizer for Flow-typed JavaScript sources.
//!
//! The tokenizer scans the source exactly once and never drops a byte: every
//! byte of the input belongs to exactly one token, trivia included. Concatenating
//! the source slice of every token therefore reproduces the input byte for byte,
//! which is what lets [`crate::format_source`] guarantee that formatting can only
//! ever rewrite trivia.
//!
//! The scanner is not a parser. It resolves the three classic JavaScript
//! tokenizer ambiguities with the standard "previous significant token" rule plus
//! an explicit context stack:
//!
//! * `/` is a regular expression when an expression may start at that position,
//!   and division otherwise;
//! * `<` starts JSX when an expression may start at that position, and is a
//!   relational/type-argument punctuator otherwise;
//! * `}` resumes a template literal when it closes a `${` interpolation.
//!
//! The context stack is an explicit [`Vec`], never recursion, so pathological
//! inputs such as `{{{{…}}}}` nested ten thousand deep cannot overflow the stack.

mod context;
mod jsx;
mod keyword;
mod literal;
mod operator;
mod punctuator;
mod scanner;
mod token;

#[cfg(test)]
mod tests;

pub use context::BraceKind;
pub use keyword::Keyword;
pub use punctuator::Punctuator;
pub use token::{Span, Token, TokenKind, Unterminated};

pub(crate) use context::{GroupKind, Prev, classify_brace, expression_allowed};

use scanner::Lexer;

/// Tokenize `source` into a lossless token stream.
///
/// The returned tokens tile the whole input: `tokens[0].span.start == 0`, each
/// token starts where the previous one ended, and the last token ends at
/// `source.len()`.
#[must_use]
pub fn tokenize(source: &str) -> Vec<Token> {
    Lexer::new(source).run()
}
