// @flow
//
// Asynchronous cells: a value that arrives later, in a graph that is otherwise
// synchronous.
//
// A resource is a derived node whose evaluation starts a load instead of
// producing a value. That one difference is what makes the rest of the graph
// work unchanged: the load runs inside the tracking frame, so whatever it
// reads becomes a dependency, and a change to any of them re-evaluates the
// node — which means starting the load again with a new input.
//
// # Why a settlement carries a generation
//
// Re-loading is where asynchronous state goes wrong. Two loads are in flight,
// the first is slower than the second, and it settles last: the value the user
// asked for most recently is overwritten by the answer to a question they have
// already moved on from. It is not a race that shows up in tests written
// against a fast local server, and it is the defect the whole design is
// pointed at.
//
// So every node counts how many times something has superseded the work it
// started — a re-evaluation, or a direct write — and a `then` handler that
// finds the count has moved drops its result on the floor.
//
// # Why the load is also told
//
// The generation drops the *answer* to a question nobody is asking any more,
// which is what keeps the value correct. On its own it does nothing about the
// work: the request is still open, the rate limit is still spent, the
// connection is still held, and on a search box that reloads per keystroke
// every abandoned request runs to completion. This file used to say so —
// "nothing cancels; the promise still settles" — as though the two halves were
// one choice. They are not. Dropping the result is what makes the cell honest;
// telling the loader is what makes the machine stop.
//
// So a load is handed an `AbortSignal`, aborted the moment this cell stops
// speaking for it: a re-evaluation, a write, or the loss of the last watcher.
// A load that ignores the signal is still correct, because whether a result is
// adopted is decided here; passing it to `fetch` is what makes the request
// actually stop.
//
// # Why an abandoned load's rejection is not a failure
//
// Aborting a `fetch` rejects it. The obvious implementation commits that
// rejection like any other, and then a component that unmounts has turned its
// own cleanup into a `"failure"` on the next thing to read the cell. The
// generation does not catch it either: losing the last watcher supersedes
// nothing, so the count has not moved. The signal is therefore the second half
// of the test — a settlement from a load whose signal is aborted is dropped
// whatever the generation says.
//
// # Why abandoning also marks the cell stale
//
// A cancelled load leaves the node holding `"pending"` with a dependency list
// nothing will ever mark, so the next subscriber would wait forever for a
// request that is not running. Marking it dirty on the way out is what makes
// the next read start again — and only when a load was actually in flight, so
// unmounting a cell whose data has already arrived does not throw that data
// away and refetch it on the way back in.
//
// # Why the load starts on first contact rather than at construction
//
// A resource declared at module scope costs nothing until something wants it,
// which is what lets a module full of them be imported by a route that uses
// one. First contact means a read or a subscription, both of which pull the
// node, and pulling a node that has never run evaluates it.
//
// # Why a failure is re-thrown on every read
//
// Swallowing it would turn a failed fetch into an indistinguishable empty
// state — `null` from a load that failed, and `null` from a load that returned
// nothing, are the same value with entirely different meanings. `status` is
// there for callers that would rather branch than catch.

import type { Cell, CellOptions, ResourceStatus } from "./graph.js";
import {
  createNode,
  currentValue,
  generationOf,
  invalidateNode,
  setStatus,
  settleNode,
  statusOf,
} from "./graph.js";

/**
 * The second argument every load is given.
 *
 * An object rather than the signal itself, so that a later addition — the
 * generation, a deadline — is a new member rather than a new parameter every
 * existing load has to be re-read to understand.
 */
export type LoadContext = {
  /**
   * Aborted when this cell stops speaking for the load: something it read
   * changed, [`refresh`] was called, a value was written over it, or it lost
   * its last watcher.
   *
   * `signal.reason` is an `Error` naming which of those it was. Nothing is
   * required of a load that ignores it — the result is dropped either way.
   */
  readonly signal: AbortSignal,
};

/**
 * A cell whose value arrives from a promise.
 *
 * Reads as `null` while the load is in flight, which keeps the type one `?T`
 * rather than forcing every consumer through a status union for a state most
 * of them render as a spinner and forget. The value it already holds survives
 * a reload until the new one settles, so a refetch does not blank the screen.
 *
 * `load` is tracked: `resource(() => fetchUser(read(userId)))` reloads when
 * `userId` changes, and the load that was in flight for the previous id is
 * discarded rather than allowed to win a race against the new one.
 *
 * It is also given a [`LoadContext`], whose `signal` is aborted at that same
 * moment — `resource(({ signal }) => fetch(url, { signal }))` is a load that
 * stops rather than one that is merely ignored.
 */
export function resource<T>(
  load: (context: LoadContext) => Promise<T>,
  options?: CellOptions<?T>,
): Cell<?T> {
  const initial: ?T = null;

  // The load this cell currently speaks for, or `null` between loads. One
  // controller per load rather than one per cell: a signal cannot be
  // un-aborted, so a shared one would abort the load that replaced the
  // abandoned one the instant it started.
  let inFlight: null | AbortController = null;

  /** Stop the load in flight, if there is one, and say why in its reason. */
  function abandon(reason: string): boolean {
    const controller = inFlight;
    if (controller === null) {
      return false;
    }
    inFlight = null;
    controller.abort(Error(`@uniflowed/cell load abandoned: ${reason}`));
    return true;
  }

  return createNode({
    kind: "resource",
    scope: "async-resource",
    value: initial,
    status: "idle",
    evaluate: (self) => {
      // The evaluation itself bumped the generation, so this is the one this
      // load speaks for. Anything that supersedes it moves the count again.
      const generation = generationOf(self);
      // Whatever was running was answering the question this evaluation is
      // replacing.
      abandon("superseded");

      const controller = new AbortController();
      inFlight = controller;
      setStatus(self, "pending");

      // Both halves of the test, and both are load-bearing: the generation
      // catches a load this cell superseded, and the signal catches one it
      // simply stopped wanting — an unmount moves no counter.
      const speaksForCell = () => generationOf(self) === generation && !controller.signal.aborted;

      let pending;
      try {
        pending = load({ signal: controller.signal });
      } catch (error) {
        // A `load` that throws rather than rejecting is still a failed load.
        inFlight = null;
        setStatus(self, "failure");
        throw error;
      }

      pending.then(
        (value: T) => {
          if (speaksForCell()) {
            inFlight = null;
            settleNode(self, value, "success", null);
          }
        },
        (error: mixed) => {
          if (speaksForCell()) {
            inFlight = null;
            settleNode(self, null, "failure", error);
          }
        },
      );

      // Not a pull: the node is mid-evaluation. What it holds now is what it
      // keeps until the load settles.
      return currentValue(self);
    },
    // A write is the one supersession the node cannot see coming. The graph
    // tells it, because the graph is where writes happen.
    abandon: () => {
      abandon("written over");
    },
    options: {
      equals: options?.equals,
      onMount: (self) => {
        const teardown = options?.onMount?.(self);
        return () => {
          // The caller's teardown first: its mount ran inside this one, so it
          // is the inner of the two and unwinds first.
          if (typeof teardown === "function") {
            teardown();
          }
          if (!abandon("unwatched")) {
            // Nothing was in flight, so the value the cell holds is a settled
            // one and is still the right answer when a watcher comes back.
            return;
          }
          invalidateNode(self);
        };
      },
    },
  });
}

/**
 * How far along a [`resource`]'s load is; `"success"` for any other cell.
 *
 * A cell that holds its value always has it, which is what `"success"` means
 * here. Returning `null` instead, and making every caller handle a state that
 * cannot happen, buys nothing.
 */
export function status<T>(source: Cell<T>): ResourceStatus {
  return statusOf(source);
}

/**
 * Load again, even though nothing the cell depends on changed.
 *
 * The escape hatch for state uf does not model: the server knows something the
 * client's dependency graph does not.
 *
 * On a cell something is watching, the reload starts at the end of the write —
 * and the load in flight, if any, is aborted as the new one begins. On a cell
 * nothing is watching this only marks; the reload happens on the next read,
 * because running it for an audience of nobody is work with no observer, and
 * until then a load already in flight is left alone to settle.
 */
export function refresh<T>(source: Cell<T>): void {
  invalidateNode(source);
}
