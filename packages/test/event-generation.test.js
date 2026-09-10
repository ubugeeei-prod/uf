// @flow
//
// Which *file* an event belongs to.
//
// A worker runs whole files one after another and its events are one stream.
// "One file at a time" bounds what the worker starts, not what a finished file
// left running: a `setTimeout` nobody awaited fires while the next file is
// running, and everything it does — printing, and in the case of an abandoned
// promise, ending a file — arrived with nothing on it to say where it came
// from. `uf` read it as the running file's. That is ubugeeei-prod/uf#203.
//
// The fix is a number: `uf` numbers the requests it sends, the worker runs each
// file inside an `AsyncLocalStorage` holding that number, and every event
// carries the number the storage held *at the moment it was written*. Work a
// file leaves behind inherits its file's store however late it runs, so a
// straggler carries the generation of the file that scheduled it and `uf` can
// tell it apart from the file it interrupted (`crates/uf_test/src/host.rs`).
//
// The stamp is a property of the wire, and only of the wire: by the time a case
// could look at one, that case is the thing being described. So this drives a
// real worker exactly the way `host.rs` drives one — a request per line on its
// stdin, one JSON event per line back — the way `lsp.test.js` talks to a real
// `uf lsp` over its real wire, and reads the events it wrote.
//
// The reproductions are written to a temporary directory rather than kept
// beside this file, because a file in this workspace that registers cases *is*
// a test of this repository: `uf test` discovers by reading a file, not by
// naming it, so a fixture here would be collected and run by the very suite
// that is supposed to be running it. Nothing outside the project has a
// `node_modules` to resolve `@uniflowed/test` from, so a fixture reaches the
// package by path.

import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { afterAll, beforeAll, describe, expect, it } from "@uniflowed/test";

const here = path.dirname(fileURLToPath(import.meta.url));
const repository = path.resolve(here, "..", "..");

/**
 * The switch that makes the race deterministic.
 *
 * The issue writes the reproduction as `setTimeout(…, 50)` in one file and
 * lets the next file happen to be running fifty milliseconds later, which does
 * reproduce the bug and makes a poor test: what has to be shown is not only
 * that the line carries the first file's number but that it *arrived after that
 * file had ended*, and a sleep proves that only while the machine is idle.
 *
 * So the second file hands the first one a switch. The callback is still a
 * detached one — registered in the first file, run from a timer, printing long
 * after the case that wrote it returned — but it cannot now run until the
 * second file has started, and the second file cannot finish until it has run.
 *
 * Both files import this module *without* the cache-busting query the worker
 * puts on a test file, so both get the one instance the worker's registry
 * already holds. That sharing is the whole mechanism.
 */
const SWITCH = `
let release;
export const begun = new Promise((resolve) => {
  release = resolve;
});
export function begin() {
  release();
}

let settle;
export const printed = new Promise((resolve) => {
  settle = resolve;
});
export function donePrinting() {
  settle();
}
`;

/** The file that leaves a callback behind, and ends long before it fires. */
const FIRST = `
import { begun, donePrinting } from "./switch.js";

describe("the first file", () => {
  it("schedules something it does not wait for", () => {
    console.log("from the first file, while it was still running");
    begun.then(() => {
      setTimeout(() => {
        console.log("from the previous file");
        donePrinting();
      }, 0);
    });
  });
});
`;

/** The file that is running when the callback finally fires. */
const SECOND = `
import { begin, printed } from "./switch.js";

describe("the second file", () => {
  it("is running when the previous file's callback fires", async () => {
    begin();
    await printed;
  });
});
`;

/** One request, in the shape `host.rs` writes it. */
type Request = {|
  readonly file: string,
  readonly timeoutMs: number,
  readonly generation?: number,
|};

/** One line the worker wrote back, in the shape `host.rs` reads. */
type Event = {
  event: string,
  generation?: number,
  stream?: string,
  test?: string | null,
  text?: string,
  name?: string,
  status?: string,
};

/**
 * How this host starts a worker, mirroring `HostCommand::with_flow_loader`.
 *
 * The worker imports Flow — `@uniflowed/test` is Flow source — so it needs the
 * host's loader, and each host registers one its own way. Deno has none in
 * `@uniflowed/host` yet, so it cannot run this at all; a named failure is
 * better than a skip that reads like a pass.
 */
function loaderArguments(): Array<string> {
  const host = path.basename(process.execPath);
  if (host.startsWith("node")) {
    const register = path.join(repository, "packages", "host", "register.js");
    return ["--enable-source-maps", "--import", pathToFileURL(register).href];
  }
  if (host.startsWith("bun")) {
    return ["--preload", path.join(repository, "packages", "host", "bun-preload.js")];
  }
  throw new Error(`no Flow loader for ${host}: this test drives the worker uf would have started`);
}

/**
 * Run `requests` in one worker, in order, and collect every event it wrote.
 *
 * One worker for all of them, because a worker serving a second file after a
 * first is the entire subject: two workers would have nothing to confuse. The
 * requests are written together and the worker queues them, which is what
 * `host.rs` does one at a time — either way the second file is imported only
 * after the first has reported.
 *
 * The environment is inherited because that is where `UF_PROJECT_ROOT` and
 * `UF_BINARY` are: `uf test` sets both when it starts this suite, so the nested
 * worker transforms through the same binary and shares its transform cache
 * instead of building a second one. The worker exits when its stdin closes,
 * which is what ends the run — the same handshake `host.rs` uses.
 */
function runInWorker(requests: Array<Request>): Promise<Array<Event>> {
  return new Promise((resolve, reject) => {
    const worker = path.join(repository, "packages", "test", "worker.js");
    const child = spawn(process.execPath, [...loaderArguments(), worker], {
      // Its stderr is the host's own noise and is not part of the report;
      // inherited so a person debugging this sees it, as `host.rs` does.
      stdio: ["pipe", "pipe", "inherit"],
    });
    let written = "";
    child.stdout.setEncoding("utf8");
    child.stdout.on("data", (chunk) => {
      written += String(chunk);
    });
    child.on("error", reject);
    child.on("close", () => {
      try {
        resolve(
          written
            .split("\n")
            .filter((line) => line !== "")
            .map((line) => JSON.parse(line)),
        );
      } catch (error) {
        reject(new Error(`the worker wrote something that is not an event: ${String(error)}`));
      }
    });
    child.stdin.end(requests.map((request) => `${JSON.stringify(request)}\n`).join(""));
  });
}

/**
 * The run as one readable list: which request each event says it came from,
 * and what it was.
 *
 * Kept as strings because the order is half of what is being asserted — a
 * straggler that arrived *before* its file's `file` event would prove nothing —
 * and a failing `toEqual` over strings shows exactly which line moved.
 */
function transcript(events: Array<Event>): Array<string> {
  return events.map((event) => {
    const from = `[${String(event.generation)}]`;
    if (event.event === "output") {
      return `${from} output: ${String(event.text).trimEnd()}`;
    }
    if (event.event === "test") {
      return `${from} test ${String(event.name)} ${String(event.status)}`;
    }
    return `${from} file ${String(event.status)}`;
  });
}

let directory = "";

/**
 * Write the two files and the switch they share into `directory`.
 *
 * `switch.js` is not a test file and is never handed to the worker as a
 * request; it is reached only through the two that are.
 */
function fixtures(): Array<string> {
  const entry = pathToFileURL(path.join(repository, "packages", "test", "index.js")).href;
  fs.writeFileSync(path.join(directory, "switch.js"), SWITCH);
  const written = [];
  for (const [name, source] of [
    ["first.js", FIRST],
    ["second.js", SECOND],
  ]) {
    const file = path.join(directory, name);
    fs.writeFileSync(file, `import { describe, it } from "${entry}";\n${source}`);
    written.push(file);
  }
  return written;
}

let events: Array<Event> = [];
let unnumbered: Array<Event> = [];

beforeAll(async () => {
  directory = fs.mkdtempSync(path.join(os.tmpdir(), "uf-event-generation-"));
  const [first, second] = fixtures();
  events = await runInWorker([
    { file: first, timeoutMs: 5000, generation: 1 },
    { file: second, timeoutMs: 5000, generation: 2 },
  ]);
  // The same first file again, from a `uf` that does not know about the field.
  unnumbered = await runInWorker([{ file: first, timeoutMs: 5000 }]);
});

afterAll(() => {
  fs.rmSync(directory, { recursive: true, force: true });
});

describe("an event written after its file has finished", () => {
  it("says it came from that file, not from the one that is running", () => {
    // The fourth line is the whole issue. It sits *after* the first file's
    // `file` event, so it was written when that file was over and the second
    // one had started, and it still says `1`. Before the fix there was no
    // number on it at all, and `uf` had nothing to read but "whatever is
    // running now" — which is the second file.
    expect(transcript(events)).toEqual([
      "[1] output: from the first file, while it was still running",
      "[1] test the first file > schedules something it does not wait for passed",
      "[1] file completed",
      "[1] output: from the previous file",
      "[2] test the second file > is running when the previous file's callback fires passed",
      "[2] file completed",
    ]);
  });

  it("does not stop the file it arrived in from being reported as its own", () => {
    // The other half: numbering the straggler must not cost the file that was
    // interrupted anything. Every event of the second file still says `2`.
    const second = events.filter((event) => event.event !== "output" && event.generation === 2);
    expect(second.map((event) => String(event.status))).toEqual(["passed", "completed"]);
  });
});

describe("a request that carries no generation", () => {
  it("is still numbered, so the worker's import cache is still busted", () => {
    // The fallback for a `uf` older than the field, and it is not decoration:
    // the number is what the worker appends to the import specifier, so
    // without something monotonic here a watch-mode rerun in the same worker
    // would ask for `?uf-run=undefined` twice and be handed the module it
    // already had. The worker's own count of served requests is that number.
    expect(unnumbered.length).toBeGreaterThan(0);
    for (const event of unnumbered) {
      expect(event.generation).toBe(1);
    }
  });
});
