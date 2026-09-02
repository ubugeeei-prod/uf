//! The runner contract `uf inspect` reports and the toolchain builds against.
//!
//! This is the declared shape of the native test runner — which package it is
//! imported from, what executes it, how it is scheduled, and what it is aiming
//! at — kept as data so a command can print it and a test can assert it.

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Inline list of builtin test package specifiers.
pub type TestImportList = SmallVec<[CompactString; 4]>;

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
