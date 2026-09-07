// @flow
//
// The channel a browser reports on, and what `uf dev` does with what arrives.
//
// Two endpoints under `/__uf/`, one channel out: a diagnostic a browser-side
// runtime produced (`@uniflowed/router`'s hydration report, through its
// `reportDiagnostic`) and the five numbers `@uniflowed/web/vitals` measures
// both become the same `diagnostic` event on the driver's control channel, and
// the Rust side renders both the way it renders a type error. See
// ubugeeei-prod/uf#557 and #583.
//
// Everything here runs without a socket, which is the point of testing it here
// rather than only through `crates/uf_cli/tests/vite.rs`: the middleware is a
// function of a request, and the two things worth being sure of — that a
// report is turned into the right severity, and that a page cannot make the
// dev server read an unbounded body — do not need a port to check.
//
// What does need one is that the dev server actually mounts this, which is
// `dev_answers_the_fixture_the_way_a_build_does` in `vite.rs`.

import { describe, expect, it } from "@uniflowed/test";
import { VITALS_ENDPOINT } from "@uniflowed/web";

// The router's own, and not a package export: `internal/` is where the half of
// `@uniflowed/router` that only exists under `uf dev` lives, and this is that
// half's way out to the terminal. Its one caller — the hydration report — is
// tested against it in `hydration.test.js`, which has a document to hydrate
// into; what is tested here is the poster and the endpoint it agrees on.
import {
  DIAGNOSTIC_ENDPOINT,
  reportDiagnostic,
} from "../../packages/router/internal/diagnostics.js";

// Not a package export, deliberately, for the reason `serve.test.js` gives
// about `internal/serve.js`: this is the dev server's own wiring rather than an
// interface anything outside `@uniflowed/vite` is invited to use.
import {
  DIAGNOSTIC_ENDPOINT as SERVED_DIAGNOSTIC_ENDPOINT,
  MAX_BODY_BYTES,
  VITALS_ENDPOINT as SERVED_VITALS_ENDPOINT,
  browserDiagnostic,
  createChannelMiddleware,
  vitalsDiagnostic,
} from "../../packages/vite/internal/diagnostics.js";

/** A diagnostic as the channel carries it, which is what `emit` is handed. */
type Reported = {
  readonly severity: string,
  readonly message: string,
  readonly origin?: string,
  readonly detail?: $ReadOnlyArray<string>,
  readonly file?: string,
  readonly line?: number,
  readonly column?: number,
};

/** Node's response, reduced to the three things this middleware writes. */
type FakeResponse = {
  statusCode: number,
  headers: { [string]: string },
  ended: boolean,
  setHeader(name: string, value: string): void,
  end(): void,
};

function outgoing(): FakeResponse {
  return {
    statusCode: 0,
    headers: {},
    ended: false,
    setHeader(name: string, value: string) {
      this.headers[name] = value;
    },
    end() {
      this.ended = true;
    },
  };
}

/**
 * Node's request, reduced to what the middleware reads: a method, a URL and a
 * body it can iterate.
 *
 * An `IncomingMessage` is an async iterable of `Buffer`s and the middleware
 * consumes it as one, which is the whole of the transport contract — so this
 * is the transport, not a stand-in for it.
 */
function incoming(method: string, url: string, body?: string) {
  const chunks: Array<Buffer> = body == null ? [] : [Buffer.from(body, "utf8")];
  return {
    method,
    url,
    async *[Symbol.asyncIterator](): AsyncGenerator<Buffer, void, void> {
      for (const chunk of chunks) {
        yield chunk;
      }
    },
  };
}

/** Drive the middleware once, and report everything it did. */
async function ask(
  method: string,
  url: string,
  body?: string,
): Promise<{
  readonly response: FakeResponse,
  readonly reported: Array<Reported>,
  readonly passedOn: boolean,
}> {
  const reported: Array<Reported> = [];
  const channel = createChannelMiddleware((diagnostic: Reported) => {
    reported.push(diagnostic);
  });
  const response = outgoing();
  let passedOn = false;
  await channel(incoming(method, url, body), response, () => {
    passedOn = true;
  });
  return { response, reported, passedOn };
}

describe("the endpoints the browser posts to", () => {
  // The two constants are written out in three packages — the client halves in
  // `@uniflowed/router` and `@uniflowed/web`, the server half in
  // `@uniflowed/vite` — because the server half is loaded by Vite before any
  // Flow transform exists and cannot import either of the others, and because
  // `tools/ci/publishable.sh` will not let a published package reach for an
  // unpublished one to share them. A duplicated constant with a test on it is
  // honest; one without is how a browser ends up posting to a path nothing
  // serves.
  it("are the same strings on both halves", () => {
    expect(SERVED_DIAGNOSTIC_ENDPOINT).toBe(DIAGNOSTIC_ENDPOINT);
    expect(SERVED_VITALS_ENDPOINT).toBe(VITALS_ENDPOINT);
  });

  it("are under the prefix no application route can reach", () => {
    // A directory in `app/` whose name begins with `_` is not a route, which is
    // what makes this prefix safe as a default destination.
    expect(DIAGNOSTIC_ENDPOINT.startsWith("/__uf/")).toBe(true);
    expect(VITALS_ENDPOINT.startsWith("/__uf/")).toBe(true);
  });
});

describe("a diagnostic the browser produced", () => {
  it("is answered with no content and reported once", async () => {
    const { response, reported, passedOn } = await ask(
      "POST",
      DIAGNOSTIC_ENDPOINT,
      JSON.stringify({
        severity: "error",
        message: "Hydration mismatch in <Posted>",
        detail: ["server   3 minutes ago", "client   5 minutes ago"],
        url: "http://127.0.0.1:5173/posts/hello",
      }),
    );

    expect(response.statusCode).toBe(204);
    expect(passedOn).toBe(false);
    expect(reported).toEqual([
      {
        severity: "error",
        message: "Hydration mismatch in <Posted>",
        origin: "http://127.0.0.1:5173/posts/hello",
        detail: ["server   3 minutes ago", "client   5 minutes ago"],
      },
    ]);
  });

  it("keeps a source position only when the whole of one is there", () => {
    // Two thirds of a position is worse than none: the terminal draws a code
    // frame when it has a file and a line, and a frame pointing at line 1 of a
    // file the report never named would send the reader somewhere else.
    const positioned = browserDiagnostic({
      severity: "warn",
      message: "a slow effect",
      file: "/app/posts/page.js",
      line: 12,
      column: 4,
    });
    expect(positioned).toEqual({
      severity: "warn",
      message: "a slow effect",
      file: "/app/posts/page.js",
      line: 12,
      column: 4,
    });

    expect(browserDiagnostic({ message: "no line", file: "/app/x.js" })).toEqual({
      severity: "error",
      message: "no line",
    });
  });

  it("refuses a report with nothing to say", () => {
    // Refused rather than guessed at: a diagnostic assembled out of a
    // malformed report is a line in somebody's terminal that describes nothing.
    expect(browserDiagnostic(null)).toBe(null);
    expect(browserDiagnostic({ severity: "error" })).toBe(null);
    expect(browserDiagnostic({ message: "   " })).toBe(null);
    expect(browserDiagnostic(["a message"])).toBe(null);
  });

  it("bounds what a page can make the terminal print", () => {
    // Every value here is page-authored — a hydration mismatch on a page whose
    // difference is in somebody's comment carries that comment into this
    // terminal — so a report with a thousand lines in it must not scroll the
    // reason for it off the screen. "No unbounded anything" applies to a dev
    // server reading a request as much as to a production one.
    const long = Array.from({ length: 200 }, (_, at) => `line ${at}`);
    const diagnostic = browserDiagnostic({ message: "x".repeat(2000), detail: long });
    expect(diagnostic?.message.length).toBe(400);
    expect(diagnostic?.detail?.length).toBe(41);
    expect(diagnostic?.detail?.[40]).toBe("…");
  });

  it("cannot draw on the terminal it is printed in", () => {
    // What is rendered was written by a page, and `uf` owns this terminal. A
    // page that could put `ESC [` into a diagnostic could move the cursor,
    // recolour the rest of the session or overwrite the line above its own
    // report — so every control character becomes a space before it goes
    // anywhere near a renderer.
    const diagnostic = browserDiagnostic({
      message: "\u001b[2Ktaken over",
      detail: ["first\u0007", "second\u001b[31m"],
    });
    expect(diagnostic?.message).toBe("[2Ktaken over");
    expect(diagnostic?.detail).toEqual(["first ", "second [31m"]);
  });

  it("takes an unknown severity as an error rather than dropping the report", () => {
    // The severity decides how loudly it reads and nothing else. A report with
    // a word nobody recognises is still a report, and the safe reading of
    // "unknown" is the loud one.
    expect(browserDiagnostic({ severity: "disaster", message: "x" })?.severity).toBe("error");
  });
});

describe("a web-vitals report", () => {
  it("reads as the worst rating in it", async () => {
    const { response, reported } = await ask(
      "POST",
      VITALS_ENDPOINT,
      JSON.stringify({
        url: "http://127.0.0.1:5173/",
        vitals: [
          { name: "CLS", value: 0.02, rating: "good", navigationType: "navigate" },
          { name: "LCP", value: 3200.4, rating: "poor", navigationType: "navigate" },
          { name: "FCP", value: 2000, rating: "needs-improvement", navigationType: "navigate" },
        ],
      }),
    );

    expect(response.statusCode).toBe(204);
    expect(reported).toEqual([
      {
        severity: "error",
        message: "web vitals: LCP is poor (3200 ms)",
        origin: "http://127.0.0.1:5173/",
        // Worst first, so the line the reader needs is the one under the
        // headline rather than wherever the browser finished measuring.
        detail: ["LCP 3200 ms — poor", "FCP 2000 ms — needs-improvement", "CLS 0.02 — good"],
      },
    ]);
  });

  it("reads as information when every number is good", () => {
    // Not a warning. A page whose numbers are all good has still reported, and
    // printing that as a warning is how a reader learns to ignore warnings.
    const diagnostic = vitalsDiagnostic({
      url: "http://127.0.0.1:5173/",
      vitals: [
        { name: "TTFB", value: 40, rating: "good" },
        { name: "CLS", value: 0, rating: "good" },
      ],
    });
    expect(diagnostic?.severity).toBe("info");
    expect(diagnostic?.message).toBe("web vitals: TTFB, CLS good");
    // Zero is a measurement here rather than an absence — the browser supports
    // `layout-shift` and nothing shifted — so it is printed as one, and without
    // the millisecond unit that would make a failing 0.24 read as the best
    // number in the report.
    expect(diagnostic?.detail).toEqual(["TTFB 40 ms — good", "CLS 0 — good"]);
  });

  it("bounds the name and the rating, which the page writes too", () => {
    // A beacon written by `@uniflowed/web/vitals` only ever sends the five
    // names and the three ratings; anything at all can post to this path, and
    // a word has no business being longer than a word.
    const diagnostic = vitalsDiagnostic({
      vitals: [{ name: "L".repeat(80), value: 1, rating: "\u001b[31mpoor" }],
    });
    expect(diagnostic?.detail?.[0]).toBe(`${"L".repeat(32)} 1 ms — [31mpoor`);
  });

  it("ignores an entry that is not a measurement", () => {
    expect(
      vitalsDiagnostic({
        vitals: [
          { name: "LCP", value: "fast", rating: "good" },
          { name: "CLS", value: Number.NaN, rating: "good" },
          { name: "INP", value: 90, rating: "good" },
        ],
      })?.detail,
    ).toEqual(["INP 90 ms — good"]);
    expect(vitalsDiagnostic({ vitals: [] })).toBe(null);
    expect(vitalsDiagnostic({ url: "/" })).toBe(null);
  });
});

describe("the channel itself", () => {
  it("leaves every other path to the application", async () => {
    // Mounted above the application middleware, so anything it does not claim
    // has to reach it untouched — including a page whose URL merely looks like
    // one of these.
    for (const path of ["/", "/__uf/hmr", "/__uf/vitals/extra", "/posts/vitals"]) {
      const { passedOn, response } = await ask("POST", path, "{}");
      expect(passedOn).toBe(true);
      expect(response.ended).toBe(false);
    }
  });

  it("recognises the path with a query string on it", async () => {
    // `sendBeacon` sends what it is given and a caller may name an endpoint
    // with a query on it; the path is what decides, and the rest is not this
    // module's business.
    const { response } = await ask(
      "POST",
      `${VITALS_ENDPOINT}?page=1`,
      JSON.stringify({ vitals: [{ name: "INP", value: 90, rating: "good" }] }),
    );
    expect(response.statusCode).toBe(204);
  });

  it("says a GET is the wrong method rather than that the path is missing", async () => {
    // A `GET` on either path is somebody checking whether the dev server has
    // them, and a 404 would say the opposite of the truth.
    const { response, passedOn } = await ask("GET", VITALS_ENDPOINT);
    expect(response.statusCode).toBe(405);
    expect(response.headers.allow).toBe("POST");
    expect(passedOn).toBe(false);
  });

  it("refuses a body larger than the ceiling", async () => {
    // Counted as the chunks arrive rather than read from `content-length`: the
    // header is the sender's claim and the bytes are the fact. A page with a
    // runaway loop in it must not be able to make `uf dev` grow without bound,
    // which is "no unbounded anything" applied to a dev server reading a
    // request.
    const { response, reported } = await ask(
      "POST",
      DIAGNOSTIC_ENDPOINT,
      JSON.stringify({ message: "x", detail: "y".repeat(MAX_BODY_BYTES) }),
    );
    expect(response.statusCode).toBe(413);
    expect(reported).toEqual([]);
  });

  it("refuses a body that is not the shape the path promises", async () => {
    for (const body of ["not json at all", "[]", JSON.stringify({ nothing: true })]) {
      const { response, reported } = await ask("POST", DIAGNOSTIC_ENDPOINT, body);
      expect(response.statusCode).toBe(400);
      expect(reported).toEqual([]);
    }
  });
});

describe("reporting from the browser", () => {
  it("posts the diagnostic to the endpoint on the page's own origin", () => {
    const posted: Array<{ readonly target: string, readonly body: mixed }> = [];
    withWindow(
      {
        document: {},
        location: { href: "http://127.0.0.1:5173/posts/hello" },
        fetch: (target: string, init: { readonly body: mixed, ... }) => {
          posted.push({ target, body: init.body });
          return Promise.resolve(null);
        },
      },
      () => {
        reportDiagnostic({ severity: "error", message: "Hydration mismatch in <Posted>" });
      },
    );

    expect(posted.length).toBe(1);
    expect(posted[0].target).toBe(DIAGNOSTIC_ENDPOINT);
    // A path rather than a URL, so this cannot be pointed at somebody else's
    // server by accident, and the page it came from filled in for the caller.
    expect(JSON.parse(String(posted[0].body))).toEqual({
      severity: "error",
      message: "Hydration mismatch in <Posted>",
      url: "http://127.0.0.1:5173/posts/hello",
    });
  });

  it("does nothing where there is no browser, and never throws", () => {
    // A diagnostic is a thing a person reads, not a thing an application
    // branches on. A reporter that could fail would make every caller wrap it —
    // from a code path that is already handling something that went wrong.
    expect(() => {
      reportDiagnostic({ severity: "error", message: "on a server" });
    }).not.toThrow();

    withWindow({ document: {}, fetch: () => Promise.reject(new Error("offline")) }, () => {
      expect(() => {
        reportDiagnostic({ severity: "info", message: "nobody listening" });
      }).not.toThrow();
    });
  });
});

/** A browser this test installs, reduced to what the reporter reads. */
type StubWindow = {
  readonly document: { ... },
  readonly location?: { readonly href: string },
  readonly fetch: (target: string, init: { readonly body: mixed, ... }) => Promise<mixed>,
};

/**
 * Run `body` with `document` and `window` installed on the global, and put
 * whatever was there back afterwards.
 *
 * `@uniflowed/hmr` reads the document first and the window second, the same
 * reading `@uniflowed/web/vitals` makes: in a browser `globalThis` *is* the
 * window, and in a test process it is not — so a test that wants the browser
 * branch has to install both.
 *
 * `Object.defineProperty` rather than assignment, and one `$FlowFixMe` on the
 * target rather than one per property: Flow types `globalThis` with the
 * browser's own declarations, so installing a stand-in is telling the checker
 * something untrue on purpose, and it should be said once, here, rather than
 * spread over every line that touches it. `web-vitals.test.js`'s `replace` is
 * the same trade for the same reason.
 */
function withWindow(win: StubWindow, body: () => void): void {
  const target: $FlowFixMe = globalThis;
  const previous = ["document", "window"].map((name) => [
    name,
    Object.getOwnPropertyDescriptor(target, name),
  ]);
  Object.defineProperty(target, "document", {
    value: win.document,
    configurable: true,
    writable: true,
  });
  Object.defineProperty(target, "window", { value: win, configurable: true, writable: true });
  try {
    body();
  } finally {
    for (const [name, descriptor] of previous) {
      if (descriptor == null) {
        delete target[name];
      } else {
        Object.defineProperty(target, name, descriptor);
      }
    }
  }
}
