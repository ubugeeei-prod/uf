//! The file-level directive prologue, and everything that only looks like one.
//!
//! A directive counts only while the prologue is still running: a run of string
//! statements before any other code. The same walk therefore has to explain
//! itself, which is why the pass that accepts directives and the pass that
//! reports the near-misses — a directive after a statement, a concatenated
//! one, a template literal — live together here.

use uf_infra::LineIndex;

use crate::scan::{Token, TokenKind, starts_statement};

use super::{
    DirectiveIssue, DirectiveKind, DirectiveScan, FileDirective, line_column, terminates_statement,
};

/// Walk the directive prologue, returning the token index it ends at.
pub(crate) fn scan_prologue(
    source: &str,
    tokens: &[Token],
    index: &LineIndex,
    consumed: &mut [bool],
    scan: &mut DirectiveScan,
) -> usize {
    let mut position = 0usize;

    while let Some(token) = tokens.get(position) {
        if token.kind != TokenKind::String {
            break;
        }
        let directive = DirectiveKind::from_content(token.quoted_content(source));
        let (line, column) = line_column(index, token);

        if !terminates_statement(tokens, position) {
            // `"use client" + ""` is an expression, not a directive. Report it so
            // the author is not left believing the module is a Client Component.
            if let Some(kind) = directive {
                consumed[position] = true;
                scan.issues
                    .push(DirectiveIssue::NotAStringLiteral { kind, line, column });
            }
            break;
        }

        consumed[position] = true;
        if let Some(kind) = directive {
            match scan.file_directive {
                None => scan.file_directive = Some(FileDirective { kind, line, column }),
                Some(existing) if existing.kind != kind => {
                    scan.issues
                        .push(DirectiveIssue::Conflicting { line, column });
                }
                Some(_) => {}
            }
        }

        position += 1;
        if tokens.get(position).is_some_and(|next| next.is_punct(b';')) {
            consumed[position] = true;
            position += 1;
        }
    }

    position
}

/// Report directive-shaped constructs that the two passes above did not accept.
pub(crate) fn scan_misplaced_directives(
    source: &str,
    tokens: &[Token],
    index: &LineIndex,
    consumed: &[bool],
    prologue_end: usize,
    scan: &mut DirectiveScan,
) {
    for (position, token) in tokens.iter().enumerate() {
        if consumed[position] {
            continue;
        }
        let content = match token.kind {
            TokenKind::String | TokenKind::Template => token.quoted_content(source),
            _ => continue,
        };
        let Some(kind) = DirectiveKind::from_content(content) else {
            continue;
        };
        if !starts_statement(tokens, position) {
            continue;
        }
        let (line, column) = line_column(index, token);

        if token.kind == TokenKind::Template || !terminates_statement(tokens, position) {
            scan.issues
                .push(DirectiveIssue::NotAStringLiteral { kind, line, column });
        } else if position >= prologue_end {
            scan.issues
                .push(DirectiveIssue::NotInPrologue { kind, line, column });
        }
    }
}
