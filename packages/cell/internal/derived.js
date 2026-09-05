// @flow
//
// Derived cells and effects: the two nodes that run a function.
//
// They are the same machinery pointed at different ends. A `computed` runs a
// function and keeps its value; an `effect` runs a function and keeps its side
// effect. Both discover what they depend on by running, both re-run when what
// they read changes, and both stop being work the moment nothing is watching.
// Keeping them in one module is the honest arrangement: an effect is a derived
// cell whose value nobody reads.
//
// # Why there is no dependency array
//
// `derive` is called with tracking on and every cell it reads links itself to
// the result, so a derive that branches — `read(showAll) ? read(all) :
// read(some)` — depends on exactly what it read *this time*. The links are
// rebuilt on every run, so flipping the branch drops the dependency on the
// branch not taken: writing to it afterwards recomputes nothing. A stale
// dependency array is not a mistake this API lets anyone make.
//
// # Why an effect is a subscriber
//
// An effect subscribes to itself with a listener that does nothing. That is
// not a trick: subscribing is what makes a node *watched*, and watched is what
// installs the back edges that let a write reach it. An effect with no
// subscriber would be a derive nobody reads, which by design never runs.
// Because it is queued like any other subscriber, a burst of writes inside one
// `batch` re-runs the body once, after the graph is consistent.

import type { Cell, CellOptions, Unsubscribe } from "./graph.js";
import { createNode, subscribeNode } from "./graph.js";

/**
 * A cell computed from other cells, which discovers what those are by running.
 *
 * Lazy while nothing is watching: an unread, unsubscribed derive costs
 * nothing, and a write to something it depends on does not run it. It runs
 * when someone asks, and — once something *is* watching — when the flush that
 * follows a write asks on the subscriber's behalf.
 *
 * The value is memoised and compared with `equals` before anything downstream
 * is told, so a derive that returns the value it returned last time wakes
 * nobody.
 *
 * A derive that throws is memoised too: the failure is the node's committed
 * state, re-thrown by every read until a dependency changes and the derive is
 * given another chance. Retrying on every read would turn one failing derive
 * into a failure repeated once per reader per render.
 */
export function computed<T>(derive: () => T, options?: CellOptions<T>): Cell<T> {
  // A node with an `evaluate` starts stale, so nothing ever reads this
  // placeholder — every path to the value evaluates first. Flow has no way to
  // spell "no value yet" for a field that must hold a `T`, so the one cast is
  // here rather than in the node record where it would apply to sources too.
  const unevaluated: T = null as $FlowFixMe;
  return createNode({
    kind: "derived",
    scope: "react-render",
    value: unevaluated,
    evaluate: () => derive(),
    options,
  });
}

/**
 * Run `body` now, and again whenever a cell it read changes.
 *
 * `body` may return a teardown, which runs before each re-run and once more
 * when the effect is stopped — the same contract as `useEffect`, for the same
 * reason: whatever the last run started has to stop before the next run starts
 * it again.
 *
 * Stopping is what the returned function does, and it is not optional in a
 * long-lived process: an effect holds its dependencies watched, and a watched
 * source keeps every `onMount` up the chain running.
 */
export function effect(body: () => void | (() => void)): Unsubscribe {
  let cleanup: null | (() => void) = null;

  function release(): void {
    const previous = cleanup;
    cleanup = null;
    if (previous !== null) {
      previous();
    }
  }

  const node = createNode({
    kind: "derived",
    scope: "react-render",
    value: null,
    evaluate: () => {
      release();
      const next = body();
      cleanup = typeof next === "function" ? next : null;
      // A constant, so the node's version never moves: nothing derives from an
      // effect, and a version that changed every run would queue a
      // notification after every run for a listener that does nothing.
      return null;
    },
    options: { onMount: () => release },
  });

  return subscribeNode(node, () => {});
}
