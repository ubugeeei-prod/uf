// @flow
//
// The process `uf test` fans work out to.
//
// One worker per core, each running whole files one at a time: `uf` writes a
// request per line on stdin, the worker imports that file (through the host's
// Flow loader, so the module is transformed by the same `uf transform` the
// build uses), runs what it registered, and writes one event per line back.
//
//   → {"file": "src/math.test.js", "filter": "adds", "timeoutMs": 5000, "generation": 7}
//   ← {"event": "test", "name": "math > adds", "status": "passed", "generation": 7, …}
//   ← {"event": "output", "stream": "stdout", "test": "math > adds", "text": "hi\n", "generation": 7}
//   ← {"event": "file", "status": "completed", "durationMicros": 1234, "generation": 7}
//
// Four decisions worth stating. Results are streamed as they happen rather
// than batched at the end, so `uf test` can draw progress and `--bail` can stop
// a long run early. A file that throws while being *imported* is a file result,
// not a test result: there were no tests to fail, and saying "0 tests" for a
// module that could not load would be a lie. The protocol does not share its
// stream with the tests: a test's own printing becomes an `output` event
// (`internal/output.js`), so a `console.log` cannot land in the middle of a
// line `uf` is parsing. And every event says which request it belongs to —
// see "Which file an event belongs to" below.
//
// This module runs on import by design — it is a process entry point, the way
// `@uniflowed/vite`'s loaders are.
//
// # Which file an event belongs to
//
// One worker's events are one stream, and a file's code outlives the file: a
// `setTimeout` nobody awaited fires while the *next* file is running, and what
// it prints used to be reported under a test in a different file. The same held
// for anything else the abandoned work reached — including the unhandled
// rejection handler at the bottom of this module, which ends a file.
//
// So an event says which request it came from rather than leaving `uf` to
// assume it came from the one in progress. `uf` numbers the requests it sends;
// this module runs each file inside an `AsyncLocalStorage` holding that number,
// and stamps every event with what the storage says *at the moment of writing*.
// Work a file leaves behind inherits its store however late it runs, so a
// straggler carries the generation of the file that scheduled it, and `uf`
// drops it instead of handing it to whatever is running now. See
// ubugeeei-prod/uf#203 and `crates/uf_test/src/host.rs`.
//
// A module-level "the file we are serving now" variable was the obvious answer
// and it is exactly the bug: at the moment the straggler writes, the file being
// served *is* the next one. The number has to come from where the work was
// started, which is what asynchronous storage is.

import * as output from "./internal/output.js";
import { AsyncLocalStorage } from "node:async_hooks";
import { writeChangedSnapshots } from "./internal/snapshot.js";
import { createInterface } from "node:readline";
import { fileURLToPath, pathToFileURL } from "node:url";

import { installInSourceTests } from "./in-source.js";
import { restoreSharedState } from "./internal/isolation.js";
import { run } from "./internal/run.js";

/** What `uf` sends for one file. */
type Request = {|
  readonly file: string,
  readonly filter?: string | null,
  readonly timeoutMs?: number,
  /**
   * Which request this is, counting from one within this worker.
   *
   * Optional only for a `uf` older than the field; `serve` falls back to its
   * own count of the requests it has served, which is the same number.
   */
  readonly generation?: number,
|};

/**
 * The request whose work the code running right now descends from.
 *
 * Read by `write`, so a callback a finished file left behind stamps its events
 * with that file's number rather than with the number of the file the worker
 * has moved on to.
 *
 * `node:async_hooks` is not guarded for: Node, Deno and Bun all provide it
 * under the `node:` specifier, and a host with no `node:` builtins could not
 * link this module at all — `node:readline` and `node:url` are imported above.
 * A host whose storage does not reach a particular callback is a different
 * matter and is handled by the fallback in `write`.
 */
const serving: AsyncLocalStorage<number> = new AsyncLocalStorage();

/**
 * The protocol's own stdout, and the capture that gave it up.
 *
 * A test can reach `console.log` and `process.stdout.write`, and both used to
 * land in the middle of a line `uf` was parsing. `internal/output.js` takes
 * those over and hands back the real write it replaced, so the protocol has a
 * stream nothing else can reach and a test's printing is still reported —
 * as an `output` event, escaped, whatever it says.
 *
 * At module scope, before a single test file can be imported, so output
 * written while a file is still loading is reported rather than lost.
 */
const emit: (chunk: string) => void = output.install((chunk) => {
  write({
    event: "output",
    stream: chunk.stream,
    test: chunk.test,
    text: chunk.text,
  });
});

/**
 * Write one event, stamped with the request it belongs to.
 *
 * Stamped here rather than at each of the five places that build an event, so
 * there is no way to write one without a generation — the module-level
 * unhandled rejection handler included, which is the one that most needed it.
 *
 * `0` is what an event carries when the storage has nothing to say: a write
 * from outside any request (a malformed request line, which `uf` answers to
 * immediately), or a host that does not carry a store into the callback that
 * wrote it — Deno 1.31 does not carry one through `setTimeout`, and Bun does
 * not carry one into `unhandledRejection`. `uf` reads `0` as "the file being
 * served", which is what every event meant before this field existed: of the
 * two ways to be less than exact, saying nothing about a straggler is the
 * behaviour that was already there, and refusing an unstamped `file` event
 * would hang a file that had in fact answered.
 */
function write(event: { readonly [string]: mixed }): void {
  emit(`${JSON.stringify({ ...event, generation: serving.getStore() ?? 0 })}\n`);
}

/**
 * Import and run one file.
 *
 * `generation` is the request's number, and it does two jobs with one value:
 * it busts the module cache so a watch-mode rerun in the same worker sees the
 * edited file rather than the one the registry already holds, and — through
 * the `serving` store this runs inside — it is what every event written from
 * this file, or from anything this file leaves behind, is stamped with.
 */
async function runFile(request: Request, generation: number): Promise<void> {
  const started = performance.now();
  // Everything the previous file changed and this package shares with it goes
  // back: the registry, the stubbed environment and globals, the clock, the
  // module stand-ins, and the document. What each of those is and why it is on
  // the list is `internal/isolation.js`, which is one place rather than five
  // calls here — a worker serves many files out of one process, `uf test` fans
  // files across workers by size, and a leak therefore makes the *result* of a
  // suite a function of the timings file rather than of the code under test.
  // See ubugeeei-prod/uf#417, #581 and #607, which are that sentence three
  // times over.
  //
  // Before the import rather than after the run, so a file that throws while
  // loading still hands the next one a clean process.
  restoreSharedState();
  // Not a restore, and so not on that list: this is the output budget for the
  // file about to run, rather than something the previous file left behind.
  output.startFile();

  // In-source blocks in *this* file get uf's test API; the same blocks in
  // every module this file imports get `undefined` and do not register. See
  // `./in-source.js` for why the marker is a call rather than a constant.
  const url = pathToFileURL(request.file).href;
  const uninstallInSourceTests = installInSourceTests(url);
  try {
    await runImportedFile(request, generation, url, started);
  } finally {
    // For the whole file rather than only for its import. A block reads the
    // marker at module scope, but a *case body* may read it too — `const
    // { uft } = import.meta.uf.test` inside an `it` is an ordinary thing to
    // write — and a marker that answered during the import and not during the
    // run would be a value that changed under the file that read it.
    //
    // Taking it away earlier would not buy the isolation it looks like it
    // buys: work a finished file left behind can register through a binding it
    // captured just as easily as through this global, so the thing that keeps
    // a straggler out of the next file is the registry reset at the top of
    // this function and the generation stamped on every event, not the
    // lifetime of one accessor.
    uninstallInSourceTests();
  }
}

/**
 * The half of [`runFile`] that has a file to run.
 *
 * Split out so the caller's `finally` covers the import *and* the run without
 * either of the two `return`s below escaping it.
 */
async function runImportedFile(
  request: Request,
  generation: number,
  url: string,
  started: number,
): Promise<void> {
  try {
    await import(`${url}?uf-run=${generation}`);
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
    const absolute = fileURLToPath(pathToFileURL(request.file).href);
    await run(
      { filter: request.filter ?? null, timeoutMs: request.timeoutMs, file: absolute },
      (result) => {
        write({
          event: "test",
          ...result.outcome,
          name: result.name,
          line: result.line,
          column: result.column,
          durationMicros: result.durationMicros,
        });
      },
    );
    // Once, at the end of the file, rather than after each snapshot: a file
    // with forty snapshots would otherwise rewrite its snapshot file forty
    // times, and a crash halfway through would leave a partial one.
    writeChangedSnapshots();
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
 *
 * "In order" bounds what the worker *starts*, not what a file leaves running,
 * which is why each file runs inside `serving`. The store is entered here and
 * not in `runFile` so that the whole of a file's work, its module import
 * included, is inside it.
 */
function serve(): void {
  let queue: Promise<void> = Promise.resolve();
  let served = 0;

  createInterface({ input: process.stdin }).on("line", (line) => {
    if (line.trim() === "") {
      return;
    }
    let request: Request;
    try {
      request = JSON.parse(line);
    } catch (error) {
      // Outside any `serving.run`, so this is stamped `0` — which is right:
      // there is no request to attribute it to, and `uf` is waiting for an
      // answer to the line it just wrote.
      write({
        event: "file",
        status: "run-failed",
        message: `malformed request: ${String(error)}`,
      });
      return;
    }
    served += 1;
    // `uf` chooses the number, because `uf` is the side that checks it. This
    // count of served requests is the same sequence and stands in for a `uf`
    // too old to send one — without something monotonic here the import below
    // would be cache-busted with `undefined` and a watch-mode rerun would see
    // the module it already had.
    const at = request.generation ?? served;
    queue = queue.then(() => serving.run(at, () => runFile(request, at)));
  });

  process.stdin.on("close", () => {
    queue.then(() => process.exit(0));
  });
}

// Unhandled rejections would otherwise take the worker down mid-file with no
// explanation; reporting one as a file failure keeps the run honest.
//
// The file it fails is whichever one the rejected promise was created in, not
// whichever one is running when Node gets round to reporting it: `write` reads
// the store, and on Node the store follows the promise. That matters because
// this is a `file` event and a `file` event *ends* a file — a promise the
// previous file abandoned used to end the next one, with a message from code
// that file does not contain.
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
