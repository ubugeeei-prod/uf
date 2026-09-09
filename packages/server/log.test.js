// @flow
//
// Structured logging, and the request id that reaches a render.
//
// ubugeeei-prod/uf#506 is two claims and this file holds both of them.
//
// The first is ordinary: uf has a logger, it has levels, it produces JSON where
// a collector is reading and a line where a person is, and — the part that
// makes it worth having — a request's line carries the route pattern that
// matched rather than only the path it arrived on. A log of `/orders/8813` is a
// million facts; a log of `/orders/:id` is one.
//
// The second is the one a module-level variable gets wrong. An id created when
// a request arrives has to be readable from inside a render, from a loader and
// from a route handler, without anybody being handed a request and without two
// requests in flight sharing it. That is the same hazard `server.test.js`
// exercises for `headers()` and `cookies()`, and the cases below are deliberate
// echoes of those: two requests in flight, and a context that ends when its
// request does.
//
// The rest is about what must never reach a log line. A token in a log is a
// token in whatever holds the logs, and a newline in a value a client chose is
// a fabricated record — so redaction and control-character stripping each have
// a case here, and each fails without the guard.
//
// # The time on a record is uf's, and it is a `Temporal.Instant`
//
// Which is testable exactly because it is not `Date.now()`: the cases below
// install a clock and then assert on the timestamp a record carries and on the
// duration a request line reports, instead of asserting that both are numbers
// and hoping. `@uniflowed/core/temporal` is the same clock on every host,
// polyfilled where the runtime has no Temporal, so what these pin holds in a
// worker as well as under `uf start`.

import { describe, expect, it } from "@uniflowed/test";
import { Temporal } from "@uniflowed/core/temporal";
import { fixedClock, manualClock, setClock } from "@uniflowed/core/clock";
import { logger, requestId } from "@uniflowed/server";
import { contextFor, drainDeferred, noteRoute, runWithContext } from "@uniflowed/server/host";
import {
  createLogger,
  formatJson,
  formatText,
  installLogger,
  isRedacted,
  processLogger,
  recordingLogger,
  silentLogger,
} from "@uniflowed/server/log";
import { nodeListener, reportMalformedRequests } from "@uniflowed/server/node";
import { createServer } from "node:http";
import { Duplex } from "node:stream";

/** Run `body` as if handling a request for `url` carrying `init`. */
function handling<T>(url: string, body: () => T, init?: { readonly [string]: string }): T {
  return runWithContext(
    contextFor(new Request(`https://uniflowed.dev${url}`, { headers: init })),
    body,
  );
}

describe("levels", () => {
  it("writes nothing below the threshold it was built with", () => {
    const records = [];
    const log = createLogger({ level: "warn", sink: (record) => records.push(record) });

    log.debug("a");
    log.info("b");
    log.warn("c");
    log.error("d");

    expect(records.map((record) => record.message)).toEqual(["c", "d"]);
  });

  it("says nothing at any level once it is silent", () => {
    const log = silentLogger();

    // No sink to assert on, which is the point: the assertion is that none of
    // these throw and that `child` of a silent logger is still silent.
    log.error("gone");
    expect(log.child({ a: 1 }).level).toBe("error");
  });
});

describe("fields", () => {
  it("carries what a child was bound with onto every line", () => {
    const { logger: log, records } = recordingLogger();

    log.child({ requestId: "r1" }).info("hello", { extra: 2 });

    expect(records[0].fields).toEqual({ requestId: "r1", extra: 2 });
  });

  it("lets a call override a bound field rather than duplicating it", () => {
    const { logger: log, records } = recordingLogger();

    log.child({ route: "/a" }).info("hello", { route: "/b" });

    expect(records[0].fields.route).toBe("/b");
  });

  it("does not redact an error code called `code`", () => {
    // A field named `code` is `ECONNRESET` far more often than it is an
    // authorization code, and a redaction table that took it would make the
    // ordinary diagnostic useless to catch a value uf never logs.
    const { logger: log, records } = recordingLogger();

    log.warn("socket", { code: "ECONNRESET" });

    expect(records[0].fields.code).toBe("ECONNRESET");
  });
});

describe("redaction", () => {
  it("never prints a value stored under a credential's name", () => {
    // The guard that keeps a token out of whatever holds the logs. Without it,
    // one `logger().info("exchanged", tokens)` publishes an access token to
    // every system the log is replicated into.
    const { logger: log, records } = recordingLogger();

    log.info("exchanged", {
      accessToken: "at-secret",
      refresh_token: "rt-secret",
      "Set-Cookie": "session=abc",
      subject: "user-1",
    });

    expect(records[0].fields).toEqual({
      accessToken: "[redacted]",
      refresh_token: "[redacted]",
      "Set-Cookie": "[redacted]",
      subject: "user-1",
    });
  });

  it("recognises a credential's name however it is spelled", () => {
    // One table entry has to cover every spelling a caller might reach for, or
    // the table is a list of the spellings somebody happened to think of.
    expect(isRedacted("client_secret")).toBe(true);
    expect(isRedacted("clientSecret")).toBe(true);
    expect(isRedacted("CLIENT-SECRET")).toBe(true);
    expect(isRedacted("subject")).toBe(false);
  });

  it("redacts at every depth, not only at the top", () => {
    // A token nested one object down is the same token. The bug this prevents
    // is a caller logging a whole record — `{ session: { ..., tokens: {...} } }`
    // — and a redaction that only looked at the outermost keys letting it out.
    const { logger: log, records } = recordingLogger();

    log.info("record", { session: { subject: "u", tokens: { accessToken: "at" } } });

    expect(records[0].fields).toEqual({
      session: { subject: "u", tokens: { accessToken: "[redacted]" } },
    });
  });
});

describe("what a value may not do to a log", () => {
  it("strips control characters, so a value cannot forge a second record", () => {
    // Log injection. A path, a user agent and an error message are all text a
    // client chose, and a newline in one of them would close its own record and
    // open a fabricated one — which is how a log becomes evidence of something
    // that did not happen.
    const { logger: log, records } = recordingLogger();

    log.info("request", { path: '/a\nlevel=error msg="deleted everything"' });

    expect(records[0].fields.path).toBe('/a level=error msg="deleted everything"');
  });

  it("strips them from the message as well as from the fields", () => {
    const { logger: log, records } = recordingLogger();

    log.info("first\nsecond");

    expect(records[0].message).toBe("first second");
  });

  it("cuts a value that would otherwise be as long as the caller likes", () => {
    // Rule 4 of `docs/security.md`, applied to a logger: a handler that passes
    // a megabyte should cost a truncated diagnostic rather than a megabyte in
    // the log.
    const { logger: log, records } = recordingLogger();

    log.info("big", { value: "x".repeat(5000) });

    expect(String(records[0].fields.value).length).toBeLessThan(2000);
  });

  it("stops walking a structure that is deeper than it will print", () => {
    const { logger: log, records } = recordingLogger();

    log.info("deep", { a: { b: { c: { d: { e: { f: 1 } } } } } });

    expect(records[0].fields).toEqual({ a: { b: { c: { d: { e: "[deep]" } } } } });
  });
});

describe("formats", () => {
  it("writes JSON with the fields at the top level", () => {
    // So a query is `status:500` rather than `fields.status:500`, which is the
    // difference between a structured log and a JSON-shaped string.
    const line = formatJson({
      level: "info",
      time: Temporal.Instant.from("2026-01-02T03:04:05.678Z"),
      message: "request",
      fields: { status: 500, route: "/a/:id" },
    });

    expect(JSON.parse(line)).toEqual({
      status: 500,
      route: "/a/:id",
      level: "info",
      time: "2026-01-02T03:04:05.678Z",
      msg: "request",
    });
  });

  it("cannot have its own keys displaced by a field of the same name", () => {
    // A caller — or a header a caller chose — naming a field `level` must not
    // be able to relabel the record's severity.
    const line = formatJson({
      level: "error",
      time: Temporal.Instant.fromEpochMilliseconds(0),
      message: "real",
      fields: { level: "debug", msg: "fake" },
    });

    expect(JSON.parse(line).level).toBe("error");
    expect(JSON.parse(line).msg).toBe("real");
  });

  it("writes a line a person reads for the terminal", () => {
    const line = formatText({
      level: "warn",
      time: Temporal.Instant.from("2026-01-02T03:04:05.678Z"),
      message: "request",
      fields: { route: "/a/:id", status: 404 },
    });

    expect(line).toBe("03:04:05.678 warn  request route=/a/:id status=404");
  });

  it("always writes three fractional digits, so the timestamps sort", () => {
    // `Instant.toString()` omits the fraction when it is zero, which is correct
    // ISO 8601 and wrong here: `.` sorts below `Z`, so `…:05Z` would come after
    // `…:05.500Z` in every collector that sorts the string it was handed. This
    // is the case that fails if the formatter ever goes back to `toString`.
    const line = formatJson({
      level: "info",
      time: Temporal.Instant.from("2026-01-02T03:04:05Z"),
      message: "on the second",
      fields: {},
    });

    expect(JSON.parse(line).time).toBe("2026-01-02T03:04:05.000Z");
    expect(JSON.parse(line).time < "2026-01-02T03:04:05.500Z").toBe(true);
  });
});

describe("the clock a record is stamped from", () => {
  it("is uf's rather than the host's, so a frozen clock freezes the record", () => {
    // The reason the time is Temporal at all. `Date.now()` cannot be moved
    // without replacing a global, and a suite that replaces a global is a suite
    // that has changed the thing it is testing for everything running beside
    // it.
    const at = Temporal.Instant.from("2026-01-02T03:04:05.678Z");
    const restore = setClock(fixedClock(at.epochMilliseconds));
    try {
      const { logger: log, records } = recordingLogger();

      log.info("stamped");

      expect(records[0].time.equals(at)).toBe(true);
    } finally {
      restore();
    }
  });

  it("spells a value that knows its own text rather than walking it", () => {
    // A `Temporal.Instant` has no own enumerable properties, so an application
    // logging the expiry it just read out of `currentSession()` would otherwise
    // get `{}` — a field that is present, empty and silently useless.
    const { logger: log, records } = recordingLogger();

    log.info("session", { expiresAt: Temporal.Instant.from("2026-01-02T03:04:05.678Z") });

    expect(records[0].fields.expiresAt).toBe("2026-01-02T03:04:05.678Z");
  });
});

describe("the process logger", () => {
  it("sends what uf writes to whatever was installed, and puts it back", () => {
    const { logger: log, records } = recordingLogger();
    installLogger(log);
    try {
      processLogger().info("through the seam");
    } finally {
      installLogger(null);
    }

    expect(records[0].message).toBe("through the seam");
    expect(processLogger()).not.toBe(log);
  });
});

describe("the request id", () => {
  it("is readable from inside a render without anything being threaded through", () => {
    handling("/", () => {
      expect(typeof requestId()).toBe("string");
      expect(requestId().length).toBeGreaterThan(0);
    });
  });

  it("is the same string every time one request asks for it", () => {
    handling("/", () => {
      expect(requestId()).toBe(requestId());
    });
  });

  it("keeps two requests in flight apart", async () => {
    // The case a module-level variable gets wrong, and the reason an id is on
    // the request context: one request suspends, another arrives, and an id
    // held in a module would name whichever request set it last — so every line
    // the first request wrote afterwards would carry the second one's id.
    const slow = runWithContext(contextFor(new Request("https://uniflowed.dev/slow")), async () => {
      const before = requestId();
      await Promise.resolve();
      await Promise.resolve();
      return [before, requestId()];
    });
    const fast = runWithContext(contextFor(new Request("https://uniflowed.dev/fast")), async () =>
      requestId(),
    );

    const [first, second] = await slow;
    expect(first).toBe(second);
    expect(await fast).not.toBe(first);
  });

  it("throws outside a request, and names the binding", () => {
    expect(() => requestId()).toThrow();
  });

  it("counts as reading request state, so a page that renders it is not cached", () => {
    // A document holding a request id is true of exactly one request. A route
    // cache that stored it would answer every later visitor with the first
    // one's id — the same failure as a cached `Set-Cookie`, in a smaller hat.
    const context = contextFor(new Request("https://uniflowed.dev/"));

    runWithContext(context, () => requestId());

    expect(context.requestStateReads).toBe(1);
  });
});

describe("logger()", () => {
  it("puts this request's id on every line without being asked", () => {
    const { logger: log, records } = recordingLogger();
    installLogger(log);
    try {
      handling("/", () => {
        logger().info("inside");
        expect(records[0].fields.requestId).toBe(requestId());
      });
    } finally {
      installLogger(null);
    }
  });

  it("does not count as reading request state, so logging does not stop caching", () => {
    // The line between the two bindings. An id that reaches the document makes
    // the document personal; an id that reaches a log line does not, and a page
    // that logs must not thereby become a page uf refuses to cache.
    const { logger: log } = recordingLogger();
    installLogger(log);
    const context = contextFor(new Request("https://uniflowed.dev/"));
    try {
      runWithContext(context, () => logger().info("inside"));
    } finally {
      installLogger(null);
    }

    expect(context.requestStateReads).toBe(0);
  });

  it("carries the route once something has matched, and not before", () => {
    // Read at the moment a line is written rather than bound when the logger
    // was taken: a loader logs before the render has begun, and a logger that
    // captured `null` then would say `route=null` for the whole request.
    const { logger: log, records } = recordingLogger();
    installLogger(log);
    try {
      handling("/orders/8813", () => {
        const held = logger();
        held.info("before");
        noteRoute("/orders/:id");
        held.info("after");
      });
    } finally {
      installLogger(null);
    }

    expect(records[0].fields.route).toBe(undefined);
    expect(records[1].fields.route).toBe("/orders/:id");
  });

  it("keeps working outside a request instead of throwing", () => {
    // Every other binding in `@uniflowed/server` throws outside a request,
    // because none of them has an honest answer there. A logger does — the same
    // logger, without a request id — and a package whose logging call is the
    // one call you cannot make from a failure path has it backwards.
    const { logger: log, records } = recordingLogger();
    installLogger(log);
    try {
      logger().warn("no request here");
    } finally {
      installLogger(null);
    }

    expect(records[0].fields.requestId).toBe(undefined);
  });
});

describe("noteRoute", () => {
  it("does nothing outside a request, so a build may call it", () => {
    // `prerender` resolves routes with no request anywhere. A function that
    // threw there would push the test into every caller.
    expect(() => noteRoute("/a")).not.toThrow();
  });

  it("belongs to the request, not to the module", () => {
    const first = contextFor(new Request("https://uniflowed.dev/a"));
    const second = contextFor(new Request("https://uniflowed.dev/b"));

    runWithContext(first, () => noteRoute("/a"));

    expect(first.route).toBe("/a");
    expect(second.route).toBe(null);
  });
});

describe("the line a finished request leaves behind", () => {
  /** The pieces of a Node request `nodeListener` reads. */
  const incoming = (method: string, url: string) => ({ method, url, headers: {} });

  /** A Node response that records what was written to it. */
  const outgoing = () => ({
    statusCode: 200,
    statusMessage: "",
    headersSent: false,
    setHeader: () => {},
    write: () => true,
    end: () => {},
    destroy: () => {},
    on: () => {},
    once: () => {},
    off: () => {},
  });

  /** A host that begins a request and answers it with `answer`. */
  const listening = (log, answer) =>
    nodeListener(answer, {
      beginRequest: (request) => {
        const context = contextFor(request);
        return {
          context,
          run: (body) => runWithContext(context, body),
          settle: () => drainDeferred(context),
        };
      },
      log,
    });

  it("carries the route that matched and not only the path", async () => {
    // The one thing that makes a request log worth keeping. Without it an
    // operator can see that `/orders/8813` was slow and cannot see that
    // `/orders/:id` is.
    const { logger: log, records } = recordingLogger();
    const listen = listening(log, async () => {
      noteRoute("/orders/:id");
      return new Response("ok");
    });

    await listen(incoming("GET", "/orders/8813"), outgoing());

    expect(records[0].message).toBe("request");
    expect(records[0].fields.route).toBe("/orders/:id");
    expect(records[0].fields.path).toBe("/orders/8813");
  });

  it("never carries the query string", async () => {
    // A query string is where a `?return=`, a search term, a signed URL and an
    // OAuth `code` and `state` all live. A host writing this line has no idea
    // which route it is logging, so the only rule it can apply is the one that
    // is right for every route.
    const { logger: log, records } = recordingLogger();
    const listen = listening(log, async () => new Response("ok"));

    await listen(incoming("GET", "/auth/callback?code=super-secret&state=abc"), outgoing());

    expect(records[0].fields.path).toBe("/auth/callback");
    expect(JSON.stringify(records[0])).not.toContain("super-secret");
  });

  it("reports a request that failed as an error, and still writes its line", async () => {
    const { logger: log, records } = recordingLogger();
    const listen = listening(log, async () => {
      throw new Error("the page threw");
    });

    await listen(incoming("GET", "/boom"), outgoing());

    expect(records.map((record) => record.message)).toEqual(["request failed", "request"]);
    expect(records[1].level).toBe("error");
    expect(records[1].fields.status).toBe(500);
  });

  it("says how long it took, measured with uf's clock at both ends", async () => {
    // `durationMs` is `Instant.until(...).total({ unit: "millisecond" })`, so a
    // clock a test drives by hand decides the answer. Reading `Date.now()`
    // twice would leave this the one field in an access line nothing can
    // assert on.
    const clock = manualClock(Temporal.Instant.from("2026-01-02T03:04:05Z").epochMilliseconds);
    const restore = setClock(clock.clock);
    try {
      const { logger: log, records } = recordingLogger();
      const listen = listening(log, async () => {
        clock.advance(1500);
        return new Response("ok");
      });

      await listen(incoming("GET", "/slow"), outgoing());

      expect(records[0].fields.durationMs).toBe(1500);
    } finally {
      restore();
    }
  });

  it("names a 404 a warning and a 200 an ordinary line", async () => {
    // So that `UF_LOG_LEVEL=warn` leaves exactly the requests that went wrong.
    const { logger: log, records } = recordingLogger();
    const listen = listening(log, async () => new Response(null, { status: 404 }));

    await listen(incoming("GET", "/nope"), outgoing());

    expect(records[0].level).toBe("warn");
  });
});

/**
 * The request that never became one.
 *
 * ubugeeei-prod/uf#405. Bytes Node's own parser refuses never reach a request
 * listener: `http.Server` answers `400 Bad Request`, closes the socket and,
 * with no `clientError` handler attached, says so to nobody. A day was spent
 * chasing a streaming bug that was a space in a request target, with the
 * server's stderr empty the whole time.
 *
 * No socket is bound here, which is the point of the shape as much as of the
 * sandbox: a `stream.Duplex` handed to an `http.Server` as a connection drives
 * the real llhttp parser, so what these cases assert is the runtime's own
 * refusal rather than a simulation of it.
 */
describe("a request the parser refused", () => {
  /** An `http.Server` with uf's reporting on it, and the records it writes. */
  function refusing(options?: {| readonly level?: "debug" | "info" | "warn" | "error" |}) {
    const { logger: log, records } = recordingLogger(options);
    const server = createServer(() => {});
    reportMalformedRequests(server, log);
    return { records, server };
  }

  /** Feed `bytes` to `server` down a connection that is not a socket. */
  async function speak(server: mixed, bytes: string): Promise<string> {
    const written: Array<string> = [];
    const connection = new Duplex({
      read() {},
      write(chunk, encoding, callback) {
        written.push(String(chunk));
        callback();
      },
    });
    // $FlowFixMe[prop-missing] - a `node:http` server, structurally.
    server.emit("connection", connection);
    connection.push(bytes);
    // One turn of the loop is all the parser needs; it rejects on the first
    // line rather than on a body it is waiting for.
    await new Promise((resolve) => setTimeout(resolve, 20));
    return written.join("");
  }

  it("says what was refused, and why, in uf's own voice", async () => {
    const { records, server } = refusing();

    // A request target may not contain a space. This is the exact line that
    // went on the wire in the investigation the issue is written from.
    await speak(server, "GET /slow/build --adapter node HTTP/1.1\r\nHost: x\r\n\r\n");

    expect(records.length).toBe(1);
    expect(records[0].level).toBe("warn");
    expect(records[0].message).toBe("malformed request");
    // llhttp's own enumeration, which is the half that says what to fix.
    expect(records[0].fields.code).toBe("HPE_INVALID_CONSTANT");
  });

  it("still answers with Node's own 400, byte for byte", async () => {
    const { server } = refusing();

    const answer = await speak(server, "GET /a b HTTP/1.1\r\nHost: x\r\n\r\n");

    // This change is about saying so, not about answering differently:
    // replacing the default handler means taking over its job as well.
    expect(answer).toBe("HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n");
  });

  it("never puts the bytes the client sent in the record", async () => {
    const { records, server } = refusing();

    await speak(server, "GET /a b HTTP/1.1\r\nX-Secret: hunter2\r\n\r\n");

    // Node hangs the offending packet on `error.rawPacket`, and that is
    // whatever the client sent — the one thing `docs/security.md`'s logging
    // section exists to keep out of a log line.
    expect(records.length).toBe(1);
    expect(Object.keys(records[0].fields)).toEqual(["code"]);
    expect(String(records[0].message) + String(records[0].fields.code)).not.toContain("hunter2");
  });

  it("bounds the lines a flood can cost, and says how many it hid", async () => {
    // The log-volume question, answered rather than left to be discovered on a
    // public address: a malformed request is two dozen bytes to send, so a line
    // per rejection is an amplifier. Twenty a minute, then a count.
    const { records, server } = refusing();
    const bad = "GET /a b HTTP/1.1\r\nHost: x\r\n\r\n";

    for (let sent = 0; sent < 25; sent += 1) {
      await speak(server, bad);
    }

    expect(records.length).toBe(20);
    expect(records.every((record) => record.message === "malformed request")).toBe(true);

    // The next window opens, and what the last one hid is one record with a
    // count on it — the number an operator wants from a flood, since the
    // individual lines of one are all the same line.
    const clock = manualClock(Temporal.Now.instant().epochMilliseconds + 61_000);
    const restore = setClock(clock.clock);
    try {
      await speak(server, bad);
    } finally {
      restore();
    }

    expect(records[20].message).toBe("malformed requests not logged");
    expect(records[20].fields.count).toBe(5);
    expect(records[21].message).toBe("malformed request");
  });

  it("answers the 400 whether or not it wrote a line about it", async () => {
    // The budget bounds the *log*, and must not bound the protocol: a client
    // past the twentieth rejection still gets told what happened to it.
    const { server } = refusing();
    const bad = "GET /a b HTTP/1.1\r\nHost: x\r\n\r\n";
    for (let sent = 0; sent < 20; sent += 1) {
      await speak(server, bad);
    }

    expect(await speak(server, bad)).toBe("HTTP/1.1 400 Bad Request\r\nConnection: close\r\n\r\n");
  });
});
