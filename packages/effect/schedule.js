// @flow
//
// `@uniflowed/effect/schedule`.
//
// When to try again, as data.
//
// # Why this is a separate module
//
// `index.js` argues that the runtime cannot be split: `Effect`, `Fiber`, `Tag`
// and `Layer` are opaque types over carriers only that file may construct, so
// moving a combinator out means handing `readKernel` to a sibling and giving
// the opacity away to buy a directory listing.
//
// A schedule is the one part of this package that is not that. It is
// arithmetic over an attempt count. It never sees a `Context`, an `Exit` or a
// fiber, it cannot reach a carrier because it has no way to name one, and
// nothing here has to run to be understood. That makes it separable in the
// sense the runtime is not: a policy can be written, read and tested with no
// runtime at all, and `retry` cannot quietly grow a dependency on a schedule's
// internals because there are none to reach.
//
// # What a schedule does not decide
//
// A schedule says *when* to try again and never *whether*. Only `retry` knows
// that a defect is a bug and an interruption is a decision already taken, and
// that repeating either just repeats it. Keeping that judgement out of here is
// what makes every function in this module total: `scheduleDelay` answers for
// every input and has no failure mode of its own.

/**
 * A retry policy: given the number of attempts already made, how long to wait
 * before the next one, or `null` for "stop".
 *
 * The delay is milliseconds and `attempt` counts *retries*, so the first
 * decision is made with `attempt` of 0 after one failure.
 *
 * - `recurs` — up to `times` more attempts, with no delay between them.
 * - `spaced` — a fixed wait, forever.
 * - `exponential` — `baseMillis` multiplied by `factorPercent / 100` per
 *   attempt, defaulting to doubling. A percentage rather than a float because
 *   1.5 written as `150` survives a JSON round trip through a config file
 *   without becoming `1.4999999999999998`.
 * - `fibonacci` — `baseMillis` times the Fibonacci number for the attempt,
 *   which grows more gently than doubling.
 * - `upTo` — one more attempt after waiting `millis`, then stop.
 * - `intersect` — retry only while *both* sides would, waiting the longer of
 *   the two.
 * - `union` — retry while *either* side would, waiting the shorter.
 * - `maxDelay` — another schedule with its wait capped, which is how an
 *   exponential policy is kept from waiting an hour on its twelfth attempt.
 */
export type Schedule =
  | { readonly kind: "recurs", readonly times: number }
  | { readonly kind: "spaced", readonly millis: number }
  | { readonly kind: "exponential", readonly baseMillis: number, readonly factorPercent?: number }
  | { readonly kind: "fibonacci", readonly baseMillis: number }
  | { readonly kind: "upTo", readonly millis: number }
  | { readonly kind: "intersect", readonly left: Schedule, readonly right: Schedule }
  | { readonly kind: "union", readonly left: Schedule, readonly right: Schedule }
  | { readonly kind: "maxDelay", readonly schedule: Schedule, readonly millis: number };

/** The default `exponential` growth, as a percentage: doubling. */
const DEFAULT_FACTOR_PERCENT = 200;

/**
 * How long to wait before retry number `attempt`, or `null` to stop.
 *
 * Total: every schedule answers for every attempt, and a caller that gets
 * `null` has its answer rather than an exception to interpret.
 *
 * Read with `switch` rather than `match`: `Schedule`'s `exponential` arm has an
 * optional property, and a `match` object pattern that binds `factorPercent`
 * does not cover the case where it is absent — the checker reports the match as
 * inexhaustive, and it is right to.
 */
export function scheduleDelay(schedule: Schedule, attempt: number): ?number {
  switch (schedule.kind) {
    case "recurs":
      return attempt < schedule.times ? 0 : null;
    case "spaced":
      return Math.max(0, schedule.millis);
    case "exponential":
      return exponentialDelay(schedule.baseMillis, schedule.factorPercent, attempt);
    case "fibonacci":
      return Math.max(0, schedule.baseMillis * fibonacci(attempt));
    case "upTo":
      return attempt === 0 ? Math.max(0, schedule.millis) : null;
    case "intersect":
      return intersectDelay(schedule.left, schedule.right, attempt);
    case "union":
      return unionDelay(schedule.left, schedule.right, attempt);
    default:
      return capDelay(schedule.schedule, schedule.millis, attempt);
  }
}

/**
 * The `index`th Fibonacci number, iteratively.
 *
 * Iterative rather than recursive because the naive recursion is exponential,
 * and a retry policy that costs more to compute than the wait it describes is
 * a strange thing to ship.
 */
function fibonacci(index: number): number {
  let previous = 0;
  let current = 1;
  for (let position = 0; position < index; position += 1) {
    const next = previous + current;
    previous = current;
    current = next;
  }
  return current;
}

function exponentialDelay(baseMillis: number, factorPercent: ?number, attempt: number): number {
  const factor = (factorPercent == null ? DEFAULT_FACTOR_PERCENT : factorPercent) / 100;
  return Math.max(0, Math.round(baseMillis * Math.pow(factor, attempt)));
}

/** Both sides must still want a retry, and the longer wait wins. */
function intersectDelay(left: Schedule, right: Schedule, attempt: number): ?number {
  const leftDelay = scheduleDelay(left, attempt);
  const rightDelay = scheduleDelay(right, attempt);
  return leftDelay == null || rightDelay == null ? null : Math.max(leftDelay, rightDelay);
}

/** Either side is enough, and the shorter wait wins. */
function unionDelay(left: Schedule, right: Schedule, attempt: number): ?number {
  const leftDelay = scheduleDelay(left, attempt);
  const rightDelay = scheduleDelay(right, attempt);
  if (leftDelay == null) {
    return rightDelay;
  }
  if (rightDelay == null) {
    return leftDelay;
  }
  return Math.min(leftDelay, rightDelay);
}

/**
 * Cap a wait without changing when the inner schedule gives up.
 *
 * A schedule that has stopped stays stopped: capping the delay of a `null` to
 * `millis` would turn "give up" into "wait and try forever".
 */
function capDelay(schedule: Schedule, millis: number, attempt: number): ?number {
  const delayed = scheduleDelay(schedule, attempt);
  return delayed == null ? null : Math.min(delayed, millis);
}
