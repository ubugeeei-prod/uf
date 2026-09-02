//! What a suite *contains*: the declarations, their nesting, and which of them
//! a run is allowed to execute.
//!
//! A plan is deliberately inert. It is produced by discovery, consumed by the
//! scheduler, the filter and the runner, and is the single value an editor
//! listing, `uf test --list` and a real run all agree on.
//!
//! Nesting is recorded as byte ranges rather than as a tree, because discovery
//! is a byte scan and never builds an AST. [`TestPlan::resolve`] turns those
//! ranges into parent links in one pass over the (already sorted) cases, so
//! neither nesting nor `.only` precedence costs a quadratic scan on a suite with
//! thousands of declarations.

use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// The enclosing `describe` chain of one case, outermost first.
pub type AncestorList = SmallVec<[usize; 4]>;

/// Kind of discovered test registration call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TestKind {
    /// A grouping call such as `describe`.
    Describe,
    /// A runnable test call such as `it` or `test`.
    Test,
}

/// The `.only` / `.skip` / `.todo` suffix written on a registration call.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TestModifier {
    /// No suffix: the declaration runs unless something else excludes it.
    #[default]
    None,
    /// `.only`: within its file, only marked declarations run.
    Only,
    /// `.skip`: never runs, and is reported as skipped.
    Skip,
    /// `.todo`: a declared-but-unwritten test, reported separately from a skip.
    Todo,
}

impl TestModifier {
    /// The suffix as written in source, for reporting.
    pub const fn suffix(self) -> &'static str {
        match self {
            Self::None => "",
            Self::Only => ".only",
            Self::Skip => ".skip",
            Self::Todo => ".todo",
        }
    }
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
    /// Suffix written on the registration call.
    #[serde(default)]
    pub modifier: TestModifier,
    /// One-based line number.
    pub line: usize,
    /// One-based column number.
    pub column: usize,
    /// Byte offset of the call identifier, skipped from serialized reports.
    #[serde(skip)]
    pub byte_offset: usize,
    /// Byte offset one past the closing parenthesis of the call.
    ///
    /// This is what makes nesting decidable without a parser: a case is inside a
    /// `describe` exactly when its range is inside the `describe`'s range.
    #[serde(skip)]
    pub end_byte_offset: usize,
}

impl TestCase {
    /// Whether `other` is lexically inside this declaration.
    pub fn contains(&self, other: &Self) -> bool {
        self.file == other.file
            && self.byte_offset < other.byte_offset
            && other.end_byte_offset <= self.end_byte_offset
    }
}

/// A registration form the native subset cannot execute, named so it is never
/// silently dropped.
///
/// `it.each`, `describe.concurrent` and friends are real declarations that this
/// runner has no way to expand. Skipping them quietly would report a green run
/// over tests that never ran, which is the one failure mode a test runner may
/// not have.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnsupportedDeclaration {
    /// Relative source file path.
    pub file: String,
    /// The call as written, e.g. `it.each`.
    pub call: CompactString,
    /// One-based line number.
    pub line: usize,
    /// One-based column number.
    pub column: usize,
}

/// Ordered native test discovery result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestPlan {
    /// Discovered test and describe calls, sorted by file then position.
    pub cases: Vec<TestCase>,
    /// Registration forms that were recognised but cannot be executed.
    #[serde(default)]
    pub unsupported: Vec<UnsupportedDeclaration>,
}

impl TestPlan {
    /// Count runnable test cases, excluding grouping calls.
    pub fn runnable_count(&self) -> usize {
        self.cases
            .iter()
            .filter(|case| case.kind == TestKind::Test)
            .count()
    }

    /// Whether the plan declares nothing at all.
    pub fn is_empty(&self) -> bool {
        self.cases.is_empty() && self.unsupported.is_empty()
    }

    /// Resolve nesting and `.only` precedence in one pass.
    pub fn resolve(&self) -> PlanResolution {
        PlanResolution::new(self)
    }
}

/// Why a declaration will not run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkipReason {
    /// The declaration, or an enclosing `describe`, is marked `.skip`.
    Explicit,
    /// The file contains a `.only`, and this declaration is not covered by one.
    NotOnly,
    /// A `--filter` or path filter excluded it.
    Filtered,
}

/// What a run will do with one declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", tag = "selection", content = "reason")]
pub enum Selection {
    /// Execute it.
    Run,
    /// Do not execute it, and report it as skipped.
    Skipped(SkipReason),
    /// Declared but not written; reported apart from a skip.
    Todo,
}

impl Selection {
    /// Whether the runner will execute this declaration.
    pub const fn is_run(self) -> bool {
        matches!(self, Self::Run)
    }
}

/// Nesting and selection for every case in a plan, computed once.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlanResolution {
    parents: Vec<Option<usize>>,
    selections: Vec<Selection>,
}

impl PlanResolution {
    fn new(plan: &TestPlan) -> Self {
        let parents = resolve_parents(&plan.cases);
        let selections = resolve_selections(&plan.cases, &parents);
        Self {
            parents,
            selections,
        }
    }

    /// The enclosing `describe` of `index`, if any.
    pub fn parent(&self, index: usize) -> Option<usize> {
        self.parents.get(index).copied().flatten()
    }

    /// The enclosing `describe` chain of `index`, outermost first.
    pub fn ancestors(&self, index: usize) -> AncestorList {
        let mut chain = AncestorList::new();
        let mut cursor = self.parent(index);
        while let Some(parent) = cursor {
            chain.push(parent);
            cursor = self.parent(parent);
        }
        chain.reverse();
        chain
    }

    /// What a run will do with `index`.
    pub fn selection(&self, index: usize) -> Selection {
        self.selections
            .get(index)
            .copied()
            .unwrap_or(Selection::Skipped(SkipReason::Filtered))
    }

    /// Replace the selection for `index`, used by filtering.
    pub fn set_selection(&mut self, index: usize, selection: Selection) {
        if let Some(slot) = self.selections.get_mut(index) {
            *slot = selection;
        }
    }

    /// How many declarations were resolved.
    pub fn len(&self) -> usize {
        self.selections.len()
    }

    /// Whether nothing was resolved.
    pub fn is_empty(&self) -> bool {
        self.selections.is_empty()
    }

    /// Append the fully qualified name of `index` — every enclosing `describe`
    /// name, then the case name — to `out`.
    ///
    /// Reuses the caller's buffer so that filtering a large suite does not
    /// allocate a name per case.
    pub fn push_full_name(&self, plan: &TestPlan, index: usize, out: &mut String) {
        for ancestor in self.ancestors(index) {
            if let Some(case) = plan.cases.get(ancestor) {
                out.push_str(&case.name);
                out.push_str(NAME_SEPARATOR);
            }
        }
        if let Some(case) = plan.cases.get(index) {
            out.push_str(&case.name);
        }
    }

    /// The fully qualified name of `index` as an owned string.
    pub fn full_name(&self, plan: &TestPlan, index: usize) -> String {
        let mut out = String::new();
        self.push_full_name(plan, index, &mut out);
        out
    }
}

/// What joins a `describe` name to the name nested inside it.
pub const NAME_SEPARATOR: &str = " > ";

/// Link every case to its enclosing `describe` in one pass.
///
/// Cases arrive sorted by file then byte offset, so an open `describe` can only
/// be closed by a case that starts after its end; a stack is therefore enough,
/// and no case is compared against more than the describes it actually sits in.
fn resolve_parents(cases: &[TestCase]) -> Vec<Option<usize>> {
    let mut parents = vec![None; cases.len()];
    let mut open: Vec<usize> = Vec::new();

    for (index, case) in cases.iter().enumerate() {
        while let Some(&top) = open.last() {
            if cases[top].contains(case) {
                break;
            }
            open.pop();
        }
        parents[index] = open.last().copied();
        if case.kind == TestKind::Describe {
            open.push(index);
        }
    }

    parents
}

/// Resolve `.only` / `.skip` / `.todo` precedence.
///
/// Precedence, highest first: `.todo` (it was never written), `.skip` (it was
/// explicitly turned off), then the file-level `.only` restriction. That order
/// is what every mainstream runner does, and it is the only one that lets a
/// developer disable a single case inside a file they are focusing.
fn resolve_selections(cases: &[TestCase], parents: &[Option<usize>]) -> Vec<Selection> {
    let mut inherited = vec![TestModifier::None; cases.len()];
    for (index, case) in cases.iter().enumerate() {
        let from_parent = parents[index]
            .map(|parent| inherited[parent])
            .unwrap_or(TestModifier::None);
        inherited[index] = combine(from_parent, case.modifier);
    }

    let mut only_files: Vec<&str> = cases
        .iter()
        .zip(inherited.iter())
        .filter(|(_, modifier)| **modifier == TestModifier::Only)
        .map(|(case, _)| case.file.as_str())
        .collect();
    only_files.sort_unstable();
    only_files.dedup();

    cases
        .iter()
        .zip(inherited.iter())
        .map(|(case, modifier)| match modifier {
            TestModifier::Todo => Selection::Todo,
            TestModifier::Skip => Selection::Skipped(SkipReason::Explicit),
            TestModifier::Only => Selection::Run,
            TestModifier::None => {
                if only_files.binary_search(&case.file.as_str()).is_ok() {
                    Selection::Skipped(SkipReason::NotOnly)
                } else {
                    Selection::Run
                }
            }
        })
        .collect()
}

/// Fold a parent's effective modifier with a child's own.
fn combine(parent: TestModifier, own: TestModifier) -> TestModifier {
    match (parent, own) {
        (_, TestModifier::Todo) | (TestModifier::Todo, _) => TestModifier::Todo,
        (_, TestModifier::Skip) | (TestModifier::Skip, _) => TestModifier::Skip,
        (TestModifier::Only, TestModifier::None) | (_, TestModifier::Only) => TestModifier::Only,
        (TestModifier::None, TestModifier::None) => TestModifier::None,
    }
}
