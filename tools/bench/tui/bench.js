// @flow
//
// `@uniflowed/tui` against React Ink, on the same workload, three ways.
//
//   uf run bench:tui
//
// # What this measures, and what it does not
//
// Two numbers per step, because they are two different claims:
//
// * **bytes written** is the mechanism. A terminal emulator on the other end
//   of a pipe parses and re-renders everything it is sent, so this is the
//   number the renderers actually differ *about*, and it is deterministic —
//   the same input produces the same count on any machine, which is why this
//   file refuses to report it if two runs disagree.
// * **wall clock, from the state update to the write returning**, is what a
//   person feels. It includes each library's own scheduling, because a
//   renderer that is fast and then waits thirty-three milliseconds for its
//   frame limiter is not fast. It is noisy, so it is reported as a median over
//   many runs with the spread beside it, and never as a single number.
//
// Not measured: input. Ink has no equivalent of `@uniflowed/tui`'s `Input`, so
// there is nothing to compare a keystroke against.
//
// # Three configurations, not two
//
// Ink 7 has an `incrementalRendering` option that is **off** by default, and
// it changes the answer by an order of magnitude — so measuring only the
// default would be choosing the comparison uf wins by more. Both settings are
// here, and the guide reports both.
//
// # Fairness
//
// Both libraries render the same frame (`workload.js`) into the same kind of
// stream (`sink.js`) from the same state container (`store.js`), at 80×24,
// with colour off and no border. Each library is warmed up before it is
// measured, and each sample mounts a fresh application so that no sample
// inherits another's frame. Ink's frame limiter is raised out of the way, so
// its wall-clock number is its renderer's rather than its throttle's.

import { performance } from "node:perf_hooks";
import process from "node:process";

import { createSink } from "./sink.js";
import type { Sink } from "./sink.js";
import { createStore } from "./store.js";
import type { Store } from "./store.js";
import { HEIGHT, START, STEPS, WIDTH, lines } from "./workload.js";

/** One measured step of one run. */
type Sample = { readonly bytes: number, readonly ms: number };

/** What a configuration must provide to be measured. */
type Driver = {
  readonly name: string,
  /** Mount, and return the function that takes it down again. */
  start(sink: Sink, store: Store): Promise<() => void>,
};

/** What one step of one configuration comes out as. */
type Row = {
  readonly bytes: number,
  readonly medianMs: number,
  readonly minMs: number,
  readonly maxMs: number,
};

const NAMES: $ReadOnlyArray<string> = ["first", ...STEPS.map((step) => step.name)];

/** Yield to the macrotask queue, which is where both schedulers finish. */
const turn = (): Promise<void> =>
  new Promise((resolve) => {
    setTimeout(resolve, 0);
  });

/**
 * Wait for a frame to land.
 *
 * A write, and then one more turn of the loop in case the library writes its
 * frame in more than one piece. Waiting for silence rather than for a
 * library's own "flushed" promise is what keeps the two stopwatches measuring
 * the same thing.
 */
async function settleWrites(sink: Sink): Promise<void> {
  for (let attempt = 0; attempt < 1000 && sink.pending() === 0; attempt += 1) {
    await turn();
  }
  await turn();
}

/** What the sink saw, relative to when the state changed. */
function sample(sink: Sink, name: string, startedAt: number): Sample {
  const taken = sink.take();
  if (taken.writes === 0) {
    // Every step of this workload changes the frame, so a step that produced
    // no write is a broken measurement rather than a fast one — and "0 bytes"
    // is exactly the kind of number a benchmark should refuse to print.
    throw new Error(`"${name}" produced no write at all; the harness is measuring nothing`);
  }
  return { bytes: taken.bytes, ms: taken.at - startedAt };
}

/** Run one configuration `runs` times and collect every step. */
async function measure(driver: Driver, runs: number): Promise<{ [string]: Array<Sample> }> {
  const series: { [string]: Array<Sample> } = {};
  for (const name of NAMES) {
    series[name] = [];
  }

  for (let run = 0; run < runs; run += 1) {
    const sink = createSink(WIDTH, HEIGHT);
    const store = createStore(START);

    const mountedAt = performance.now();
    const stop = await driver.start(sink, store);
    await settleWrites(sink);
    series.first.push(sample(sink, "first", mountedAt));

    for (const step of STEPS) {
      const startedAt = performance.now();
      store.set(step.to);
      await settleWrites(sink);
      series[step.name].push(sample(sink, step.name, startedAt));
    }
    stop();
    await turn();
  }
  return series;
}

/** `@uniflowed/tui`, the package this repository ships. */
async function ufDriver(): Promise<Driver> {
  const tui = await import("@uniflowed/tui");
  const React = await import("@uniflowed/react");

  component UfApp(store: Store) {
    const frame = React.useSyncExternalStore(store.subscribe, store.snapshot, store.snapshot);
    return React.createElement(
      tui.Box,
      { flexDirection: "column" },
      ...lines(frame).map((line, index) =>
        React.createElement(tui.Text, { key: String(index), wrap: "none" }, line),
      ),
    );
  }

  return {
    name: "@uniflowed/tui",
    async start(sink: Sink, store: Store) {
      const handle = tui.render(React.createElement(UfApp, { store }), {
        stdout: sink,
        // Nothing is typed at this, and a renderer holding the real stdin open
        // would keep the process alive after the benchmark finished.
        stdin: { isTTY: false },
        color: "never",
        env: { COLUMNS: String(WIDTH), LINES: String(HEIGHT) },
        alternateScreen: false,
      });
      return () => {
        handle.stop();
      };
    },
  };
}

/** A stdin for a library that insists on having one. */
function quietStdin() {
  return {
    isTTY: false,
    on() {},
    off() {},
    once() {},
    removeListener() {},
    setRawMode() {},
    setEncoding() {},
    resume() {},
    pause() {},
    ref() {},
    unref() {},
    read() {
      return null;
    },
  };
}

/** React Ink, at the two settings that draw the same picture differently. */
async function inkDriver(incremental: boolean): Promise<Driver> {
  const ink = await import("ink");
  const React = await import("react");

  // Deliberately not Flow's `component` syntax, which is what the rule below
  // asks for and what the uf application above uses. uf compiles a `component`
  // through the React Compiler, and the compiled output calls `c()` from
  // `react/compiler-runtime` — resolved, from this file, out of the repository
  // root. That is uf's React, and this component has to run inside Ink's; two
  // Reacts in one process means two hook dispatchers, and the second renderer
  // to mount finds a null one. It is not a style preference here, it is the
  // difference between a benchmark and a crash.
  // uf-lint-disable-next-line react/component-syntax
  const InkApp = ({ store }: { store: Store }) => {
    const frame = React.useSyncExternalStore(store.subscribe, store.snapshot, store.snapshot);
    return React.createElement(
      ink.Box,
      { flexDirection: "column" },
      ...lines(frame).map((line, index) =>
        React.createElement(ink.Text, { key: String(index), wrap: "truncate" }, line),
      ),
    );
  };

  return {
    name: incremental ? "ink (incrementalRendering)" : "ink (default)",
    async start(sink: Sink, store: Store) {
      const instance = ink.render(React.createElement(InkApp, { store }), {
        stdout: sink,
        stderr: sink,
        stdin: quietStdin(),
        patchConsole: false,
        exitOnCtrlC: false,
        // The frame limiter is thirty frames a second by default. Leaving it
        // there would measure the limiter rather than the renderer; it does not
        // touch the byte counts either way.
        maxFps: 10_000,
        incrementalRendering: incremental,
      });
      return () => {
        instance.unmount();
      };
    },
  };
}

/** The median of a copy, since the caller's order is the run order. */
function median(values: Array<number>): number {
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 === 0 ? (sorted[middle - 1] + sorted[middle]) / 2 : sorted[middle];
}

/** Three decimal places, which is a microsecond and is already optimistic. */
const round = (value: number): number => Number(value.toFixed(3));

/** Reduce one configuration's samples to the row the guide prints. */
function summarise(driver: Driver, series: { [string]: Array<Sample> }): { [string]: Row } {
  const rows: { [string]: Row } = {};
  for (const name of NAMES) {
    const samples = series[name];
    const bytes = samples.map((entry) => entry.bytes);
    const low = Math.min(...bytes);
    const high = Math.max(...bytes);
    if (low !== high) {
      throw new Error(
        `${driver.name} wrote ${String(low)}..${String(high)} bytes for "${name}" across runs: ` +
          "the workload is not deterministic, so no byte count from it is worth publishing",
      );
    }
    const times = samples.map((entry) => entry.ms);
    rows[name] = {
      bytes: low,
      medianMs: round(median(times)),
      minMs: round(Math.min(...times)),
      maxMs: round(Math.max(...times)),
    };
  }
  return rows;
}

async function main(): Promise<void> {
  const runs = Number.parseInt(process.env.BENCH_RUNS ?? "60", 10);
  const drivers = [await ufDriver(), await inkDriver(false), await inkDriver(true)];
  const results: { [string]: { [string]: Row } } = {};

  for (const driver of drivers) {
    // Warm up. The first mount of anything on a JIT measures the JIT. The
    // samples are thrown away, and it is said out loud here because a
    // benchmark that warms up quietly is a benchmark that can be accused of it.
    await measure(driver, 3);
    results[driver.name] = summarise(driver, await measure(driver, runs));
  }

  process.stdout.write(
    `${JSON.stringify(
      {
        node: process.version,
        platform: `${process.platform} ${process.arch}`,
        terminal: `${String(WIDTH)}x${String(HEIGHT)}`,
        runs,
        results,
      },
      null,
      2,
    )}\n`,
  );
}

// Not top-level `await`: the Flow parser uf vendors does not accept it, which
// is a real limit of running a Flow entry point on the Capability JS Host and
// is worth meeting here rather than in somebody's application.
main().catch((error: mixed) => {
  process.stderr.write(`${String(error)}\n`);
  process.exitCode = 1;
});
