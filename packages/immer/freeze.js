// @flow
//
// `@uniflowed/immer`: freezing, and the switch that turns it off.
//
// Immutable updates are a convention until something enforces them. Freezing
// is that enforcement: a value that came out of `produce` throws on assignment
// in a module (every ES module is strict mode), so the bug where a component
// mutates the state it was handed is a stack trace at the mutation instead of
// a re-render that never happens.
//
// # Why the whole result is frozen, base and all
//
// The result of a `produce` shares every subtree the recipe did not touch with
// the base — that is the point of structural sharing. Those shared subtrees
// have to be frozen for the *result* to be immutable, and they are the base's
// objects, so `produce` does freeze parts of its input. This is not an
// accident and it is not avoidable: an immutable value that contains a mutable
// one is not immutable, and copying the untouched subtrees to avoid it would
// give up the only property that makes this library fast.
//
// # Why the switch exists
//
// Freezing a large tree once is cheap; freezing it on every keystroke in a
// hot loop is not, and a production build has already been checked by a
// development build that froze everything. `setAutoFreeze(false)` is for that
// case, and for interop with code that legitimately mutates a value it owns
// after handing it over. It is global rather than per-call because it is a
// property of the application, and threading it through every `produce` would
// put it in the signature of every function that wraps one.

import { eachEntry, isDraft, isDraftable } from "./draft.js";

/**
 * Whether `produce` freezes what it returns.
 *
 * Module state, read when a `produce` call opens its scope, so a change takes
 * effect on the next call and never halfway through one.
 */
let autoFreeze: boolean = true;

/** Turn auto-freezing on or off for every subsequent `produce`. */
export function setAutoFreeze(next: boolean): void {
  autoFreeze = next;
}

/** Whether auto-freezing is on. `produce` reads this once per call. */
export function isAutoFreeze(): boolean {
  return autoFreeze;
}

/**
 * Whether `value` can no longer be written to.
 *
 * `true` for anything that is not an object, which is what every caller here
 * wants: a primitive is already as immutable as it will ever be, and the walk
 * that publishes a result should stop at one rather than inspect it.
 */
export function isFrozen(value: mixed): boolean {
  // $FlowFixMe[incompatible-call]
  // $FlowFixMe[incompatible-type]
  return Object.isFrozen(value);
}

/**
 * What a frozen collection's mutators become.
 *
 * `Object.freeze` seals a `Map`'s properties and does nothing at all to its
 * entries, because they live in an internal slot. Without this, a frozen state
 * tree containing a `Map` would silently accept `state.byId.set(...)` — the
 * one place where "frozen" would have been a lie.
 */
function refuseCollectionWrite(): empty {
  throw new TypeError("@uniflowed/immer: cannot modify a frozen Map or Set");
}

/**
 * Make `value` immutable, and everything under it when `deep` is set.
 *
 * A draft is left alone: it is still being written to, and freezing it would
 * break the produce it belongs to. An already-frozen value is left alone too,
 * which is what makes deep freezing cheap on a result that mostly consists of
 * subtrees an earlier `produce` already froze — the walk stops at the first
 * frozen node rather than descending through the whole tree again.
 */
export function freeze<T>(value: T, deep?: boolean): T {
  if (isFrozen(value) || isDraft(value) || !isDraftable(value)) {
    return value;
  }
  if (value instanceof Map || value instanceof Set) {
    // Shadowing the prototype's mutators is the only way to close a
    // collection, and it is done once, here.
    // $FlowFixMe[class-object-subtyping]
    // $FlowFixMe[incompatible-type]
    const collection = value as { [string]: mixed, ... };
    collection.set = refuseCollectionWrite;
    collection.add = refuseCollectionWrite;
    collection.clear = refuseCollectionWrite;
    collection.delete = refuseCollectionWrite;
  }
  Object.freeze(value);
  if (deep === true) {
    // Map keys are deliberately not frozen. A key is the caller's handle on an
    // entry, often an object it owns and uses elsewhere, and freezing it would
    // reach outside the state tree this call was given.
    eachEntry(value, (_key, child) => {
      freeze(child, true);
    });
  }
  return value;
}
