// @flow
//
// `@uniflowed/server/schedule`: when a schedule is due, and who may hold one.
//
// The two halves of ubugeeei-prod/uf#531 that exist. What is tested here is
// uf's half — the expression's meaning, and not running a minute twice — and
// the refusal that keeps the other half honest: a target with no process and
// no platform call must say so rather than build a deployment whose scheduled
// work never runs.
//
// Instants are written, never read from a clock. A schedule test that asked
// what time it is would pass or fail depending on when it ran, which is the
// one thing a test about time must not do.

import { describe, expect, it } from "@uniflowed/test";
import { Temporal } from "@uniflowed/core/temporal";
import {
  createScheduler,
  defineSchedule,
  processScheduler,
  startSchedules,
} from "@uniflowed/server/schedule";
import { nodeCapabilities } from "@uniflowed/server/node";
import { SCHEDULED_HEADER, createWorkerScheduled } from "@uniflowed/server/edge";
import { lambdaCapabilities } from "@uniflowed/server/lambda";

const at = (iso: string) => Temporal.Instant.from(iso);

/** Whether `cron` is due at `iso`, through the public surface. */
const due = (cron: string, iso: string): boolean => {
  const schedule = defineSchedule({ name: "s", cron, run: () => {} });
  // `createScheduler` is the only thing that reads a `Cron`, so asking it is
  // asking the code that ships rather than a matcher exported for a test.
  let ran = false;
  const scheduler = createScheduler({
    schedules: [
      defineSchedule({
        name: schedule.name,
        cron,
        run: () => {
          ran = true;
        },
      }),
    ],
    log: { info: () => {}, warn: () => {}, error: () => {}, debug: () => {} },
  });
  void scheduler.tick(at(iso));
  return ran;
};

describe("a cron expression", () => {
  it("refuses anything but five fields, and says how many it got", () => {
    expect(() => defineSchedule({ name: "s", cron: "* * * *", run: () => {} })).toThrow(
      /five fields/,
    );
    expect(() => defineSchedule({ name: "s", cron: "* * * * * *", run: () => {} })).toThrow(
      /has 6/,
    );
  });

  it("refuses a field outside its range, naming the field", () => {
    expect(() => defineSchedule({ name: "s", cron: "60 * * * *", run: () => {} })).toThrow(
      /minute field .* outside 0-59/,
    );
    expect(() => defineSchedule({ name: "s", cron: "* 24 * * *", run: () => {} })).toThrow(
      /hour field .* outside 0-23/,
    );
    expect(() => defineSchedule({ name: "s", cron: "* * 0 * *", run: () => {} })).toThrow(
      /day-of-month field .* outside 1-31/,
    );
  });

  // No wrap-around: `22-2` reads as "late at night" and every cron reads it as
  // empty. Twenty hours apart is too far to guess.
  it("refuses a backwards range rather than matching nothing", () => {
    expect(() => defineSchedule({ name: "s", cron: "* 22-2 * * *", run: () => {} })).toThrow(
      /counts backwards/,
    );
  });

  // The step would otherwise be dropped and `0 5/15 * * *` would run once a
  // day while looking hourly — the one refusal that is about a form uf could
  // have half-read rather than one it cannot read at all.
  it("refuses a step on a single value, and says what to write instead", () => {
    expect(() => defineSchedule({ name: "s", cron: "0 5/15 * * *", run: () => {} })).toThrow(
      /a step needs .* or a range before it/,
    );
    expect(() => defineSchedule({ name: "s", cron: "0 5/15 * * *", run: () => {} })).toThrow(
      /5-23\/15/,
    );
    // The two spellings that do carry a step still work.
    expect(due("0 */6 * * *", "2026-03-04T06:00:00Z")).toBe(true);
    expect(due("0 5-23/15 * * *", "2026-03-04T20:00:00Z")).toBe(true);
    expect(due("0 5-23/15 * * *", "2026-03-04T06:00:00Z")).toBe(false);
  });

  it("refuses the syntaxes it does not implement, instead of ignoring them", () => {
    for (const cron of ["@daily", "0 0 * * MON", "0 0 L * *", "0 0 * * 1#2"]) {
      expect(() => defineSchedule({ name: "s", cron, run: () => {} })).toThrow();
    }
  });

  it("matches a plain minute and hour, in UTC", () => {
    expect(due("30 9 * * *", "2026-03-04T09:30:00Z")).toBe(true);
    expect(due("30 9 * * *", "2026-03-04T09:31:00Z")).toBe(false);
    // The same wall-clock minute in another zone is a different instant, and
    // this is the assertion that the fields are read in UTC and not locally.
    expect(due("30 9 * * *", "2026-03-04T10:30:00+01:00")).toBe(true);
  });

  it("matches a list, a range and a step", () => {
    expect(due("0,30 * * * *", "2026-03-04T07:30:00Z")).toBe(true);
    expect(due("0,30 * * * *", "2026-03-04T07:15:00Z")).toBe(false);
    expect(due("* 9-17 * * *", "2026-03-04T17:00:00Z")).toBe(true);
    expect(due("* 9-17 * * *", "2026-03-04T18:00:00Z")).toBe(false);
    expect(due("*/15 * * * *", "2026-03-04T07:45:00Z")).toBe(true);
    expect(due("*/15 * * * *", "2026-03-04T07:46:00Z")).toBe(false);
    expect(due("0 0-6/3 * * *", "2026-03-04T03:00:00Z")).toBe(true);
    expect(due("0 0-6/3 * * *", "2026-03-04T04:00:00Z")).toBe(false);
  });

  // Temporal counts 1 = Monday … 7 = Sunday; cron counts 0 = Sunday … 6 =
  // Saturday. Sunday is where an off-by-one lands, so Sunday is asserted twice
  // — as `0` and as `7`, both of which every cron accepts.
  it("reads day-of-week the way cron does, including both spellings of Sunday", () => {
    // 2026-03-08 is a Sunday; 2026-03-09 is a Monday.
    expect(due("0 0 * * 0", "2026-03-08T00:00:00Z")).toBe(true);
    expect(due("0 0 * * 7", "2026-03-08T00:00:00Z")).toBe(true);
    expect(due("0 0 * * 0", "2026-03-09T00:00:00Z")).toBe(false);
    expect(due("0 0 * * 1", "2026-03-09T00:00:00Z")).toBe(true);
    expect(due("0 0 * * 6", "2026-03-07T00:00:00Z")).toBe(true);
  });

  // POSIX's rule, and the one everybody is surprised by once.
  it("matches either day when day-of-month and day-of-week are both restricted", () => {
    // The 1st of March 2026 is a Sunday, the 2nd a Monday, the 9th a Monday.
    const cron = "0 0 1 * 1";
    expect(due(cron, "2026-03-01T00:00:00Z")).toBe(true); // the 1st, not a Monday
    expect(due(cron, "2026-03-09T00:00:00Z")).toBe(true); // a Monday, not the 1st
    expect(due(cron, "2026-03-03T00:00:00Z")).toBe(false); // neither
  });

  it("requires the one restricted day when the other is a star", () => {
    expect(due("0 0 15 * *", "2026-03-15T00:00:00Z")).toBe(true);
    expect(due("0 0 15 * *", "2026-03-16T00:00:00Z")).toBe(false);
    expect(due("0 0 * * 3", "2026-03-04T00:00:00Z")).toBe(true); // a Wednesday
    expect(due("0 0 * * 3", "2026-03-05T00:00:00Z")).toBe(false);
  });

  it("matches the month field", () => {
    expect(due("0 0 1 3 *", "2026-03-01T00:00:00Z")).toBe(true);
    expect(due("0 0 1 4 *", "2026-03-01T00:00:00Z")).toBe(false);
  });
});

describe("the scheduler", () => {
  const quiet = { info: () => {}, warn: () => {}, error: () => {}, debug: () => {} };

  it("runs a schedule once for a minute, however often it is ticked", async () => {
    let runs = 0;
    const scheduler = createScheduler({
      schedules: [
        defineSchedule({
          name: "hourly",
          cron: "0 * * * *",
          run: () => {
            runs += 1;
          },
        }),
      ],
      log: quiet,
    });

    // Three ticks inside the same minute — which is what a 30-second interval
    // does, and what a late tick does after an early one.
    await scheduler.tick(at("2026-03-04T09:00:00Z"));
    await scheduler.tick(at("2026-03-04T09:00:29Z"));
    await scheduler.tick(at("2026-03-04T09:00:59Z"));
    expect(runs).toBe(1);

    // And the next occurrence still runs.
    await scheduler.tick(at("2026-03-04T10:00:00Z"));
    expect(runs).toBe(2);
  });

  it("runs a minute it was late for rather than skipping it", async () => {
    let runs = 0;
    const scheduler = createScheduler({
      schedules: [
        defineSchedule({
          name: "top",
          cron: "0 * * * *",
          run: () => {
            runs += 1;
          },
        }),
      ],
      log: quiet,
    });
    // The tick that should have happened at :00 arrives at :40. The minute is
    // still the minute the schedule names.
    await scheduler.tick(at("2026-03-04T09:00:40Z"));
    expect(runs).toBe(1);
  });

  it("reports what ran and carries past one that threw", async () => {
    const order: Array<string> = [];
    const scheduler = createScheduler({
      schedules: [
        defineSchedule({
          name: "first",
          cron: "* * * * *",
          run: () => {
            order.push("first");
          },
        }),
        defineSchedule({
          name: "broken",
          cron: "* * * * *",
          run: () => {
            throw new Error("nope");
          },
        }),
        defineSchedule({
          name: "third",
          cron: "* * * * *",
          run: () => {
            order.push("third");
          },
        }),
      ],
      log: quiet,
    });

    const tick = await scheduler.tick(at("2026-03-04T09:00:00Z"));
    // The one that threw did not stop the one after it: there is no caller
    // above a tick to catch for them.
    expect(order).toEqual(["first", "third"]);
    expect(tick.ran).toEqual(["first", "third"]);
    expect(tick.failed).toEqual(["broken"]);
  });

  it("awaits an async schedule before calling it run", async () => {
    let finished = false;
    const scheduler = createScheduler({
      schedules: [
        defineSchedule({
          name: "slow",
          cron: "* * * * *",
          run: async () => {
            await Promise.resolve();
            finished = true;
          },
        }),
      ],
      log: quiet,
    });
    const tick = await scheduler.tick(at("2026-03-04T09:00:00Z"));
    expect(finished).toBe(true);
    expect(tick.ran).toEqual(["slow"]);
  });

  // `lastRun` is keyed by name, so two schedules sharing one would take turns
  // suppressing each other — silently, and only in the minutes they overlap.
  it("refuses two schedules with one name rather than letting them suppress each other", () => {
    expect(() =>
      createScheduler({
        schedules: [
          defineSchedule({ name: "sweep", cron: "* * * * *", run: () => {} }),
          defineSchedule({ name: "sweep", cron: "0 * * * *", run: () => {} }),
        ],
        log: quiet,
      }),
    ).toThrow(/two schedules are named "sweep"/);
  });

  it("needs a name, because a platform's scheduler calls one back", () => {
    expect(() => defineSchedule({ name: "  ", cron: "* * * * *", run: () => {} })).toThrow(/name/);
  });
});

describe("which targets may hold a schedule", () => {
  it("lets a target that keeps a process tick one itself", () => {
    const capabilities = nodeCapabilities({ scheduler: processScheduler });
    expect(capabilities.scheduler?.name).toBe("process");
    expect(capabilities.scheduler?.triggered).toBe(false);
  });

  // The refusal #531 asks for, and the sentence matters: not "uf has no
  // scheduler" but "nothing would be running at that minute".
  it("refuses an in-process schedule on a target that keeps no process", () => {
    expect(() => lambdaCapabilities({ scheduler: processScheduler })).toThrow(
      /nothing would be running at the minute/,
    );
  });

  it("accepts one the platform triggers, on the same target", () => {
    const capabilities = lambdaCapabilities({
      scheduler: { name: "eventbridge", triggered: true },
    });
    expect(capabilities.scheduler?.triggered).toBe(true);
  });

  it("names no scheduler by default", () => {
    expect(nodeCapabilities().scheduler).toBe(null);
    expect(lambdaCapabilities().scheduler).toBe(null);
  });
});

describe("a host starting schedules", () => {
  const quiet = { info: () => {}, warn: () => {}, error: () => {}, debug: () => {} };

  // The bargain `serve` makes: a deployment that declares none pays for none.
  // Asserted because the alternative — an interval armed for an empty list —
  // is invisible until something asks why a process will not exit.
  it("arms nothing when there are no schedules", () => {
    const said: Array<string> = [];
    const stop = startSchedules(undefined, { ...quiet, info: (m) => said.push(m) });
    expect(said).toEqual([]);
    expect(typeof stop).toBe("function");
    stop();

    const stopEmpty = startSchedules([], quiet);
    expect(typeof stopEmpty).toBe("function");
    stopEmpty();
  });

  it("says how many it started, and hands back the way to stop", () => {
    const said: Array<{ message: string, count: mixed }> = [];
    const stop = startSchedules(
      [defineSchedule({ name: "one", cron: "* * * * *", run: () => {} })],
      { ...quiet, info: (message, fields) => said.push({ message, count: fields?.count }) },
    );
    expect(said).toEqual([{ message: "schedules started", count: 1 }]);
    // Stopping is the half a test needs: an interval nobody cleared keeps a
    // worker alive past the file that armed it.
    stop();
  });
});

describe("a schedule on a worker", () => {
  // What Cloudflare hands `scheduled()`, and what uf hands it back.
  const fired = (routes, cron, handle) => {
    const seen = [];
    const settled = [];
    const scheduled = createWorkerScheduled({
      routes,
      handle: async (request) => {
        seen.push(request);
        return handle == null ? new Response("ok") : handle(request);
      },
      beginRequest: () => ({
        context: { id: "test-id", route: null },
        run: (body) => body(),
        settle: async () => {},
      }),
    });
    return { scheduled, seen, settled, ctx: { waitUntil: (p) => settled.push(p) } };
  };

  it("dispatches the route the expression names, as a GET", async () => {
    const { scheduled, seen, ctx } = fired({ "*/15 * * * *": "/api/sweep" }, "*/15 * * * *");
    await scheduled({ cron: "*/15 * * * *" }, {}, ctx);

    expect(seen).toHaveLength(1);
    // A schedule is a request the platform makes: the same path, through the
    // same handler, so a route needs to know nothing about schedules.
    expect(new URL(seen[0].url).pathname).toBe("/api/sweep");
    expect(seen[0].method).toBe("GET");
  });

  it("names the expression that fired, on the request", async () => {
    const { scheduled, seen, ctx } = fired({ "0 6 * * 1": "/api/digest" });
    await scheduled({ cron: "0 6 * * 1" }, {}, ctx);
    expect(seen[0].headers.get(SCHEDULED_HEADER)).toBe("0 6 * * 1");
  });

  // `wrangler.json` is a file a person can edit after uf writes it, so a cron
  // added by hand is not a reason to fail an invocation — but it is a reason
  // to say so, because the alternative is firing into silence.
  it("drops a trigger no route claims, without throwing", async () => {
    const { scheduled, seen, ctx } = fired({ "*/15 * * * *": "/api/sweep" });
    await scheduled({ cron: "0 0 * * *" }, {}, ctx);
    expect(seen).toHaveLength(0);
  });

  it("settles the request through waitUntil where there is one", async () => {
    const { scheduled, settled, ctx } = fired({ "* * * * *": "/api/tick" });
    await scheduled({ cron: "* * * * *" }, {}, ctx);
    // `after()` work outlives the handler on a worker, which is what
    // `waitUntil` is for — dropping the promise would lose the work.
    expect(settled).toHaveLength(1);
    await Promise.all(settled);
  });

  it("does not let a failing route reject the invocation", async () => {
    const { scheduled, ctx } = fired({ "* * * * *": "/api/boom" }, undefined, () => {
      throw new Error("the database is on fire");
    });
    // Logged and swallowed: there is no caller above a scheduled invocation to
    // catch it, and a rejection here is a Worker error with no request behind
    // it — less legible than the line uf writes.
    await scheduled({ cron: "* * * * *" }, {}, ctx);
  });
});
