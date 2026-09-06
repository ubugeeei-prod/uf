//! GraphQL: a lexer, a parser and a printer, for the text inside a tagged
//! template.
//!
//! Prettier formats the contents of a template whose tag it recognises and
//! re-indents them to the code around it; `uf fmt` does the same for the one
//! embedded language this toolchain has a reason to know, which is GraphQL.
//! Which templates those are, and how the result is spliced back into the
//! JavaScript, is the Flow printer's `embed` module's business — this module
//! knows only the language.
//!
//! The printer is Prettier's `printer-graphql` ported onto uf's document IR
//! ([`crate::doc`]), so its layout is reproducible arm for arm rather than
//! approximated: the fixtures beside it are Prettier's own output.
//!
//! **What is not here is as important as what is.** A document this module
//! cannot reproduce byte for byte is [`Declined`], and the caller's answer to
//! that is to leave the template exactly as the author wrote it. Emitting
//! something close would be worse than emitting nothing: a formatter that
//! rewrites a query into a different query is a formatter nobody can run.
//! The declined cases are a syntax error, a nesting ceiling, and — the one
//! that is a decision rather than a limit — any document holding a `#`
//! comment. See [`Declined::Comment`].

pub mod ast;
mod lexer;
mod parser;
mod printer;

#[cfg(test)]
mod tests;

pub use ast::Document;
pub use parser::{Declined, parse};

use std::borrow::Cow;

use crate::doc::{Doc, Docs};
use lexer::{Lexer, TokenKind};

/// The GraphQL tokens of `source`, rendered so that two spellings of the
/// same document produce the same string, or [`None`] when `source` is not
/// GraphQL at all.
///
/// This exists for the formatter's tree-preservation guarantee, which says
/// the output re-parses to the same tree as the input. Formatting the
/// inside of a template rewrites a string literal's contents, so that
/// guarantee has to say what a template's text *means* rather than compare
/// it byte for byte — and what it means is its token sequence: GraphQL
/// treats commas and every run of whitespace as nothing, and Prettier
/// reprints a string value from the value rather than from the source
/// spelling, so `"\u0041"` and `"A"` are the same token.
///
/// Everything else is compared. A dropped directive, a reordered
/// selection, a renamed field, a changed number — each is a different token
/// sequence and each fails the guarantee, which is the point. Text that
/// does not lex is not GraphQL, has no signature, and is compared as text.
///
/// ```
/// use uf_fmt::graphql::token_signature;
///
/// assert_eq!(
///     token_signature("{ a, b }"),
///     token_signature("{\n  a\n  b\n}"),
/// );
/// assert_ne!(token_signature("{ a }"), token_signature("{ b }"));
/// assert_eq!(token_signature("union U = A | B"), token_signature("union U =\n  | A\n  | B"));
/// assert_eq!(token_signature("SELECT * FROM t"), None);
/// ```
#[must_use]
pub fn token_signature(source: &str) -> Option<String> {
    let mut lexer = Lexer::new(source);
    let mut signature = String::new();
    // The `|` in `union U = | A | B` and `directive @d on | FIELD`, and the
    // `&` in `type T implements & A & B`, are optional in the grammar.
    // Prettier writes the `|` or not depending on whether the line broke,
    // and never writes the `&`. Neither says anything, so neither is in the
    // signature — and only in that one position: every other `|` and `&`
    // separates two members, and dropping one would hide a real change.
    let mut leading_separator: Option<TokenKind> = None;
    loop {
        let token = lexer.next_token().ok()?;
        let next_leading_separator = match token.kind {
            TokenKind::Equals => Some(TokenKind::Pipe),
            TokenKind::Name if token.text == "on" => Some(TokenKind::Pipe),
            TokenKind::Name if token.text == "implements" => Some(TokenKind::Amp),
            _ => None,
        };
        if Some(token.kind) == leading_separator {
            leading_separator = None;
            continue;
        }
        leading_separator = next_leading_separator;
        match token.kind {
            TokenKind::Eof => return Some(signature),
            // A string is its value: Prettier prints the value, so the
            // source spelling of an escape is not part of what it says.
            // Whether it was written as a block string is, because that is
            // a spelling Prettier keeps.
            TokenKind::String | TokenKind::BlockString => {
                let value = token.string.as_ref()?;
                signature.push_str(if value.block { "block" } else { "str" });
                // A one-line block string is trimmed on the way out —
                // `"""  x  """` prints as `x` — so its surrounding spaces
                // are not what it says. Prettier's own AST comparison does
                // exactly this (`massageAstNode`), for exactly this reason.
                let text = if value.block && !value.value.contains('\n') {
                    value.value.trim()
                } else {
                    &value.value
                };
                signature.push_str(&format!("{text:?}"));
            }
            // Trailing whitespace inside a comment is not content; the
            // comments-only path trims it.
            TokenKind::Comment => {
                signature.push_str(token.text.trim_end());
            }
            _ => signature.push_str(token.text),
        }
        signature.push(' ');
    }
}

/// Escape `text` for the template literal it is about to be spliced into.
///
/// Prettier's `escapeTemplateCharacters`, which it runs over the whole
/// embedded doc once the language printer has produced it: a backslash, a
/// backtick and a `${` mean something to JavaScript that they do not mean
/// to GraphQL, and writing one back unescaped would change what the
/// template says — or end it early. The GraphQL printer prints a string
/// from its *value*, so `"a\"b"` really does come back out with a
/// backslash in it, and this is what makes that legal again.
///
/// Borrows when there is nothing to change, which is almost always.
pub(crate) fn escape_template_characters(text: &str) -> Cow<'_, str> {
    if !text.contains(['\\', '`']) && !text.contains("${") {
        return Cow::Borrowed(text);
    }
    let mut escaped = String::with_capacity(text.len() + 8);
    let mut rest = text;
    while let Some(at) = rest.find(['\\', '`', '$']) {
        let (before, tail) = rest.split_at(at);
        escaped.push_str(before);
        let mut chars = tail.chars();
        let ch = chars.next().expect("find landed on a character");
        if ch == '$' && !tail.starts_with("${") {
            escaped.push('$');
        } else {
            escaped.push('\\');
            escaped.push(ch);
        }
        rest = chars.as_str();
    }
    escaped.push_str(rest);
    Cow::Owned(escaped)
}

/// Parse `source` as a GraphQL document and print it as a doc.
///
/// # Errors
///
/// Returns [`Declined`] when the document is not one this module reproduces
/// exactly; the caller leaves the template unformatted.
pub(crate) fn format<'a>(docs: &Docs<'a>, source: &str) -> Result<Doc<'a>, Declined> {
    let document = parse(source)?;
    Ok(printer::print(docs, source, &document))
}
