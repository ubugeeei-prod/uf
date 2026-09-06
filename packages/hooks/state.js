// @flow
//
// `@uniflowed/hooks/state`: state with a shape.
//
// `useStorage` is the one worth reading. Persisted state has three problems a
// `useState` plus a `useEffect` does not solve: the first render on a
// prerendered page has no storage to read, two components using the same key
// must agree, and another tab writing the key should be seen. All three are
// what `useSyncExternalStore` is for.
//
// # What belongs in this module
//
// A `useState` a component would otherwise write out by hand, returned as the
// operations that make sense on it rather than as a setter: a boolean with
// `toggle`, a number with `increment` and a clamp, a list with the six edits
// anyone ever makes to one, a set with `toggle`, a position in a cycle, a value
// that can be undone, a value that survives a reload. The test is that the hook
// owns the value and hands back a small API over it.
//
// Every operation here produces a new value rather than editing the one it was
// given, and an operation that would change nothing returns the *same* value —
// removing an index that is not there, adding a member that is already in the
// set. That is not thrift: `useState` compares with `Object.is`, so returning
// the old value is what makes a no-op cost no render.
//
// Not here: shared application state. An atom two routes both read is
// `@uniflowed/state`'s, and a value derived from a server response is
// `@uniflowed/query`'s. A value that arrives from outside the page — another
// tab, the system clipboard — is `channels.js`, one file over. Everything in
// this file is local to one component; `useStorage` reaches outside only to
// persist, and only under a key the caller named.

import { useCallback, useMemo, useState, useSyncExternalStore } from "@uniflowed/react";

import { browserWindow } from "./browser.js";
import { useStableCallback } from "./lifecycle.js";

/** A boolean and the three things a caller ever does to one. */
export type UseToggleReturn = {|
  readonly on: boolean,
  readonly toggle: () => void,
  readonly set: (value: boolean) => void,
|};

/** A boolean with the three things a caller ever does to one. */
export hook useToggle(initial: boolean = false): UseToggleReturn {
  const [on, setOn] = useState(initial);
  const toggle = useCallback(() => setOn((value) => !value), []);
  return useMemo(() => ({ on, toggle, set: setOn }), [on, toggle]);
}

/** A number and the operations that suit one. */
export type UseCounterReturn = {|
  readonly count: number,
  readonly increment: (by?: number) => void,
  readonly decrement: (by?: number) => void,
  readonly set: (value: number) => void,
  readonly reset: () => void,
|};

/** A number, optionally clamped. */
export hook useCounter(
  initial: number = 0,
  bounds?: {| readonly min?: number, readonly max?: number |},
): UseCounterReturn {
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
  const move = useCallback((delta: number) => setCount((value) => clamp(value + delta)), [clamp]);

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

/** A list and the edits anyone makes to one. */
export type UseListReturn<T> = {|
  readonly items: $ReadOnlyArray<T>,
  readonly set: (items: $ReadOnlyArray<T>) => void,
  readonly push: (item: T) => void,
  readonly insertAt: (index: number, item: T) => void,
  readonly replaceAt: (index: number, item: T) => void,
  readonly removeAt: (index: number) => void,
  readonly move: (from: number, to: number) => void,
  readonly clear: () => void,
|};

/**
 * A list, with the six edits anyone ever makes to one.
 *
 * An index outside the list is not an error and not a throw: it leaves the
 * list alone and returns the same array, so a row removed twice by a
 * double-clicked button is removed once. `@uniflowed/form`'s `useFieldArray`
 * is the version of this for form rows, and knows about keys, errors and
 * dirty flags; this one is for a list that is only a list.
 */
export hook useList<T>(initial: $ReadOnlyArray<T> = []): UseListReturn<T> {
  const [items, setItems] = useState<$ReadOnlyArray<T>>(initial);

  const set = useStableCallback((next: $ReadOnlyArray<T>) => setItems(next));

  const push = useStableCallback((item: T) => setItems((current) => [...current, item]));

  const insertAt = useStableCallback((index: number, item: T) =>
    setItems((current) =>
      index < 0 || index > current.length
        ? current
        : [...current.slice(0, index), item, ...current.slice(index)],
    ),
  );

  const replaceAt = useStableCallback((index: number, item: T) =>
    setItems((current) =>
      index < 0 || index >= current.length
        ? current
        : current.map((existing, at) => (at === index ? item : existing)),
    ),
  );

  const removeAt = useStableCallback((index: number) =>
    setItems((current) =>
      index < 0 || index >= current.length ? current : current.filter((_, at) => at !== index),
    ),
  );

  const move = useStableCallback((from: number, to: number) =>
    setItems((current) => {
      if (from < 0 || from >= current.length || to < 0 || to >= current.length || from === to) {
        return current;
      }
      const next = [...current];
      const [moved] = next.splice(from, 1);
      next.splice(to, 0, moved);
      return next;
    }),
  );

  const clear = useStableCallback(() =>
    setItems((current) => (current.length === 0 ? current : [])),
  );

  return useMemo(
    () => ({ items, set, push, insertAt, replaceAt, removeAt, move, clear }),
    [items, set, push, insertAt, replaceAt, removeAt, move, clear],
  );
}

/** A set of members, and the questions asked of one. */
export type UseSetReturn<T> = {|
  readonly items: $ReadOnlySet<T>,
  readonly has: (item: T) => boolean,
  readonly add: (item: T) => void,
  readonly remove: (item: T) => void,
  readonly toggle: (item: T) => void,
  readonly clear: () => void,
  readonly set: (items: Iterable<T>) => void,
|};

/**
 * A set, which is what a multi-select or a list of expanded rows actually is.
 *
 * The `Set` is replaced rather than mutated on every change, because a `Set`
 * edited in place is the same object and React would not re-render — the bug
 * people meet the first time they put a collection in `useState`.
 */
export hook useSet<T>(initial?: Iterable<T>): UseSetReturn<T> {
  const [items, setItems] = useState<$ReadOnlySet<T>>(() => new Set(initial));

  // `useCallback` rather than `useStableCallback`, and the difference is a bug
  // this hook had: a stable callback's body is installed in an insertion
  // effect, which runs *after* the render that produced it — so a render
  // asking `has(item)` about the set it is currently displaying would be
  // answered from the previous one. A stable identity is for a callback that
  // crosses into an effect or an event handler; a question a render asks has
  // to change when the answer does.
  const has = useCallback((item: T) => items.has(item), [items]);

  const add = useStableCallback((item: T) =>
    setItems((current) => (current.has(item) ? current : new Set(current).add(item))),
  );

  const remove = useStableCallback((item: T) =>
    setItems((current) => {
      if (!current.has(item)) {
        return current;
      }
      const next = new Set(current);
      next.delete(item);
      return next;
    }),
  );

  const toggle = useStableCallback((item: T) =>
    setItems((current) => {
      const next = new Set(current);
      if (!next.delete(item)) {
        next.add(item);
      }
      return next;
    }),
  );

  const clear = useStableCallback(() =>
    setItems((current) => (current.size === 0 ? current : new Set())),
  );

  const set = useStableCallback((next: Iterable<T>) => setItems(new Set(next)));

  return useMemo(
    () => ({ items, has, add, remove, toggle, clear, set }),
    [items, has, add, remove, toggle, clear, set],
  );
}

/** A position in a list that wraps. */
export type UseCycleReturn<T> = {|
  /** The value at the current position, or `null` when the list is empty. */
  readonly value: T | null,
  readonly index: number,
  readonly next: () => void,
  readonly previous: () => void,
  readonly go: (index: number) => void,
|};

/**
 * Step through a list, wrapping at both ends.
 *
 * A theme switcher, a carousel, a sort order that cycles. The counter behind
 * this is unbounded and the position is worked out from it on each render, so
 * `values` may change length between renders without the position becoming
 * invalid — and `values` is deliberately not a dependency of anything, so
 * writing the list inline in the call is free.
 *
 * `value` is `T | null` rather than `T` because an empty list has no current
 * value. Flow's array access would happily have said `T` and handed back an
 * `undefined` at runtime; this package does not claim what it cannot show.
 */
export hook useCycle<T>(values: $ReadOnlyArray<T>, initialIndex: number = 0): UseCycleReturn<T> {
  const [raw, setRaw] = useState(initialIndex);

  const next = useStableCallback(() => setRaw((current) => current + 1));
  const previous = useStableCallback(() => setRaw((current) => current - 1));
  const go = useStableCallback((index: number) => setRaw(index));

  const length = values.length;
  // Two modulos, because JavaScript's `%` keeps the sign of its left operand
  // and `previous()` from position zero would otherwise be -1.
  const index = length === 0 ? -1 : ((raw % length) + length) % length;
  const value = index < 0 ? null : values[index];

  return useMemo(() => ({ value, index, next, previous, go }), [value, index, next, previous, go]);
}

/** A value with the history behind and ahead of it. */
export type UseUndoableReturn<T> = {|
  readonly value: T,
  readonly set: (next: T) => void,
  readonly undo: () => void,
  readonly redo: () => void,
  readonly canUndo: boolean,
  readonly canRedo: boolean,
  /** Keep the current value, forget how it got here. */
  readonly clear: () => void,
  /** Back to the value the hook started with, history and all. */
  readonly reset: () => void,
|};

/** The three parts of an undo stack, kept in one state so they cannot disagree. */
type Timeline<T> = {|
  readonly past: $ReadOnlyArray<T>,
  readonly present: T,
  readonly future: $ReadOnlyArray<T>,
|};

/**
 * A value that can be undone and redone.
 *
 * One `useState` holding all three parts, not three: past, present and future
 * change together, and three separate states would be three renders and a
 * window in which they disagree.
 *
 * `set` clears the future, which is what every editor does — typing after an
 * undo abandons what was undone. `limit` bounds the past so that a long
 * editing session does not hold every version of a large value alive; the
 * oldest entries are dropped, and `canUndo` stops being true when they run
 * out.
 */
export hook useUndoable<T>(
  initial: T,
  options?: {| readonly limit?: number |},
): UseUndoableReturn<T> {
  const limit = options?.limit ?? 100;
  const [timeline, setTimeline] = useState<Timeline<T>>({
    past: [],
    present: initial,
    future: [],
  });

  const set = useStableCallback((next: T) =>
    setTimeline((current) => {
      if (Object.is(current.present, next)) {
        return current;
      }
      const past = [...current.past, current.present];
      return {
        past: past.length > limit ? past.slice(past.length - limit) : past,
        present: next,
        future: [],
      };
    }),
  );

  const undo = useStableCallback(() =>
    setTimeline((current) => {
      const previous = current.past[current.past.length - 1];
      if (current.past.length === 0) {
        return current;
      }
      return {
        past: current.past.slice(0, -1),
        present: previous,
        future: [current.present, ...current.future],
      };
    }),
  );

  const redo = useStableCallback(() =>
    setTimeline((current) => {
      const [ahead, ...rest] = current.future;
      if (current.future.length === 0) {
        return current;
      }
      return { past: [...current.past, current.present], present: ahead, future: rest };
    }),
  );

  const clear = useStableCallback(() =>
    setTimeline((current) =>
      current.past.length === 0 && current.future.length === 0
        ? current
        : { past: [], present: current.present, future: [] },
    ),
  );

  const reset = useStableCallback(() => setTimeline({ past: [], present: initial, future: [] }));

  return useMemo(
    () => ({
      value: timeline.present,
      set,
      undo,
      redo,
      canUndo: timeline.past.length > 0,
      canRedo: timeline.future.length > 0,
      clear,
      reset,
    }),
    [timeline, set, undo, redo, clear, reset],
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

/**
 * `localStorage` or `sessionStorage`, or `null` where neither is readable.
 *
 * # Why this guard is written twice
 *
 * `@uniflowed/state`'s `createJSONStorage`
 * (`packages/state/internal/composed.js`) guards the same four hazards — a
 * storage property that throws, a read that throws, a write that throws, and
 * finding the object a `storage` event arrives on — and reaches the same
 * conclusions about each. Merging the two was considered and declined;
 * ubugeeei-prod/uf#318 is the issue, and this is half of the decision. The other half is
 * in `createJSONStorage`, which carries the argument in full.
 *
 * In short: the helper would have to live in a package both may depend on,
 * `@uniflowed/web` is the only candidate, and it is not on npm while this
 * package is — so the edge would make `npm install @uniflowed/hooks` answer
 * `ETARGET`. `tools/ci/publishable.sh` refuses it now.
 *
 * What is *not* shared even in principle is the listener registry above. Two
 * components reading one key in one document have to agree, so a write here
 * announces itself; `createJSONStorage` deliberately does not announce, so
 * that two stores in one process stay two stores. A shared helper would have
 * had to leave that decision to its caller, which is most of what there was
 * to share.
 */
function area(session: boolean): Storage | null {
  const win = browserWindow();
  if (win == null) {
    return null;
  }
  try {
    return (session ? win.sessionStorage : win.localStorage) ?? null;
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
export hook useStorage<T>(
  key: string,
  initial: T,
  options?: {| readonly session?: boolean |},
): [T, (value: T) => void] {
  const session = options?.session ?? false;

  const subscribe = useCallback(
    (notify: () => void) => {
      const set = listeners.get(key) ?? new Set();
      set.add(notify);
      listeners.set(key, set);
      const onStorage = (event: StorageEvent) => {
        // A `null` key is the whole area being cleared, which every key is
        // affected by.
        if (event.key == null || event.key === key) {
          notify();
        }
      };
      const win = browserWindow();
      win?.addEventListener("storage", onStorage);
      return () => {
        set.delete(notify);
        win?.removeEventListener("storage", onStorage);
      };
    },
    [key],
  );

  const raw = useSyncExternalStore(
    subscribe,
    useCallback(() => {
      try {
        return area(session)?.getItem(key) ?? null;
      } catch {
        return null;
      }
    }, [key, session]),
    () => null,
  );

  const value = useMemo((): T => {
    if (raw == null) {
      return initial;
    }
    try {
      // The one unchecked step in this hook, and the comparison with
      // `createJSONStorage` is what turned it up: that one marks the cast and
      // offers a `revive` to close it, this one used to hand `JSON.parse`'s
      // `any` back as a `T` without saying so. The trade is the same and so is
      // the reason — persistence is a cache, and a cache that refuses to start
      // because an older version of the application wrote the key is worse
      // than one that is occasionally stale — but it is a trade, so it is
      // named.
      return JSON.parse(raw) as $FlowFixMe;
    } catch {
      return initial;
    }
  }, [raw, initial]);

  const write = useStableCallback((next: T) => {
    try {
      // `JSON.stringify` has no string for `undefined`, and writing the word
      // "undefined" would be a value that parses back as something else.
      // Removing the key is what makes the next read fall back to `initial`,
      // which is what a caller who wrote `undefined` meant.
      const encoded = JSON.stringify(next);
      if (encoded == null) {
        area(session)?.removeItem(key);
      } else {
        area(session)?.setItem(key, encoded);
      }
    } catch {
      // Full, or blocked. The announcement still happens so the components
      // sharing this key agree with each other for this session.
    }
    announce(key);
  });

  return [value, write];
}
