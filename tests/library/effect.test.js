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
  deferred,
  deferredAwait,
  deferredFail,
  deferredIsDone,
  deferredSucceed,
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
  forkDaemon,
  interrupt,
  join,
  layerEffect,
  layerMerge,
  layerProvide,
  layerProvideMerge,
  layerScoped,
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
  ref,
  refGet,
  refGetAndSet,
  refGetAndUpdate,
  refModify,
  refSet,
  refUpdate,
  refUpdateAndGet,
  refUpdateEffect,
  repeat,
  retry,
  runFork,
  runPromise,
  runPromiseExit,
  runSync,
  runSyncExit,
  scoped,
  semaphore,
  sleep,
  succeed,
  suspend,
  sync,
  tag,
  tap,
  tapError,
  timeout,
  tryPromise,
  withPermit,
  withPermits,
  zip,
} from "@uniflowed/effect";
import { scheduleDelay } from "@uniflowed/effect/schedule";
import {
  streamEnsuring,
  streamFilter,
  streamFromArray,
  streamFromEffect,
  streamFromIterator,
  streamFromReadableStream,
  streamMap,
  streamMapEffect,
  streamPaginate,
  streamRunCollect,
  streamRunDrain,
  streamRunFold,
  streamRunForEach,
  streamRunHead,
  streamTake,
  streamTap,
} from "@uniflowed/effect/stream";

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

  it("keeps a synchronous program synchronous through a scope", () => {
    // Acquiring is not inherently asynchronous, and a program made of
    // synchronous steps should not lose `runSync` for using a resource.
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
        (handle) => sync(() => `used ${handle}`),
      ),
    );

    expect(runSync(program)).toBe("used handle");
    expect(events).toEqual(["acquire", "release"]);
  });

  it("reports a finaliser that cannot run synchronously rather than skipping it", () => {
    // A release that did not happen is the news this returns; a silent skip is
    // the failure `acquireRelease` exists to prevent.
    const program = scoped(
      andThen(
        acquireRelease(
          sync(() => "handle"),
          () => sleep(1),
        ),
        succeed("body"),
      ),
    );

    const result = runSyncExit(program);
    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("die");
    }
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

describe("what a forked fiber's parent owns", () => {
  /** Ticks every twenty milliseconds, five times, and says how far it got. */
  const ticker = () => {
    let ticks = 0;
    return {
      count: () => ticks,
      effect: effect(function* () {
        for (let index = 0; index < 5; index += 1) {
          yield* sleep(20);
          ticks += 1;
        }
        return ticks;
      }),
    };
  };

  it("interrupts a forked child when the fiber that forked it is interrupted", async () => {
    // The reproduction from #258. `fork` was `detachedContext`, so the parent
    // was gone at thirty milliseconds and the child ran to completion at a
    // hundred — reported by nobody, because the only handle to it was the one
    // the parent dropped.
    const child = ticker();
    const parent = effect(function* () {
      yield* fork(child.effect);
      yield* sleep(1000);
    });

    const fiber = runFork(parent);
    await runPromise(sleep(30));
    await runPromise(interrupt(fiber));
    const atInterrupt = child.count();
    await runPromise(sleep(150));

    expect(atInterrupt).toBeLessThan(5);
    expect(child.count()).toBe(atInterrupt);
  });

  it("leaves a forkDaemon child running when its parent is interrupted", async () => {
    // The behaviour `fork` used to have, under the name that says what it is.
    const child = ticker();
    const parent = effect(function* () {
      yield* forkDaemon(child.effect);
      yield* sleep(1000);
    });

    const fiber = runFork(parent);
    await runPromise(sleep(30));
    await runPromise(interrupt(fiber));
    const atInterrupt = child.count();
    await runPromise(sleep(150));

    expect(atInterrupt).toBeLessThan(5);
    expect(child.count()).toBe(5);
  });

  it("interrupts a child that has not reached its first checkpoint yet", async () => {
    // `childContext` promises that a child born to an interrupted parent
    // starts interrupted. This is the earliest moment that promise can be
    // asked for: the fork has happened but the child has not run a step.
    const events = [];
    const program = effect(function* () {
      yield* fork(
        effect(function* () {
          yield* sleep(20);
          events.push("child ran");
        }),
      );
      yield* sleep(400);
    });

    const fiber = runFork(program);
    await runPromise(interrupt(fiber));
    await runPromise(sleep(80));

    expect(events).toEqual([]);
  });

  it("stops a forked child when the fiber that forked it simply ends", async () => {
    // Not an interruption: the parent returned. A child that outlived a
    // finished parent would be unreachable from any handle, which is the same
    // leak by a quieter route.
    const events = [];
    const child = ensuring(sleep(400), () => sync(() => events.push("child stopped")));

    await runPromise(
      effect(function* () {
        yield* fork(child);
        yield* sleep(10);
        return "parent finished";
      }),
    );
    await runPromise(sleep(40));

    expect(events).toEqual(["child stopped"]);
  });

  it("still ends a daemon's own children when the daemon ends", async () => {
    // A daemon is detached from its parent, not from its children. If the
    // escape were inherited, one `forkDaemon` would detach a whole subtree.
    const events = [];
    const grandchild = ensuring(sleep(400), () => sync(() => events.push("grandchild stopped")));

    await runPromise(
      effect(function* () {
        yield* forkDaemon(
          effect(function* () {
            yield* fork(grandchild);
            yield* sleep(10);
          }),
        );
        yield* sleep(80);
      }),
    );

    expect(events).toEqual(["grandchild stopped"]);
  });

  it("does not interrupt the fiber that forked it when the child is interrupted", async () => {
    // The link is one-way on purpose: a child is cancellable on its own, which
    // is the whole reason to hold a handle to it.
    const settled = await runPromise(
      effect(function* () {
        const fiber = yield* fork(sleep(400));
        yield* interrupt(fiber);
        yield* sleep(5);
        return "parent finished";
      }),
    );

    expect(settled).toBe("parent finished");
  });

  it("keeps a joined child's value when the parent ends right after it", async () => {
    // Ending a fiber must not reach a child that has already settled and left
    // the tree, or every `fork`-then-`join` would race its own bookkeeping.
    const settled = await runPromise(
      effect(function* () {
        const fiber = yield* fork(as(sleep(5), "child value"));
        return yield* join(fiber);
      }),
    );

    expect(settled).toBe("child value");
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

  it("stops at the first failure the predicate rules out", async () => {
    // `isRetriable` draws the line the runtime knows: a defect is a bug, an
    // interruption is a decision. It cannot draw the line the application
    // knows — a 429 is worth another attempt and a 400 is not, and both are
    // typed failures.
    let attempts = 0;
    const forbidden = suspend(() => {
      attempts += 1;
      return fail({ kind: "forbidden" });
    });

    await runPromiseExit(
      retry(
        forbidden,
        { kind: "recurs", times: 3 },
        { while: (error) => error.kind === "timeout" },
      ),
    );
    expect(attempts).toBe(1);
  });

  it("keeps retrying the failure the predicate allows", async () => {
    let attempts = 0;
    const timingOut = suspend(() => {
      attempts += 1;
      return fail({ kind: "timeout" });
    });

    await runPromiseExit(
      retry(
        timingOut,
        { kind: "recurs", times: 3 },
        { while: (error) => error.kind === "timeout" },
      ),
    );
    expect(attempts).toBe(4);
  });

  it("stops once until is satisfied", async () => {
    let attempts = 0;
    const failing = suspend(() => {
      attempts += 1;
      return fail({ kind: attempts >= 2 ? "forbidden" : "timeout" });
    });

    await runPromiseExit(
      retry(
        failing,
        { kind: "recurs", times: 5 },
        { until: (error) => error.kind === "forbidden" },
      ),
    );
    // One attempt, one retry that failed with `forbidden`, and no more.
    expect(attempts).toBe(2);
  });

  it("keeps the failure the predicate refused rather than replacing it", async () => {
    const result = await runPromiseExit(
      retry(fail({ kind: "forbidden" }), { kind: "recurs", times: 3 }, { while: () => false }),
    );

    if (result.kind === "failure" && result.cause.kind === "fail") {
      expect(result.cause.error.kind).toBe("forbidden");
    } else {
      throw new Error("expected the original typed failure");
    }
  });

  it("turns a predicate that throws into a defect", async () => {
    // A predicate is a caller's code; a bug in it is a bug, not one more
    // attempt and not a silent stop.
    const result = await runPromiseExit(
      retry(
        fail("no"),
        { kind: "recurs", times: 3 },
        {
          while: () => {
            throw new Error("bad predicate");
          },
        },
      ),
    );

    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("die");
    }
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

  it("spreads a wait over a range, and the midpoint leaves it alone", () => {
    const schedule = { kind: "jittered", schedule: { kind: "spaced", millis: 100 } };
    // The factor is an argument, so the whole range is testable without a seed
    // and without hoping about `Math.random`.
    expect(scheduleDelay(schedule, 0, 0)).toBe(80);
    expect(scheduleDelay(schedule, 0, 0.5)).toBe(100);
    expect(scheduleDelay(schedule, 0, 1)).toBe(120);
    // Two arguments means the midpoint, so a jittered schedule stays as
    // deterministic as every other arm when nobody asks for a factor.
    expect(scheduleDelay(schedule, 0)).toBe(100);
  });

  it("honours a jitter range given as percentages", () => {
    const schedule = {
      kind: "jittered",
      schedule: { kind: "spaced", millis: 200 },
      minPercent: 50,
      maxPercent: 150,
    };
    expect(scheduleDelay(schedule, 0, 0)).toBe(100);
    expect(scheduleDelay(schedule, 0, 1)).toBe(300);
  });

  it("jitters the schedule it wraps rather than replacing it", () => {
    // A jittered exponential still grows.
    const schedule = { kind: "jittered", schedule: { kind: "exponential", baseMillis: 10 } };
    expect(scheduleDelay(schedule, 0, 1)).toBe(12);
    expect(scheduleDelay(schedule, 3, 1)).toBe(96);
  });

  it("does not revive a schedule that has stopped by jittering it", () => {
    // Jitter is about *when*, and `null` is not a when.
    expect(scheduleDelay({ kind: "jittered", schedule: { kind: "upTo", millis: 5 } }, 1, 1)).toBe(
      null,
    );
    expect(scheduleDelay({ kind: "jittered", schedule: { kind: "recurs", times: 1 } }, 1, 1)).toBe(
      null,
    );
  });

  it("clamps a factor outside the unit interval rather than escaping the range", () => {
    const schedule = { kind: "jittered", schedule: { kind: "spaced", millis: 100 } };
    expect(scheduleDelay(schedule, 0, -1)).toBe(80);
    expect(scheduleDelay(schedule, 0, 4)).toBe(120);
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

describe("repeat", () => {
  it("runs again while the schedule says to, and the first run is not a repeat", async () => {
    let runs = 0;
    const counting = sync(() => {
      runs += 1;
      return runs;
    });

    await expect(runPromise(repeat(counting, { kind: "recurs", times: 2 }))).resolves.toBe(3);
    expect(runs).toBe(3);
  });

  it("gives back the effect's last value", async () => {
    const values: Array<number> = [];
    const collecting = sync(() => {
      values.push(values.length);
      return `run ${values.length}`;
    });

    await expect(runPromise(repeat(collecting, { kind: "upTo", millis: 1 }))).resolves.toBe(
      "run 2",
    );
  });

  it("ends on a failure and reports it rather than polling through it", async () => {
    // Swallowing the failure to keep polling would hide the outage the poll
    // exists to notice.
    let runs = 0;
    const failsThird = suspend(() => {
      runs += 1;
      return runs === 3 ? fail("down") : succeed(runs);
    });

    const result = await runPromiseExit(repeat(failsThird, { kind: "recurs", times: 10 }));
    if (result.kind === "failure" && result.cause.kind === "fail") {
      expect(result.cause.error).toBe("down");
    } else {
      throw new Error("expected the failure that ended the repetition");
    }
    expect(runs).toBe(3);
  });

  it("stops when the fiber is interrupted rather than at the end of the schedule", async () => {
    // A `spaced` schedule never stops on its own, so nothing but the
    // interruption can end this.
    let runs = 0;
    const polling = effect(function* () {
      runs += 1;
      yield* sleep(5);
      return runs;
    });

    const outcome = await runPromise(
      effect(function* () {
        const fiber = yield* fork(repeat(polling, { kind: "spaced", millis: 5 }));
        yield* sleep(40);
        return yield* interrupt(fiber);
      }),
    );

    expect(outcome.kind).toBe("failure");
    if (outcome.kind === "failure") {
      expect(outcome.cause.kind).toBe("interrupt");
    }

    const seen = runs;
    await runPromise(sleep(60));
    expect(runs).toBe(seen);
  });

  it("composes with retry: tolerate a blip, stop on a real failure", async () => {
    let attempts = 0;
    const flaky = suspend(() => {
      attempts += 1;
      // One blip in the middle of a poll, recovered by the retry; then a
      // failure that stays failed, which no number of retries will settle.
      if (attempts === 2) {
        return fail("blip");
      }
      if (attempts >= 5) {
        return fail("down");
      }
      return succeed(attempts);
    });

    const result = await runPromiseExit(
      repeat(retry(flaky, { kind: "recurs", times: 1 }), { kind: "recurs", times: 5 }),
    );

    if (result.kind === "failure" && result.cause.kind === "fail") {
      // The blip was retried away; the second failure survived its one retry
      // and ended the repetition.
      expect(result.cause.error).toBe("down");
    } else {
      throw new Error("expected the failure that outlasted its retry");
    }
  });
});

describe("what two fibers can share", () => {
  it("lands on the right total when four fibers count into one ref", async () => {
    // A closure over a `let` would land on the right number too, and it would
    // not be interruption-aware and would not survive an `await` between the
    // read and the write. This is the shape that does.
    const program = effect(function* () {
      const counter = yield* ref(0);
      yield* forEach(
        [1, 2, 3, 4, 5, 6, 7, 8],
        (item) =>
          effect(function* () {
            yield* sleep(1);
            yield* refUpdate(counter, (total) => total + item);
          }),
        { concurrency: 4 },
      );
      return yield* refGet(counter);
    });

    await expect(runPromise(program)).resolves.toBe(36);
  });

  it("answers synchronously, so a program with a ref keeps runSync", () => {
    const program = effect(function* () {
      const held = yield* ref("first");
      const previous = yield* refGetAndSet(held, "second");
      yield* refUpdate(held, (value) => `${value}!`);
      return `${previous} then ${yield* refGet(held)}`;
    });

    expect(runSync(program)).toBe("first then second!");
  });

  it("reads, computes and writes without letting another fiber in", async () => {
    // `refModify` cannot yield, so the increment either happens whole or not
    // at all. Two hundred fibers racing on it land exactly on two hundred.
    const program = effect(function* () {
      const counter = yield* ref(0);
      const items = Array.from({ length: 200 }, (unused, index) => index);
      yield* forEach(items, () => refModify(counter, (total) => [total, total + 1]), {
        concurrency: "unbounded",
      });
      return yield* refGet(counter);
    });

    await expect(runPromise(program)).resolves.toBe(200);
  });

  it("leaves the ref alone when the transform throws", () => {
    const program = effect(function* () {
      const held = yield* ref("kept");
      const outcome = yield* exit(
        refUpdate(held, () => {
          throw new Error("bad transform");
        }),
      );
      return { outcome, value: yield* refGet(held) };
    });

    const settled = runSync(program);
    expect(settled.outcome.kind).toBe("failure");
    if (settled.outcome.kind === "failure") {
      expect(settled.outcome.cause.kind).toBe("die");
    }
    // Half of a read-modify-write is worse than none of it.
    expect(settled.value).toBe("kept");
  });

  it("gives every operation the value it names", () => {
    const program = effect(function* () {
      const held = yield* ref(1);
      const updated = yield* refUpdateAndGet(held, (value) => value + 1);
      const before = yield* refGetAndUpdate(held, (value) => value * 10);
      const after = yield* refGet(held);
      yield* refSet(held, 0);
      const modified = yield* refModify(held, (value) => [`was ${value}`, value + 5]);
      return [updated, before, after, modified, yield* refGet(held)];
    });

    expect(runSync(program)).toEqual([2, 2, 20, "was 0", 5]);
  });

  it("hands a deferred's value to everyone waiting for it", async () => {
    const program = effect(function* () {
      const handshake = yield* deferred();
      const first = yield* fork(deferredAwait(handshake));
      const second = yield* fork(deferredAwait(handshake));
      yield* sleep(5);
      const won = yield* deferredSucceed(handshake, "ready");
      const again = yield* deferredSucceed(handshake, "ignored");
      return [yield* join(first), yield* join(second), won, again];
    });

    await expect(runPromise(program)).resolves.toEqual(["ready", "ready", true, false]);
  });

  it("puts a deferred's failure in the error channel of whoever waited", async () => {
    const program = effect(function* () {
      const handshake = yield* deferred();
      const waiting = yield* fork(deferredAwait(handshake));
      yield* deferredFail(handshake, { kind: "unavailable" });
      return yield* exit(join(waiting));
    });

    const result = await runPromise(program);
    if (result.kind === "failure" && result.cause.kind === "fail") {
      expect(result.cause.error.kind).toBe("unavailable");
    } else {
      throw new Error("expected the deferred's typed failure");
    }
  });

  it("answers a deferred that is already done without waiting", async () => {
    const program = effect(function* () {
      const handshake = yield* deferred();
      const before = yield* deferredIsDone(handshake);
      yield* deferredSucceed(handshake, 7);
      const after = yield* deferredIsDone(handshake);
      return [before, after, yield* deferredAwait(handshake)];
    });

    await expect(runPromise(program)).resolves.toEqual([false, true, 7]);
  });

  it("stops a fiber blocked on a deferred rather than hanging", async () => {
    // Nothing will ever complete this one. A hand-written promise would make
    // the fiber uninterruptible, which is the bug the waker protocol exists
    // to prevent — the same one `never` is written the way it is to avoid.
    const program = effect(function* () {
      const handshake = yield* deferred();
      const waiting = yield* fork(deferredAwait(handshake));
      yield* sleep(5);
      return yield* interrupt(waiting);
    });

    const result = await runPromise(program);
    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("interrupt");
    }
  });

  it("limits concurrency across two independent call sites", async () => {
    // `all`'s concurrency bounds one call. Two `forEach`es against the same
    // rate-limited host need one budget between them, and this is it.
    let inFlight = 0;
    let peak = 0;
    const program = effect(function* () {
      const budget = yield* semaphore(2);
      const request = (item: number) =>
        withPermit(
          budget,
          effect(function* () {
            inFlight += 1;
            peak = Math.max(peak, inFlight);
            yield* sleep(5);
            inFlight -= 1;
            return item;
          }),
        );

      return yield* all(
        [
          forEach([1, 2, 3], request, { concurrency: "unbounded" }),
          forEach([4, 5, 6], request, { concurrency: "unbounded" }),
        ],
        { concurrency: "unbounded" },
      );
    });

    await expect(runPromise(program)).resolves.toEqual([
      [1, 2, 3],
      [4, 5, 6],
    ]);
    expect(peak).toBe(2);
  });

  it("gives a permit back when the fiber holding it is interrupted", async () => {
    // A permit that leaks on cancellation is a budget that shrinks every time
    // somebody cancels a request, until nothing can run at all.
    const program = effect(function* () {
      const budget = yield* semaphore(1);
      const holder = yield* fork(withPermit(budget, sleep(400)));
      yield* sleep(10);
      yield* interrupt(holder);
      // If the permit had leaked, this would never finish.
      return yield* withPermit(budget, succeed("took it"));
    });

    await expect(runPromise(timeout(program, 300))).resolves.toBe("took it");
  });

  it("gives a permit back when the body fails", async () => {
    const program = effect(function* () {
      const budget = yield* semaphore(1);
      yield* exit(withPermit(budget, fail("body")));
      return yield* withPermit(budget, succeed("took it"));
    });

    await expect(runPromise(timeout(program, 300))).resolves.toBe("took it");
  });

  it("serves the queue in the order it was asked in", async () => {
    // A later small request that happens to fit must not overtake a waiting
    // large one, or the widest caller can be starved for ever.
    const order = [];
    const program = effect(function* () {
      const budget = yield* semaphore(2);
      // Holds one of two, so one permit stays free the whole time.
      const holder = yield* fork(
        withPermit(
          budget,
          effect(function* () {
            yield* sleep(20);
            order.push("holder");
          }),
        ),
      );
      yield* sleep(5);
      // Wants both, so it has to wait for the holder.
      const wide = yield* fork(
        withPermits(
          budget,
          2,
          sync(() => order.push("wide")),
        ),
      );
      yield* sleep(5);
      // Would fit right now — the free permit is there — and must not take it.
      const narrow = yield* fork(
        withPermit(
          budget,
          sync(() => order.push("narrow")),
        ),
      );
      yield* join(holder);
      yield* join(wide);
      yield* join(narrow);
      return order;
    });

    await expect(runPromise(program)).resolves.toEqual(["holder", "wide", "narrow"]);
  });

  it("refuses more permits than the semaphore has rather than waiting for ever", async () => {
    const program = effect(function* () {
      const budget = yield* semaphore(2);
      return yield* exit(withPermits(budget, 3, succeed("never")));
    });

    const result = await runPromise(program);
    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("die");
    }
  });

  it("serialises an effectful update so the second write does not lose the first", async () => {
    // Two fibers read, both await, both write. Without the lock the second
    // write is computed from a value the first has already replaced.
    const program = effect(function* () {
      const held = yield* ref(0);
      const lock = yield* semaphore(1);
      const increment = () =>
        refUpdateEffect(held, lock, (value) =>
          effect(function* () {
            yield* sleep(5);
            return value + 1;
          }),
        );

      yield* all([increment(), increment(), increment()], { concurrency: "unbounded" });
      return yield* refGet(held);
    });

    await expect(runPromise(program)).resolves.toBe(3);
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

  it("builds a layer reached twice in one graph exactly once", async () => {
    // The diamond: `Database` and `Logger` both want `Config`. Built once per
    // path, a layer that opens a connection pool opens two, and a layer that
    // reads a config file may get two different answers.
    const Config = tag("Config");
    const Database = tag("Database");
    const Logger = tag("Logger");

    let builds = 0;
    const configLayer = layerEffect(
      Config,
      sync(() => {
        builds += 1;
        return { url: "postgres://" };
      }),
    );
    const databaseLayer = layerProvide(
      layerEffect(
        Database,
        effect(function* () {
          const config = yield* Config;
          return { at: config.url };
        }),
      ),
      configLayer,
    );
    const loggerLayer = layerProvide(
      layerEffect(
        Logger,
        effect(function* () {
          const config = yield* Config;
          return { about: config.url };
        }),
      ),
      configLayer,
    );

    const program = effect(function* () {
      const database = yield* Database;
      const logger = yield* Logger;
      return `${database.at}|${logger.about}`;
    });

    await expect(
      runPromise(provide(program, layerMerge(databaseLayer, loggerLayer))),
    ).resolves.toBe("postgres://|postgres://");
    expect(builds).toBe(1);
  });

  it("builds a layer again for the next provide", async () => {
    // The honest other half: the memo lives for one build, not for the
    // process. Building once for a whole application is a `Runtime`, and there
    // is not one yet.
    const Config = tag("Config");
    let builds = 0;
    const configLayer = layerEffect(
      Config,
      sync(() => {
        builds += 1;
        return { url: "postgres://" };
      }),
    );

    await runPromise(provide(succeed(1), configLayer));
    await runPromise(provide(succeed(2), configLayer));
    expect(builds).toBe(2);
  });

  it("answers synchronously when the layer and the body both can", () => {
    // Nothing here is asynchronous, and until `provide` had a synchronous
    // kernel this died with "effect is asynchronous" — which made "use a
    // layer" and "use runSync" mutually exclusive, including in a test.
    const Clock = tag("Clock");
    expect(runSync(provide(succeed(1), layerSucceed(Clock, { now: () => 1 })))).toBe(1);

    const reading = effect(function* () {
      const clock = yield* Clock;
      return clock.now();
    });
    expect(
      runSync(
        provide(
          reading,
          layerEffect(
            Clock,
            sync(() => ({ now: () => 7 })),
          ),
        ),
      ),
    ).toBe(7);
  });

  it("refuses synchronously when the layer needs to wait", () => {
    const Clock = tag("Clock");
    const result = runSyncExit(
      provide(
        succeed(1),
        layerEffect(
          Clock,
          promise(() => Promise.resolve({ now: () => 1 })),
        ),
      ),
    );

    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("die");
    }
  });

  it("feeds one layer into another with layerProvide", async () => {
    const Config = tag("Config");
    const Database = tag("Database");
    const configLayer = layerSucceed(Config, { url: "postgres://" });
    const databaseLayer = layerProvide(
      layerEffect(
        Database,
        effect(function* () {
          const config = yield* Config;
          return { at: config.url };
        }),
      ),
      configLayer,
    );

    const program = effect(function* () {
      const database = yield* Database;
      return database.at;
    });

    // `Config` is discharged: the program is run with the database layer alone
    // and never sees the service the layer needed.
    await expect(runPromise(provide(program, databaseLayer))).resolves.toBe("postgres://");
  });

  it("keeps the outer services with layerProvideMerge", async () => {
    const Config = tag("Config");
    const Database = tag("Database");
    const configLayer = layerSucceed(Config, { url: "postgres://" });
    const databaseLayer = layerProvideMerge(
      layerEffect(
        Database,
        effect(function* () {
          const config = yield* Config;
          return { at: config.url };
        }),
      ),
      configLayer,
    );

    const program = effect(function* () {
      const database = yield* Database;
      const config = yield* Config;
      return `${database.at}|${config.url}`;
    });

    await expect(runPromise(provide(program, databaseLayer))).resolves.toBe(
      "postgres://|postgres://",
    );
  });

  it("carries an inner layer's failure out of layerProvide", async () => {
    const Config = tag("Config");
    const Database = tag("Database");
    const databaseLayer = layerProvide(
      layerEffect(Database, fail({ kind: "NoDatabase" })),
      layerSucceed(Config, { url: "postgres://" }),
    );

    const result = await runPromiseExit(provide(succeed(1), databaseLayer));
    if (result.kind === "failure" && result.cause.kind === "fail") {
      expect(result.cause.error.kind).toBe("NoDatabase");
    } else {
      throw new Error("expected the inner layer's failure");
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

describe("a layer that acquires something", () => {
  /** A pool layer that records every open and close. */
  const pooling = (events: Array<string>) => {
    const Pool = tag("Pool");
    return {
      Pool,
      layer: layerScoped(
        Pool,
        acquireRelease(
          sync(() => {
            events.push("open");
            return { query: () => "row" };
          }),
          () => sync(() => events.push("close")),
        ),
      ),
    };
  };

  it("releases when the body succeeds", async () => {
    const events: Array<string> = [];
    const pool = pooling(events);
    const program = effect(function* () {
      const handle = yield* pool.Pool;
      events.push(handle.query());
      return "done";
    });

    await expect(runPromise(provide(program, pool.layer))).resolves.toBe("done");
    expect(events).toEqual(["open", "row", "close"]);
  });

  it("releases when the body fails", async () => {
    const events: Array<string> = [];
    const pool = pooling(events);
    const program = effect(function* () {
      yield* pool.Pool;
      return yield* fail("body");
    });

    const result = await runPromiseExit(provide(program, pool.layer));
    if (result.kind === "failure" && result.cause.kind === "fail") {
      expect(result.cause.error).toBe("body");
    } else {
      throw new Error("expected the body's failure to survive the release");
    }
    expect(events).toEqual(["open", "close"]);
  });

  it("releases when the fiber is interrupted", async () => {
    const events: Array<string> = [];
    const pool = pooling(events);
    const program = effect(function* () {
      yield* pool.Pool;
      yield* sleep(400);
    });

    const outcome = await runPromise(
      effect(function* () {
        const fiber = yield* fork(provide(program, pool.layer));
        yield* sleep(10);
        return yield* interrupt(fiber);
      }),
    );

    expect(outcome.kind).toBe("failure");
    if (outcome.kind === "failure") {
      expect(outcome.cause.kind).toBe("interrupt");
    }
    expect(events).toEqual(["open", "close"]);
  });

  it("releases synchronously when everything in the program can", () => {
    // A scoped layer does not force the program asynchronous either: the
    // finalizers close on the synchronous path too.
    const events: Array<string> = [];
    const pool = pooling(events);
    const program = effect(function* () {
      const handle = yield* pool.Pool;
      return handle.query();
    });

    expect(runSync(provide(program, pool.layer))).toBe("row");
    expect(events).toEqual(["open", "close"]);
  });

  it("keeps the pool open for the whole body rather than for the build", async () => {
    // The resource has to outlive the build that acquired it — a layer that
    // closed its pool when the build finished would hand the body a closed
    // one, which is the failure a scope on the layer exists to prevent.
    const events: Array<string> = [];
    const pool = pooling(events);
    const program = effect(function* () {
      const handle = yield* pool.Pool;
      yield* sleep(5);
      return handle.query();
    });

    await expect(runPromise(provide(program, pool.layer))).resolves.toBe("row");
    expect(events).toEqual(["open", "close"]);
  });
});

describe("streams", () => {
  it("collects what a source produces, in order", async () => {
    const collected = await runPromise(
      streamRunCollect(streamMap(streamFromArray([1, 2, 3, 4]), (value) => value * 2)),
    );
    expect(collected).toEqual([2, 4, 6, 8]);
  });

  it("keeps take honest whatever the batching was", async () => {
    // The batch size is a throughput knob and nothing else depends on it.
    const wide = streamFromArray([1, 2, 3, 4, 5], { chunkSize: 4 });
    const narrow = streamFromArray([1, 2, 3, 4, 5], { chunkSize: 1 });
    await expect(runPromise(streamRunCollect(streamTake(wide, 3)))).resolves.toEqual([1, 2, 3]);
    await expect(runPromise(streamRunCollect(streamTake(narrow, 3)))).resolves.toEqual([1, 2, 3]);
  });

  it("does not treat an emptied batch as the end", async () => {
    // A filter that rejects everything in one batch returns `[]`, and only
    // `null` ends a traversal.
    const evens = streamFilter(
      streamFromArray([1, 3, 5, 2, 7, 4], { chunkSize: 3 }),
      (value) => value % 2 === 0,
    );
    await expect(runPromise(streamRunCollect(evens))).resolves.toEqual([2, 4]);
  });

  it("takes three from an infinite source and releases what it opened", async () => {
    const events = [];
    const counting = function* () {
      for (let value = 0; ; value += 1) {
        yield value;
      }
    };
    const source = streamEnsuring(streamFromIterator(counting, { chunkSize: 1 }), () =>
      sync(() => events.push("closed")),
    );

    await expect(runPromise(streamRunCollect(streamTake(source, 3)))).resolves.toEqual([0, 1, 2]);
    // The traversal ended without draining the source, and the finalizer still
    // ran — the guarantee `acquireRelease` gives, for a stream.
    expect(events).toEqual(["closed"]);
  });

  it("reports a failure mid-stream and not the elements before it", async () => {
    const failing = streamMapEffect(streamFromArray([1, 2, 3, 4]), (value) =>
      value === 3 ? fail({ kind: "row", value }) : succeed(value),
    );

    const result = await runPromiseExit(streamRunCollect(failing));
    if (result.kind === "failure" && result.cause.kind === "fail") {
      expect(result.cause.error).toEqual({ kind: "row", value: 3 });
    } else {
      throw new Error("expected the failing row's typed error");
    }
  });

  it("keeps a defect a defect", async () => {
    const broken = streamMap(streamFromArray([1, 2, 3]), (value) => {
      if (value === 2) {
        throw new Error("bad transform");
      }
      return value;
    });

    const result = await runPromiseExit(streamRunCollect(broken));
    expect(result.kind).toBe("failure");
    if (result.kind === "failure") {
      expect(result.cause.kind).toBe("die");
    }
  });

  it("closes the source when the failure came from a step", async () => {
    const events = [];
    const source = streamEnsuring(streamFromArray([1, 2, 3]), () =>
      sync(() => events.push("closed")),
    );

    await runPromiseExit(streamRunCollect(streamMapEffect(source, () => fail("no"))));
    expect(events).toEqual(["closed"]);
  });

  it("stops pulling when the fiber draining it is interrupted, and closes", async () => {
    const events = [];
    let pulled = 0;
    const slow = function* () {
      for (let value = 0; ; value += 1) {
        pulled += 1;
        yield value;
      }
    };
    const source = streamEnsuring(streamFromIterator(slow, { chunkSize: 1 }), () =>
      sync(() => events.push("closed")),
    );
    const draining = streamRunForEach(source, () => sleep(5));

    const outcome = await runPromise(
      effect(function* () {
        const fiber = yield* fork(draining);
        yield* sleep(30);
        return yield* interrupt(fiber);
      }),
    );

    expect(outcome.kind).toBe("failure");
    if (outcome.kind === "failure") {
      expect(outcome.cause.kind).toBe("interrupt");
    }
    expect(events).toEqual(["closed"]);

    const seen = pulled;
    await runPromise(sleep(40));
    expect(pulled).toBe(seen);
  });

  it("preserves order under a concurrency limit, and honours the limit", async () => {
    let inFlight = 0;
    let peak = 0;
    // One element per batch, so the window has to be filled across batch
    // boundaries for the limit to mean anything at all.
    const source = streamFromArray([1, 2, 3, 4, 5, 6], { chunkSize: 1 });
    const mapped = streamMapEffect(
      source,
      (value: number) =>
        effect(function* () {
          inFlight += 1;
          peak = Math.max(peak, inFlight);
          yield* sleep(5);
          inFlight -= 1;
          return value * 10;
        }),
      { concurrency: 3 },
    );

    await expect(runPromise(streamRunCollect(mapped))).resolves.toEqual([10, 20, 30, 40, 50, 60]);
    expect(peak).toBe(3);
  });

  it("walks a paginated source until a page says there is no next one", async () => {
    const pages: Map<string, { items: Array<number>, next: string | null }> = new Map([
      ["first", { items: [1, 2], next: "second" }],
      ["second", { items: [3], next: "third" }],
      ["third", { items: [4, 5], next: null }],
    ]);
    const walked: Array<string> = [];
    const source = streamPaginate("first", (cursor: string) =>
      sync(() => {
        walked.push(cursor);
        const page = pages.get(cursor);
        if (page == null) {
          throw new Error(`no page ${cursor}`);
        }
        return page;
      }),
    );

    await expect(runPromise(streamRunCollect(source))).resolves.toEqual([1, 2, 3, 4, 5]);
    expect(walked).toEqual(["first", "second", "third"]);
  });

  it("does not ask a paginated source for a page it will not use", async () => {
    let requested = 0;
    const source = streamPaginate(0, (cursor: number) =>
      sync(() => {
        requested += 1;
        return { items: [cursor], next: cursor + 1 };
      }),
    );

    await expect(runPromise(streamRunCollect(streamTake(source, 2)))).resolves.toEqual([0, 1]);
    expect(requested).toBe(2);
  });

  it("folds, drains and reads a head without collecting the rest", async () => {
    const source = streamFromArray([1, 2, 3, 4]);
    await expect(runPromise(streamRunFold(source, 0, (total, item) => total + item))).resolves.toBe(
      10,
    );
    await expect(runPromise(streamRunDrain(source))).resolves.toBe(undefined);
    await expect(runPromise(streamRunHead(source))).resolves.toBe(1);
    await expect(runPromise(streamRunHead(streamFromArray([])))).resolves.toBe(null);
  });

  it("traverses the same stream twice, from the start each time", async () => {
    // `open` per traversal is what makes this true; an iterator handed over as
    // a value would leave the second traversal empty.
    const source = streamFromIterator(function* () {
      yield 1;
      yield 2;
    });

    await expect(runPromise(streamRunCollect(source))).resolves.toEqual([1, 2]);
    await expect(runPromise(streamRunCollect(source))).resolves.toEqual([1, 2]);
  });

  it("reads a web ReadableStream and cancels it when it stops early", async () => {
    // Structural, so this is the same shape `Response.body` has on Node, Deno,
    // Bun and an edge runtime — and a double can stand in for all four.
    let cancelled = 0;
    const chunks = ["a", "b", "c", "d"];
    const readable = () => {
      let index = 0;
      return {
        getReader: () => ({
          read: () =>
            Promise.resolve(
              index >= chunks.length ? { done: true } : { done: false, value: chunks[index++] },
            ),
          cancel: () => {
            cancelled += 1;
            return Promise.resolve();
          },
        }),
      };
    };

    await expect(runPromise(streamRunCollect(streamFromReadableStream(readable)))).resolves.toEqual(
      ["a", "b", "c", "d"],
    );

    await expect(
      runPromise(streamRunCollect(streamTake(streamFromReadableStream(readable), 2))),
    ).resolves.toEqual(["a", "b"]);
    // Stopping early closes the connection rather than reading to the end.
    expect(cancelled).toBe(2);
  });

  it("makes a stream from an effect, and one element out of it", async () => {
    await expect(runPromise(streamRunCollect(streamFromEffect(succeed(7))))).resolves.toEqual([7]);

    const result = await runPromiseExit(streamRunCollect(streamFromEffect(fail("no"))));
    expect(result.kind).toBe("failure");
  });

  it("looks at every element without changing it", async () => {
    const seen = [];
    const source = streamTap(streamFromArray([1, 2, 3]), (value) => sync(() => seen.push(value)));

    await expect(runPromise(streamRunCollect(source))).resolves.toEqual([1, 2, 3]);
    expect(seen).toEqual([1, 2, 3]);
  });
});
