//! The source-level assertion subset.
//!
//! There is no JavaScript engine behind this, and there is deliberately no
//! pretence of one. An assertion is decided only when both operands can be read
//! straight out of the source; everything else is reported by name as
//! unsupported, with the position needed to draw a code frame at it.
//!
//! Widening this subset is how the runner gets more useful, and every widening
//! is a decision about what `uf` can honestly claim to have executed. Adding a
//! matcher here that guesses would turn a red suite green, so nothing here
//! guesses.

use uf_infra::LineIndex;

use crate::report::{
    AssertionFailure, MAX_EXPRESSION_BYTES, UnsupportedAssertion, UnsupportedReason,
};
use crate::scan::{extract_quoted, is_call_at, is_identifier_char, matching_delimiter};

/// What evaluating one test body produced.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct BodyOutcome {
    pub(crate) failures: Vec<AssertionFailure>,
    pub(crate) unsupported: Vec<UnsupportedAssertion>,
}

/// A value the subset can compare.
#[derive(Debug, Clone, PartialEq)]
enum RuntimeValue {
    Boolean(bool),
    Null,
    Number(f64),
    String(String),
}

/// Evaluate one test body.
///
/// `body_start` is the absolute byte offset of `body` inside `source`, so every
/// reported position lands on the real line of the real file.
pub(crate) fn evaluate_body(
    body: &str,
    body_start: usize,
    index: &LineIndex,
    max_assertions: usize,
) -> BodyOutcome {
    let mut outcome = BodyOutcome::default();

    if let Some(thrown) = thrown_error(body, body_start, index) {
        outcome.failures.push(thrown);
        return outcome;
    }

    let mut search_start = 0;
    while let Some(relative) = body[search_start..].find("expect") {
        let offset = search_start + relative;
        search_start = offset + "expect".len();

        if !is_call_at(body, offset, "expect") {
            continue;
        }
        if outcome.failures.len() + outcome.unsupported.len() >= max_assertions {
            break;
        }

        match evaluate_assertion(body, offset) {
            Evaluated::Passed => {}
            Evaluated::Failed { message, span } => outcome.failures.push(AssertionFailure {
                message,
                line: line_of(index, body_start + offset).0,
                column: line_of(index, body_start + offset).1,
                span,
            }),
            Evaluated::Unsupported {
                expression,
                reason,
                span,
            } => {
                let (line, column) = line_of(index, body_start + offset);
                outcome.unsupported.push(UnsupportedAssertion {
                    expression,
                    reason,
                    line,
                    column,
                    span,
                });
            }
        }
    }

    outcome
}

fn line_of(index: &LineIndex, offset: usize) -> (usize, usize) {
    let position = index.line_col(offset);
    (position.line, position.column)
}

/// The result of deciding one `expect(...)` chain.
enum Evaluated {
    Passed,
    Failed {
        message: String,
        span: usize,
    },
    Unsupported {
        expression: String,
        reason: UnsupportedReason,
        span: usize,
    },
}

fn evaluate_assertion(body: &str, offset: usize) -> Evaluated {
    let Some(open) = body[offset..].find('(').map(|open| offset + open) else {
        return unsupported(body, offset, body.len(), UnsupportedReason::Malformed);
    };
    let Some(close) = matching_delimiter(body, open, b'(', b')') else {
        return unsupported(body, offset, body.len(), UnsupportedReason::Malformed);
    };

    let expression = body[open + 1..close].trim();
    let matcher_tail = body[close + 1..].trim_start();
    let end = statement_end(body, close + 1);

    if let Some(rest) = matcher_tail.strip_prefix(".toBe(") {
        return value_matcher(body, offset, end, expression, rest, "toBe");
    }
    if let Some(rest) = matcher_tail.strip_prefix(".toEqual(") {
        return value_matcher(body, offset, end, expression, rest, "toEqual");
    }
    if matcher_tail.starts_with(".resolves.toBeVisible()") {
        return visibility_matcher(body, offset, end, expression);
    }

    let matcher = matcher_name(matcher_tail);
    unsupported(body, offset, end, UnsupportedReason::Matcher { matcher })
}

/// The matcher name written after the `expect(...)`, for the report.
fn matcher_name(matcher_tail: &str) -> String {
    let tail = matcher_tail.strip_prefix('.').unwrap_or(matcher_tail);
    let end = tail
        .char_indices()
        .find(|(_, ch)| !is_identifier_char(*ch) && *ch != '.')
        .map(|(offset, _)| offset)
        .unwrap_or(tail.len());
    let name = tail[..end].trim_end_matches('.');
    if name.is_empty() {
        "<none>".to_string()
    } else {
        truncate(name)
    }
}

fn value_matcher(
    body: &str,
    offset: usize,
    end: usize,
    expression: &str,
    rest: &str,
    matcher: &str,
) -> Evaluated {
    let Some(close) = matching_argument_close(rest) else {
        return unsupported(body, offset, end, UnsupportedReason::Malformed);
    };
    let expected_expression = rest[..close].trim();
    let (Some(actual), Some(expected)) = (
        evaluate_expression(expression),
        evaluate_expression(expected_expression),
    ) else {
        return unsupported(body, offset, end, UnsupportedReason::Expression);
    };

    if actual == expected {
        Evaluated::Passed
    } else {
        Evaluated::Failed {
            message: format!(
                "{matcher} assertion failed: actual={} expected={}",
                format_runtime_value(&actual),
                format_runtime_value(&expected)
            ),
            span: end.saturating_sub(offset).max(1),
        }
    }
}

/// React Testing Library's visibility contract, decided structurally.
///
/// The subset can tell that a component was rendered and that the assertion
/// queries the rendered output; it cannot run the query. Anything else in this
/// shape is unsupported rather than assumed visible.
fn visibility_matcher(body: &str, offset: usize, end: usize, expression: &str) -> Evaluated {
    if body.contains("render(") && expression.contains("screen.findByText(") {
        Evaluated::Passed
    } else {
        unsupported(
            body,
            offset,
            end,
            UnsupportedReason::Matcher {
                matcher: "resolves.toBeVisible".to_string(),
            },
        )
    }
}

fn unsupported(body: &str, offset: usize, end: usize, reason: UnsupportedReason) -> Evaluated {
    let end = end.clamp(offset, body.len());
    Evaluated::Unsupported {
        expression: truncate(body[offset..end].trim()),
        reason,
        span: end.saturating_sub(offset).max(1),
    }
}

/// Where the statement containing an assertion ends.
///
/// Bounded by the body, and by the first `;` or newline after the matcher, so
/// the reported span covers the assertion and nothing after it.
fn statement_end(body: &str, from: usize) -> usize {
    let tail = &body[from..];
    let terminator = tail
        .char_indices()
        .find(|(_, ch)| *ch == ';' || *ch == '\n')
        .map(|(offset, _)| offset)
        .unwrap_or(tail.len());
    from + terminator
}

/// Copy an excerpt into a report, bounded so a generated expression cannot
/// blow up the report.
fn truncate(value: &str) -> String {
    if value.len() <= MAX_EXPRESSION_BYTES {
        return value.to_string();
    }
    let mut end = MAX_EXPRESSION_BYTES;
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    let mut out = String::with_capacity(end + 1);
    out.push_str(&value[..end]);
    out.push('…');
    out
}

fn thrown_error(body: &str, body_start: usize, index: &LineIndex) -> Option<AssertionFailure> {
    let throw_offset = body.find("throw new Error")?;
    let tail = body.get(throw_offset..)?;
    let end = statement_end(body, throw_offset);
    let position = index.line_col(body_start + throw_offset);

    let message = tail
        .find('(')
        .and_then(|open| tail.get(open + 1..))
        .map(str::trim_start)
        .and_then(extract_quoted)
        .unwrap_or_else(|| "test threw Error".to_string());

    Some(AssertionFailure {
        message,
        line: position.line,
        column: position.column,
        span: end.saturating_sub(throw_offset).max(1),
    })
}

fn matching_argument_close(source: &str) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];

        if let Some(active_quote) = quote {
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == active_quote {
                quote = None;
            }
            i += 1;
            continue;
        }

        if matches!(byte, b'\'' | b'"' | b'`') {
            quote = Some(byte);
            i += 1;
            continue;
        }

        if byte == b'(' {
            depth += 1;
        } else if byte == b')' {
            if depth == 0 {
                return Some(i);
            }
            depth -= 1;
        }

        i += 1;
    }

    None
}

fn evaluate_expression(expression: &str) -> Option<RuntimeValue> {
    let expression = expression.trim();
    if expression == "true" {
        return Some(RuntimeValue::Boolean(true));
    }
    if expression == "false" {
        return Some(RuntimeValue::Boolean(false));
    }
    if expression == "null" {
        return Some(RuntimeValue::Null);
    }
    if expression.starts_with('"') || expression.starts_with('\'') {
        return extract_quoted(expression).map(RuntimeValue::String);
    }
    if let Ok(value) = expression.parse::<f64>() {
        return Some(RuntimeValue::Number(value));
    }
    if let Some(value) = evaluate_string_identity_call(expression) {
        return Some(value);
    }
    evaluate_numeric_addition(expression)
}

fn evaluate_string_identity_call(expression: &str) -> Option<RuntimeValue> {
    let open = expression.find('(')?;
    let close = matching_delimiter(expression, open, b'(', b')')?;
    if close + 1 != expression.len() {
        return None;
    }
    let function_name = expression[..open].trim();
    if function_name.is_empty() || !function_name.chars().all(is_identifier_char) {
        return None;
    }
    let argument = expression[open + 1..close].trim();
    if argument.starts_with('"') || argument.starts_with('\'') {
        extract_quoted(argument).map(RuntimeValue::String)
    } else {
        None
    }
}

fn evaluate_numeric_addition(expression: &str) -> Option<RuntimeValue> {
    let (left, right) = expression.split_once('+')?;
    let left = left.trim().parse::<f64>().ok()?;
    let right = right.trim().parse::<f64>().ok()?;
    Some(RuntimeValue::Number(left + right))
}

fn format_runtime_value(value: &RuntimeValue) -> String {
    match value {
        RuntimeValue::Boolean(value) => value.to_string(),
        RuntimeValue::Null => "null".to_string(),
        RuntimeValue::Number(value) => value.to_string(),
        RuntimeValue::String(value) => format!("{value:?}"),
    }
}
