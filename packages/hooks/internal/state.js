// @flow
//
// State with a shape.
//
// `useStorage` is the one worth reading. Persisted state has three problems a
// `useState` plus a `useEffect` does not solve: the first render on a
// prerendered page has no storage to read, two components using the same key
// must agree, and another tab writing the key should be seen. All three are
// what `useSyncExternalStore` is for.

import { useCallback, useMemo, useState, useSyncExternalStore } from "@uniflowed/react";

import { useStableCallback } from "./lifecycle.js";

/** A boolean with the three things a caller ever does to one. */
export function useToggle(initial: boolean = false): {|
  +on: boolean,
  +toggle: () => void,
  +set: (value: boolean) => void,
|} {
  const [on, setOn] = useState(initial);
  const toggle = useCallback(() => setOn((value) => !value), []);
  return useMemo(() => ({ on, toggle, set: setOn }), [on, toggle]);
}

/** A number, optionally clamped. */
export function useCounter(
  initial: number = 0,
  bounds?: {| +min?: number, +max?: number |},
): {|
  +count: number,
  +increment: (by?: number) => void,
  +decrement: (by?: number) => void,
  +set: (value: number) => void,
  +reset: () => void,
|} {
  const min = bounds?.min;
  const max = bounds?.max;

  const clamp = useCallback(
    (value: number) => {
      const lower = min == null ? value : Math.max(min, value);
      return max == null ? lower : Math.min(max, lower);
    },
    [min, max],
  );

  const [count, setCount] = useState(() => clamp(initial));
  const move = useCallback(
    (delta: number) => setCount((value) => clamp(value + delta)),
    [clamp],
  );

  return useMemo(
    () => ({
      count,
      increment: (by?: number) => move(by ?? 1),
      decrement: (by?: number) => move(-(by ?? 1)),
      set: (value: number) => setCount(clamp(value)),
      reset: () => setCount(clamp(initial)),
    }),
    [count, move, clamp, initial],
  );
}

/**
 * Every subscriber of a storage key, so a write is seen by all of them.
 *
 * A `storage` event does not fire in the tab that made the change, so without
 * this two components sharing a key drift apart until one of them re-renders
 * for an unrelated reason.
 */
const listeners: Map<string, Set<() => void>> = new Map();

function announce(key: string): void {
  for (const listener of listeners.get(key) ?? []) {
    listener();
  }
}

function area(session: boolean): mixed {
  try {
    return session ? globalThis.sessionStorage : globalThis.localStorage;
  } catch {
    // A browser with site data blocked throws on the property itself.
    return null;
  }
}

/**
 * State kept in `localStorage`, or in `sessionStorage`.
 *
 * `initial` is what a prerender uses and what an unset or unreadable key falls
 * back to, so the first paint is stated rather than accidental. A value that
 * will not parse is treated as absent rather than thrown: storage is shared
 * with older versions of the same application, and refusing to start because
 * of a stale key would be worse than starting fresh.
 */
export function useStorage<T>(
  key: string,
  initial: T,
  options?: {| +session?: boolean |},
): [T, (value: T) => void] {
  const session = options?.session ?? false;

  const subscribe = useCallback(
    (notify: () => void) => {
      const set = listeners.get(key) ?? new Set();
      set.add(notify);
      listeners.set(key, set);
      const onStorage = (event: mixed) => {
        if ((event: any)?.key === key) {
          notify();
        }
      };
      const win: any = globalThis.window ?? globalThis;
      win.addEventListener?.("storage", onStorage);
      return () => {
        set.delete(notify);
        win.removeEventListener?.("storage", onStorage);
      };
    },
    [key],
  );

  const raw = useSyncExternalStore(
    subscribe,
    () => {
      const store: any = area(session);
      try {
        return store?.getItem(key) ?? null;
      } catch {
        return null;
      }
    },
    () => null,
  );

  const value = useMemo(() => {
    if (raw == null) {
      return initial;
    }
    try {
      return JSON.parse(raw);
    } catch {
      return initial;
    }
  }, [raw, initial]);

  const write = useStableCallback((next: T) => {
    const store: any = area(session);
    try {
      store?.setItem(key, JSON.stringify(next));
    } catch {
      // Full, or blocked. The announcement still happens so the components
      // sharing this key agree with each other for this session.
    }
    announce(key);
  });

  return [value, write];
}
