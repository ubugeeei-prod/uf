// @flow
//
// The process `uf test` fans work out to.
//
// One worker per core, each running whole files one at a time: `uf` writes a
// request per line on stdin, the worker imports that file (through the host's
// Flow loader, so the module is transformed by the same `uf transform` the
// build uses), runs what it registered, and writes one event per line back.
//
//   → {"file": "src/math.test.js", "filter": "adds", "timeoutMs": 5000}
//   ← {"event": "test", "name": "math > adds", "status": "passed", …}
//   ← {"event": "file", "status": "completed", "durationMicros": 1234}
//
// Two decisions worth stating. Results are streamed as they happen rather than
// batched at the end, so `uf test` can draw progress and `--bail` can stop a
// long run early. And a file that throws while being *imported* is a file
// result, not a test result: there were no tests to fail, and saying "0 tests"
// for a module that could not load would be a lie.
//
// This module runs on import by design — it is a process entry point, the way
// `@uniflowed/vite`'s loaders are.

import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

import { reset } from "./internal/registry.js";
import { run } from "./internal/run.js";

/** What `uf` sends for one file. */
type Request = {|
  +file: string,
  +filter?: string | null,
  +timeoutMs?: number,
|};

function write(event: { +[string]: mixed }): void {
  process.stdout.write(`${JSON.stringify(event)}\n`);
}

/**
 * Import and run one file.
 *
 * The module is imported with a cache-busting query so a watch-mode rerun in
 * the same worker sees the edited file rather than the one the module registry
 * already holds.
 */
async function runFile(request: Request, generation: number): Promise<void> {
  const started = performance.now();
  reset();

  try {
    await import(`${pathToFileURL(request.file).href}?uf-run=${generation}`);
  } catch (thrown) {
    const error = thrown instanceof Error ? thrown : new Error(String(thrown));
    write({
      event: "file",
      status: "load-failed",
      message: `${error.name}: ${error.message}`,
      stack: error.stack ?? null,
      durationMicros: Math.round((performance.now() - started) * 1000),
    });
    return;
  }

  try {
    await run({ filter: request.filter ?? null, timeoutMs: request.timeoutMs }, (result) => {
      write({ event: "test", ...result.outcome, name: result.name, line: result.line, column: result.column, durationMicros: result.durationMicros });
    });
    write({
      event: "file",
      status: "completed",
      durationMicros: Math.round((performance.now() - started) * 1000),
    });
  } catch (thrown) {
    const error = thrown instanceof Error ? thrown : new Error(String(thrown));
    write({
      event: "file",
      status: "run-failed",
      message: `${error.name}: ${error.message}`,
      stack: error.stack ?? null,
      durationMicros: Math.round((performance.now() - started) * 1000),
    });
  }
}

/**
 * Serve requests until stdin closes.
 *
 * Requests are queued and served strictly in order: a worker runs one file at
 * a time, because two files sharing a process would share globals and module
 * state, and a test suite that passes alone but fails beside another is the
 * worst failure a runner can produce.
 */
function serve(): void {
  let queue: Promise<void> = Promise.resolve();
  let generation = 0;

  createInterface({ input: process.stdin }).on("line", (line) => {
    if (line.trim() === "") {
      return;
    }
    let request: Request;
    try {
      request = JSON.parse(line);
    } catch (error) {
      write({ event: "file", status: "run-failed", message: `malformed request: ${String(error)}` });
      return;
    }
    generation += 1;
    const at = generation;
    queue = queue.then(() => runFile(request, at));
  });

  process.stdin.on("close", () => {
    queue.then(() => process.exit(0));
  });
}

// Unhandled rejections would otherwise take the worker down mid-file with no
// explanation; reporting one as a file failure keeps the run honest.
process.on("unhandledRejection", (reason: mixed) => {
  const error = reason instanceof Error ? reason : new Error(String(reason));
  write({
    event: "file",
    status: "run-failed",
    message: `unhandled rejection: ${error.message}`,
    stack: error.stack ?? null,
  });
  process.exit(1);
});

serve();
