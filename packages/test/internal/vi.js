// @flow
//
// Internal to `@uniflowed/test`: the `vi` namespace.
//
// Vitest's name, deliberately. A project moving to uf should not have to
// rewrite the parts of its tests that were never about Vite — `vi.fn`,
// `vi.spyOn`, `vi.stubEnv` are a vocabulary, and inventing a second one for the
// same operations would buy nothing and cost every migration.
//
// What is *not* here is as deliberate. `vi.mock` intercepts a module before it
// is imported, which needs the loader rather than the runner, and uf's loader is
// `@uniflowed/host` — so it is a real piece of work rather than a wrapper, and
// it is not pretended at here. A missing binding throws with what it would take;
// a binding that silently did nothing would be worse than not having it.

import { clearAllMocks, fn, resetAllMocks, restoreAllMocks, spyOn } from "./spy.js";
import * as timers from "./timers.js";

/** Environment variables `stubEnv` replaced, and what they were. */
const stubbedEnv: Map<string, string | void> = new Map();

/** Globals `stubGlobal` replaced, and what they were. */
const stubbedGlobals: Map<string, { readonly owned: boolean, readonly value: mixed }> = new Map();

/**
 * Raised for a `vi` binding uf has not implemented.
 *
 * Names what the binding needs rather than only that it is missing: `vi.mock`
 * is absent because module interception belongs to the loader, and a reader who
 * knows that can decide whether to wait or to restructure the test.
 */
export class UnsupportedError extends Error {
  /** The binding that was called. */
  binding: string;

  constructor(binding: string, reason: string) {
    super(`vi.${binding} is not implemented yet: ${reason}`);
    this.name = "UnsupportedError";
    this.binding = binding;
  }
}

/**
 * Read the process environment, whichever host this is.
 *
 * Node, Deno and Bun all expose `process.env`; Deno also has `Deno.env`, and
 * reaching for `process` first keeps one code path across the three.
 */
function environment(): { [string]: string } | null {
  const host = globalThis as $FlowFixMe;
  return host.process?.env ?? null;
}

/**
 * Replace an environment variable for the rest of the test.
 *
 * Undone by `unstubAllEnvs`, which the runner calls between files — a stub that
 * outlived its test would be a test that passes alone and fails in a suite.
 */
export function stubEnv(name: string, value: string | void): void {
  const env = environment();
  if (env == null) {
    throw new UnsupportedError("stubEnv", "this host exposes no process environment");
  }
  if (!stubbedEnv.has(name)) {
    stubbedEnv.set(name, Object.hasOwn(env, name) ? env[name] : undefined);
  }
  if (value === undefined) {
    delete env[name];
  } else {
    env[name] = value;
  }
}

/** Put every environment variable `stubEnv` replaced back. */
export function unstubAllEnvs(): void {
  const env = environment();
  if (env == null) {
    stubbedEnv.clear();
    return;
  }
  for (const [name, previous] of stubbedEnv) {
    if (previous === undefined) {
      delete env[name];
    } else {
      env[name] = previous;
    }
  }
  stubbedEnv.clear();
}

/**
 * Replace a global for the rest of the test.
 *
 * Whether the global was the object's own property is recorded, because putting
 * back an inherited one by assignment would leave a copy that shadows whatever
 * it was inherited from.
 */
export function stubGlobal(name: string, value: mixed): void {
  const host = globalThis as $FlowFixMe;
  if (!stubbedGlobals.has(name)) {
    stubbedGlobals.set(name, {
      owned: Object.hasOwn(host, name),
      value: host[name],
    });
  }
  host[name] = value;
}

/** Put every global `stubGlobal` replaced back. */
export function unstubAllGlobals(): void {
  const host = globalThis as $FlowFixMe;
  for (const [name, previous] of stubbedGlobals) {
    if (previous.owned) {
      host[name] = previous.value;
    } else {
      delete host[name];
    }
  }
  stubbedGlobals.clear();
}

/** How often `waitFor` re-runs its body while it is failing. */
const WAIT_INTERVAL_MS = 20;

/** How long `waitFor` keeps trying before giving up. */
const WAIT_TIMEOUT_MS = 1_000;

/**
 * Run `body` until it stops throwing, or the timeout passes.
 *
 * The last failure is what is raised, not a timeout — "expected 2, got 1" says
 * what went wrong, and "timed out" says only that something did.
 */
export async function waitFor<T>(
  body: () => T | Promise<T>,
  options?: { readonly timeout?: number, readonly interval?: number },
): Promise<T> {
  const timeout = options?.timeout ?? WAIT_TIMEOUT_MS;
  const interval = options?.interval ?? WAIT_INTERVAL_MS;
  const deadline = Date.now() + timeout;
  let last: mixed = null;

  for (;;) {
    try {
      return await body();
    } catch (thrown) {
      last = thrown;
    }
    if (Date.now() >= deadline) {
      throw last ?? new Error(`vi.waitFor: gave up after ${timeout}ms`);
    }
    await new Promise((resolve) => setTimeout(resolve, interval));
  }
}

/** Run `body` until it returns something truthy, or the timeout passes. */
export async function waitUntil(
  body: () => mixed | Promise<mixed>,
  options?: { readonly timeout?: number, readonly interval?: number },
): Promise<mixed> {
  return waitFor(async () => {
    const value = await body();
    if (value == null || value === false) {
      throw new Error("vi.waitUntil: the condition is not true yet");
    }
    return value;
  }, options);
}

/**
 * Hand a value back with its mock methods visible to the type checker.
 *
 * Purely a type-level convenience, exactly as in Vitest: at runtime it is the
 * identity function, and its whole job is letting a test write
 * `vi.mocked(client.send).mockReturnValue(…)` without a cast.
 */
export function mocked<T>(value: T): $FlowFixMe {
  return value;
}

/** Not implemented, and specific about what it would take. */
function unsupported(binding: string, reason: string): () => empty {
  return () => {
    throw new UnsupportedError(binding, reason);
  };
}

/** The reason every module-interception binding is absent. */
const NEEDS_LOADER =
  "intercepting a module before it is imported belongs to the loader " +
  "(`@uniflowed/host`), not to the runner, and uf has not wired it yet";

/**
 * The `vi` namespace.
 *
 * A frozen object rather than a class: it is a namespace, nothing about it is
 * per-instance, and freezing it means a test cannot leave a monkey-patch behind
 * for the next one.
 */
export const vi: $FlowFixMe = Object.freeze({
  fn,
  spyOn,
  mocked,

  clearAllMocks,
  resetAllMocks,
  restoreAllMocks,

  stubEnv,
  unstubAllEnvs,
  stubGlobal,
  unstubAllGlobals,

  waitFor,
  waitUntil,

  // The clock a test controls. A test about "after five minutes the session
  // expires" should not take five minutes.
  useFakeTimers: timers.useFakeTimers,
  useRealTimers: timers.useRealTimers,
  isFakeTimers: timers.isFaked,
  advanceTimersByTime: timers.advanceTimersByTime,
  advanceTimersByTimeAsync: timers.advanceTimersByTimeAsync,
  advanceTimersToNextTimer: timers.advanceTimersToNextTimer,
  runAllTimers: timers.runAllTimers,
  runOnlyPendingTimers: timers.runOnlyPendingTimers,
  getTimerCount: timers.getTimerCount,
  setSystemTime: timers.setSystemTime,
  getMockedSystemTime: timers.getMockedSystemTime,

  // Module interception. Absent rather than faked; see `NEEDS_LOADER`.
  mock: unsupported("mock", NEEDS_LOADER),
  doMock: unsupported("doMock", NEEDS_LOADER),
  unmock: unsupported("unmock", NEEDS_LOADER),
  doUnmock: unsupported("doUnmock", NEEDS_LOADER),
  importActual: unsupported("importActual", NEEDS_LOADER),
  importMock: unsupported("importMock", NEEDS_LOADER),
  resetModules: unsupported("resetModules", NEEDS_LOADER),
});
