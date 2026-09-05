// @flow
//
// Atoms assembled out of other atoms.
//
// Everything here is written with the same four constructors an application
// has, and none of it reaches into a store: a family is a memoised factory, a
// default is a selector over a hidden primitive, persisted state is a writable
// selector whose write also touches storage. That is the point of keeping them
// in one module — they are worked examples of the public API, and if one of
// them needed a private hook the API would be missing something.
//
// # Why a hidden primitive rather than a flag on the store
//
// `atomWithDefault` needs to remember "nobody has set this yet", and the
// obvious home for that is a field on the store's entry for the atom. It would
// also be invisible to the dependency graph: the atom would not recompute when
// the flag changed. Keeping the flag in an ordinary atom means the machinery
// that already tracks values tracks this too — and it is what makes the
// dependency on the *default* disappear the moment a value is set, because the
// selector stops reading it.

import type { AtomGetter, AtomOptions, AtomRecord, Loadable, SetAction } from "./atom.js";
import { definePrimitive, defineSelector } from "./atom.js";

/**
 * The argument that puts an atom back the way it was.
 *
 * Opaque so it cannot be confused with a value: an atom of `symbol` would
 * otherwise have a value that silently means "reset".
 */
export opaque type Reset = symbol;

export const RESET: Reset = Symbol("@uniflowed/state RESET");

/**
 * The part of `localStorage` this package uses.
 *
 * Named rather than taken as `Storage`, because a server has no `Storage` and
 * a test should not need one: anything with these two methods will do.
 */
export type StorageAdapter = {
  readonly getItem: (key: string) => null | string,
  readonly setItem: (key: string, value: string) => void,
};

/**
 * A keyed collection of atoms, created on first use.
 *
 * `remove` matters more than it looks: a family keyed by something unbounded —
 * a search term, a date — otherwise keeps one atom per key ever asked for, and
 * the atoms are reachable from the family, so nothing collects them.
 */
export type AtomFamily<Key, Member> = {
  (key: Key): Member,
  readonly remove: (key: Key) => void,
  readonly size: () => number,
  ...
};

/**
 * One atom per key, created on first use and the same one thereafter.
 *
 * The alternative — one atom holding a map — re-renders every reader when any
 * entry changes, because the map is one value. A family gives each key its own
 * atom, so a list of a thousand rows re-renders one row.
 */
export function atomFamily<Key, Member>(create: (key: Key) => Member): AtomFamily<Key, Member> {
  const members: Map<Key, Member> = new Map();
  const family = (key: Key): Member => {
    const existing = members.get(key);
    if (existing !== undefined) {
      return existing;
    }
    const created = create(key);
    members.set(key, created);
    return created;
  };
  family.remove = (key: Key) => {
    members.delete(key);
  };
  family.size = () => members.size;
  return family;
}

/** Whether an override has been written, and what it is. */
type Slot<T> = { readonly filled: false } | { readonly filled: true, readonly value: T };

/**
 * An atom whose value is computed until someone writes one.
 *
 * The interesting property is that the dependency on the default is dynamic:
 * while nothing has been written, the atom depends on everything `getDefault`
 * read; after a write it depends on the override alone, and changes to what
 * the default would have read recompute nothing.
 *
 * Writing [`RESET`] puts it back, and the dependency on the default with it.
 */
export function atomWithDefault<T>(
  getDefault: (get: AtomGetter) => T,
  options?: AtomOptions<T>,
): AtomRecord<T, SetAction<T> | Reset> {
  const empty: Slot<T> = { filled: false };
  const override = definePrimitive<Slot<T>>(empty, {
    debugLabel: `${options?.debugLabel ?? "atomWithDefault"} override`,
  });

  const resolve = (get: AtomGetter): T => {
    const slot = get(override);
    return slot.filled ? slot.value : getDefault(get);
  };

  return defineSelector(
    resolve,
    (get, set, argument) => {
      if (argument === RESET) {
        set(override, empty);
        return;
      }
      const next =
        typeof argument === "function"
          ? (argument as $FlowFixMe)(resolve(get))
          : (argument as $FlowFixMe);
      set(override, { filled: true, value: next });
    },
    options,
  );
}

/**
 * An atom mirrored into a key-value store.
 *
 * Persistence happens in the *write*, not in a subscription, so it does not
 * depend on the atom being mounted: state written from a route handler or a
 * test is stored the same as state written from a component. A subscription
 * would also have to be per store, and would keep the atom mounted for as long
 * as the process lived.
 *
 * Unreadable or malformed stored data falls back to `initial` rather than
 * throwing. Persistence is a cache, and a cache that can brick an application
 * on a schema change — or on a user editing their own `localStorage` — is
 * worse than no cache. Passing no `storage` yields a plain atom, which is what
 * makes the same module import safely on a server.
 */
export function atomWithStorage<T>(
  key: string,
  initial: T,
  storage?: StorageAdapter,
  options?: AtomOptions<T>,
): AtomRecord<T, SetAction<T>> {
  if (storage == null) {
    return definePrimitive(initial, { debugLabel: options?.debugLabel ?? key });
  }
  const base = definePrimitive<T>(restore(storage, key, initial), {
    debugLabel: `${options?.debugLabel ?? key} value`,
  });
  return defineSelector(
    (get) => get(base),
    (get, set, argument) => {
      const next = typeof argument === "function" ? (argument as $FlowFixMe)(get(base)) : argument;
      set(base, next);
      storage.setItem(key, JSON.stringify(next));
    },
    options,
  );
}

/**
 * The data an asynchronous atom is holding, or `fallback` until it has some.
 *
 * For the component that has nothing useful to render while a load is in
 * flight and does not want to say so three times in one file. It keeps the
 * failure quiet, which is the trade: a screen that shows the fallback forever
 * is the price of not handling the error, and `useAtomValue` on the loadable
 * itself is the version that makes the caller look at it.
 */
export function unwrap<T>(
  target: AtomRecord<Loadable<T>, empty>,
  fallback: T,
): AtomRecord<T, empty> {
  return defineSelector((get) => {
    const settled = get(target);
    return match (settled) {
      {state: "hasData", data: const data, ...} => data,
      _ => fallback,
    };
  }, null);
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
