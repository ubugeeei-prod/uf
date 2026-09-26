// @flow
//
// Internal to `@uniflowed/test`: a value that is a function, as one to call.
//
// A test hands this package values whose types it cannot know: the subject of
// `expect(() => ...).toThrow()`, the implementation given to `fn(body)`, the
// constructor in `expect.any(Date)`. Each is checked with `typeof` before it is
// used, and that check is all JavaScript offers. Flow reads `typeof value ===
// "function"` as a function whose parameters are unknown, which it will not
// call, construct with or put on the right of `instanceof`.
//
// So this is the one place that turns such a value into something callable:
// the run-time check and the single suppression that stands for it live
// together, rather than an `any` at every call site.

/** A function of unknown signature: it accepts any `this` and arguments. */
export type Callable = (this: mixed, ...args: $ReadOnlyArray<mixed>) => mixed;

/** `value` as a function to call, or `null` when it is not a function. */
export function callable(value: mixed): Callable | null {
  if (typeof value !== "function") {
    return null;
  }
  // Checked just above, which is as far as JavaScript can check a function;
  // Flow has no type for "a function with unknown parameters" that it calls.
  // $FlowFixMe[incompatible-type]
  return value;
}
