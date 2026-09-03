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

import * as React from "@uniflowed/react";
import { act } from "@uniflowed/react";

import { installDom } from "./dom.js";

/** What `render` hands back. */
export type RenderResult = {|
  /** The element the tree was mounted into. */
  +container: Element,
  /** The document body, which is where a portal ends up. */
  +baseElement: Element,
  /** Render different elements into the same container. */
  +rerender: (ui: React.Node) => void,
  /** Take the tree down and remove the container. */
  +unmount: () => void,
  /** The container's markup, for a failure message. */
  +asFragment: () => string,
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
export function render(ui: React.Node, options?: {| +container?: Element |}): RenderResult {
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
    baseElement: (globalThis.document.body: any),
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
    client = (load("react-dom/client"): any);
  }
  return (client: any);
}

/**
 * Run `body`, letting React flush everything it queues.
 *
 * Exported because a test that changes state outside an event — a timer
 * firing, a promise settling — has to tell React that the change happened,
 * and this is how.
 */
export function actively<T>(body: () => T): T {
  let result: T;
  act(() => {
    result = body();
  });
  return (result: any);
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
  options?: {| +timeout?: number, +interval?: number |},
): Promise<T> {
  const timeout = options?.timeout ?? 1000;
  const interval = options?.interval ?? 20;
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
