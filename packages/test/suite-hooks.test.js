// @flow
//
// `@uniflowed/test`'s own suite hooks: when `beforeAll` and `afterAll` run.
//
// A file's tests almost always live inside a `describe`, so the file's own
// `beforeAll` has no case of its own to run before — it has a suite. Deferring
// setup until a *direct* child case ran meant that hook never ran at all, in
// the ordinary shape, silently: a file that set up a fixture at the top and
// asserted on it inside a `describe` saw whatever the fixture was before the
// hook it wrote.
//
// The order matters as much as the fact. An inner suite's setup runs after its
// ancestors' so that it can build on what they made, and teardown unwinds the
// other way.

import { afterAll, beforeAll, describe, expect, it } from "@uniflowed/test";
import {
  beforeAll as registerBeforeAll,
  beforeEach as registerBeforeEach,
  describe as registerSuite,
  it as registerCase,
  reset,
} from "./internal/registry.js";
import { type Result, run as runRegistered } from "./internal/run.js";

const order: Array<string> = [];

beforeAll(() => {
  order.push("file setup");
});

describe("an outer suite whose children are all suites", () => {
  beforeAll(() => {
    order.push("outer setup");
  });

  afterAll(() => {
    order.push("outer teardown");
  });

  describe("the inner one, which holds the only case", () => {
    beforeAll(() => {
      order.push("inner setup");
    });

    it("has seen every ancestor's setup, outermost first", () => {
      expect(order).toEqual(["file setup", "outer setup", "inner setup"]);
    });
  });
});

describe("the suite that reads what the one before it left", () => {
  it("saw the inner suite torn down before this one ran", () => {
    // The outer suite above closed when its last case finished, so its
    // teardown has run by the time a sibling suite starts.
    expect(order).toEqual(["file setup", "outer setup", "inner setup", "outer teardown"]);
  });
});

describe("a suite whose cases are all skipped", () => {
  beforeAll(() => {
    order.push("never");
  });

  it.skip("does not run", () => {});
  it.skipBecause("does not run for a named reason", "the skip reason belongs in the report");
});

describe("what the skipped suite did", () => {
  it("set nothing up, because nothing in it ran", () => {
    expect(order).not.toContain("never");
  });
});

describe("a reasoned skip without a body", () => {
  it("is reported as skipped rather than todo", async () => {
    const results: Array<Result> = [];
    reset();
    registerCase.skipBecause("needs another host", "Deno cannot exercise Node hooks");

    await runRegistered({ file: "virtual.test.js" }, (result) => {
      results.push(result);
    });

    expect(results.length).toBe(1);
    expect(results[0].name).toBe("needs another host");
    expect(results[0].outcome).toEqual({
      status: "skipped",
      reason: "explicit",
      message: "Deno cannot exercise Node hooks",
    });
  });
});

describe("a beforeAll that fails", () => {
  it("fails every case it was setting up for, and runs none of their bodies", async () => {
    // The first case used to take the hook's error and every case after it
    // ran anyway, against a setup that never happened — so a slow machine's
    // one timed-out hook read as three unrelated failures, two of them
    // assertions on empty fixtures.
    const results: Array<Result> = [];
    const ran: Array<string> = [];
    reset();
    registerSuite("needs a server", () => {
      registerBeforeAll(() => {
        throw new Error("could not start the server");
      });
      registerCase("first", () => {
        ran.push("first");
      });
      registerCase("second", () => {
        ran.push("second");
      });
      registerSuite("nested", () => {
        registerCase("third", () => {
          ran.push("third");
        });
      });
    });

    await runRegistered({ file: "virtual.test.js" }, (result) => {
      results.push(result);
    });

    expect(ran).toEqual([]);
    expect(results.map((result) => result.name)).toEqual([
      "needs a server > first",
      "needs a server > second",
      "needs a server > nested > third",
    ]);
    for (const { outcome } of results) {
      expect(outcome.status).toBe("failed");
      expect(outcome.status === "failed" ? outcome.message : "").toContain(
        "could not start the server",
      );
    }
  });

  it("runs once, however many cases it fails", async () => {
    let calls = 0;
    reset();
    registerBeforeAll(() => {
      calls += 1;
      throw new Error("no");
    });
    registerCase("a", () => {});
    registerCase("b", () => {});

    await runRegistered({ file: "virtual.test.js" }, () => {});

    expect(calls).toBe(1);
  });
});

describe("a hook's own timeout", () => {
  const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

  const outcomes = async (register: () => void, timeoutMs: number): Promise<Array<string>> => {
    const results: Array<Result> = [];
    reset();
    register();
    await runRegistered({ file: "virtual.test.js", timeoutMs }, (result) => {
      results.push(result);
    });
    return results.map((result) => String(result.outcome.status));
  };

  it("lets a slow setup finish where the cases' budget would not", async () => {
    // `{ timeout }`, the way `it` spells it, and a bare number, the way Jest
    // and Vitest do.
    expect(
      await outcomes(() => {
        registerBeforeAll(() => sleep(60), { timeout: 5000 });
        registerBeforeEach(() => sleep(60), 5000);
        registerCase("fast", () => {});
      }, 20),
    ).toEqual(["passed"]);
  });

  it("holds a hook to its own budget rather than the file's", async () => {
    expect(
      await outcomes(() => {
        registerBeforeAll(() => sleep(200), { timeout: 10 });
        registerCase("fast", () => {});
      }, 5000),
    ).toEqual(["failed"]);
  });

  it("falls back to the case's budget when it names none", async () => {
    expect(
      await outcomes(() => {
        registerBeforeEach(() => sleep(200));
        registerCase("fast", () => {}, { timeout: 10 });
      }, 5000),
    ).toEqual(["failed"]);
  });
});
