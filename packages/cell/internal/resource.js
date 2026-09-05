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
// finds the count has moved drops its result on the floor. Nothing cancels;
// the promise still settles. It simply no longer speaks for the cell.
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
 */
export function resource<T>(load: () => Promise<T>, options?: CellOptions<?T>): Cell<?T> {
  const initial: ?T = null;
  return createNode({
    kind: "resource",
    scope: "async-resource",
    value: initial,
    status: "idle",
    evaluate: (self) => {
      // The evaluation itself bumped the generation, so this is the one this
      // load speaks for. Anything that supersedes it moves the count again.
      const generation = generationOf(self);
      setStatus(self, "pending");

      let pending;
      try {
        pending = load();
      } catch (error) {
        // A `load` that throws rather than rejecting is still a failed load.
        setStatus(self, "failure");
        throw error;
      }

      pending.then(
        (value: T) => {
          if (generationOf(self) === generation) {
            settleNode(self, value, "success", null);
          }
        },
        (error: mixed) => {
          if (generationOf(self) === generation) {
            settleNode(self, null, "failure", error);
          }
        },
      );

      // Not a pull: the node is mid-evaluation. What it holds now is what it
      // keeps until the load settles.
      return currentValue(self);
    },
    options,
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
 * client's dependency graph does not. The in-flight load, if any, is
 * superseded rather than cancelled.
 *
 * On a cell nothing is watching this only marks; the reload happens on the
 * next read, because running it for an audience of nobody is work with no
 * observer.
 */
export function refresh<T>(source: Cell<T>): void {
  invalidateNode(source);
}
