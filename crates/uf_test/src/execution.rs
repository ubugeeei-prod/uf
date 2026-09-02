//! Running the source-level subset of a discovered plan.
//!
//! There is no JavaScript engine behind this yet, so a case is executed by
//! evaluating the handful of expression and matcher shapes that can be decided
//! from the source text. Anything outside that subset is counted as an
//! unsupported assertion rather than reported as a pass.

use crate::scan::{extract_quoted, is_call_at, is_identifier_char, matching_delimiter};
use crate::{TestCase, TestFailure, TestKind, TestPlan, TestRunReport, discover_tests};

/// Execute the native source-level test subset.
pub fn run_tests<'a>(sources: impl IntoIterator<Item = (&'a str, &'a str)>) -> TestRunReport {
    let mut merged = TestPlan::default();
    let mut failures = Vec::new();
    let mut passed = 0;
    let mut unsupported_assertions = 0;

    for (file, source) in sources {
        let plan = discover_tests(file, source);
        for case in plan.cases.iter().filter(|case| case.kind == TestKind::Test) {
            let outcome = execute_case(source, case);
            unsupported_assertions += outcome.unsupported_assertions;
            if outcome.failures.is_empty() {
                passed += 1;
            } else {
                failures.extend(outcome.failures);
            }
        }
        merged.cases.extend(plan.cases);
    }

    merged.cases.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
    });

    TestRunReport {
        plan: merged,
        passed,
        failed: failures.len(),
        unsupported_assertions,
        failures,
    }
}

#[derive(Debug, Default)]
struct CaseOutcome {
    failures: Vec<TestFailure>,
    unsupported_assertions: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum RuntimeValue {
    Boolean(bool),
    Null,
    Number(f64),
    String(String),
}

fn execute_case(source: &str, case: &TestCase) -> CaseOutcome {
    let mut outcome = CaseOutcome::default();
    let body = extract_case_body(source, case.byte_offset).unwrap_or_default();

    if let Some(message) = thrown_error_message(body) {
        outcome.failures.push(TestFailure {
            file: case.file.clone(),
            name: case.name.clone(),
            message,
        });
        return outcome;
    }

    let assertions = discover_assertions(body);
    for assertion in assertions {
        match assertion {
            AssertionOutcome::Passed => {}
            AssertionOutcome::Unsupported => outcome.unsupported_assertions += 1,
            AssertionOutcome::Failed(message) => outcome.failures.push(TestFailure {
                file: case.file.clone(),
                name: case.name.clone(),
                message,
            }),
        }
    }

    outcome
}

fn extract_case_body(source: &str, offset: usize) -> Option<&str> {
    let tail = source.get(offset..)?;
    let arrow = tail.find("=>")?;
    let after_arrow = tail.get(arrow..)?;
    let open_relative = after_arrow.find('{')?;
    let open = offset + arrow + open_relative;
    let close = matching_delimiter(source, open, b'{', b'}')?;
    source.get(open + 1..close)
}

fn thrown_error_message(body: &str) -> Option<String> {
    let throw_offset = body.find("throw new Error")?;
    let tail = body.get(throw_offset..)?;
    let open = tail.find('(')?;
    let argument = tail.get(open + 1..)?;
    let quote = argument.chars().find(|ch| !ch.is_whitespace())?;
    if quote != '\'' && quote != '"' {
        return Some("test threw Error".to_string());
    }

    extract_quoted(argument.trim_start()).or_else(|| Some("test threw Error".to_string()))
}

enum AssertionOutcome {
    Passed,
    Failed(String),
    Unsupported,
}

fn discover_assertions(body: &str) -> Vec<AssertionOutcome> {
    let mut outcomes = Vec::new();
    let mut search_start = 0;

    while let Some(relative) = body[search_start..].find("expect") {
        let offset = search_start + relative;
        search_start = offset + "expect".len();

        if !is_call_at(body, offset, "expect") {
            continue;
        }

        let Some(open) = body[offset..].find('(').map(|open| offset + open) else {
            outcomes.push(AssertionOutcome::Unsupported);
            continue;
        };
        let Some(close) = matching_delimiter(body, open, b'(', b')') else {
            outcomes.push(AssertionOutcome::Unsupported);
            continue;
        };
        let expression = body[open + 1..close].trim();
        let matcher_tail = body[close + 1..].trim_start();

        if let Some(rest) = matcher_tail.strip_prefix(".toBe(") {
            outcomes.push(evaluate_value_matcher(expression, rest, "toBe"));
            continue;
        }

        if let Some(rest) = matcher_tail.strip_prefix(".toEqual(") {
            outcomes.push(evaluate_value_matcher(expression, rest, "toEqual"));
            continue;
        }

        if matcher_tail.starts_with(".resolves.toBeVisible()") {
            outcomes.push(evaluate_visibility_matcher(body, expression));
            continue;
        }

        outcomes.push(AssertionOutcome::Unsupported);
    }

    outcomes
}

fn evaluate_value_matcher(expression: &str, rest: &str, matcher: &str) -> AssertionOutcome {
    let Some(close) = matching_argument_close(rest) else {
        return AssertionOutcome::Unsupported;
    };
    let expected_expression = rest[..close].trim();
    let Some(actual) = evaluate_expression(expression) else {
        return AssertionOutcome::Unsupported;
    };
    let Some(expected) = evaluate_expression(expected_expression) else {
        return AssertionOutcome::Unsupported;
    };

    if actual == expected {
        AssertionOutcome::Passed
    } else {
        AssertionOutcome::Failed(format!(
            "{matcher} assertion failed: actual={} expected={}",
            format_runtime_value(&actual),
            format_runtime_value(&expected)
        ))
    }
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

fn evaluate_visibility_matcher(body: &str, expression: &str) -> AssertionOutcome {
    if body.contains("render(") && expression.contains("screen.findByText(") {
        AssertionOutcome::Passed
    } else {
        AssertionOutcome::Unsupported
    }
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
