// @flow
//
// Correct uses of `uft` timer controls from plain test functions.
//
// This file is expected to have no diagnostics. It exists because Flow's React
// hook rule reads a declaration named `useFakeTimers` or `useRealTimers` as a
// hook, even when the public API is a runner namespace and no component is
// involved.

import { uft } from "../../npm/test/index.js";

export function controlsTheClock(): void {
  uft.useFakeTimers();
  uft.advanceTimersByTime(100);
  uft.useRealTimers();
}
