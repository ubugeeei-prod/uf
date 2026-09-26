// @flow
//
// Internal to `@uniflowed/test`: what a test printed, attributed to the test
// that printed it.
//
// The worker and the tests it runs share one stdout, and the worker's protocol
// is one JSON object per line on it. A `console.log` in a test therefore wrote
// a line into the middle of that protocol, `uf` read it as an event it could
// not parse, and the whole file died — a green suite killed by a debugging
// statement somebody left in.
//
// So the stream stops being shared. This module takes over `console` and the
// two `write` methods a test can reach directly, turns everything written
// through them into an ordinary protocol event, and hands the caller the real
// `process.stdout.write` it took — the protocol keeps the raw stream, and it
// is the only thing that has it. A test that prints something shaped exactly
// like a protocol line is then a string inside a JSON field, which is the
// point: the escaping is what makes the channel robust, not a filter that
// tries to recognise the imposter.
//
// # Why this is its own module
//
// Two callers need it and neither owns it: `worker.js` installs the capture at
// start-up, and `run.js` runs each case inside the ownership kept here, so a
// chunk can be named. "Who printed this" is also state with a lifetime of its
// own — one case's hooks and body — exactly like the snapshot key in
// `snapshot.js`, and for the same reason it lives beside the thing it describes
// rather than inside either caller.
//
// # What "the test that printed it" means
//
// The case whose *asynchronous context* the write happened in, which is the
// case whose code produced it.
//
// The obvious answer was a module-level variable the runner set before a case
// and cleared after it, and it was wrong in one shape that matters: the name a
// chunk carried was whatever the worker happened to be running when the chunk
// arrived. A `setTimeout` a test left behind fires while the *next* case is
// running, so the line it printed was reported under that next case — a test
// accused of printing something it never printed, which is worse than not
// naming it at all, because a reader chasing the message finds it under code
// that does not contain it. See ubugeeei-prod/uf#207.
//
// So the owner is an `AsyncLocalStorage`, and what `run.js` calls is
// `runInTest` rather than an `enterTest` / `exitTest` pair: the store is only
// carried by work started *inside* the case, so the case's setup, body and
// teardown have to run within it. Everything they schedule inherits it,
// whenever it eventually runs.
//
// # When there is no owner
//
// `getStore()` answers nothing outside a case, and a chunk with no owner is
// filed under the file — which is right for an import, a `beforeAll`, or a
// straggler from a case that is long gone.
//
// It is also the answer on a host whose storage does not reach the callback.
// Deno 1.31 has `AsyncLocalStorage` and propagates it across `await`, but not
// through `setTimeout`, so a detached callback there is filed under the file
// rather than under the case that scheduled it. That degradation is the point:
// of the two ways to be less than exact, naming the file says less, and naming
// the next case says something false.
//
// `node:async_hooks` itself is not guarded for, because a guard could not run.
// Node, Deno and Bun all provide it under the `node:` specifier, and a host
// that had no `node:` builtins could not start this worker at all — `node:util`
// is imported below, `node:readline` and `node:url` by `worker.js`. A `typeof`
// check around the constructor would only ever execute on a host where this
// module had already linked.
//
// The one thing this does not answer is a straggler that outlives its *file*:
// the worker runs the next file in the same process, and a chunk still carrying
// a name from the file before is a name the next file's report has no test for.
// The host files it under the file it arrived in, which is honest but not
// exact, and closing it properly needs the file's generation in the protocol.
// That is ubugeeei-prod/uf#203, and it is not this module's to fix.

// # Bounds
//
// A test that prints in a loop must not be able to fill the pipe, the worker's
// memory or the report, so one file's captured output is bounded and the
// bound is announced rather than hidden: the chunk that reaches it says so and
// nothing after it is kept. The budget starts over for each file, so a chatty
// file does not silence the next one in the same worker.

import { AsyncLocalStorage } from "node:async_hooks";
import { format, inspect } from "node:util";

import { userFrames } from "./frames.js";

/** Which of the process's two streams a chunk was written to. */
export type OutputStream = "stdout" | "stderr";

/** One thing a test — or the file around it — printed. */
export type OutputChunk = {|
  readonly stream: OutputStream,
  /** Full name of the case that was running, or `null` when none was. */
  readonly test: string | null,
  /** The text as it would have reached the terminal, newline included. */
  readonly text: string,
|};

/** Where captured output goes. */
export type OutputSink = (chunk: OutputChunk) => void;

/**
 * Longest single write kept, in UTF-16 code units.
 *
 * A test that prints a megabyte-long serialised fixture meant to print
 * something; the first few kilobytes of it are what says what happened, and
 * the rest is not a report's to carry.
 */
export const MAX_CHUNK_LENGTH: number = 8 * 1024;

/** Most output kept from one file, in UTF-16 code units. */
export const MAX_FILE_LENGTH: number = 128 * 1024;

/** Which stream each replaced `console` method writes to, as Node routes them. */
const CONSOLE_STREAMS: { readonly [string]: OutputStream } = {
  log: "stdout",
  info: "stdout",
  debug: "stdout",
  warn: "stderr",
  error: "stderr",
};

/** Decoder for a `write` that was handed bytes rather than a string. */
const DECODER = new TextDecoder();

/** What a stream's `write` calls when it has taken the chunk. */
type WriteCallback = () => mixed;

/**
 * The case a write belongs to, kept in the asynchronous context it ran in.
 *
 * Full names rather than a record, because that is the whole of what a chunk
 * needs to say and the protocol carries it as a string either way.
 */
const owner: AsyncLocalStorage<string> = new AsyncLocalStorage();

let sink: OutputSink | null = null;
let raw: ((chunk: string) => void) | null = null;
let captured = 0;
let stopped = false;

/**
 * The globals to patch, in one place.
 *
 * The one untyped expression in this module, and it is the trust boundary
 * itself: `console` and `process.stdout` are the host's, their libdef types
 * are read-only, and replacing them is exactly what this module is for.
 */
function host(): $FlowFixMe {
  return globalThis;
}

/**
 * `args` rendered the way `console.log` renders them.
 *
 * Node's console hands its arguments to `util.format`, and this is that in two
 * cases rather than one, because `format`'s first parameter is the template:
 * with a leading string it substitutes `%s`, `%d`, `%o` and the rest and
 * inspects whatever is left over, and with anything else there is no template
 * to substitute into, so every argument is inspected and the results joined
 * with a space. Both are what Node prints.
 */
function formatArguments(args: $ReadOnlyArray<mixed>): string {
  const [first, ...rest] = args;
  if (typeof first === "string") {
    return format(first, ...rest);
  }
  return args.map((value) => (typeof value === "string" ? value : inspect(value))).join(" ");
}

/** The text of one `write` argument, whether it arrived as bytes or a string. */
function textOf(chunk: mixed): string {
  if (typeof chunk === "string") {
    return chunk;
  }
  if (chunk instanceof Uint8Array) {
    return DECODER.decode(chunk);
  }
  return String(chunk);
}

/**
 * Record one piece of output, within the file's budget.
 *
 * Silently doing nothing when no sink is installed is deliberate: a test that
 * prints must never fail because of how it was run.
 */
function capture(stream: OutputStream, text: string): void {
  const to = sink;
  if (to == null || stopped || text === "") {
    return;
  }
  let kept = text.length > MAX_CHUNK_LENGTH ? `${text.slice(0, MAX_CHUNK_LENGTH)}…\n` : text;
  const room = MAX_FILE_LENGTH - captured;
  if (kept.length >= room) {
    kept = `${kept.slice(0, Math.max(room, 0))}\n[uf] output stopped after ${MAX_FILE_LENGTH} characters\n`;
    stopped = true;
  }
  captured += kept.length;
  to({ stream, test: owner.getStore() ?? null, text: kept });
}

/** A stand-in for `process.stdout.write` / `process.stderr.write`. */
function writer(
  stream: OutputStream,
): (chunk: mixed, encoding?: string | WriteCallback, callback?: WriteCallback) => boolean {
  return (chunk, encoding, callback) => {
    capture(stream, textOf(chunk));
    // `write(chunk, callback)` and `write(chunk, encoding, callback)` are both
    // real calls, and a caller that passed a callback is waiting for it.
    //
    // Deferred, because the real `Writable.write` never calls back before it
    // returns: a caller that writes and then does something on the next line
    // has that line run first, and one whose callback ran inline would see the
    // two in the other order. `queueMicrotask` rather than `process.nextTick`
    // so this holds on every host uf supports.
    const done = typeof encoding === "function" ? encoding : callback;
    if (done != null) {
      queueMicrotask(done);
    }
    // The real method returns whether the stream has room for more. Nothing is
    // buffered here, so it always has — and a caller told otherwise would wait
    // for a `drain` that never comes.
    return true;
  };
}

/**
 * Route everything a test prints to `to` instead of to the process's streams,
 * and return the real `process.stdout.write` that was replaced.
 *
 * The caller gets the raw stream because the caller is the protocol, and a
 * protocol sharing its stream with the code it reports on is the bug this
 * module exists for. Handing it back from here rather than letting the caller
 * read it first is what makes "taken before anything could have replaced it"
 * true by construction.
 *
 * Installed once, for the life of the worker: a worker runs many files, and
 * restoring the real methods between them would leave a window in which a
 * straggling `setTimeout` from the previous file writes into the protocol.
 *
 * # A page has no stream to take
 *
 * `uf test --browser` runs this same capture inside a page
 * (`./browser/page.js`), and there the whole premise of the returned value is
 * absent: there is no `process.stdout`, so there is nothing for a test to
 * write into by accident, and the protocol's channel is a separate HTTP
 * request rather than a stream anything else can reach. So the two `write`
 * methods are only replaced when there are two `write` methods, and the
 * "raw stream" handed back is a no-op nobody has a use for.
 *
 * Deliberately not a `typeof process` check at each use: the question is asked
 * once, here, because the answer cannot change under a running host.
 */
export function install(to: OutputSink): (chunk: string) => void {
  const global = host();
  const already = raw;
  if (already != null) {
    return already;
  }
  const stdout = global.process?.stdout;
  const real = stdout?.write;
  const protocol =
    real == null
      ? (_chunk: string) => {}
      : (chunk: string) => {
          real.call(stdout, chunk);
        };
  raw = protocol;
  sink = to;
  for (const method of Object.keys(CONSOLE_STREAMS)) {
    const stream = CONSOLE_STREAMS[method];
    global.console[method] = (...args: $ReadOnlyArray<mixed>) => {
      capture(stream, `${formatArguments(args)}\n`);
    };
  }
  global.console.trace = (...args: $ReadOnlyArray<mixed>) => {
    // `console.trace` is a message *and* the stack under it, which is the
    // whole reason to call it rather than `console.error`. `Error.stack`
    // opens with `Trace: <message>`, so the trace Node prints is that string
    // with this module's own frames taken off it.
    const error = new Error(formatArguments(args));
    error.name = "Trace";
    capture("stderr", `${userFrames(error.stack) ?? `Trace: ${error.message}`}\n`);
  };

  if (global.process?.stdout != null) {
    global.process.stdout.write = writer("stdout");
    global.process.stderr.write = writer("stderr");
  }
  return protocol;
}

/**
 * Run `body` as `name`, so what it prints — and what it leaves behind to print
 * later — is filed under that case.
 *
 * The runner wraps one case's `beforeEach`, body and `afterEach` in a single
 * call, because those are the one case's work. Whatever `body` returns is
 * returned unchanged, so an `await` on this is an `await` on the case.
 *
 * Nothing here needs an "and now nothing is running" counterpart. Output from
 * an import, a `beforeAll` or a case that has already been reported was never
 * inside this call, so it has no owner and is the file's — which is the
 * property the previous module-level variable had to be reset to keep, and
 * kept only for as long as nothing straggled.
 */
export function runInTest<T>(name: string, body: () => T): T {
  return owner.run(name, body);
}

/**
 * Start one file's output budget over.
 *
 * Only the budget: there is no current case to clear, because a case's
 * ownership lives in the callbacks it started rather than in this module. A
 * straggler from the file before still carries the name it was written under,
 * which the host cannot match to a test of the new file and files under that
 * file instead. See ubugeeei-prod/uf#203.
 */
export function startFile(): void {
  captured = 0;
  stopped = false;
}
