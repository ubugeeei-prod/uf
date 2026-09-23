// @flow
//
// Internal to `@uniflowed/router`: a document as bytes rather than a string.
//
// `server.js` decides *what* the document says; this decides how it leaves.
// The split is here because the two answers are unrelated — a redirect, an
// error page and a page that suspends for half a second all produce the same
// three shapes for a host to choose from — and because everything below is
// about React's two server renderers and the difference between them, which is
// not something the renderer above should have to spell out twice.
//
// # The head is written before the body
//
// `packages/web/head.js` says this from the other side, as the reason `useHead`
// does nothing on a server: once the body is streaming, the head has gone and
// no component can still change it. This module is what makes that true rather
// than merely claimed.
//
// It is also the one thing that stops a document from being a straight
// pass-through of React's chunks. uf has head content React does not know
// about — the built asset URLs and the loader data the client hydrates from —
// and both have to be in the head. So the first chunks are held until the head
// is complete, the tags go in, and everything after that is forwarded
// untouched. What is held is bounded by the head: React writes `<head>` before
// any body content, so waiting for `</head>` never waits for a page.
//
// That is the whole of the buffering, and it is worth being precise about what
// it does *not* delay. A `<Suspense>` fallback is part of the shell, so it goes
// out with the head; the content that replaces it arrives in later chunks that
// pass straight through. The fallback is not what is being waited on — it is
// what is being sent.
//
// # Two renderers, and how the host picks
//
// `renderToPipeableStream` exists in React's Node build and not in its
// Web-standard ones; `renderToReadableStream` is in both. A namespace import is
// what makes that a runtime question rather than an import-time crash: a named
// import of `renderToPipeableStream` from `react-dom/server` in a worker is a
// module that will not link, and the failure is a blank deploy rather than a
// message. So the check is `typeof …` on the namespace, once, below.

import * as React from "react";
import * as ReactDOMServer from "react-dom/server";
import * as ReactDOMStatic from "react-dom/static";

import { createChunkEncoder } from "./flight-chunks.js";
import type { FormState } from "./form-action.js";
import { type StreamRecord, inspected } from "./inspector.js";

/**
 * Where a document is written, when the host has a Node stream.
 *
 * The three methods React's own `pipe` uses, and nothing else. Typed here
 * rather than imported so this module holds no Node types: a worker bundles it
 * too, and `stream$Writable` in the signature would be a Node type in a file
 * that must not need one.
 */
export type WritableLike = {
  readonly write: (chunk: string) => mixed,
  readonly end: () => mixed,
  ...
};

/**
 * A rendered document, in whichever shape the host can take.
 *
 * Three methods rather than one, because the three hosts uf actually has want
 * three different things and converting between them costs a copy of the
 * document: `uf dev` has a `ServerResponse`, `uf start` builds a `Response`,
 * and `uf build` and the tests want the text. Each is a single pass over the
 * same chunks, so exactly one of them may be called.
 */
export type DocumentBody = {|
  /** Write the document into a Node response. Resolves when the last byte is in. */
  readonly pipe: (destination: WritableLike) => Promise<void>,
  /** The document as a web stream, for `new Response(…)`. */
  readonly stream: () => ReadableStream,
  /** The whole document, once it is finished. */
  readonly text: () => Promise<string>,
|};

/**
 * The parts of React's Node destination that Fizz actually uses.
 *
 * Written out rather than `any`, and rather than `stream$Writable`: this module
 * is bundled for workers too, so a Node type in a signature here would be a Node
 * type in a file that must not need one. What is below is the whole contract —
 * if React starts calling something else, this stops compiling, which is the
 * point of writing it down.
 */
type NodeDestination = {
  readonly write: (chunk: string | Uint8Array) => boolean,
  readonly end: () => mixed,
  readonly destroy: (error?: mixed) => mixed,
  readonly on: (event: string, listener: (...args: Array<mixed>) => mixed) => mixed,
  readonly once: (event: string, listener: (...args: Array<mixed>) => mixed) => mixed,
  readonly off: () => mixed,
  readonly removeListener: () => mixed,
  readonly emit: () => boolean,
};

/** The controller a `ReadableStream` source is handed. */
type StreamController = {
  readonly enqueue: (chunk: Uint8Array) => mixed,
  readonly close: () => mixed,
  ...
};

/**
 * A stream of bytes, as much of one as this module reads.
 *
 * Both of React's web-shaped outputs are one — `renderToReadableStream`'s
 * result and `react-dom/static`'s `prelude` — and neither is typed by anything
 * uf can import, so the shape it is used through is stated here.
 */
type ByteSource = {
  readonly getReader: () => {
    readonly read: () => Promise<{ readonly done?: boolean, readonly value?: Uint8Array, ... }>,
    readonly releaseLock: () => mixed,
    ...
  },
  ...
};

/** How the document is assembled around the app's markup. */
export type DocumentShell = {|
  /**
   * Head tags for an app that renders its own `<html>`, inserted before the
   * `</head>` React writes.
   */
  readonly head: string,
  /**
   * For an app that renders no document: everything up to the point uf's own
   * head can still take tags — so it ends *inside* an open `<head>`.
   */
  readonly open: string,
  /** The rest of that head, and everything up to the app's markup. */
  readonly body: string,
  /** Everything after it. */
  readonly close: string,
|};

/** How much unread output the producer is allowed to run ahead by. */
const HIGH_WATER_MARK = 16;

/**
 * The chunks of one render, as an async iterable, with backpressure in both
 * directions.
 *
 * React's Node renderer pushes and a `ReadableStream` pulls, and a host may be
 * slower than either. One queue in the middle answers all three: the producer
 * is told to stop once `HIGH_WATER_MARK` chunks are waiting — which is the
 * `false` from `write` that React's `pipe` honours — and is let go again when
 * the consumer has caught up.
 *
 * Without it a slow client would be answered by a server holding an entire
 * rendered document per request in memory, which is the failure mode streaming
 * exists to avoid; a queue that only ever grows would have been streaming in
 * shape and buffering in fact.
 */
class ChunkQueue {
  #chunks: Array<string> = [];
  #ended: boolean = false;
  #failure: mixed = null;
  #failed: boolean = false;
  #wake: ?() => void = null;
  #drain: Array<() => void> = [];

  /** Add a chunk. Returns whether the producer may keep going. */
  push(chunk: string): boolean {
    this.#chunks.push(chunk);
    this.#ring();
    return this.#chunks.length < HIGH_WATER_MARK;
  }

  /** No more chunks are coming. */
  end(): void {
    this.#ended = true;
    this.#ring();
  }

  /** The render failed after the shell went out; the document is truncated. */
  fail(error: mixed): void {
    this.#failed = true;
    this.#failure = error;
    this.#ended = true;
    this.#ring();
  }

  /** Run `resume` when there is room again. */
  onDrain(resume: () => void): void {
    this.#drain.push(resume);
  }

  #ring(): void {
    const wake = this.#wake;
    this.#wake = null;
    if (wake != null) {
      wake();
    }
  }

  #room(): void {
    if (this.#chunks.length >= HIGH_WATER_MARK || this.#drain.length === 0) {
      return;
    }
    const waiting = this.#drain;
    this.#drain = [];
    for (const resume of waiting) {
      resume();
    }
  }

  async *chunks(): AsyncGenerator<string, void, void> {
    while (true) {
      while (this.#chunks.length > 0) {
        const chunk = this.#chunks.shift();
        this.#room();
        if (chunk != null) {
          yield chunk;
        }
      }
      if (this.#failed) {
        throw this.#failure;
      }
      if (this.#ended) {
        return;
      }
      await new Promise<void>((resolve) => {
        this.#wake = resolve;
      });
    }
  }
}

/**
 * A destination React's `pipe` will write into, backed by a queue.
 *
 * React's Node renderer wants a `Writable`, and the only parts of one it uses
 * are `write`, `end`, `destroy` and the `drain` and `error` events. Handing it
 * the real thing would mean importing `node:stream` into a module a worker also
 * loads, for four methods.
 */
function queueDestination(queue: ChunkQueue): NodeDestination {
  const decoder = new TextDecoder();
  const listeners: Map<string, Array<(...args: Array<mixed>) => mixed>> = new Map();
  const destination = {
    write(chunk: string | Uint8Array): boolean {
      const text = typeof chunk === "string" ? chunk : decoder.decode(chunk, { stream: true });
      const room = queue.push(text);
      if (!room) {
        queue.onDrain(() => {
          for (const listener of listeners.get("drain") ?? []) {
            listener();
          }
        });
      }
      return room;
    },
    end(): mixed {
      queue.end();
      return destination;
    },
    destroy(error?: mixed): mixed {
      if (error != null) {
        queue.fail(error);
      } else {
        queue.end();
      }
      return destination;
    },
    on(event: string, listener: (...args: Array<mixed>) => mixed): mixed {
      listeners.set(event, [...(listeners.get(event) ?? []), listener]);
      return destination;
    },
    once(event: string, listener: (...args: Array<mixed>) => mixed): mixed {
      return destination.on(event, listener);
    },
    off(): mixed {
      return destination;
    },
    removeListener(): mixed {
      return destination;
    },
    emit(): boolean {
      return false;
    },
  };
  return destination;
}

/**
 * Insert uf's head tags, or wrap markup that is not a document.
 *
 * The two shapes `assemble` used to decide between, decided the same way and at
 * the same moment — on the opening bytes rather than on a finished string. An
 * app whose root layout renders `<html>` owns the document and React writes its
 * head; an app that renders only content gets the minimal shell around
 * `<div id="uf-root">` that the client hydrates instead.
 *
 * The document case waits for `</head>`, because that is where the tags go. If
 * React writes a document with no head at all — an app whose root layout is
 * `<html><body>` — the tags go in a head of uf's own, inserted after the
 * opening tag, which is what the browser would have synthesized anyway.
 *
 * The shell case waits for the end of the run of hoistable elements React
 * opened with, because that is where *its* tags go — see [`hoisted`]. Both
 * waits are bounded by the head, and both are the same idea: the head goes out
 * once, and everything that belongs in it has to be in hand by then.
 */
async function* assembled(
  chunks: AsyncGenerator<string, void, void>,
  shell: DocumentShell,
  transformHead?: (html: string) => Promise<string>,
  layout?: Layout,
): AsyncGenerator<string, void, void> {
  let held = "";
  let shape = "unknown";
  // A document uf wraps is the only one whose root is not `<body>` itself.
  const wrapped = () => {
    if (layout != null) {
      layout.rootDepth = advanced(NOTHING_WRITTEN, shell.open + shell.body, 0).depth ?? 0;
    }
  };
  // The opening chunk is the only one the hook sees, and every path below
  // reaches exactly one of them. Awaiting here rather than at each `yield`
  // keeps them from drifting apart. The hook sees only the document opening:
  // Vite's HTML parser is happy with an open body, but not with a React chunk
  // that happens to end inside an attribute.
  const opening = async (html: string, rest: string = ""): Promise<string> => {
    const split = transformableOpening(html);
    const transformed = transformHead == null ? split.opening : await transformHead(split.opening);
    return transformed + split.rest + rest;
  };

  for await (const chunk of chunks) {
    if (shape === "document-open" || shape === "shell-open") {
      yield chunk;
      continue;
    }
    held += chunk;
    if (shape === "unknown") {
      shape = documentShape(held);
      if (shape === "unknown") {
        continue;
      }
    }
    if (shape === "shell") {
      const split = hoisted(held);
      if (!split.complete) {
        continue;
      }
      shape = "shell-open";
      wrapped();
      yield await opening(shell.open + split.head + shell.body, split.rest);
      held = "";
      continue;
    }
    // A document, and the tags go where its head closes.
    const close = held.indexOf("</head>");
    if (close !== -1) {
      shape = "document-open";
      yield await opening(ufDoctype(held.slice(0, close) + shell.head + held.slice(close)));
      held = "";
      continue;
    }
    // `<body` before `</head>` means React wrote no head; give the tags one.
    const body = held.search(/<body[\s>]/i);
    if (body !== -1) {
      const bodyEnd = held.indexOf(">", body);
      if (bodyEnd === -1) {
        continue;
      }
      shape = "document-open";
      yield await opening(
        ufDoctype(`${held.slice(0, body)}<head>${shell.head}</head>${held.slice(body)}`),
      );
      held = "";
    }
  }

  // The render ended before the decision could be made, or before what was
  // being waited for arrived: an empty document, one with neither `</head>` nor
  // `<body>` in it, or a shell that is hoistable elements all the way down.
  // There is nothing left to wait for in any of them.
  if (shape === "document") {
    yield await opening(ufDoctype(held + shell.head));
    shape = "document-open";
  } else if (shape === "shell" || shape === "unknown") {
    // `complete` is not consulted: nothing more is coming, so a run that was
    // still open is over and whatever was left of it is markup like any other.
    const split = hoisted(held);
    shape = "shell-open";
    wrapped();
    yield await opening(shell.open + split.head + shell.body, split.rest);
  }
  if (shape === "shell-open") {
    yield shell.close;
  } else {
    // The newline `assemble` ended a document with, kept: `uf build` writes
    // these to files, and a file without a trailing newline is a diff with a
    // "\ No newline at end of file" in it forever.
    yield "\n";
  }
}

function transformableOpening(html: string): {| readonly opening: string, readonly rest: string |} {
  const lower = html.toLowerCase();
  const headEnd = lower.indexOf("</head>");
  if (headEnd === -1) {
    return { opening: html, rest: "" };
  }
  const afterHead = headEnd + "</head>".length;
  const body = lower.indexOf("<body", afterHead);
  if (body === -1) {
    return { opening: html.slice(0, afterHead), rest: html.slice(afterHead) };
  }
  const bodyEnd = html.indexOf(">", body);
  if (bodyEnd === -1) {
    return { opening: html.slice(0, afterHead), rest: html.slice(afterHead) };
  }
  const afterBody = bodyEnd + 1;
  return { opening: html.slice(0, afterBody), rest: html.slice(afterBody) };
}

/**
 * The head elements React opened the app's markup with, split from the rest.
 *
 * `complete` is false while the buffer might still be in the middle of one — a
 * chunk that ends inside `<meta cont`, or after a `<link>` and before whatever
 * follows it. Deciding on an incomplete buffer would be a classification that
 * depends on where React split its output, which is the bug `documentShape`
 * above is written the way it is to avoid. The split is filled in either way,
 * because the caller that has run out of chunks has nothing left to wait for
 * and wants it.
 *
 * # Why a leading run rather than the whole document
 *
 * React hoists a `<title>`, a `<meta>` and a `<link>` into the `<head>` of a
 * document *it* rendered. uf's shell is not one — React is handed the app, not
 * the document — so with the shell every one of those landed in the body, and
 * `<link rel="canonical">` in a body is a canonical link Google does not read.
 * The fix is for uf to do the hoisting into the head it wrote itself.
 *
 * A leading run is what can be hoisted without holding the document. `RouteView`
 * renders the route's metadata first, before the layouts and the page, so the
 * run is exactly that metadata and the wait ends at the first byte of the
 * application's own markup. Scanning further would mean buffering an arbitrary
 * amount of a document to find a `<meta>` that might be at the end of it, which
 * is streaming in shape and buffering in fact — the same trade this module
 * refuses in `ChunkQueue`. So a tag a component renders further in stays where
 * it is, and on a client React will hoist it into `document.head` itself.
 *
 * # Reading React's markup with a regular expression
 *
 * Which is only safe because it is React's. React escapes `>` in an attribute
 * value and `<` in text, so the first `>` after an opening tag ends it and the
 * first `</title>` ends a title — neither can appear inside one. This function
 * is not an HTML parser and must never be handed markup from anywhere else.
 */
function hoisted(held: string): HoistedHead {
  let index = 0;
  while (index < held.length) {
    const rest = held.slice(index);
    const split = { head: held.slice(0, index), rest, complete: false };
    const open = rest.match(/^<(title|meta|link)(?=[\s/>])/i);
    if (open == null) {
      // Not a hoistable element, or not yet enough bytes to say it is not one.
      return { ...split, complete: !couldOpenHoistable(rest) };
    }
    const close = held.indexOf(">", index);
    if (close === -1) {
      return split;
    }
    if (open[1].toLowerCase() !== "title") {
      index = close + 1;
      continue;
    }
    const end = held.indexOf("</title>", close);
    if (end === -1) {
      return split;
    }
    index = end + "</title>".length;
  }
  // Every byte so far is a complete hoistable element, and the next one may
  // still be on its way — the case a document that is metadata and nothing else
  // ends in, and the reason the loop is bounded by the buffer rather than by
  // `true`: a `while (true)` here is a function the checker reads as returning
  // `void` on a path it cannot see is unreachable.
  return { head: held, rest: "", complete: false };
}

/** What [`hoisted`] found, and whether more bytes could still change it. */
type HoistedHead = {|
  /** The hoistable elements the markup opened with. */
  readonly head: string,
  /** Everything after them. */
  readonly rest: string,
  /** Whether the run is known to have ended. */
  readonly complete: boolean,
|};

/**
 * Whether `rest` could still turn into a hoistable element once more bytes
 * arrive.
 *
 * True for `"<"` and for every proper prefix of `<title`, `<meta` and `<link` —
 * the states a chunk boundary can leave the buffer in. False for `<main`, and
 * false for `<titlebar>`, which is somebody's component and not a title however
 * much of it has arrived.
 */
function couldOpenHoistable(rest: string): boolean {
  const text = rest.toLowerCase();
  return ["<title", "<meta", "<link"].some((tag) => tag.startsWith(text));
}

/**
 * uf's spelling of the doctype, in place of whichever one React wrote.
 *
 * A doctype is case-insensitive, so this is a formatting choice and not a
 * correctness one — and uf already made it: `redirectDocument` and the shell
 * around an app that renders no document both write `<!doctype html>`. Leaving
 * React's `<!DOCTYPE html>` here would mean a project's documents were spelled
 * one way or the other depending on whether its root layout renders `<html>`,
 * which is not a distinction anybody asked for.
 */
function ufDoctype(document: string): string {
  const existing = document.match(/^\s*<!doctype[^>]*>\s*/i);
  const rest = existing == null ? document.replace(/^\s+/, "") : document.slice(existing[0].length);
  return `<!doctype html>\n${rest}`;
}

/**
 * Whether the bytes so far are the start of a document, and `"unknown"` when
 * they are still too few to say.
 *
 * Read off the buffer rather than off the first chunk, because React decides
 * where to split its output and a classification that depended on the split
 * would be a bug that only appeared under load. `<!DOCTYPE html>` alone is the
 * case that makes the third answer necessary: it is a complete token, it is
 * not yet `<html`, and calling it either answer would be wrong.
 */
function documentShape(held: string): string {
  const text = held.replace(/^\s+/, "").toLowerCase();
  if (text === "") {
    return "unknown";
  }
  let rest = text;
  if (text.startsWith("<!doctype")) {
    const close = text.indexOf(">");
    if (close === -1) {
      return "unknown";
    }
    rest = text.slice(close + 1).replace(/^\s+/, "");
  } else if ("<!doctype".startsWith(text)) {
    return "unknown";
  }
  if (rest === "" || "<html".startsWith(rest)) {
    return "unknown";
  }
  // The delimiter matters: `<htmlish>` is somebody's component, not a document.
  return /^<html[\s>]/.test(rest) ? "document" : "shell";
}

/**
 * The three shapes, over one pass of the chunks.
 *
 * `stop` is what a reader that gives up has to be able to do. A `HEAD` asks for
 * a document and wants none of it, and a browser that navigates away closes the
 * socket — and in both cases React is still rendering into a queue whose
 * consumer is gone. Without this it renders until the queue is full and then
 * waits for a drain that is never coming, which is a request that never ends.
 */
function bodyOf(chunks: AsyncGenerator<string, void, void>, stop?: () => void): DocumentBody {
  return {
    async pipe(destination: WritableLike): Promise<void> {
      // `finally`, so a render that fails partway still closes the response.
      // The alternative is a client holding an open connection to a document
      // that stopped, waiting for bytes nobody is going to send.
      try {
        for await (const chunk of chunks) {
          destination.write(chunk);
        }
      } finally {
        destination.end();
      }
    },
    stream(): ReadableStream {
      const encoder = new TextEncoder();
      // `pull`, not a loop in `start`: the queue's backpressure only means
      // anything if this end waits to be asked.
      return new ReadableStream({
        async pull(controller: StreamController) {
          const next = await chunks.next();
          if (next.done === true) {
            controller.close();
            return;
          }
          controller.enqueue(encoder.encode(next.value));
        },
        cancel(): void {
          stop?.();
          void chunks.return(undefined);
        },
      });
    },
    async text(): Promise<string> {
      let out = "";
      for await (const chunk of chunks) {
        out += chunk;
      }
      return out;
    },
  };
}

/** A document that is already text — a redirect, or a caller's own markup. */
export function bodyOfText(html: string): DocumentBody {
  async function* one(): AsyncGenerator<string, void, void> {
    yield html;
  }
  return bodyOf(one());
}

/** What React is told about a render, over both renderers. */
export type RenderOptions = {|
  readonly shell: DocumentShell,
  /**
   * Every exception React recovers from, including the ones inside a
   * `<Suspense>` that it answered by streaming the boundary's fallback. The
   * shell's own failure is not reported here — it rejects instead.
   */
  readonly onError: (error: mixed) => void,
  /**
   * Rewrite the document opening — the head and, when present, the body start
   * tag — before it goes out.
   *
   * For `uf dev`, and only for it. Vite's `transformIndexHtml` rewrites asset
   * URLs and injects `/@vite/client` and the refresh preamble, and it is a
   * *whole document* hook, so the development server used to collect the page
   * and transform it at the end. That made the one place a developer would
   * notice streaming the one place it did not happen: a slow page showed
   * nothing until it was finished, and `$loading.js` looked broken.
   * See ubugeeei-prod/uf#374.
   *
   * The hook never sees application body markup, which is what makes this
   * safe. Vite's injections are string-based against `<head>` and `<body>`,
   * and its parser accepts a document that stops after the body start tag.
   * It does not accept a React chunk that ends in the middle of an attribute.
   *
   * Absent everywhere else. `uf start`, `uf preview` and every deploy adapter
   * have no such hook and stream already.
   */
  readonly transformHead?: (html: string) => Promise<string>,
  /**
   * Told what left, in what order, and what each chunk built.
   *
   * For `uf dev`, like `transformHead` above, and absent everywhere else —
   * which is what makes it free rather than cheap: with nothing supplied no
   * recorder is constructed, `./inspector.js` is never entered, and the
   * generator a host consumes is the same object it was. See that module for
   * what is done with the record and why it is a development affordance rather
   * than a metric.
   *
   * Called once per document, after the last chunk and also after a render that
   * was abandoned partway. `prerenderDocument` never calls it: a build resolves
   * everything before it writes a byte, so there is no order to report.
   */
  readonly onStream?: (record: StreamRecord) => void,
  /**
   * The Flight payload this document's tree was read from, to write into the
   * document for the browser to hydrate from.
   *
   * Absent for a document rendered from the route's modules, which is what
   * every document was before ubugeeei-prod/uf#519 and what `app.rsc: false`
   * still asks for. Present, it is copied into the document as it arrives, by
   * [`interleaved`], which says where it may and may not go.
   */
  readonly payload?: ReadableStream<Uint8Array>,
  /**
   * This response's Content-Security-Policy nonce, or absent for none.
   *
   * Absent is what every render had before nonces existed and what every
   * render still has in a project that has not asked for one, so a document
   * written without it is byte-for-byte the document uf has always written.
   *
   * Present, it reaches three places, and it has to reach all three or the
   * page is broken rather than merely unprotected: React's own option, which
   * nonces every inline script React emits — the runtime that reveals a
   * `<Suspense>` boundary, and the bootstrap; `shell.open`/`shell.body`, which
   * carry the client entry; and the payload chunk elements, which
   * [`interleaved`] writes as text.
   *
   * `prerenderDocument` is deliberately never given one. See its own paragraph.
   */
  readonly nonce?: string | null,
  /**
   * React's `formState`, for a document that answers a form posted before
   * hydration: the `useActionState` that submitted starts from the action's
   * result, and React marks it so the browser's hydration picks the same one.
   * The shell carries the same value for `hydrateRoot`; see
   * `./form-action.js`. Absent for every other render.
   */
  readonly formState?: FormState,
|};

/**
 * `chunks`, recorded on the way out when `uf dev` asked for it.
 *
 * The wrapping goes here rather than around `assembled` in each of the callers
 * so that what is recorded is unambiguous: these are the bytes the host is
 * handed, head and all, and not React's own output on the way past.
 */
function outgoing(
  chunks: AsyncGenerator<string, void, void>,
  onStream?: (record: StreamRecord) => void,
): AsyncGenerator<string, void, void> {
  return onStream == null ? chunks : inspected(chunks, onStream, () => performance.now());
}

/**
 * Stream `node` as a document, resolving once the shell is ready.
 *
 * Resolving on the shell rather than on the whole document is the entire point:
 * the caller has a status and a body to answer with while the page is still
 * rendering. It rejects when the *shell* throws, and that is a different event
 * from a page throwing — nothing has been written yet, so the caller can still
 * resolve the error route and render it instead, which is what `createRenderer`
 * does and what ubugeeei-prod/uf#257 is about.
 */
export function renderDocument(node: React.Node, options: RenderOptions): Promise<DocumentBody> {
  const queue = new ChunkQueue();
  return new Promise((resolve, reject) => {
    if (typeof ReactDOMServer.renderToPipeableStream === "function") {
      const { pipe, abort } = ReactDOMServer.renderToPipeableStream(node, {
        onShellReady() {
          pipe(queueDestination(queue));
          resolve(
            bodyOf(
              outgoing(
                withPayload(
                  queue.chunks(),
                  options.shell,
                  options.transformHead,
                  options.payload,
                  options.nonce,
                ),
                options.onStream,
              ),
              () => abort(),
            ),
          );
        },
        onShellError(error: mixed) {
          reject(error);
        },
        onError: options.onError,
        // Every inline script React writes for this document, from one option:
        // the streaming runtime that reveals a boundary and patches a segment,
        // and the bootstrap. uf nonces the scripts it writes itself; these are
        // React's, and there is no other way to reach them.
        nonce: options.nonce ?? undefined,
        formState: options.formState ?? null,
      });
      return;
    }
    // A Web-standard host: no `pipe`, and the stream itself is what is
    // awaited. `renderToReadableStream`'s promise settles on the shell, which
    // is the same moment `onShellReady` is.
    renderWithReadableStream(ReactDOMServer.renderToReadableStream, node, options).then(
      resolve,
      reject,
    );
  });
}

/**
 * A `renderToReadableStream`, as this module calls one.
 *
 * Written down so [`renderWithReadableStream`] can be driven with something
 * that is not React's. The branch below only runs on a host that has no
 * `renderToPipeableStream` — a worker, never a test process — and a branch no
 * test can reach is exactly how it came to be the one missing the cancellation
 * its Node twin has had since it was written.
 */
type ReadableStreamRenderer = (
  node: React.Node,
  settings: {|
    readonly onError: (error: mixed) => void,
    readonly signal: AbortSignal,
    readonly nonce?: string,
    readonly formState?: FormState | null,
  |},
) => Promise<ByteSource>;

/**
 * The Web-standard half of [`renderDocument`], with a way to stop the render.
 *
 * The Node path holds `renderToPipeableStream`'s own `abort` and calls it when
 * the consumer gives up. This one has no such handle, so it renders under an
 * `AbortSignal` and aborts it in the same place — and without that,
 * `releaseLock` in [`decoded`] merely detaches the reader while React goes on
 * rendering into a stream nobody will ever read again, for however long the
 * page's slowest boundary takes.
 *
 * That is not the exotic case. A `HEAD` cancels, and so does every browser that
 * navigates away mid-document; on a worker each one would leave a render
 * running against whatever CPU budget the host meters. The two paths answer
 * every other question the same way, and this was the last one where they
 * disagreed.
 */
export function renderWithReadableStream(
  render: ReadableStreamRenderer,
  node: React.Node,
  options: RenderOptions,
): Promise<DocumentBody> {
  const controller = new AbortController();
  return render(node, {
    onError: options.onError,
    signal: controller.signal,
    nonce: options.nonce ?? undefined,
    formState: options.formState ?? null,
  }).then((stream: ByteSource) =>
    bodyOf(
      outgoing(
        withPayload(
          decoded(stream),
          options.shell,
          options.transformHead,
          options.payload,
          options.nonce,
        ),
        options.onStream,
      ),
      () => {
        controller.abort();
      },
    ),
  );
}

/** A web stream of bytes, as the string chunks the rest of this module speaks. */
async function* decoded(stream: ByteSource): AsyncGenerator<string, void, void> {
  const decoder = new TextDecoder();
  const reader = stream.getReader();
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done === true) {
        const rest = decoder.decode();
        if (rest !== "") {
          yield rest;
        }
        return;
      }
      yield decoder.decode(value, { stream: true });
    }
  } finally {
    // Reached when a consumer stops early — `return()` on this generator lands
    // here — and the lock has to go back or the render behind it never ends.
    reader.releaseLock();
  }
}

/**
 * Render `node` to a finished document, with everything resolved.
 *
 * `uf build`'s renderer, and deliberately not `renderDocument` with the chunks
 * joined up. React has two server renderers and they answer two different
 * questions: the streaming one sends a fallback and then patches it from a
 * script, because a browser is on the other end and the point is what it can
 * paint first; the static one waits, and writes the resolved content where the
 * fallback would have been. A file in `dist/` has no first paint to optimize
 * and no guarantee that whatever serves it runs scripts at all, so it wants the
 * second — an `index.html` full of `<template>` placeholders waiting for
 * `$RC()` would be a page that is blank to a crawler and to `curl`.
 *
 * That is the static/streaming split, and it is this function versus the one
 * above rather than a flag threaded through one of them.
 */
export async function prerenderDocument(node: React.Node, options: RenderOptions): Promise<string> {
  const settings = { onError: options.onError };
  const result =
    typeof ReactDOMStatic.prerenderToNodeStream === "function"
      ? await ReactDOMStatic.prerenderToNodeStream(node, settings)
      : await ReactDOMStatic.prerender(node, settings);
  return bodyOf(
    withPayload(
      preludeChunks(result.prelude),
      options.shell,
      options.transformHead,
      options.payload,
    ),
  ).text();
}

/**
 * A page's static shell, and what React needs to finish it per request.
 *
 * What `uf build` writes for a page it prerenders partially (`ppr`): the whole
 * document React could render without a request, with each `<Suspense>`
 * boundary that waited on one written as its fallback, and React's record of
 * where those holes are.
 */
export type PrerenderedShell = {|
  /**
   * The document as far as a server sends it before it renders anything: the
   * head, uf's tags in it, and every byte React finished. It stops where the
   * request's part begins, which is before the closing tags.
   */
  readonly html: string,
  /**
   * What follows the request's part: the closing tags of a document uf wraps
   * around the application. Empty for a document the application renders
   * itself, because React's `resume` writes `</body></html>` for that one.
   */
  readonly close: string,
  /** See [`Layout`]'s field of the same name. */
  readonly rootDepth: number,
  /**
   * React's postponed state: which boundaries are holes and how to find them
   * again. Plain JSON, which is what lets a build write it to a file and a
   * server read it back. `null` when the prerender left nothing for the
   * request, and `html` is then a whole document.
   */
  readonly postponed: mixed,
|};

/**
 * Prerender `node` to a static shell, leaving whatever is still waiting when
 * `settle` resolves as a hole.
 *
 * React's `prerender` with an abort. A boundary whose content is waiting on
 * something when the render is stopped is written as its fallback, and React
 * returns its postponed state beside the markup; [`resumeDocument`] hands that
 * state back to React per request and streams only what was left.
 *
 * `settle` decides what is waiting, and it can only ever make a hole larger:
 * stopped early, a boundary that would have finished is a hole instead, which
 * the request then renders — slower, never wrong. The router waits until the
 * payload it feeds this render has been read and every module it names has
 * loaded, so what is left waiting is what was left out on purpose.
 *
 * Like [`prerenderDocument`], it is never given a nonce: what it writes is a
 * file every request is sent.
 */
export async function prerenderShell(
  node: React.Node,
  options: {|
    readonly shell: DocumentShell,
    readonly onError: (error: mixed) => void,
    readonly settle: () => Promise<void>,
  |},
): Promise<PrerenderedShell> {
  const controller = new AbortController();
  // The reason every hole is reported with when the render stops. Stopping is
  // the point rather than a failure, so it is not passed on.
  const stopped = new Error("@uniflowed/router: the static shell is complete");
  const settings = {
    onError: (error: mixed) => {
      if (error !== stopped) options.onError(error);
    },
    signal: controller.signal,
  };
  const pending =
    typeof ReactDOMStatic.prerenderToNodeStream === "function"
      ? ReactDOMStatic.prerenderToNodeStream(node, settings)
      : ReactDOMStatic.prerender(node, settings);
  await options.settle();
  controller.abort(stopped);
  const result = await pending;
  const layout: Layout = { rootDepth: 0 };
  const text = await bodyOf(
    assembled(preludeChunks(result.prelude), options.shell, undefined, layout),
  ).text();
  const postponed = result.postponed ?? null;
  if (postponed == null) {
    return { html: text, close: "", rootDepth: layout.rootDepth, postponed: null };
  }
  // A shell uf wrapped ends in uf's closing tags, which `resume` does not write;
  // a document React wrote ends in its own, which `resume` writes again.
  if (text.endsWith(options.shell.close)) {
    return {
      html: text.slice(0, text.length - options.shell.close.length),
      close: options.shell.close,
      rootDepth: layout.rootDepth,
      postponed,
    };
  }
  return {
    html: text.replace(/<\/body>\s*<\/html>\s*$/i, ""),
    close: "",
    rootDepth: layout.rootDepth,
    postponed,
  };
}

/**
 * A page's static shell, then what the request fills its holes with.
 *
 * The shell goes out as the first chunk, before React has rendered anything for
 * this request — that is the whole of what partial prerendering buys, and it is
 * why the shell is text the build wrote rather than a render. React's `resume`
 * then renders the holes, and its output follows the shell unchanged: each
 * hole's content in a hidden element and the script that moves it into place,
 * which is what a streamed `<Suspense>` boundary always was.
 *
 * `payload` is the request's Flight payload, the one the holes were rendered
 * from, and it is written into the document by [`interleaved`] under the same
 * rules as a streamed render's. The shell ends between elements at the root's
 * own level, so the payload may follow it as soon as it exists.
 */
export function resumeDocument(
  node: React.Node,
  shell: PrerenderedShell,
  options: {|
    readonly onError: (error: mixed) => void,
    readonly payload: ReadableStream<Uint8Array>,
    readonly nonce?: string | null,
    readonly onStream?: (record: StreamRecord) => void,
  |},
): DocumentBody {
  const controller = new AbortController();
  // Begun now and awaited only after the shell has gone: the shell must not
  // wait for anything React does for this request, and React must not wait
  // for the shell to be read before it starts on the holes.
  const resuming: Promise<ByteSource> = ReactDOMServer.resume(node, shell.postponed, {
    onError: options.onError,
    signal: controller.signal,
    nonce: options.nonce ?? undefined,
  });
  // A resume that fails before its first byte fails the stream below, once it
  // is read; this keeps the rejection from being reported as unhandled while
  // the shell is still being written.
  resuming.catch(() => {});
  async function* chunks(): AsyncGenerator<string, void, void> {
    yield shell.html;
    yield* decoded(await resuming);
    if (shell.close !== "") {
      yield shell.close;
    }
  }
  const layout: Layout = { rootDepth: shell.rootDepth };
  return bodyOf(
    outgoing(interleaved(chunks(), options.payload, options.nonce, layout), options.onStream),
    () => {
      controller.abort();
    },
  );
}

/**
 * What [`assembled`] learns about a document that [`interleaved`] needs.
 *
 * Which shape a document has is decided on its opening bytes, inside
 * `assembled`, and `interleaved` reads that document's chunks — so by the time
 * it has HTML to follow, the answer is here.
 */
type Layout = {|
  /**
   * How many elements are open inside `<body>` where React's root begins: none
   * for a document React wrote, and uf's own container for one uf wraps.
   */
  rootDepth: number,
|};

/** [`assembled`] alone when there is no payload, and [`interleaved`] with one. */
function withPayload(
  chunks: AsyncGenerator<string, void, void>,
  shell: DocumentShell,
  transformHead: ?(html: string) => Promise<string>,
  payload: ?ReadableStream<Uint8Array>,
  nonce?: string | null,
): AsyncGenerator<string, void, void> {
  if (payload == null) {
    return assembled(chunks, shell, transformHead ?? undefined);
  }
  const layout: Layout = { rootDepth: 0 };
  return interleaved(
    assembled(chunks, shell, transformHead ?? undefined, layout),
    payload,
    nonce,
    layout,
  );
}

/**
 * A document's chunks, with the Flight payload it was rendered from written
 * into it as it arrives.
 *
 * Six rules, and each is the answer to a way the obvious version is wrong.
 *
 * **Nothing before the head.** The first chunk this is handed is the whole
 * opening of the document — `assembled` does not let one go until the head is
 * complete — and a payload element written in front of it would sit before
 * `<head>`, where the parser would open a body for it and every tag after it
 * would land in the wrong element.
 *
 * **Written as soon as it exists.** A payload row usually exists before the
 * HTML rendered from it — React's client reads the row, then the boundary
 * renders — so waiting for the next HTML chunk would put the browser's copy
 * behind the markup it hydrates. Each HTML chunk that ends between elements is
 * followed by whatever payload is waiting, and a payload that arrives while the
 * HTML is idle there is written then, without a chunk of HTML to follow.
 *
 * **Only between elements.** React hands its HTML on through a fixed-size
 * buffer, so a chunk can end anywhere: inside a tag, an attribute's value, a
 * comment, a character reference or an inline `<script>`. A payload element
 * written after such a chunk is not an element. It is part of the attribute,
 * the comment or the text it landed in, the browser never reads it, and React's
 * client closes the payload with rows missing, which is the "Connection closed"
 * a page reports instead of hydrating. So a waiting payload goes out only where
 * the HTML written so far ends between elements, which [`advanced`] follows
 * from chunk to chunk, and otherwise waits for the HTML that finishes what is
 * open.
 *
 * **Only where React steps over it.** Between elements is where the *parser*
 * keeps a payload element. React is stricter: hydrating, it steps over an
 * element it did not render only among the children of its root and of
 * `<html>`, `<head>` and `<body>`. Anywhere deeper the element is a child the
 * tree has no fiber for, the hydration fails there, and React throws the
 * server's markup away and renders the document again. A shell that is one
 * large element — a frame around the page, all of it rendered on the server —
 * is written in many pieces, several of which end between two of its children,
 * and a payload that was already waiting used to go out at the first of them.
 * So [`advanced`] also follows how many elements are open inside `<body>`, and
 * a payload waits for the root's own level: between React's top-level
 * elements, which is where every streamed segment is written anyway.
 *
 * **Outside replaceable boundaries.** Even between tags, a payload inside a
 * Suspense fallback disappears when React reveals its content. A browser that
 * loads the Flight reader afterwards then sees an incomplete stream. React's
 * boundary comments keep those ranges closed to injection until their end.
 *
 * **`</body></html>` waits for the end of the payload.** The HTML can finish
 * first — the last boundary's markup is rendered from rows that are already
 * written — and a payload element after `</html>` is one the parser moves
 * rather than one React expects. So the closing tags are held back, the rest of
 * the payload and its end marker are written, and the tags go last. For a
 * document uf wraps, the closing run is `</div></body></html>`, and only the
 * part from `</body>` is held: the root element React hydrates must hold
 * nothing React did not render.
 *
 * A consumer that stops early stops the payload too: the reader is cancelled
 * in the `finally`, which is where `return()` on this generator lands.
 */
async function* interleaved(
  chunks: AsyncGenerator<string, void, void>,
  payload: ReadableStream<Uint8Array>,
  nonce?: string | null,
  layout?: Layout,
): AsyncGenerator<string, void, void> {
  const encoder = createChunkEncoder(nonce);
  const reader = payload.getReader();
  let written = "";
  let ended = false;
  let wake: ?() => void = null;
  const ring = () => {
    const resume = wake;
    wake = null;
    if (resume != null) {
      resume();
    }
  };
  const pumping = (async () => {
    try {
      while (true) {
        const step = await reader.read();
        if (step.done === true) {
          break;
        }
        if (step.value != null) {
          written += encoder.encode(step.value);
          ring();
        }
      }
    } catch {
      // A payload that failed partway is still ended, with the end marker, so
      // the browser's reader closes and React reports the rows it never got
      // rather than waiting for them for as long as the page is open.
    } finally {
      written += encoder.end();
      ended = true;
      ring();
    }
  })();

  // Nothing written yet, and nothing may go before the head.
  let boundary: Boundary = NOTHING_WRITTEN;
  let closing = "";
  try {
    let next = chunks.next();
    while (true) {
      if (boundary.safe && written !== "") {
        const out = written;
        written = "";
        yield out;
      }
      const idle = new Promise<null>((resolve) => {
        wake = () => resolve(null);
      });
      const outcome = await Promise.race([next, idle]);
      if (outcome == null) {
        continue;
      }
      if (outcome.done === true) {
        break;
      }
      let text = outcome.value;
      const close = text.search(/<\/body>\s*<\/html>\s*$/i);
      if (close !== -1) {
        closing = text.slice(close);
        text = text.slice(0, close);
      }
      if (text !== "") {
        yield text;
        boundary = advanced(boundary, text, layout?.rootDepth ?? 0);
      }
      next = chunks.next();
    }
    while (!ended || written !== "") {
      if (written !== "") {
        const out = written;
        written = "";
        yield out;
        continue;
      }
      await new Promise<void>((resolve) => {
        wake = resolve;
        if (ended || written !== "") {
          ring();
        }
      });
    }
    await pumping;
    yield closing;
  } finally {
    if (!ended) {
      void reader.cancel();
    }
  }
}

/** Where the HTML written so far leaves the next payload element. */
type Boundary = {|
  /** Whether a payload element may be written now. */
  readonly safe: boolean,
  /** The `script` or `style` element the HTML is inside, if it is inside one. */
  readonly rawText: string | null,
  /** Suspense/Activity ranges React can replace before hydration starts. */
  readonly replaceable: number,
  /** A tag, or a comment, the HTML has started and not yet finished. */
  readonly open: string,
  /** How many elements are open inside `<body>`; `null` until it has opened. */
  readonly depth: number | null,
|};

const NOTHING_WRITTEN: Boundary = {
  safe: false,
  rawText: null,
  replaceable: 0,
  open: "",
  depth: null,
};

/** Elements with no closing tag, for markup whose `/>` React did not write. */
const VOID_ELEMENTS: $ReadOnlySet<string> = new Set([
  "area",
  "base",
  "br",
  "col",
  "embed",
  "hr",
  "img",
  "input",
  "link",
  "meta",
  "param",
  "source",
  "track",
  "wbr",
]);

/**
 * `boundary`, once `html` has been written after it.
 *
 * A payload element may follow HTML that ends with a `>` outside a `<script>`
 * and a `<style>`. The test can be that short because React wrote the HTML: it
 * escapes `<` and `>` in text and in attribute values, so a `>` it wrote closes
 * a tag or a comment, and HTML that ends any other way ends inside one, or
 * inside a character reference or a run of text. What React does not escape is
 * the content of an inline script or stylesheet, where a `>` is code, so an
 * opening `<script>` or `<style>` is followed to its closing tag. A tag split
 * across two chunks is carried in `open` and read whole with the next one.
 *
 * And it may follow only HTML that leaves `rootDepth` elements open inside
 * `<body>`, or none: the children of React's root and of `<body>` are the ones
 * hydration steps over. Every tag from `<body>` on is counted for that, React's
 * void elements by the `/>` it writes them with. Markup React did not write —
 * `dangerouslySetInnerHTML` — can leave a tag open that the parser would close,
 * and the count then stays too deep for the rest of the document. That errs the
 * safe way: the payload waits, and goes out before `</body>` as it always can.
 */
function advanced(boundary: Boundary, html: string, rootDepth: number): Boundary {
  const text = boundary.open + html;
  let rawText = boundary.rawText;
  let replaceable = boundary.replaceable;
  let depth = boundary.depth;
  const tags = /<!--([\s\S]*?)-->|<(\/?)([a-z][^\s/>]*)[^>]*?(\/?)>/gi;
  let tag = tags.exec(text);
  while (tag != null) {
    const comment = tag[1];
    if (comment != null) {
      if (rawText == null) {
        if (/^[\$&][?!~]?$/.test(comment)) replaceable += 1;
        else if (comment === "/$" || comment === "/&") replaceable = Math.max(0, replaceable - 1);
      }
    } else {
      const name = tag[3].toLowerCase();
      const closes = tag[2] === "/";
      if (rawText != null) {
        // Inside a script or a stylesheet a `<` is code, up to its own end tag.
        if (closes && rawText === name) {
          rawText = null;
          if (depth != null) depth -= 1;
        }
      } else if (closes) {
        if (depth != null) depth -= 1;
      } else if (depth == null) {
        if (name === "body") depth = 0;
        else if (name === "script" || name === "style") rawText = name;
      } else if (tag[4] !== "/" && !VOID_ELEMENTS.has(name)) {
        depth += 1;
        if (name === "script" || name === "style") rawText = name;
      }
    }
    tag = tags.exec(text);
  }
  const start = text.lastIndexOf("<");
  return {
    safe:
      rawText == null &&
      replaceable === 0 &&
      text.endsWith(">") &&
      (depth === 0 || (depth != null && depth === rootDepth)),
    rawText,
    replaceable,
    open: start > text.lastIndexOf(">") ? text.slice(start) : "",
    depth,
  };
}

/**
 * The prelude of a static prerender, whichever stream this build produced.
 *
 * `prerenderToNodeStream` hands back a Node `Readable` and `prerender` a web
 * `ReadableStream`, and which one a build has depends on which React entry
 * point exists — so the union is real rather than defensive, and `getReader`
 * is what tells them apart.
 */
async function* preludeChunks(
  prelude: ByteSource | AsyncIterable<string | Uint8Array>,
): AsyncGenerator<string, void, void> {
  if (typeof prelude.getReader === "function") {
    yield* decoded(prelude);
    return;
  }
  const decoder = new TextDecoder();
  for await (const chunk of prelude) {
    yield typeof chunk === "string" ? chunk : decoder.decode(chunk, { stream: true });
  }
}
