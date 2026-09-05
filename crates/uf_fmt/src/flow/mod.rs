//! The Flow printer: from the official parser's tree to Prettier's output.
//!
//! The pipeline is parse → attach comments → build a doc → lay it out, and
//! each stage lives in its own module: [`text`] indexes the source,
//! [`node`] gives the tree one shape, [`comments`] decides where each
//! comment belongs, and [`print`] holds the node printers. This module is
//! the seam between them and the crate's public entry point, and it is
//! where the formatter's promise not to lose a comment is enforced: the
//! printer marks every comment it emits, and a comment left unmarked turns
//! the whole run into an error rather than into silently shorter output.

pub mod comments;
pub mod node;
pub mod print;
pub mod text;

use thiserror::Error;
use uf_config::FmtConfig;
use uf_flow::ast::{expression::ExpressionInner as E, jsx};
use uf_flow::{Loc, ParseFailure, Parsed};
use uf_infra::Bump;

use crate::doc::Docs;
use crate::doc::printer::{PrintOptions, print};
use comments::Comments;
use node::{NodeRef, Program};
use print::{Options, Printer};
use text::SourceText;

/// Why a Flow source could not be formatted.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FlowFormatError {
    /// The source did not parse. The file is left as it is: a recovered
    /// tree is the parser's guess, and printing a guess loses code.
    #[error("syntax error at {line}:{column}: {message}")]
    Syntax {
        /// One-based line of the first error.
        line: u32,
        /// Zero-based column of the first error.
        column: u32,
        /// The parser's message.
        message: String,
    },
    /// The source was refused before parsing: too large or too deeply
    /// nested.
    #[error(transparent)]
    Refused(#[from] ParseFailure),
    /// The printer emitted the tree but not every comment in it. This is a
    /// bug in the printer, reported rather than hidden because the
    /// alternative is output that silently dropped a `$FlowFixMe`.
    #[error("{count} comment(s) would have been lost, the first at byte {first_offset}")]
    CommentsLost {
        /// How many comments were not printed.
        count: usize,
        /// Byte offset of the first of them.
        first_offset: usize,
    },
}

/// Format `source` (already normalised to LF line endings, without a BOM)
/// as Flow, returning the text without its final newline handling applied.
pub fn format(source: &str, config: &FmtConfig) -> Result<String, FlowFormatError> {
    let parsed: Parsed = uf_flow::parse(source)?;
    if let Some(diagnostic) = parsed.diagnostics.first() {
        return Err(FlowFormatError::Syntax {
            line: diagnostic.line.unwrap_or(0),
            column: diagnostic.column.unwrap_or(0),
            message: diagnostic.message.clone(),
        });
    }

    let text = SourceText::new(source);

    // The parser recovers some truncated JSX without reporting it, and a
    // recovered tree is missing the closing tag it invented.
    if let Some(opening) = unterminated_jsx(&parsed.program) {
        let name = text.slice(text.span(opening.name.loc()));
        return Err(FlowFormatError::Syntax {
            line: u32::try_from(opening.loc.start.line).unwrap_or(0),
            column: u32::try_from(opening.loc.start.column).unwrap_or(0),
            message: format!("Unexpected end of input, expected the closing tag `</{name}>`"),
        });
    }

    let comments = Comments::attach(&parsed.program, &text);
    let arena = Bump::with_capacity(source.len() * 4 + 4096);
    let docs = Docs::new(&arena);
    let options = Options::from_config(config);
    let mut printer = Printer::new(docs, &text, comments, options, &parsed.program);
    let doc = printer.print_program();

    let unprinted = printer.comments.unprinted();
    if let Some(first) = unprinted.first() {
        return Err(FlowFormatError::CommentsLost {
            count: unprinted.len(),
            first_offset: first.span.start,
        });
    }

    let group_count = printer.docs.group_count();
    Ok(print(
        doc,
        PrintOptions {
            width: options.line_width,
            indent_width: options.indent_width,
        },
        group_count,
    ))
}

/// The opening tag of the first JSX element whose closing tag never arrived.
///
/// Flow's port recovers a truncated element by closing it for the author,
/// and for one shape of truncation — end of input, no trailing newline — it
/// reports no diagnostic while doing so. `format` would then print the
/// recovered tree, which no longer contains a `</div>`, and the output does
/// not parse. Both halves of that break a guarantee `guarantees.rs` states
/// outright: invalid syntax is refused rather than rewritten, and the output
/// parses. See ubugeeei-prod/uf#128.
///
/// The check is exact rather than heuristic. An element with no closing tag
/// and no `/>` is not something anyone can write; the grammar has no such
/// production, so the only way to hold one is to have recovered it. Elements
/// that really are self-closing carry `self_closing`, and they are the
/// common case, so the distinction has to be made on that flag rather than
/// on `closing_element` alone — which is `None` for both.
///
/// One walk of the tree, no allocation past the child buffer, and only on
/// sources that reached the printer. It stops at the first one found:
/// refusing names a position, and a list of positions in a file that will
/// not be written is noise.
fn unterminated_jsx(program: &Program) -> Option<&jsx::Opening<Loc, Loc>> {
    let mut stack = vec![NodeRef::Program(program)];
    let mut children: Vec<NodeRef<'_>> = Vec::new();

    while let Some(node) = stack.pop() {
        if let NodeRef::Expression(expression) = node
            && let E::JSXElement { inner, .. } = &**expression
            && inner.closing_element.is_none()
            && !inner.opening_element.self_closing
        {
            return Some(&inner.opening_element);
        }
        children.clear();
        node.children(&mut children);
        // Reversed, so the stack hands them back in source order and the
        // position reported is the first one in the file rather than an
        // arbitrary one.
        stack.extend(children.iter().rev().copied());
    }

    None
}
