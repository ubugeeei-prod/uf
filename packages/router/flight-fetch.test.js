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

const globals: $FlowFixMe = globalThis;
const saved = { fetch: globals.fetch, window: globals.window };

afterEach(() => {
  globals.fetch = saved.fetch;
  globals.window = saved.window;
});

/** A page at `href`, whose `fetch` answers every request with `answer`. */
function pageAt(href: string, answer: () => Promise<Response>): void {
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
});
