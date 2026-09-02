//! Finding the test declarations a source file contains.
//!
//! Discovery answers what exists and where, never what it does: it records the
//! `describe`, `it` and `test` calls with their positions and `.only` / `.skip`
//! / `.todo` suffixes so a run, a filter or an editor listing can all work from
//! the same plan.
//!
//! Two guards bound the work a hostile or generated file can cause: a file
//! larger than [`MAX_SOURCE_BYTES`] is not scanned at all, and no more than
//! [`MAX_CASES_PER_FILE`] declarations are recorded. Both are unbounded
//! allocation defences — `code_byte_mask` allocates one byte per source byte,
//! and a generated file of a million `it(` calls would otherwise turn discovery
//! into a memory exhaustion primitive.

use compact_str::ToCompactString;
use uf_infra::LineIndex;

use crate::plan::{TestCase, TestKind, TestModifier, TestPlan, UnsupportedDeclaration};
use crate::scan::{
    CallShape, call_shape_at, code_byte_mask, extract_first_string_arg, matching_delimiter,
};

/// Largest source file discovery will scan, in bytes.
///
/// Matches `uf_rsc::scan::MAX_SOURCE_BYTES` so that a file the module graph
/// refuses is also a file the runner refuses.
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;

/// Largest number of declarations recorded from one file.
pub const MAX_CASES_PER_FILE: usize = 100_000;

/// The registration identifiers discovery recognises.
const REGISTRATIONS: [(&str, TestKind); 3] = [
    ("describe", TestKind::Describe),
    ("it", TestKind::Test),
    ("test", TestKind::Test),
];

/// Discover test declarations in a single source file.
///
/// Returns an empty plan for a source past [`MAX_SOURCE_BYTES`]; the runner
/// turns that into a named failure rather than a silent pass, see
/// [`crate::runner`].
pub fn discover_tests(file: &str, source: &str) -> TestPlan {
    if source.len() > MAX_SOURCE_BYTES {
        return TestPlan::default();
    }

    let line_index = LineIndex::new(source);
    let code_mask = code_byte_mask(source);
    let mut cases = Vec::new();
    let mut unsupported = Vec::new();

    for (call, kind) in REGISTRATIONS {
        let mut search_start = 0;
        while let Some(relative) = source[search_start..].find(call) {
            let offset = search_start + relative;
            search_start = offset + call.len();

            if !code_mask.get(offset).copied().unwrap_or(false) {
                continue;
            }
            let Some(shape) = call_shape_at(source, offset, call) else {
                continue;
            };

            let (modifier, args_from) = match shape {
                CallShape::Plain => (TestModifier::None, offset + call.len()),
                CallShape::Property { name, end } => match modifier_for(name) {
                    Some(modifier) => (modifier, end),
                    None => {
                        if unsupported.len() < MAX_CASES_PER_FILE {
                            let position = line_index.line_col(offset);
                            unsupported.push(UnsupportedDeclaration {
                                file: file.to_string(),
                                call: format_args!("{call}.{name}").to_compact_string(),
                                line: position.line,
                                column: position.column,
                            });
                        }
                        continue;
                    }
                },
            };

            let Some(name) = extract_first_string_arg(&source[args_from..]) else {
                continue;
            };
            if cases.len() >= MAX_CASES_PER_FILE {
                break;
            }

            let position = line_index.line_col(offset);
            cases.push(TestCase {
                file: file.to_string(),
                name,
                kind,
                modifier,
                line: position.line,
                column: position.column,
                byte_offset: offset,
                end_byte_offset: call_end(source, args_from),
            });
        }
    }

    cases.sort_by(|a, b| a.line.cmp(&b.line).then(a.column.cmp(&b.column)));
    unsupported.sort_by(|a, b| a.line.cmp(&b.line).then(a.column.cmp(&b.column)));
    TestPlan { cases, unsupported }
}

/// Map a member suffix onto a modifier, or reject it as unexpandable.
fn modifier_for(property: &str) -> Option<TestModifier> {
    match property {
        "only" => Some(TestModifier::Only),
        "skip" => Some(TestModifier::Skip),
        "todo" => Some(TestModifier::Todo),
        _ => None,
    }
}

/// The byte offset one past the closing parenthesis of the call whose argument
/// list starts at or after `from`.
///
/// An unbalanced call yields an empty range rather than the rest of the file, so
/// a truncated source cannot make one `describe` appear to enclose every
/// declaration after it.
fn call_end(source: &str, from: usize) -> usize {
    let Some(open) = source[from..].find('(').map(|open| from + open) else {
        return from;
    };
    match matching_delimiter(source, open, b'(', b')') {
        Some(close) => close + 1,
        None => from,
    }
}

/// Merge several discovery plans into deterministic file order.
pub fn merge_plans(plans: impl IntoIterator<Item = TestPlan>) -> TestPlan {
    let mut cases = Vec::new();
    let mut unsupported = Vec::new();
    for plan in plans {
        cases.extend(plan.cases);
        unsupported.extend(plan.unsupported);
    }
    sort_cases(&mut cases);
    unsupported.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
    });
    TestPlan { cases, unsupported }
}

/// The one ordering every report is emitted in: file, then position.
pub(crate) fn sort_cases(cases: &mut [TestCase]) {
    cases.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.column.cmp(&b.column))
    });
}
