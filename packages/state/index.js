// @flow
//
// `@uniflowed/state`: atoms, stores, and the React binding over them.
//
// # Where the line between this package and `@uniflowed/cell` is
//
// `@uniflowed/cell` is the reactive core: dependency tracking, memoisation,
// the equality cutoff, batching, glitch-free propagation. It has no opinion
// about React and no concept of a store.
//
// This package adds two things and nothing else:
//
// * an atom is a *definition* rather than a value, so the same declaration can
//   have a different value in every store — which is what makes one process
//   able to render two requests, and one page able to hold a draft of itself;
// * React, through `useSyncExternalStore`.
//
// There is no second dependency graph here. A derived atom is instantiated
// into a `computed` cell whose read is bound to a store, and every question
// about *when* it recomputes is answered one layer down. Two implementations
// of dependency tracking in one product is one too many, and the one that
// would rot is the copy.
//
// # The file map
//
// * `internal/atom.js` — what an atom is before any store exists.
// * `internal/store.js` — where an atom's value lives, and how React reaches
//   it.
// * `internal/composed.js` — atoms assembled out of other atoms: families,
//   defaults, persistence.
// * `internal/provider.js` — which store a React subtree uses.
//
// This entry point is the vocabulary: the types an application writes down,
// the constructors it calls, and the four hooks. Every one of them is thin,
// because the machinery is behind them rather than in them.
//
// # Why the constructors are named rather than overloaded
//
// Jotai spells all of this `atom(...)` and decides what was meant from the
// arguments. That works in TypeScript, where the overload set is resolved at
// the call site; in Flow the honest version is an intersection of function
// types that the checker resolves poorly, and inference through composition is
// a stated requirement here. It is also ambiguous at runtime: `atom(f)` cannot
// distinguish a derived atom from a primitive one holding a function.
//
// So the four kinds are four names — `atom`, `selector`, `writableSelector`,
// `action` — and each returns exactly one type. In Jotai's terms they are
// `atom(value)`, `atom(read)`, `atom(read, write)` and `atom(null, write)`.
//
// # Why `useSyncExternalStore`
//
// A store is an external store, so the binding is the hook React provides for
// exactly that rather than a `useState` mirror kept in step by an effect. The
// difference is not stylistic: a mirror is written during commit, so a
// concurrent render can read one component's copy before another's has caught
// up, and the two disagree on screen. That is tearing, and this hook is the
// API that exists to prevent it.
//
// Nothing here mutates anything a render can observe, no hook returns a live
// mutable object, and the callbacks handed to React have an identity that does
// not change — see `internal/store.js` for why each of those matters.
//
// # What is deliberately not here
//
// `jotai/utils` is fifteen names and this package answers most of them. The
// ones it does not are listed here rather than left to be discovered, because
// "we thought about it and decided against" is worth more to somebody porting
// an application than silence is. ubugeeei-prod/uf#292 is the triage in full.
//
// * `atomWithLazy` — `atomWithDefault` already takes exactly that function.
//   A second name for one thing is worse than one name for it.
// * `useHydrateAtoms` — hydrates the store the component is already in, on
//   first render. That is a write to shared state during render, and
//   `internal/store.js` sets out the conditions under which this package
//   makes its one render-time write; this would meet none of them. The same
//   job outside React is `createStore()`, `write(...)`, `<Provider store>`,
//   which is one line longer and writes nothing during a render.
// * `atomFamily`'s `areEqual` — it turns an `O(1)` `Map` lookup into a linear
//   scan of every key the family has ever seen, on a call that happens once
//   per row per render. A key that is a tuple should be made a string;
//   `family(\`${year}:${month}\`)` costs nothing and says what it costs.
// * `loadable` — deprecated upstream in favour of `unwrap`, which is here,
//   and `asyncAtom` produces the `Loadable` shape directly rather than
//   needing a wrapper to put it there.
// * `atomWithRefresh` — `refresh(target, store?)` is the same capability as a
//   free function; the doc comment on it says why that shape was chosen.
// * `splitAtom` — an atom of one atom per row, each keeping its identity
//   across an insert, a remove and a move. Not a stub and not a few lines:
//   the row atoms have to be memoised somewhere, and a `selector`'s read is
//   handed a `get` and nothing else, so there is no store to memoise them
//   against. Every version of it either invents a per-store cache the read
//   cannot reach or shares row atoms between stores and then has to answer
//   what a row means in a store whose list does not contain it. That is a
//   change to the store's model, and it does not belong inside a utility.
// * `atomWithObservable` — an atom fed by an `Observable` that may itself
//   depend on other atoms. The half that does not is already `atom(initial,
//   { onMount })`. The half that does needs a mount that re-runs when a
//   dependency changes, and `onMount` is a subscription lifecycle: it runs
//   for the first subscriber and stops for the last, and never in between.
//   Offering it on a derived atom — which `internal/store.js` would already
//   honour — would hand callers a `get` whose changes their subscription
//   never hears about, which is worse than not offering it.

import type { Cell, LoadContext, Unsubscribe } from "@uniflowed/cell";
import * as React from "@uniflowed/react";
import { useSyncExternalStore } from "@uniflowed/react";

import type {
  AtomOptions,
  AtomRecord,
  Loadable,
  PrimitiveOptions,
  SetAction,
} from "./internal/atom.js";
import { defineAction, defineAsync, definePrimitive, defineSelector } from "./internal/atom.js";
import type {
  AsyncSetAction,
  AsyncStorageAdapter,
  AsyncStorageOptions,
  Reset,
  StorageAdapter,
  StorageOptions,
} from "./internal/composed.js";
import {
  RESET,
  atomWithAsyncStorage as composeWithAsyncStorage,
  atomWithDefault as composeWithDefault,
  atomWithReducer as composeWithReducer,
  atomWithReset as composeWithReset,
  atomWithStorage as composeWithStorage,
  freezeAtom as composeFreeze,
  selectAtom as composeSelect,
  unwrap as composeUnwrap,
} from "./internal/composed.js";
import type { StoreInstance } from "./internal/store.js";
import { bindCell, createStore as createStoreInstance, defaultStore } from "./internal/store.js";
import { StoreScope, useStoreInstance } from "./internal/provider.js";

export type {
  AsyncSetAction,
  AsyncStorageAdapter,
  AsyncStorageOptions,
  AtomFamily,
  JSONStorageOptions,
  Reset,
  StorageAdapter,
  StorageOptions,
  StringStorage,
} from "./internal/composed.js";
export type {
  AtomMount,
  AtomOptions,
  Loadable,
  PrimitiveOptions,
  SetAction,
} from "./internal/atom.js";
export type { Cell, LoadContext, Unsubscribe };

export { atomFamily, createJSONStorage, RESET } from "./internal/composed.js";
export { batch } from "@uniflowed/cell";

/**
 * Any atom, as something to read.
 *
 * Every other atom type is a subtype of this one, so a function that only
 * reads takes a `ReadonlyAtom<T>` and accepts all of them.
 */
export opaque type ReadonlyAtom<T> = AtomRecord<T, empty>;

/**
 * An atom that can be written with an argument of type `A`.
 *
 * `A` is not always the value: an atom holding a list may take an "add this
 * one" argument, and keeping the two apart is what lets the setter a component
 * is handed be typed exactly.
 */
export opaque type WritableAtom<T, A>: ReadonlyAtom<T> = AtomRecord<T, A>;

/**
 * A piece of state: readable, and writable the way `useState` is.
 *
 * The `useState`-shaped argument — a value, or a function of the current one —
 * is the reason this is a type of its own rather than
 * `WritableAtom<T, T>`: `setCount((n) => n + 1)` has to mean what it does
 * everywhere else in React.
 */
export opaque type Atom<T>: WritableAtom<T, SetAction<T>> = AtomRecord<T, SetAction<T>>;

/** An atom that is only ever written: an action. */
export opaque type WriteOnlyAtom<A>: WritableAtom<null, A> = AtomRecord<null, A>;

/**
 * An atom whose value arrives from a promise: what [`asyncAtom`] returns.
 *
 * Nominally distinct from `ReadonlyAtom<Loadable<T>>` even though the two are
 * the same record, and the distinction earns its place at exactly one call
 * site: [`refresh`] can only mean something for an atom that has a load to
 * run, and a `selector` that happens to return a `Loadable` — a cache lookup
 * projected into one, say — has nothing to refresh. Without the name that
 * mistake is a runtime error; with it, Flow says so at the call site.
 */
export opaque type AsyncAtom<T>: ReadonlyAtom<Loadable<T>> = AtomRecord<Loadable<T>, empty>;

/** Reading another atom, inside a read or a write. */
export type Getter = <V>(target: ReadonlyAtom<V>) => V;

/** Writing another atom, inside a write. */
export type Setter = <V, A>(target: WritableAtom<V, A>, argument: A) => void;

/** What `useSetAtom` hands back for a `useState`-shaped atom. */
export type AtomSetter<T> = (next: SetAction<T>) => void;

/** What `useAtom` hands back: the shape `useState` returns. */
export type AtomTuple<T> = [T, AtomSetter<T>];

/**
 * Where atom values live.
 *
 * Opaque, and deliberately small: `get`, `set` and `sub` are everything an
 * application needs, and everything a test needs to assert on a tree's state
 * without rendering one.
 */
export opaque type Store = StoreInstance;

/**
 * A piece of state.
 *
 * Declaring one allocates nothing and belongs to no store — the value appears
 * the first time a store is asked for it. That is what makes it safe to
 * declare atoms at module scope in a file a server imports.
 *
 * `onMount` runs when the first subscriber in a store arrives and its return
 * value runs when the last one leaves, which is where a subscription to
 * anything outside the graph belongs: a socket, an interval, a media query.
 */
export function atom<T>(initial: T, options?: PrimitiveOptions<T>): Atom<T> {
  return definePrimitive(initial, options);
}

/**
 * State derived from other atoms, recomputed only when what it read changes.
 *
 * No dependency array: the read discovers its own dependencies by running, and
 * they are rebuilt every time it runs. A read that branches —
 * `get(showAll) ? get(all) : get(some)` — depends on the branch it took, so
 * writing to the other one recomputes nothing.
 *
 * A read that returns an unchanged value does not re-render its readers, and
 * `options.equals` is how a read that builds a fresh array each time says what
 * "unchanged" means for it.
 */
export function selector<T>(read: (get: Getter) => T, options?: AtomOptions<T>): ReadonlyAtom<T> {
  return defineSelector(read, null, options);
}

/**
 * A selector you can also write to.
 *
 * The write is given `get` and `set`, so it can decide what a change to this
 * atom means in terms of the atoms it is derived from — a "full name" atom
 * whose write splits into first and last, a filter atom that also resets the
 * page number. Everything it sets happens in one batch, so subscribers to
 * three of those atoms are woken once each rather than once per `set`.
 *
 * `get` inside a write does not create a dependency. A write is not a
 * computation, and an atom that recomputed because a handler looked at
 * something would be very hard to explain.
 */
export function writableSelector<T, A>(
  read: (get: Getter) => T,
  write: (get: Getter, set: Setter, argument: A) => void,
  options?: AtomOptions<T>,
): WritableAtom<T, A> {
  return defineSelector(read, write, options);
}

/**
 * An atom that is only written: a named operation over a store.
 *
 * A component that dispatches one does not subscribe to anything, so it does
 * not re-render when the state the action changes changes. That is the whole
 * point of having it as an atom rather than a function: it is written where
 * the state is, it can be replaced in a test by providing a different store,
 * and dispatching it costs the caller no subscription.
 */
export function action<A>(
  write: (get: Getter, set: Setter, argument: A) => void,
  options?: AtomOptions<null>,
): WriteOnlyAtom<A> {
  return defineAction(write, options);
}

/**
 * A derived atom whose read is asynchronous.
 *
 * Its value is a [`Loadable`] — `loading`, `hasData` or `hasError` — rather
 * than a promise a component suspends on. Suspense is not available to a
 * `useSyncExternalStore` reader without throwing a promise from inside a
 * snapshot, which is neither supported nor safe under concurrent rendering, so
 * this package makes the loading state a value the caller renders rather than
 * a control-flow trick. `unwrap` is there for callers that just want a
 * fallback.
 *
 * The load is tracked: `asyncAtom((get) => fetchUser(get(userId)))` reloads
 * when `userId` changes, and — this is the part that is hard to get right by
 * hand — the load already in flight for the previous id is discarded rather
 * than allowed to win a race and deliver the wrong user.
 *
 * Discarded, and also stopped. The load's second argument carries an
 * `AbortSignal`, aborted when a newer load supersedes this one, when
 * [`refresh`] asks for another, and when the atom loses its last subscriber:
 *
 * ```
 * const user = asyncAtom((get, { signal }) =>
 *   fetch(`/users/${get(userId)}`, { signal }).then((response) => response.json()),
 * );
 * ```
 *
 * A load that ignores the signal is still correct — whether a result is
 * adopted is decided by the store either way — but on a search box that
 * reloads per keystroke, ignoring it is one live request per keystroke.
 *
 * An aborted load's rejection is not the atom's error: it never becomes
 * `{ state: "hasError" }`, because it is the answer to a question the atom
 * stopped asking.
 */
export function asyncAtom<T>(
  load: (get: Getter, context: LoadContext) => Promise<T>,
  options?: AtomOptions<Loadable<T>>,
): AsyncAtom<T> {
  return defineAsync(load, options);
}

/**
 * Load an asynchronous atom again, with the dependencies it already has.
 *
 * The ordinary case after a mutation, and behind the Retry button on the error
 * state [`Loadable`] exists to make renderable. An `asyncAtom` otherwise
 * reloads only when something it read changes, and writing a dependency the
 * value it already holds is correctly dropped by the equality cutoff — so
 * without this there is no way to say "ask again" at all.
 *
 * A free function rather than a write, which is the choice Jotai makes with
 * `atomWithRefresh`. Two reasons, and the second is the one that decided it:
 * `WritableAtom<T, A>` promises that `A` is the argument type, and an atom
 * that is suddenly writable with no argument muddies that; and a free function
 * works from a route handler, an event handler and a test, none of which have
 * a component to hold a setter.
 *
 * The atom passes through `{ state: "loading" }` on the way, so a list that
 * shows a spinner while it refetches gets one without asking.
 */
export function refresh<T>(target: AsyncAtom<T>, store?: Store): void {
  (store ?? defaultStore()).reload(target);
}

/**
 * An atom whose value is computed until someone writes one, and again after
 * [`RESET`].
 */
export function atomWithDefault<T>(
  getDefault: (get: Getter) => T,
  options?: AtomOptions<T>,
): WritableAtom<T, SetAction<T> | Reset> {
  return composeWithDefault(getDefault, options);
}

/**
 * A piece of state that also answers to [`RESET`].
 *
 * `atom(0)` cannot be reset — its argument type is `SetAction<number>` and
 * nothing else — so this is what an application reaches for when "back to the
 * default" is a thing the interface offers. It is [`atomWithDefault`] taking a
 * value instead of a read, which is what a caller who has one already has.
 */
export function atomWithReset<T>(
  initial: T,
  options?: AtomOptions<T>,
): WritableAtom<T, SetAction<T> | Reset> {
  return composeWithReset(initial, options);
}

/**
 * State and the actions that change it: `useReducer`, as an atom.
 *
 * ```
 * const count = atomWithReducer<number, "increment" | "reset">(0, (n, action) =>
 *   action === "increment" ? n + 1 : 0,
 * );
 * ```
 *
 * The value and the argument are different types, which is the reason to use
 * this rather than an [`atom`]: `useSetAtom(count)` hands a component a
 * dispatch that takes an action, so a state written to it by mistake is a
 * type error rather than a state machine with a hole in it.
 */
export function atomWithReducer<State, Action>(
  initial: State,
  reduce: (state: State, action: Action) => State,
  options?: AtomOptions<State>,
): WritableAtom<State, Action> {
  return composeWithReducer(initial, reduce, options);
}

/**
 * One part of another atom, so that a reader of the part is woken only when
 * the part changes.
 *
 * ```
 * const name = selectAtom(user, (current) => current.name);
 * ```
 *
 * A component reading that re-renders when the name changes and not when
 * anything else about the user does. `equals` is for a selection that builds a
 * fresh value each time — an array of ids, a filtered list — which would
 * otherwise be a new value on every recompute and wake every reader.
 *
 * Jotai's selector also takes the previous slice. This one does not: a read
 * that can see its own output is not a pure function of its dependencies,
 * which is what the equality cutoff underneath rests on, and `equals` is the
 * direct way to say the thing that parameter was used to say.
 */
export function selectAtom<T, Slice>(
  source: ReadonlyAtom<T>,
  select: (value: T) => Slice,
  equals?: (previous: Slice, next: Slice) => boolean,
): ReadonlyAtom<Slice> {
  return composeSelect(source, select, equals);
}

/**
 * The same atom, deeply frozen, so a mutation in place fails where it happens.
 *
 * `state.items.push(row)` does not replace the value, so the equality cutoff
 * correctly reports no change and nothing re-renders — the most expensive
 * beginner bug in this style of state, because the symptom appears in a
 * component that is not the one at fault. Frozen, the push throws in a module
 * and does nothing outside one, and either way it is at the line that did it.
 *
 * There is one object, not two: the value handed back is the value that came
 * in, so the atom this derives from is frozen along with it. That is what
 * makes the guard worth anything, and it is worth knowing before wrapping an
 * atom other code writes to.
 */
export function freezeAtom<T>(source: ReadonlyAtom<T>): ReadonlyAtom<T> {
  return composeFreeze(source);
}

/**
 * An atom mirrored into a key-value store on every write, and read back from
 * it when a store mounts it.
 *
 * ```
 * const theme = atomWithStorage("theme", "light", createJSONStorage(() => localStorage));
 * ```
 *
 * The read happens on mount rather than when the atom is declared, and that is
 * the whole difference between a persisted preference that survives hydration
 * and one that does not: a server renders `initial`, so the first client
 * render has to be `initial` too, and the stored value arrives immediately
 * after commit. `options.getOnInit` is for an application with no server
 * render to agree with.
 *
 * Writing [`RESET`] removes the key rather than storing the initial value,
 * which is the difference between "back to the default" and "persisting the
 * default forever".
 */
export function atomWithStorage<T>(
  key: string,
  initial: T,
  storage?: StorageAdapter<T>,
  options?: StorageOptions<T>,
): WritableAtom<T, SetAction<T> | Reset> {
  return composeWithStorage(key, initial, storage, options);
}

/**
 * An atom persisted to a storage whose read is asynchronous — IndexedDB, a
 * React Native `AsyncStorage`, a preference store behind a request.
 *
 * ```
 * const draft = atomWithAsyncStorage("draft", "", indexedDb);
 * // { state: "loading" }, and then { state: "hasData", data: "…" }
 * ```
 *
 * A second constructor rather than an option on [`atomWithStorage`], because
 * the value is a different type. It is a [`Loadable`] until the first read
 * settles, for the reason [`asyncAtom`] gives: a `useSyncExternalStore` reader
 * cannot suspend, so a value that has not arrived is a state to render rather
 * than a promise to throw. Jotai's `atomWithStorage` takes an async storage
 * and makes the value `T | Promise<T>`; taking that signature without Suspense
 * would leave the type promising a `T` that is not there.
 *
 * The setter takes a `T`, so the two type parameters differ. Its reducer form
 * is handed the `Loadable`, not the value, because a write can happen before
 * the first read has settled and there may be nothing to reduce.
 *
 * Three behaviours worth knowing before reaching for it, each of them a
 * decision rather than a consequence:
 *
 * * a write while the first read is in flight wins, and the read is abandoned
 *   and its signal aborted — the load stops being a dependency, and the cell
 *   underneath decides the rest;
 * * a read that *fails* is `{ state: "hasError" }` rather than `initial`,
 *   which is the whole reason this constructor can exist and the synchronous
 *   one cannot do it: "the database is locked" is not "nobody set a
 *   preference". An absent key is still `initial`;
 * * a *write* that fails is silent. The value the caller wrote is the atom's
 *   value regardless; it simply will not outlive the session.
 *
 * [`RESET`] removes the key and puts the atom back to `initial` without
 * reading again — a re-read would race the removal it has not waited for.
 */
export function atomWithAsyncStorage<T>(
  key: string,
  initial: T,
  storage: AsyncStorageAdapter<T>,
  options?: AsyncStorageOptions<T>,
): WritableAtom<Loadable<T>, AsyncSetAction<T> | Reset> {
  return composeWithAsyncStorage(key, initial, storage, options);
}

/** The data an asynchronous atom is holding, or `fallback` until it has some. */
export function unwrap<T>(target: ReadonlyAtom<Loadable<T>>, fallback: T): ReadonlyAtom<T> {
  return composeUnwrap(target, fallback);
}

/**
 * A store of your own.
 *
 * One per request on a server, one per test that wants a clean slate, one per
 * subtree that needs to disagree with the page around it.
 */
export function createStore(): Store {
  return createStoreInstance();
}

/** The store everything that does not name one uses. */
export function getDefaultStore(): Store {
  return defaultStore();
}

/**
 * Read an atom out of a store, or out of the default store.
 *
 * The same value a component would see, with no component: this is how a route
 * handler, an event handler outside React, or a test reads state.
 */
export function read<T>(target: ReadonlyAtom<T>, store?: Store): T {
  return (store ?? defaultStore()).get(target);
}

/** Write an atom in a store, or in the default store. */
export function write<T, A>(target: WritableAtom<T, A>, argument: A, store?: Store): void {
  (store ?? defaultStore()).set(target, argument);
}

/**
 * Be told when an atom's value changes, outside React.
 *
 * Subscribing is also what mounts the atom, so an atom with an `onMount` is
 * started by the first subscriber and stopped by the last — including when
 * that subscriber is a component.
 */
export function subscribe<T>(
  target: ReadonlyAtom<T>,
  listener: () => void,
  store?: Store,
): Unsubscribe {
  return (store ?? defaultStore()).sub(target, listener);
}

/**
 * Give a subtree its own store.
 *
 * With no `store`, the provider owns one it creates for itself, which is the
 * shortest way to isolate a subtree — or a test — from everything else.
 */
export component Provider(store?: Store, children: React.Node) {
  return <StoreScope store={store ?? null}>{children}</StoreScope>;
}

/** The store this part of the tree reads: the scoped one, or the default. */
export hook useStore(store?: Store): Store {
  return useStoreInstance(store);
}

/**
 * Read an atom, re-rendering when — and only when — its value changes.
 *
 * A component that reads three atoms re-renders when any of the three changes
 * and not when a fourth does, because the subscription is per atom rather than
 * per store. A derived atom that recomputes to the value it already had does
 * not re-render its readers at all.
 */
export hook useAtomValue<T>(target: ReadonlyAtom<T>, store?: Store): T {
  const bound = useStoreInstance(store).bind(target);
  // The server snapshot is the same read: a store holds its value on both
  // sides, so hydration compares like with like instead of a placeholder.
  return useSyncExternalStore(bound.subscribe, bound.snapshot, bound.snapshot);
}

/**
 * Write an atom without reading it.
 *
 * A component that only dispatches does not subscribe, so it does not
 * re-render when the value it writes changes. The setter's identity is stable
 * for as long as the store and the atom are, so passing it to a memoised child
 * costs that child nothing.
 */
export hook useSetAtom<T, A>(target: WritableAtom<T, A>, store?: Store): (argument: A) => void {
  return useStoreInstance(store).bind(target).setter;
}

/** Read and write an atom, in the shape `useState` returns. */
export hook useAtom<T, A>(target: WritableAtom<T, A>, store?: Store): [T, (argument: A) => void] {
  return [useAtomValue(target, store), useSetAtom(target, store)];
}

/**
 * Put a resettable atom back, from a component.
 *
 * Jotai's `useResetAtom`. `useSetAtom(target)` already does this — the
 * argument is [`RESET`] — and the difference is the shape of what comes back:
 * `() => void` goes straight onto an `onClick`, where the setter needs a
 * wrapper that supplies the symbol.
 *
 * It takes an atom whose write accepts `RESET` as well as a value, which is
 * what [`atomWithReset`], [`atomWithDefault`] and [`atomWithStorage`] all
 * return. A plain [`atom`] is refused, and refused at the call rather than
 * with a runtime error, because its argument type does not include the symbol.
 * [`atomWithAsyncStorage`]'s setter has a different value type, so it resets
 * through `useSetAtom` — its own doc comment says so.
 */
export hook useResetAtom<T>(
  target: WritableAtom<T, SetAction<T> | Reset>,
  store?: Store,
): () => void {
  const setter = useSetAtom<T, SetAction<T> | Reset>(target, store);
  return React.useCallback(() => {
    setter(RESET);
  }, [setter]);
}

/**
 * A callback that can read and write any atom, and subscribes to none of them.
 *
 * Jotai's `useAtomCallback`. For the handler that needs the *current* value of
 * something it does not render — a submit that reads a draft, an analytics
 * call that reads a filter — where `useAtomValue` would re-render the
 * component every time that value changed for a value it only ever looks at
 * once.
 *
 * The `get` and `set` are the same pair a [`writableSelector`]'s write is
 * given, resolved through the store this component is in. Reading through it
 * records no dependency, because a handler runs outside any computation and
 * there is nothing for one to be recorded against — which is the point: the
 * component that holds this callback is subscribed to nothing.
 *
 * The identity of what comes back changes when `callback` does, so a handler
 * written inline is a new function every render. Wrap it in `useCallback` when
 * that matters — the same rule as every other hook that takes a function.
 */
export hook useAtomCallback<Args extends $ReadOnlyArray<mixed>, Result>(
  callback: (get: Getter, set: Setter, ...args: Args) => Result,
  store?: Store,
): (...args: Args) => Result {
  const instance = useStoreInstance(store);
  return React.useCallback(
    (...args: Args) => callback(instance.get, instance.set, ...args),
    [callback, instance],
  );
}

/**
 * Subscribe a component to a cell directly.
 *
 * The escape hatch to the layer below: a route loader hands out
 * `@uniflowed/cell` cells, and a component that reads one should not have to
 * wrap it in an atom to do so. A cell holds its own value, so no store is
 * involved and the `store` argument the other hooks take would mean nothing.
 */
export hook useCell<T>(source: Cell<T>): T {
  const bound = bindCell(source);
  return useSyncExternalStore(bound.subscribe, bound.snapshot, bound.snapshot);
}
