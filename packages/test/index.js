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
 * Vitest's namespace, under its own name.
 *
 * A project moving to uf should not have to rewrite the parts of its tests
 * that were never about Vite: `vi.fn`, `vi.spyOn` and `vi.stubEnv` are a
 * vocabulary, and a second one for the same operations would cost every
 * migration and buy nothing.
 */
export { UnsupportedError, vi } from "./internal/vi.js";

export { RunawayTimersError } from "./internal/timers.js";

export { DEFAULT_TIMEOUT_MS, NAME_SEPARATOR } from "./internal/run.js";

export { equals, render } from "./internal/equality.js";
