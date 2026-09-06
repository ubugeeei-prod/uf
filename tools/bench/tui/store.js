// @flow
//
// The state both applications read, through the hook both Reacts have.
//
// A benchmark that drove one library through a key press and the other through
// `rerender()` would be comparing two different paths and calling the
// difference a renderer's. `useSyncExternalStore` exists in both Reacts, does
// the same thing in both, and lets one line — `store.set(next)` — be the
// "state update" that both stopwatches start on.

import type { Frame } from "./workload.js";

/** A state container with React's subscription shape. */
export type Store = {
  set(next: Frame): void,
  subscribe(listener: () => void): () => void,
  snapshot(): Frame,
};

/** A store holding `initial`. */
export function createStore(initial: Frame): Store {
  let current = initial;
  const listeners = new Set<() => void>();
  return {
    set(next: Frame) {
      current = next;
      for (const listener of listeners) {
        listener();
      }
    },
    subscribe(listener: () => void) {
      listeners.add(listener);
      return () => {
        listeners.delete(listener);
      };
    },
    snapshot() {
      return current;
    },
  };
}
