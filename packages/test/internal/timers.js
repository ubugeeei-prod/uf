// @flow
//
// Internal to `@uniflowed/test`: a clock a test controls.
//
// A test about "after five minutes the session expires" should not take five
// minutes, and a test that waits a real 50ms to see a debounce is a test that
// fails on a loaded machine. Fake timers replace the scheduling globals with a
// queue the test advances by hand, so elapsed time becomes an input rather than
// something to wait for.
//
// # What is faked
//
// `setTimeout`, `setInterval`, `setImmediate` and their `clear` partners, plus
// `Date.now` and `new Date()` with no arguments. Not `queueMicrotask` and not
// promises: a microtask is not scheduled *in time*, it runs at the end of the
// current turn, and pretending otherwise would let a test claim a promise
// resolved "after 100ms" when the two have nothing to do with each other. The
// async advance methods are the honest way to let microtasks run in between.
//
// # Why the ids are numbers
//
// Node's `setTimeout` returns a `Timeout` object and a browser's returns a
// number, and code written for either passes what it got straight to
// `clearTimeout`. A number works in both places — `clearTimeout` only ever
// looks the value up — and a test that stores one in a `Map` keyed by number
// keeps working.

/** One scheduled callback. */
type Task = {
  readonly id: number,
  /** When it is due, on the fake clock. */
  due: number,
  /** How often it repeats, or `null` for a one-shot. */
  readonly every: number | null,
  readonly body: (...args: $ReadOnlyArray<mixed>) => mixed,
  readonly args: $ReadOnlyArray<mixed>,
  /** Ordering among tasks due at the same instant: first scheduled, first run. */
  readonly sequence: number,
};

/** The globals a clock replaces, so they can be put back exactly. */
type Saved = {
  readonly setTimeout: mixed,
  readonly clearTimeout: mixed,
  readonly setInterval: mixed,
  readonly clearInterval: mixed,
  readonly setImmediate: mixed,
  readonly clearImmediate: mixed,
  readonly Date: mixed,
};

/**
 * How many times a run loop will fire a timer before deciding it will not stop.
 *
 * An interval reschedules itself, so `runAllTimers` on one would never finish.
 * A bound turns an unbounded hang into a failure that names the problem, which
 * is the difference between a test suite that stops and one that has to be
 * killed.
 */
const RUNAWAY_LIMIT = 10_000;

let installed: Saved | null = null;
let now = 0;
let nextId = 1;
let sequence = 0;
let tasks: Array<Task> = [];

/** Whether a clock is currently installed. */
export function isFaked(): boolean {
  return installed != null;
}

/** The globals to patch, in one place so install and restore cannot diverge. */
function host(): $FlowFixMe {
  return globalThis;
}

/**
 * Replace the scheduling globals with a clock this module controls.
 *
 * `now` starts at the real current time rather than at zero, so a test that
 * formats a date sees a plausible one — and `setSystemTime` is how a test that
 * cares says which.
 */
export function useFakeTimers(): void {
  if (installed != null) {
    return;
  }
  const global = host();
  installed = {
    setTimeout: global.setTimeout,
    clearTimeout: global.clearTimeout,
    setInterval: global.setInterval,
    clearInterval: global.clearInterval,
    setImmediate: global.setImmediate,
    clearImmediate: global.clearImmediate,
    Date: global.Date,
  };

  now = Date.now();
  tasks = [];
  nextId = 1;
  sequence = 0;

  global.setTimeout = (body: $FlowFixMe, delay?: number, ...args: $ReadOnlyArray<mixed>) =>
    schedule(body, delay ?? 0, null, args);
  global.setInterval = (body: $FlowFixMe, delay?: number, ...args: $ReadOnlyArray<mixed>) =>
    // A zero-delay interval would be scheduled at the same instant forever, so
    // it advances by one tick — which is what every runtime does with it.
    schedule(body, delay ?? 0, Math.max(delay ?? 0, 1), args);
  // `setImmediate` is "before any timer, after this turn", which on a fake
  // clock is a zero-delay timer that sorts ahead by having been scheduled at
  // the current instant.
  global.setImmediate = (body: $FlowFixMe, ...args: $ReadOnlyArray<mixed>) =>
    schedule(body, 0, null, args);
  global.clearTimeout = cancel;
  global.clearInterval = cancel;
  global.clearImmediate = cancel;
  global.Date = fakeDate(installed.Date as $FlowFixMe);
}

/** Put the real scheduling globals back. */
export function useRealTimers(): void {
  if (installed == null) {
    return;
  }
  const global = host();
  global.setTimeout = installed.setTimeout;
  global.clearTimeout = installed.clearTimeout;
  global.setInterval = installed.setInterval;
  global.clearInterval = installed.clearInterval;
  global.setImmediate = installed.setImmediate;
  global.clearImmediate = installed.clearImmediate;
  global.Date = installed.Date;
  installed = null;
  tasks = [];
}

/**
 * A `Date` whose "now" is the fake clock's.
 *
 * A proxy over the real constructor rather than a subclass of it, and the
 * difference is three bugs rather than a preference:
 *
 * * `Date()` — called as a function, with no `new` — returns a string in every
 *   runtime. A `class` cannot be called that way at all, so a subclass turned
 *   every such call into a `TypeError`, under fake timers only.
 * * `before instanceof Date`, for a date built *before* the clock was faked,
 *   was false: the object's prototype chain runs through the real `Date` and a
 *   subclass's prototype is not in it. Anything branching on that — including
 *   `setSystemTime` below, once — silently took the wrong branch. A proxy has
 *   no prototype of its own, so the check is the real one.
 * * `Date.name` was `"FakeDate"`, and a subclass's own `toString` reads as
 *   class source rather than native code.
 *
 * Only `now` is replaced. `parse`, `UTC`, and every prototype method are the
 * real ones, reached through the proxy.
 */
function fakeDate(Real: $FlowFixMe): $FlowFixMe {
  return new Proxy(Real, {
    // `Date(anything)` ignores its arguments and returns the current time as a
    // string, which on this clock is the fake one.
    apply(): string {
      return new Real(now).toString();
    },

    construct(target: $FlowFixMe, args: $ReadOnlyArray<mixed>, newTarget: $FlowFixMe) {
      // Only the no-argument form reads the clock; every other form is
      // constructing a specific date and has nothing to do with "now".
      const actual = args.length === 0 ? [now] : args;
      // `newTarget` rather than `Real`, so a subclass of the faked `Date` gets
      // its own prototype instead of the real one's.
      return Reflect.construct(Real, actual, newTarget === undefined ? Real : newTarget);
    },

    get(target: $FlowFixMe, property: $FlowFixMe, receiver: $FlowFixMe): $FlowFixMe {
      if (property === "now") {
        return fakeNow;
      }
      return Reflect.get(target, property, receiver);
    },
  });
}

/** `Date.now`, as its own function so the proxy hands back a stable identity. */
function fakeNow(): number {
  return now;
}

/** Put a task on the queue and hand back its id. */
function schedule(
  body: $FlowFixMe,
  delay: number,
  every: number | null,
  args: $ReadOnlyArray<mixed>,
): number {
  const id = nextId;
  nextId += 1;
  sequence += 1;
  tasks.push({
    id,
    due: now + Math.max(delay, 0),
    every,
    body,
    args,
    sequence,
  });
  return id;
}

/** Take a task off the queue. Unknown ids are ignored, as the real ones are. */
function cancel(id: mixed): void {
  tasks = tasks.filter((task) => task.id !== id);
}

/** How many timers are waiting. */
export function getTimerCount(): number {
  return tasks.length;
}

/** The next task due, or `null`. Ties break by scheduling order. */
function nextTask(before: number): Task | null {
  let best: Task | null = null;
  for (const task of tasks) {
    if (task.due > before) {
      continue;
    }
    if (
      best == null ||
      task.due < best.due ||
      (task.due === best.due && task.sequence < best.sequence)
    ) {
      best = task;
    }
  }
  return best;
}

/** Run one task, rescheduling it when it repeats. */
function fire(task: Task): void {
  now = task.due;
  if (task.every == null) {
    cancel(task.id);
  } else {
    task.due = now + task.every;
  }
  task.body(...task.args);
}

/** Raised when a run loop will not terminate. */
export class RunawayTimersError extends Error {
  constructor(method: string) {
    super(
      `uf.${method}: still firing after ${RUNAWAY_LIMIT} timers. ` +
        "A timer that reschedules itself never drains — advance the clock by a " +
        "fixed amount instead.",
    );
    this.name = "RunawayTimersError";
  }
}

/** Move the clock forward, firing everything that comes due. */
export function advanceTimersByTime(millis: number): void {
  const target = now + Math.max(millis, 0);
  let fired = 0;
  for (;;) {
    const task = nextTask(target);
    if (task == null) {
      break;
    }
    fired += 1;
    if (fired > RUNAWAY_LIMIT) {
      throw new RunawayTimersError("advanceTimersByTime");
    }
    fire(task);
  }
  // Land on the requested instant even when nothing was due there, so two
  // advances of 50ms are the same as one of 100ms.
  now = target;
}

/** The same, yielding to the microtask queue between timers. */
export async function advanceTimersByTimeAsync(millis: number): Promise<void> {
  const target = now + Math.max(millis, 0);
  let fired = 0;
  for (;;) {
    const task = nextTask(target);
    if (task == null) {
      break;
    }
    fired += 1;
    if (fired > RUNAWAY_LIMIT) {
      throw new RunawayTimersError("advanceTimersByTimeAsync");
    }
    fire(task);
    // The point of the async form: a callback that awaited something gets to
    // continue before the next timer fires.
    await Promise.resolve();
  }
  now = target;
}

/** Fire the next timer due, whenever it is due. */
export function advanceTimersToNextTimer(): void {
  const task = nextTask(Number.POSITIVE_INFINITY);
  if (task != null) {
    fire(task);
  }
}

/** Fire everything until the queue is empty. */
export function runAllTimers(): void {
  let fired = 0;
  for (;;) {
    const task = nextTask(Number.POSITIVE_INFINITY);
    if (task == null) {
      break;
    }
    fired += 1;
    if (fired > RUNAWAY_LIMIT) {
      throw new RunawayTimersError("runAllTimers");
    }
    fire(task);
  }
}

/**
 * Fire only what is already queued, not what those callbacks schedule.
 *
 * The way to drain an interval once: `runAllTimers` on one never terminates,
 * and this fires each waiting task exactly once.
 */
export function runOnlyPendingTimers(): void {
  const pending = [...tasks].sort((a, b) => a.due - b.due || a.sequence - b.sequence);
  for (const task of pending) {
    // A callback may have cancelled a later one, so check it is still queued.
    if (tasks.some((queued) => queued.id === task.id)) {
      fire(task);
    }
  }
}

/**
 * Move the wall clock without firing anything.
 *
 * Duck-typed rather than `instanceof Date`, and deliberately: `Date` here is
 * whatever is installed, so the check would be asking about the *fake* one
 * while the caller may be holding a date made before the clock was faked, or
 * one from another realm. Either answered `false`, fell through to the number
 * branch, and set the clock to an object — after which every comparison
 * against it was `false` and no timer ever came due.
 *
 * Anything that can say what instant it is, is an instant.
 */
export function setSystemTime(time: number | Date): void {
  now = typeof time === "number" ? time : Number((time as $FlowFixMe).getTime());
}

/** What the fake clock currently reads. */
export function getMockedSystemTime(): Date | null {
  return installed == null ? null : new (installed.Date as $FlowFixMe)(now);
}
