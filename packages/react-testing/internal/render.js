// @flow
//
// Mounting a component into a real document.
//
// Everything that changes the tree goes through React's `act`, which is what
// makes a test able to assert immediately afterwards: `act` runs the render,
// the effects, and the microtasks React queued, then returns. Without it a
// test asserts against the DOM as it was before React got to it, and the fix
// people reach for is a sleep.

import { createRequire } from "node:module";

// A type-only import: `React.Node` is the only thing this module needs from
// React itself, and importing the namespace as a value left an unused binding
// in the bundle.
import type * as React from "@uniflowed/react";
import { act } from "@uniflowed/react";

import { installActEnvironment, installDom, setActEnvironment } from "./dom.js";

/** What `render` hands back. */
export type RenderResult = {|
  /** The element the tree was mounted into. */
  readonly container: Element,
  /** The document body, which is where a portal ends up. */
  readonly baseElement: Element,
  /** Render different elements into the same container. */
  readonly rerender: (ui: React.Node) => void,
  /** Take the tree down and remove the container. */
  readonly unmount: () => void,
  /** The container's markup, for a failure message. */
  readonly asFragment: () => string,
|};

type Mounted = {|
  container: Element,
  root: { render(node: React.Node): void, unmount(): void },
|};

const mounted: Array<Mounted> = [];

/**
 * Render `ui` into a fresh container in the document body.
 *
 * Anything still mounted from an earlier `render` is taken down first. `screen`
 * queries the whole document, so a tree left over from the previous test would
 * make "there is exactly one Save button" false for reasons that have nothing
 * to do with the test being read.
 */
export function render(ui: React.Node, options?: {| readonly container?: Element |}): RenderResult {
  installDom();
  cleanup();

  const { createRoot } = requireClient();
  const container = options?.container ?? createContainer();
  const root = createRoot(container);
  mounted.push({ container, root });

  act(() => {
    root.render(ui);
  });

  return {
    container,
    baseElement: globalThis.document.body as any,
    rerender: (next: React.Node) => {
      act(() => {
        root.render(next);
      });
    },
    unmount: () => {
      act(() => {
        root.unmount();
      });
      container.remove();
      const index = mounted.findIndex((entry) => entry.container === container);
      if (index >= 0) {
        mounted.splice(index, 1);
      }
    },
    asFragment: () => container.innerHTML,
  };
}

/** Unmount everything this module has mounted. */
export function cleanup(): void {
  while (mounted.length > 0) {
    const entry = mounted.pop();
    if (entry == null) {
      break;
    }
    act(() => {
      entry.root.unmount();
    });
    entry.container.remove();
  }
}

function createContainer(): Element {
  const container = globalThis.document.createElement("div");
  globalThis.document.body.appendChild(container);
  return container;
}

/**
 * `react-dom/client`, loaded only once a test renders.
 *
 * It reads `document` while it is being evaluated, so it cannot be a static
 * import of this module: ESM evaluates every import before the module body,
 * and the DOM has to exist first. `render` is synchronous — a test asserts on
 * the line after it — so this cannot be `await import(…)` either.
 *
 * That leaves a synchronous require, through `node:module`. All three hosts
 * uf supports provide it, and React ships a CommonJS build for exactly this
 * kind of caller.
 */
let client: mixed = null;
function requireClient(): { createRoot: (Element) => any } {
  if (client == null) {
    const load = createRequire(import.meta.url);
    client = load("react-dom/client") as any;
  }
  return client as any;
}

/**
 * Run `body`, letting React flush everything it queues.
 *
 * Exported because a test that changes state outside an event — a timer
 * firing, a promise settling — has to tell React that the change happened,
 * and this is how.
 */
export function actively<T>(body: () => T): T {
  // A test that acts without having rendered — a timer firing in a hook test
  // — reaches `act` without going through `render`, and `act` still has to
  // know it is being called by a test.
  installActEnvironment();
  let result: mixed;
  const scope: mixed = act(() => {
    result = body();
    // Handed back so React keeps the scope open until an async body settles.
    // Without this the scope closed on the first tick and every update the
    // body was still waiting for landed outside it, which React reports as
    // "an update was not wrapped in act(...)".
    return result;
  });
  if (isThenable(result) && isThenable(scope)) {
    // `Promise.resolve`, not `scope.then(…)`: `act` hands back a bare thenable
    // — an object with a `then` and nothing else — whose `then` returns
    // `undefined` rather than a promise. Chaining off it directly produced an
    // `undefined` that `await` resolved immediately, so the caller carried on
    // while the scope was still open: the body's timers had not fired, and
    // every later `act` nested inside the scope that was never closed and
    // flushed nothing. `render` after one of those returned an empty
    // container.
    return Promise.resolve(scope).then(() => result) as any;
  }
  return result as any;
}

/** Whether `value` is something to await. */
function isThenable(value: mixed): boolean {
  return value != null && typeof value === "object" && typeof (value as any).then === "function";
}

/**
 * Wait until `body` stops throwing, or give up.
 *
 * Polling rather than observing mutations, because what a test waits for is
 * usually not a DOM change at all — it is a promise resolving, a fetch
 * settling, a timer firing — and a mutation observer sees none of those.
 */
export async function waitFor<T>(
  body: () => T | Promise<T>,
  options?: {| readonly timeout?: number, readonly interval?: number |},
): Promise<T> {
  installActEnvironment();
  const timeout = options?.timeout ?? 1000;
  const interval = options?.interval ?? 20;

  // React is told this is not an act environment for as long as the wait
  // lasts, and told again afterwards.
  //
  // The update a test waits for arrives between two polls, and React reports
  // it as "an update to X inside a test was not wrapped in act(...)" —
  // correctly, since nothing was there to flush it. The fix cannot be to put
  // the polling loop inside an `act` scope: `act` holds updates back until
  // the scope closes, so the loop would poll a tree that cannot change and
  // every `waitFor` would run to its timeout.
  //
  // So the scope is stood down instead. The warning exists to catch an update
  // a test did not know it was causing; a test that wrote `waitFor` knows.
  // Counted rather than saved and restored, because waits nest: every
  // `findBy…` is a `waitFor`, and a test may put one inside another. The
  // outermost wait stands the environment down and the outermost restores it.
  waits += 1;
  if (waits === 1) {
    setActEnvironment(false);
  }
  try {
    return await poll(body, timeout, interval);
  } finally {
    waits -= 1;
    if (waits === 0) {
      setActEnvironment(true);
    }
  }
}

/** How many waits are in progress. */
let waits = 0;

/** Call `body` until it stops throwing, or give up after `timeout`. */
async function poll<T>(body: () => T | Promise<T>, timeout: number, interval: number): Promise<T> {
  const deadline = Date.now() + timeout;
  let lastError: mixed = null;

  while (true) {
    try {
      return await body();
    } catch (error) {
      lastError = error;
    }
    if (Date.now() >= deadline) {
      throw lastError instanceof Error
        ? lastError
        : new Error(`waitFor timed out after ${timeout}ms: ${String(lastError)}`);
    }
    await new Promise((resolve) => setTimeout(resolve, interval));
  }
}
