//! Turning Flow's printable errors into [`TypeDiagnostic`]s.
//!
//! Flow keeps an error as a tree of message fragments with the locations they
//! reference held out of line, and `error_utils`'s own accessors expose the
//! code, kind, and primary location of one directly. The message tree itself is
//! private to `flow_common_errors`, and `json_output`'s v2 rendering is the
//! only public way to reach it — so that is what this module walks, mapping
//! each fragment back onto a typed segment instead of concatenating it into a
//! sentence.

use compact_str::CompactString;
use flow_common_errors::error_codes::ErrorCode;
use flow_common_errors::error_utils::{
    ConcreteLocPrintableErrorSet, ErrorKind, PrintableError, code_of_printable_error, json_output,
    kind_of_printable_error,
};
use flow_parser::loc::Loc;
use flow_parser::offset_utils::OffsetKind;
use serde_json::Value;
use smallvec::SmallVec;

use crate::diagnostic::{
    DiagnosticKind, MessageFeatures, MessageSegment, Position, RelatedLocation, RelatedLocations,
    Severity, Span, TypeDiagnostic,
};

/// Strip [`super::VIRTUAL_ROOT`] back off, so a diagnostic reports the
/// project-relative path `uf` handed in rather than the synthetic absolute one
/// the port insists on resolving it to.
const STRIP_ROOT: Option<&str> = Some(super::VIRTUAL_ROOT);

/// How deep a message tree is walked before the rest is dropped.
///
/// Speculation errors nest one group per branch, and branch counts come from
/// user-written unions. A bound here is what keeps a hostile union from turning
/// message rendering into unbounded recursion on the check thread.
const MAX_MARKUP_DEPTH: u8 = 16;

/// Convert both error sets into diagnostics, errors first.
///
/// `fallback_path` names the file being checked; it is used only for a
/// diagnostic Flow gave no location at all, which is rare enough that pointing
/// at the head of the file is better than dropping the diagnostic.
pub(super) fn diagnostics(
    errors: &ConcreteLocPrintableErrorSet,
    warnings: &ConcreteLocPrintableErrorSet,
    fallback_path: &str,
) -> Vec<TypeDiagnostic> {
    // `json_of_errors_with_context` walks `errors` then `warnings`, both of
    // which are ordered sets, so the rendered array lines up one-to-one with
    // the same traversal here. Passing no stdin file keeps Flow's byte columns
    // (the code-frame renderer wants bytes) and skips building an offset table
    // per error.
    let rendered = json_output::json_of_errors_with_context(
        STRIP_ROOT,
        &None,
        &[],
        json_output::JsonVersion::JsonV2,
        OffsetKind::Utf8,
        errors,
        warnings,
    );
    let rendered = rendered.as_array().map(Vec::as_slice).unwrap_or_default();

    let printable = errors
        .iter()
        .map(|error| (Severity::Error, error))
        .chain(warnings.iter().map(|error| (Severity::Warning, error)));

    printable
        .zip(rendered)
        .map(|((severity, error), value)| diagnostic(severity, error, value, fallback_path))
        .collect()
}

fn diagnostic(
    severity: Severity,
    error: &PrintableError<Loc>,
    value: &Value,
    fallback_path: &str,
) -> TypeDiagnostic {
    let primary = span(value.get("primaryLoc"))
        .or_else(|| {
            span_of_loc(&flow_common_errors::error_utils::loc_of_printable_error(
                error,
            ))
        })
        .unwrap_or_else(|| Span {
            path: CompactString::new(fallback_path),
            start: Position::START,
            end: Position::START,
        });

    let error_kind = kind_of_printable_error(error);
    let code = code_of_printable_error(error).map(ErrorCode::as_str);
    let mut message = segments(value.get("messageMarkup"));
    strip_code_suffix(&mut message, code, error_kind);

    TypeDiagnostic {
        severity,
        kind: kind(error_kind),
        code,
        primary,
        root: span(value.get("rootLoc")),
        message,
        related: related(value.get("referenceLocs")),
    }
}

/// Drop the ` [error-code]` Flow appends to every rendered message.
///
/// `json_output` renders for a CLI that prints nothing else, so it folds the
/// code into the prose. Here the code is a typed field and the renderer already
/// prints it in the frame header, so leaving it in the message would show it
/// twice — and would be exactly the string-flattening this module exists to
/// avoid. A message that does not end with the suffix is left untouched.
fn strip_code_suffix(segments: &mut MessageFeatures, code: Option<&'static str>, kind: ErrorKind) {
    let mut suffix = String::with_capacity(24);
    suffix.push_str(" [");
    match code {
        Some(code) => suffix.push_str(code),
        // Uncoded errors are labelled with their kind instead; `ErrorKind`'s
        // `Display` is what upstream uses for exactly this.
        None => suffix.push_str(&kind.to_string()),
    }
    suffix.push(']');

    let trailing = segments
        .iter()
        .rev()
        .take_while(|segment| matches!(segment, MessageSegment::Text { .. }))
        .count();
    if trailing == 0 {
        return;
    }
    let tail: String = segments[segments.len() - trailing..]
        .iter()
        .map(MessageSegment::text)
        .collect();
    let Some(kept) = tail.strip_suffix(suffix.as_str()) else {
        return;
    };

    segments.truncate(segments.len() - trailing);
    if !kept.is_empty() {
        segments.push(MessageSegment::Text {
            text: CompactString::new(kept),
        });
    }
}

/// Map Flow's error kinds onto `uf`'s.
///
/// `InferWarning` is an infer error whose name upstream calls outdated; which
/// set an error landed in decides its severity, not its kind.
fn kind(kind: ErrorKind) -> DiagnosticKind {
    match kind {
        ErrorKind::ParseError | ErrorKind::PseudoParseError => DiagnosticKind::Parse,
        ErrorKind::InferError | ErrorKind::InferWarning(_) => DiagnosticKind::Infer,
        ErrorKind::InternalError => DiagnosticKind::Internal,
        ErrorKind::DuplicateProviderError => DiagnosticKind::DuplicateProvider,
        ErrorKind::RecursionLimitError => DiagnosticKind::RecursionLimit,
        ErrorKind::LintError(_) => DiagnosticKind::Lint,
    }
}

/// Read one rendered location.
///
/// `json_of_loc` reports a one-based start column and an exclusive zero-based
/// end column, which are the same number written two different ways. Adding one
/// to the end makes both one-based, so `end.column - start.column` is the byte
/// length of the span.
fn span(value: Option<&Value>) -> Option<Span> {
    let value = value?;
    let path = value.get("source")?.as_str()?;
    let start = value.get("start")?;
    let end = value.get("end")?;
    Some(Span {
        path: CompactString::new(path),
        start: Position {
            line: number(start.get("line")?)?,
            column: number(start.get("column")?)?,
        },
        end: Position {
            line: number(end.get("line")?)?,
            column: number(end.get("column")?)?.saturating_add(1),
        },
    })
}

/// The same conversion from a `Loc`, for the rare error `json_output` renders
/// without a primary location.
fn span_of_loc(loc: &Loc) -> Option<Span> {
    let source = loc.source.as_ref()?;
    Some(Span {
        path: CompactString::new(source.as_str()),
        start: Position {
            line: u32::try_from(loc.start.line).ok()?,
            column: u32::try_from(loc.start.column).ok()?.saturating_add(1),
        },
        end: Position {
            line: u32::try_from(loc.end.line).ok()?,
            column: u32::try_from(loc.end.column).ok()?.saturating_add(1),
        },
    })
}

fn number(value: &Value) -> Option<u32> {
    u32::try_from(value.as_i64()?.max(0)).ok()
}

fn related(value: Option<&Value>) -> RelatedLocations {
    let Some(Value::Object(references)) = value else {
        return RelatedLocations::new();
    };
    let mut locations: RelatedLocations = references
        .iter()
        .filter_map(|(id, loc)| {
            Some(RelatedLocation {
                id: id.parse().ok()?,
                span: span(Some(loc))?,
            })
        })
        .collect();
    // Reference ids are the numbers printed in the message, so they must come
    // out in that order however the renderer keyed them.
    locations.sort_unstable_by_key(|location| location.id);
    locations
}

fn segments(value: Option<&Value>) -> MessageFeatures {
    let mut out = MessageFeatures::new();
    if let Some(value) = value {
        push_markup(&mut out, value, 0);
    }
    out
}

/// Walk one `messageMarkup` node.
///
/// A node is either a flat array of fragments or an `UnorderedList` group whose
/// items are themselves nodes. Groups are joined with a single space so that a
/// one-line rendering reads as a sentence, while the fragments stay separate.
fn push_markup(out: &mut MessageFeatures, value: &Value, depth: u8) {
    if depth >= MAX_MARKUP_DEPTH {
        return;
    }
    match value {
        Value::Array(features) => {
            for feature in features {
                push_feature(out, feature);
            }
        }
        Value::Object(group) => {
            if let Some(message) = group.get("message") {
                push_markup(out, message, depth + 1);
            }
            if let Some(Value::Array(items)) = group.get("items") {
                for item in items {
                    push_separator(out);
                    push_markup(out, item, depth + 1);
                }
            }
            if let Some(post) = group.get("post_message") {
                push_separator(out);
                push_markup(out, post, depth + 1);
            }
        }
        _ => {}
    }
}

fn push_separator(out: &mut MessageFeatures) {
    out.push(MessageSegment::Text {
        text: CompactString::const_new(" "),
    });
}

fn push_feature(out: &mut MessageFeatures, value: &Value) {
    let Some(kind) = value.get("kind").and_then(Value::as_str) else {
        return;
    };
    match kind {
        "Text" => out.push(MessageSegment::Text {
            text: text_of(value),
        }),
        "Code" => out.push(MessageSegment::Code {
            text: text_of(value),
        }),
        "Reference" => {
            let id = value
                .get("referenceId")
                .and_then(Value::as_str)
                .and_then(|id| id.parse().ok())
                .unwrap_or_default();
            let inline: SmallVec<[&str; 4]> = value
                .get("message")
                .and_then(Value::as_array)
                .map(|inlines| {
                    inlines
                        .iter()
                        .filter_map(|inline| inline.get("text").and_then(Value::as_str))
                        .collect()
                })
                .unwrap_or_default();
            out.push(MessageSegment::Reference {
                text: CompactString::new(inline.concat()),
                id,
            });
        }
        _ => {}
    }
}

fn text_of(value: &Value) -> CompactString {
    value
        .get("text")
        .and_then(Value::as_str)
        .map(CompactString::new)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests;
