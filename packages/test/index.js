// @flow
//
// `@uniflowed/test`: the test API a project writes against.
//
// This is the implementation, not a declaration of one. `describe`, `it` and
// the hooks collect into a tree ([`./internal/registry.js`]); `expect` is a
// real matcher set ([`./internal/expect.js`]); `./worker.js` is the process
// `uf test` runs them in. `uf` owns discovery, scheduling across cores, the
// timings that order a run longest-first, watch invalidation and the terminal
// report — everything that is faster in Rust — and the host owns executing
// JavaScript, which is the one thing Rust cannot do.
//
// The whole surface is importable from here, so a test file has one import.

export type { Body as TestBody, Case, Modifier, Suite, TestOptions } from "./internal/registry.js";
export type { Outcome, Result, RunOptions } from "./internal/run.js";
export type { Site } from "./internal/frames.js";
export type { SpyCall, SpyResult } from "./internal/spy.js";
export type { Strictness } from "./internal/equality.js";

export {
  afterAll,
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  it,
  test,
} from "./internal/registry.js";

export { AssertionError, expect } from "./internal/expect.js";

export { fn, spyOn } from "./internal/spy.js";

/**
 * Spies, stubs, a controllable clock, and waiting — under uf's own name.
 *
 * The same operations Vitest groups under `vi`, so a suite being ported keeps
 * its shape; the name is uf's, because borrowing another tool's brand for it
 * would be claiming something uf has not earned.
 *
 * A namespace rather than loose named exports, because several of these names
 * are generic enough to collide: `@uniflowed/testing` re-exports both this
 * package and `@uniflowed/react-testing`, and both have a `waitFor`.
 */
export { UnsupportedError, uft } from "./internal/namespace.js";

export { RunawayTimersError } from "./internal/timers.js";

export { DEFAULT_TIMEOUT_MS, NAME_SEPARATOR } from "./internal/run.js";

export { equals, render } from "./internal/equality.js";
