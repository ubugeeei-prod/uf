//! Scanner tests, grouped by the ambiguity or literal family each one pins down.
//!
//! The two helpers below are shared by every topic: one keeps trivia, the other
//! drops it, because most rules are about the significant tokens alone.

mod jsx;
mod literal;
mod punctuation;
mod regex;
mod stream;

use super::{TokenKind, tokenize};

/// Every token kind the scanner produced, trivia included.
fn kinds(source: &str) -> Vec<TokenKind> {
    tokenize(source).into_iter().map(|t| t.kind).collect()
}

/// Every token kind that carries program meaning.
fn significant(source: &str) -> Vec<TokenKind> {
    tokenize(source)
        .into_iter()
        .filter(|token| !token.kind.is_trivia())
        .map(|token| token.kind)
        .collect()
}
