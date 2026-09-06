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
//
// # Why persisted state is read on mount and not when it is declared
//
// `atomWithStorage` used to read storage as an argument to the atom it was
// building, so the read happened while the module was being evaluated. Every
// other constructor here can be called at module scope in a file a server
// imports; this one did I/O there, and it produced the one bug a state library
// must not have.
//
// The server renders the page with `initial`. The browser downloads the
// module, evaluates it, reads `"dark"` out of `localStorage` — and the first
// client render disagrees with the HTML that is already on screen. That is a
// hydration mismatch, and no amount of care in the React binding can prevent
// it, because the value was already wrong before a hook was called. It also
// gave every store in the process the same read and the same key, which is the
// case `internal/atom.js` says stores exist for.
//
// So the read moved into the hidden primitive's `onMount`, which the store
// binds to itself and React runs after commit. The first client render is
// `initial`, exactly like the server's; the stored value arrives immediately
// afterwards, in the store that mounted. The same hook is where a `subscribe`
// belongs, so a second tab's write reaches every mounted store and no
// unmounted one.
//
// The cost is honest and worth naming: an atom nothing has mounted reads
// `initial` even when storage holds something else, so a route handler that
// only reads sees the default. `getOnInit` is for that caller — it moves the
// read to first use rather than to mount, which is still per store and still
// not at import.

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
 * Where [`atomWithStorage`] keeps a value.
 *
 * Typed in the value rather than in strings, which is the difference between
 * this and the string store underneath it: a cookie jar, a React Native
 * `AsyncStorage`, an in-memory map in a test and `localStorage` do not agree on
 * a serialisation, and the one they would agree on is JSON — which is
 * [`createJSONStorage`]'s job, not this type's.
 *
 * `getItem` takes the initial value so that "nothing is stored" and "the
 * stored value will not parse" have one answer in one place rather than an
 * `?T` every caller unwraps the same way.
 *
 * `removeItem` is required, not optional. A persistence layer that cannot
 * delete is not one: state that can be stored and never un-stored has no
 * expression for `RESET`, and an application that offers "clear my
 * preferences" would have to reach past this type to do it.
 *
 * `subscribe` is optional because most storages have no way to tell anyone
 * they changed. One that does — the browser's `storage` event — is how a
 * second tab reaches this one. It is given the initial value to report when
 * the key is removed elsewhere.
 */
export type StorageAdapter<T> = {
  readonly getItem: (key: string, initial: T) => T,
  readonly setItem: (key: string, value: T) => void,
  readonly removeItem: (key: string) => void,
  readonly subscribe?: (key: string, onChange: (value: T) => void, initial: T) => () => void,
};

/**
 * The part of the Web Storage API [`createJSONStorage`] needs.
 *
 * Named rather than taken as `Storage`, because a server has no `Storage` and
 * a test should not need one: anything with these three methods will do.
 * Inexact, so a real `localStorage` — which has `length`, `key` and `clear`
 * as well — is one of these.
 */
export type StringStorage = {
  readonly getItem: (key: string) => null | string,
  readonly setItem: (key: string, value: string) => void,
  readonly removeItem: (key: string) => void,
  ...
};

/** What [`createJSONStorage`] can be told about the values it reads. */
export type JSONStorageOptions<T> = {
  /**
   * Turn what `JSON.parse` produced into a `T`, or throw to fall back to the
   * initial value.
   *
   * This is the package's one unchecked step, and the option exists so that a
   * caller who minds can check it: `JSON.parse` answers `any`, and what comes
   * back out of storage was written by an older version of the application, by
   * another tab, or by a person editing their own `localStorage`. The default
   * trusts it, because persistence is a cache and a cache that refuses to
   * start is worse than one that is occasionally stale. `revive: (raw) =>
   * schema.parse(raw)` with `@uniflowed/validator` is the version that does
   * not trust it, and it needs no support from here — a `revive` that throws
   * is a value that will not parse.
   */
  readonly revive?: (raw: mixed) => T,
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

/** What this store knows about the value behind a storage key. */
type Stored<T> = { readonly known: false } | { readonly known: true, readonly value: T };

/** What [`atomWithStorage`] accepts on top of what every atom does. */
export type StorageOptions<T> = {
  readonly debugLabel?: string,
  readonly equals?: (previous: T, next: T) => boolean,
  /**
   * Read the stored value the first time the atom is used in a store, rather
   * than when it is mounted there. `false` by default.
   *
   * The default is what makes a server-rendered page hydrate: the first client
   * render has to be the one the server already sent, and it cannot be if the
   * value has been read out of `localStorage` before React runs. Turning this
   * on says "there is no server render to agree with" — an application that is
   * only ever a browser tab, where a first paint holding `initial` is a flash
   * of the wrong theme and nothing else is at stake.
   *
   * Even on, the read is per store and happens on demand; it never happens
   * while the module is being evaluated.
   */
  readonly getOnInit?: boolean,
};

/**
 * An atom mirrored into a key-value store.
 *
 * Persistence happens in the *write*, not in a subscription, so it does not
 * depend on the atom being mounted: state written from a route handler or a
 * test is stored the same as state written from a component.
 *
 * The read is the other direction and does depend on it — see the note at the
 * top of this file. The stored value arrives on mount, in the store that
 * mounted, so that the first client render is the one the server rendered.
 * `getOnInit` is for the application that has no server render to agree with.
 *
 * Unreadable or malformed stored data falls back to `initial` rather than
 * throwing. Persistence is a cache, and a cache that can brick an application
 * on a schema change — or on a user editing their own `localStorage` — is
 * worse than no cache.
 *
 * Writing [`RESET`] removes the key, which is the only way to stop persisting
 * something: setting it back to `initial` stores `initial`.
 *
 * Passing no `storage` yields an atom that behaves the same in every other
 * way, including `RESET`, which is what a test and a server want.
 */
export function atomWithStorage<T>(
  key: string,
  initial: T,
  storage?: StorageAdapter<T>,
  options?: StorageOptions<T>,
): AtomRecord<T, SetAction<T> | Reset> {
  const label = options?.debugLabel ?? key;
  const equals = options?.equals;
  if (storage == null) {
    // Not a degenerate case worth a different shape: this is the form a test
    // and a prerender use, and `RESET` has to mean the same thing in it.
    return atomWithDefault<T>(() => initial, { debugLabel: label, equals });
  }

  const eager = options?.getOnInit ?? false;
  const unknown: Stored<T> = { known: false };

  const stored = definePrimitive<Stored<T>>(unknown, {
    debugLabel: `${label} stored`,
    onMount: (mount) => {
      if (!eager) {
        // Storage is the source of truth as this store joins it. A value
        // written before the mount was written to storage too, so adopting
        // what is there cannot lose one.
        mount.set({ known: true, value: storage.getItem(key, initial) });
      }
      const listen = storage.subscribe;
      if (listen == null) {
        return undefined;
      }
      return listen(
        key,
        (value) => {
          mount.set({ known: true, value });
        },
        initial,
      );
    },
  });

  const resolve = (get: AtomGetter): T => {
    const current = get(stored);
    if (current.known) {
      return current.value;
    }
    // Nothing has mounted this atom in this store, or `RESET` put it back.
    // `initial` is the answer a server gives and therefore the answer the
    // first client render has to give; `getOnInit` is the caller saying there
    // is no server.
    return eager ? storage.getItem(key, initial) : initial;
  };

  return defineSelector(
    resolve,
    (get, set, argument) => {
      if (argument === RESET) {
        set(stored, unknown);
        storage.removeItem(key);
        return;
      }
      const next =
        typeof argument === "function"
          ? (argument as $FlowFixMe)(resolve(get))
          : (argument as $FlowFixMe);
      set(stored, { known: true, value: next });
      storage.setItem(key, next);
    },
    { debugLabel: label, equals },
  );
}

/**
 * A [`StorageAdapter`] over `localStorage`, `sessionStorage`, or anything else
 * that holds strings.
 *
 * ```
 * const theme = atomWithStorage("theme", "light", createJSONStorage(() => localStorage));
 * ```
 *
 * The thunk is what makes that line safe in a file a server imports, and the
 * reason is more specific than "the storage might be missing": on a runtime
 * without Web Storage the *identifier* `localStorage` is not defined, so
 * evaluating it throws a `ReferenceError` rather than producing `undefined`.
 * The thunk is called inside a `try` every time, so a module that names a
 * storage no runtime here has still imports, and every operation on it becomes
 * a no-op returning the initial value.
 *
 * That is the whole portability story, and it was checked against four
 * runtimes: Node has Web Storage from 22 and only with a flag, Deno has
 * `localStorage` for a page it can name an origin for, Bun has it since 1.2,
 * and an edge runtime has neither it nor a `window`. Every one of those is a
 * `try` around the thunk and a `null` check afterwards.
 *
 * `getItem` and `setItem` are also wrapped: a browser with site data blocked
 * throws on the property, and Safari in private mode throws on a write once
 * its quota is reached. A storage that cannot be written to degrades to an
 * atom that is not persisted, which is the behaviour every one of those
 * applications wants and none of them would write by hand.
 *
 * `subscribe` listens for the browser's `storage` event, which is how another
 * tab reaches this one. It deliberately does *not* announce writes made in
 * this process: two stores in one process are two stores, and keeping them in
 * step would undo the isolation they exist for.
 */
export function createJSONStorage<T>(
  getStringStorage: () => StringStorage | null | void,
  options?: JSONStorageOptions<T>,
): StorageAdapter<T> {
  const revive = options?.revive;

  const resolve = (): StringStorage | null => {
    try {
      return getStringStorage() ?? null;
    } catch {
      return null;
    }
  };

  const parse = (raw: null | string, initial: T): T => {
    if (raw == null) {
      return initial;
    }
    try {
      const decoded = JSON.parse(raw);
      // The one unchecked step in the package, and it is one line rather than
      // one per call site. `revive` is how a caller closes it.
      return revive == null ? (decoded as $FlowFixMe) : revive(decoded);
    } catch {
      return initial;
    }
  };

  return {
    getItem: (key, initial) => {
      const store = resolve();
      if (store === null) {
        return initial;
      }
      try {
        return parse(store.getItem(key), initial);
      } catch {
        return initial;
      }
    },
    setItem: (key, value) => {
      const store = resolve();
      if (store === null) {
        return;
      }
      try {
        store.setItem(key, JSON.stringify(value));
      } catch {
        // Out of quota, or site data blocked. The value is still the atom's;
        // it simply will not outlive the tab.
      }
    },
    removeItem: (key) => {
      const store = resolve();
      if (store === null) {
        return;
      }
      try {
        store.removeItem(key);
      } catch {
        // As above.
      }
    },
    subscribe: (key, onChange, initial) => {
      const host = storageEvents();
      if (host === null) {
        return () => {};
      }
      const listener = (event: StorageEvent) => {
        // A `null` key is the whole area being cleared, which every key in it
        // is affected by.
        if (event.key != null && event.key !== key) {
          return;
        }
        onChange(parse(event.key == null ? null : event.newValue, initial));
      };
      host.addEventListener("storage", listener);
      return () => {
        host.removeEventListener("storage", listener);
      };
    },
  };
}

/** What a `storage` event is dispatched on, where there is one. */
type StorageEvents = {
  readonly addEventListener: (type: "storage", listener: (event: StorageEvent) => mixed) => void,
  readonly removeEventListener: (type: "storage", listener: (event: StorageEvent) => mixed) => void,
  ...
};

/**
 * The object a `storage` event will arrive on, or `null` where none will.
 *
 * In a browser `globalThis` *is* the window, so `globalThis.addEventListener`
 * looks right. It is wrong anywhere a document has been installed onto another
 * host's global — every uf test process, where `globalThis` is Node's and has
 * no `addEventListener` at all. Asking the document's own window first and
 * falling back covers both, and the `typeof` check covers the servers where
 * neither answers.
 */
function storageEvents(): null | StorageEvents {
  const host = globalThis.window ?? globalThis;
  return typeof host?.addEventListener === "function" ? host : null;
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
