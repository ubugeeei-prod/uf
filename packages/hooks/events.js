// @flow
//
// `@uniflowed/hooks/events`: a stream the server is pushing.
//
// One hook, and it is here rather than in `./channels.js` for a reason that
// module's header states from the other side: what belongs there is a value
// crossing the page's boundary that is *not* the server, because a request is
// `@uniflowed/query`'s and `@uniflowed/fetch`'s and comes with caching, retry
// and revalidation that a broadcast does not want. An event stream is the
// server and has none of that either — there is no cache entry, nothing to
// revalidate, and no request to repeat. It is a connection, which is a third
// thing, so it is a third file.
//
// `@uniflowed/server/events` is the other end of it.
//
// # What the platform already does, and the one thing it does not
//
// `EventSource` reconnects. It backs off, it sends the last `id:` it saw back
// as `Last-Event-ID`, and it needs no help — for as long as the server keeps
// answering. There is exactly one case where it gives up permanently: a
// response that is not a `200` with `content-type: text/event-stream`. Then it
// fires `error`, moves to `CLOSED`, and never tries again. A deployment
// restarting behind a load balancer answers `502` for a second or two, and
// every open stream in every tab is dead until somebody reloads the page.
//
// That is the case this hook exists for. When the browser gives up, it opens a
// new `EventSource` on a doubling delay; when the browser is retrying on its
// own it stays out of the way, because two reconnection schedules on one
// connection is worse than either.
//
// The other half is that none of the platform's reconnecting is visible. An
// application that wants to draw "reconnecting…" cannot ask `EventSource`
// anything but `readyState`, and only from an event handler. [`status`] is that
// as state.
//
// # What a reconnection this hook makes cannot carry
//
// `Last-Event-ID`. The browser sends it on its *own* retry and there is no way
// to set it on a new `EventSource` — the constructor takes a URL and one flag.
// So [`lastEventId`] is handed back, and a stream that needs to resume puts it
// in the URL itself:
//
//   const stream = useEventSource(`/api/feed?after=${cursor ?? ""}`);
//
// Written down rather than worked around: a hook that silently appended a query
// parameter would be inventing a protocol on the application's behalf.
//
// # Before hydration
//
// `status` is `"idle"` on the server and on the client's first render, through
// `useSupported` — see the reason there. Nothing is opened during a render; the
// connection is made in an effect, so a prerender neither connects nor pretends
// it did. `"idle"` rather than `"connecting"` because they are different facts
// and only one of them is true: nothing has been attempted here.

import { useCallback, useEffect, useMemo, useRef, useState } from "@uniflowed/react";

import { useSupported } from "./browser.js";
import { useStableCallback } from "./lifecycle.js";

/**
 * The part of `EventSource` this module uses.
 *
 * Declared here rather than taken from Flow's library definition, for the
 * reason `./channels.js` gives about `BroadcastChannel`: the constructor is
 * looked up at runtime and may be absent, so it has to be a value with a known
 * constructor signature, and the listener is typed with the two fields that are
 * read — which removes an `instanceof MessageEvent` whose name is not defined
 * in every host these tests run in.
 */
type StreamEvent = { data?: mixed, lastEventId?: mixed, ... };

declare class Stream {
  constructor(url: string, options?: { withCredentials?: boolean, ... }): void;
  readonly readyState: number;
  close(): void;
  addEventListener(type: string, listener: (event: StreamEvent) => mixed): void;
  removeEventListener(type: string, listener: (event: StreamEvent) => mixed): void;
}

/**
 * `EventSource`, which is the host's rather than the document's.
 *
 * The same split `./channels.js` describes: in a browser the two are one
 * object, and in a process where a document has been installed onto another
 * runtime's global they are not — an event stream is a facility of the runtime,
 * and a hosted `Window` is not required to carry one.
 */
declare var EventSource: Class<Stream> | void;

function streamConstructor(): Class<Stream> | null {
  return typeof EventSource === "undefined" ? null : (EventSource ?? null);
}

/** `EventSource.CLOSED`, spelled as the number so no global is read for it. */
const CLOSED = 2;

/** Where the connection is. */
export type EventStreamStatus =
  /** Nothing has been attempted: a prerender, no URL, or `enabled: false`. */
  | "idle"
  /** Opening, or reopening — by the browser or by this hook. */
  | "connecting"
  /** Receiving. */
  | "open"
  /** Closed on purpose, by `close()`. It will not reopen on its own. */
  | "closed";

/** One message, as the three fields the format carries. */
export type ServerEvent = {|
  /** The `event:` name, or `message` where the stream sent none. */
  readonly name: string,
  /** The `data:` payload, as text. */
  readonly data: string,
  /** The `id:`, or `null`. */
  readonly id: string | null,
|};

/** How the connection behaves. */
export type EventSourceOptions = {|
  /**
   * Named events to listen for, besides the unnamed `message`.
   *
   * A stream that sends `event: progress` delivers nothing to a `message`
   * listener, which is the single most common way an event stream appears to
   * be connected and silent.
   */
  readonly events?: $ReadOnlyArray<string>,
  /** Send cookies to another origin. Same-origin streams do not need it. */
  readonly withCredentials?: boolean,
  /** `false` closes the connection and opens none; `null` for the URL does too. */
  readonly enabled?: boolean,
  /** Called for every event, for a stream whose messages are events. */
  readonly onEvent?: (event: ServerEvent) => mixed,
  /** Milliseconds before this hook's first reopen; doubles. Default 1000. */
  readonly retryDelay?: number,
  /** The ceiling the doubling stops at. Default 30000. */
  readonly maxRetryDelay?: number,
|};

/** What `useEventSource` hands back. */
export type UseEventSourceReturn = {|
  readonly status: EventStreamStatus,
  /**
   * The most recent event, or `null` before one arrives.
   *
   * `./channels.js` deliberately does not hand back the last broadcast, and
   * the argument there is that a message is an event: a tab that just opened
   * has never received one and cannot ask. A stream is not that. It is a
   * connection with a lifetime, and for as long as it is open "the latest
   * message" is defined — which is what a progress bar, a price and a presence
   * indicator all are. `onEvent` is there for the streams that really are
   * events.
   */
  readonly last: ServerEvent | null,
  /**
   * The last `id:` seen, for a stream that resumes.
   *
   * The browser sends this itself when *it* reconnects. It is here for the
   * reconnection this hook makes, which cannot; see the module header.
   */
  readonly lastEventId: string | null,
  /** Close it. Nothing reopens until `enabled` or the URL changes. */
  readonly close: () => void,
  /** Whether this runtime has `EventSource` at all. */
  readonly supported: boolean,
|};

const DEFAULT_RETRY = 1_000;
const DEFAULT_MAX_RETRY = 30_000;

/**
 * Subscribe to a server-sent event stream.
 *
 * `url` is re-read when it changes, and a changed URL is a new connection —
 * which is what makes the resume in the module header work at all. Pass `null`
 * to connect to nothing, which is how a stream that depends on something not
 * loaded yet waits without a second hook.
 */
export hook useEventSource(url: string | null, options?: EventSourceOptions): UseEventSourceReturn {
  const supported = useSupported(() => streamConstructor() != null);
  const [status, setStatus] = useState<EventStreamStatus>("idle");
  const [last, setLast] = useState<ServerEvent | null>(null);
  const [lastEventId, setLastEventId] = useState<string | null>(null);
  // Bumped to ask for a new connection, which is the only thing that can
  // reopen one the browser has given up on: `EventSource` in `CLOSED` cannot
  // be restarted, so the effect has to run again with a new object.
  const [attempt, setAttempt] = useState(0);
  const [stopped, setStopped] = useState(false);

  const onEvent = options?.onEvent;
  const stable = useStableCallback((event: ServerEvent) => {
    onEvent?.(event);
  });

  // The names are carried as one string rather than as the array the caller
  // passed, and rebuilt inside the effect from it. An array literal in a
  // render is a new array every time, so depending on one would tear the
  // connection down on every keystroke anywhere in the component — and a ref
  // written during a render is the other way to get this wrong, which this
  // package's header rules out.
  const key = (options?.events ?? []).join(",");
  const withCredentials = options?.withCredentials === true;
  const retryDelay = options?.retryDelay ?? DEFAULT_RETRY;
  const maxRetryDelay = options?.maxRetryDelay ?? DEFAULT_MAX_RETRY;
  // How many times *this hook* has reopened since the last success. Not
  // `attempt`, which also moves for a URL change, and not state, because
  // resetting it on `open` must not be a render.
  const backoff = useRef(0);

  const enabled = (options?.enabled ?? true) && !stopped;

  useEffect(() => {
    const Constructor = streamConstructor();
    if (Constructor == null || url == null || !enabled) {
      setStatus(stopped ? "closed" : "idle");
      return;
    }

    const source = new Constructor(url, { withCredentials });
    setStatus("connecting");
    let reopen: TimeoutID | null = null;

    const onOpen = () => {
      // The delay is reset here rather than on the first message: a stream
      // that connects and sends nothing for an hour has still connected, and
      // treating it as a failure would put the next real outage at the top of
      // the backoff.
      backoff.current = 0;
      setStatus("open");
    };

    const onMessage = (name: string) => (event: StreamEvent) => {
      const sent = event.lastEventId;
      const id = typeof sent === "string" && sent !== "" ? sent : null;
      const received: ServerEvent = { name, data: String(event.data ?? ""), id };
      setLast(received);
      if (id != null) {
        setLastEventId(id);
      }
      stable(received);
    };

    const onError = () => {
      if (source.readyState !== CLOSED) {
        // The browser is retrying on its own, with `Last-Event-ID`, which is
        // strictly better than anything this hook can do. Say so and wait.
        setStatus("connecting");
        return;
      }
      // It has given up — a non-200, or a body that was not an event stream.
      // Nothing reopens this object, so a new one is asked for on a delay.
      setStatus("connecting");
      const wait = Math.min(retryDelay * 2 ** backoff.current, maxRetryDelay);
      backoff.current += 1;
      reopen = setTimeout(() => setAttempt((count) => count + 1), wait);
    };

    const listeners: Array<[string, (event: StreamEvent) => mixed]> = [
      ["message", onMessage("message")],
    ];
    for (const name of key === "" ? [] : key.split(",")) {
      listeners.push([name, onMessage(name)]);
    }

    source.addEventListener("open", onOpen);
    source.addEventListener("error", onError);
    for (const [name, listener] of listeners) {
      source.addEventListener(name, listener);
    }

    return () => {
      if (reopen != null) {
        clearTimeout(reopen);
      }
      source.removeEventListener("open", onOpen);
      source.removeEventListener("error", onError);
      for (const [name, listener] of listeners) {
        source.removeEventListener(name, listener);
      }
      // Closed as well as unsubscribed: an `EventSource` nobody is listening
      // to still holds a connection and still reconnects, so leaving it open
      // is a socket per navigation for the life of the tab.
      source.close();
    };
    // `key` stands in for `options.events`, and `attempt` is what a reopen
    // moves. `backoff` is a ref, and `stable` never changes identity.
  }, [url, enabled, stopped, key, attempt, retryDelay, maxRetryDelay, withCredentials, stable]);

  const close = useCallback(() => {
    setStopped(true);
  }, []);

  return useMemo(
    () => ({ status, last, lastEventId, close, supported }),
    [status, last, lastEventId, close, supported],
  );
}
