// @flow
//
// Atoms assembled out of other atoms.
//
// Everything here is written with the same constructors an application has,
// and none of it reaches into a store: a family is a memoised factory, a
// default is a selector over a hidden primitive, persisted state is a writable
// selector whose write also touches storage, and an atom persisted to
// something asynchronous is that selector over an `asyncAtom`. That is the
// point of keeping them in one module — they are worked examples of the public
// API, and if one of them needed a private hook the API would be missing
// something.
//
// The parity work in ubugeeei-prod/uf#292 is the strongest evidence that claim is true:
// `selectAtom`, `atomWithReducer`, `atomWithReset` and `freezeAtom` are four
// of `jotai/utils`' names, they are between one and six lines each here, and
// not one of them needed anything the four constructors do not already offer.
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

import type { LoadContext } from "@uniflowed/cell";

import type { AtomGetter, AtomOptions, AtomRecord, Loadable, SetAction } from "./atom.js";
import { defineAsync, definePrimitive, defineSelector } from "./atom.js";

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

/**
 * One part of another atom, recomputed only when that part changes.
 *
 * Jotai's `selectAtom(anAtom, selector, equalityFn)`. The point is the
 * equality cutoff rather than the projection: a component reading
 * `selectAtom(user, (u) => u.name)` re-renders when the name changes and not
 * when the avatar does, even though both live in one atom.
 *
 * One difference from Jotai, kept rather than papered over. Its selector is
 * called with `(value, previousSlice)`, and this one is called with the value
 * alone. A read that can see its own previous output is not a pure function of
 * its dependencies, which is the property `@uniflowed/cell`'s equality cutoff
 * is built on, so the parameter has nowhere to come from. It also has nothing
 * left to do: `prevSlice` exists so that a selector building a fresh array
 * each time can return the previous one and avoid a re-render, and `equals` is
 * the direct way to say that.
 */
export function selectAtom<T, Slice>(
  source: AtomRecord<T, empty>,
  select: (value: T) => Slice,
  equals?: (previous: Slice, next: Slice) => boolean,
): AtomRecord<Slice, empty> {
  return defineSelector(
    (get) => select(get(source)),
    null,
    equals == null ? undefined : { equals },
  );
}

/**
 * A value and the actions that change it: `useReducer`, as an atom.
 *
 * Jotai's `atomWithReducer(initialValue, reducer)`, built the way
 * [`atomWithDefault`] is — a hidden primitive holding the value, and a
 * writable selector over it whose write is the only way in.
 *
 * The reason to reach for this rather than an [`atom`] is in the type. It is a
 * `WritableAtom<State, Action>` where the two parameters differ, so a state
 * cannot be handed to it by mistake, and `useSetAtom` gives a component a
 * `dispatch` whose argument Flow checks against the actions the reducer names
 * rather than against `State`.
 */
export function atomWithReducer<State, Action>(
  initial: State,
  reduce: (state: State, action: Action) => State,
  options?: AtomOptions<State>,
): AtomRecord<State, Action> {
  const value = definePrimitive<State>(initial, {
    debugLabel: `${options?.debugLabel ?? "atomWithReducer"} value`,
  });

  return defineSelector(
    (get) => get(value),
    (get, set, action) => {
      // `get` inside a write records no dependency, so reading the current
      // state to reduce it does not make the atom recompute for it.
      set(value, reduce(get(value), action));
    },
    options,
  );
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
 * A piece of state that also answers to [`RESET`].
 *
 * Jotai's `atomWithReset(initialValue)`. It is [`atomWithDefault`] with the
 * signature a caller who has a value rather than a derivation actually wants,
 * and that is the whole of it — but it is worth the name, because `atom(0)`
 * cannot take `RESET` and "the resettable one is the one called
 * `atomWithDefault`" is not something anybody guesses.
 *
 * Jotai's `atomWithLazy` is deliberately *not* here for the mirror-image
 * reason: its argument is already the function `atomWithDefault` takes, so it
 * would be a second name for one thing rather than a second signature for it.
 */
export function atomWithReset<T>(
  initial: T,
  options?: AtomOptions<T>,
): AtomRecord<T, SetAction<T> | Reset> {
  return atomWithDefault(() => initial, options);
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
 *
 * # Why this guard is written twice
 *
 * `@uniflowed/hooks`'s `useStorage` (`packages/hooks/state.js`) guards the
 * same four hazards — a storage property that throws, a read that throws, a
 * write that throws, and finding the object a `storage` event arrives on —
 * and was written from the same reasoning by somebody who had not read this.
 * Merging the two was considered and declined; ubugeeei-prod/uf#318 is the issue, and this
 * is half of the decision. The other half is in `useStorage`.
 *
 * A shared helper has to live somewhere both packages may depend on, and
 * there is no such place. `@uniflowed/web` is the obvious home — it already
 * owns the platform bindings, and `cookie.js` reaches for `globalThis` the
 * same way — but `@uniflowed/hooks` is on npm and `@uniflowed/web` is not
 * (`tools/release/pending-packages.txt`). That edge would make
 * `npm install @uniflowed/hooks` answer `ETARGET`: the tarball would name a
 * version of `@uniflowed/web` the registry does not have, and it would
 * install perfectly from this workspace, which is exactly how #409 stayed
 * hidden. `tools/ci/publishable.sh` now refuses that edge, so the reason this
 * decision rests on is a check rather than a paragraph.
 *
 * The two are also less alike than the list of hazards suggests. This one is
 * defined over a thunk the caller supplies and answers a `T`; that one picks
 * its area from a boolean and answers a string. That one keeps a registry of
 * same-tab listeners so two components sharing a key agree, which this one
 * must not have — see the paragraph above. What is common once those are
 * taken out is four `try` blocks and a `JSON.parse` with a fallback, and the
 * shared thing worth extracting from that is the *reasoning*, which is why
 * each copy now names the other.
 *
 * What would change the answer, precisely: `@uniflowed/web` reaching npm
 * (#210) removes the blocking reason, and a *third* caller writing these
 * guards a third time removes the other one. Neither has happened, and a
 * helper built before either is a dependency edge bought on speculation.
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
 * Where [`atomWithAsyncStorage`] keeps a value: IndexedDB, a React Native
 * `AsyncStorage`, a server-backed preference store.
 *
 * The same shape as [`StorageAdapter`] with the read made a promise, and it is
 * a separate type rather than a widening of that one because the difference is
 * not the adapter's — it is the *atom's value type*, which becomes a
 * [`Loadable`]. See [`atomWithAsyncStorage`].
 *
 * `getItem` is handed the load's `LoadContext`, so an adapter that can stop an
 * IndexedDB request or a fetch has the signal to stop it with. Ignoring the
 * third parameter is fine and is what a `Map`-backed adapter in a test does:
 * whether a settled read is still the one the atom is waiting for is decided
 * by `@uniflowed/cell` either way, and the signal only decides whether the
 * work carries on in the meantime.
 *
 * `setItem` and `removeItem` answer a promise or nothing, and the atom does
 * not wait for either — see the note about failed writes on
 * [`atomWithAsyncStorage`].
 */
export type AsyncStorageAdapter<T> = {
  readonly getItem: (key: string, initial: T, context: LoadContext) => Promise<T>,
  readonly setItem: (key: string, value: T) => Promise<mixed> | void,
  readonly removeItem: (key: string) => Promise<mixed> | void,
  readonly subscribe?: (key: string, onChange: (value: T) => void, initial: T) => () => void,
};

/**
 * What [`atomWithAsyncStorage`]'s setter accepts.
 *
 * A value, or a function of what the atom currently holds — which is a
 * [`Loadable`] rather than a `T`, and that is not a wrinkle to be smoothed
 * over. A write can happen before the first read has settled, so there may be
 * no current value to reduce; a reducer typed `(current: T) => T` would be a
 * promise the atom cannot keep. Handed the loadable, a caller who wants to
 * increment a persisted counter has to say what "increment" means before the
 * count has arrived, which is a question they have to answer anyway.
 */
export type AsyncSetAction<T> = T | ((current: Loadable<T>) => T);

/** What [`atomWithAsyncStorage`] accepts on top of what every atom does. */
export type AsyncStorageOptions<T> = {
  readonly debugLabel?: string,
  /**
   * When two values of `T` are the same value.
   *
   * Over `T` rather than over `Loadable<T>`, because `T` is what a caller has
   * an opinion about; the constructor lifts it to the loadable, where
   * `loading` equals `loading` and an error equals itself.
   */
  readonly equals?: (previous: T, next: T) => boolean,
};

/**
 * A storage write whose outcome is nobody's value.
 *
 * The rejection has to be attached — a promise that rejects with no handler is
 * an unhandled rejection, and on Node that is a process that exits — but there
 * is nothing to attach it *to*. See the note on [`atomWithAsyncStorage`].
 */
function discardOutcome(result: Promise<mixed> | void): void {
  if (result != null) {
    result.then(doNothing, doNothing);
  }
}

function doNothing(): void {}

/**
 * Lift an equality over `T` to one over `Loadable<T>`.
 *
 * Without this the atom would re-render every reader on every write, including
 * one that stored the value already showing: the loadable is built fresh by
 * the read, so `Object.is` on it is never true. The synchronous
 * [`atomWithStorage`] gets this for nothing, because the value it returns *is*
 * the `T`.
 */
function sameLoadable<T>(
  equals: void | ((previous: T, next: T) => boolean),
): (previous: Loadable<T>, next: Loadable<T>) => boolean {
  const sameValue = equals ?? Object.is;
  return (previous, next) => {
    if (previous.state !== next.state) {
      return false;
    }
    if (previous.state === "hasData" && next.state === "hasData") {
      return sameValue(previous.data, next.data);
    }
    if (previous.state === "hasError" && next.state === "hasError") {
      return Object.is(previous.error, next.error);
    }
    return true;
  };
}

/**
 * An atom persisted to a storage whose read is asynchronous.
 *
 * ```
 * const draft = atomWithAsyncStorage("draft", "", indexedDbStorage);
 * // read(draft) is { state: "loading" }, then { state: "hasData", data: … }
 * ```
 *
 * A second constructor rather than an option on [`atomWithStorage`], because
 * the value type is different: it is a [`Loadable`] until the first read
 * settles, and it stays one afterwards so that a component renders the same
 * three cases it renders for any other asynchronous value. Jotai's
 * `atomWithStorage` makes the value `T | Promise<T>` and leans on Suspense to
 * render it; this package has declined Suspense (see `asyncAtom`), and taking
 * the signature without it would leave the type saying `T` for a value that is
 * not there yet. ubugeeei-prod/uf#317 is where that was worked out.
 *
 * Three things had to be decided rather than typed, and each is a property
 * `tests/library/state.test.js` holds this to.
 *
 * **A write while the first read is in flight wins, and the read is dropped.**
 * Not by a flag counting generations — `@uniflowed/cell` already has one, and
 * a second answer to one question is how two of them come to disagree. A write
 * fills a hidden override, the read stops looking at the load, and the load
 * loses its last reader: the cell abandons it, aborts its signal, and never
 * delivers. It is the mechanism [`atomWithDefault`] uses to stop depending on
 * its default, applied to a load instead of a derivation.
 *
 * **A read that fails is `hasError`, not `initial`.** This is the one place
 * where the two constructors part company on purpose. The synchronous one
 * falls back, because persistence is a cache and it has nowhere to put the
 * failure; here there is somewhere to put it, and "the database is locked" is
 * not the same fact as "nobody has set a preference". An *absent* key is still
 * the adapter's business and still answers `initial`, so the two stay
 * distinguishable.
 *
 * **A write that fails is silent**, which is the opposite choice and the same
 * reasoning: the value the caller wrote is the atom's value whatever storage
 * did with it, so a failed write has no value to be. It will simply not
 * outlive the session. The promise is still attached, because an unhandled
 * rejection ends a Node process.
 *
 * `RESET` removes the key and puts the atom back to `{ hasData, initial }`
 * rather than forgetting and reading again. Reading again would race the
 * removal — the read is asynchronous and the removal has not finished — and
 * would answer with the value that was just deleted. The synchronous version
 * can afford to forget precisely because its removal is already done by the
 * time anything reads.
 *
 * The one habit that does not carry over from [`atomWithStorage`]: the read
 * starts as soon as a store is asked for the atom, mounted or not. That one
 * waits for a mount because the first client render has to be the one the
 * server already sent; here both sides render `loading`, so there is nothing
 * to disagree with and nothing to wait for.
 */
export function atomWithAsyncStorage<T>(
  key: string,
  initial: T,
  storage: AsyncStorageAdapter<T>,
  options?: AsyncStorageOptions<T>,
): AtomRecord<Loadable<T>, AsyncSetAction<T> | Reset> {
  const label = options?.debugLabel ?? key;
  const empty: Slot<T> = { filled: false };

  // Everything this store knows that the load does not: what was written here,
  // and what another tab told this store while it was mounted. Filling it is
  // what takes the load out of the atom's dependencies.
  const override = definePrimitive<Slot<T>>(empty, {
    debugLabel: `${label} override`,
    onMount: (mount) => {
      const listen = storage.subscribe;
      if (listen == null) {
        return undefined;
      }
      return listen(
        key,
        (value) => {
          mount.set({ filled: true, value });
        },
        initial,
      );
    },
  });

  const loaded = defineAsync<T>((_get, context) => storage.getItem(key, initial, context), {
    debugLabel: `${label} loaded`,
  });

  const resolve = (get: AtomGetter): Loadable<T> => {
    const slot = get(override);
    // Reading `loaded` only in this branch is the whole of the write-wins
    // behaviour: once the override is filled the load is not a dependency, so
    // nothing recomputes when it settles and the cell stops it.
    return slot.filled ? { state: "hasData", data: slot.value } : get(loaded);
  };

  return defineSelector(
    resolve,
    (get, set, argument) => {
      if (argument === RESET) {
        set(override, { filled: true, value: initial });
        discardOutcome(storage.removeItem(key));
        return;
      }
      const next =
        typeof argument === "function"
          ? (argument as $FlowFixMe)(resolve(get))
          : (argument as $FlowFixMe);
      set(override, { filled: true, value: next });
      discardOutcome(storage.setItem(key, next));
    },
    { debugLabel: label, equals: sameLoadable(options?.equals) },
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

/**
 * Every object this process has already walked.
 *
 * Module scope rather than per atom, and that is not a shortcut: freezing is
 * idempotent and global — an object frozen for one atom is frozen for every
 * other — so the only thing a per-atom set would buy is walking the same
 * structure again. It is weak, so nothing here keeps a value alive.
 */
const walked: WeakSet<interface {}> = new WeakSet();

/**
 * The same atom, with its value frozen so that nothing can change it in place.
 *
 * Jotai's `freezeAtom(anAtom)`. The bug it exists for is the first one anybody
 * meets: `state.items.push(row)` changes the value without replacing it, the
 * equality cutoff correctly sees no change, and nothing re-renders. Frozen,
 * that line throws in a module and fails silently outside one — either way it
 * is at the mutation rather than three components away.
 *
 * The freeze is deep, and the source's value is frozen with it. There is only
 * one object: this returns what it was given rather than a copy, so freezing
 * the derived atom freezes the atom it derives from too. That is the point —
 * a guard that only covered the copy would leave the mutation that matters
 * unguarded — and it is worth knowing before wrapping an atom somebody else
 * writes to.
 *
 * The cost is one walk of the structure the first time it is seen. A value
 * that shares most of its objects with the previous one — which is what
 * immutable updates produce — is walked only where it is new.
 *
 * What it cannot guard, said here rather than discovered: `Object.freeze` is
 * about properties, so a `Map` or a `Set` in the value is frozen as an object
 * and still accepts `set` and `add`. That is `Object.freeze`'s limit rather
 * than this function's, and it is the same limit Jotai's `freezeAtom` has;
 * state built out of plain objects and arrays is fully covered.
 */
export function freezeAtom<T>(source: AtomRecord<T, empty>): AtomRecord<T, empty> {
  return defineSelector((get) => {
    const value = get(source);
    freezeDeeply(value);
    return value;
  }, null);
}

function freezeDeeply(value: mixed): void {
  if (value === null || typeof value !== "object" || walked.has(value)) {
    return;
  }
  // Added before the walk rather than after it, so a structure that refers to
  // itself — a tree with parent links, a graph — terminates.
  walked.add(value);
  Object.freeze(value);
  // `Object.values` covers an array's elements as well as an object's
  // properties, so there is no second branch for one.
  for (const entry of Object.values(value)) {
    freezeDeeply(entry);
  }
}
