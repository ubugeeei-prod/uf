// @flow
//
// `@uniflowed/server/image`: the request-time image endpoint, and every
// refusal it makes.
//
// Two halves. The endpoint is driven with a fetch and a transformer that are
// plain functions, so what is under test is the endpoint's own decisions —
// which URL it asks for, which redirect it follows, which body it reads, what
// it keys the cache on — and nothing depends on a network. The Node fetch is
// then driven against a server this file starts on loopback, which is the
// one place a real socket is the thing being tested: that a name resolving
// to a private address is refused *before* a connection, and that a body is
// cut off while it is still arriving.
//
// Nothing here reaches the public internet. A test that needs a host to be
// public gets one from an injected resolver and is refused before it would
// connect; a test that needs a connection makes it to 127.0.0.1 with
// `allowPrivateAddresses`, which is the switch a test is for.

import { Buffer } from "node:buffer";
import { createServer } from "node:http";

import { describe, expect, it } from "@uniflowed/test";
import type { Application, DocumentAssets } from "@uniflowed/server/fetch";
import { createFetchHandler } from "@uniflowed/server/fetch";
import { beginRequest } from "@uniflowed/server/host";
import type { ImageFetch, ImageTransform, TransformInput } from "@uniflowed/server/image";
import {
  IMAGE_ENDPOINT,
  MAX_SOURCE_BYTES,
  acceptsAvif,
  createImageEndpoint,
  lifetimeOf,
  sniff,
} from "@uniflowed/server/image";
import { nodeImageFetch } from "@uniflowed/server/image/node";

import { isPublicAddress } from "./internal/addresses.js";

const PNG = new Uint8Array([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0]);
const SVG = new TextEncoder().encode('<svg xmlns="http://www.w3.org/2000/svg"></svg>');

/** A fetch that answers from a table, and remembers every URL it was asked. */
function fakeFetch(table: { readonly [url: string]: () => Response }): {|
  readonly fetch: ImageFetch,
  readonly asked: Array<string>,
|} {
  const asked: Array<string> = [];
  return {
    asked,
    fetch: async (url) => {
      asked.push(url.href);
      const answer = table[url.href];
      if (answer == null) return new Response(null, { status: 404 });
      return answer();
    },
  };
}

/** A transformer that records its inputs and answers with a labelled body. */
function fakeTransform(): {|
  readonly transform: ImageTransform,
  readonly calls: Array<TransformInput>,
|} {
  const calls: Array<TransformInput> = [];
  return {
    calls,
    transform: async (input) => {
      calls.push(input);
      return {
        bytes: new TextEncoder().encode(`${input.avif ? "avif" : "png"}@${input.width}`),
        type: input.avif ? "image/avif" : "image/png",
      };
    },
  };
}

function png(cacheControl?: string): () => Response {
  return () => {
    const headers = new Headers({ "content-type": "image/png" });
    if (cacheControl != null) headers.set("cache-control", cacheControl);
    return new Response(PNG, { status: 200, headers });
  };
}

function endpointWith(
  fetch: ImageFetch,
  transform: ImageTransform,
  extra?: {| readonly qualities?: $ReadOnlyArray<number> |},
) {
  return createImageEndpoint({
    remotePatterns: [
      { hostname: "images.example.com", pathname: "/uploads/**" },
      { hostname: "*.cdn.example.com" },
    ],
    widths: [320, 640],
    quality: 75,
    ...extra,
    fetch,
    transform,
  });
}

function ask(source: string, query: string = "w=640&q=75", accept?: string): Request {
  const url = `https://app.example/__uf/image?url=${encodeURIComponent(source)}&${query}`;
  return new Request(url, accept == null ? undefined : { headers: { accept } });
}

async function answered(
  endpoint: (request: Request) => Promise<Response | null>,
  request: Request,
): Promise<Response> {
  const response = await endpoint(request);
  if (response == null) throw new Error("the endpoint declined its own path");
  return response;
}

describe("the allow-list", () => {
  it("refuses a host it does not list, without fetching anything", async () => {
    const { fetch, asked } = fakeFetch({});
    const endpoint = endpointWith(fetch, fakeTransform().transform);
    for (const source of [
      "https://evil.example/uploads/a.png",
      // The apex is not under its own `*.`.
      "https://cdn.example.com/a.png",
      // `*.` is exactly one label.
      "https://a.b.cdn.example.com/a.png",
      // Listed host, unlisted path.
      "https://images.example.com/private/a.png",
      // Listed host, plain HTTP, which the pattern did not say.
      "http://images.example.com/uploads/a.png",
      // Listed host, another port.
      "https://images.example.com:8443/uploads/a.png",
      // Credentials: a URL that reads as one host to a person.
      "https://images.example.com@evil.example/uploads/a.png",
      "https://user:pass@images.example.com/uploads/a.png",
    ]) {
      const response = await answered(endpoint, ask(source));
      expect(response.status).toBe(403);
    }
    expect(asked).toEqual([]);
  });

  it("follows a redirect that stays in the list, and refuses one that leaves it", async () => {
    const { fetch, asked } = fakeFetch({
      "https://images.example.com/uploads/moved.png": () =>
        new Response(null, {
          status: 301,
          headers: { location: "https://x.cdn.example.com/a.png" },
        }),
      "https://x.cdn.example.com/a.png": png(),
      "https://images.example.com/uploads/escape.png": () =>
        new Response(null, {
          status: 302,
          headers: { location: "http://169.254.169.254/latest/meta-data/" },
        }),
      "https://images.example.com/uploads/relative.png": () =>
        new Response(null, { status: 307, headers: { location: "/private/secret.png" } }),
    });
    const { transform, calls } = fakeTransform();
    const endpoint = endpointWith(fetch, transform);

    const followed = await answered(endpoint, ask("https://images.example.com/uploads/moved.png"));
    expect(followed.status).toBe(200);

    const escaped = await answered(endpoint, ask("https://images.example.com/uploads/escape.png"));
    expect(escaped.status).toBe(403);
    expect(await escaped.text()).toContain("redirected to http://169.254.169.254/");

    // Resolved against the URL that sent it, and then it is a path the
    // pattern does not list.
    const relative = await answered(
      endpoint,
      ask("https://images.example.com/uploads/relative.png"),
    );
    expect(relative.status).toBe(403);

    expect(asked).not.toContain("http://169.254.169.254/latest/meta-data/");
    expect(asked).not.toContain("https://images.example.com/private/secret.png");
    expect(calls.length).toBe(1);
  });

  it("stops after three redirects", async () => {
    const loop = () =>
      new Response(null, {
        status: 302,
        headers: { location: "https://images.example.com/uploads/loop.png" },
      });
    const { fetch, asked } = fakeFetch({ "https://images.example.com/uploads/loop.png": loop });
    const endpoint = endpointWith(fetch, fakeTransform().transform);
    const response = await answered(endpoint, ask("https://images.example.com/uploads/loop.png"));
    expect(response.status).toBe(502);
    expect(asked.length).toBe(4);
  });

  it("will not be built from an empty list, or from one that admits everything", () => {
    const build = (remotePatterns: $FlowFixMe) => () =>
      createImageEndpoint({
        remotePatterns,
        widths: [640],
        quality: 75,
        fetch: fakeFetch({}).fetch,
        transform: fakeTransform().transform,
      });
    expect(build([])).toThrow("at least one entry");
    expect(build([{ hostname: "*" }])).toThrow("remotePatterns[0]");
    expect(build([{ hostname: "**" }])).toThrow("remotePatterns[0]");
    expect(build([{ hostname: "images.*.com" }])).toThrow("remotePatterns[0]");
    expect(build([{ hostname: "example.com", pathname: "/a*" }])).toThrow("remotePatterns[0]");
  });
});

describe("what a request may ask for", () => {
  it("answers only its own path and declines every other", async () => {
    const endpoint = endpointWith(fakeFetch({}).fetch, fakeTransform().transform);
    expect(await endpoint(new Request("https://app.example/about"))).toBe(null);
    expect(await endpoint(new Request("https://app.example/__uf/image/x"))).toBe(null);
    const post = await answered(
      endpoint,
      new Request(`https://app.example${IMAGE_ENDPOINT}`, { method: "POST" }),
    );
    expect(post.status).toBe(405);
  });

  it("takes only the widths and qualities the project declared", async () => {
    const { fetch, asked } = fakeFetch({ "https://images.example.com/uploads/a.png": png() });
    const endpoint = endpointWith(fetch, fakeTransform().transform, { qualities: [50] });
    const source = "https://images.example.com/uploads/a.png";
    for (const query of ["w=641&q=75", "w=640&q=76", "q=75", "w=640&w=320", "w=6.4e2", "w=-640"]) {
      expect((await answered(endpoint, ask(source, query))).status).toBe(400);
    }
    // Twice is refused rather than resolved.
    const twice = new Request(
      `https://app.example/__uf/image?url=${encodeURIComponent(source)}&url=x&w=640`,
    );
    expect((await answered(endpoint, twice)).status).toBe(400);
    // A relative URL is this origin's own file, which is not this endpoint's.
    expect((await answered(endpoint, ask("/uploads/a.png"))).status).toBe(400);
    expect((await answered(endpoint, ask("file:///etc/passwd"))).status).toBe(400);
    expect(asked).toEqual([]);

    // The declared ones, and `q` defaulting to `quality`.
    expect((await answered(endpoint, ask(source, "w=320&q=50"))).status).toBe(200);
    expect((await answered(endpoint, ask(source, "w=640"))).status).toBe(200);
  });

  it("refuses an SVG and anything else that is not a raster it decodes", async () => {
    const { fetch } = fakeFetch({
      "https://images.example.com/uploads/a.svg": () =>
        new Response(SVG, { status: 200, headers: { "content-type": "image/svg+xml" } }),
      // Labelled a PNG by its server, and an SVG by its bytes.
      "https://images.example.com/uploads/lying.png": () =>
        new Response(SVG, { status: 200, headers: { "content-type": "image/png" } }),
    });
    const { transform, calls } = fakeTransform();
    const endpoint = endpointWith(fetch, transform);
    expect((await answered(endpoint, ask("https://images.example.com/uploads/a.svg"))).status).toBe(
      415,
    );
    expect(
      (await answered(endpoint, ask("https://images.example.com/uploads/lying.png"))).status,
    ).toBe(415);
    expect(calls).toEqual([]);
  });

  it("refuses a source larger than the bound, declared or not", async () => {
    const huge = new Uint8Array(MAX_SOURCE_BYTES + 1);
    huge.set(PNG);
    const { fetch } = fakeFetch({
      "https://images.example.com/uploads/declared.png": () =>
        new Response(null, {
          status: 200,
          headers: { "content-length": String(MAX_SOURCE_BYTES + 1) },
        }),
      // No length, so it is found out while the body arrives.
      "https://images.example.com/uploads/streamed.png": () => {
        let sent = 0;
        return new Response(
          new ReadableStream({
            pull(controller) {
              const chunk = huge.subarray(sent, sent + 1024 * 1024);
              sent += chunk.byteLength;
              if (chunk.byteLength === 0) controller.close();
              else controller.enqueue(chunk);
            },
          }),
          { status: 200 },
        );
      },
    });
    const { transform, calls } = fakeTransform();
    const endpoint = endpointWith(fetch, transform);
    expect(
      (await answered(endpoint, ask("https://images.example.com/uploads/declared.png"))).status,
    ).toBe(413);
    expect(
      (await answered(endpoint, ask("https://images.example.com/uploads/streamed.png"))).status,
    ).toBe(413);
    expect(calls).toEqual([]);
  });
});

describe("the variant", () => {
  it("is AVIF for a browser that accepts it, and the source's family for one that does not", async () => {
    const { fetch } = fakeFetch({ "https://images.example.com/uploads/a.png": png() });
    const { transform, calls } = fakeTransform();
    const endpoint = endpointWith(fetch, transform);
    const source = "https://images.example.com/uploads/a.png";

    const modern = await answered(endpoint, ask(source, "w=320&q=75", "image/avif,image/webp,*/*"));
    expect(modern.status).toBe(200);
    expect(modern.headers.get("content-type")).toBe("image/avif");
    expect(modern.headers.get("vary")).toBe("Accept");
    expect(await modern.text()).toBe("avif@320");

    const older = await answered(endpoint, ask(source, "w=320&q=75", "image/webp,*/*"));
    expect(older.headers.get("content-type")).toBe("image/png");
    expect(calls.map((call) => [call.avif, call.width, call.quality, call.type])).toEqual([
      [true, 320, 75, "image/png"],
      [false, 320, 75, "image/png"],
    ]);
  });

  it("is kept, and the second request for it is answered from the cache", async () => {
    const { fetch, asked } = fakeFetch({ "https://images.example.com/uploads/a.png": png() });
    const { transform, calls } = fakeTransform();
    const endpoint = endpointWith(fetch, transform);
    const source = "https://images.example.com/uploads/a.png";

    const first = await answered(endpoint, ask(source, "w=640&q=75", "image/avif"));
    expect(first.headers.get("x-uf-cache")).toBe("MISS");
    expect(first.headers.get("cache-control")).toBe("public, max-age=3600");
    const second = await answered(endpoint, ask(source, "w=640&q=75", "image/avif"));
    expect(second.headers.get("x-uf-cache")).toBe("HIT");
    expect(await second.text()).toBe("avif@640");

    // Every parameter is in the key: another width, another format and a
    // fragment that never reaches a server.
    expect(
      (await answered(endpoint, ask(source, "w=320&q=75", "image/avif"))).headers.get("x-uf-cache"),
    ).toBe("MISS");
    expect((await answered(endpoint, ask(source, "w=640&q=75"))).headers.get("x-uf-cache")).toBe(
      "MISS",
    );
    expect(
      (await answered(endpoint, ask(`${source}#x`, "w=640&q=75", "image/avif"))).headers.get(
        "x-uf-cache",
      ),
    ).toBe("HIT");

    expect(asked.length).toBe(3);
    expect(calls.length).toBe(3);
  });

  it("is not kept when the origin says not to keep it", async () => {
    const { fetch } = fakeFetch({
      "https://images.example.com/uploads/private.png": png("private, max-age=600"),
    });
    const { transform, calls } = fakeTransform();
    const endpoint = endpointWith(fetch, transform);
    const source = "https://images.example.com/uploads/private.png";
    const first = await answered(endpoint, ask(source));
    expect(first.headers.get("x-uf-cache")).toBe("BYPASS");
    expect(first.headers.get("cache-control")).toBe("private, no-store");
    await answered(endpoint, ask(source));
    expect(calls.length).toBe(2);
  });

  it("carries nothing of the visitor to the origin, and nothing of the origin to the visitor", async () => {
    // The fetch is handed a URL and a signal and nothing else, so a cookie or
    // an `Authorization` on the incoming request has no way to reach the
    // origin — and an origin that cannot tell visitors apart cannot give one
    // of them an answer the cache would then hand to the next.
    const calls: Array<$ReadOnlyArray<mixed>> = [];
    const fetch: ImageFetch = async (...args) => {
      calls.push(args);
      const headers = new Headers({
        "content-type": "text/html",
        "content-disposition": 'attachment; filename="invoice.html"',
        "set-cookie": "session=origin",
      });
      return new Response(PNG, { status: 200, headers });
    };
    const endpoint = endpointWith(fetch, fakeTransform().transform);
    const request = new Request(ask("https://images.example.com/uploads/a.png").url, {
      headers: { cookie: "session=visitor", authorization: "Bearer visitor" },
    });
    const response = await answered(endpoint, request);

    expect(calls.length).toBe(1);
    expect(calls[0].length).toBe(2);
    expect(String(calls[0][0])).toBe("https://images.example.com/uploads/a.png");
    expect(calls[0][1] instanceof AbortSignal).toBe(true);
    expect(response.headers.get("content-type")).toBe("image/png");
    expect(response.headers.get("content-disposition")).toBe(null);
    expect(response.headers.get("set-cookie")).toBe(null);
    expect(response.headers.get("x-content-type-options")).toBe("nosniff");
  });

  it("answers HEAD with the headers and no body", async () => {
    const { fetch } = fakeFetch({ "https://images.example.com/uploads/a.png": png() });
    const endpoint = endpointWith(fetch, fakeTransform().transform);
    const head = await answered(
      endpoint,
      new Request(ask("https://images.example.com/uploads/a.png").url, { method: "HEAD" }),
    );
    expect(head.status).toBe(200);
    expect(head.headers.get("content-type")).toBe("image/png");
    expect(head.body).toBe(null);
  });

  it("reads Accept, the origin's lifetime and the first bytes the way it says", () => {
    expect(acceptsAvif("image/avif,image/webp,*/*")).toBe(true);
    expect(acceptsAvif("image/avif;q=0, image/webp")).toBe(false);
    expect(acceptsAvif("*/*")).toBe(false);
    expect(acceptsAvif(null)).toBe(false);

    expect(lifetimeOf(null)).toBe(3600);
    expect(lifetimeOf("public, max-age=5")).toBe(60);
    expect(lifetimeOf("max-age=86400")).toBe(86400);
    expect(lifetimeOf("max-age=60, s-maxage=7200")).toBe(7200);
    expect(lifetimeOf("max-age=999999999")).toBe(365 * 24 * 60 * 60);
    expect(lifetimeOf("no-store")).toBe(null);

    expect(sniff(PNG)).toBe("image/png");
    expect(sniff(new Uint8Array([0xff, 0xd8, 0xff, 0xe0]))).toBe("image/jpeg");
    expect(sniff(new TextEncoder().encode("RIFF\0\0\0\0WEBPVP8 "))).toBe("image/webp");
    expect(sniff(SVG)).toBe(null);
  });
});

describe("the fetch handler", () => {
  it("answers the image path before the application is asked anything", async () => {
    const untouched = () => {
      throw new Error("the application was asked about an image request");
    };
    const app: Application = {
      runMiddleware: untouched,
      callAction: untouched,
      dispatch: untouched,
      render: untouched,
      beginRequest,
    } as $FlowFixMe;
    const document: DocumentAssets = { scripts: [], styles: [], preloads: [] };
    const { fetch } = fakeFetch({ "https://images.example.com/uploads/a.png": png() });
    const handle = createFetchHandler({
      app,
      document,
      images: endpointWith(fetch, fakeTransform().transform),
    });
    const request = ask("https://images.example.com/uploads/a.png");
    const lifecycle = beginRequest(request);
    const response = await lifecycle.run(() => handle(request));
    await lifecycle.settle();
    expect(response.status).toBe(200);
    expect(response.headers.get("content-type")).toBe("image/png");
  });

  it("is not redirected by a trailing-slash policy, which would move it off its path", async () => {
    const untouched = () => {
      throw new Error("the application was asked about an image request");
    };
    const app: Application = {
      runMiddleware: untouched,
      callAction: untouched,
      dispatch: untouched,
      render: untouched,
      beginRequest,
      routing: { trailingSlash: "always" },
    } as $FlowFixMe;
    const { fetch } = fakeFetch({ "https://images.example.com/uploads/a.png": png() });
    const handle = createFetchHandler({
      app,
      document: { scripts: [], styles: [], preloads: [] },
      images: endpointWith(fetch, fakeTransform().transform),
    });
    const request = ask("https://images.example.com/uploads/a.png");
    const lifecycle = beginRequest(request);
    const response = await lifecycle.run(() => handle(request));
    await lifecycle.settle();
    expect(response.status).toBe(200);
  });
});

describe("the address check", () => {
  it("calls only public unicast addresses public", () => {
    for (const address of [
      "127.0.0.1",
      "127.8.9.10",
      "10.0.0.5",
      "172.16.0.1",
      "172.31.255.255",
      "192.168.1.1",
      "169.254.169.254",
      "100.64.0.1",
      "0.0.0.0",
      "224.0.0.1",
      "255.255.255.255",
      "192.0.2.1",
      "::",
      "::1",
      "fe80::1",
      "fe80::1%en0",
      "fd00::1",
      "fc00::1",
      "ff02::1",
      "::ffff:127.0.0.1",
      "::ffff:7f00:1",
      "::ffff:169.254.169.254",
      "64:ff9b::a9fe:a9fe",
      "2002:7f00:1::1",
      "2001:db8::1",
      "2001::1",
      "not an address",
      "1.2.3",
      "01.2.3.4",
    ]) {
      expect([address, isPublicAddress(address)]).toEqual([address, false]);
    }
    for (const address of [
      "93.184.216.34",
      "8.8.8.8",
      "172.32.0.1",
      "2606:4700::6810:84e5",
      "::ffff:93.184.216.34",
      "2002:5db8:d822::1",
    ]) {
      expect([address, isPublicAddress(address)]).toEqual([address, true]);
    }
  });
});

describe("the Node fetch", () => {
  /** A server on loopback that counts what reaches it. */
  async function origin(answer: (path: string, response: $FlowFixMe) => void): Promise<{|
    readonly port: number,
    readonly hits: () => number,
    readonly close: () => Promise<void>,
  |}> {
    let hits = 0;
    const server = createServer((request, response) => {
      hits += 1;
      answer(request.url ?? "/", response);
    });
    await new Promise<void>((resolve) => {
      server.listen(0, "127.0.0.1", () => resolve());
    });
    const bound: $FlowFixMe = server.address();
    return {
      port: bound.port,
      hits: () => hits,
      close: () =>
        new Promise<void>((resolve) => {
          server.closeAllConnections?.();
          server.close(() => resolve());
        }),
    };
  }

  it("refuses a name that resolves to a private address, before connecting", async () => {
    // No server at all: a connection attempt to this port would be a refused
    // connection, answered `502`, so a `403` is the proof that none was made.
    const port = "9";
    const endpoint = createImageEndpoint({
      remotePatterns: [
        { protocol: "http", hostname: "127.0.0.1", port },
        { protocol: "http", hostname: "localhost", port },
        { protocol: "http", hostname: "images.example.com", port },
        { protocol: "http", hostname: "metadata.example.com", port },
      ],
      widths: [640],
      quality: 75,
      fetch: nodeImageFetch({
        // What a hostile DNS answers: a name the allow-list admitted, pointed
        // somewhere inside. `localhost` goes to the real resolver.
        lookup: async (hostname) => {
          if (hostname === "images.example.com") {
            // One public answer and one private one is still a refusal.
            return [
              { address: "93.184.216.34", family: 4 },
              { address: "::ffff:127.0.0.1", family: 6 },
            ];
          }
          if (hostname === "metadata.example.com") {
            return [{ address: "169.254.169.254", family: 4 }];
          }
          const { lookup } = await import("node:dns/promises");
          return lookup(hostname, { all: true });
        },
      }),
      transform: fakeTransform().transform,
    });
    for (const host of ["127.0.0.1", "localhost", "images.example.com", "metadata.example.com"]) {
      const response = await answered(endpoint, ask(`http://${host}:${port}/a.png`, "w=640"));
      expect([host, response.status]).toEqual([host, 403]);
      expect(await response.text()).toContain("private");
    }
  });

  it("with private addresses allowed, fetches, refuses a redirect out, and cuts off a body over the bound", async () => {
    const served = await origin((path, response) => {
      if (path === "/a.png") {
        response.writeHead(200, { "content-type": "image/png" });
        response.end(Buffer.from(PNG));
      } else if (path === "/out.png") {
        response.writeHead(302, { location: "http://unlisted.invalid/a.png" });
        response.end();
      } else if (path === "/endless.png") {
        // No length, and more than the bound, one megabyte at a time.
        response.writeHead(200, { "content-type": "image/png" });
        const chunk = Buffer.alloc(1024 * 1024);
        let sent = 0;
        const more = () => {
          while (sent <= MAX_SOURCE_BYTES + chunk.length) {
            sent += chunk.length;
            if (!response.write(chunk)) {
              response.once("drain", more);
              return;
            }
          }
          response.end();
        };
        response.write(Buffer.from(PNG));
        more();
      } else {
        response.writeHead(404);
        response.end();
      }
    });
    try {
      const { transform, calls } = fakeTransform();
      const endpoint = createImageEndpoint({
        remotePatterns: [{ protocol: "http", hostname: "127.0.0.1", port: String(served.port) }],
        widths: [640],
        quality: 75,
        fetch: nodeImageFetch({ allowPrivateAddresses: true }),
        transform,
      });
      const base = `http://127.0.0.1:${served.port}`;
      const fetched = await answered(endpoint, ask(`${base}/a.png`, "w=640", "image/avif"));
      expect(fetched.status).toBe(200);
      expect(fetched.headers.get("content-type")).toBe("image/avif");
      expect(Array.from(calls[0].bytes)).toEqual(Array.from(PNG));

      expect((await answered(endpoint, ask(`${base}/out.png`, "w=640"))).status).toBe(403);
      expect((await answered(endpoint, ask(`${base}/endless.png`, "w=640"))).status).toBe(413);
      expect(calls.length).toBe(1);
    } finally {
      await served.close();
    }
  });
});
