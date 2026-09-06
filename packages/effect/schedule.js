// @flow
//
// `@uniflowed/effect/schedule`.
//
// When to try again, and whether to, as data.
//
// # Why this is a separate module
//
// `index.js` argues that the runtime cannot be split: `Effect`, `Fiber`, `Tag`
// and `Layer` are opaque types over carriers only that file may construct, so
// moving a combinator out means handing `readKernel` to a sibling and giving
// the opacity away to buy a directory listing.
//
// A schedule is the one part of this package that is not that. It never sees a
// `Context`, an `Exit` or a fiber, it cannot reach a carrier because it has no
// way to name one, and nothing here has to run to be understood. That makes it
// separable in the sense the runtime is not: a policy can be written, read and
// tested with no runtime at all, and `retry` cannot quietly grow a dependency
// on a schedule's internals because there are none to reach.
//
// # A state machine, not an attempt counter
//
// This was arithmetic over an attempt count — `scheduleDelay(schedule, n)`,
// answering "how long before attempt *n*" and nothing else. Three things
// followed, and #327 is the issue for all three.
//
// `repeat` could not return the schedule's answer, because a schedule had no
// answer to give beyond a delay. Nothing could decide on a *value*, so
// "poll until the job reports finished" was not a schedule at all. And the
// timetable arms — `fixed`, `windowed`, `elapsed`, `recurUpTo` — need to know
// what the clock said when the run began, which an attempt counter cannot
// carry.
//
// So a schedule is now a state machine: `scheduleStep` takes a state and an
// input and answers with a decision — continue, with an output and a delay, or
// stop. One type drives both `retry`, whose input is the error, and `repeat`,
// whose input is the value, which is what makes `whileInput`, `untilInput`,
// `whileOutput` and `compose` one set of combinators rather than two.
//
// The state is one flat record shared by every arm of a tree rather than a
// nested per-arm structure, and nothing is lost by that: every arm here is a
// function of the same three facts — how many decisions have been made, when
// the first was asked for, and what the last one produced. A composite steps
// its children with the state it was given and rebuilds the next one itself, so
// `intersect` and `compose` do not have to thread two.
//
// There is deliberately no "when the previous decision was" in it, and that is
// the difference between a period and a gap. A `fixed` schedule measured
// against the previous decision accumulates every attempt's own duration —
// four hundred milliseconds of drift per attempt, until a one-second poll is
// running every one and a half — so it is measured against the first reading
// and the count instead, which is the timetable a caller asking for a period
// meant.
//
// # Time is an argument, not an import
//
// The same rule the random factor already followed, now load-bearing for the
// timetable arms. `scheduleStep` takes `now`; it does not read a clock. A
// `fixed` schedule under a step that overran its period, a `windowed` one
// aligned to a boundary, an `elapsed` output — all of them can be asked for
// their whole behaviour by naming the milliseconds, with no clock to stub, no
// sleep to flake and no seed to hope about. `retry` and `repeat` read
// `Date.now()` once per decision and pass it in.
//
// # What a schedule decides, and what it still does not
//
// A schedule now says *whether* as well as *when*, which is what `whileInput`
// and `untilInput` are. What it still does not decide is whether a *cause* is
// worth another attempt: only `retry` knows that a defect is a bug and an
// interruption is a decision already taken, and that repeating either just
// repeats it. That judgement stays in `index.js`, which is why nothing here
// mentions `Cause`.
//
// `retry`'s and `repeat`'s predicate parameters stay too, and they are not a
// second way to say `whileInput`. They are the same judgement in a different
// place: a policy that a config file could name, and a condition the call site
// knows. Both must allow an attempt for one to happen.
//
// This module was total. It is now total *given total predicates*, and that is
// the one thing #327 cost: `whileInput` holds a caller's function, and a
// function can throw. `retry` and `repeat` catch around the step and turn a
// throwing predicate into a defect, exactly as they already did for their own
// predicate parameters, so a bug in a predicate is reported as a bug rather
// than as an extra attempt or a silent stop.
//
// The same argument still rules out Effect's `*Effect` schedule combinators —
// `whileInputEffect`, `mapEffect` — which put an `Effect` inside a schedule and
// drag the runtime back in here. A predicate is a function this module calls;
// an `Effect` is a program only the runtime can run.
//
// # Why the output is a number and not a type parameter
//
// Effect's type is `Schedule<Out, In, R>`. `R` is deliberately absent — a
// schedule that needs a service forces the runtime dependency back into this
// module — and `Out` is absent for a reason worth writing down, because it
// looks at first like an oversight.
//
// A schedule here is a data union written as a literal:
// `{ kind: "exponential", baseMillis: 100 }`, which survives a JSON round trip
// through a config file. A union arm cannot constrain a type parameter it does
// not mention, so with an `Out` parameter that literal would inhabit
// `Schedule<string, In>` as happily as `Schedule<number, In>` and `repeat`
// would promise a `string` where the run time has a number. The escapes do not
// work either: a phantom field has to be optional for the literal to stay
// writable, and an optional phantom constrains nothing.
//
// `In` has no such problem, and the difference is variance rather than luck.
// `In` is contravariant, so an arm that ignores its input — every timetable arm
// — genuinely *is* usable at any input type, and an arm that reads one says so
// in its predicate's signature. The parameter that can be checked is here and
// the one that cannot is not, which is the same rule that keeps `any` out of
// this package.
//
// So the output is a number, and every arm documents which number it is. That
// is a real loss against Effect — `Schedule.map` over an output type is not
// expressible — and it buys one thing back: `compose` needs to name the
// intermediate type between two schedules, and because the output type is
// fixed it can. With an existential `Out`, `compose` would need a type variable
// a union arm cannot bind, which is exactly why `mapInput` is absent below.
//
// # Reading it
//
// `switch` and not `match`, for the reason the old `scheduleDelay` recorded:
// several arms have optional properties, and a `match` object pattern that
// binds one does not cover the case where it is absent — the checker reports
// the match as inexhaustive, and it is right to. A wider union makes that more
// true rather than less. Filed as #205.

/**
 * A policy: given what has happened so far and the latest input, whether to go
 * again and how long to wait first.
 *
 * `In` is what the schedule gets to look at — the error for `retry`, the value
 * for `repeat` — and is contravariant, so a policy that ignores its input can
 * be used with any.
 *
 * Every arm's *output* is a number, and which number differs by arm: a
 * schedule whose point is a count reports the count, and one whose point is a
 * duration reports the duration it chose. `repeat` gives that number back.
 *
 * The timetable, none of which reads its input:
 *
 * - `recurs` — up to `times` more attempts with no delay between them; outputs
 *   the number of attempts so far.
 * - `spaced` — a fixed *gap* after each attempt, for ever; outputs the count.
 * - `fixed` — a fixed *period*, for ever: a step that took 400ms of a
 *   one-second period waits 600ms and not a second, and one that overran waits
 *   not at all. Outputs the count. This is what an attempt counter could not
 *   express, because it needs to know when the last attempt started.
 * - `windowed` — a fixed period aligned to boundaries from the start rather
 *   than to the previous attempt, so a step that overran one window waits for
 *   the next boundary rather than starting immediately. Outputs the count.
 * - `exponential` — `baseMillis` multiplied by `factorPercent / 100` per
 *   attempt, defaulting to doubling. A percentage rather than a float because
 *   1.5 written as `150` survives a JSON round trip through a config file
 *   without becoming `1.4999999999999998`. Outputs the delay.
 * - `fibonacci` — `baseMillis` times the Fibonacci number for the attempt,
 *   which grows more gently than doubling. Outputs the delay.
 * - `upTo` — one more attempt after waiting `millis`, then stop. Outputs the
 *   delay.
 * - `elapsed` — never waits and never stops; outputs the milliseconds since
 *   the first decision, which is what makes it worth intersecting with.
 * - `count` — never waits and never stops; outputs the number of attempts.
 * - `recurUpTo` — no delay, until `millis` have passed since the first
 *   decision; outputs the elapsed milliseconds.
 *
 * The combinators over a schedule:
 *
 * - `intersect` — go again only while *both* sides would, waiting the longer of
 *   the two, and outputting whichever side's wait won.
 * - `union` — go again while *either* side would, waiting the shorter, and
 *   outputting whichever side's wait won.
 * - `maxDelay` — another schedule with its wait capped, which is how an
 *   exponential policy is kept from waiting an hour on its twelfth attempt.
 * - `jittered` — another schedule with each wait scaled by a random factor
 *   between `minPercent` and `maxPercent`, defaulting to 80 and 120. Without
 *   it every client that failed at the same moment retries at the same moment,
 *   for ever, which is what turns a brief outage into a sustained one.
 *   `maxDelay` caps the wait; nothing else spreads it.
 * - `compose` — pipe one schedule's output into another's input: the first
 *   decides on the caller's input, the second decides on the first's number.
 *   Goes again while both would, waiting the longer, and outputs the second's
 *   number. `elapsed` as the first, with an `untilInput` as the second, is how
 *   a policy gives up after half a second rather than after five attempts.
 *
 * The combinators that read a value:
 *
 * - `whileInput` — go again only while the predicate holds of the input.
 * - `untilInput` — stop as soon as it holds. "Poll until the job reports
 *   finished" is `untilInput` over a `spaced`.
 * - `whileOutput` — go again only while the predicate holds of the inner
 *   schedule's number, which is how "back off, but never past ten seconds of
 *   waiting" is said without a second schedule to intersect with.
 * - `untilOutput` — stop as soon as it holds.
 *
 * Effect's `mapInput` is deliberately absent. It adapts a schedule written for
 * one input type to another, which needs a type variable for the intermediate
 * input that a union arm cannot bind — Flow has no existentials — and the only
 * encoding that compiles makes the inner schedule's predicates take `mixed`,
 * which is the typing it existed to preserve. Writing the predicate you want is
 * shorter than the workaround and better typed than the encoding.
 *
 * Effect distinguishes `jittered` from `jitteredWith`; here they are one arm
 * with the bounds optional, because a schedule is data rather than a function
 * and there is nothing for a second name to mean.
 */
export type Schedule<in In> =
  | { readonly kind: "recurs", readonly times: number }
  | { readonly kind: "spaced", readonly millis: number }
  | { readonly kind: "fixed", readonly millis: number }
  | { readonly kind: "windowed", readonly millis: number }
  | { readonly kind: "exponential", readonly baseMillis: number, readonly factorPercent?: number }
  | { readonly kind: "fibonacci", readonly baseMillis: number }
  | { readonly kind: "upTo", readonly millis: number }
  | { readonly kind: "elapsed" }
  | { readonly kind: "count" }
  | { readonly kind: "recurUpTo", readonly millis: number }
  | { readonly kind: "intersect", readonly left: Schedule<In>, readonly right: Schedule<In> }
  | { readonly kind: "union", readonly left: Schedule<In>, readonly right: Schedule<In> }
  | { readonly kind: "maxDelay", readonly schedule: Schedule<In>, readonly millis: number }
  | {
      readonly kind: "jittered",
      readonly schedule: Schedule<In>,
      readonly minPercent?: number,
      readonly maxPercent?: number,
    }
  | { readonly kind: "compose", readonly first: Schedule<In>, readonly second: Schedule<number> }
  | {
      readonly kind: "whileInput",
      readonly schedule: Schedule<In>,
      readonly predicate: (input: In) => boolean,
    }
  | {
      readonly kind: "untilInput",
      readonly schedule: Schedule<In>,
      readonly predicate: (input: In) => boolean,
    }
  | {
      readonly kind: "whileOutput",
      readonly schedule: Schedule<In>,
      readonly predicate: (output: number) => boolean,
    }
  | {
      readonly kind: "untilOutput",
      readonly schedule: Schedule<In>,
      readonly predicate: (output: number) => boolean,
    };

/**
 * What a schedule knows about what has already happened.
 *
 * Four numbers, and every arm above is a function of them and the input.
 * `attempt` counts decisions already made, so the first is made with 0.
 * `startedAt` is a reading of whatever clock the caller passed to
 * `scheduleStep`, which is what makes `fixed` and `windowed` testable without
 * one. `output` is the last number the schedule produced, so a decision to stop
 * can report what the schedule had reached rather than nothing.
 */
export type ScheduleState = {
  readonly attempt: number,
  readonly startedAt: number,
  readonly output: number,
};

/**
 * Go again after `delayMillis`, or stop.
 *
 * `continue` carries the state to make the next decision with, so a caller
 * threads one value rather than four. Both arms carry an output: the number the
 * schedule reached, which is what `repeat` gives back.
 */
export type ScheduleDecision =
  | {
      readonly kind: "continue",
      readonly output: number,
      readonly delayMillis: number,
      readonly state: ScheduleState,
    }
  | { readonly kind: "done", readonly output: number };

/** The default `exponential` growth, as a percentage: doubling. */
const DEFAULT_FACTOR_PERCENT = 200;

/** The default `jittered` range, as percentages: Effect's [0.8, 1.2]. */
const DEFAULT_JITTER_MIN_PERCENT = 80;
const DEFAULT_JITTER_MAX_PERCENT = 120;

/** The factor a `scheduleStep` with no fourth argument jitters with. */
const UNJITTERED_FACTOR = 0.5;

/**
 * Where a schedule starts: nothing decided yet, and the clock as it reads now.
 *
 * `now` is the caller's number rather than a reading taken here, for the reason
 * the header gives: a schedule that reads a clock cannot be asked what it does
 * over an hour without waiting one.
 */
export function scheduleStart(now: number): ScheduleState {
  return { attempt: 0, startedAt: now, output: 0 };
}

/**
 * The next decision: go again after a wait, or stop.
 *
 * Total for every input, given predicates that are. A schedule that has stopped
 * reports the last number it reached rather than nothing, and one that
 * continues carries the state its successor should be asked with.
 *
 * `randomFactor` is the one number a `jittered` schedule needs, in [0, 1], and
 * it is passed in rather than drawn here. Every `jittered` arm in one tree
 * scales by the same factor, which is what a caller asking a schedule "how
 * long, this time" means. It defaults to the midpoint, so a call that does not
 * name one gets a jittered schedule's mean wait and stays as deterministic as
 * every other arm.
 */
export function scheduleStep<In>(
  schedule: Schedule<In>,
  state: ScheduleState,
  input: In,
  now: number,
  randomFactor: number = UNJITTERED_FACTOR,
): ScheduleDecision {
  switch (schedule.kind) {
    case "recurs":
      return state.attempt < schedule.times ? goOn(state, state.attempt + 1, 0) : stop(state);
    case "spaced":
      return goOn(state, state.attempt + 1, schedule.millis);
    case "fixed":
      // The next period boundary counted from the start, less where the clock
      // has got to. A step that used 400ms of a one-second period waits 600ms,
      // and one that overran waits not at all — but the attempt after it is
      // back on the timetable rather than a period late, because the boundary
      // is counted and not accumulated.
      return goOn(
        state,
        state.attempt + 1,
        state.startedAt + (state.attempt + 1) * schedule.millis - now,
      );
    case "windowed":
      return goOn(state, state.attempt + 1, windowRemaining(schedule.millis, state, now));
    case "exponential": {
      const delay = exponentialDelay(schedule.baseMillis, schedule.factorPercent, state.attempt);
      return goOn(state, delay, delay);
    }
    case "fibonacci": {
      const delay = Math.max(0, schedule.baseMillis * fibonacci(state.attempt));
      return goOn(state, delay, delay);
    }
    case "upTo":
      return state.attempt === 0
        ? goOn(state, Math.max(0, schedule.millis), schedule.millis)
        : stop(state);
    case "elapsed":
      return goOn(state, now - state.startedAt, 0);
    case "count":
      return goOn(state, state.attempt + 1, 0);
    case "recurUpTo": {
      const elapsed = now - state.startedAt;
      return elapsed < schedule.millis ? goOn(state, elapsed, 0) : stop(state);
    }
    case "intersect":
      return bothOf(schedule.left, schedule.right, state, input, now, randomFactor);
    case "union":
      return eitherOf(schedule.left, schedule.right, state, input, now, randomFactor);
    case "maxDelay":
      return cappedBy(schedule.schedule, schedule.millis, state, input, now, randomFactor);
    case "jittered":
      return spreadBy(schedule, state, input, now, randomFactor);
    case "compose":
      return pipedInto(schedule.first, schedule.second, state, input, now, randomFactor);
    case "whileInput":
      return schedule.predicate(input)
        ? scheduleStep(schedule.schedule, state, input, now, randomFactor)
        : stop(state);
    case "untilInput":
      return schedule.predicate(input)
        ? stop(state)
        : scheduleStep(schedule.schedule, state, input, now, randomFactor);
    case "whileOutput":
      return decidedOnOutput(
        schedule.schedule,
        schedule.predicate,
        state,
        input,
        now,
        randomFactor,
      );
    default:
      return decidedOnOutput(
        schedule.schedule,
        (output: number) => !schedule.predicate(output),
        state,
        input,
        now,
        randomFactor,
      );
  }
}

/**
 * Go again: the decision, and the state the next one is made with.
 *
 * The delay is clamped and rounded here rather than in fifteen arms, so a
 * schedule cannot produce a negative wait however its arithmetic went — which
 * is what `fixed` relies on for a step that overran its period.
 */
function goOn(state: ScheduleState, output: number, delayMillis: number): ScheduleDecision {
  const waited = Math.max(0, Math.round(delayMillis));
  return {
    kind: "continue",
    output,
    delayMillis: waited,
    state: { attempt: state.attempt + 1, startedAt: state.startedAt, output },
  };
}

/** Stop, reporting the last number the schedule reached. */
function stop(state: ScheduleState): ScheduleDecision {
  return { kind: "done", output: state.output };
}

/** How long is left of the window this moment falls in. */
function windowRemaining(millis: number, state: ScheduleState, now: number): number {
  const window = Math.max(1, millis);
  return window - ((now - state.startedAt) % window);
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

/**
 * Both sides must still want another attempt, and the longer wait wins.
 *
 * The children are stepped with the state this arm was given rather than with
 * two states of their own, which is what the flat state record buys: their
 * successor states would be identical anyway, because the next state is a
 * function of this one and `now`.
 */
function bothOf<In>(
  left: Schedule<In>,
  right: Schedule<In>,
  state: ScheduleState,
  input: In,
  now: number,
  randomFactor: number,
): ScheduleDecision {
  const leftSide = scheduleStep(left, state, input, now, randomFactor);
  const rightSide = scheduleStep(right, state, input, now, randomFactor);
  if (leftSide.kind === "done" || rightSide.kind === "done") {
    return stop(state);
  }
  return leftSide.delayMillis >= rightSide.delayMillis
    ? goOn(state, leftSide.output, leftSide.delayMillis)
    : goOn(state, rightSide.output, rightSide.delayMillis);
}

/** Either side is enough, and the shorter wait wins. */
function eitherOf<In>(
  left: Schedule<In>,
  right: Schedule<In>,
  state: ScheduleState,
  input: In,
  now: number,
  randomFactor: number,
): ScheduleDecision {
  const leftSide = scheduleStep(left, state, input, now, randomFactor);
  const rightSide = scheduleStep(right, state, input, now, randomFactor);
  if (leftSide.kind === "done") {
    return rightSide;
  }
  if (rightSide.kind === "done") {
    return leftSide;
  }
  return leftSide.delayMillis <= rightSide.delayMillis
    ? goOn(state, leftSide.output, leftSide.delayMillis)
    : goOn(state, rightSide.output, rightSide.delayMillis);
}

/**
 * Cap a wait without changing when the inner schedule gives up.
 *
 * A schedule that has stopped stays stopped: capping the delay of a decision to
 * stop would turn "give up" into "wait and try for ever".
 */
function cappedBy<In>(
  schedule: Schedule<In>,
  millis: number,
  state: ScheduleState,
  input: In,
  now: number,
  randomFactor: number,
): ScheduleDecision {
  const inner = scheduleStep(schedule, state, input, now, randomFactor);
  return inner.kind === "done"
    ? inner
    : goOn(state, inner.output, Math.min(inner.delayMillis, millis));
}

/**
 * Spread a wait over a range, so clients that failed together do not retry
 * together.
 *
 * The factor scales the inner schedule's answer rather than replacing it, so a
 * jittered exponential still grows and a jittered schedule that has given up
 * stays given up — jitter is about when, and stopping is not a when.
 *
 * The bounds are percentages for the same reason `exponential`'s factor is:
 * 1.2 written as `120` survives a JSON round trip through a config file
 * without becoming `1.1999999999999997`.
 */
function spreadBy<In>(
  schedule: {
    readonly kind: "jittered",
    readonly schedule: Schedule<In>,
    readonly minPercent?: number,
    readonly maxPercent?: number,
  },
  state: ScheduleState,
  input: In,
  now: number,
  randomFactor: number,
): ScheduleDecision {
  const inner = scheduleStep(schedule.schedule, state, input, now, randomFactor);
  if (inner.kind === "done") {
    return inner;
  }
  const low = schedule.minPercent == null ? DEFAULT_JITTER_MIN_PERCENT : schedule.minPercent;
  const high = schedule.maxPercent == null ? DEFAULT_JITTER_MAX_PERCENT : schedule.maxPercent;
  const clamped = Math.min(1, Math.max(0, randomFactor));
  const spread = (inner.delayMillis * (low + (high - low) * clamped)) / 100;
  return goOn(state, inner.output, spread);
}

/**
 * One schedule's number as another's input.
 *
 * Both must still want another attempt, and the longer wait wins — the rule
 * `intersect` uses, for the same reason: a composition that ignored one side's
 * wait would be that side not being in the composition.
 */
function pipedInto<In>(
  first: Schedule<In>,
  second: Schedule<number>,
  state: ScheduleState,
  input: In,
  now: number,
  randomFactor: number,
): ScheduleDecision {
  const firstSide = scheduleStep(first, state, input, now, randomFactor);
  if (firstSide.kind === "done") {
    return stop(state);
  }
  const secondSide = scheduleStep(second, state, firstSide.output, now, randomFactor);
  if (secondSide.kind === "done") {
    return stop(state);
  }
  return goOn(state, secondSide.output, Math.max(firstSide.delayMillis, secondSide.delayMillis));
}

/**
 * Step the inner schedule, and stop if its number fails the predicate.
 *
 * The decision to stop reports the number that failed rather than the one
 * before it, because that is the number the schedule computed at the step where
 * it stopped and the one a caller is asking about.
 */
function decidedOnOutput<In>(
  schedule: Schedule<In>,
  keepGoing: (output: number) => boolean,
  state: ScheduleState,
  input: In,
  now: number,
  randomFactor: number,
): ScheduleDecision {
  const inner = scheduleStep(schedule, state, input, now, randomFactor);
  if (inner.kind === "done") {
    return inner;
  }
  return keepGoing(inner.output) ? inner : { kind: "done", output: inner.output };
}
