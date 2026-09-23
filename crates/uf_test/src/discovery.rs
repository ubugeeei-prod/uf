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
    CallShape, call_shape_at, code_byte_mask, declares_locally, extract_first_string_arg,
    has_second_argument, matching_delimiter, value_imports,
};

/// Largest source file discovery will scan, in bytes.
///
/// Matches `uf_rsc::scan::MAX_SOURCE_BYTES` so that a file the module graph
/// refuses is also a file the runner refuses.
pub const MAX_SOURCE_BYTES: usize = 8 * 1024 * 1024;

/// Largest number of declarations recorded from one file.
pub const MAX_CASES_PER_FILE: usize = 100_000;

/// The registration identifiers discovery recognises.
const REGISTRATIONS: [(&str, TestKind); 4] = [
    ("describe", TestKind::Describe),
    ("it", TestKind::Test),
    ("test", TestKind::Test),
    ("bench", TestKind::Bench),
];

/// Modules whose `describe`, `it` and `test` belong to a different runner.
///
/// `test("…", …)` is the same eight characters whoever exports the binding,
/// and `uf test` can only execute the one that registers with
/// `@uniflowed/test`. A file that imports it from `node:test` declares tests
/// this runner will never run — and `node:test` is the case that hurts,
/// because it *accepts* the registration and keeps it for a runner that is not
/// there, so the file loads, reports nothing, and used to be counted as
/// runnable by `--list` and passed by the run (ubugeeei-prod/uf#482).
///
/// A named list rather than "anything that is not `@uniflowed/test`", and the
/// asymmetry is the whole of the design. A project may well re-export uf's
/// `it` from a local helper or an internal package — `import { it } from
/// "../support/setup.js"` is an ordinary thing to write — and calling that
/// unsupported would turn a suite that runs perfectly well red. Claiming "this
/// is another runner's" is only honest when the runner can be named. Whatever
/// this list misses is caught at run time instead, by a file that registered
/// nothing failing rather than passing; see [`crate::runner`].
const FOREIGN_RUNNERS: &[&str] = &[
    "@jest/globals",
    "ava",
    "bun:test",
    "jest",
    "mocha",
    "node:test",
    "tap",
    "tape",
    "uvu",
    "vitest",
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
    let imports = value_imports(source, &code_mask);
    let foreign = REGISTRATIONS.map(|(call, _)| foreign_runner(&imports, call));
    // A registration name the file declares for itself — `function describe(…)`
    // — is that function wherever the file calls it, not uf's.
    let local = REGISTRATIONS.map(|(call, _)| declares_locally(source, &code_mask, call));
    let mut cases = Vec::new();
    let mut unsupported = Vec::new();

    let bytes = source.as_bytes();
    let mut offset = 0;
    while offset < bytes.len() {
        if !code_mask.get(offset).copied().unwrap_or(false) {
            offset += 1;
            continue;
        }
        let Some(registration) = registration_at(bytes, offset) else {
            offset += 1;
            continue;
        };
        let (call, kind) = REGISTRATIONS[registration];
        offset += call.len();

        let Some(shape) = call_shape_at(source, offset - call.len(), call) else {
            continue;
        };

        let call_offset = offset - call.len();
        // Somebody else's `test`, named as such. Recorded rather than
        // dropped: the file is still handed to a worker — it may hold
        // uf's own declarations too — and `--list` has to stop counting
        // this one as a test the run will execute.
        if let Some(module) = foreign[registration] {
            if unsupported.len() < MAX_CASES_PER_FILE {
                let position = line_index.line_col(call_offset);
                unsupported.push(UnsupportedDeclaration {
                    file: file.to_string(),
                    call: call.to_compact_string(),
                    imported_from: Some(module.to_compact_string()),
                    line: position.line,
                    column: position.column,
                });
            }
            continue;
        }

        let (modifier, args_from) = match shape {
            CallShape::Plain => (TestModifier::None, call_offset + call.len()),
            CallShape::Property { name, end } => match modifier_for(name, kind) {
                Some(modifier) => (modifier, end),
                None => {
                    if unsupported.len() < MAX_CASES_PER_FILE {
                        let position = line_index.line_col(call_offset);
                        unsupported.push(UnsupportedDeclaration {
                            file: file.to_string(),
                            call: format_args!("{call}.{name}").to_compact_string(),
                            imported_from: None,
                            line: position.line,
                            column: position.column,
                        });
                    }
                    continue;
                }
            },
        };

        let Some(name) = extract_first_string_arg(&source[args_from..]) else {
            // Nor is a call to a function this file declares under the
            // same name: `packages/router/internal/hydration.js` calls its own
            // `describe(node, index)`.
            //
            // One argument that is not a name is not a registration at all:
            // every form that registers takes a body as well, and the ones
            // that do not (`.todo`) take a name. `describe(schema)` is a
            // helper that happens to be called `describe`, and recording it
            // made a validator module a test file with an unreadable
            // declaration in it.
            if local[registration] || !has_second_argument(source, &code_mask, args_from) {
                continue;
            }
            // A registration whose name is not a literal — `it(name, …)`
            // inside a loop, a template with a substitution. Discovery
            // cannot read it, and dropping it silently made the file look
            // like it held no tests at all: the run reported "0 passed" and
            // exited 0 for a file with tests in it. Recorded instead, so
            // the file is still handed to a worker and the report says what
            // could not be read.
            if unsupported.len() < MAX_CASES_PER_FILE {
                let position = line_index.line_col(call_offset);
                unsupported.push(UnsupportedDeclaration {
                    file: file.to_string(),
                    call: call.to_compact_string(),
                    imported_from: None,
                    line: position.line,
                    column: position.column,
                });
            }
            continue;
        };
        if cases.len() >= MAX_CASES_PER_FILE {
            continue;
        }

        let position = line_index.line_col(call_offset);
        cases.push(TestCase {
            file: file.to_string(),
            name,
            kind,
            modifier,
            line: position.line,
            column: position.column,
            byte_offset: call_offset,
            end_byte_offset: call_end(source, args_from),
        });
    }

    cases.sort_by(|a, b| a.line.cmp(&b.line).then(a.column.cmp(&b.column)));
    unsupported.sort_by(|a, b| a.line.cmp(&b.line).then(a.column.cmp(&b.column)));
    TestPlan { cases, unsupported }
}

fn registration_at(bytes: &[u8], offset: usize) -> Option<usize> {
    match bytes[offset] {
        b'd' if bytes[offset..].starts_with(b"describe") => Some(0),
        b'i' if bytes[offset..].starts_with(b"it") => Some(1),
        b't' if bytes[offset..].starts_with(b"test") => Some(2),
        b'b' if bytes[offset..].starts_with(b"bench") => Some(3),
        _ => None,
    }
}

/// The other runner `call` was imported from, when the file imports it from
/// one.
///
/// Reads the file's own imports rather than guessing from the name, because
/// the name is all the two have in common. A file with no import of `call` at
/// all — a project whose host installs the globals, or one being read out of
/// context — is not foreign: it is the shape discovery has always assumed.
fn foreign_runner<'a>(imports: &[crate::scan::ImportedBinding<'a>], call: &str) -> Option<&'a str> {
    imports
        .iter()
        .find(|binding| binding.local == call)
        .map(|binding| binding.module)
        .filter(|module| FOREIGN_RUNNERS.contains(module))
}

/// Map a member suffix onto a modifier, or reject it as unexpandable for the
/// receiver that owns it.
fn modifier_for(property: &str, kind: TestKind) -> Option<TestModifier> {
    match (property, kind) {
        ("only", _) => Some(TestModifier::Only),
        ("skip", _) => Some(TestModifier::Skip),
        ("skipBecause", TestKind::Test) => Some(TestModifier::Skip),
        ("todo", _) => Some(TestModifier::Todo),
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
