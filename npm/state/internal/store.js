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
  derived,
  peek,
  read,
  refresh,
  resource,
  state,
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
  readonly reload: <V>(target: AtomRecord<V, empty>) => void,
};

/**
 * An atom, as the thing a store's maps are keyed by.
 *
 * A key is an identity and nothing else: the maps are looked up by the atom
 * the caller already holds, and no property of one is ever read through this
 * type. So the honest key type is not "an atom record of some value type" —
 * which is `AtomRecord<any, any>`, and is a claim that every field of one may
 * be read and will answer `any` — it is "something with an atom's name on it".
 * An interface is how Flow says that: `AtomRecord<V, A>` satisfies it for
 * every `V` and `A`, and nothing that comes back out of a map has this type.
 */
type AtomIdentity = interface { readonly label: string };

/**
 * A cell or a binding whose value type is not known here.
 *
 * This is the existential the key type escaped, and it does not escape: a
 * store's maps hold every atom in the application at once, and the lookup that
 * comes back out has to be a `Cell<V>` for the `V` of the atom it was found
 * under. That is a *correspondence* between a key's type and its value's, and
 * Flow has no way to write one — not an existential (`some T` loses which
 * `T`), not variance (`Cell` is invariant, deliberately: a `Cell<Dog>` is not
 * a `Cell<Animal>` because anything holding the second may write a `Cat`), and
 * not `mixed` with a cast at the read, which is this same unsoundness spelled
 * three times instead of twice.
 *
 * `Binding` is the near miss worth recording. Its `T` is only ever returned
 * and its `A` only ever taken, so with the sigils that say so —
 * `Binding<out T, in A>` — every binding really would be a
 * `Binding<mixed, empty>` and could be *stored* as one. It is reading it back
 * as the caller's `Binding<V, A>` that has nowhere to go, so the sigils would
 * buy nothing here and are not added for the look of it.
 *
 * Every function that reaches a value is generic in its type, so nothing
 * outside this file sees either of these.
 */
// Suppressed rather than left to fail `check:lib`: the reason above is the
// whole argument, and it does not end in a change anyone can make to this file.
// uf-lint-disable flow/unclear-type
type AnyCell = Cell<any>;
type AnyBinding = Binding<any, any>;
// uf-lint-enable flow/unclear-type

export function createStore(): StoreInstance {
  const cells: WeakMap<AtomIdentity, AnyCell> = new WeakMap();
  const bindings: WeakMap<AtomIdentity, AnyBinding> = new WeakMap();
  /**
   * How to make an asynchronous atom load again, per atom instantiated here.
   *
   * An async atom is two cells — a resource and the projection over it — and
   * `cells` holds the projection, because that is the one an application
   * reads. Refreshing the projection would do nothing: it is a `derived` cell,
   * and recomputing it reads a resource that is perfectly up to date. So the
   * resource is recorded here as it is built, next to the map that hides it.
   */
  const reloads: WeakMap<AtomIdentity, () => void> = new WeakMap();

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
      "primitive" => state(target.initial, options),
      "async" => loadable<V, A>(target, options),
      _ => derivedAtom<V, A>(target, options),
    };
  }

  /**
   * The cell for an atom of kind `"derived"`.
   *
   * The atom kind and the cell constructor carry the same name one layer
   * apart, so the local one takes the suffix: bare `derived` here is
   * `@uniflowed/cell`'s.
   */
  function derivedAtom<V, A>(target: AtomRecord<V, A>, options: CellOptions<V>): Cell<V> {
    const reader = target.read;
    if (reader === null) {
      // A write-only atom. It still gets a cell, so that `useSetAtom` on one
      // works the same way as on any other atom, but its value is a constant
      // and nothing ever recomputes it.
      return state(target.initial, options);
    }
    return derived(() => reader(get), options);
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
      return state(target.initial, options);
    }
    // The load's own context is forwarded rather than rebuilt: the cell owns
    // the signal, because the cell is what abandons the load.
    const pending = resource((context) => loader(get, context));
    reloads.set(target, () => {
      refresh(pending);
    });
    return derived(() => {
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

  /**
   * Start an asynchronous atom's load again with the dependencies it already
   * has.
   *
   * Instantiating first is what makes this work on an atom this store has
   * never been asked for: the resource is built as the atom is instantiated,
   * so the entry exists by the time it is looked up.
   *
   * The throw is a runtime guard behind a static one. `refresh` takes an
   * `AsyncAtom`, which only `asyncAtom` produces, so a `selector` that happens
   * to return a `Loadable` is rejected by Flow before it gets here — but the
   * type is erased at run time and an untyped caller deserves the name of the
   * atom rather than a silent no-op.
   */
  const reload = <V>(target: AtomRecord<V, empty>): void => {
    cellFor(target);
    const again = reloads.get(target);
    if (again === undefined) {
      throw Error(`@uniflowed/state ${target.label} is not an asynchronous atom`);
    }
    again();
  };

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

  return { get, set, sub, bind, reload };
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
