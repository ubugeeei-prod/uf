// @flow
//
// `bench`: registered like a test, skipped by an ordinary run, and timed by a
// run of benchmarks.
//
// Each case builds a registry of its own and runs it with the runner's own
// `run`, as `suite-hooks.test.js` does, so what is asserted is what the worker
// reports for a file holding these declarations.

import { describe, expect, it } from "@uniflowed/test";
import { bench as registerBench, it as registerCase, reset } from "./internal/registry.js";
import { type Result, run as runRegistered } from "./internal/run.js";

/** Run whatever was registered since `reset()`, and collect every result. */
async function results(benches: boolean): Promise<Array<Result>> {
  const collected: Array<Result> = [];
  await runRegistered({ file: "virtual.bench.js", bench: benches }, (result) => {
    collected.push(result);
  });
  return collected;
}

describe("bench", () => {
  it("is skipped by an ordinary run, which never calls its body", async () => {
    let calls = 0;
    reset();
    registerBench("counts its calls", () => {
      calls += 1;
    });
    registerCase("is a test", () => {});

    const reported = await results(false);

    expect(calls).toBe(0);
    expect(reported.map((result) => [result.name, result.outcome.status])).toEqual([
      ["counts its calls", "skipped"],
      ["is a test", "passed"],
    ]);
    expect(reported[0].outcome).toEqual({ status: "skipped", reason: "bench" });
  });

  it("is timed by a run of benchmarks, which skips the tests", async () => {
    let calls = 0;
    reset();
    registerBench(
      "counts its calls",
      () => {
        calls += 1;
      },
      { warmup: 3, iterations: 7 },
    );
    registerCase("is a test", () => {});

    const reported = await results(true);

    expect(calls).toBe(10);
    const outcome = reported[0].outcome;
    expect(outcome.status).toBe("passed");
    const samples = outcome.status === "passed" ? (outcome.samples ?? []) : [];
    expect(samples.length).toBe(7);
    expect(samples.every((sample) => Number.isInteger(sample) && sample >= 0)).toBe(true);
    expect(reported[1].outcome).toEqual({ status: "skipped", reason: "not-bench" });
  });

  it("awaits an async body on every call", async () => {
    let finished = 0;
    reset();
    registerBench(
      "waits",
      async () => {
        await Promise.resolve();
        finished += 1;
      },
      { warmup: 0, iterations: 4 },
    );

    await results(true);

    expect(finished).toBe(4);
  });

  it("refuses a count that is not a whole number in range, naming the option", async () => {
    reset();
    registerBench("never runs", () => {}, { iterations: 0 });

    const reported = await results(true);

    const outcome = reported[0].outcome;
    expect(outcome.status).toBe("failed");
    expect(outcome.status === "failed" ? outcome.message : "").toContain(
      "bench `iterations` has to be a whole number from 1 to 100000, and was 0",
    );
  });

  it("keeps `.skip` and `.todo` ahead of the mode", async () => {
    reset();
    registerBench.skip("is off", () => {});
    registerBench.todo("is later");

    const reported = await results(true);

    expect(reported.map((result) => result.outcome.status)).toEqual(["skipped", "todo"]);
  });
});
