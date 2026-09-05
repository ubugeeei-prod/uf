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

import type { Cell, Unsubscribe } from "@uniflowed/cell";
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
import type { Reset, StorageAdapter } from "./internal/composed.js";
import {
  atomWithDefault as composeWithDefault,
  atomWithStorage as composeWithStorage,
  unwrap as composeUnwrap,
} from "./internal/composed.js";
import type { StoreInstance } from "./internal/store.js";
import { bindCell, createStore as createStoreInstance, defaultStore } from "./internal/store.js";
import { StoreScope, useStoreInstance } from "./internal/provider.js";

export type { AtomFamily, Reset, StorageAdapter } from "./internal/composed.js";
export type {
  AtomMount,
  AtomOptions,
  Loadable,
  PrimitiveOptions,
  SetAction,
} from "./internal/atom.js";
export type { Cell, Unsubscribe };

export { atomFamily, RESET } from "./internal/composed.js";
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
 */
export function asyncAtom<T>(
  load: (get: Getter) => Promise<T>,
  options?: AtomOptions<Loadable<T>>,
): ReadonlyAtom<Loadable<T>> {
  return defineAsync(load, options);
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

/** An atom mirrored into a key-value store on every write. */
export function atomWithStorage<T>(
  key: string,
  initial: T,
  storage?: StorageAdapter,
  options?: AtomOptions<T>,
): Atom<T> {
  return composeWithStorage(key, initial, storage, options);
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
