// @flow
//
// Which case a printed line is filed under.
//
// `uf test` reports what a test printed under the test that printed it, and
// the hard half of that promise is a callback a test leaves behind. One
// scheduled by a case fires after that case has been reported, while the next
// one is running, and the runner used to name the case it had got to rather
// than the case the callback came from. The line was then reported under a
// test whose source does not contain it, which is the one wrong answer a
// reader cannot recover from — worse than not naming it at all. That is
// ubugeeei-prod/uf#207; ownership now lives in the asynchronous context the
// case ran in (`packages/test/internal/output.js`).
//
// Attribution is not observable from inside a test. The name a chunk carries
// belongs to the protocol between the worker and `uf`, and by the time a case
// could look at it, that case is the thing being named. So this drives a real
// worker exactly the way `crates/uf_test/src/host.rs` drives one — a request
// per line on its stdin, one JSON event per line back — and reads the `output`
// events for the `test` field they carry. `lsp.test.js` talks to `uf lsp` over
// its real wire for the same reason: a promise made on a wire has to be
// checked on that wire.
//
// The reproduction is written to a temporary directory rather than kept beside
// this file, because a file in this workspace that registers cases *is* a test
// of this repository. `uf test` discovers by reading a file, not by naming it,
// so a fixture here would be collected and run by the very suite that is
// supposed to be running it. Nothing outside the project has a `node_modules`
// to resolve `@uniflowed/test` from, so the fixture reaches the package by
// path — the same modules the worker loads, so the two halves share one
// registry rather than each collecting into its own.

import { spawn } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { afterAll, beforeAll, describe, expect, it } from "@uniflowed/test";

const here = path.dirname(fileURLToPath(import.meta.url));
const repository = path.resolve(here, "..", "..");

/**
 * The reproduction from the issue, made deterministic.
 *
 * The issue writes it as a `setTimeout(…, 30)` in one case and an
 * `await sleep(60)` in the next, and that does reproduce the bug. It makes a
 * poor test, though: what this file has to show is not only that the line is
 * named correctly but that it *arrived after its own case had been reported*,
 * and with two sleeps racing each other that proof holds only as long as the
 * machine is not busy. So the second case hands the first one a switch. The
 * callback is still a detached one — registered inside the first case, run
 * from a timer, printing long after the case that wrote it returned — but it
 * cannot now run before the next case has started.
 */
const REPRODUCTION = `
// Printed while the module is being imported. No case is running, and no case
// should be blamed for it.
console.log("while the file was still loading");

let begin;
const started = new Promise((resolve) => {
  begin = resolve;
});
let printed;

describe("late output", () => {
  it("prints, and leaves a callback behind", () => {
    console.log("from the test itself");
    // Registered here, so this case's context is what it carries; run from a
    // timer the next case releases, so it prints when this case is over.
    printed = started.then(
      () =>
        new Promise((resolve) => {
          setTimeout(() => {
            console.log("from the test before");
            resolve();
          }, 0);
        }),
    );
  });

  it("is the case that happens to be running when it fires", async () => {
    begin();
    await printed;
  });
});
`;

/** One line the worker wrote back, in the shape `host.rs` reads. */
type Event = {
  event: string,
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
 * Run `file` in a worker of its own and collect every event it wrote.
 *
 * The environment is inherited because that is where `UF_PROJECT_ROOT` and
 * `UF_BINARY` are: `uf test` sets both when it starts this suite, so the
 * nested worker transforms through the same binary and shares its transform
 * cache instead of building a second one. The worker exits when its stdin
 * closes, which is what ends the run — the same handshake `host.rs` uses.
 */
function runInWorker(file: string): Promise<Array<Event>> {
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
    child.stdin.end(`${JSON.stringify({ file, timeoutMs: 5000 })}\n`);
  });
}

/**
 * The run as one readable list: who each line was filed under, in order,
 * with the case reports left in between.
 *
 * Kept as strings rather than objects because the order is half of what is
 * being asserted, and a failing `toEqual` over strings shows which line moved.
 */
function transcript(events: Array<Event>): Array<string> {
  return events.map((event) => {
    if (event.event === "output") {
      return `${event.test ?? "<the file>"}: ${String(event.text).trimEnd()}`;
    }
    if (event.event === "test") {
      return `${String(event.name)} ${String(event.status)}`;
    }
    return `file ${String(event.status)}`;
  });
}

let events: Array<Event> = [];
let directory = "";

beforeAll(async () => {
  directory = fs.mkdtempSync(path.join(os.tmpdir(), "uf-output-owner-"));
  const fixture = path.join(directory, "late-output.js");
  const entry = pathToFileURL(path.join(repository, "packages", "test", "index.js")).href;
  fs.writeFileSync(fixture, `import { describe, it } from "${entry}";\n${REPRODUCTION}`);
  events = await runInWorker(fixture);
});

afterAll(() => {
  fs.rmSync(directory, { recursive: true, force: true });
});

describe("output from a callback a finished test left behind", () => {
  it("is filed under the case that scheduled it, not the case that was running", () => {
    // The fourth line is the whole issue. It sits *after* the first case's
    // report, so it arrived when that case was over and the next one was
    // running, and it is still that case's line. Before the fix it read
    // `late output > is the case that happens to be running when it fires`.
    expect(transcript(events)).toEqual([
      "<the file>: while the file was still loading",
      "late output > prints, and leaves a callback behind: from the test itself",
      "late output > prints, and leaves a callback behind passed",
      "late output > prints, and leaves a callback behind: from the test before",
      "late output > is the case that happens to be running when it fires passed",
      "file completed",
    ]);
  });

  it("leaves a line that belongs to no case under the file", () => {
    // The other half of the contract, and the one the fallback rests on: a
    // chunk with no owner names no test. A host whose storage does not reach a
    // detached callback lands here rather than on the wrong case, which is why
    // "no owner" has to keep meaning the file.
    const loading = events.filter((event) => event.text === "while the file was still loading\n");
    expect(loading).toHaveLength(1);
    expect(loading[0].test).toBe(null);
  });
});
