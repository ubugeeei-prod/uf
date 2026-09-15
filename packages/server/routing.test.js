// @flow
//
// `app.router`'s base path, trailing-slash policy, redirects, rewrites and
// headers, as every front door reads them. `tests/library/deploy.test.js` asks
// the front doors themselves; this is the one reading of the rules they share.

import { describe, expect, it } from "@uniflowed/test";

import { spellPath as routerSpellPath } from "../router/internal/base-path.js";
import {
  admit,
  headersFor,
  rewriteFor,
  spellPath,
  wasAdmitted,
  withHeaders,
} from "./internal/routing.js";

const at = (url: string, init?: mixed) => new Request(`http://uf.test${url}`, init);

/** What `admit` decided, as something an assertion can read in one line. */
function admitted(rules: mixed, url: string, init?: mixed): string {
  // $FlowFixMe[incompatible-call] - the fixtures below are `RoutingRules`.
  const decision = admit(rules, at(url, init));
  if (decision.kind === "answer") {
    const { response } = decision;
    return `${String(response.status)} ${response.headers.get("location") ?? "-"}`;
  }
  return `continue ${decision.request.url}`;
}

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
    expect(admitted(rules, "/old-blog/hello?ref=feed")).toBe("308 /blog/hello?ref=feed");
  });

  it("is temporary unless it says otherwise, and may leave the origin with the rest of the path", () => {
    expect(admitted(rules, "/docs/guide/routing")).toBe(
      "307 https://docs.example.com/guide/routing",
    );
    // A catch-all that took nothing takes its segment with it.
    expect(admitted(rules, "/docs")).toBe("307 https://docs.example.com/");
  });

  it("keeps the destination's own query and adds what the request had that it does not name", () => {
    expect(admitted(rules, "/campaign?utm_source=feed&page=2")).toBe(
      "307 /sale?utm_source=mail&page=2",
    );
  });

  it("matches whole segments and nothing else", () => {
    expect(admitted(rules, "/old-blog")).toBe("continue http://uf.test/old-blog");
    expect(admitted(rules, "/old-blog/a/b")).toBe("continue http://uf.test/old-blog/a/b");
    expect(admitted(rules, "/old-blogs/hello")).toBe("continue http://uf.test/old-blogs/hello");
  });

  it("sends a navigating browser to the destination's payload, and another origin as written", () => {
    expect(admitted(rules, "/old-blog/hello/__uf.flight")).toBe("308 /blog/hello/__uf.flight");
    expect(admitted(rules, "/docs/a/__uf.flight")).toBe("307 https://docs.example.com/a");
  });

  it("is nothing for a bundle that carries no rules", () => {
    expect(admitted(undefined, "/old-blog/hello")).toBe("continue http://uf.test/old-blog/hello");
    expect(admitted({}, "/old-blog/hello")).toBe("continue http://uf.test/old-blog/hello");
  });
});

describe("a base path", () => {
  const based = { basePath: "/docs", redirects: rules.redirects.slice(0, 1) };

  it("is taken off a request inside it, which continues at its application path", () => {
    expect(admitted(based, "/docs/guide?x=1")).toBe("continue http://uf.test/guide?x=1");
    expect(admitted(based, "/docs")).toBe("continue http://uf.test/");
    expect(admitted(based, "/docs/")).toBe("continue http://uf.test/");
  });

  it("answers a request outside it with a 404, and a base is whole segments", () => {
    expect(admitted(based, "/")).toBe("404 -");
    expect(admitted(based, "/guide")).toBe("404 -");
    expect(admitted(based, "/docsx/guide")).toBe("404 -");
  });

  it("is put back in front of a redirect's destination on this application", () => {
    expect(admitted(based, "/docs/old-blog/hello")).toBe("308 /docs/blog/hello");
    expect(admitted(based, "/docs/old-blog/hello/__uf.flight")).toBe(
      "308 /docs/blog/hello/__uf.flight",
    );
  });

  it("is taken off once, however many doors ask", () => {
    const first = admit(based, at("/docs/docs/guide"));
    expect(first.kind === "continue" ? first.request.url : null).toBe("http://uf.test/docs/guide");
    if (first.kind === "continue") {
      expect(wasAdmitted(first.request)).toBe(true);
      const second = admit(based, first.request);
      expect(second.kind === "continue" ? second.request.url : null).toBe(
        "http://uf.test/docs/guide",
      );
    }
  });

  it("matches headers against the application path, and gives a request outside it none", () => {
    const headed = { basePath: "/docs", headers: rules.headers };
    expect(headersFor(headed, at("/docs/assets/client.js")).length).toBe(3);
    expect(headersFor(headed, at("/assets/client.js"))).toEqual([]);
  });
});

describe("a trailing-slash policy", () => {
  it("redirects the other spelling to the one it uses, with the query", () => {
    expect(admitted({ trailingSlash: "never" }, "/guide/?x=1")).toBe("308 /guide?x=1");
    expect(admitted({ trailingSlash: "always" }, "/guide?x=1")).toBe("308 /guide/?x=1");
    expect(admitted({ trailingSlash: "never" }, "/guide")).toBe("continue http://uf.test/guide");
    expect(admitted({ trailingSlash: "always" }, "/guide/")).toBe(
      "continue http://uf.test/guide/",
    );
  });

  it("spells the root of a base path as the base, unless it is always", () => {
    expect(admitted({ basePath: "/docs", trailingSlash: "never" }, "/docs/")).toBe("308 /docs");
    expect(admitted({ basePath: "/docs", trailingSlash: "always" }, "/docs")).toBe("308 /docs/");
    expect(admitted({ trailingSlash: "never" }, "/")).toBe("continue http://uf.test/");
  });

  it("leaves a file, a payload and a request that is not a navigation alone", () => {
    expect(admitted({ trailingSlash: "always" }, "/robots.txt")).toBe(
      "continue http://uf.test/robots.txt",
    );
    expect(admitted({ trailingSlash: "always" }, "/guide/__uf.flight")).toBe(
      "continue http://uf.test/guide/__uf.flight",
    );
    expect(admitted({ trailingSlash: "always" }, "/api/items", { method: "POST", body: "{}" })).toBe(
      "continue http://uf.test/api/items",
    );
  });

  it("spells a redirect's destination so following it is not a second redirect", () => {
    const always = { trailingSlash: "always", redirects: rules.redirects.slice(0, 1) };
    expect(admitted(always, "/old-blog/hello/")).toBe("308 /blog/hello/");
  });

  it("is the spelling the router writes its links with", () => {
    for (const policy of ["never", "always", "ignore"]) {
      for (const underBase of [false, true]) {
        for (const path of ["/", "/guide", "/guide/", "/a/b", "/robots.txt", "/docs/"]) {
          expect(`${policy} ${String(underBase)} ${path}: ${spellPath(path, policy, underBase)}`).toBe(
            `${policy} ${String(underBase)} ${path}: ${routerSpellPath(path, policy, underBase)}`,
          );
        }
      }
    }
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
