// @flow
//
// Internal to `@uniflowed/test`: the worker, as it exists inside a page.
//
// `../../worker.js` and this module are the same program written for two
// different hosts, and the half that matters is the same code in both:
// `internal/run.js` walks the tree, `internal/registry.js` holds it,
// `internal/expect.js` decides what passed. What differs is only how a file
// arrives and how a result leaves.
//
//   worker.js                      page.js
//   ─────────────────────────────  ──────────────────────────────────
//   a request per line on stdin    a long-polled GET /uf-test/next
//   an event per line on stdout    a POST /uf-test/events
//   one process, many files        one page load, one file
//   `restoreSharedState()`         a reload
//
// The last row is the one worth reading twice. A Node worker serves many files
// out of one process, so everything two files could share has to be put back
// between them by hand — the registry, the clock, the module stand-ins, the
// document — and `internal/isolation.js` is that list, three issues long. A
// page reloads instead. The realm is new, the document is new, the module
// registry is new, and a `setInterval` the previous file abandoned is not
// merely unstamped, it is gone. Browser mode gets stronger isolation than
// Node mode for free, and it is the reload that buys it.
//
// # What this file does not do
//
// Snapshots. `toMatchSnapshot` writes a file, a page has no filesystem, and
// `./node.js` refuses it in a sentence rather than answering "no snapshot yet"
// and passing. `toMatchInlineSnapshot` needs no file and works here.
//
// Module mocking. `uft.mock` intercepts resolution, which on Node is
// `node:module`'s synchronous hooks and in a page is nothing at all. Same
// refusal, same reason.

import * as output from "../output.js";
import { run } from "../run.js";
import { installInSourceTests } from "../../in-source.js";

/** One file, as the server hands it over. */
type PageRequest = {
  readonly file?: string,
  readonly filter?: string | null,
  readonly timeoutMs?: number,
  readonly generation?: number,
  readonly done?: boolean,
  ...
};

/** The generation every event written from this page load carries. */
let serving = 0;

/**
 * Events waiting to be posted, and the post that will carry them.
 *
 * Batched rather than one request per event, and still streamed rather than
 * held to the end: a `POST` is in flight or it is not, and everything that
 * accumulates while one is in flight goes in the next. So `uf test` draws
 * progress and `--bail` stops a long file early, without a request per
 * assertion — a file with four hundred cases would otherwise spend more time
 * in the loopback than in the tests.
 */
const queue: Array<{ readonly [string]: mixed }> = [];
let flushing: Promise<void> | null = null;

function write(event: { readonly [string]: mixed }): void {
  queue.push({ ...event, generation: serving });
  flush();
}

function flush(): Promise<void> {
  const already = flushing;
  if (already != null) return already;
  if (queue.length === 0) return Promise.resolve();
  const batch = queue.splice(0, queue.length);
  const sent = fetch("/uf-test/events", {
    method: "POST",
    headers: { "content-type": "application/json" },
    body: JSON.stringify(batch),
  })
    .catch(() => {
      // The run is over, or `uf` killed the driver on a deadline. Either way
      // there is nobody to tell, and throwing here would replace a report with
      // an unhandled rejection.
    })
    .then(() => {
      flushing = null;
      if (queue.length > 0) return flush();
      return undefined;
    });
  flushing = sent;
  return sent;
}

/**
 * Take over `console`, so a test's printing is an event rather than a line in
 * a console nobody is reading.
 *
 * The same `internal/output.js` the Node worker installs. It has no
 * `process.stdout` to take here and does not need one — the protocol's channel
 * is a separate HTTP request, not a shared stream — which is the one thing
 * that module had to learn about a page.
 */
output.install((chunk) => {
  write({ event: "output", stream: chunk.stream, test: chunk.test, text: chunk.text });
});

/** Run one file and report it, whatever happens to it. */
async function runFile(request: PageRequest): Promise<void> {
  const file = request.file;
  if (typeof file !== "string") {
    write({ event: "file", status: "run-failed", message: "a request with no file" });
    return;
  }
  const started = performance.now();
  const url = new URL(`/@fs${file.split("/").map(encodeURIComponent).join("/")}`, location.href)
    .href;
  output.startFile();
  const uninstall = installInSourceTests(url);
  try {
    try {
      await import(url);
    } catch (thrown) {
      const error = asError(thrown);
      write({
        event: "file",
        status: "load-failed",
        message: `${error.name}: ${error.message}`,
        stack: error.stack ?? null,
        durationMicros: micros(started),
      });
      return;
    }
    await run(
      {
        filter: request.filter ?? null,
        timeoutMs: request.timeoutMs,
        file,
      },
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
    write({ event: "file", status: "completed", durationMicros: micros(started) });
  } catch (thrown) {
    const error = asError(thrown);
    write({
      event: "file",
      status: "run-failed",
      message: `${error.name}: ${error.message}`,
      stack: error.stack ?? null,
      durationMicros: micros(started),
    });
  } finally {
    uninstall();
  }
}

function asError(thrown: mixed): Error {
  return thrown instanceof Error ? thrown : new Error(String(thrown));
}

function micros(started: number): number {
  return Math.round((performance.now() - started) * 1000);
}

/**
 * The page's global object, as somewhere to hang a listener.
 *
 * `globalThis` is a namespace to the checker rather than an object, so
 * `globalThis.addEventListener` is not an expression that can be written. The
 * same one-line trust boundary `internal/output.js` draws around `console`,
 * for the same reason and with the same amount of type in it.
 */
function page(): $FlowFixMe {
  return globalThis;
}

/**
 * Ask for a file, run it, and hand the page back for the next one.
 *
 * The reload is the isolation, so it is unconditional: a page that ran a file
 * does not run a second one, whatever the first one left behind. `location`
 * rather than a fresh `import` with a busted cache, because the cache is the
 * smallest of the things two files would otherwise share.
 */
async function serve(): Promise<void> {
  let request: PageRequest;
  try {
    request = await (await fetch("/uf-test/next")).json();
  } catch {
    // The driver has gone. Nothing to report it to.
    return;
  }
  if (request.done === true) return;
  serving = typeof request.generation === "number" ? request.generation : 0;
  await runFile(request);
  await flush();
  location.reload();
}

/**
 * An unhandled rejection ends the file, exactly as it does on a Node worker.
 *
 * A promise nobody awaited is the one failure a runner cannot see from inside
 * `run.js`: the case has already been reported as passed by the time the
 * rejection surfaces. Reporting it as a file failure is what keeps the run
 * honest, and it is the same decision `worker.js` makes at the bottom of the
 * file for the same reason.
 *
 * Then the page is *not* reloaded, so the run's own deadline is what ends this
 * file. `uf` is holding a wall clock the page cannot argue with; a page that
 * reloaded itself here would race that clock and could report the next file's
 * events against this one's generation.
 */
page().addEventListener("unhandledrejection", (event: mixed) => {
  const reason = event != null && typeof event === "object" ? event.reason : undefined;
  const error = asError(reason);
  write({
    event: "file",
    status: "run-failed",
    message: `unhandled rejection: ${error.message}`,
    stack: error.stack ?? null,
  });
  void flush();
});

/**
 * A window error ends the file too.
 *
 * A page has one more way to fail than a process does: a script that throws
 * during evaluation of something the test did not `await` — an image handler,
 * a `requestAnimationFrame` callback, a listener a component attached — is
 * reported to `window` and to nothing else. On Node the equivalent takes the
 * process down and `uf` reports a worker that died; here nothing at all would
 * happen, and the file would sit until its deadline.
 */
page().addEventListener("error", (event: mixed) => {
  const thrown = event != null && typeof event === "object" ? event.error : undefined;
  const error = asError(thrown ?? "an error with no value");
  write({
    event: "file",
    status: "run-failed",
    message: `${error.name}: ${error.message}`,
    stack: error.stack ?? null,
  });
  void flush();
});

void serve();
