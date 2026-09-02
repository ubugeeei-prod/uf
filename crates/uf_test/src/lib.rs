#![deny(missing_docs)]
//! Native test discovery and runner planning for `uf test` and `@uniflowed/test`.

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use thiserror::Error;

mod discovery;
mod execution;
mod scan;

pub use discovery::{discover_tests, merge_plans};
pub use execution::run_tests;

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

#[cfg(test)]
mod tests;
