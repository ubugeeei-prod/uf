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
  andThen,
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
  forEach,
  fork,
  interrupt,
  join,
  layerEffect,
  layerMerge,
  layerSucceed,
  map,
  mapError,
  never,
  orDie,
  orElse,
  promise,
  provide,
  provideService,
  race,
  retry,
  runFork,
  runPromise,
  runPromiseExit,
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
import { scheduleDelay } from "@uniflowed/effect/schedule";

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
  it("composes with yield* and keeps each step's own type", async () => {
    const program = effect(function* () {
      // `yield*` is the typed form: `first` is a number here, where a bare
      // `yield` would hand back `mixed`.
      const first = yield* succeed(2);
      const second = yield* succeed(3);
      return first * second;
    });

    await expect(runPromise(program)).resolves.toBe(6);
  });

  it("still accepts a bare yield", async () => {
    const program = effect(function* () {
      const first = yield succeed(2);
      const second = yield succeed(3);
      return Number(first) * Number(second);
    });

    await expect(runPromise(program)).resolves.toBe(6);
  });

  it("delegates to another generator and carries its failures out", async () => {
    // The point of the test: `readName` can fail with `MissingName` and the
    // caller adds `Empty`, so the pipeline's failure type is the union — and
    // at run time either of them arrives at the same `catchTag`.
    function* readName(record) {
      const found = yield* succeed(record);
      if (found.name == null) {
        yield* fail({ kind: "MissingName" });
      }
      return found.name;
    }

    const program = (record) =>
      effect(function* () {
        const name = yield* readName(record);
        if (name === "") {
          yield* fail({ kind: "Empty" });
        }
        return name.toUpperCase();
      });

    await expect(runPromise(program({ name: "ada" }))).resolves.toBe("ADA");

    const missing = await runPromiseExit(program({ name: null }));
    if (missing.kind === "failure" && missing.cause.kind === "fail") {
      expect(missing.cause.error.kind).toBe("MissingName");
    } else {
      throw new Error("expected the delegated failure to come out");
    }

    const empty = await runPromiseExit(program({ name: "" }));
    if (empty.kind === "failure" && empty.cause.kind === "fail") {
      expect(empty.cause.error.kind).toBe("Empty");
    } else {
      throw new Error("expected the caller's own failure");
    }
  });

  it("does not run the rest of a delegated generator after it fails", async () => {
    const reached = [];

    function* step() {
      yield* fail("stop");
      reached.push("after the failure");
      return 1;
    }

    const program = effect(function* () {
      const value = yield* step();
      reached.push("after the delegation");
      return value;
    });

    const result = await runPromiseExit(program);
    expect(result.kind).toBe("failure");
    expect(reached).toEqual([]);
  });

  it("runs a wholly synchronous pipeline without a promise", () => {
    const program = effect(function* () {
      const base = yield* succeed(4);
      const doubled = yield* sync(() => base * 2);
      return doubled + 1;
    });

    expect(runSync(program)).toBe(9);
  });

  it("refuses a pipeline with an asynchronous step in runSync", () => {
    const program = effect(function* () {
      yield* sleep(1);
      return 1;
    });

    const result = runSyncExit(program);
    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("die");
    }
  });

  it("reads a service with yield* and gets the service back", async () => {
    const Clock = tag("Clock");
    const program = effect(function* () {
      const clock = yield* Clock;
      return clock.now() + 1;
    });

    await expect(runPromise(provideService(program, Clock, { now: () => 41 }))).resolves.toBe(42);
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

describe("a defect is not a failure", () => {
  it("is not reified by either", () => {
    const result = runSyncExit(either(die("bug")));
    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("die");
    }
  });

  it("is not caught by catchTag", () => {
    let caught = false;
    const result = runSyncExit(
      catchTag(die("bug"), "Anything", () => {
        caught = true;
        return succeed("recovered");
      }),
    );
    expect(caught).toBe(false);
    expect(result.kind).toBe("failure");
  });

  it("does not trigger the orElse fallback", () => {
    let fell = false;
    const result = runSyncExit(
      orElse(die("bug"), () => {
        fell = true;
        return succeed("fallback");
      }),
    );
    expect(fell).toBe(false);
    expect(result.kind).toBe("failure");
  });

  it("is what orDie turns a typed failure into", () => {
    const result = runSyncExit(orDie(fail("was typed")));
    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("die");
    }
  });

  it("does not swallow a failure whose error is null", () => {
    // `null` is a legitimate typed error, and a recovery path that looked for
    // "is there an error" rather than "is there a fail node" treated it as a
    // defect and refused to catch it.
    const recovered = runSync(
      catchAll(fail(null), (error) => succeed(error === null ? "caught null" : "caught other")),
    );
    expect(recovered).toBe("caught null");

    expect(runSync(either(fail(null)))).toEqual({ ok: false, error: null });
  });
});

describe("resource release under interruption", () => {
  it("releases a scoped resource when the fiber is interrupted", async () => {
    const events = [];
    const program = scoped(
      flatMap(
        acquireRelease(
          sync(() => {
            events.push("acquire");
            return "handle";
          }),
          // The release is itself interruptible. Run under the interrupted
          // fiber it would stop at its own first checkpoint and never push,
          // which is why finalizers run detached.
          () =>
            andThen(
              sleep(1),
              sync(() => events.push("release")),
            ),
        ),
        () => sleep(400),
      ),
    );

    const outcome = await runPromise(
      effect(function* () {
        const fiber = yield* fork(program);
        yield* sleep(10);
        return yield* interrupt(fiber);
      }),
    );

    expect(outcome.kind).toBe("failure");
    if (outcome.kind === "failure") {
      expect(outcome.cause.kind).toBe("interrupt");
    }
    expect(events).toEqual(["acquire", "release"]);
  });

  it("runs an ensuring finaliser when the fiber is interrupted", async () => {
    const events = [];
    const program = ensuring(sleep(400), () =>
      andThen(
        sleep(1),
        sync(() => events.push("finalised")),
      ),
    );

    await runPromise(
      effect(function* () {
        const fiber = yield* fork(program);
        yield* sleep(10);
        return yield* interrupt(fiber);
      }),
    );

    expect(events).toEqual(["finalised"]);
  });

  it("refuses acquireRelease outside a scope rather than skipping the release", () => {
    const result = runSyncExit(
      // Deliberately not wrapped in `scoped`. A silent skip is the failure
      // this combinator exists to prevent, so it is a defect.
      exit(
        acquireRelease(
          sync(() => "handle"),
          () => sync(() => {}),
        ),
      ),
    );
    expect(result.kind).toBe("success");
    if (result.kind === "success" && result.value.kind === "failure") {
      expect(result.value.cause.kind).toBe("die");
    } else {
      throw new Error("expected a defect from acquireRelease without a scope");
    }
  });

  it("turns a failing finaliser into a defect rather than a typed failure", async () => {
    // The scope's error channel belongs to the body. A release that fails is a
    // bug in the release, and handing it back as a typed failure would let
    // `catchAll` treat it as a condition the body's type promised.
    const program = ensuring(succeed(1), () => die("release blew up"));
    const result = await runPromiseExit(program);

    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("die");
    }
  });

  it("keeps the body's own failure when the finaliser also fails", async () => {
    const result = await runPromiseExit(ensuring(fail("body"), () => die("release")));

    if (result.kind === "failure" && result.cause.kind === "fail") {
      expect(result.cause.error).toBe("body");
    } else {
      throw new Error("expected the body's failure to survive");
    }
  });
});

describe("what a concurrent failure does to its siblings", () => {
  it("stops a sibling that would otherwise never finish", async () => {
    // Deterministic rather than timed: if `all` did not interrupt its
    // siblings, `never()` would keep the combinator waiting for ever.
    const result = await runPromiseExit(all([never(), fail("boom")]));

    expect(result.kind).toBe("failure");
    if (result.kind === "failure" && result.cause.kind === "fail") {
      expect(result.cause.error).toBe("boom");
    } else {
      throw new Error("expected the typed failure, not an interruption");
    }
  });

  it("does not return until the siblings it interrupted have stopped", async () => {
    const events = [];
    const slow = ensuring(
      effect(function* () {
        yield* sleep(400);
        events.push("slow finished");
        return 1;
      }),
      () => sync(() => events.push("slow stopped")),
    );
    const failing = effect(function* () {
      yield* sleep(5);
      return yield* fail("boom");
    });

    const started = Date.now();
    const result = await runPromiseExit(all([slow, failing]));
    const elapsed = Date.now() - started;

    expect(result.kind).toBe("failure");
    // The sibling was stopped, not awaited to completion, and `all` had
    // already seen it stop by the time it returned.
    expect(events).toEqual(["slow stopped"]);
    expect(elapsed).toBeLessThan(300);
  });

  it("keeps at most the requested number of effects in flight", async () => {
    let inFlight = 0;
    let peak = 0;
    const program = forEach(
      [1, 2, 3, 4, 5, 6],
      (item) =>
        effect(function* () {
          inFlight += 1;
          peak = Math.max(peak, inFlight);
          yield* sleep(5);
          inFlight -= 1;
          return item * 2;
        }),
      { concurrency: 2 },
    );

    // Results keep the order of the input, whatever order they finished in.
    await expect(runPromise(program)).resolves.toEqual([2, 4, 6, 8, 10, 12]);
    expect(peak).toBe(2);
  });

  it("stops taking new work once its fiber has been interrupted", async () => {
    const ran = [];
    const record = (item) => sync(() => ran.push(item));
    // A raw promise cannot be interrupted, so this element finishes well after
    // the cancellation was asked for — which is the moment `all` has to notice
    // on its own behalf, because `sync` has no checkpoint inside it.
    const stubborn = promise(() => new Promise((resolve) => setTimeout(resolve, 40)));

    const outcome = await runPromise(
      effect(function* () {
        const fiber = yield* fork(all([stubborn, record(1), record(2)], { concurrency: 1 }));
        yield* sleep(5);
        return yield* interrupt(fiber);
      }),
    );

    expect(outcome.kind).toBe("failure");
    if (outcome.kind === "failure") {
      expect(outcome.cause.kind).toBe("interrupt");
    }
    expect(ran).toEqual([]);
  });

  it("has a synchronous answer when every element has one", () => {
    expect(runSync(all([succeed(1), sync(() => 2), succeed(3)]))).toEqual([1, 2, 3]);
    expect(runSync(all([]))).toEqual([]);
  });

  it("refuses synchronously when one element is asynchronous", () => {
    const result = runSyncExit(all([succeed(1), sleep(1)]));
    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("die");
    }
  });

  it("waits for a success in a race rather than taking the first failure", async () => {
    const failsFirst = effect(function* () {
      yield* sleep(5);
      return yield* fail("early");
    });
    const succeedsLater = as(sleep(40), "late");

    await expect(runPromise(race([failsFirst, succeedsLater]))).resolves.toBe("late");
  });

  it("reports every entrant's failure when none of them succeeds", async () => {
    const result = await runPromiseExit(race([fail("a"), fail("b")]));

    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("parallel");
    }

    // Composite or not, recovery still finds a typed error inside it.
    await expect(
      runPromise(catchAll(race([fail("a"), fail("b")]), (error) => succeed(`caught ${error}`))),
    ).resolves.toBe("caught a");
  });

  it("interrupts the entrants that lost", async () => {
    const events = [];
    const loser = ensuring(sleep(400), () => sync(() => events.push("loser stopped")));

    await expect(runPromise(race([as(sleep(5), "won"), loser]))).resolves.toBe("won");

    // A loser is interrupted and deliberately not awaited, so give the
    // interruption a turn — thirty milliseconds, against the four hundred it
    // would have taken to finish on its own.
    await runPromise(sleep(30));
    expect(events).toEqual(["loser stopped"]);
  });

  it("interrupts a fiber that is doing nothing at all", async () => {
    // `never` used to ignore the flag, so this hung for the life of the process.
    const result = await runPromise(interrupt(runFork(never())));

    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("interrupt");
    }
  });

  it("stops the effect it timed out on, and waits for it", async () => {
    const events = [];
    const slow = ensuring(sleep(400), () => sync(() => events.push("stopped")));

    const result = await runPromiseExit(timeout(slow, 10));

    if (result.kind === "failure" && result.cause.kind === "fail") {
      expect(result.cause.error).toEqual({ kind: "timeout", millis: 10 });
    } else {
      throw new Error("expected a typed timeout failure");
    }
    expect(events).toEqual(["stopped"]);
  });

  it("times out an effect that could never settle on its own", async () => {
    const result = await runPromiseExit(timeout(never(), 10));
    expect(result.kind).toBe("failure");
  });
});

describe("retry counts", () => {
  const failing = () => {
    let attempts = 0;
    return {
      attempts: () => attempts,
      effect: suspend(() => {
        attempts += 1;
        return fail("no");
      }),
    };
  };

  it("makes one more attempt for upTo", async () => {
    const flaky = failing();
    await runPromiseExit(retry(flaky.effect, { kind: "upTo", millis: 1 }));
    expect(flaky.attempts()).toBe(2);
  });

  it("stops when either side of an intersection stops", async () => {
    const flaky = failing();
    await runPromiseExit(
      retry(flaky.effect, {
        kind: "intersect",
        left: { kind: "recurs", times: 2 },
        right: { kind: "spaced", millis: 0 },
      }),
    );
    // `spaced` would go on for ever; `recurs` is what ends it, after two
    // retries on top of the first attempt.
    expect(flaky.attempts()).toBe(3);
  });

  it("keeps going while either side of a union would", async () => {
    const flaky = failing();
    await runPromiseExit(
      retry(flaky.effect, {
        kind: "union",
        left: { kind: "recurs", times: 1 },
        right: { kind: "upTo", millis: 0 },
      }),
    );
    // Attempt 0: both sides still want one. Attempt 1: both have stopped, so
    // there is nothing left to keep the union going. Two attempts in all.
    expect(flaky.attempts()).toBe(2);
  });

  it("does not turn a stopped schedule into a capped one", async () => {
    const flaky = failing();
    await runPromiseExit(
      retry(flaky.effect, {
        kind: "maxDelay",
        schedule: { kind: "recurs", times: 1 },
        millis: 100,
      }),
    );
    // Capping a delay must not turn "give up" into "wait and try for ever".
    expect(flaky.attempts()).toBe(2);
  });

  it("does not retry after the fiber has been interrupted", async () => {
    let attempts = 0;
    const always = effect(function* () {
      attempts += 1;
      yield* sleep(20);
      return yield* fail("no");
    });

    await runPromise(
      effect(function* () {
        const fiber = yield* fork(retry(always, { kind: "spaced", millis: 5 }));
        yield* sleep(30);
        return yield* interrupt(fiber);
      }),
    );

    const seen = attempts;
    await runPromise(sleep(60));
    expect(attempts).toBe(seen);
  });
});

describe("schedules", () => {
  it("counts recurrences and then stops", () => {
    expect(scheduleDelay({ kind: "recurs", times: 2 }, 0)).toBe(0);
    expect(scheduleDelay({ kind: "recurs", times: 2 }, 1)).toBe(0);
    expect(scheduleDelay({ kind: "recurs", times: 2 }, 2)).toBe(null);
  });

  it("spaces attempts for ever", () => {
    expect(scheduleDelay({ kind: "spaced", millis: 25 }, 0)).toBe(25);
    expect(scheduleDelay({ kind: "spaced", millis: 25 }, 99)).toBe(25);
  });

  it("doubles by default and honours a factor given as a percentage", () => {
    expect(scheduleDelay({ kind: "exponential", baseMillis: 10 }, 0)).toBe(10);
    expect(scheduleDelay({ kind: "exponential", baseMillis: 10 }, 3)).toBe(80);
    expect(scheduleDelay({ kind: "exponential", baseMillis: 10, factorPercent: 150 }, 2)).toBe(23);
  });

  it("grows more gently on a fibonacci schedule", () => {
    const schedule = { kind: "fibonacci", baseMillis: 10 };
    expect([0, 1, 2, 3, 4].map((attempt) => scheduleDelay(schedule, attempt))).toEqual([
      10, 10, 20, 30, 50,
    ]);
  });

  it("gives upTo exactly one attempt", () => {
    expect(scheduleDelay({ kind: "upTo", millis: 30 }, 0)).toBe(30);
    expect(scheduleDelay({ kind: "upTo", millis: 30 }, 1)).toBe(null);
  });

  it("takes the longer wait of an intersection and stops with the first side", () => {
    const schedule = {
      kind: "intersect",
      left: { kind: "recurs", times: 2 },
      right: { kind: "spaced", millis: 40 },
    };
    expect(scheduleDelay(schedule, 0)).toBe(40);
    expect(scheduleDelay(schedule, 2)).toBe(null);
  });

  it("takes the shorter wait of a union and continues while either side does", () => {
    const schedule = {
      kind: "union",
      left: { kind: "recurs", times: 1 },
      right: { kind: "spaced", millis: 40 },
    };
    expect(scheduleDelay(schedule, 0)).toBe(0);
    expect(scheduleDelay(schedule, 1)).toBe(40);
  });

  it("caps a delay without reviving a schedule that has stopped", () => {
    expect(
      scheduleDelay(
        { kind: "maxDelay", schedule: { kind: "exponential", baseMillis: 10 }, millis: 25 },
        3,
      ),
    ).toBe(25);
    expect(
      scheduleDelay({ kind: "maxDelay", schedule: { kind: "upTo", millis: 5 }, millis: 100 }, 1),
    ).toBe(null);
  });
});

describe("the runners", () => {
  it("returns an Exit from runPromiseExit instead of raising", async () => {
    await expect(runPromiseExit(succeed(1))).resolves.toEqual({ kind: "success", value: 1 });

    const failed = await runPromiseExit(fail("no"));
    expect(failed).toEqual({ kind: "failure", cause: { kind: "fail", error: "no" } });
  });

  it("raises the typed error from runPromise", async () => {
    await expect(runPromise(fail(new Error("typed")))).rejects.toThrow("typed");
  });

  it("raises a described cause when the failure carries no typed error", () => {
    expect(() => runSync(die("a defect"))).toThrow("a defect");
  });

  it("hands back a fiber from runFork that can be joined", async () => {
    const fiber = runFork(as(sleep(1), "done"));
    await expect(runPromise(join(fiber))).resolves.toBe("done");
  });
});

describe("layers", () => {
  it("carries a layer's own failure into the effect's error channel", async () => {
    const Config = tag("Config");
    const layer = layerEffect(Config, fail({ kind: "NoConfig" }));
    const program = effect(function* () {
      const config = yield* Config;
      return config.value;
    });

    const result = await runPromiseExit(provide(program, layer));
    if (result.kind === "failure" && result.cause.kind === "fail") {
      expect(result.cause.error.kind).toBe("NoConfig");
    } else {
      throw new Error("expected the layer's failure");
    }
  });

  it("reports a service that was never provided as a defect", async () => {
    const Missing = tag("Missing");
    const result = await runPromiseExit(
      effect(function* () {
        return yield* Missing;
      }),
    );

    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      // The requirement channel already tracks this; reaching it at run time
      // means the type was bypassed, which is a bug and not a condition.
      expect(result.cause.kind).toBe("die");
    }
  });

  it("keeps interruption working underneath a layer", async () => {
    const Config = tag("Config");
    const program = provide(sleep(400), layerSucceed(Config, { value: 1 }));

    const outcome = await runPromise(
      effect(function* () {
        const fiber = yield* fork(program);
        yield* sleep(10);
        return yield* interrupt(fiber);
      }),
    );

    expect(outcome.kind).toBe("failure");
    if (outcome.kind === "failure") {
      expect(outcome.cause.kind).toBe("interrupt");
    }
  });
});
