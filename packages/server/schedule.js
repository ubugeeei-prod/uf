// @flow
//
// `@uniflowed/server/schedule`: work no request starts.
//
// `./queue.js` is the other half of this and the shape to read first: uf owns
// the job, the payload and the retry policy, and the deployment owns where the
// work waits. A schedule splits the same way. uf owns *when* — the expression,
// what it means, and not running a minute twice — and the deployment owns
// *being there* to run it. See ubugeeei-prod/uf#531.
//
// # What uf provides
//
//   * **`defineSchedule`** — a name, an expression and a function. The name is
//     what a platform's own scheduler names when it calls back into the
//     application, which is why a schedule is a value with a name rather than
//     a closure, exactly as `defineJob` is.
//   * **The expression's meaning**, in `./internal/cron.js`: five fields, UTC,
//     and POSIX's rule about which day wins.
//   * **`createScheduler`**, which is the part that decides a schedule is due
//     and refuses to decide it twice for the same minute.
//
// # What the deployment must provide
//
// Something alive at the right minute, and no configuration flag can supply it.
//
// A `node`, `bun` or `container` deployment holds a process, so uf can tick the
// scheduler itself — `processScheduler` below, which is honest about being a
// `setInterval` and nothing more: a restart between two ticks misses whatever
// was due in the gap, and two instances behind a load balancer each run every
// schedule. That is a legitimate small deployment, and it says so on the value
// as `triggered: false` so that an adapter can refuse it rather than a person
// having to have read this paragraph.
//
// A `serverless` or `edge` deployment has no such process. Its platform has a
// scheduler of its own — EventBridge, Cloudflare's `[triggers]`, Vercel's
// `crons` — and what it needs from uf is the *configuration* naming this
// schedule and the route that answers it. That is `triggered: true`, and it is
// the half of #531 that is not written yet: emitting it per platform is
// tracked there, and until it exists those targets refuse a schedule at the
// point the host is wired rather than building a deployment whose scheduled
// work silently never runs.
//
// # Why not a `setTimeout` per schedule
//
// Because the interesting failure is a process that was asleep. A timer armed
// for the next occurrence is wrong across a suspend, a laptop lid, and a
// container that was throttled to zero — it fires late and then computes the
// *next* occurrence from the late time. Asking "is this minute one of yours"
// on a tick is right in all three cases, and the cost is a set lookup a minute.

import type { Instant } from "@uniflowed/core/temporal";
import { Temporal } from "@uniflowed/core/temporal";

import type { SchedulerBackend } from "./internal/capabilities.js";
import type { Cron } from "./internal/cron.js";
import { cronMatches, parseCron } from "./internal/cron.js";
import type { Logger } from "./internal/log.js";
import { processLogger } from "./log.js";

/** A named schedule: when it runs, and what runs. */
export type Schedule = {|
  readonly name: string,
  readonly cron: Cron,
  readonly run: () => mixed,
|};

// The backend type lives beside the other capabilities and is re-exported
// here, the way `./queue.js` re-exports `QueueBackend`: an adapter reads it and
// this module only produces one, so a second definition would be the drift
// rather than the convenience.
export type { SchedulerBackend } from "./internal/capabilities.js";

/**
 * uf's own scheduler: a tick in this process.
 *
 * `triggered: false`, which is what makes a target that does not keep a
 * process refuse it.
 */
export const processScheduler: SchedulerBackend = Object.freeze({
  name: "process",
  triggered: false,
});

/**
 * Declare a schedule.
 *
 * The expression is parsed here rather than on the first tick, so a typo is a
 * module that fails to load instead of a schedule that quietly never matches.
 *
 * @throws SyntaxError when `cron` is not a five-field expression uf accepts.
 */
export function defineSchedule(options: {|
  readonly name: string,
  readonly cron: string,
  readonly run: () => mixed,
|}): Schedule {
  if (options.name.trim() === "") {
    throw new TypeError("a schedule needs a name: it is what a platform's scheduler calls back");
  }
  return { name: options.name, cron: parseCron(options.cron), run: options.run };
}

/** What a scheduler did with one minute. */
export type Tick = {|
  readonly ran: $ReadOnlyArray<string>,
  readonly failed: $ReadOnlyArray<string>,
|};

/**
 * Decide and run whatever is due, once per minute at most.
 *
 * `tick` is the whole interface, and `start` is a convenience over it. A host
 * that has its own loop — a platform scheduler calling in, or a test — drives
 * `tick` with the instant it means, and gets the same decisions.
 */
export function createScheduler(options: {|
  readonly schedules: $ReadOnlyArray<Schedule>,
  readonly log?: Logger,
|}): {|
  readonly tick: (instant: Instant) => Promise<Tick>,
  readonly start: () => () => void,
|} {
  const log = options.log ?? processLogger();
  // The last minute each schedule ran, so a tick every thirty seconds does not
  // run a schedule twice — and so a tick that is late by ten seconds still
  // runs the minute it was late for.
  const lastRun: Map<string, string> = new Map();

  const tick = async (instant: Instant): Promise<Tick> => {
    const minute = minuteOf(instant);
    const ran: Array<string> = [];
    const failed: Array<string> = [];
    for (const schedule of options.schedules) {
      if (lastRun.get(schedule.name) === minute) continue;
      if (!cronMatches(schedule.cron, instant)) continue;
      lastRun.set(schedule.name, minute);
      try {
        await schedule.run();
        ran.push(schedule.name);
      } catch (error) {
        // Logged and carried past, not rethrown: one schedule that throws must
        // not stop the others due in the same minute, and there is no caller
        // above a tick to catch it.
        log.error("schedule failed", { error, schedule: schedule.name });
        failed.push(schedule.name);
      }
    }
    return { ran, failed };
  };

  const start = (): (() => void) => {
    // Twice a minute, so a tick that drifts still lands inside every minute.
    // The interval is unref'd where the host allows it: a scheduler must not be
    // the reason a process refuses to exit.
    const timer = setInterval(() => {
      void tick(Temporal.Now.instant());
    }, 30_000);
    if (typeof timer === "object" && timer != null && typeof timer.unref === "function") {
      timer.unref();
    }
    return () => {
      clearInterval(timer);
    };
  };

  return { tick, start };
}

/** The minute an instant is in, as the key `tick` remembers it by. */
function minuteOf(instant: Instant): string {
  const at = instant.toZonedDateTimeISO("UTC");
  return `${String(at.year)}-${String(at.month)}-${String(at.day)}T${String(at.hour)}:${String(at.minute)}`;
}

/**
 * Start `schedules` ticking, and hand back the way to stop.
 *
 * The two lines every host that keeps a process needs, in one place: both
 * `./node.js` and `./bun.js` take an optional list on `serve` and neither
 * should own the decision about how often a tick happens.
 *
 * A host given no schedules pays nothing — no timer is armed, which is the
 * same bargain `installInterception` makes for module mocks.
 */
export function startSchedules(
  schedules: $ReadOnlyArray<Schedule> | void,
  log: Logger,
): () => void {
  if (schedules == null || schedules.length === 0) {
    return () => {};
  }
  log.info("schedules started", { count: schedules.length });
  return createScheduler({ schedules, log }).start();
}
