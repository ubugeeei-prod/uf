// @flow
//
// `@uniflowed/test`: the self-hosted runner entry point. The assertion API is
// re-exported by name from `./testing.js` so a bundler can drop the halves an
// application does not use.

import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/test";

export type { Expectation, RenderResult, Screen, TestBody } from "@uniflowed/testing";
export {
  afterEach,
  beforeEach,
  describe,
  expect,
  fireEvent,
  it,
  render,
  screen,
  test,
  userEvent,
  waitFor,
} from "@uniflowed/testing";

export type TestRuntime = "uf-self-hosted";
export type TestScheduler = "native-work-stealing";
export type TestPerformanceTarget = "faster-than-bun";

export type NativeTestRunnerPlan = {
  +module: "@uniflowed/test",
  +runtime: TestRuntime,
  +scheduler: TestScheduler,
  +performanceTarget: TestPerformanceTarget,
  +imports: $ReadOnlyArray<"@uniflowed/test" | "@uniflowed/testing" | "inflow">,
  +reactTestingLibraryNative: true,
  +officialFlowParser: true,
};

export function plan(): NativeTestRunnerPlan {
  return nativeRuntimeRequired(MODULE, "plan");
}
