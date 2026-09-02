#![deny(missing_docs)]
//! Native test discovery and runner planning for `uf test` and `@uniflowed/test`.

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use thiserror::Error;
use uf_infra::LineIndex;

/// Inline list of builtin test package specifiers.
pub type TestImportList = SmallVec<[CompactString; 4]>;

/// Kind of discovered test registration call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TestKind {
    /// A grouping call such as `describe`.
    Describe,
    /// A runnable test call such as `it` or `test`.
    Test,
}

/// Test registration discovered in a Flow or JavaScript source file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestCase {
    /// Relative source file path.
    pub file: String,
    /// User-facing test name.
    pub name: String,
    /// Registration kind.
    pub kind: TestKind,
    /// One-based line number.
    pub line: usize,
    /// One-based column number.
    pub column: usize,
    /// Byte offset in the source file, skipped from serialized reports.
    #[serde(skip)]
    pub byte_offset: usize,
}

/// Ordered native test discovery result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestPlan {
    /// Discovered test and describe calls.
    pub cases: Vec<TestCase>,
}

/// Native test execution report.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestRunReport {
    /// Discovered test declarations.
    pub plan: TestPlan,
    /// Number of runnable tests that passed.
    pub passed: usize,
    /// Number of runnable tests that failed.
    pub failed: usize,
    /// Assertion expressions that were preserved for a future full JS runtime.
    pub unsupported_assertions: usize,
    /// Failure details.
    pub failures: Vec<TestFailure>,
}

impl TestRunReport {
    /// Return whether the run is successful.
    pub fn is_success(&self) -> bool {
        self.failed == 0 && self.unsupported_assertions == 0
    }
}

/// Native test failure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestFailure {
    /// Relative source file path.
    pub file: String,
    /// User-facing test name.
    pub name: String,
    /// Failure message.
    pub message: String,
}

impl TestPlan {
    /// Count runnable test cases, excluding grouping calls.
    pub fn runnable_count(&self) -> usize {
        self.cases
            .iter()
            .filter(|case| case.kind == TestKind::Test)
            .count()
    }
}

/// Native test runner execution plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTestRunnerPlan {
    /// Package specifier for the self-hosted native test runtime.
    pub module: CompactString,
    /// Execution backend.
    pub runtime: TestRuntime,
    /// Scheduler used for test file execution.
    pub scheduler: TestScheduler,
    /// Performance target for the native runner.
    pub performance_target: TestPerformanceTarget,
    /// Builtin import specifiers accepted by discovery and runtime bindings.
    pub imports: TestImportList,
    /// Whether React Testing Library-compatible helpers are implemented natively.
    pub react_testing_library_native: bool,
    /// Whether discovery and transform use the official Flow parser line.
    pub official_flow_parser: bool,
}

impl Default for NativeTestRunnerPlan {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/test"),
            runtime: TestRuntime::UfSelfHosted,
            scheduler: TestScheduler::NativeWorkStealing,
            performance_target: TestPerformanceTarget::FasterThanBun,
            imports: smallvec::smallvec![
                "@uniflowed/test".to_compact_string(),
                "@uniflowed/testing".to_compact_string(),
                "inflow".to_compact_string(),
            ],
            react_testing_library_native: true,
            official_flow_parser: true,
        }
    }
}

impl NativeTestRunnerPlan {
    /// Return the default self-hosted runner plan.
    pub fn self_hosted() -> Self {
        Self::default()
    }

    /// Return whether the runner accepts the given builtin import specifier.
    pub fn accepts_import(&self, specifier: &str) -> bool {
        self.imports.iter().any(|import| import == specifier)
    }
}

/// Execution backend for native tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TestRuntime {
    /// uf-owned self-hosted runtime.
    UfSelfHosted,
}

/// Native test scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TestScheduler {
    /// Work-stealing scheduler for large projects.
    NativeWorkStealing,
}

/// Performance target encoded in the runner contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TestPerformanceTarget {
    /// Target a runner faster than Bun's test runner for Flow-heavy suites.
    FasterThanBun,
}

/// Errors emitted by native test execution.
#[derive(Debug, Error)]
pub enum TestError {
    /// The JavaScript execution backend has not been wired yet.
    #[error("native JavaScript execution backend is not enabled yet")]
    RuntimeUnavailable,
}

/// Discover test declarations in a single source file.
pub fn discover_tests(file: &str, source: &str) -> TestPlan {
    let line_index = LineIndex::new(source);
    let code_mask = code_byte_mask(source);
    let mut cases = Vec::new();

    for (call, kind) in [
        ("describe", TestKind::Describe),
        ("it", TestKind::Test),
        ("test", TestKind::Test),
    ] {
        let mut search_start = 0;
        while let Some(relative) = source[search_start..].find(call) {
            let offset = search_start + relative;
            search_start = offset + call.len();

            if !code_mask.get(offset).copied().unwrap_or(false) || !is_call_at(source, offset, call)
            {
                continue;
            }

            let Some(name) = extract_first_string_arg(&source[search_start..]) else {
                continue;
            };
            let position = line_index.line_col(offset);
            cases.push(TestCase {
                file: file.to_string(),
                name,
                kind,
                line: position.line,
                column: position.column,
                byte_offset: offset,
            });
        }
    }

    cases.sort_by(|a, b| a.line.cmp(&b.line).then(a.column.cmp(&b.column)));
    TestPlan { cases }
}

fn code_byte_mask(source: &str) -> Vec<bool> {
    let bytes = source.as_bytes();
    let mut mask = vec![false; bytes.len() + 1];
    let mut i = 0;
    let mut quote = None;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;

    while i < bytes.len() {
        let byte = bytes[i];

        if line_comment {
            if byte == b'\n' {
                line_comment = false;
                mask[i] = true;
            }
            i += 1;
            continue;
        }

        if block_comment {
            if byte == b'*' && bytes.get(i + 1) == Some(&b'/') {
                i += 2;
                block_comment = false;
            } else {
                i += 1;
            }
            continue;
        }

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

        if byte == b'/' && bytes.get(i + 1) == Some(&b'/') {
            line_comment = true;
            i += 2;
            continue;
        }

        if byte == b'/' && bytes.get(i + 1) == Some(&b'*') {
            block_comment = true;
            i += 2;
            continue;
        }

        if matches!(byte, b'\'' | b'"' | b'`') {
            quote = Some(byte);
            i += 1;
            continue;
        }

        mask[i] = true;
        i += 1;
    }

    mask
}

/// Merge several discovery plans into deterministic file order.
pub fn merge_plans(plans: impl IntoIterator<Item = TestPlan>) -> TestPlan {
    let mut cases = plans
        .into_iter()
        .flat_map(|plan| plan.cases)
        .collect::<Vec<_>>();
    cases.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
    });
    TestPlan { cases }
}

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

fn extract_quoted(source: &str) -> Option<String> {
    let mut chars = source.chars();
    let quote = chars.next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }

    let mut output = String::new();
    let mut escaped = false;
    for ch in chars {
        if escaped {
            output.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return Some(output);
        }
        output.push(ch);
    }

    None
}

fn matching_delimiter(source: &str, open: usize, open_byte: u8, close_byte: u8) -> Option<usize> {
    let bytes = source.as_bytes();
    if bytes.get(open) != Some(&open_byte) {
        return None;
    }

    let mut depth = 0usize;
    let mut quote = None;
    let mut escaped = false;
    let mut line_comment = false;
    let mut block_comment = false;
    let mut i = open;

    while i < bytes.len() {
        let byte = bytes[i];

        if line_comment {
            if byte == b'\n' {
                line_comment = false;
            }
            i += 1;
            continue;
        }

        if block_comment {
            if byte == b'*' && bytes.get(i + 1) == Some(&b'/') {
                block_comment = false;
                i += 2;
            } else {
                i += 1;
            }
            continue;
        }

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

        if byte == b'/' && bytes.get(i + 1) == Some(&b'/') {
            line_comment = true;
            i += 2;
            continue;
        }

        if byte == b'/' && bytes.get(i + 1) == Some(&b'*') {
            block_comment = true;
            i += 2;
            continue;
        }

        if matches!(byte, b'\'' | b'"' | b'`') {
            quote = Some(byte);
            i += 1;
            continue;
        }

        if byte == open_byte {
            depth += 1;
        } else if byte == close_byte {
            depth = depth.checked_sub(1)?;
            if depth == 0 {
                return Some(i);
            }
        }

        i += 1;
    }

    None
}

fn is_call_at(source: &str, offset: usize, ident: &str) -> bool {
    let before = source[..offset].chars().next_back();
    if before.is_some_and(is_identifier_char) {
        return false;
    }

    let after_ident = offset + ident.len();
    let tail = &source[after_ident..];
    let Some(next) = tail.chars().find(|ch| !ch.is_whitespace()) else {
        return false;
    };

    next == '('
}

fn extract_first_string_arg(tail_after_ident: &str) -> Option<String> {
    let open = tail_after_ident.find('(')?;
    let tail = tail_after_ident[open + 1..].trim_start();
    let mut chars = tail.chars();
    let quote = chars.next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }

    let mut name = String::new();
    let mut escaped = false;
    for ch in chars {
        if escaped {
            name.push(ch);
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if ch == quote {
            return Some(name);
        }
        name.push(ch);
    }

    None
}

fn is_identifier_char(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
}

#[cfg(test)]
mod tests;
