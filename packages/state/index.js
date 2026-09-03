// @flow
//
// `@uniflowed/state`: the cell primitives, plus the React binding.
//
// # Why this is thin
//
// Everything reactive lives in `@uniflowed/cell`: tracking, memoisation, the
// equality cutoff, batching. This package adds React and nothing else, so the
// same store works in a server action, a worker, a test and a component
// without a provider, a context, or a second copy of the state.
//
// # Why `useSyncExternalStore`
//
// A cell already *is* an external store, so the binding is the hook React
// provides for exactly that, rather than a `useState` mirror kept in step by
// an effect. The difference is not stylistic: a mirror is written during
// commit, so a concurrent render can read one component's copy before another
// component's copy has caught up, and the two disagree on screen. That is
// tearing, and `useSyncExternalStore` is the API that exists to prevent it.
//
// # Why the subscribe and snapshot callbacks are cached per atom
//
// React re-subscribes whenever the identity of the `subscribe` argument
// changes. A `useCallback` would keep it stable within one component; caching
// it on the atom keeps it stable across every component reading that atom, so
// mounting the thousandth reader allocates nothing.

import type { Cell, CellScope, ResourceStatus, Unsubscribe } from "@uniflowed/cell";
import {
  batch,
  cell,
  computed,
  read,
  resource,
  status,
  subscribe,
  untracked,
  update,
  write,
} from "@uniflowed/cell";
import { useSyncExternalStore } from "@uniflowed/react";

// $FlowFixMe[value-as-type] uf package-to-package inference is not wired yet.
export opaque type Atom<T> = Cell<T>;
// $FlowFixMe[value-as-type] uf package-to-package inference is not wired yet.
export opaque type ReadonlyAtom<T> = Cell<T>;
export type AtomSetter<T> = (next: T | ((current: T) => T)) => void;
export type AtomTuple<T> = [T, AtomSetter<T>];
export type StorageAdapter = {|
  readonly getItem: (key: string) => null | string,
  readonly setItem: (key: string, value: string) => void,
|};

export type { Cell, CellScope, ResourceStatus, Unsubscribe };
export { batch, cell, computed, read, resource, status, subscribe, untracked, update, write };

/**
 * A writable piece of state, readable from anywhere and owned by nobody.
 *
 * There is no store object and no provider: an atom is a value you import.
 * That is the whole ergonomic bet, and it is why the same atom can be read in
 * a loader, written from an event handler, and asserted on in a test with no
 * React in sight.
 */
export function atom<T>(initial: T): Atom<T> {
  return cell(initial);
}

/**
 * State derived from other atoms, recomputed only when what it read changes.
 *
 * No dependency array, because the derive discovers its own dependencies by
 * running — see `@uniflowed/cell`. A selector that returns an unchanged value
 * does not re-render its readers.
 */
export function selector<T>(derive: () => T): ReadonlyAtom<T> {
  return computed(derive);
}

/**
 * An atom mirrored into a key-value store.
 *
 * Unreadable or malformed stored data falls back to `initial` rather than
 * throwing. Persistence is a cache, and a cache that can brick an application
 * on a schema change — or on a user editing their own `localStorage` — is
 * worse than no cache. Passing no `storage` yields a plain atom, which is what
 * makes the same module import safely on a server.
 */
export function atomWithStorage<T>(key: string, initial: T, storage?: StorageAdapter): Atom<T> {
  if (storage == null) {
    return atom(initial);
  }
  const source = atom(restore(storage, key, initial));
  subscribe(source, () => {
    storage.setItem(key, JSON.stringify(read(source)));
  });
  return source;
}

function restore<T>(storage: StorageAdapter, key: string, initial: T): T {
  const stored = storage.getItem(key);
  if (stored == null) {
    return initial;
  }
  try {
    return JSON.parse(stored);
  } catch {
    return initial;
  }
}

/**
 * A family of atoms addressed by key, created on first use.
 *
 * The alternative — one atom holding a map — re-renders every reader when any
 * entry changes, because the map is one value. A family gives each key its own
 * cell, so a list of a thousand rows updates one row.
 */
export function atomFamily<Key, T>(create: (key: Key) => Atom<T>): (key: Key) => Atom<T> {
  const members: Map<Key, Atom<T>> = new Map();
  return (key) => {
    const existing = members.get(key);
    if (existing != null) {
      return existing;
    }
    const created = create(key);
    members.set(key, created);
    return created;
  };
}

/**
 * Per-atom `subscribe` callbacks, so their identity never changes.
 *
 * Keyed weakly: an atom that goes out of scope takes its cached callbacks with
 * it, which matters for [`atomFamily`], where keys can be unbounded.
 */
const subscribers: WeakMap<
  // $FlowFixMe[value-as-type] uf package-to-package inference is not wired yet.
  Cell<any>,
  (listener: () => void) => Unsubscribe,
> = new WeakMap();

// $FlowFixMe[value-as-type] uf package-to-package inference is not wired yet.
const readers: WeakMap<Cell<any>, () => mixed> = new WeakMap();

function subscriberFor<T>(
  // $FlowFixMe[value-as-type] uf package-to-package inference is not wired yet.
  source: Cell<T>,
): (listener: () => void) => Unsubscribe {
  const existing = subscribers.get(source);
  if (existing != null) {
    return existing;
  }
  const created = (listener: () => void) => subscribe(source, listener);
  subscribers.set(source, created);
  return created;
}

function readerFor<T>(
  // $FlowFixMe[value-as-type] uf package-to-package inference is not wired yet.
  source: Cell<T>,
): () => T {
  const existing = readers.get(source);
  if (existing != null) {
    // $FlowFixMe[incompatible-return] the map is keyed by the cell it reads.
    return existing;
  }
  const created = () => read(source);
  readers.set(source, created);
  return created;
}

/**
 * Apply a setter argument, which may be a value or a reducer.
 *
 * The reducer form is applied through `update`, so it reads and writes as one
 * step and two updates in the same tick both land.
 */
function applyAtomUpdate<T>(source: Atom<T>, next: T | ((current: T) => T)): void {
  if (typeof next === "function") {
    // Flow cannot refine callable union payloads here yet, so the dynamic
    // updater boundary lives in one helper instead of leaking into every hook.
    // $FlowFixMe[incompatible-call]
    update(source, next);
    return;
  }
  write(source, next);
}

/** Subscribe a component to any cell, including a derived one. */
export hook useCell<T>(
  // $FlowFixMe[value-as-type] uf package-to-package inference is not wired yet.
  source: Cell<T>,
): T {
  const reader = readerFor(source);
  // The server snapshot is the same read: a cell holds its value on both
  // sides, so hydration compares like with like instead of a placeholder.
  return useSyncExternalStore(subscriberFor(source), reader, reader);
}

/** Read an atom, re-rendering when it changes. */
export hook useAtomValue<T>(source: Atom<T>): T {
  return useCell(source);
}

/**
 * Write an atom without reading it.
 *
 * A component that only dispatches does not subscribe, so it does not
 * re-render when the value it writes changes. The setter's identity is stable
 * for the life of the atom, so passing it to a memoised child is free.
 */
export hook useSetAtom<T>(source: Atom<T>): AtomSetter<T> {
  return setterFor(source);
}

/** Read and write an atom, in the shape `useState` returns. */
export hook useAtom<T>(source: Atom<T>): AtomTuple<T> {
  return [useCell(source), setterFor(source)];
}

// $FlowFixMe[value-as-type] uf package-to-package inference is not wired yet.
const setters: WeakMap<Cell<any>, AtomSetter<any>> = new WeakMap();

function setterFor<T>(source: Atom<T>): AtomSetter<T> {
  const existing = setters.get(source);
  if (existing != null) {
    return existing;
  }
  const created: AtomSetter<T> = (next) => {
    applyAtomUpdate(source, next);
  };
  setters.set(source, created);
  return created;
}
