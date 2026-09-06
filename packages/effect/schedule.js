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
//
// It does not decide whether the *error* is worth another attempt either. A
// 429 should be retried and a 400 should not, and both are typed failures, so
// that judgement needs to see an `E` — which would put a caller's predicate,
// and therefore a caller's exception, inside a module whose whole claim is
// that nothing in it can fail. `retry` takes the predicate instead and
// composes it with the schedule; see its `options` parameter in `index.js`.
//
// The same argument rules out Effect's `*Effect` schedule combinators —
// `whileInputEffect`, `mapEffect` — which put an `Effect` inside a schedule
// and drag the runtime back in here. They are deliberately absent rather than
// smuggled in behind a `mixed` callback, and the state machine that would make
// the rest of Effect's schedule surface expressible is filed as #327.
//
// # Randomness is an argument, not an import
//
// `jittered` needs a random number and this module does not read one. The
// factor is the third parameter of `scheduleDelay`, so a jittered schedule is
// still a pure function of its inputs: it can be tested for the whole range by
// naming the factor rather than by hoping about `Math.random`, and the module
// keeps the totality its header claims for every function in it.

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
 * - `jittered` — another schedule with each wait scaled by a random factor
 *   between `minPercent` and `maxPercent`, defaulting to 80 and 120. Without
 *   it every client that failed at the same moment retries at the same
 *   moment, for ever, which is what turns a brief outage into a sustained
 *   one. `maxDelay` caps the wait; nothing else spreads it.
 *
 * Effect distinguishes `jittered` from `jitteredWith`; here they are one arm
 * with the bounds optional, because a schedule is data rather than a function
 * and there is nothing for a second name to mean.
 */
export type Schedule =
  | { readonly kind: "recurs", readonly times: number }
  | { readonly kind: "spaced", readonly millis: number }
  | { readonly kind: "exponential", readonly baseMillis: number, readonly factorPercent?: number }
  | { readonly kind: "fibonacci", readonly baseMillis: number }
  | { readonly kind: "upTo", readonly millis: number }
  | { readonly kind: "intersect", readonly left: Schedule, readonly right: Schedule }
  | { readonly kind: "union", readonly left: Schedule, readonly right: Schedule }
  | { readonly kind: "maxDelay", readonly schedule: Schedule, readonly millis: number }
  | {
      readonly kind: "jittered",
      readonly schedule: Schedule,
      readonly minPercent?: number,
      readonly maxPercent?: number,
    };

/** The default `exponential` growth, as a percentage: doubling. */
const DEFAULT_FACTOR_PERCENT = 200;

/** The default `jittered` range, as percentages: Effect's [0.8, 1.2]. */
const DEFAULT_JITTER_MIN_PERCENT = 80;
const DEFAULT_JITTER_MAX_PERCENT = 120;

/** The factor a two-argument `scheduleDelay` jitters with: the midpoint. */
const UNJITTERED_FACTOR = 0.5;

/**
 * How long to wait before retry number `attempt`, or `null` to stop.
 *
 * Total: every schedule answers for every attempt, and a caller that gets
 * `null` has its answer rather than an exception to interpret.
 *
 * `randomFactor` is the one number a `jittered` schedule needs, in [0, 1], and
 * it is passed in rather than drawn here — see the header. Every `jittered`
 * arm in one tree scales by the same factor, which is what a caller asking a
 * schedule "how long, this time" means. It defaults to the midpoint, so a
 * two-argument call gets a jittered schedule's mean wait and stays as
 * deterministic as every other arm.
 *
 * Read with `switch` rather than `match`: `Schedule`'s `exponential` arm has an
 * optional property, and a `match` object pattern that binds `factorPercent`
 * does not cover the case where it is absent — the checker reports the match as
 * inexhaustive, and it is right to. `jittered` has two such properties, so a
 * wider union makes that more true rather than less. Filed as #205.
 */
export function scheduleDelay(
  schedule: Schedule,
  attempt: number,
  randomFactor: number = UNJITTERED_FACTOR,
): ?number {
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
      return intersectDelay(schedule.left, schedule.right, attempt, randomFactor);
    case "union":
      return unionDelay(schedule.left, schedule.right, attempt, randomFactor);
    case "jittered":
      return jitteredDelay(schedule, attempt, randomFactor);
    default:
      return capDelay(schedule.schedule, schedule.millis, attempt, randomFactor);
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
function intersectDelay(
  left: Schedule,
  right: Schedule,
  attempt: number,
  randomFactor: number,
): ?number {
  const leftDelay = scheduleDelay(left, attempt, randomFactor);
  const rightDelay = scheduleDelay(right, attempt, randomFactor);
  return leftDelay == null || rightDelay == null ? null : Math.max(leftDelay, rightDelay);
}

/** Either side is enough, and the shorter wait wins. */
function unionDelay(
  left: Schedule,
  right: Schedule,
  attempt: number,
  randomFactor: number,
): ?number {
  const leftDelay = scheduleDelay(left, attempt, randomFactor);
  const rightDelay = scheduleDelay(right, attempt, randomFactor);
  if (leftDelay == null) {
    return rightDelay;
  }
  if (rightDelay == null) {
    return leftDelay;
  }
  return Math.min(leftDelay, rightDelay);
}

/**
 * Spread a wait over a range, so clients that failed together do not retry
 * together.
 *
 * The factor scales the *inner* schedule's answer rather than replacing it, so
 * a jittered exponential still grows and a jittered schedule that has given up
 * stays given up — jitter is about when, and `null` is not a when.
 *
 * The bounds are percentages for the same reason `exponential`'s factor is:
 * 1.2 written as `120` survives a JSON round trip through a config file
 * without becoming `1.1999999999999997`.
 */
function jitteredDelay(
  schedule: {
    readonly kind: "jittered",
    readonly schedule: Schedule,
    readonly minPercent?: number,
    readonly maxPercent?: number,
  },
  attempt: number,
  randomFactor: number,
): ?number {
  const delayed = scheduleDelay(schedule.schedule, attempt, randomFactor);
  if (delayed == null) {
    return null;
  }
  const low = schedule.minPercent == null ? DEFAULT_JITTER_MIN_PERCENT : schedule.minPercent;
  const high = schedule.maxPercent == null ? DEFAULT_JITTER_MAX_PERCENT : schedule.maxPercent;
  const clamped = Math.min(1, Math.max(0, randomFactor));
  return Math.max(0, Math.round((delayed * (low + (high - low) * clamped)) / 100));
}

/**
 * Cap a wait without changing when the inner schedule gives up.
 *
 * A schedule that has stopped stays stopped: capping the delay of a `null` to
 * `millis` would turn "give up" into "wait and try forever".
 */
function capDelay(
  schedule: Schedule,
  millis: number,
  attempt: number,
  randomFactor: number,
): ?number {
  const delayed = scheduleDelay(schedule, attempt, randomFactor);
  return delayed == null ? null : Math.min(delayed, millis);
}
