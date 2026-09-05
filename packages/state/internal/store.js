// @flow
//
// A store: where an atom's value actually lives.
//
// One `WeakMap` from an atom definition to the cell holding its value in this
// store, and the four operations everything else in the package is built from
// — get, set, subscribe, and the React binding. There is no graph here. A
// derived atom's read is handed a `get` that resolves through this store, and
// the cell it is instantiated into does the tracking, the memoisation, the
// glitch-free propagation and the batching. That is the whole reason
// `@uniflowed/cell` is a separate package: two implementations of dependency
// tracking in one product is one too many.
//
// # Why a WeakMap
//
// An `atomFamily` can be keyed by anything — a row id, a date, a filter — and
// a long-lived store must not accumulate a cell per key ever asked for. Keying
// weakly means a family member that nothing references any more takes its
// value with it. It also means the store never has to be told an atom exists:
// instantiation happens on first contact, so importing a module full of atoms
// costs nothing until one is read.
//
// # Why instantiation during a React render is safe
//
// `useAtomValue` reads through `getSnapshot`, which React calls during render,
// and that read may create the cell. It is a mutation, and it is the one kind
// React permits: it is idempotent, keyed by an identity the caller already
// holds, and unobservable — two renders that race produce the same cell, and
// the second finds the first's. Nothing outside the store can tell whether the
// cell existed before the render.
//
// What is *not* done during render is mounting: `onMount` runs from
// `subscribe`, which React calls after commit. A render that is thrown away
// therefore starts nothing that would need stopping.

import type { Cell, CellOptions, Unsubscribe } from "@uniflowed/cell";
import {
  batch,
  cell,
  computed,
  peek,
  read,
  resource,
  status,
  subscribe,
  untracked,
  update,
  write,
} from "@uniflowed/cell";

import type { AtomGetter, AtomRecord, AtomSetter, Loadable } from "./atom.js";

/**
 * The three callbacks React needs for one atom, cached so their identity never
 * changes.
 *
 * `useSyncExternalStore` re-subscribes whenever the identity of `subscribe`
 * changes, and a memoised child re-renders whenever the identity of a setter
 * changes. A `useCallback` would keep them stable within one component;
 * caching them on the store keeps them stable across every component reading
 * that atom, so mounting the thousandth reader allocates nothing.
 */
export type Binding<T, A> = {
  readonly subscribe: (listener: () => void) => Unsubscribe,
  readonly snapshot: () => T,
  readonly setter: (arg: A) => void,
};

export type StoreInstance = {
  readonly get: AtomGetter,
  readonly set: AtomSetter,
  readonly sub: <V>(target: AtomRecord<V, empty>, listener: () => void) => Unsubscribe,
  readonly bind: <V, A>(target: AtomRecord<V, A>) => Binding<V, A>,
};

/**
 * An atom, a cell or a binding whose types are not known here.
 *
 * A store's maps hold every atom in the application at once, and the lookup
 * does not care what any of them holds — only the call that comes back out
 * does, and those are generic. Flow has no existential to say "some `T`", and
 * the obvious substitute does not work: an atom's `equals` and `onMount` take
 * a `T`, so `AtomRecord<T, A>` is not an `AtomRecord<mixed, empty>`.
 *
 * These three aliases are the whole of it. Every function that reaches a value
 * is generic in its type, so nothing outside this file sees them.
 */
type AnyRecord = AtomRecord<any, any>;
type AnyCell = Cell<any>;
type AnyBinding = Binding<any, any>;

export function createStore(): StoreInstance {
  const cells: WeakMap<AnyRecord, AnyCell> = new WeakMap();
  const bindings: WeakMap<AnyRecord, AnyBinding> = new WeakMap();

  function cellFor<V, A>(target: AtomRecord<V, A>): Cell<V> {
    const existing = cells.get(target);
    if (existing != null) {
      return existing;
    }
    const created = instantiate<V, A>(target);
    cells.set(target, created);
    return created;
  }

  function instantiate<V, A>(target: AtomRecord<V, A>): Cell<V> {
    const options = cellOptions<V, A>(target);
    return match (target.kind) {
      "primitive" => cell(target.initial, options),
      "async" => loadable<V, A>(target, options),
      _ => derived<V, A>(target, options),
    };
  }

  function derived<V, A>(target: AtomRecord<V, A>, options: CellOptions<V>): Cell<V> {
    const reader = target.read;
    if (reader === null) {
      // A write-only atom. It still gets a cell, so that `useSetAtom` on one
      // works the same way as on any other atom, but its value is a constant
      // and nothing ever recomputes it.
      return cell(target.initial, options);
    }
    return computed(() => reader(get), options);
  }

  /**
   * An asynchronous atom: a resource that reloads when its inputs change,
   * projected into the [`Loadable`] the atom's readers see.
   *
   * The projection is a separate cell rather than logic inside the resource
   * because the two have different equality: the resource's value changes once
   * per settlement, while the loadable also has to change when the *status*
   * does — a reload that returns to `loading` is a render, even though the
   * data it holds has not changed yet.
   */
  function loadable<V, A>(target: AtomRecord<V, A>, options: CellOptions<V>): Cell<V> {
    const loader = target.load;
    if (loader === null) {
      return cell(target.initial, options);
    }
    const pending = resource(() => loader(get));
    return computed(() => {
      try {
        // Read first, and unconditionally: this is what makes the projection
        // depend on the resource. A load in flight reads as `null`, and a
        // reload that has not settled reads as the status rather than as the
        // value it still holds — a refetch is a loading state, not stale data
        // presented as current.
        const settled = read(pending);
        return status(pending) === "success" && settled != null ? settled : target.initial;
      } catch (error) {
        // Only a `load` that threw synchronously reaches here: a rejection is
        // folded into the value where the atom was defined.
        const failure: Loadable<mixed> = { state: "hasError", error };
        return failure as $FlowFixMe;
      }
    }, options);
  }

  function cellOptions<V, A>(target: AtomRecord<V, A>): CellOptions<V> {
    const equals = target.equals;
    const onMount = target.onMount;
    if (onMount === null) {
      return equals === null ? {} : { equals };
    }
    // The mount is handed the atom bound to *this* store, so a subscription it
    // starts feeds this store's value and no other's.
    const mounted = (self: Cell<V>) =>
      onMount({
        get: () => peek(self),
        set: (value: V) => {
          set(target, value as $FlowFixMe);
        },
        subscribe: (listener) => subscribe(self, listener),
      });
    return equals === null ? { onMount: mounted } : { equals, onMount: mounted };
  }

  const get: AtomGetter = (target) => read(cellFor(target));

  /**
   * Apply one write.
   *
   * A writable atom's own `write` runs untracked: a writer is not a derive,
   * and a `get` inside one that recorded a dependency would make the atom
   * recompute because of something a *handler* looked at.
   */
  function apply<V, A>(target: AtomRecord<V, A>, argument: A): void {
    const writer = target.write;
    if (writer !== null) {
      untracked(() => {
        writer(get, set, argument);
      });
      return;
    }
    if (target.kind !== "primitive") {
      throw Error(`@uniflowed/state ${target.label} is read-only`);
    }
    const node = cellFor<V, A>(target);
    // A function argument is a reducer, exactly as `useState` reads one — with
    // the same consequence, that an atom holding a function must be written
    // through a reducer returning it.
    if (typeof argument === "function") {
      update(node, argument as $FlowFixMe);
      return;
    }
    write(node, argument as $FlowFixMe);
  }

  /**
   * Every write is a batch, so an atom whose writer sets three others wakes
   * each subscriber once rather than three times, and no subscriber ever runs
   * against a store that is halfway through one logical change.
   */
  const set: AtomSetter = (target, argument) => {
    batch(() => {
      apply(target, argument);
    });
  };

  const sub = <V>(target: AtomRecord<V, empty>, listener: () => void): Unsubscribe =>
    subscribe(cellFor(target), listener);

  function bind<V, A>(target: AtomRecord<V, A>): Binding<V, A> {
    const existing = bindings.get(target);
    if (existing != null) {
      return existing;
    }
    const created: Binding<V, A> = {
      subscribe: (listener) => subscribe(cellFor(target), listener),
      // `peek`, not `read`: a snapshot taken during a React render must not
      // become a dependency of whatever happens to be evaluating.
      snapshot: () => peek(cellFor(target)),
      setter: (argument) => {
        set(target, argument);
      },
    };
    bindings.set(target, created);
    return created;
  }

  return { get, set, sub, bind };
}

/**
 * The same three callbacks for a cell that is not an atom.
 *
 * `@uniflowed/cell` is the layer below this one and applications do reach it
 * — a route loader hands out cells — so the React binding accepts one
 * directly. There is no store involved: a cell already holds its own value,
 * which is precisely the difference between a cell and an atom.
 */
const cellBindings: WeakMap<AnyCell, AnyBinding> = new WeakMap();

export function bindCell<T>(source: Cell<T>): Binding<T, T> {
  const existing = cellBindings.get(source);
  if (existing != null) {
    return existing;
  }
  const created: Binding<T, T> = {
    subscribe: (listener) => subscribe(source, listener),
    snapshot: () => peek(source),
    setter: (value) => {
      write(source, value);
    },
  };
  cellBindings.set(source, created);
  return created;
}

/**
 * The store used by anything that does not name one.
 *
 * Created on first use rather than at import, so a module that imports this
 * package and only declares atoms allocates nothing — and so the cost lands in
 * a stack the profiler can attribute.
 */
let fallback: null | StoreInstance = null;

export function defaultStore(): StoreInstance {
  if (fallback === null) {
    fallback = createStore();
  }
  return fallback;
}
