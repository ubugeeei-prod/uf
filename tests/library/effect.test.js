// @flow
//
// `@uniflowed/effect`.
//
// The tests are grouped by the promise each part of the runtime makes, because
// that is what breaks: not "does `map` work" but "does a failure stop the rest
// of the pipeline", "does a released resource stay released when the body
// throws", "does an interrupted fiber stop between steps rather than inside
// one".

import { describe, expect, it } from "@uniflowed/test";

import {
  acquireRelease,
  all,
  as,
  catchAll,
  catchTag,
  die,
  effect,
  either,
  ensuring,
  exit,
  fail,
  filterOrFail,
  flatMap,
  fork,
  interrupt,
  join,
  layerMerge,
  layerSucceed,
  map,
  mapError,
  orElse,
  promise,
  provide,
  provideService,
  race,
  retry,
  runPromise,
  runSync,
  runSyncExit,
  scoped,
  sleep,
  succeed,
  suspend,
  sync,
  tag,
  tap,
  tapError,
  timeout,
  tryPromise,
  zip,
} from "@uniflowed/effect";

describe("succeed and fail", () => {
  it("runs a pure success synchronously", () => {
    expect(runSync(succeed(3))).toBe(3);
  });

  it("makes a failure an Exit rather than a thrown value", () => {
    const result = runSyncExit(fail("nope"));
    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause).toEqual({ kind: "fail", error: "nope" });
    }
  });

  it("distinguishes a failure from a defect", () => {
    const failure = runSyncExit(fail("expected"));
    const defect = runSyncExit(die("unexpected"));
    expect(failure.kind).toBe("failure");
    expect(defect.kind).toBe("failure");
    if (failure.kind === "failure" && defect.kind === "failure") {
      expect(failure.cause.kind).toBe("fail");
      expect(defect.cause.kind).toBe("die");
    }
  });

  it("turns a thrown value inside sync into a defect, not a failure", () => {
    const result = runSyncExit(
      sync(() => {
        throw new Error("boom");
      }),
    );
    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("die");
    }
  });

  it("does not run a sync body until the effect is run", () => {
    let ran = 0;
    const lazy = sync(() => {
      ran += 1;
      return ran;
    });
    expect(ran).toBe(0);
    runSync(lazy);
    expect(ran).toBe(1);
  });

  it("runs a suspended effect only when it is needed", () => {
    let built = 0;
    const lazy = suspend(() => {
      built += 1;
      return succeed(built);
    });
    expect(built).toBe(0);
    expect(runSync(lazy)).toBe(1);
  });
});

describe("map and flatMap", () => {
  it("maps a success", () => {
    expect(runSync(map(succeed(2), (value) => value * 5))).toBe(10);
  });

  it("leaves a failure alone", () => {
    const result = runSyncExit(map(fail("stop"), (value) => value));
    expect(result.kind).toBe("failure");
  });

  it("does not call the mapper on a failure", () => {
    let calls = 0;
    runSyncExit(
      map(fail("stop"), (value) => {
        calls += 1;
        return value;
      }),
    );
    expect(calls).toBe(0);
  });

  it("maps the error and not the value", () => {
    const result = runSyncExit(mapError(fail(1), (error) => error + 1));
    if (result.kind === "failure" && result.cause.kind === "fail") {
      expect(result.cause.error).toBe(2);
    } else {
      throw new Error("expected a failure");
    }
  });

  it("sequences with flatMap", () => {
    expect(runSync(flatMap(succeed(2), (value) => succeed(value + 1)))).toBe(3);
  });

  it("stops a sequence at the first failure", () => {
    let reached = false;
    const result = runSyncExit(
      flatMap(fail("first"), () => {
        reached = true;
        return succeed(1);
      }),
    );
    expect(reached).toBe(false);
    expect(result.kind).toBe("failure");
  });

  it("replaces a value with as", () => {
    expect(runSync(as(succeed(1), "done"))).toBe("done");
  });

  it("zips two successes into a pair", () => {
    expect(runSync(zip(succeed(1), succeed("a")))).toEqual([1, "a"]);
  });
});

describe("the generator form", () => {
  it("yields effects and resumes with their values", async () => {
    const program = effect(function* () {
      const first = yield succeed(2);
      const second = yield succeed(3);
      return first * second;
    });

    await expect(runPromise(program)).resolves.toBe(6);
  });

  it("stops at a yielded failure without running the rest", async () => {
    let reached = false;
    const program = effect(function* () {
      yield fail("stop");
      reached = true;
      return 1;
    });

    const result = await runPromise(exit(program));
    expect(reached).toBe(false);
    expect(result.kind).toBe("failure");
  });

  it("makes a throw inside the body a defect", async () => {
    const program = effect(function* () {
      yield succeed(1);
      throw new Error("boom");
    });

    const result = await runPromise(exit(program));
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("die");
    } else {
      throw new Error("expected a failure");
    }
  });
});

describe("recovery", () => {
  it("catches a failure and continues", () => {
    expect(runSync(catchAll(fail("gone"), (error) => succeed(`saw ${error}`)))).toBe("saw gone");
  });

  it("does not catch a defect", () => {
    let caught = false;
    const result = runSyncExit(
      catchAll(die("defect"), () => {
        caught = true;
        return succeed("recovered");
      }),
    );
    expect(caught).toBe(false);
    expect(result.kind).toBe("failure");
  });

  it("catches only the tagged error it was asked for", () => {
    const wrongTag = runSyncExit(
      catchTag(fail({ kind: "Other", detail: 1 }), "NotFound", () => succeed("handled")),
    );
    expect(wrongTag.kind).toBe("failure");

    const rightTag = runSync(
      catchTag(fail({ kind: "NotFound" }), "NotFound", () => succeed("handled")),
    );
    expect(rightTag).toBe("handled");
  });

  it("falls back with orElse", () => {
    expect(runSync(orElse(fail("no"), () => succeed("yes")))).toBe("yes");
  });

  it("turns a failure into an Either rather than stopping", () => {
    expect(runSync(either(fail("e")))).toEqual({ ok: false, error: "e" });
    expect(runSync(either(succeed(1)))).toEqual({ ok: true, value: 1 });
  });

  it("fails a success that does not hold with filterOrFail", () => {
    const kept = runSync(
      filterOrFail(
        succeed(4),
        (n) => n > 2,
        () => "too small",
      ),
    );
    expect(kept).toBe(4);

    const rejected = runSyncExit(
      filterOrFail(
        succeed(1),
        (n) => n > 2,
        () => "too small",
      ),
    );
    if (rejected.kind === "failure" && rejected.cause.kind === "fail") {
      expect(rejected.cause.error).toBe("too small");
    } else {
      throw new Error("expected a failure");
    }
  });
});

describe("tap", () => {
  it("sees the value and keeps it", () => {
    const seen = [];
    const result = runSync(
      tap(succeed(7), (value) => {
        seen.push(value);
        return succeed(undefined);
      }),
    );
    expect(result).toBe(7);
    expect(seen).toEqual([7]);
  });

  it("sees the error and keeps the failure", () => {
    const seen = [];
    const result = runSyncExit(
      tapError(fail("bad"), (error) => {
        seen.push(error);
        return succeed(undefined);
      }),
    );
    expect(seen).toEqual(["bad"]);
    expect(result.kind).toBe("failure");
  });

  it("does not run the error tap on a success", () => {
    let calls = 0;
    runSync(
      tapError(succeed(1), () => {
        calls += 1;
        return succeed(undefined);
      }),
    );
    expect(calls).toBe(0);
  });
});

describe("all", () => {
  it("collects every success in order", async () => {
    await expect(runPromise(all([succeed(1), succeed(2), succeed(3)]))).resolves.toEqual([1, 2, 3]);
  });

  it("fails with the first failure", async () => {
    const result = await runPromise(exit(all([succeed(1), fail("second"), succeed(3)])));
    if (result.kind === "failure" && result.cause.kind === "fail") {
      expect(result.cause.error).toBe("second");
    } else {
      throw new Error("expected a failure");
    }
  });

  it("is a success on an empty list", async () => {
    await expect(runPromise(all([]))).resolves.toEqual([]);
  });
});

describe("resources", () => {
  it("releases what it acquired", async () => {
    const events = [];
    const program = scoped(
      flatMap(
        acquireRelease(
          sync(() => {
            events.push("acquire");
            return "handle";
          }),
          () =>
            sync(() => {
              events.push("release");
            }),
        ),
        (handle) =>
          sync(() => {
            events.push(`use ${handle}`);
            return handle;
          }),
      ),
    );

    await expect(runPromise(program)).resolves.toBe("handle");
    expect(events).toEqual(["acquire", "use handle", "release"]);
  });

  it("releases what it acquired when the body fails", async () => {
    const events = [];
    const program = scoped(
      flatMap(
        acquireRelease(
          sync(() => {
            events.push("acquire");
            return "handle";
          }),
          () =>
            sync(() => {
              events.push("release");
            }),
        ),
        () => fail("body failed"),
      ),
    );

    const result = await runPromise(exit(program));
    expect(result.kind).toBe("failure");
    expect(events).toEqual(["acquire", "release"]);
  });

  it("runs an ensuring finaliser on both paths", async () => {
    const events = [];
    const finalise = () =>
      sync(() => {
        events.push("finalised");
      });

    await runPromise(exit(ensuring(succeed(1), finalise)));
    await runPromise(exit(ensuring(fail("x"), finalise)));

    expect(events).toEqual(["finalised", "finalised"]);
  });
});

describe("services", () => {
  it("reads a service provided directly", async () => {
    const Clock = tag<{| readonly now: () => number |}>("Clock");
    const program = flatMap(Clock, (clock) => succeed(clock.now()));

    await expect(runPromise(provideService(program, Clock, { now: () => 42 }))).resolves.toBe(42);
  });

  it("reads a service provided by a layer", async () => {
    const Greeter = tag("Greeter");
    const program = flatMap(Greeter, (greeter) => succeed(greeter.hello()));
    const layer = layerSucceed(Greeter, { hello: () => "hi" });

    await expect(runPromise(provide(program, layer))).resolves.toBe("hi");
  });

  it("merges two layers into one context", async () => {
    const A = tag("A");
    const B = tag("B");
    const layer = layerMerge(layerSucceed(A, 1), layerSucceed(B, 2));
    const program = effect(function* () {
      const a = yield A;
      const b = yield B;
      return a + b;
    });

    await expect(runPromise(provide(program, layer))).resolves.toBe(3);
  });
});

describe("concurrency", () => {
  it("takes the first fiber to finish in a race", async () => {
    const slow = as(sleep(60), "slow");
    const quick = as(sleep(1), "quick");
    await expect(runPromise(race([quick, slow]))).resolves.toBe("quick");
  });

  it("joins a forked fiber", async () => {
    const program = effect(function* () {
      const fiber = yield fork(as(sleep(1), "done"));
      return yield join(fiber);
    });
    await expect(runPromise(program)).resolves.toBe("done");
  });

  it("interrupts a fiber and reports it in the Exit", async () => {
    const program = effect(function* () {
      const fiber = yield fork(as(sleep(200), "never"));
      return yield interrupt(fiber);
    });

    const result = await runPromise(program);
    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("interrupt");
    }
  });

  it("times out an effect that takes too long", async () => {
    const result = await runPromise(exit(timeout(sleep(200), 5)));
    expect(result.kind).toBe("failure");
  });

  it("does not time out an effect that finishes in time", async () => {
    await expect(runPromise(timeout(as(sleep(1), "in time"), 500))).resolves.toBe("in time");
  });
});

describe("retry", () => {
  it("stops as soon as an attempt succeeds", async () => {
    let attempts = 0;
    const flaky = suspend(() => {
      attempts += 1;
      return attempts < 3 ? fail("again") : succeed(attempts);
    });

    await expect(runPromise(retry(flaky, { kind: "recurs", times: 5 }))).resolves.toBe(3);
    expect(attempts).toBe(3);
  });

  it("gives up after the schedule is exhausted, keeping the last failure", async () => {
    let attempts = 0;
    const always = suspend(() => {
      attempts += 1;
      return fail("still failing");
    });

    const result = await runPromise(exit(retry(always, { kind: "recurs", times: 2 })));
    expect(result.kind).toBe("failure");
    // The first attempt is not a retry: three runs for two retries.
    expect(attempts).toBe(3);
  });

  it("does not retry a defect", async () => {
    let attempts = 0;
    const broken = suspend(() => {
      attempts += 1;
      return die("defect");
    });

    await runPromise(exit(retry(broken, { kind: "recurs", times: 5 })));
    expect(attempts).toBe(1);
  });
});

describe("promises", () => {
  it("adopts a resolved promise", async () => {
    await expect(runPromise(promise(() => Promise.resolve(5)))).resolves.toBe(5);
  });

  it("turns a rejected promise into a failure through the given mapper", async () => {
    const program = tryPromise({
      try: () => Promise.reject(new Error("network")),
      catch: (error) => ({ kind: "NetworkError", cause: String(error) }),
    });

    const result = await runPromise(exit(program));
    if (result.kind === "failure" && result.cause.kind === "fail") {
      expect(result.cause.error.kind).toBe("NetworkError");
    } else {
      throw new Error("expected a failure");
    }
  });

  it("refuses a promise in runSync rather than returning one", () => {
    const result = runSyncExit(promise(() => Promise.resolve(1)));
    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("die");
    }
  });
});

describe("what the review found", () => {
  it("reports an interruption inside timeout as an interruption", async () => {
    const program = effect(function* () {
      const fiber = yield fork(timeout(sleep(500), 400));
      // Interrupting while the effect waits inside `timeout` used to resolve
      // the pause early and be reported as a timeout — a typed failure, which
      // `retry` would then have run again after somebody asked it to stop.
      return yield interrupt(fiber);
    });

    const result = await runPromise(program);
    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("interrupt");
    }
  });

  it("runs tapError synchronously", () => {
    const seen = [];
    const result = runSyncExit(
      tapError(fail("bad"), (error) => {
        seen.push(error);
        return succeed(undefined);
      }),
    );

    expect(seen).toEqual(["bad"]);
    // The original typed failure, not a defect saying the effect could not run
    // synchronously.
    expect(result.kind).toBe("failure");
    if (result.kind === "failure" && result.cause.kind === "fail") {
      expect(result.cause.error).toBe("bad");
    } else {
      throw new Error(`expected a typed failure, got ${JSON.stringify(result)}`);
    }
  });

  it("runs ensuring synchronously", () => {
    const events = [];
    const value = runSync(
      ensuring(succeed(7), () =>
        sync(() => {
          events.push("finalised");
        }),
      ),
    );

    expect(value).toBe(7);
    expect(events).toEqual(["finalised"]);
  });

  it("keeps a synchronous failure through ensuring", () => {
    const events = [];
    const result = runSyncExit(
      ensuring(fail("no"), () =>
        sync(() => {
          events.push("finalised");
        }),
      ),
    );

    expect(events).toEqual(["finalised"]);
    if (result.kind === "failure" && result.cause.kind === "fail") {
      expect(result.cause.error).toBe("no");
    } else {
      throw new Error("expected the original failure");
    }
  });
});
