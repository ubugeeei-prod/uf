// @flow
//
// `@uniflowed/query/structural`: deciding that nothing changed.
//
// A server that is polled every five seconds answers with the same rows and a
// brand new object graph each time. `JSON.parse` cannot know that, so every
// response is a fresh identity, every observer re-renders, and every memoised
// child below it re-renders — twelve times a minute, forever, over data that
// has not moved. That is the cost this module removes.
//
// [`structuralShare`] takes the value the cache holds and the value that just
// arrived and returns a graph in which every subtree that is deeply equal is
// the *old* reference. A response with one changed row shares every other row,
// so a list re-renders one item instead of a thousand, and a response with no
// changes at all returns the previous object itself — `Object.is` says so,
// `useSyncExternalStore` bails out of the render, and `React.memo` below it
// never runs.
//
// # What the obvious version gets wrong
//
// The comparison people reach for first is `JSON.stringify(a) === JSON.stringify(b)`.
// It is wrong three ways, and each is a bug someone has shipped:
//
//   * It answers a different question. It tells you the values are equal, and
//     then you still hand React `next` — a new identity — because you have
//     nothing else to hand it. Equality was never the goal; *sharing* was.
//   * It cannot share subtrees. One changed field makes the whole tree
//     unequal, so the other nine hundred rows get new identities too.
//   * It depends on key order, so two objects that are equal in every way that
//     matters compare unequal because a server reordered its fields.
//
// The walk below is `O(size of the response)` once, with no serialisation and
// no allocation for the parts it shares.
//
// # What it deliberately does not do
//
// Only arrays and plain objects are walked. A `Date`, a `Map`, a class
// instance, or anything with a prototype is compared by identity and replaced
// wholesale — because "deeply equal" for those is a question with no single
// right answer, and guessing at it silently keeps a stale object alive. A
// cache that holds such values still works; it just does not get sharing for
// free inside them.
//
// Nothing here freezes. Freezing the graph would make the sharing enforceable,
// and it would also freeze objects the application owns and did not consent to
// hand over. `@uniflowed/immer` is where a value becomes structurally
// immutable; this module only decides which references may be reused.

/**
 * Whether `value` is an object literal rather than a class instance.
 *
 * `Object.prototype` or a null prototype, and nothing else: the walk may only
 * rebuild things it can rebuild faithfully, and a class instance rebuilt as an
 * object literal has silently lost its methods.
 */
export function isPlainObject(value: mixed): boolean {
  if (typeof value !== "object" || value === null || Array.isArray(value)) {
    return false;
  }
  const prototype = Object.getPrototypeOf(value);
  return prototype === Object.prototype || prototype === null;
}

/**
 * `next`, with every deeply-equal subtree replaced by `previous`'s reference.
 *
 * Returns `previous` itself when the two are deeply equal, which is the whole
 * point: an unchanged response must not produce a new identity, or nothing
 * downstream can tell that it was unchanged.
 *
 * The shape of the *result* is always `next`'s shape. Sharing never resurrects
 * a key that `next` dropped or a row it removed; it only reuses references for
 * the parts that are still there and still equal.
 */
export function structuralShare<T>(previous: mixed, next: T): T {
  if (previous === next) {
    return next;
  }

  const bothArrays = Array.isArray(previous) && Array.isArray(next);
  if (!bothArrays && !(isPlainObject(previous) && isPlainObject(next))) {
    return next;
  }

  const before = previous as $FlowFixMe;
  const after = next as $FlowFixMe;
  const keys = bothArrays ? null : Object.keys(after);
  const size = bothArrays ? after.length : (keys as $FlowFixMe).length;
  const beforeSize = bothArrays ? before.length : Object.keys(before).length;
  const copy = bothArrays ? new Array(size) : ({}: $FlowFixMe);

  // Counted rather than tracked with a flag, because "every child was shared"
  // is only half the answer: a child can be shared while the parent has gained
  // or lost a sibling, and then the parent is not the same object.
  let shared = 0;
  for (let index = 0; index < size; index += 1) {
    const key = bothArrays ? index : (keys as $FlowFixMe)[index];
    const child = structuralShare(before[key], after[key]);
    if (child === before[key] && (bothArrays || Object.hasOwn(before, key))) {
      shared += 1;
    }
    copy[key] = child;
  }

  return beforeSize === size && shared === size ? (before as T) : (copy as T);
}

/**
 * Whether two records have the same keys and `Object.is`-identical values.
 *
 * This is how an observer decides whether the snapshot it just built is the
 * one it already returned. It is a *shallow* comparison on purpose: every
 * field it compares is either a primitive or a value that already went through
 * [`structuralShare`], so identity is the correct question and a deep walk
 * here would be paying twice for an answer we already have.
 */
export function shallowEqual(left: { +[string]: mixed }, right: { +[string]: mixed }): boolean {
  if (left === right) {
    return true;
  }
  const keys = Object.keys(left);
  if (keys.length !== Object.keys(right).length) {
    return false;
  }
  for (const key of keys) {
    if (!Object.is(left[key], right[key])) {
      return false;
    }
  }
  return true;
}
