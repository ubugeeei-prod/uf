// @flow
//
// `app.router.redirects`, `rewrites` and `headers`, as every front door reads
// them. `tests/library/deploy.test.js` asks the front doors themselves; this is
// the one reading of the rules they all share.

import { describe, expect, it } from "@uniflowed/test";

import { headersFor, redirectFor, rewriteFor, withHeaders } from "./internal/routing.js";

const at = (url: string, init?: mixed) => new Request(`http://uf.test${url}`, init);

const rules = {
  redirects: [
    { source: "/old-blog/:slug", destination: "/blog/:slug", permanent: true },
    { source: "/docs/:path*", destination: "https://docs.example.com/:path*", permanent: false },
    { source: "/campaign", destination: "/sale?utm_source=mail", permanent: false },
  ],
  rewrites: [
    { source: "/articles/:slug", destination: "/posts/:slug" },
    { source: "/shop/:path*", destination: "/store/:path*" },
  ],
  headers: [
    { source: "/:path*", headers: { "X-Frame-Options": "DENY", "cache-control": "no-cache" } },
    {
      source: "/assets/:file*",
      headers: { "Cache-Control": "public, max-age=31536000, immutable" },
    },
  ],
};

describe("a redirect", () => {
  it("answers a matching source with the destination's parameters and the request's query", () => {
    const moved = redirectFor(rules, at("/old-blog/hello?ref=feed"));
    expect(moved?.status).toBe(308);
    expect(moved?.headers.get("location")).toBe("/blog/hello?ref=feed");
  });

  it("is temporary unless it says otherwise, and may leave the origin with the rest of the path", () => {
    const moved = redirectFor(rules, at("/docs/guide/routing"));
    expect(moved?.status).toBe(307);
    expect(moved?.headers.get("location")).toBe("https://docs.example.com/guide/routing");
    // A catch-all that took nothing takes its segment with it.
    expect(redirectFor(rules, at("/docs"))?.headers.get("location")).toBe(
      "https://docs.example.com/",
    );
  });

  it("keeps the destination's own query and adds what the request had that it does not name", () => {
    const moved = redirectFor(rules, at("/campaign?utm_source=feed&page=2"));
    expect(moved?.headers.get("location")).toBe("/sale?utm_source=mail&page=2");
  });

  it("matches whole segments and nothing else", () => {
    expect(redirectFor(rules, at("/old-blog"))).toBe(null);
    expect(redirectFor(rules, at("/old-blog/a/b"))).toBe(null);
    expect(redirectFor(rules, at("/old-blogs/hello"))).toBe(null);
    expect(redirectFor(rules, at("/"))).toBe(null);
  });

  it("sends a navigating browser to the destination's payload, and another origin as written", () => {
    expect(redirectFor(rules, at("/old-blog/hello/__uf.flight"))?.headers.get("location")).toBe(
      "/blog/hello/__uf.flight",
    );
    expect(redirectFor(rules, at("/docs/a/__uf.flight"))?.headers.get("location")).toBe(
      "https://docs.example.com/a",
    );
  });

  it("is nothing for a bundle that carries no rules", () => {
    expect(redirectFor(undefined, at("/old-blog/hello"))).toBe(null);
    expect(redirectFor({}, at("/old-blog/hello"))).toBe(null);
  });
});

describe("a rewrite", () => {
  it("is the same request at the destination, with its query", async () => {
    const rewritten = rewriteFor(
      rules,
      at("/articles/hello?x=1", { headers: { cookie: "session=1" } }),
    );
    expect(rewritten?.url).toBe("http://uf.test/posts/hello?x=1");
    expect(rewritten?.headers.get("cookie")).toBe("session=1");
  });

  it("keeps the method and hands the body on unread", async () => {
    const rewritten = rewriteFor(rules, at("/articles/hello", { method: "POST", body: "uf" }));
    expect(rewritten?.method).toBe("POST");
    expect(await rewritten?.text()).toBe("uf");
  });

  it("passes a percent-encoded segment through as it arrived", () => {
    expect(rewriteFor(rules, at("/articles/caf%C3%A9"))?.url).toBe(
      "http://uf.test/posts/caf%C3%A9",
    );
  });

  it("rewrites a payload into the destination's payload", () => {
    expect(rewriteFor(rules, at("/articles/hello/__uf.flight"))?.url).toBe(
      "http://uf.test/posts/hello/__uf.flight",
    );
    expect(rewriteFor(rules, at("/shop/__uf.flight"))?.url).toBe(
      "http://uf.test/store/__uf.flight",
    );
  });

  it("is null for a path no source matches", () => {
    expect(rewriteFor(rules, at("/posts/hello"))).toBe(null);
  });
});

describe("response headers", () => {
  it("are every matching rule's, in order, lowercased", () => {
    expect(headersFor(rules, at("/assets/client.js"))).toEqual([
      ["x-frame-options", "DENY"],
      ["cache-control", "no-cache"],
      ["cache-control", "public, max-age=31536000, immutable"],
    ]);
  });

  it("win over the answer's own header, the later rule over the earlier", () => {
    const answered = withHeaders(
      new Response("chunk", { headers: { "cache-control": "private" } }),
      headersFor(rules, at("/assets/client.js")),
    );
    expect(answered.headers.get("cache-control")).toBe("public, max-age=31536000, immutable");
    expect(answered.headers.get("x-frame-options")).toBe("DENY");
  });

  it("reach a response whose headers cannot be changed, by copying it", async () => {
    const answered = withHeaders(
      Response.redirect("http://uf.test/elsewhere", 307),
      headersFor(rules, at("/moved")),
    );
    expect(answered.status).toBe(307);
    expect(answered.headers.get("location")).toBe("http://uf.test/elsewhere");
    expect(answered.headers.get("x-frame-options")).toBe("DENY");
  });

  it("match a payload by its document", () => {
    expect(headersFor(rules, at("/assets/__uf.flight")).length).toBe(3);
  });
});
