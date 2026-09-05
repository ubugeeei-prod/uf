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
// start-up, and `run.js` says which case is running so a chunk can be named.
// "Who printed this" is also state with a lifetime of its own — set around a
// case's body and hooks, cleared between them — exactly like the snapshot key
// in `snapshot.js`, and for the same reason it lives beside the thing it
// describes rather than inside either caller.
//
// # What "the test that printed it" means
//
// The name a chunk carries is the case the *worker* is running when the chunk
// arrives, which is not the same as the case whose code produced it. A
// `setTimeout` a test leaves behind prints while the next case is running and
// is filed under that one; a chunk from no case at all is filed under the
// file.
//
// Getting this exactly right needs the printing to be tied to the asynchronous
// context the case ran in — `AsyncLocalStorage` and everything under it — and
// that is a bigger change than this module, because it has to reach the
// scheduler that runs the cases. It is written down here rather than left to
// be discovered from a confusing report. See ubugeeei-prod/uf#207.

// # Bounds
//
// A test that prints in a loop must not be able to fill the pipe, the worker's
// memory or the report, so one file's captured output is bounded and the
// bound is announced rather than hidden: the chunk that reaches it says so and
// nothing after it is kept. The budget starts over for each file, so a chatty
// file does not silence the next one in the same worker.

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

let sink: OutputSink | null = null;
let raw: ((chunk: string) => void) | null = null;
let current: string | null = null;
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
  to({ stream, test: current, text: kept });
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
 */
export function install(to: OutputSink): (chunk: string) => void {
  const global = host();
  const already = raw;
  if (already != null) {
    return already;
  }
  const stdout = global.process.stdout;
  const real = stdout.write;
  const protocol = (chunk: string) => {
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

  global.process.stdout.write = writer("stdout");
  global.process.stderr.write = writer("stderr");
  return protocol;
}

/**
 * Say which case is running, so what it prints can be named.
 *
 * The runner calls this around a case's body and hooks and clears it between
 * them: output written while the module is being imported, from a `beforeAll`,
 * or after the last case finished belongs to the file, not to whichever case
 * happened to run last.
 */
export function enterTest(name: string): void {
  current = name;
}

/** Say that no case is running. */
export function exitTest(): void {
  current = null;
}

/** Start one file's output budget over, with no case running. */
export function startFile(): void {
  captured = 0;
  stopped = false;
  current = null;
}
