//! Lazy pipelines, deferred work and lock modes.
//!
//! Describes computation that has been staged but not run: the steps a pipeline
//! will apply, the phase a deferred task is released in, and the exclusivity a
//! lock is taken with.

use compact_str::{CompactString, ToCompactString};
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;

/// Lazy pipeline description.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LazyPipeline {
    /// Pipeline identifier.
    pub name: CompactString,
    /// Deferred steps.
    pub steps: SmallVec<[PipelineStep; 8]>,
}

/// Deferred work descriptor for `@uniflowed/std/defer`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeferredTask {
    /// Task identifier.
    pub id: CompactString,
    /// Scheduling phase.
    pub phase: DeferPhase,
}

impl DeferredTask {
    /// Create a deferred task descriptor.
    pub fn new(id: &str, phase: DeferPhase) -> Self {
        Self {
            id: id.to_compact_string(),
            phase,
        }
    }
}

/// Deferred scheduling phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeferPhase {
    /// Run after the current task boundary.
    Microtask,
    /// Run after I/O has yielded.
    Idle,
    /// Run after response streaming commits.
    PostResponse,
}

impl LazyPipeline {
    /// Create an empty lazy pipeline.
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_compact_string(),
            steps: SmallVec::new(),
        }
    }

    /// Add a deferred step to the pipeline.
    pub fn then(mut self, step: PipelineStep) -> Self {
        self.steps.push(step);
        self
    }
}

/// Deferred pipeline step.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PipelineStep {
    /// Map over items.
    Map,
    /// Filter items.
    Filter,
    /// Batch items before execution.
    Batch,
    /// Collect results.
    Collect,
}

/// Lock mode for mutex and reader-writer lock wrappers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LockMode {
    /// Exclusive lock.
    Exclusive,
    /// Shared read lock.
    Shared,
}
