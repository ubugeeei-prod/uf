// @flow
//
// `@uniflowed/cell`: the reactive primitive every other uf state package is
// built from.
//
// A cell is a value that knows who is reading it. That is the whole idea: no
// store, no reducer, no framework, no import-time work. `@uniflowed/state`
// adds atoms and React on top of this; `@uniflowed/loader` reads cells a route
// filled in. Neither of them keeps a second graph, because there is one here.
//
// # The file map, and why it is this one
//
// This entry point is the *verbs* — what you can do to a cell, whatever kind
// it is. The mechanism is split by the question it answers:
//
// * `internal/graph.js` — what a cell is, how it learns what it depends on,
//   and how one write reaches everything that cares exactly once. It owns
//   every mutation of a node.
// * `internal/schedule.js` — when subscribers are told, as opposed to when
//   values change. Batching lives here and knows nothing about cells.
// * `internal/source.js` — cells that hold a value: the roots.
// * `internal/derived.js` — cells that run a function: `computed` and
//   `effect`, which are the same machinery pointed at different ends.
// * `internal/resource.js` — cells whose value arrives from a promise.
//
// The split is by *concept*, not by size. The alternative — one
// `internal/reactive.js` with everything in it and an entry point that only
// re-exports — is what this package used to be, and it made the interesting
// question ("where does the glitch-freedom live?") unanswerable without
// reading the whole file.
//
// # What a reader should know before using it
//
// Reads are consistent the instant a write returns: batching defers *waking
// subscribers*, never the values a read observes. Recomputation is lazy while
// nothing is watching, and glitch-free once something is — a diamond
// dependency runs its join once per write, not once per path. Named exports
// only, so a bundler can drop what an application does not reach.

import type {
  Cell,
  CellOptions,
  CellScope,
  CellSnapshot,
  Listener,
  ResourceStatus,
  Unsubscribe,
} from "./internal/graph.js";
import {
  peekNode,
  readNode,
  snapshotNode,
  subscribeNode,
  untracked,
  writeNode,
} from "./internal/graph.js";

export type { Cell, CellOptions, CellScope, CellSnapshot, ResourceStatus, Unsubscribe };

export { batch } from "./internal/schedule.js";
export { cell } from "./internal/source.js";
export { computed, effect } from "./internal/derived.js";
export { refresh, resource, status } from "./internal/resource.js";
export { untracked };

/**
 * Read a cell.
 *
 * Called inside a `computed` or an `effect`, this is also what records the
 * dependency — there is no separate subscribe step, and no way to read a value
 * a derive depends on without depending on it, short of [`peek`].
 *
 * Reading a failed cell re-throws what it failed with.
 */
export function read<T>(source: Cell<T>): T {
  return readNode(source);
}

/**
 * Read a cell without depending on it.
 *
 * For the derive that wants to *look at* a value without waking when it
 * changes — a computation that reads a configuration flag it does not want to
 * recompute for. `untracked` is the same escape hatch for a whole block.
 */
export function peek<T>(source: Cell<T>): T {
  return peekNode(source);
}

/**
 * Replace what a cell holds.
 *
 * A write of the value it already holds is dropped, so nothing downstream runs
 * and no subscriber is woken. Derived cells refuse: their value is a function
 * of their dependencies, and a write that stood would be silently undone by
 * the next recompute.
 */
export function write<T>(source: Cell<T>, value: T): void {
  writeNode(source, value);
}

/**
 * Write the result of `reduce` applied to what the cell currently holds.
 *
 * The read and the write are one step so that updaters compose:
 * `update(count, (n) => n + 1)` twice increments twice, where
 * `write(count, read(count) + 1)` twice against a value read once does not.
 * The read is untracked — reducing a value is not depending on it.
 */
export function update<T>(source: Cell<T>, reduce: (current: T) => T): void {
  writeNode(
    source,
    untracked(() => reduce(peekNode(source))),
  );
}

/**
 * Be told when a cell's value changes.
 *
 * Subscribing is what makes a cell *live*: it installs the links that let a
 * write reach it, runs its `onMount`, and keeps the cells it derives from live
 * too. Unsubscribing the last listener undoes all of that, so a subscription
 * that is never returned is a subscription that never stops.
 *
 * The listener is called after the graph is consistent, at most once per
 * `batch`, and only when the value it would read actually changed.
 */
export function subscribe<T>(source: Cell<T>, listener: Listener): Unsubscribe {
  return subscribeNode(source, listener);
}

/** What a cell holds, and where that value belongs. */
export function snapshot<T>(source: Cell<T>): CellSnapshot<T> {
  return snapshotNode(source);
}
