// @flow
//
// The clock a test controls.
//
// Every test here installs and removes the clock itself rather than in a hook,
// because a leaked fake clock is the failure mode that matters most: the next
// file's `setTimeout` never fires and the run hangs with no explanation.

import { afterEach, describe, expect, it, vi } from "@uniflowed/test";

afterEach(() => {
  vi.useRealTimers();
});

describe("installing and removing", () => {
  it("reports whether it is installed", () => {
    expect(vi.isFakeTimers()).toBe(false);
    vi.useFakeTimers();
    expect(vi.isFakeTimers()).toBe(true);
    vi.useRealTimers();
    expect(vi.isFakeTimers()).toBe(false);
  });

  it("puts the real globals back exactly", () => {
    const realTimeout = globalThis.setTimeout;
    const RealDate = globalThis.Date;

    vi.useFakeTimers();
    expect(globalThis.setTimeout).not.toBe(realTimeout);

    vi.useRealTimers();
    expect(globalThis.setTimeout).toBe(realTimeout);
    expect(globalThis.Date).toBe(RealDate);
  });

  it("installing twice is not an error and does not double-save", () => {
    const realTimeout = globalThis.setTimeout;

    vi.useFakeTimers();
    vi.useFakeTimers();
    vi.useRealTimers();

    expect(globalThis.setTimeout).toBe(realTimeout);
  });
});

describe("setTimeout", () => {
  it("does not fire until the clock reaches it", () => {
    vi.useFakeTimers();
    let fired = false;
    setTimeout(() => {
      fired = true;
    }, 100);

    vi.advanceTimersByTime(99);
    expect(fired).toBe(false);

    vi.advanceTimersByTime(1);
    expect(fired).toBe(true);
  });

  it("fires in due order, and by scheduling order within an instant", () => {
    vi.useFakeTimers();
    const order: Array<string> = [];
    setTimeout(() => order.push("late"), 20);
    setTimeout(() => order.push("first at ten"), 10);
    setTimeout(() => order.push("second at ten"), 10);

    vi.advanceTimersByTime(20);

    expect(order).toEqual(["first at ten", "second at ten", "late"]);
  });

  it("passes the extra arguments through", () => {
    vi.useFakeTimers();
    let seen: mixed = null;
    setTimeout(
      (a: mixed, b: mixed) => {
        seen = [a, b];
      },
      1,
      "x",
      2,
    );

    vi.advanceTimersByTime(1);

    expect(seen).toEqual(["x", 2]);
  });

  it("can be cancelled", () => {
    vi.useFakeTimers();
    let fired = false;
    const id = setTimeout(() => {
      fired = true;
    }, 10);

    clearTimeout(id);
    vi.advanceTimersByTime(100);

    expect(fired).toBe(false);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("fires a timer scheduled by another timer inside the same advance", () => {
    vi.useFakeTimers();
    const order: Array<string> = [];
    setTimeout(() => {
      order.push("outer");
      setTimeout(() => order.push("inner"), 10);
    }, 10);

    vi.advanceTimersByTime(20);

    expect(order).toEqual(["outer", "inner"]);
  });

  it("lands on the requested instant even when nothing was due", () => {
    // Two advances of 50 have to be the same as one of 100, or a test that
    // splits an advance sees different timers fire.
    vi.useFakeTimers();
    vi.setSystemTime(0);
    let at = -1;
    setTimeout(() => {
      at = Date.now();
    }, 100);

    vi.advanceTimersByTime(50);
    vi.advanceTimersByTime(50);

    expect(at).toBe(100);
  });
});

describe("setInterval", () => {
  it("repeats", () => {
    vi.useFakeTimers();
    let ticks = 0;
    setInterval(() => {
      ticks += 1;
    }, 10);

    vi.advanceTimersByTime(35);

    expect(ticks).toBe(3);
  });

  it("stops when cleared", () => {
    vi.useFakeTimers();
    let ticks = 0;
    const id = setInterval(() => {
      ticks += 1;
    }, 10);

    vi.advanceTimersByTime(20);
    clearInterval(id);
    vi.advanceTimersByTime(100);

    expect(ticks).toBe(2);
  });

  it("advances by at least a tick when the delay is zero", () => {
    // A zero-delay interval scheduled at the same instant forever would never
    // let the clock move.
    vi.useFakeTimers();
    let ticks = 0;
    const id = setInterval(() => {
      ticks += 1;
    }, 0);

    vi.advanceTimersByTime(3);
    clearInterval(id);

    expect(ticks).toBeGreaterThan(0);
  });
});

describe("running the queue", () => {
  it("runAllTimers drains everything, including what the callbacks add", () => {
    vi.useFakeTimers();
    const order: Array<string> = [];
    setTimeout(() => {
      order.push("first");
      setTimeout(() => order.push("second"), 1000);
    }, 10);

    vi.runAllTimers();

    expect(order).toEqual(["first", "second"]);
  });

  it("runAllTimers refuses an interval rather than hanging", () => {
    // An unbounded hang has to be killed; a failure names the problem.
    vi.useFakeTimers();
    setInterval(() => {}, 1);

    expect(() => vi.runAllTimers()).toThrow();
  });

  it("runOnlyPendingTimers fires what is queued and not what they queue", () => {
    vi.useFakeTimers();
    const order: Array<string> = [];
    setTimeout(() => {
      order.push("queued");
      setTimeout(() => order.push("added later"), 1);
    }, 10);

    vi.runOnlyPendingTimers();

    expect(order).toEqual(["queued"]);
  });

  it("advanceTimersToNextTimer fires exactly one, however far away", () => {
    vi.useFakeTimers();
    const order: Array<string> = [];
    setTimeout(() => order.push("near"), 10);
    setTimeout(() => order.push("far"), 10_000);

    vi.advanceTimersToNextTimer();
    expect(order).toEqual(["near"]);

    vi.advanceTimersToNextTimer();
    expect(order).toEqual(["near", "far"]);
  });

  it("counts what is waiting", () => {
    vi.useFakeTimers();
    setTimeout(() => {}, 1);
    setTimeout(() => {}, 2);

    expect(vi.getTimerCount()).toBe(2);
    vi.advanceTimersByTime(1);
    expect(vi.getTimerCount()).toBe(1);
  });
});

describe("the async advance", () => {
  it("lets a callback's await continue before the next timer", async () => {
    // The whole reason the async form exists: without it, the second timer
    // fires before the first callback has resumed.
    vi.useFakeTimers();
    const order: Array<string> = [];

    setTimeout(async () => {
      order.push("first");
      await Promise.resolve();
      order.push("first resumed");
    }, 10);
    setTimeout(() => order.push("second"), 20);

    await vi.advanceTimersByTimeAsync(20);

    expect(order).toEqual(["first", "first resumed", "second"]);
  });
});

describe("the clock itself", () => {
  it("Date.now follows the fake clock", () => {
    vi.useFakeTimers();
    vi.setSystemTime(1_000);

    expect(Date.now()).toBe(1_000);
    vi.advanceTimersByTime(500);
    expect(Date.now()).toBe(1_500);
  });

  it("new Date() reads the clock, and a given date does not", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-01-01T00:00:00.000Z"));

    expect(new Date().toISOString()).toBe("2026-01-01T00:00:00.000Z");
    expect(new Date("2020-05-05T00:00:00.000Z").getUTCFullYear()).toBe(2020);
  });

  it("a faked date is still a Date", () => {
    // A subclass rather than a wrapper, so `instanceof` and every method come
    // along unchanged.
    vi.useFakeTimers();

    expect(new Date() instanceof Date).toBe(true);
    expect(typeof new Date().toISOString()).toBe("string");
  });

  it("setSystemTime moves the clock without firing anything", () => {
    vi.useFakeTimers();
    let fired = false;
    setTimeout(() => {
      fired = true;
    }, 10);

    vi.setSystemTime(Date.now() + 10_000);

    expect(fired).toBe(false);
    expect(vi.getTimerCount()).toBe(1);
  });

  it("reports the mocked time, and nothing when the clock is real", () => {
    expect(vi.getMockedSystemTime()).toBe(null);

    vi.useFakeTimers();
    vi.setSystemTime(42);

    expect(vi.getMockedSystemTime()?.getTime()).toBe(42);
  });
});
