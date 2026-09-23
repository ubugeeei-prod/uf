// @flow
//
// Fetching the next route's payload, and when the browser loads a document
// instead.
//
// `internal/flight-browser.js`'s `fetchFlight` is how a page React Server
// Components rendered navigates. Anything it cannot hand React as a payload it
// hands back as a document to load, which is what the anchor would have done —
// and a navigation that did neither would be a click that does nothing. See
// ubugeeei-prod/uf#519.

import { afterEach, describe, expect, it } from "@uniflowed/test";

import { fetchFlight } from "./internal/flight-browser.js";
import { INTERCEPTED_FROM_HEADER } from "./internal/flight.js";

const globals: $FlowFixMe = globalThis;
const saved = { fetch: globals.fetch, window: globals.window };

afterEach(() => {
  globals.fetch = saved.fetch;
  globals.window = saved.window;
});

/** A page at `href`, whose `fetch` answers every request with `answer`. */
function pageAt(
  href: string,
  answer: (input?: mixed, init?: $FlowFixMe) => Promise<Response>,
): void {
  globals.window = { location: new URL(href) };
  globals.fetch = answer;
}

describe("fetching a route's payload", () => {
  it("loads the document when the request cannot be made or followed", async () => {
    // A redirect to another origin — a sign-in page — is one `fetch` may not
    // follow without CORS, and it rejects exactly as a dropped connection does.
    pageAt("http://uf.test/feed", () => Promise.reject(new TypeError("Failed to fetch")));

    expect(await fetchFlight("/account")).toEqual({ kind: "document", url: "/account" });
  });

  it("loads the document the reader asked for when a redirect ends on a page", async () => {
    // A middleware's sign-in page, reached by following a redirect from the
    // payload URL. Loading the document URL lets the middleware see that and
    // name it in its own `next=`, rather than the payload URL.
    const signIn = new Response("<!doctype html><title>sign in</title>", {
      headers: { "content-type": "text/html; charset=utf-8" },
    });
    Object.defineProperty(signIn, "url", {
      value: "http://uf.test/login?next=/account/__uf.flight",
    });
    pageAt("http://uf.test/feed", () => Promise.resolve(signIn));

    expect(await fetchFlight("/account")).toEqual({ kind: "document", url: "/account" });
  });

  it("sends the page an intercepted payload should render over", async () => {
    let headers = new Headers();
    pageAt("http://uf.test/feed", async (_input, init) => {
      headers = new Headers(init?.headers);
      const payload = new Response("", { headers: { "content-type": "text/x-component" } });
      Object.defineProperty(payload, "url", {
        value: "http://uf.test/feed/photo/1/__uf.flight",
      });
      return payload;
    });

    await fetchFlight("/feed/photo/1", { interceptedFrom: "/feed" });

    expect(headers.get(INTERCEPTED_FROM_HEADER)).toBe("/feed");
  });
});

// A static host that does not know `.flight` — the Workers asset server, Pages,
// many others — serves a prerendered payload with no `Content-Type`, or as
// `application/octet-stream`. Refusing it made every client navigation on such
// a host a full page load; the deploy matrix found it under `wrangler dev`
// (ubugeeei-prod/uf#1495). The type check is replaced for those answers by a
// look at the first row, and by nothing weaker.
describe("a payload a static host served untyped", () => {
  /**
   * An answer at `url` with `body` and `headers`, as `fetch` would give it.
   * Bytes rather than a string, because a string body gives a `Response` a
   * `text/plain` type of its own and the case here is a host that sent none.
   */
  function answered(url: string, body: string, init?: $FlowFixMe): Response {
    const response = new Response(new TextEncoder().encode(body), init);
    Object.defineProperty(response, "url", { value: url });
    return response;
  }

  it("is a payload when its first row is a Flight row", async () => {
    for (const type of [null, "application/octet-stream"]) {
      pageAt("http://uf.test/", () =>
        Promise.resolve(
          answered(
            "http://uf.test/posts/first/__uf.flight",
            '0:["$","h1",null,{"children":"post: first"}]\n',
            type == null ? undefined : { headers: { "content-type": type } },
          ),
        ),
      );
      const fetched = await fetchFlight("/posts/first");
      expect(fetched.kind).toBe("flight");
      expect(fetched.url).toBe("/posts/first");
    }
  });

  it("is a document when it is untyped HTML", async () => {
    pageAt("http://uf.test/", () =>
      Promise.resolve(
        answered("http://uf.test/posts/first/__uf.flight", "<!doctype html><p>sign in</p>"),
      ),
    );
    expect(await fetchFlight("/posts/first")).toEqual({ kind: "document", url: "/posts/first" });
  });

  it("is a document when a type says it is something else", async () => {
    // `text/html` is a document whatever its bytes look like.
    pageAt("http://uf.test/", () =>
      Promise.resolve(
        answered("http://uf.test/posts/first/__uf.flight", '0:["$","h1"]\n', {
          headers: { "content-type": "text/html; charset=utf-8" },
        }),
      ),
    );
    expect(await fetchFlight("/posts/first")).toEqual({ kind: "document", url: "/posts/first" });
  });

  it("is a document when it is not a 200", async () => {
    pageAt("http://uf.test/", () =>
      Promise.resolve(
        answered("http://uf.test/posts/first/__uf.flight", '0:["$","h1"]\n', { status: 404 }),
      ),
    );
    expect(await fetchFlight("/posts/first")).toEqual({ kind: "document", url: "/posts/first" });
  });

  it("is a document when it came from another origin", async () => {
    pageAt("http://uf.test/", () =>
      Promise.resolve(answered("http://cdn.example/posts/first/__uf.flight", '0:["$","h1"]\n')),
    );
    expect(await fetchFlight("/posts/first")).toEqual({ kind: "document", url: "/posts/first" });
  });
});
