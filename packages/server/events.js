// @flow
//
// `@uniflowed/server/events`: a route handler that answers with a stream.
//
// Server-sent events are the smallest of the three things ubugeeei-prod/uf#529
// asks for and the one most applications actually want. A notification badge,
// a build's progress, a document somebody else is editing, a job finishing —
// all of them are the server telling the browser something, none of them is a
// conversation, and a WebSocket for any of them is a second protocol, a second
// failure mode and a second thing to keep alive through a proxy.
//
// # It is a `Response`, so it needs nothing new from the dispatcher
//
// A handler already returns a `Response` and a `Response` body is already
// allowed to be a `ReadableStream`, so an event stream is a handler that
// returns one:
//
//   // app/api/progress/$route.js
//   // @flow
//   import { eventStream } from "@uniflowed/server/events";
//
//   export function GET(): Response {
//     return eventStream((sink) => {
//       const timer = setInterval(() => sink.send({ event: "tick", data: "." }), 1000);
//       return () => clearInterval(timer);
//     });
//   }
//
// No new export name, no new return type, no second dispatcher. That was the
// whole design constraint: this had to fit the surface in
// `@uniflowed/router/handler` rather than sit beside it, because a second way
// to declare a route is a second set of rules about middleware, matching and
// `after()`.
//
// # What this module is actually for, given that
//
// The `Response` is four lines. What is not four lines is everything around
// it, and each piece below is a bug this has instead of a feature:
//
//   * **The wire format.** A payload with a newline in it is two `data:` lines
//     or it is a truncated message; a `\r\n` in it is a third line the reader
//     never sees. [`encodeEvent`] is exported so the suite can drive that
//     directly rather than through a socket.
//   * **The headers.** `text/event-stream` is the one anybody remembers.
//     `cache-control: no-store` stops a shared cache holding the first few
//     events forever and replaying them to the next reader, and
//     `x-accel-buffering: no` is what stops nginx buffering the whole stream
//     and delivering nothing until it ends — the single most common way an
//     event stream that works locally does nothing in production.
//   * **The heartbeat.** An idle connection is closed by load balancers,
//     phones and captive portals, and none of them say so; a comment line
//     every fifteen seconds costs two bytes and keeps the path open. It is a
//     comment rather than an event so a reader never sees it.
//   * **A slow reader.** `docs/security.md` rule 4 is "no unbounded anything",
//     and the producer here is an application while the consumer is somebody's
//     phone on a train. A reader that falls far enough behind is disconnected
//     rather than accumulated; see [`eventStream`].
//
// # Reconnection is the client's half, and `id` is what makes it possible
//
// The browser's own `EventSource` reconnects on its own and sends the last
// `id:` it saw back as `Last-Event-ID`, so a source that numbers its events can
// answer a reconnect with what was missed instead of starting again.
// `retry:` is how a stream states the delay before that attempt.
// `@uniflowed/hooks/events`'s `useEventSource` is the client half, and it
// exists because the platform's reconnection is invisible: an application that
// wants to show "reconnecting…" cannot ask `EventSource` anything.
//
// # Where it does not work, said here rather than discovered there
//
// A target that buffers the response body cannot serve this at all: the
// invocation holds every event in memory and answers when the stream ends,
// which for a stream that never ends is a timeout. That is the serverless
// adapter, and `./internal/capabilities.js` is where a host says so — this
// module refuses at the point the handler builds the response, with the
// target named, rather than returning something that hangs.
//
// A host that declared nothing is not refused. The two capabilities that are
// objects are absent when they do not exist, so absence is the answer; `stream`
// is a boolean, absence is silence, and a `Response` streams by default
// everywhere except where somebody said otherwise.
//
// # `after()` on a stream that never ends
//
// It runs when the stream does, because that is when the host finishes writing
// the response — which for an event stream open for an hour is an hour later.
// Nothing is wrong with that and it surprises people, so: work that should
// happen when the *connection* is established belongs in the handler, and work
// that should happen when it ends belongs in the cleanup this module already
// runs.

import type { ServerCapabilities } from "./internal/capabilities.js";
import { CapabilityUnavailableError } from "./internal/capabilities.js";
import { currentContext } from "./internal/context.js";

export type { ServerCapabilities } from "./internal/capabilities.js";

/**
 * One event, as the fields the format has.
 *
 * `data` is required and everything else is not, because an event with no data
 * is a message a reader cannot act on — the format allows it and no
 * application means it.
 */
export type ServerSentEvent = {|
  /** The event's name, which is what a client listens for. */
  readonly event?: string,
  /** The payload. A newline in it becomes a second `data:` line. */
  readonly data: string,
  /** The cursor a reconnecting client sends back as `Last-Event-ID`. */
  readonly id?: string,
  /** Milliseconds a client should wait before reconnecting. */
  readonly retry?: number,
|};

/**
 * The controller a `ReadableStream` source is handed.
 *
 * Declared here rather than imported, the way `@uniflowed/router`'s renderer
 * declares its own: the shape is four members, and naming it locally is what
 * lets this file be read without knowing which host's stream types are in
 * scope. `desiredSize` is the one that is load-bearing — it is how a producer
 * finds out that the reader is behind.
 */
interface StreamController {
  // An interface with methods, and both halves of that are load-bearing. What
  // a host hands the source is a class instance, and a class instance is not a
  // subtype of an object type however well the shapes line up; a property
  // typed as a function is a function taken off its object, which Flow refuses
  // to unbind when it reads `this` — which every host's controller does.
  enqueue(chunk: Uint8Array): mixed;
  close(): mixed;
  // An `Error` rather than a `mixed`: this is what the reader is given, and a
  // reader that has to guess what it caught is no better off than one that was
  // told nothing. `beginSource` builds one round whatever a source threw.
  error(reason: Error): mixed;
  readonly desiredSize: number | null;
}

/** What a source is handed. */
export type EventSink = {|
  /** Send one event. A bare string is the data with no name and no id. */
  readonly send: (event: ServerSentEvent | string) => void,
  /** Send a comment, which no reader sees. The heartbeat is one of these. */
  readonly comment: (text: string) => void,
  /** End the stream from this end. */
  readonly close: () => void,
  /**
   * Aborted once the stream is over, however it ended.
   *
   * One signal for the client hanging up, `close()`, and the reader falling
   * too far behind, because a producer's cleanup is the same in all three and
   * a producer that had to tell them apart would get one of them wrong.
   */
  readonly signal: AbortSignal,
|};

/**
 * What produces the events.
 *
 * A returned function is run when the stream ends — the shape `useEffect`
 * taught everybody — and `sink.signal` is there for a producer that would
 * rather pass a signal to something than write a cleanup.
 */
export type EventStreamSource = (sink: EventSink) => mixed;

/** How the stream behaves. */
export type EventStreamOptions = {|
  /** Milliseconds between heartbeat comments. `0` sends none. Default 15000. */
  readonly heartbeat?: number,
  /** A `retry:` sent once, before anything else, in milliseconds. */
  readonly retry?: number,
  /** Extra response headers; the four this module sets cannot be replaced. */
  readonly headers?: { readonly [string]: string },
  /**
   * Events the stream may hold for a reader that is behind. Default 64.
   *
   * Not a tuning knob so much as a ceiling: past twice this many the
   * connection is closed. See the module header.
   */
  readonly buffer?: number,
|};

const DEFAULT_HEARTBEAT = 15_000;
const DEFAULT_BUFFER = 64;

/**
 * An event stream, as a `Response` a route handler returns.
 *
 * `source` is called once, immediately, with the sink it writes to. It may
 * return a cleanup function, and it may be `async` — a rejection ends the
 * stream with that error rather than becoming an unhandled rejection, which is
 * the whole reason it is awaited at all.
 *
 * # The reader that cannot keep up
 *
 * `buffer` is the stream's high-water mark, so `desiredSize` is that number
 * minus what is queued: it reaches zero when a reader is `buffer` events
 * behind and `-buffer` when it is twice that. The second is the ceiling, and
 * past it the connection is closed with an error rather than held — the
 * alternative is a queue whose producer is an application and whose consumer
 * is a phone on a train, which is the unbounded thing `docs/security.md` rule
 * 4 is about. A source that would rather shed events than lose the reader can
 * watch `sink.signal` and stop sending.
 */
export function eventStream(source: EventStreamSource, options?: EventStreamOptions): Response {
  const capabilities = currentContext()?.capabilities ?? null;
  refuseWhereItCannotArrive(capabilities);

  const buffer = Math.max(1, options?.buffer ?? DEFAULT_BUFFER);
  const heartbeat = options?.heartbeat ?? DEFAULT_HEARTBEAT;
  const encoder = new TextEncoder();
  const ended = new AbortController();

  let controller: StreamController | null = null;
  let timer: IntervalID | null = null;
  let cleanup: (() => mixed) | null = null;

  /** End everything exactly once, whichever of the three reasons got here. */
  const finish = (error: Error | null) => {
    if (ended.signal.aborted) {
      return;
    }
    ended.abort();
    if (timer != null) {
      clearInterval(timer);
      timer = null;
    }
    const open = controller;
    controller = null;
    if (open != null) {
      // A stream already closed by the reader throws from both of these, and
      // there is nothing left to report it to: the response is over.
      try {
        if (error == null) {
          open.close();
        } else {
          open.error(error);
        }
      } catch {
        // The reader is gone. That is the case this branch exists for.
      }
    }
    const run = cleanup;
    cleanup = null;
    if (run != null) {
      try {
        run();
      } catch (failure) {
        reportSourceFailure(failure);
      }
    }
  };

  const write = (text: string) => {
    const open = controller;
    if (open == null) {
      return;
    }
    // Written before the check, so an event that arrives exactly at the
    // ceiling is delivered rather than being the one that trips it: the reader
    // is behind, not gone, and dropping the message it was waiting for is a
    // worse answer than one more chunk.
    open.enqueue(encoder.encode(text));
    const room = open.desiredSize;
    if (room != null && room <= -buffer) {
      finish(
        new Error(
          `@uniflowed/server: an event stream reader fell ${2 * buffer} events behind and was ` +
            "disconnected. Raise `buffer`, or send less.",
        ),
      );
    }
  };

  const sink: EventSink = {
    send: (event) => write(encodeEvent(typeof event === "string" ? { data: event } : event)),
    comment: (text) => write(encodeComment(text)),
    close: () => finish(null),
    signal: ended.signal,
  };

  const stream = new ReadableStream(
    {
      start(given: StreamController) {
        controller = given;
        if (options?.retry != null) {
          write(encodeRetry(options.retry));
        }
        if (heartbeat > 0) {
          timer = setInterval(() => sink.comment("uf"), heartbeat);
          // A heartbeat must not be the reason a process cannot exit. Node has
          // `unref`; a worker runtime has neither the method nor the problem,
          // because nothing there is waiting to exit.
          (timer as $FlowFixMe)?.unref?.();
        }
        void beginSource(source, sink, finish, (release) => {
          cleanup = release;
        });
      },
      cancel(): void {
        // The client hung up. Everything the source is holding open — a
        // database subscription, an interval, an upstream request — stops
        // here, which is the difference between this and dropping the stream.
        finish(null);
      },
    },
    // The count strategy spelled out rather than reached for as
    // `CountQueuingStrategy`, which is a global three of the four target
    // runtimes have and none of them needs: one event is one, which is what a
    // queue of events wants and what the class would have said.
    { highWaterMark: buffer, size: () => 1 },
  );

  return new Response(stream, {
    status: 200,
    headers: {
      ...(options?.headers ?? {}),
      "content-type": "text/event-stream; charset=utf-8",
      // A stream held by a shared cache is the first few events replayed to
      // whoever asks next, forever. `no-transform` is the other half: a proxy
      // that gzips this decides on its own when to flush.
      "cache-control": "no-store, no-transform",
      connection: "keep-alive",
      // nginx and several CDNs buffer a proxied response by default, which
      // turns an event stream into one delivery when it ends. This is the
      // header they all agree to read.
      "x-accel-buffering": "no",
    },
  });
}

/**
 * One event, as the bytes that go on the wire.
 *
 * Exported because the encoding is the part with the bugs in it and the suite
 * should be able to drive it without a socket — the same reason
 * `@uniflowed/server/host` exports the pieces of `beginRequest`.
 *
 * A payload's newlines become further `data:` lines, which is what the format
 * says and what nobody writes by hand. `\r\n` and a bare `\r` are normalised
 * first: a reader splits on any of the three, so leaving them in would make
 * one message into two on some clients and not others.
 */
export function encodeEvent(event: ServerSentEvent): string {
  let out = "";
  if (event.id != null) {
    // The format has no escape, so a field that cannot be written is refused
    // rather than truncated at the newline — a truncated `id:` is a cursor
    // that silently means something else on reconnect.
    out += `id: ${single("id", event.id)}\n`;
  }
  if (event.event != null) {
    out += `event: ${single("event", event.event)}\n`;
  }
  if (event.retry != null) {
    out += encodeRetry(event.retry);
  }
  for (const line of event.data.replace(/\r\n?/g, "\n").split("\n")) {
    out += `data: ${line}\n`;
  }
  return `${out}\n`;
}

/** A comment line, which a reader ignores and a proxy counts as traffic. */
function encodeComment(text: string): string {
  return `: ${text.replace(/[\r\n]+/g, " ")}\n\n`;
}

/** The reconnection delay, which the format writes as its own field. */
function encodeRetry(millis: number): string {
  if (!Number.isFinite(millis) || millis < 0) {
    throw new TypeError(`@uniflowed/server: retry must be a non-negative number, not ${millis}`);
  }
  return `retry: ${Math.floor(millis)}\n`;
}

/**
 * A field value that has to be one line, or a named failure.
 *
 * A newline ends the field, and a NUL makes a reader discard an `id:` outright
 * — the specification says so about that field in particular, which is the one
 * where being quietly dropped means the next reconnect resumes from the wrong
 * place. Refused rather than stripped: a cursor with a character removed is
 * still a cursor, pointing somewhere nobody chose.
 */
function single(field: string, value: string): string {
  if (/[\r\n\0]/.test(value)) {
    throw new TypeError(
      `@uniflowed/server: an event's ${field} may not contain a newline or a NUL, and this one ` +
        "does. The format has no escape for either, so there is no correct way to send it.",
    );
  }
  return value;
}

/**
 * Start the source and keep hold of whatever it wants cleaned up.
 *
 * Separate from `eventStream` because it is the one part that is asynchronous,
 * and inlining it put a `.then` in the middle of `start` where a rejection had
 * nowhere to go. A source that rejects ends the stream with that error, which
 * is the only place the client can be told anything went wrong at all.
 */
async function beginSource(
  source: EventStreamSource,
  sink: EventSink,
  finish: (error: Error) => void,
  keep: (release: () => mixed) => void,
): Promise<void> {
  try {
    const release = await source(sink);
    if (typeof release !== "function") {
      return;
    }
    const cleanup = release as $FlowFixMe;
    if (sink.signal.aborted) {
      // The stream ended while the source was still starting. Nothing else
      // will run this, so it runs here.
      cleanup();
      return;
    }
    keep(cleanup);
  } catch (error) {
    // A source may throw anything; a reader may only be handed an `Error`. The
    // original's message survives inside the one that is built, which is what
    // a person reading the failure needs — the identity of a thrown string is
    // not.
    finish(error instanceof Error ? error : new Error(String(error)));
  }
}

/** Refuse a target that would hold every event until the stream ended. */
function refuseWhereItCannotArrive(capabilities: ServerCapabilities | null): void {
  if (capabilities == null || capabilities.stream) {
    return;
  }
  throw new CapabilityUnavailableError(
    "streaming response",
    capabilities.target,
    "It reads a response body to the end before answering, so an event stream would be held " +
      "in memory until it closed and delivered as one block — for a stream that stays open, " +
      "that is a timeout. Serve this route from a target that streams.",
  );
}

/**
 * Report a cleanup that threw.
 *
 * The console, for the reason `drainDeferred` gives: this runs after the
 * response is decided, so there is no longer anywhere else to put it.
 */
function reportSourceFailure(error: mixed): void {
  // eslint-disable-next-line no-console
  console.error("uf: an event stream's cleanup failed", error);
}
