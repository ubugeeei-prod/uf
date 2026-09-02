// @flow
//
// `@uniflowed/vrt`.

import type { VisualSnapshot } from "@uniflowed/browser";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/vrt";

export type VrtEngine = "uf-native-playwright-compatible";
export type DiffAlgorithm = "pixelmatch-compatible";
export type BaselinePolicy = "fail-on-missing" | "explicit-update-only";

export type VisualRegressionPlan = {
  +engine: VrtEngine,
  +baselines: string,
  +threshold: number,
  +diff: DiffAlgorithm,
  +baselinePolicy: BaselinePolicy,
  +snapshots: $ReadOnlyArray<VisualSnapshot>,
};

export function plan(
  snapshots?: $ReadOnlyArray<VisualSnapshot>,
): VisualRegressionPlan {
  return nativeRuntimeRequired(MODULE, "plan");
}

export function snapshot(storyId: string, viewport: string): VisualSnapshot {
  return nativeRuntimeRequired(MODULE, "snapshot");
}

export function diff(plan: VisualRegressionPlan): VisualRegressionPlan {
  return nativeRuntimeRequired(MODULE, "diff");
}

export function baseline(path: string): VisualRegressionPlan {
  return nativeRuntimeRequired(MODULE, "baseline");
}
