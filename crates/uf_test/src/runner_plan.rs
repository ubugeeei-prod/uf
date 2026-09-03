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

/// Inline list of JavaScript hosts the native runner can target.
pub type TestHostList = SmallVec<[TestHost; 4]>;

/// Native test runner execution plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeTestRunnerPlan {
    /// Package specifier for the native test runtime surface.
    pub module: CompactString,
    /// Execution backend.
    pub runtime: TestRuntime,
    /// Capability JavaScript hosts the runner can drive without changing config.
    pub hosts: TestHostList,
    /// Scheduler used for test file execution.
    pub scheduler: TestScheduler,
    /// Performance target for the runner.
    pub performance_target: TestPerformanceTarget,
    /// Builtin import specifiers accepted by discovery and runtime bindings.
    pub imports: TestImportList,
    /// Whether React Testing Library-compatible helpers are implemented.
    ///
    /// They are: `@uniflowed/react-testing` installs a document on the host on
    /// first render, so a component test needs nothing configured.
    pub react_testing_library_native: bool,
    /// Whether discovery and transform use the official Flow parser line.
    pub official_flow_parser: bool,
}

impl Default for NativeTestRunnerPlan {
    fn default() -> Self {
        Self {
            module: CompactString::const_new("@uniflowed/test"),
            runtime: TestRuntime::CapabilityJsHost,
            hosts: smallvec::smallvec![TestHost::Node, TestHost::Deno, TestHost::Bun],
            scheduler: TestScheduler::NativeWorkStealing,
            performance_target: TestPerformanceTarget::FasterThanBun,
            imports: smallvec::smallvec![
                "@uniflowed/test".to_compact_string(),
                "@uniflowed/testing".to_compact_string(),
            ],
            react_testing_library_native: true,
            official_flow_parser: true,
        }
    }
}

impl NativeTestRunnerPlan {
    /// Return the default runtime-agnostic runner plan.
    pub fn runtime_agnostic() -> Self {
        Self::default()
    }

    /// Return the default runner plan.
    ///
    /// Kept for callers that predate the Capability JS Host split; the runner is
    /// still self-hosted in Rust, but JavaScript execution is delegated to a
    /// Capability JS Host.
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
    /// Vite+'s Rust task runner, invoked through `vp run`.
    ViteTask,
    /// Native Rust runner driving a detected JavaScript host.
    CapabilityJsHost,
    /// uf-owned self-hosted runtime.
    UfSelfHosted,
}

/// JavaScript runtimes that can host Flow test execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TestHost {
    /// Node.js.
    Node,
    /// Deno.
    Deno,
    /// Bun.
    Bun,
}

/// Native test scheduler.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TestScheduler {
    /// Vite Task's cache and dependency-aware scheduler.
    ViteTaskCache,
    /// Work-stealing scheduler for large test suites.
    NativeWorkStealing,
}

/// Performance target encoded in the runner contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TestPerformanceTarget {
    /// Follow Vite Task's native Rust runner performance profile.
    ViteTask,
    /// Beat Bun Test and Vitest for Flow-heavy suites.
    FasterThanBun,
}
