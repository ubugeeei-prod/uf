//! Executing one file's worth of declarations.
//!
//! A file is the unit of scheduling, of budgeting and of failure isolation:
//! whatever happens inside it, the run keeps going and the file is named. Three
//! things can go wrong before a single assertion is read, and each is a
//! [`FileStatus`] rather than an error that stops the run — the file is too
//! large to scan, it exhausted its wall-clock budget, or executing it panicked.

use std::panic::AssertUnwindSafe;
use std::time::{Duration, Instant};

use uf_infra::LineIndex;

use crate::assertion::{BodyOutcome, evaluate_body};
use crate::discovery::discover_tests;
use crate::filter::TestFilter;
use crate::options::RunOptions;
use crate::plan::{PlanResolution, Selection, SkipReason, TestCase, TestKind, TestPlan};
use crate::report::{FileReport, FileStatus, TestRecord, TestStatus};
use crate::retry_schedule::Attempt;
use crate::scan::matching_delimiter;

/// Discover and execute one file.
///
/// Never panics and never returns an error: a file that misbehaves is reported.
pub(crate) fn execute_file(
    file: &str,
    source: &str,
    options: &RunOptions,
    filter: &TestFilter,
) -> (TestPlan, FileReport) {
    let started = Instant::now();
    let limit = options.effective_max_source_bytes();
    if source.len() > limit {
        return (
            TestPlan::default(),
            FileReport {
                file: file.to_string(),
                status: FileStatus::TooLarge {
                    bytes: source.len(),
                    limit,
                },
                duration_micros: micros_since(started),
                records: Vec::new(),
            },
        );
    }

    // Isolation, not paranoia: the scanners are total on any UTF-8 input, but a
    // runner whose own crash takes down a thousand-file run is worse than one
    // that names the file it choked on. Release builds set `panic = "abort"`,
    // so in those this is inert and the real guard is the bounded work above
    // and the deadline below.
    let outcome = std::panic::catch_unwind(AssertUnwindSafe(|| {
        run_declarations(file, source, options, filter, started)
    }));

    match outcome {
        Ok((plan, status, records)) => (
            plan,
            FileReport {
                file: file.to_string(),
                status,
                duration_micros: micros_since(started),
                records,
            },
        ),
        Err(payload) => (
            TestPlan::default(),
            FileReport {
                file: file.to_string(),
                status: FileStatus::Panicked {
                    message: panic_message(&payload),
                },
                duration_micros: micros_since(started),
                records: Vec::new(),
            },
        ),
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        return (*message).to_string();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    "non-string panic payload".to_string()
}

fn run_declarations(
    file: &str,
    source: &str,
    options: &RunOptions,
    filter: &TestFilter,
    started: Instant,
) -> (TestPlan, FileStatus, Vec<TestRecord>) {
    let plan = discover_tests(file, source);
    let mut resolution = plan.resolve();
    apply_filter(&plan, &mut resolution, filter);

    let index = LineIndex::new(source);
    let budget = options.effective_file_timeout();
    let max_assertions = options.effective_max_assertions();
    let mut records = Vec::with_capacity(plan.runnable_count());
    let mut status = FileStatus::Completed;
    let mut name = String::new();

    for (position, case) in plan.cases.iter().enumerate() {
        if case.kind != TestKind::Test {
            continue;
        }
        if started.elapsed() >= budget {
            status = FileStatus::TimedOut {
                budget_micros: duration_micros(budget),
            };
            break;
        }

        name.clear();
        resolution.push_full_name(&plan, position, &mut name);

        let record = match resolution.selection(position) {
            Selection::Skipped(reason) => TestRecord {
                file: case.file.clone(),
                name: name.clone(),
                line: case.line,
                column: case.column,
                status: TestStatus::Skipped { reason },
                attempts: 0,
            },
            Selection::Todo => TestRecord {
                file: case.file.clone(),
                name: name.clone(),
                line: case.line,
                column: case.column,
                status: TestStatus::Todo,
                attempts: 0,
            },
            Selection::Run => {
                let (status, attempts) =
                    execute_with_retries(source, case, &index, options, max_assertions);
                TestRecord {
                    file: case.file.clone(),
                    name: name.clone(),
                    line: case.line,
                    column: case.column,
                    status,
                    attempts,
                }
            }
        };
        records.push(record);
    }

    (plan, status, records)
}

/// Narrow a resolution with the command-line filter.
///
/// A `describe` is never itself run, so only test declarations are considered;
/// the filter sees the fully qualified name, which is what a developer types.
fn apply_filter(plan: &TestPlan, resolution: &mut PlanResolution, filter: &TestFilter) {
    if filter.name_pattern().is_none() {
        return;
    }
    let mut name = String::new();
    for (position, case) in plan.cases.iter().enumerate() {
        if case.kind != TestKind::Test || !resolution.selection(position).is_run() {
            continue;
        }
        name.clear();
        resolution.push_full_name(plan, position, &mut name);
        if !filter.matches_name(&name) {
            resolution.set_selection(position, Selection::Skipped(SkipReason::Filtered));
        }
    }
}

/// Execute one case, retrying while the policy says to.
fn execute_with_retries(
    source: &str,
    case: &TestCase,
    index: &LineIndex,
    options: &RunOptions,
    max_assertions: usize,
) -> (TestStatus, u32) {
    // `retries` follows the `Schedule` convention: it counts retries already
    // made, so the first decision is taken at zero.
    let mut retries = Attempt::first();
    let mut attempts = 0u32;
    loop {
        let started = Instant::now();
        let outcome = execute_case(source, case, index, max_assertions);
        let taken = started.elapsed();
        attempts = attempts.saturating_add(1);

        if outcome.failures.is_empty() {
            return (status_for(outcome), attempts);
        }
        let Some(delay) = options.retry.next_delay(retries) else {
            return (status_for(outcome), attempts);
        };
        retries = retries.advance(taken);
        if !delay.is_zero() {
            std::thread::sleep(delay);
        }
    }
}

fn status_for(outcome: BodyOutcome) -> TestStatus {
    if !outcome.failures.is_empty() {
        return TestStatus::Failed {
            failures: outcome.failures,
            unsupported: outcome.unsupported,
        };
    }
    if !outcome.unsupported.is_empty() {
        return TestStatus::Unsupported {
            assertions: outcome.unsupported,
        };
    }
    TestStatus::Passed
}

fn execute_case(
    source: &str,
    case: &TestCase,
    index: &LineIndex,
    max_assertions: usize,
) -> BodyOutcome {
    match case_body(source, case) {
        Some((start, body)) => evaluate_body(body, start, index, max_assertions),
        None => BodyOutcome::default(),
    }
}

/// The body of a case's callback, and where it starts in `source`.
///
/// Bounded to the registration call's own byte range, so a body that fails to
/// close cannot swallow every declaration after it — which is exactly what an
/// unbounded search for the next `{` used to do.
fn case_body<'a>(source: &'a str, case: &TestCase) -> Option<(usize, &'a str)> {
    let span_start = case.byte_offset;
    let span_end = case.end_byte_offset.min(source.len());
    if span_end <= span_start {
        return None;
    }
    let span = source.get(span_start..span_end)?;

    let after_callback_head = match span.find("=>") {
        Some(arrow) => span_start + arrow + "=>".len(),
        None => function_body_start(source, span, span_start)?,
    };

    let tail = source.get(after_callback_head..span_end)?;
    let (relative, first) = tail.char_indices().find(|(_, ch)| !ch.is_whitespace())?;
    let open = after_callback_head + relative;

    if first == '{' {
        let close = matching_delimiter(source, open, b'{', b'}')?;
        return Some((open + 1, source.get(open + 1..close)?));
    }

    // A concise arrow body: `it('x', () => expect(1).toBe(1))`. Everything up
    // to the call's closing parenthesis is the body.
    let end = span_end.saturating_sub(1).max(open);
    Some((open, source.get(open..end)?))
}

/// Where a `function () { ... }` callback's brace begins.
fn function_body_start(source: &str, span: &str, span_start: usize) -> Option<usize> {
    let keyword = span.find("function")?;
    let params_open = span_start + keyword + span[keyword..].find('(')?;
    let params_close = matching_delimiter(source, params_open, b'(', b')')?;
    Some(params_close + 1)
}

fn micros_since(started: Instant) -> u64 {
    duration_micros(started.elapsed())
}

fn duration_micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}
