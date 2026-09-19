// @flow
import type { BrowserPlan, VisualSnapshot, ScreenshotResult } from "@uniflowed/browser";
export type VrtEngine = "chromium-cdp";
export type DiffAlgorithm = "pixelmatch";
export type BaselinePolicy = "explicit-update-only";
export type VisualRegressionPlan = {|
  readonly engine: VrtEngine,
  readonly baselines: string,
  readonly threshold: number,
  readonly diff: DiffAlgorithm,
  readonly baselinePolicy: BaselinePolicy,
  readonly snapshots: $ReadOnlyArray<VisualSnapshot>,
|};
export function plan(snapshots?: $ReadOnlyArray<VisualSnapshot>): VisualRegressionPlan {
  return {
    engine: "chromium-cdp",
    baselines: "__uf_vrt__",
    threshold: 0,
    diff: "pixelmatch",
    baselinePolicy: "explicit-update-only",
    snapshots: snapshots ?? [],
  };
}
export function snapshot(storyId: string, viewport: string): VisualSnapshot {
  for (const name of [storyId, viewport]) {
    if (!/^[a-zA-Z0-9][a-zA-Z0-9._-]{0,49}$/.test(name) || name.includes(".."))
      throw new TypeError("VRT story and viewport names must be short file-safe identifiers");
  }
  return { storyId, viewport, baseline: `${storyId.length}-${storyId}-${viewport}` };
}
/** The application resolves each story and viewport to a ready, isolated page. */
export async function diff(
  value: VisualRegressionPlan,
  render: (snapshot: VisualSnapshot) => Promise<BrowserPlan>,
): Promise<$ReadOnlyArray<ScreenshotResult>> {
  const results = [];
  for (const entry of value.snapshots) {
    const page = await render(entry);
    try {
      results.push(
        await page.screenshot(entry.baseline, {
          threshold: value.threshold,
          baselines: value.baselines,
        }),
      );
    } finally {
      await page.close();
    }
  }
  return results;
}
export function baseline(path: string): VisualRegressionPlan {
  return { ...plan(), baselines: path };
}
