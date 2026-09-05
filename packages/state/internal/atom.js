// @flow
//
// What an atom is before any store exists.
//
// An atom is a *definition*, not a value. `atom(0)` allocates no state,
// belongs to nothing, and can be declared at module scope in a file that a
// server imports. The value lives in a store, and the same definition has a
// different value in every store that ever instantiates it.
//
// # Why definitions and values are separated
//
// The alternative — an atom that holds its own value, which is what this
// package used to ship — is simpler right up to the point where two of
// something must exist at once, and then it is unfixable:
//
// * a server renders two requests concurrently in one process, and module
//   state is shared between them, so one user's cart is the other user's cart;
// * a test writes an atom and the next test in the same file inherits it;
// * a component tree wants its own copy — a preview pane, a modal editing a
//   draft of what the page behind it shows — and there is no copy to have.
//
// A store is the unit of isolation those three want, and `<Provider>` is how a
// React subtree picks one. This is also Jotai's model, for the same reasons.
//
// # Why the record carries functions rather than a class
//
// A definition is a plain frozen-shaped record of the two functions a store
// needs — how to read it, and what happens when it is written — so the store
// is a `WeakMap` lookup plus a call, and an atom that is never read costs one
// object. There is no base class to extend and no registry holding a reference
// to every atom ever declared, which is what lets an `atomFamily` member be
// collected once the key is gone.

import type { Unsubscribe } from "@uniflowed/cell";

/** What a setter accepts: a value, or a reducer over the current one. */
export type SetAction<T> = T | ((current: T) => T);

/**
 * The three states an asynchronous read can be in.
 *
 * A discriminated union rather than `{ loading, data, error }` with three
 * optional fields, because two of those eight combinations are nonsense and
 * `match` can prove this one is exhaustive.
 */
export type Loadable<T> =
  | { readonly state: "loading" }
  | { readonly state: "hasData", readonly data: T }
  | { readonly state: "hasError", readonly error: mixed };

/**
 * What an atom's `onMount` is handed: the atom, bound to the store it was
 * mounted in.
 *
 * `set` goes through the store's ordinary write path, so an atom that mounts a
 * subscription feeds values in exactly the way an event handler would.
 */
export type AtomMount<T> = {
  readonly get: () => T,
  readonly set: (value: T) => void,
  readonly subscribe: (listener: () => void) => Unsubscribe,
};

/**
 * Reading another atom, from inside a read or a write.
 *
 * Inside a read this is also what records the dependency — which is why a read
 * that branches depends on the branch it took and not on the other one.
 */
export type AtomGetter = <V>(target: AtomRecord<V, empty>) => V;

/** Writing another atom, from inside a write. */
export type AtomSetter = <V, A>(target: AtomRecord<V, A>, arg: A) => void;

/**
 * An atom, as the store sees it.
 *
 * Every field is read-only, which is what makes `AtomRecord<T, A>` a subtype
 * of `AtomRecord<T, empty>`: a writable atom is accepted anywhere a readable
 * one is, and the argument type stays exact at the call site rather than
 * widening to `mixed` the moment an atom is passed somewhere general.
 */
export type AtomRecord<T, A> = {
  readonly kind: "primitive" | "derived" | "async",
  /** For diagnostics only. Never load-bearing. */
  readonly label: string,
  /** The value before anything is computed: a primitive's initial value, an
   * async atom's `loading`, a write-only atom's `null`. */
  readonly initial: T,
  readonly read: null | ((get: AtomGetter) => T),
  readonly load: null | ((get: AtomGetter) => Promise<T>),
  readonly write: null | ((get: AtomGetter, set: AtomSetter, arg: A) => void),
  readonly equals: null | ((previous: T, next: T) => boolean),
  readonly onMount: null | ((mount: AtomMount<T>) => void | (() => void)),
};

/** What every constructor accepts. */
export type AtomOptions<T> = {
  readonly debugLabel?: string,
  readonly equals?: (previous: T, next: T) => boolean,
};

/** What a primitive atom accepts, which is a mount as well. */
export type PrimitiveOptions<T> = {
  readonly debugLabel?: string,
  readonly equals?: (previous: T, next: T) => boolean,
  readonly onMount?: (mount: AtomMount<T>) => void | (() => void),
};

/**
 * The fields every kind of atom shares, filled in from whichever options that
 * kind accepts.
 *
 * The options parameter is inexact on purpose: a primitive's options carry an
 * `onMount` that the other kinds do not have, and this is the one place all of
 * them meet.
 */
function baseRecord<T, A>(
  kind: "primitive" | "derived" | "async",
  label: string,
  initial: T,
  options: void | {
    readonly debugLabel?: string,
    readonly equals?: (previous: T, next: T) => boolean,
    ...
  },
): AtomRecord<T, A> {
  return {
    kind,
    label: options?.debugLabel ?? label,
    initial,
    read: null,
    load: null,
    write: null,
    equals: options?.equals ?? null,
    onMount: null,
  };
}

/** A value a store holds directly. */
export function definePrimitive<T>(
  initial: T,
  options?: PrimitiveOptions<T>,
): AtomRecord<T, SetAction<T>> {
  return {
    ...baseRecord("primitive", "atom", initial, options),
    onMount: options?.onMount ?? null,
  };
}

/**
 * A value computed from other atoms, with an optional write of its own.
 *
 * The record's `initial` is never read for one of these — a selector's value
 * comes from running `read` — but the field exists for the kinds that do have
 * one, so the placeholder is cast here, once, rather than at every call site.
 */
export function defineSelector<T, A>(
  read: (get: AtomGetter) => T,
  write: null | ((get: AtomGetter, set: AtomSetter, argument: A) => void),
  options?: AtomOptions<T>,
): AtomRecord<T, A> {
  const unevaluated: T = null as $FlowFixMe;
  return { ...baseRecord("derived", "selector", unevaluated, options), read, write };
}

/**
 * A write with no value: an action.
 *
 * Its value really is `null` rather than a placeholder, so reading one is
 * defined behaviour, and a component that only dispatches never subscribes to
 * anything.
 */
export function defineAction<A>(
  write: (get: AtomGetter, set: AtomSetter, argument: A) => void,
  options?: AtomOptions<null>,
): AtomRecord<null, A> {
  return { ...baseRecord("derived", "action", null, options), write };
}

/**
 * A value that arrives from a promise, projected into a [`Loadable`].
 *
 * The rejection is folded into the value here, at definition time, rather than
 * left for the store: a promise that rejects with nobody attached is an
 * unhandled rejection, and the store attaches its handler one turn later than
 * this does.
 */
export function defineAsync<T>(
  load: (get: AtomGetter) => Promise<T>,
  options?: AtomOptions<Loadable<T>>,
): AtomRecord<Loadable<T>, empty> {
  const pending: Loadable<T> = { state: "loading" };
  return {
    ...baseRecord("async", "asyncAtom", pending, options),
    load: (get) => load(get).then(asData, asError),
  };
}

function asData<T>(data: T): Loadable<T> {
  return { state: "hasData", data };
}

function asError<T>(error: mixed): Loadable<T> {
  return { state: "hasError", error };
}
