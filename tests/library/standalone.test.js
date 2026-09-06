// @flow
//
// `@uniflowed/server/standalone`: what a compiled binary does with a request.
//
// This is the half of `uf build --compile` that has nothing to do with
// compiling. The Rust side proves that one file comes out and that the file
// carries the site; what it cannot prove on a machine that is not allowed to
// bind a socket is that the file answers correctly, and that is what is below.
//
// It drives `createHandler` rather than `serve`, which is why it needs no
// socket at all — the handler is the whole of the request path and the socket
// is only how bytes reach it.
//
// The order the handler resolves things in is the subject of most of these:
// asset, then handler, then prerendered document, then render. Every one of
// those steps can shadow the next, and a shadow in the wrong direction is how
// a route handler stops answering or a page starts being served for `POST`.

import { Buffer } from "node:buffer";

import { describe, expect, it } from "@uniflowed/testing";
import { createHandler } from "@uniflowed/server/standalone";

/** A file as the build embeds it. */
function embed(type: string, contents: string) {
  return { type, body: Buffer.from(contents, "utf8").toString("base64") };
}

/**
 * A build's worth of embedded files: a home page, a nested page, and a chunk.
 */
const assets = {
  "index.html": embed("text/html; charset=utf-8", "<!doctype html><p>prerendered home</p>"),
  "guide/index.html": embed("text/html; charset=utf-8", "<!doctype html><p>prerendered guide</p>"),
  "assets/app-a1b2c3.js": embed("text/javascript; charset=utf-8", "console.log('hydrate')"),
  "brand/logo с пробелом.svg": embed("image/svg+xml", "<svg/>"),
};

const document = { scripts: ["/assets/app-a1b2c3.js"], styles: [], preloads: [] };

/**
 * An application that records what it was asked, so a test can tell "the
 * renderer produced this" from "a file was found".
 */
function application() {
  const asked = { rendered: [], dispatched: [] };
  const app = {
    render: async (url: string) => {
      asked.rendered.push(url);
      return url.startsWith("/nowhere")
        ? { status: 404, html: "<!doctype html><p>not found</p>" }
        : { status: 200, html: `<!doctype html><p>rendered ${url}</p>` };
    },
    dispatch: async (request: Request) => {
      const { pathname } = new URL(request.url);
      asked.dispatched.push(`${request.method} ${pathname}`);
      if (pathname !== "/api/echo") {
        return null;
      }
      return new Response(`handled ${request.method}`, {
        status: 201,
        headers: { "content-type": "text/plain" },
      });
    },
  };
  return { app, asked };
}

/** A Node response that remembers everything written to it. */
function recorder() {
  const chunks = [];
  return {
    statusCode: 0,
    headers: ({}: { [string]: string }),
    setHeader(name: string, value: string) {
      this.headers[name.toLowerCase()] = value;
    },
    write(chunk) {
      chunks.push(Buffer.from(chunk));
    },
    end(chunk) {
      if (chunk != null) {
        chunks.push(Buffer.from(chunk));
      }
    },
    body(): string {
      return Buffer.concat(chunks).toString("utf8");
    },
  };
}

/** One request through a fresh handler, with what the application saw. */
async function request(method: string, url: string) {
  const { app, asked } = application();
  const handle = createHandler({ app, assets, document });
  const response = recorder();
  await handle({ method, url, headers: { host: "example.test" } }, response);
  return { response, asked };
}

describe("embedded files", () => {
  it("serves a chunk out of the binary, with the type the build decided", async () => {
    const { response, asked } = await request("GET", "/assets/app-a1b2c3.js");
    expect(response.statusCode).toBe(200);
    expect(response.headers["content-type"]).toBe("text/javascript; charset=utf-8");
    expect(response.body()).toBe("console.log('hydrate')");
    // The renderer must never have been consulted: a request for a chunk is
    // not a route, and in `uf dev` it never reaches uf's middleware at all.
    expect(asked.rendered).toEqual([]);
  });

  it("sends a length, so a client can keep the connection", async () => {
    const { response } = await request("GET", "/assets/app-a1b2c3.js");
    expect(response.headers["content-length"]).toBe("22");
  });

  it("percent-decodes a path before looking for the file", async () => {
    const { response } = await request(
      "GET",
      "/brand/logo%20%D1%81%20%D0%BF%D1%80%D0%BE%D0%B1%D0%B5%D0%BB%D0%BE%D0%BC.svg",
    );
    expect(response.statusCode).toBe(200);
    expect(response.headers["content-type"]).toBe("image/svg+xml");
  });

  it("renders rather than crashing when a path will not decode", async () => {
    // `%ZZ` is not an escape. A malformed request is a request, not a 500.
    const { response, asked } = await request("GET", "/%ZZ");
    expect(response.statusCode).toBe(200);
    expect(asked.rendered).toEqual(["/%ZZ"]);
  });

  it("cannot be made to find something on Object.prototype", async () => {
    // The lookup is a `Map` for exactly this: with a plain object, a request
    // for `/constructor` finds a function and serves it.
    for (const path of ["/constructor", "/__proto__", "/hasOwnProperty"]) {
      const { response, asked } = await request("GET", path);
      expect(response.statusCode).toBe(200);
      expect(asked.rendered).toEqual([path]);
    }
  });
});

describe("prerendered documents", () => {
  it("serves the home page that `uf build` already wrote", async () => {
    const { response, asked } = await request("GET", "/");
    expect(response.statusCode).toBe(200);
    expect(response.body()).toBe("<!doctype html><p>prerendered home</p>");
    expect(response.headers["cache-control"]).toBe("no-cache");
    expect(asked.rendered).toEqual([]);
  });

  it("finds a nested page with or without the trailing slash", async () => {
    for (const url of ["/guide", "/guide/"]) {
      const { response, asked } = await request("GET", url);
      expect(response.body()).toBe("<!doctype html><p>prerendered guide</p>");
      expect(asked.rendered).toEqual([]);
    }
  });

  it("renders a route that was not prerendered", async () => {
    const { response, asked } = await request("GET", "/guide/dynamic?q=1");
    expect(response.statusCode).toBe(200);
    expect(response.body()).toBe("<!doctype html><p>rendered /guide/dynamic?q=1</p>");
    // The query string reaches the renderer: a page that reads it would
    // otherwise render the wrong thing for every search on the site.
    expect(asked.rendered).toEqual(["/guide/dynamic?q=1"]);
  });

  it("answers an unrouted path with the renderer's own status", async () => {
    const { response } = await request("GET", "/nowhere/at/all");
    expect(response.statusCode).toBe(404);
  });
});

describe("route handlers", () => {
  it("answers a GET the handler claims", async () => {
    const { response } = await request("GET", "/api/echo");
    expect(response.statusCode).toBe(201);
    expect(response.body()).toBe("handled GET");
  });

  it("answers a POST, which no page ever can", async () => {
    const { response, asked } = await request("POST", "/api/echo");
    expect(response.statusCode).toBe(201);
    expect(response.body()).toBe("handled POST");
    expect(asked.rendered).toEqual([]);
  });

  it("offers every request to the dispatcher before rendering", async () => {
    const { asked } = await request("GET", "/guide/dynamic");
    expect(asked.dispatched).toEqual(["GET /guide/dynamic"]);
  });

  it("refuses a POST to a page instead of rendering one", async () => {
    // A page cannot answer a POST. Rendering one would turn a missing handler
    // into a 200 where the caller expected to be told the method was wrong.
    const { response, asked } = await request("POST", "/guide/");
    expect(response.statusCode).toBe(405);
    // Required by the specification on every 405, and computable here because
    // a page has exactly these two methods.
    expect(response.headers["allow"]).toBe("GET, HEAD");
    expect(asked.rendered).toEqual([]);
  });
});

describe("HEAD", () => {
  it("answers an embedded file with its headers and no body", async () => {
    const { response } = await request("HEAD", "/assets/app-a1b2c3.js");
    expect(response.statusCode).toBe(200);
    expect(response.headers["content-length"]).toBe("22");
    expect(response.body()).toBe("");
  });

  it("answers a rendered page with its headers and no body", async () => {
    const { response } = await request("HEAD", "/guide/dynamic");
    expect(response.statusCode).toBe(200);
    expect(response.body()).toBe("");
  });
});
