// @flow
//
// A worker `uf test --watch` keeps between runs, told that a file changed.
//
// The promise is the one `crates/uf_test/src/pool.rs` makes: after an edit, a
// kept worker's next file sees the edit — through every module between it and
// the test — and keeps every module the edit does not reach. And a worker that
// cannot promise that says so, so `uf` can run the next file in a fresh one
// instead of in one that still holds the code as it was.
//
// Driven the way `uf` drives it, through the worker's own protocol, because the
// question is about one process across several requests and that is the only
// place it can be asked. The fixtures are written to a temporary directory: a
// file in this repository that registers cases would be collected as a test.

import { denoWorkerArguments } from "../../tests/library/deno-worker.js";
import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { describe, expect, it } from "@uniflowed/test";

const here = path.dirname(fileURLToPath(String(import.meta.url)));
const repository = path.resolve(here, "..", "..");

/** How this host starts a worker with its Flow loader, as `uf` would. */
function loaderArguments(): Array<string> {
  const host = path.basename(process.execPath);
  if (host.startsWith("deno")) return denoWorkerArguments(repository);
  if (host.startsWith("node")) {
    const register = path.join(repository, "packages", "host", "register.js");
    return ["--enable-source-maps", "--import", String(pathToFileURL(register).href)];
  }
  if (host.startsWith("bun")) {
    return ["--preload", path.join(repository, "packages", "host", "bun-preload.js")];
  }
  throw new Error(`no Flow loader for ${host}: this test drives the worker uf would have started`);
}

/** Whether this host has the in-thread loader a kept worker answers with. */
function hasInThreadHooks(): boolean {
  return !path.basename(process.execPath).startsWith("bun");
}

type Answer = { event: string, status?: string, ok?: boolean, text?: string };

/**
 * A worker started as `uf` starts one, and a way to ask it one thing at a time.
 *
 * `keep` is `UF_TEST_KEEP_WORKERS`, which `uf test --watch` sets.
 */
function startWorker(keep: boolean): {
  ask: (request: { ... }) => Promise<Array<Answer>>,
  stop: () => void,
} {
  const worker = path.join(repository, "packages", "test", "worker.js");
  const child = spawn(process.execPath, [...loaderArguments(), worker], {
    env: { ...process.env, UF_TEST_KEEP_WORKERS: keep ? "1" : "" },
    stdio: ["pipe", "pipe", "inherit"],
  });
  let buffer = "";
  let answered: Array<Answer> = [];
  let waiting: ((events: Array<Answer>) => void) | null = null;
  child.stdout.setEncoding("utf8");
  child.stdout.on("data", (chunk) => {
    buffer += String(chunk);
    let end = buffer.indexOf("\n");
    while (end !== -1) {
      const event: Answer = JSON.parse(buffer.slice(0, end));
      buffer = buffer.slice(end + 1);
      answered.push(event);
      if ((event.event === "file" || event.event === "invalidated") && waiting != null) {
        const done = waiting;
        waiting = null;
        const events = answered;
        answered = [];
        done(events);
      }
      end = buffer.indexOf("\n");
    }
  });
  return {
    ask: (request) =>
      new Promise((resolve) => {
        waiting = resolve;
        child.stdin.write(`${JSON.stringify(request)}\n`);
      }),
    stop: () => {
      child.stdin.end();
    },
  };
}

/** A project of four modules: a test, the module it reaches through, and two more. */
function project(): { root: string, test: string, write: (name: string, source: string) => void } {
  const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-kept-worker-")));
  const write = (name: string, source: string) => fs.writeFileSync(path.join(root, name), source);
  const entry = pathToFileURL(path.join(repository, "packages", "test", "index.js")).href;
  write("dep.js", 'export const value = "before";\n');
  write("mid.js", 'export { value } from "./dep.js";\n');
  // Evaluated once per instance, so a second evaluation is a new number.
  write("other.js", "export const instance = Math.random();\n");
  write(
    "probe.test.js",
    `import { it } from ${String(JSON.stringify(entry))};\n` +
      'import { value } from "./mid.js";\n' +
      'import { instance } from "./other.js";\n' +
      'it("reads", () => { console.log(value + " " + String(instance)); });\n',
  );
  return { root, test: path.join(root, "probe.test.js"), write };
}

/** What the probe printed, split into the value it read and the instance it saw. */
function printed(events: Array<Answer>): { value: string, instance: string } {
  const line = events.find((event) => event.event === "output")?.text?.trim() ?? "";
  const [value = "", instance = ""] = line.split(" ");
  return { value, instance };
}

const BUDGET = { timeout: 60_000 };

describe("a kept worker told that a file changed", () => {
  it(
    "runs the next file against the edit, through every module in between",
    async () => {
      const { root, test, write } = project();
      const worker = startWorker(true);
      try {
        const first = await worker.ask({ file: test, timeoutMs: 20_000, generation: 1 });
        expect(printed(first).value).toBe("before");

        write("dep.js", 'export const value = "after";\n');
        const told = await worker.ask({
          invalidate: [path.join(root, "dep.js")],
          generation: 2,
        });
        expect(told.map((event) => event.ok)).toEqual([hasInThreadHooks()]);
        if (!hasInThreadHooks()) return;

        const second = await worker.ask({ file: test, timeoutMs: 20_000, generation: 3 });
        // Through `mid.js`, which re-exports it: the edit reaches the test
        // however many modules are in between.
        expect(printed(second).value).toBe("after");
        // `other.js` is evaluated afresh too, but not because of the edit:
        // every test file gets its own copy of the project's modules
        // (ubugeeei-prod/uf#1443). What a kept worker keeps between runs is
        // what the file scope shares — installed packages and the runner.
        expect(printed(second).instance).not.toBe(printed(first).instance);
        expect(second.find((event) => event.event === "file")?.status).toBe("completed");
      } finally {
        worker.stop();
        fs.rmSync(root, { recursive: true, force: true });
      }
    },
    BUDGET,
  );

  it(
    "says it cannot promise that when it was not started to be kept",
    async () => {
      // A one-shot run's worker records no graph, so it has nothing to walk; it
      // says so rather than running the next file against the old code.
      const { root, test, write } = project();
      const worker = startWorker(false);
      try {
        await worker.ask({ file: test, timeoutMs: 20_000, generation: 1 });
        write("dep.js", 'export const value = "after";\n');
        const told = await worker.ask({
          invalidate: [path.join(root, "dep.js")],
          generation: 2,
        });
        expect(told.map((event) => event.ok)).toEqual([false]);
      } finally {
        worker.stop();
        fs.rmSync(root, { recursive: true, force: true });
      }
    },
    BUDGET,
  );
});
