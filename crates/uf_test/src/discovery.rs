//! Finding the test declarations a source file contains.
//!
//! Discovery answers what exists and where, never what it does: it records the
//! `describe`, `it` and `test` calls with their positions so a run, a filter or
//! an editor listing can all work from the same plan.

use uf_infra::LineIndex;

use crate::scan::{code_byte_mask, extract_first_string_arg, is_call_at};
use crate::{TestCase, TestKind, TestPlan};

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
