// @flow
//
// Internal to `@uniflowed/test`: the `uft` namespace.
//
// The operations Vitest groups under `vi`, under uf's own name. The *shape* is
// what a migration needs — `fn`, `spyOn`, `stubEnv`, `useFakeTimers` doing what
// they do elsewhere — and borrowing another tool's brand for it would be
// claiming something uf has not earned.
//
// A namespace rather than loose named exports, because several of these names
// are generic enough to collide in a test file: `@uniflowed/testing` re-exports
// both this package and `@uniflowed/react-testing`, and both have a `waitFor`.
//
// `uft` rather than `uf`, and rather than `uf.test`. A bare `uf` is the command
// and the project, and a test file would be using the name for something much
// smaller than everything else in the toolchain answers to. `uf.test` reads as
// the `test` function this package also exports, which is `it` under another
// name — two very different things one dot apart. `uft` is three characters,
// belongs to nothing else, and is what a reader types a hundred times a file.
//
// `uft.mock` and the six names beside it intercept a module before it is
// imported, which is the loader's job rather than the runner's: by the time the
// runner sees an `import`, the module has been fetched, linked and evaluated.
// So the mechanism is `@uniflowed/host`'s (`module-mocks.js`) and the API is
// `./modules.js`'s, and this file only names them. A host that cannot provide
// synchronous module hooks gets an `UnsupportedError` that says which host it
// is and what to do instead — never a binding that silently does nothing.

import {
  importActual as importActualModule,
  importMock as importMockModule,
  mock as mockModule,
  resetModules as resetModulesNow,
  unmock as unmockModule,
} from "./modules.js";
import { clearAllMocks, fn, resetAllMocks, restoreAllMocks, spyOn } from "./spy.js";
import * as timers from "./timers.js";
import { UnsupportedError } from "./unsupported.js";

// Declared in `./unsupported.js` rather than here, because `./modules.js` needs
// it too and a class both halves of a pair reach for is a third module.
export { UnsupportedError } from "./unsupported.js";

/** Environment variables `stubEnv` replaced, and what they were. */
const stubbedEnv: Map<string, string | void> = new Map();

/** Globals `stubGlobal` replaced, and what they were. */
const stubbedGlobals: Map<string, { readonly owned: boolean, readonly value: mixed }> = new Map();

/**
 * Read the process environment, whichever host this is.
 *
 * Node and Bun expose `process` as a global; **Deno does not**, and has the
 * same object under `node:process`. `../worker.js` installs it on the global
 * before anything here runs, which is what keeps this one code path across the
 * three — and is why the `null` branch below is still reachable, for a host
 * that is neither.
 */
function environment(): { [string]: string } | null {
  const host = globalThis as $FlowFixMe;
  return host.process?.env ?? null;
}

/**
 * Replace an environment variable for the rest of the file.
 *
 * The file, and not the test: `process.env` belongs to the process, so a stub
 * stands until something puts it back. `./worker.js` does that between files,
 * beside the spy registry it clears for the same reason — a worker serves many
 * files, and a stub that outlived its file would be a test that passes because
 * of another one, in a suite where which files share a worker is decided by a
 * timings file. A case that wants a narrower scope calls `unstubAllEnvs` in an
 * `afterEach`, which is also what makes the scope visible to a reader.
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
 * Replace a global for the rest of the file.
 *
 * Undone by `unstubAllGlobals`, which `./worker.js` calls between files, for
 * the reason [`stubEnv`] above gives: `globalThis` outlives every file that
 * writes to it.
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
      throw last ?? new Error(`uft.waitFor: gave up after ${timeout}ms`);
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
      throw new Error("uft.waitUntil: the condition is not true yet");
    }
    return value;
  }, options);
}

/**
 * Hand a value back with its mock methods visible to the type checker.
 *
 * Purely a type-level convenience, exactly as in Vitest: at runtime it is the
 * identity function, and its whole job is letting a test write
 * `uft.mocked(client.send).mockReturnValue(…)` without a cast.
 */
export function mocked<T>(value: T): $FlowFixMe {
  return value;
}

/**
 * The `uft` namespace's type.
 *
 * Written out member by member rather than left as one `$FlowFixMe`, because
 * the module-mocking half of it is the half a type can genuinely check: a
 * factory that hands back the wrong shape for the module it is standing in for
 * is an error at the call site, and that only works if `uft` has a type at all.
 * The members that were already typed loosely keep the types they have —
 * `typeof` reads them from their definitions, so this list cannot drift from
 * them.
 */
export type Uft = {
  readonly fn: typeof fn,
  readonly spyOn: typeof spyOn,
  readonly mocked: typeof mocked,

  readonly clearAllMocks: typeof clearAllMocks,
  readonly resetAllMocks: typeof resetAllMocks,
  readonly restoreAllMocks: typeof restoreAllMocks,

  readonly stubEnv: typeof stubEnv,
  readonly unstubAllEnvs: typeof unstubAllEnvs,
  readonly stubGlobal: typeof stubGlobal,
  readonly unstubAllGlobals: typeof unstubAllGlobals,

  readonly waitFor: typeof waitFor,
  readonly waitUntil: typeof waitUntil,

  readonly useFakeTimers: typeof timers.useFakeTimers,
  readonly useRealTimers: typeof timers.useRealTimers,
  readonly isFakeTimers: typeof timers.isFaked,
  readonly advanceTimersByTime: typeof timers.advanceTimersByTime,
  readonly advanceTimersByTimeAsync: typeof timers.advanceTimersByTimeAsync,
  readonly advanceTimersToNextTimer: typeof timers.advanceTimersToNextTimer,
  readonly runAllTimers: typeof timers.runAllTimers,
  readonly runOnlyPendingTimers: typeof timers.runOnlyPendingTimers,
  readonly getTimerCount: typeof timers.getTimerCount,
  readonly setSystemTime: typeof timers.setSystemTime,
  readonly getMockedSystemTime: typeof timers.getMockedSystemTime,

  readonly mock: typeof mockModule,
  readonly doMock: typeof mockModule,
  readonly unmock: typeof unmockModule,
  readonly doUnmock: typeof unmockModule,
  readonly importActual: typeof importActualModule,
  readonly importMock: typeof importMockModule,
  readonly resetModules: typeof resetModulesNow,
};

/**
 * The `uft` namespace.
 *
 * A frozen object rather than a class: it is a namespace, nothing about it is
 * per-instance, and freezing it means a test cannot leave a monkey-patch behind
 * for the next one.
 */
export const uft: Uft = Object.freeze({
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

  // Module interception. `doMock` is `mock` and `doUnmock` is `unmock`, under
  // the names Vitest gives the un-hoisted forms: there is one form here,
  // because uf hoists neither, and a `doMock` that was a different function
  // would be claiming a difference that does not exist.
  mock: mockModule,
  doMock: mockModule,
  unmock: unmockModule,
  doUnmock: unmockModule,
  importActual: importActualModule,
  importMock: importMockModule,
  resetModules: resetModulesNow,
});
