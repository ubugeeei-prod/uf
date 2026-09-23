// @flow
//
// Each test file a worker runs sees the project's modules as if it were the
// first file that worker ran. ubugeeei-prod/uf#1443.
//
// A worker serves many files from one process, and a process keeps every module
// it evaluated. Before this, a file that left state in a module — a React
// context's current value, a cache, a counter — handed it to whichever file
// the schedule put next in that worker, and the file that failed was the one
// that read it. `packages/web/web.test.js` printed the machine's date after
// `tests/library/payload.test.js` had hydrated a page in the same worker.
//
// Driven through the worker's own protocol, because the promise is about two
// files in one process and that is the only place it can be tested. The
// generic case writes its fixtures to a temporary directory: a file in this
// repository that registers cases would be collected as a test.

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

/** Whether this host's loader resolves in the importing thread, which the scope needs. */
function hasInThreadHooks(): boolean {
  return !path.basename(process.execPath).startsWith("bun");
}

type Event = {
  event: string,
  generation?: number,
  name?: string,
  status?: string,
  message?: string,
  text?: string,
};

/** One request, in the shape `crates/uf_test/src/host.rs` writes it. */
type Request = {|
  readonly file: string,
  readonly filter?: string,
  readonly timeoutMs: number,
  readonly generation: number,
|};

/** Run `requests` in one worker, in order, and collect every event it wrote. */
function runInOneWorker(requests: $ReadOnlyArray<Request>): Promise<Array<Event>> {
  return new Promise((resolve, reject) => {
    const worker = path.join(repository, "packages", "test", "worker.js");
    const child = spawn(process.execPath, [...loaderArguments(), worker], {
      cwd: repository,
      env: { ...process.env, UF_PROJECT_ROOT: repository },
      stdio: ["pipe", "pipe", "inherit"],
    });
    let written = "";
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      written += String(chunk);
    });
    child.on("error", reject);
    child.on("close", () => {
      resolve(
        written
          .split("\n")
          .filter((line) => line !== "")
          .map((line) => JSON.parse(line)),
      );
    });
    child.stdin.end(requests.map((request) => `${JSON.stringify(request)}\n`).join(""));
  });
}

/** What each case said, as `generation name status`. */
function outcomes(events: Array<Event>): Array<string> {
  return events
    .filter((event) => event.event === "test")
    .map(
      (event) =>
        `${String(event.generation)} ${String(event.name)} ${String(event.status)}` +
        (event.status === "failed" ? `: ${String(event.message)}` : ""),
    );
}

const BUDGET = { timeout: 120_000 };

describe("the next file in the same worker", () => {
  it(
    "sees a module the previous file changed as it was before either ran",
    async () => {
      const directory = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-file-scope-")));
      try {
        const entry = pathToFileURL(path.join(repository, "packages", "test", "index.js")).href;
        const write = (name: string, source: string) => {
          fs.writeFileSync(path.join(directory, name), source);
          return path.join(directory, name);
        };
        write(
          "state.js",
          'let value = "as the module was written";\n' +
            "export function set(next) { value = next; }\n" +
            "export function read() { return value; }\n",
        );
        const first = write(
          "first.test.js",
          `import { expect, it } from ${String(JSON.stringify(entry))};\n` +
            'import { read, set } from "./state.js";\n' +
            'it("changes it", () => { set("left behind"); expect(read()).toBe("left behind"); });\n',
        );
        const second = write(
          "second.test.js",
          `import { expect, it } from ${String(JSON.stringify(entry))};\n` +
            'import { read } from "./state.js";\n' +
            'it("reads it", () => { expect(read()).toBe("as the module was written"); });\n',
        );

        const events = await runInOneWorker([
          { file: first, timeoutMs: 30_000, generation: 1 },
          { file: second, timeoutMs: 30_000, generation: 2 },
        ]);

        // Bun resolves through a plugin the scope does not reach yet; the
        // guide's host table says so, and the promise is held on the hosts
        // that keep it.
        if (!hasInThreadHooks()) return;
        expect(outcomes(events)).toEqual(["1 changes it passed", "2 reads it passed"]);
      } finally {
        fs.rmSync(directory, { recursive: true, force: true });
      }
    },
    BUDGET,
  );

  it(
    "renders a time in UTC after a file that hydrated a page in another zone",
    async () => {
      // The reproduction from the issue, in the order that failed: the payload
      // file hydrates a page whose render envelope names the machine's zone,
      // and the next file's `<Time>`, which promises UTC when nothing fixed a
      // zone, used to print the machine's date instead.
      if (!hasInThreadHooks()) return;
      const events = await runInOneWorker([
        {
          file: path.join(repository, "tests", "library", "payload.test.js"),
          filter: "hydrates from the rows",
          timeoutMs: 30_000,
          generation: 1,
        },
        {
          file: path.join(repository, "packages", "web", "web.test.js"),
          filter: "falls back to UTC",
          timeoutMs: 30_000,
          generation: 2,
        },
      ]);

      const ran = outcomes(events).filter((line) => !line.endsWith(" skipped"));
      expect(ran).toEqual([
        "1 the browser applying a payload > hydrates from the rows the document carried, without running the loader again passed",
        "2 Time > falls back to UTC rather than to the machine it is running on passed",
      ]);
    },
    BUDGET,
  );
});
