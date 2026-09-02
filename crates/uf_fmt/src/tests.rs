//! Formatter tests, grouped by the guarantee each group is protecting.
//!
//! The helpers below are shared by every topic. Three of them are assertions
//! about what formatting may not do -- change the token stream, drop or invent a
//! token, or edit a comment -- which is the property most of these tests are
//! really checking underneath their surface expectations.

mod config;
mod encoding;
mod flow;
mod jsx;
mod layout;
mod malformed;
mod property;
mod quote;
mod semicolon;
mod spacing;

use super::*;
use crate::lexer::{Token, TokenKind, tokenize};
use uf_config::QuoteStyle;

/// A default config with `mutate` applied.
///
/// `FmtConfig` is `#[non_exhaustive]`, because every feature that lands adds
/// a knob to it and a struct literal would break on each one.
fn config_with(mutate: impl FnOnce(&mut FmtConfig)) -> FmtConfig {
    let mut config = FmtConfig::default();
    mutate(&mut config);
    config
}

fn format(source: &str) -> String {
    format_source(source, &FmtConfig::default())
        .expect("default config formats")
        .output
}

fn format_with(source: &str, config: &FmtConfig) -> String {
    format_source(source, config)
        .expect("config formats")
        .output
}

/// Canonical spelling of a token, so that a requoted string compares equal to
/// the literal it came from.
fn canonical(token: Token, source: &str) -> String {
    let text = token.text(source);
    match token.kind {
        TokenKind::String | TokenKind::JsxString => {
            let body = text
                .strip_prefix(['\'', '"'])
                .and_then(|rest| rest.strip_suffix(['\'', '"']))
                .unwrap_or(text);
            let mut canonical = String::with_capacity(body.len());
            let mut chars = body.chars();
            while let Some(ch) = chars.next() {
                if ch == '\\' {
                    match chars.next() {
                        Some(next @ ('"' | '\'')) => canonical.push(next),
                        Some(next) => {
                            canonical.push('\\');
                            canonical.push(next);
                        }
                        None => canonical.push('\\'),
                    }
                } else {
                    canonical.push(ch);
                }
            }
            canonical
        }
        _ => text.to_string(),
    }
}

/// The formatter may only change trivia, quote style and semicolons. Anything
/// else is a bug that could silently corrupt a program.
///
/// `>>` and `>>>` are compared as the `>` characters they stand for, because
/// closing nested type arguments legally joins `> >` into `>>`, exactly as a
/// Flow parser splits it again. The printer never inserts characters inside a
/// token, so the character count can only be preserved.
fn assert_token_preserving(input: &str, output: &str) {
    use crate::lexer::Punctuator;

    let interesting = |source: &str| -> Vec<(TokenKind, String)> {
        let mut tokens = Vec::new();
        for token in tokenize(source) {
            if token.kind.is_trivia() || token.kind == TokenKind::Punctuator(Punctuator::Semicolon)
            {
                continue;
            }
            let repeats = match token.kind {
                TokenKind::Punctuator(Punctuator::Greater) => 1,
                TokenKind::Punctuator(Punctuator::GreaterGreater) => 2,
                TokenKind::Punctuator(Punctuator::GreaterGreaterGreater) => 3,
                _ => 0,
            };
            if repeats == 0 {
                tokens.push((token.kind, canonical(token, source)));
                continue;
            }
            for _ in 0..repeats {
                tokens.push((TokenKind::Punctuator(Punctuator::Greater), ">".to_string()));
            }
        }
        tokens
    };
    similar_asserts::assert_eq!(
        interesting(input),
        interesting(output),
        "token stream changed while formatting {input:?}"
    );
}

/// The weaker guarantee that still holds for input that does not lex cleanly.
///
/// Full token preservation cannot: the printer emits exactly one final
/// newline, and for a source ending inside an unterminated comment or string
/// that newline necessarily lands *inside* that token, changing its text.
/// The input was already not valid JavaScript, and the comment stays
/// unterminated either way, so that is harmless.
///
/// What must still hold is that recovery neither drops nor invents a token,
/// which is the failure a formatter could actually cause here.
fn assert_token_kinds_preserved(input: &str, output: &str) {
    use crate::lexer::Punctuator;

    let kinds = |source: &str| -> Vec<TokenKind> {
        tokenize(source)
            .into_iter()
            .map(|token| token.kind)
            .filter(|kind| {
                !kind.is_trivia() && *kind != TokenKind::Punctuator(Punctuator::Semicolon)
            })
            .collect()
    };
    similar_asserts::assert_eq!(
        kinds(input),
        kinds(output),
        "a token was dropped or invented while formatting {input:?}"
    );
}

fn assert_comments_preserved(input: &str, output: &str) {
    let comments = |source: &str| -> Vec<String> {
        tokenize(source)
            .into_iter()
            .filter(|token| token.kind.is_comment())
            .map(|token| token.text(source).to_string())
            .collect()
    };
    assert_eq!(comments(input), comments(output), "comments changed");
}
