// @flow
//
// The clock a test controls.
//
// Every test here installs and removes the clock itself rather than in a hook,
// because a leaked fake clock is the failure mode that matters most: the next
// file's `setTimeout` never fires and the run hangs with no explanation.

import { afterEach, describe, expect, it, uft } from "@uniflowed/test";

afterEach(() => {
  uft.useRealTimers();
});

describe("installing and removing", () => {
  it("reports whether it is installed", () => {
    expect(uft.isFakeTimers()).toBe(false);
    uft.useFakeTimers();
    expect(uft.isFakeTimers()).toBe(true);
    uft.useRealTimers();
    expect(uft.isFakeTimers()).toBe(false);
  });

  it("puts the real globals back exactly", () => {
    const realTimeout = globalThis.setTimeout;
    const RealDate = globalThis.Date;

    uft.useFakeTimers();
    expect(globalThis.setTimeout).not.toBe(realTimeout);

    uft.useRealTimers();
    expect(globalThis.setTimeout).toBe(realTimeout);
    expect(globalThis.Date).toBe(RealDate);
  });

  it("installing twice is not an error and does not double-save", () => {
    const realTimeout = globalThis.setTimeout;

    uft.useFakeTimers();
    uft.useFakeTimers();
    uft.useRealTimers();

    expect(globalThis.setTimeout).toBe(realTimeout);
  });
});

describe("setTimeout", () => {
  it("does not fire until the clock reaches it", () => {
    uft.useFakeTimers();
    let fired = false;
    setTimeout(() => {
      fired = true;
    }, 100);

    uft.advanceTimersByTime(99);
    expect(fired).toBe(false);

    uft.advanceTimersByTime(1);
    expect(fired).toBe(true);
  });

  it("fires in due order, and by scheduling order within an instant", () => {
    uft.useFakeTimers();
    const order: Array<string> = [];
    setTimeout(() => order.push("late"), 20);
    setTimeout(() => order.push("first at ten"), 10);
    setTimeout(() => order.push("second at ten"), 10);

    uft.advanceTimersByTime(20);

    expect(order).toEqual(["first at ten", "second at ten", "late"]);
  });

  it("passes the extra arguments through", () => {
    uft.useFakeTimers();
    let seen: mixed = null;
    setTimeout(
      (a: mixed, b: mixed) => {
        seen = [a, b];
      },
      1,
      "x",
      2,
    );

    uft.advanceTimersByTime(1);

    expect(seen).toEqual(["x", 2]);
  });

  it("can be cancelled", () => {
    uft.useFakeTimers();
    let fired = false;
    const id = setTimeout(() => {
      fired = true;
    }, 10);

    clearTimeout(id);
    uft.advanceTimersByTime(100);

    expect(fired).toBe(false);
    expect(uft.getTimerCount()).toBe(0);
  });

  it("fires a timer scheduled by another timer inside the same advance", () => {
    uft.useFakeTimers();
    const order: Array<string> = [];
    setTimeout(() => {
      order.push("outer");
      setTimeout(() => order.push("inner"), 10);
    }, 10);

    uft.advanceTimersByTime(20);

    expect(order).toEqual(["outer", "inner"]);
  });

  it("lands on the requested instant even when nothing was due", () => {
    // Two advances of 50 have to be the same as one of 100, or a test that
    // splits an advance sees different timers fire.
    uft.useFakeTimers();
    uft.setSystemTime(0);
    let at = -1;
    setTimeout(() => {
      at = Date.now();
    }, 100);

    uft.advanceTimersByTime(50);
    uft.advanceTimersByTime(50);

    expect(at).toBe(100);
  });
});

describe("setInterval", () => {
  it("repeats", () => {
    uft.useFakeTimers();
    let ticks = 0;
    setInterval(() => {
      ticks += 1;
    }, 10);

    uft.advanceTimersByTime(35);

    expect(ticks).toBe(3);
  });

  it("stops when cleared", () => {
    uft.useFakeTimers();
    let ticks = 0;
    const id = setInterval(() => {
      ticks += 1;
    }, 10);

    uft.advanceTimersByTime(20);
    clearInterval(id);
    uft.advanceTimersByTime(100);

    expect(ticks).toBe(2);
  });

  it("advances by at least a tick when the delay is zero", () => {
    // A zero-delay interval scheduled at the same instant forever would never
    // let the clock move.
    uft.useFakeTimers();
    let ticks = 0;
    const id = setInterval(() => {
      ticks += 1;
    }, 0);

    uft.advanceTimersByTime(3);
    clearInterval(id);

    expect(ticks).toBeGreaterThan(0);
  });
});

describe("running the queue", () => {
  it("runAllTimers drains everything, including what the callbacks add", () => {
    uft.useFakeTimers();
    const order: Array<string> = [];
    setTimeout(() => {
      order.push("first");
      setTimeout(() => order.push("second"), 1000);
    }, 10);

    uft.runAllTimers();

    expect(order).toEqual(["first", "second"]);
  });

  it("runAllTimers refuses an interval rather than hanging", () => {
    // An unbounded hang has to be killed; a failure names the problem.
    uft.useFakeTimers();
    setInterval(() => {}, 1);

    expect(() => uft.runAllTimers()).toThrow();
  });

  it("runOnlyPendingTimers fires what is queued and not what they queue", () => {
    uft.useFakeTimers();
    const order: Array<string> = [];
    setTimeout(() => {
      order.push("queued");
      setTimeout(() => order.push("added later"), 1);
    }, 10);

    uft.runOnlyPendingTimers();

    expect(order).toEqual(["queued"]);
  });

  it("advanceTimersToNextTimer fires exactly one, however far away", () => {
    uft.useFakeTimers();
    const order: Array<string> = [];
    setTimeout(() => order.push("near"), 10);
    setTimeout(() => order.push("far"), 10_000);

    uft.advanceTimersToNextTimer();
    expect(order).toEqual(["near"]);

    uft.advanceTimersToNextTimer();
    expect(order).toEqual(["near", "far"]);
  });

  it("counts what is waiting", () => {
    uft.useFakeTimers();
    setTimeout(() => {}, 1);
    setTimeout(() => {}, 2);

    expect(uft.getTimerCount()).toBe(2);
    uft.advanceTimersByTime(1);
    expect(uft.getTimerCount()).toBe(1);
  });
});

describe("the async advance", () => {
  it("lets a callback's await continue before the next timer", async () => {
    // The whole reason the async form exists: without it, the second timer
    // fires before the first callback has resumed.
    uft.useFakeTimers();
    const order: Array<string> = [];

    setTimeout(async () => {
      order.push("first");
      await Promise.resolve();
      order.push("first resumed");
    }, 10);
    setTimeout(() => order.push("second"), 20);

    await uft.advanceTimersByTimeAsync(20);

    expect(order).toEqual(["first", "first resumed", "second"]);
  });
});

describe("the clock itself", () => {
  it("Date.now follows the fake clock", () => {
    uft.useFakeTimers();
    uft.setSystemTime(1_000);

    expect(Date.now()).toBe(1_000);
    uft.advanceTimersByTime(500);
    expect(Date.now()).toBe(1_500);
  });

  it("new Date() reads the clock, and a given date does not", () => {
    uft.useFakeTimers();
    uft.setSystemTime(new Date("2026-01-01T00:00:00.000Z"));

    expect(new Date().toISOString()).toBe("2026-01-01T00:00:00.000Z");
    expect(new Date("2020-05-05T00:00:00.000Z").getUTCFullYear()).toBe(2020);
  });

  it("a faked date is still a Date", () => {
    // A proxy over the real constructor rather than a wrapper, so `instanceof`
    // and every method come along unchanged.
    uft.useFakeTimers();

    expect(new Date() instanceof Date).toBe(true);
    expect(typeof new Date().toISOString()).toBe("string");
  });

  it("Date without new is still a string, as it is in every runtime", () => {
    // `Date()` called as a function returns a string and ignores its
    // arguments. A `class` cannot be called without `new` at all, so the
    // subclass this used to be turned every such call into a TypeError —
    // under fake timers only, which is the worst way for it to fail.
    uft.useFakeTimers();
    uft.setSystemTime(new Date("2026-01-01T00:00:00.000Z"));

    const text = Date();

    expect(typeof text).toBe("string");
    expect(text).toBe(new Date("2026-01-01T00:00:00.000Z").toString());
  });

  it("recognises a Date made before the clock was faked", () => {
    // `instanceof Date` after installation asks about the *fake* Date, and an
    // object built by the real one is not an instance of a subclass of it. A
    // caller holding a date from before `useFakeTimers` had it silently read
    // as a number.
    const before = new Date("2026-03-01T00:00:00.000Z");
    uft.useFakeTimers();

    expect(before instanceof Date).toBe(true);

    uft.setSystemTime(before);

    expect(Date.now()).toBe(before.getTime());
    expect(uft.getMockedSystemTime()?.toISOString()).toBe("2026-03-01T00:00:00.000Z");
  });

  it("keeps the real constructor's own statics", () => {
    uft.useFakeTimers();

    expect(Date.parse("2020-05-05T00:00:00.000Z")).toBe(1_588_636_800_000);
    expect(Date.UTC(2020, 4, 5)).toBe(1_588_636_800_000);
    expect(Date.name).toBe("Date");
  });

  it("setSystemTime moves the clock without firing anything", () => {
    uft.useFakeTimers();
    let fired = false;
    setTimeout(() => {
      fired = true;
    }, 10);

    uft.setSystemTime(Date.now() + 10_000);

    expect(fired).toBe(false);
    expect(uft.getTimerCount()).toBe(1);
  });

  it("reports the mocked time, and nothing when the clock is real", () => {
    expect(uft.getMockedSystemTime()).toBe(null);

    uft.useFakeTimers();
    uft.setSystemTime(42);

    expect(uft.getMockedSystemTime()?.getTime()).toBe(42);
  });
});
