#![deny(missing_docs)]
//! Native visual regression contracts for `@uniflowed/vrt`.

use compact_str::CompactString;
use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use uf_browser::VisualSnapshot;

/// Inline VRT snapshot list.
pub type VrtSnapshotList = SmallVec<[VisualSnapshot; 16]>;

/// Visual regression plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VisualRegressionPlan {
    /// Browser-driven VRT engine.
    pub engine: VrtEngine,
    /// Baseline directory.
    pub baselines: CompactString,
    /// Pixel threshold. Zero means exact match.
    pub threshold: u16,
    /// Diff algorithm.
    pub diff: DiffAlgorithm,
    /// Baseline update policy.
    pub baseline_policy: BaselinePolicy,
    /// Snapshots included in this plan.
    pub snapshots: VrtSnapshotList,
}

impl Default for VisualRegressionPlan {
    fn default() -> Self {
        Self {
            engine: VrtEngine::UfNativePlaywrightCompatible,
            baselines: CompactString::const_new("__uf_vrt__"),
            threshold: 0,
            diff: DiffAlgorithm::PixelmatchCompatible,
            baseline_policy: BaselinePolicy::FailOnMissing,
            snapshots: SmallVec::new(),
        }
    }
}

impl VisualRegressionPlan {
    /// Create a strict VRT plan from snapshots.
    pub fn from_snapshots(snapshots: impl IntoIterator<Item = VisualSnapshot>) -> Self {
        let mut plan = Self::default();
        plan.snapshots.extend(snapshots);
        plan
    }

    /// Return whether this plan requires exact pixel matches.
    pub fn is_strict(&self) -> bool {
        self.threshold == 0 && self.baseline_policy == BaselinePolicy::FailOnMissing
    }
}

/// VRT engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VrtEngine {
    /// uf native runner with Playwright-compatible browser contracts.
    UfNativePlaywrightCompatible,
}

/// Diff algorithm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiffAlgorithm {
    /// Pixelmatch-compatible image diffing.
    PixelmatchCompatible,
}

/// Baseline policy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BaselinePolicy {
    /// Missing baselines fail the run.
    FailOnMissing,
    /// Missing baselines can be written by an explicit update command.
    ExplicitUpdateOnly,
}

/// Return the default VRT plan.
pub fn plan() -> VisualRegressionPlan {
    VisualRegressionPlan::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use uf_browser::Viewport;

    #[test]
    fn defaults_to_strict_native_vrt() {
        let plan = plan();

        assert_eq!(plan.baselines, "__uf_vrt__");
        assert!(plan.is_strict());
    }

    #[test]
    fn builds_snapshot_plan_from_browser_snapshots() {
        let snapshots = [VisualSnapshot::new("dialog-open", &Viewport::desktop())];
        let plan = VisualRegressionPlan::from_snapshots(snapshots);

        assert_eq!(plan.snapshots.len(), 1);
        assert_eq!(plan.snapshots[0].baseline, "dialog-open.desktop.png");
    }
}
