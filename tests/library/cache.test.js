// @flow
//
// The cache uf did not have, and what it promises.
//
// `rendering.cache` was four booleans that reached `dist/uf-build-manifest.json`
// and nothing else — no store, no revalidation, and no way to tell a cache that
// was off from a cache that was absent. ubugeeei-prod/uf#277 is that, and this
// file is the half of the answer that can be checked without a socket.
//
// It is organised as the store's contract, because every cache bug is one of
// five questions being unstated and `packages/server/internal/cache-store.js`
// states all five: what a key is, what an entry is, when an entry is stale, who
// evicts, and what happens to a request that arrives while an entry is being
// filled. There is a `describe` for each, and then the two things wired to
// them — a rendered document and a request.
//
// # Time is injected, never waited for
//
// Every entry's staleness is a fact about a clock, and a test that establishes
// that fact by sleeping is a test that fails on a loaded machine for reasons
// that have nothing to do with the code. So the store takes `now`, every test
// here owns one, and "sixty seconds later" is an assignment.
//
// # What is deliberately not here
//
// `rendering.cache.data` and `rendering.cache.actions`. Neither is implemented,
// and `crates/uf_config` refuses them by name rather than letting them reach a
// manifest — `refuses_a_cache_switch_uf_does_not_implement` is that assertion,
// and it belongs there because it is a fact about loading a config file.

import fs from "node:fs";
import os from "node:os";
import nodePath from "node:path";
import { pathToFileURL } from "node:url";

import { describe, expect, it } from "@uniflowed/test";

import {
  OutsideCacheScopeError,
  cacheLife,
  cacheTag,
  createCacheStore,
  createCachedFetch,
  decodeCacheValue,
  encodeCacheValue,
  noStore,
  revalidatePath,
  revalidateTag,
} from "@uniflowed/server/cache";
import { createFilesystemCache } from "@uniflowed/server/cache/filesystem";
import { cookies, draftMode, headers } from "@uniflowed/server";
import { createDispatcher } from "@uniflowed/router/handler";
import { createFetchHandler } from "@uniflowed/server/fetch";
import { beginRequest } from "@uniflowed/server/host";

// Not a package export, deliberately — `serve.test.js` says why. It is what
// `uf preview` and `uf start` build their handler with, and it is the only
// place `rendering.cache` becomes a store for those two commands, so it is
// reached by path here for the same reason it is there.
import { createApplicationHandler, providerSpecifier } from "../../packages/vite/internal/serve.js";

const assets = { scripts: ["/assets/client.js"], styles: [], preloads: [] };

const request = (url: string, init?: mixed) => new Request(`http://localhost${url}`, init);

/**
 * The clock a test drives.
 *
 * Milliseconds, because that is what `Date.now` answers and the store compares
 * against; the lifetimes a caller states are seconds, because that is what a
 * person writes. Keeping the two units apart in one place is cheaper than
 * getting the conversion wrong in twenty assertions.
 */
function clock(): {| now: () => number, advance: (seconds: number) => void |} {
  let millis = 1_000_000;
  return {
    now: () => millis,
    advance: (seconds: number) => {
      millis += seconds * 1000;
    },
  };
}

/**
 * Let every microtask that is already queued run.
 *
 * A background refresh is by definition something nobody is awaiting, so a
 * test that wants to see it land has to give the queue a turn. A count of
 * turns rather than a timer: `setTimeout(0)` would make this a test that
 * sleeps, and the whole point of injecting the clock is not to have one.
 */
async function settled(): Promise<void> {
  for (let turn = 0; turn < 20; turn += 1) {
    await Promise.resolve();
  }
}

/** The half of a `ReadableStream` controller these fixtures use. */
type StreamController = {
  readonly enqueue: (chunk: Uint8Array) => mixed,
  readonly close: () => mixed,
  ...
};

/**
 * A server bundle, as `handler.js` imports one.
 *
 * The same shape `deploy.test.js` and `serve.test.js` build, with two
 * additions this file needs: `render` may run something (that is where
 * `cacheLife` and `cookies()` are called from), and `tail` runs while the body
 * is being read — which is where a component inside a `<Suspense>` boundary
 * renders, long after the shell resolved.
 */
function appWith(options: {
  guard?: (request: Request) => Promise<Response | null> | Response | null,
  handler?: (request: Request) => Promise<Response | null> | Response | null,
  render?: (url: string) => mixed,
  tail?: () => mixed,
  status?: number,
  headers?: { [string]: string },
  renders?: Array<string>,
}) {
  const renders = options.renders ?? [];
  return {
    renders,
    beginRequest,
    runMiddleware: async (request: Request) => (options.guard ? options.guard(request) : null),
    // Part of the `Application` contract since server actions landed, and it
    // declines every request here: these tests are about the route cache, and
    // an action call is the one request that never reaches a render. A double
    // that omitted it would be a double of a contract nothing implements —
    // which is what `standalone.js` found when it called this.
    callAction: async (_request: Request) => null,
    dispatch: async (request: Request) => (options.handler ? options.handler(request) : null),
    render: async (url: string) => {
      renders.push(url);
      if (options.render) options.render(url);
      const html = `<!doctype html><p>${url}</p><b>${renders.length}</b>`;
      return {
        status: options.status ?? 200,
        headers: options.headers,
        pipe: (destination: { write: (chunk: string) => mixed, end: () => mixed, ... }) => {
          destination.write(html);
          destination.end();
        },
        stream: () =>
          new ReadableStream({
            start(controller: StreamController) {
              controller.enqueue(new TextEncoder().encode(html));
              if (options.tail) options.tail();
              controller.close();
            },
          }),
      };
    },
  };
}

/**
 * Answer one request the way every host does: begin, run, settle.
 *
 * Spelled out rather than hidden, because `createFetchHandler` deliberately
 * begins no request — it has a `Response` in hand and not a response on the
 * wire — and a fixture that forgot would be testing a handler no host runs.
 */
async function serve(handle, app, url: string, init?: mixed): Promise<Response> {
  const asRequest = request(url, init);
  const { run, settle } = app.beginRequest(asRequest);
  try {
    return await run(() => handle(asRequest));
  } finally {
    await settle();
  }
}

/** A store, a handler over it, and the app underneath, for one test. */
function servingWith(options: mixed, cacheOptions?: {| route?: boolean, fetch?: boolean |}) {
  const time = clock();
  const store = createCacheStore({ now: time.now });
  const app = appWith(options);
  const handle = createFetchHandler({
    app,
    document: assets,
    cache: { store, route: cacheOptions?.route ?? true, fetch: cacheOptions?.fetch ?? false },
  });
  return { app, handle, store, time };
}

/**
 * A durable provider that is a `Map`, so a test can be about the seam.
 *
 * Almost every assertion below is about what the *store* does with a provider —
 * when it reads one, what it writes, what it invalidates — and none of that is
 * a fact about a disk. A `Map` shared between two stores is two processes with
 * one cache between them, which is the arrangement the whole feature is for,
 * expressed in a way a test can drive without a filesystem.
 *
 * `reads` and `writes` are counted because "the second store did not render"
 * and "the second store read the shared entry" are two different claims and
 * only the second one is what a durable cache promises.
 */
function fakeProvider(): $FlowFixMe {
  const entries: Map<string, mixed> = new Map();
  const provider: $FlowFixMe = {
    name: "fake",
    entries,
    reads: 0,
    writes: 0,
    failures: 0,
    /** Set to a message to make every read and write throw. */
    broken: null,
    async read(key: string) {
      provider.reads += 1;
      if (provider.broken != null) throw new Error(provider.broken);
      return entries.get(key) ?? null;
    },
    async write(key: string, entry: mixed) {
      provider.writes += 1;
      if (provider.broken != null) throw new Error(provider.broken);
      entries.set(key, entry);
    },
    async remove(key: string) {
      entries.delete(key);
    },
    async invalidateTag(tag: string) {
      return drop(entries, (entry) => entry.tags.includes(tag));
    },
    async invalidatePath(path: string) {
      return drop(entries, (entry) => entry.path === path);
    },
    async clear() {
      entries.clear();
    },
  };
  return provider;
}

/** Take every entry `matches` describes out of `entries`, counting them. */
function drop(entries: Map<string, $FlowFixMe>, matches: ($FlowFixMe) => boolean): number {
  let dropped = 0;
  for (const [key, entry] of Array.from(entries.entries())) {
    if (matches(entry)) {
      entries.delete(key);
      dropped += 1;
    }
  }
  return dropped;
}

/**
 * A directory that goes away with the test.
 *
 * `mkdtemp` rather than a fixed path under the repository, because two of these
 * tests run at once in different workers and a shared directory would make one
 * of them read the other's entries — which is the very thing being tested, from
 * the wrong direction.
 */
function tempDirectory(): string {
  return fs.mkdtempSync(nodePath.join(os.tmpdir(), "uf-cache-"));
}

/** A store over `provider`, with an injectable clock and a stated build. */
function durableStore(provider: mixed, options?: {| now?: () => number, build?: string |}) {
  return createCacheStore({
    now: options?.now,
    provider: (provider: $FlowFixMe),
    build: options?.build ?? "build-one",
    // Collected rather than printed: several of these tests make a provider
    // fail on purpose, and a suite that prints a stack per deliberate failure
    // teaches its reader to skip the output.
    onError: () => {},
  });
}

describe("the key", () => {
  it("is a list of strings, and refuses anything else", () => {
    const store = createCacheStore();

    // A number in a key is the `1` versus `"1"` trap `@uniflowed/query/key`
    // documents and tells you to avoid. Here it is refused instead, because a
    // server cache's wrong answer is somebody else's data.
    expect(() => store.peek(["user", 1] as $FlowFixMe)).toThrow();
    expect(() => store.peek(["user", "1"])).not.toThrow();
  });

  it("cannot spell one key two ways, or two keys one way", async () => {
    const time = clock();
    const store = createCacheStore({ now: time.now });
    const fill = async (mark: string) => {
      cacheLife({ revalidate: 60 });
      return mark;
    };

    // `["a/b", "c"]` and `["a", "b/c"]` are the collision a join on a
    // separator would produce. They are different entries.
    const first = await store.resolve({ key: ["a/b", "c"] }, () => fill("first"));
    const second = await store.resolve({ key: ["a", "b/c"] }, () => fill("second"));

    expect(first.value).toBe("first");
    expect(second.value).toBe("second");
    expect(store.size()).toBe(2);
  });
});

describe("an entry", () => {
  it("is not stored at all unless the fill states a lifetime", async () => {
    const store = createCacheStore({ now: clock().now });
    let produced = 0;
    const produce = async () => {
      produced += 1;
      return produced;
    };

    const first = await store.resolve({ key: ["silent"] }, produce);
    const second = await store.resolve({ key: ["silent"] }, produce);

    expect(first.outcome).toBe("uncacheable");
    expect(first.stored).toBe(false);
    expect(second.value).toBe(2);
    expect(store.size()).toBe(0);
  });

  it("takes the shortest lifetime it was told, not the last one", async () => {
    const time = clock();
    const store = createCacheStore({ now: time.now });

    await store.resolve({ key: ["page"] }, async () => {
      cacheLife({ revalidate: 3600 });
      cacheLife({ revalidate: 60 });
      cacheLife({ revalidate: 900 });
      return "composed";
    });

    const entry = store.peek(["page"]);
    expect(entry?.revalidateAt).toBe(time.now() + 60_000);
  });

  it("keeps no failure: a fill that threw leaves nothing behind", async () => {
    const store = createCacheStore({ now: clock().now });
    let attempts = 0;

    const failing = async () => {
      attempts += 1;
      cacheLife({ revalidate: 60 });
      throw new Error("the upstream is down");
    };

    let raised = null;
    try {
      await store.resolve({ key: ["flaky"] }, failing);
    } catch (error) {
      raised = error;
    }

    expect(raised).not.toBe(null);
    expect(store.size()).toBe(0);

    // And the next caller tries again rather than being handed the failure.
    const recovered = await store.resolve({ key: ["flaky"] }, async () => {
      cacheLife({ revalidate: 60 });
      return "up again";
    });
    expect(recovered.value).toBe("up again");
    expect(attempts).toBe(1);
  });

  it("refuses a window that ends before it opens", async () => {
    const store = createCacheStore({ now: clock().now });

    let raised = null;
    try {
      await store.resolve({ key: ["backwards"] }, async () => {
        cacheLife({ revalidate: 60, expire: 30 });
        return "impossible";
      });
    } catch (error) {
      raised = error;
    }

    expect(raised instanceof RangeError).toBe(true);
  });
});

describe("when an entry is stale", () => {
  it("is fresh until revalidate, and produced once", async () => {
    const time = clock();
    const store = createCacheStore({ now: time.now });
    let produced = 0;
    const produce = async () => {
      produced += 1;
      cacheLife({ revalidate: 60 });
      return `answer ${produced}`;
    };

    expect((await store.resolve({ key: ["k"] }, produce)).outcome).toBe("miss");
    time.advance(59);
    const second = await store.resolve({ key: ["k"] }, produce);

    expect(second.outcome).toBe("hit");
    expect(second.value).toBe("answer 1");
    expect(produced).toBe(1);
  });

  it("blocks and refills at revalidate when no window was asked for", async () => {
    const time = clock();
    const store = createCacheStore({ now: time.now });
    let produced = 0;
    const produce = async () => {
      produced += 1;
      cacheLife({ revalidate: 60 });
      return `answer ${produced}`;
    };

    await store.resolve({ key: ["k"] }, produce);
    time.advance(60);
    const second = await store.resolve({ key: ["k"] }, produce);

    // No stale-while-revalidate unless it is asked for: `expire` defaults to
    // `revalidate`, so the entry is expired rather than servable-and-stale.
    expect(second.outcome).toBe("miss");
    expect(second.value).toBe("answer 2");
    expect(produced).toBe(2);
  });

  it("serves the old value inside a window that was asked for, and refreshes behind it", async () => {
    const time = clock();
    const store = createCacheStore({ now: time.now });
    let produced = 0;
    const produce = async () => {
      produced += 1;
      cacheLife({ revalidate: 60, expire: 600 });
      return `answer ${produced}`;
    };

    await store.resolve({ key: ["k"] }, produce);
    time.advance(120);
    const stale = await store.resolve({ key: ["k"] }, produce);

    expect(stale.outcome).toBe("stale");
    expect(stale.value).toBe("answer 1");

    // The refresh is behind the reader, so it has not necessarily landed when
    // the reader was answered.
    await settled();
    expect(produced).toBe(2);

    const refreshed = await store.resolve({ key: ["k"] }, produce);
    expect(refreshed.outcome).toBe("hit");
    expect(refreshed.value).toBe("answer 2");
  });

  it("cannot be served past expire", async () => {
    const time = clock();
    const store = createCacheStore({ now: time.now });
    let produced = 0;
    const produce = async () => {
      produced += 1;
      cacheLife({ revalidate: 60, expire: 600 });
      return `answer ${produced}`;
    };

    await store.resolve({ key: ["k"] }, produce);
    time.advance(601);
    const after = await store.resolve({ key: ["k"] }, produce);

    expect(after.outcome).toBe("miss");
    expect(after.value).toBe("answer 2");
  });

  it("is expired rather than made stale by a tag, so no window can serve it", async () => {
    const time = clock();
    const store = createCacheStore({ now: time.now });
    let produced = 0;
    const produce = async () => {
      produced += 1;
      cacheTag("posts");
      cacheLife({ revalidate: 60, expire: 3600 });
      return `answer ${produced}`;
    };

    await store.resolve({ key: ["k"] }, produce);
    expect(store.revalidateTag("posts")).toBe(1);

    // Inside the stale-while-revalidate window, and still not served: somebody
    // said this answer is wrong, which is not the same as it being old.
    const after = await store.resolve({ key: ["k"] }, produce);
    expect(after.outcome).toBe("miss");
    expect(after.value).toBe("answer 2");
  });

  it("is reached by the tag it carries and by no other", async () => {
    const store = createCacheStore({ now: clock().now });
    const fill = (tag: string) => async () => {
      cacheTag(tag);
      cacheLife({ revalidate: 60 });
      return tag;
    };

    await store.resolve({ key: ["posts"] }, fill("posts"));
    await store.resolve({ key: ["users"] }, fill("users"));

    expect(store.revalidateTag("comments")).toBe(0);
    expect(store.revalidateTag("posts")).toBe(1);
    expect(store.peek(["posts"])).toBe(null);
    expect(store.peek(["users"])).not.toBe(null);
  });

  it("is reached by the path it was filled for", async () => {
    const store = createCacheStore({ now: clock().now });
    const fill = async () => {
      cacheLife({ revalidate: 60 });
      return "document";
    };

    await store.resolve({ key: ["route", "GET", "/posts", ""], path: "/posts" }, fill);
    await store.resolve({ key: ["route", "GET", "/about", ""], path: "/about" }, fill);

    expect(store.revalidatePath("/posts")).toBe(1);
    expect(store.peek(["route", "GET", "/posts", ""])).toBe(null);
    expect(store.peek(["route", "GET", "/about", ""])).not.toBe(null);
  });

  it("refuses to be stored at all when the fill said so", async () => {
    const store = createCacheStore({ now: clock().now });

    const result = await store.resolve({ key: ["private"] }, async () => {
      cacheLife({ revalidate: 60 });
      noStore("this one is about one person");
      return "secret";
    });

    expect(result.outcome).toBe("uncacheable");
    expect(store.size()).toBe(0);
  });
});

describe("who evicts", () => {
  it("drops the least recently used entry rather than the oldest one", async () => {
    const store = createCacheStore({ now: clock().now, maxEntries: 2 });
    const fill = async () => {
      cacheLife({ revalidate: 600 });
      return "value";
    };

    await store.resolve({ key: ["a"] }, fill);
    await store.resolve({ key: ["b"] }, fill);
    // Reading `a` makes `b` the least recently used, so `b` is what goes.
    await store.resolve({ key: ["a"] }, fill);
    await store.resolve({ key: ["c"] }, fill);

    expect(store.size()).toBe(2);
    expect(store.peek(["a"])).not.toBe(null);
    expect(store.peek(["b"])).toBe(null);
    expect(store.peek(["c"])).not.toBe(null);
    expect(store.stats().evictions).toBe(1);
  });

  it("refuses a store that can hold nothing", () => {
    expect(() => createCacheStore({ maxEntries: 0 })).toThrow();
  });
});

describe("a request that arrives while an entry is being filled", () => {
  it("joins the fill rather than starting a second one", async () => {
    const store = createCacheStore({ now: clock().now });
    let started = 0;
    let release = () => {};
    const gate = new Promise((resolve) => {
      release = resolve;
    });
    const produce = async () => {
      started += 1;
      await gate;
      cacheLife({ revalidate: 60 });
      return "one render";
    };

    const first = store.resolve({ key: ["slow"] }, produce);
    const second = store.resolve({ key: ["slow"] }, produce);
    const third = store.resolve({ key: ["slow"] }, produce);
    release();
    const [a, b, c] = await Promise.all([first, second, third]);

    expect(started).toBe(1);
    expect(a.value).toBe("one render");
    expect(b.value).toBe("one render");
    expect(c.value).toBe("one render");
    expect(b.outcome).toBe("coalesced");
    expect(store.stats().coalesced).toBe(2);
  });

  it("hands every joiner the failure, and caches none of it", async () => {
    const store = createCacheStore({ now: clock().now });
    let release = () => {};
    const gate = new Promise((resolve) => {
      release = resolve;
    });
    const produce = async () => {
      await gate;
      cacheLife({ revalidate: 60 });
      throw new Error("upstream is down");
    };

    const first = store.resolve({ key: ["slow"] }, produce);
    const second = store.resolve({ key: ["slow"] }, produce);
    release();

    const settled = await Promise.allSettled([first, second]);
    expect(settled[0].status).toBe("rejected");
    expect(settled[1].status).toBe("rejected");
    expect(store.size()).toBe(0);
  });
});

describe("what cacheLife and cacheTag mean outside a fill", () => {
  it("say which of them was out of place rather than failing generically", () => {
    let raised = null;
    try {
      cacheTag("posts");
    } catch (error) {
      raised = error;
    }

    expect(raised instanceof OutsideCacheScopeError).toBe(true);
    expect(String(raised)).toContain("cacheTag()");
  });

  it("refuse a tag that is not a name", async () => {
    const store = createCacheStore({ now: clock().now });

    let raised = null;
    try {
      await store.resolve({ key: ["k"] }, async () => {
        cacheTag("");
        return null;
      });
    } catch (error) {
      raised = error;
    }
    expect(raised instanceof TypeError).toBe(true);
  });
});

describe("the route cache", () => {
  it("renders once for two requests, and says so in a header", async () => {
    const { app, handle } = servingWith({
      render: () => {
        cacheLife({ revalidate: 60 });
      },
    });

    const first = await serve(handle, app, "/posts");
    const second = await serve(handle, app, "/posts");

    expect(first.headers.get("x-uf-cache")).toBe("MISS");
    expect(second.headers.get("x-uf-cache")).toBe("HIT");
    expect(app.renders.length).toBe(1);
    // The body is the *document*, byte for byte, not a re-render of it.
    expect(await second.text()).toBe(await first.text());
  });

  it("keeps a URL's search apart, because two searches are two documents", async () => {
    const { app, handle } = servingWith({
      render: () => {
        cacheLife({ revalidate: 60 });
      },
    });

    await serve(handle, app, "/posts?page=1");
    await serve(handle, app, "/posts?page=2");
    const again = await serve(handle, app, "/posts?page=1");

    expect(app.renders.length).toBe(2);
    expect(again.headers.get("x-uf-cache")).toBe("HIT");
  });

  it("stores nothing when the render never stated a lifetime", async () => {
    const { app, handle } = servingWith({});

    const first = await serve(handle, app, "/posts");
    await serve(handle, app, "/posts");

    expect(first.headers.get("x-uf-cache")).toBe("BYPASS");
    expect(app.renders.length).toBe(2);
  });

  it("stores nothing when the render read the request", async () => {
    const { app, handle } = servingWith({
      render: () => {
        cacheLife({ revalidate: 60 });
        cookies().get("session");
      },
    });

    const first = await serve(handle, app, "/account", { headers: { cookie: "session=abc" } });
    const second = await serve(handle, app, "/account", { headers: { cookie: "session=xyz" } });

    expect(first.headers.get("x-uf-cache")).toBe("BYPASS");
    expect(second.headers.get("x-uf-cache")).toBe("BYPASS");
    expect(app.renders.length).toBe(2);
  });

  it("is bypassed entirely for a request in draft mode", async () => {
    // The other direction from every case above. Those are about what the
    // cache refuses to *store*; this is about what it refuses to *answer* with.
    // A stored entry is a document from before the draft existed, so serving it
    // to the editor who came to look at the draft answers a different question
    // from the one they asked — and draft mode would be a feature that works
    // on every page except the ones anybody looks at twice.
    // ubugeeei-prod/uf#282.
    const { app, handle } = servingWith({
      handler: createDispatcher({
        handlers: [
          {
            path: "/api/preview",
            params: [],
            file: "app/api/preview/_uf.route.js",
            load: async () => ({
              GET: () => {
                draftMode().enable();
                return new Response("on");
              },
            }),
          },
        ],
      }),
      render: () => {
        cacheLife({ revalidate: 60 });
      },
    });

    expect((await serve(handle, app, "/posts")).headers.get("x-uf-cache")).toBe("MISS");
    expect((await serve(handle, app, "/posts")).headers.get("x-uf-cache")).toBe("HIT");
    expect(app.renders.length).toBe(1);

    const issued = await serve(handle, app, "/api/preview");
    const set =
      issued.headers.getSetCookie().find((value) => value.startsWith("__Host-uf.draft=")) ?? "";
    const cookie = set.slice(0, set.indexOf(";"));

    const drafted = await serve(handle, app, "/posts", { headers: { cookie } });

    // No `x-uf-cache` at all, because the request never reached the store: a
    // `BYPASS` would mean it went in and was refused, and the point is that it
    // did not go in.
    expect(drafted.headers.get("x-uf-cache")).toBe(null);
    expect(app.renders.length).toBe(2);
    // And the entry is still there for everybody else.
    expect((await serve(handle, app, "/posts")).headers.get("x-uf-cache")).toBe("HIT");
    expect(app.renders.length).toBe(2);
  });

  it("stores nothing when a component below the shell read the request", async () => {
    // The read that a cache deciding at the shell would miss: this runs while
    // the body is being drained, which is where a `<Suspense>` boundary's
    // contents render.
    const { app, handle } = servingWith({
      render: () => {
        cacheLife({ revalidate: 60 });
      },
      tail: () => {
        headers().get("authorization");
      },
    });

    const first = await serve(handle, app, "/feed");
    await serve(handle, app, "/feed");

    expect(first.headers.get("x-uf-cache")).toBe("BYPASS");
    expect(app.renders.length).toBe(2);
  });

  it("is not stopped by a guard that read the request and let it through", async () => {
    // The other half of the same rule, and the reason the read is a counter
    // rather than a flag: every application with an authentication guard reads
    // a cookie on every request, and none of that says the *page* varies.
    const { app, handle } = servingWith({
      guard: () => {
        cookies().get("session");
        return null;
      },
      render: () => {
        cacheLife({ revalidate: 60 });
      },
    });

    await serve(handle, app, "/posts", { headers: { cookie: "session=abc" } });
    const second = await serve(handle, app, "/posts", { headers: { cookie: "session=abc" } });

    expect(second.headers.get("x-uf-cache")).toBe("HIT");
    expect(app.renders.length).toBe(1);
  });

  it("stores nothing for a render that did not answer 200", async () => {
    const { app, handle } = servingWith({
      status: 404,
      render: () => {
        cacheLife({ revalidate: 60 });
      },
    });

    const first = await serve(handle, app, "/nope");
    expect(first.status).toBe(404);
    expect(first.headers.get("x-uf-cache")).toBe("BYPASS");

    await serve(handle, app, "/nope");
    expect(app.renders.length).toBe(2);
  });

  it("stores nothing for a render that set a cookie", async () => {
    const { app, handle } = servingWith({
      headers: { "set-cookie": "session=abc; Path=/" },
      render: () => {
        cacheLife({ revalidate: 60 });
      },
    });

    const first = await serve(handle, app, "/login");
    expect(first.headers.get("x-uf-cache")).toBe("BYPASS");
  });

  it("is emptied for one URL by a route handler that changed it", async () => {
    const time = clock();
    const store = createCacheStore({ now: time.now });
    const app = appWith({
      handler: (asRequest: Request) =>
        asRequest.method === "POST" ? Response.json({ expired: revalidateTag("posts") }) : null,
      render: () => {
        cacheTag("posts");
        cacheLife({ revalidate: 3600 });
      },
    });
    const handle = createFetchHandler({
      app,
      document: assets,
      cache: { store, route: true, fetch: false },
    });

    await serve(handle, app, "/posts");
    expect((await serve(handle, app, "/posts")).headers.get("x-uf-cache")).toBe("HIT");

    const mutation = await serve(handle, app, "/posts", { method: "POST" });
    expect(await mutation.json()).toEqual({ expired: 1 });

    const after = await serve(handle, app, "/posts");
    expect(after.headers.get("x-uf-cache")).toBe("MISS");
    expect(app.renders.length).toBe(2);
  });

  it("is emptied for one URL by revalidatePath", async () => {
    const store = createCacheStore({ now: clock().now });
    const app = appWith({
      handler: (asRequest: Request) =>
        asRequest.method === "POST" ? Response.json({ expired: revalidatePath("/posts") }) : null,
      render: () => {
        cacheLife({ revalidate: 3600 });
      },
    });
    const handle = createFetchHandler({
      app,
      document: assets,
      cache: { store, route: true, fetch: false },
    });

    await serve(handle, app, "/posts");
    const mutation = await serve(handle, app, "/posts", { method: "POST" });

    expect(await mutation.json()).toEqual({ expired: 1 });
    expect((await serve(handle, app, "/posts")).headers.get("x-uf-cache")).toBe("MISS");
  });

  it("goes stale on the clock and not before it", async () => {
    const { app, handle, time } = servingWith({
      render: () => {
        cacheLife({ revalidate: 60 });
      },
    });

    await serve(handle, app, "/posts");
    time.advance(59);
    expect((await serve(handle, app, "/posts")).headers.get("x-uf-cache")).toBe("HIT");
    time.advance(1);
    expect((await serve(handle, app, "/posts")).headers.get("x-uf-cache")).toBe("MISS");
    expect(app.renders.length).toBe(2);
  });

  it("does not fill an entry for a HEAD, and does not read one either", async () => {
    const { app, handle } = servingWith({
      render: () => {
        cacheLife({ revalidate: 60 });
      },
    });

    const head = await serve(handle, app, "/posts", { method: "HEAD" });
    expect(head.status).toBe(200);
    expect(head.headers.get("x-uf-cache")).toBe(null);
    expect(await head.text()).toBe("");

    const get = await serve(handle, app, "/posts");
    expect(get.headers.get("x-uf-cache")).toBe("MISS");
    expect(app.renders.length).toBe(2);
  });

  it("does nothing when the switch is off, and does not make cacheLife an error", async () => {
    // `rendering.cache.route: false` has to leave an application that states a
    // lifetime working, or turning the cache off would be a code change.
    const { app, handle } = servingWith(
      {
        render: () => {
          cacheLife({ revalidate: 60 });
          cacheTag("posts");
        },
      },
      { route: false },
    );

    const first = await serve(handle, app, "/posts");
    const second = await serve(handle, app, "/posts");

    expect(first.status).toBe(200);
    expect(first.headers.get("x-uf-cache")).toBe(null);
    expect(second.headers.get("x-uf-cache")).toBe(null);
    expect(app.renders.length).toBe(2);
  });

  it("leaves a handler with no cache at all rendering every request", async () => {
    const app = appWith({
      render: () => {
        cacheLife({ revalidate: 60 });
      },
    });
    const handle = createFetchHandler({ app, document: assets });

    await serve(handle, app, "/posts");
    const second = await serve(handle, app, "/posts");

    expect(second.headers.get("x-uf-cache")).toBe(null);
    expect(app.renders.length).toBe(2);
  });
});

describe("the fetch cache", () => {
  /** A client that counts what it was asked for. */
  function clientWith(): {| calls: Array<string>, request: (path: string) => Promise<string> |} {
    const calls = [];
    return {
      calls,
      request: async (path: string) => {
        calls.push(path);
        return `body ${calls.length}`;
      },
    };
  }

  it("passes a request with no cache option straight through", async () => {
    const store = createCacheStore({ now: clock().now });
    const client = clientWith();
    const cached = createCachedFetch({ client, name: "api", store });

    await cached.request("/users");
    await cached.request("/users");

    expect(client.calls.length).toBe(2);
    expect(store.size()).toBe(0);
  });

  it("asks once for a request that stated a lifetime", async () => {
    const time = clock();
    const store = createCacheStore({ now: time.now });
    const client = clientWith();
    const cached = createCachedFetch({ client, name: "api", store });
    const options = { cache: { lifetime: { revalidate: 60 }, tags: ["users"] } };

    const first = await cached.request("/users", options);
    const second = await cached.request("/users", options);

    expect(first).toBe("body 1");
    expect(second).toBe("body 1");
    expect(client.calls.length).toBe(1);

    time.advance(60);
    expect(await cached.request("/users", options)).toBe("body 2");
  });

  it("keeps two clients apart even when they request the same path", async () => {
    const store = createCacheStore({ now: clock().now });
    const first = clientWith();
    const second = clientWith();
    const options = { cache: { lifetime: { revalidate: 60 } } };
    const one = createCachedFetch({ client: first, name: "billing", store });
    const two = createCachedFetch({ client: second, name: "identity", store });

    await one.request("/users", options);
    await two.request("/users", options);

    expect(first.calls.length).toBe(1);
    expect(second.calls.length).toBe(1);
  });

  it("is reached by the same tags a route is", async () => {
    const store = createCacheStore({ now: clock().now });
    const client = clientWith();
    const cached = createCachedFetch({ client, name: "api", store });
    const options = { cache: { lifetime: { revalidate: 3600 }, tags: ["users"] } };

    await cached.request("/users", options);
    expect(store.revalidateTag("users")).toBe(1);
    await cached.request("/users", options);

    expect(client.calls.length).toBe(2);
  });

  it("passes everything through when no store is installed for the request", async () => {
    // `rendering.cache.fetch: false`, or a call outside a request: the wrapper
    // becomes the client it wraps. Slower, never wrong.
    const client = clientWith();
    const cached = createCachedFetch({ client, name: "api" });
    const options = { cache: { lifetime: { revalidate: 60 } } };

    await cached.request("/users", options);
    await cached.request("/users", options);

    expect(client.calls.length).toBe(2);
  });

  it("uses the store the host installed for the request when fetch caching is on", async () => {
    const time = clock();
    const store = createCacheStore({ now: time.now });
    const client = clientWith();
    const cached = createCachedFetch({ client, name: "api" });
    const options = { cache: { lifetime: { revalidate: 60 } } };
    const app = appWith({
      handler: async () => Response.json({ body: await cached.request("/users", options) }),
    });
    const handle = createFetchHandler({
      app,
      document: assets,
      cache: { store, route: false, fetch: true },
    });

    const first = await serve(handle, app, "/api/users");
    const second = await serve(handle, app, "/api/users");

    expect(await first.json()).toEqual({ body: "body 1" });
    expect(await second.json()).toEqual({ body: "body 1" });
    expect(client.calls.length).toBe(1);
  });

  it("does not use it when fetch caching is off, even though the route cache is on", async () => {
    const store = createCacheStore({ now: clock().now });
    const client = clientWith();
    const cached = createCachedFetch({ client, name: "api" });
    const options = { cache: { lifetime: { revalidate: 60 } } };
    const app = appWith({
      handler: async () => Response.json({ body: await cached.request("/users", options) }),
    });
    const handle = createFetchHandler({
      app,
      document: assets,
      cache: { store, route: true, fetch: false },
    });

    await serve(handle, app, "/api/users");
    await serve(handle, app, "/api/users");

    expect(client.calls.length).toBe(2);
  });
});

describe("what rendering.cache reaches", () => {
  // `uf preview` and `uf start` hand `config.app.rendering.cache` straight to
  // `createApplicationHandler`, and this is that function. The other two doors
  // — the `handler.js` an adapter writes and the compiled binary — construct
  // the same three fields from the same two switches; #277's PR says which of
  // them is checked where.

  it("builds a store when the switch is on, and caches through it", async () => {
    const app = appWith({
      render: () => {
        cacheLife({ revalidate: 60 });
      },
    });
    const handle = createApplicationHandler({
      entry: app,
      assets,
      cache: { route: true, fetch: false },
    });

    await serve(handle, app, "/posts");
    const second = await serve(handle, app, "/posts");

    expect(second.headers.get("x-uf-cache")).toBe("HIT");
    expect(app.renders.length).toBe(1);
  });

  it("builds nothing when both switches are off", async () => {
    const app = appWith({
      render: () => {
        cacheLife({ revalidate: 60 });
      },
    });
    const handle = createApplicationHandler({
      entry: app,
      assets,
      cache: { route: false, fetch: false },
    });

    const first = await serve(handle, app, "/posts");
    await serve(handle, app, "/posts");

    // No store at all rather than a store with both switches off, so a project
    // that turned the cache off is told so by `revalidateTag` raising rather
    // than reporting that it expired nothing.
    expect(first.headers.get("x-uf-cache")).toBe(null);
    expect(app.renders.length).toBe(2);
  });

  it("builds nothing when the project said nothing", async () => {
    const app = appWith({
      render: () => {
        cacheLife({ revalidate: 60 });
      },
    });
    const handle = createApplicationHandler({ entry: app, assets });

    await serve(handle, app, "/posts");
    await serve(handle, app, "/posts");

    expect(app.renders.length).toBe(2);
  });
});

// The rest of this file is the half of the contract that only exists once an
// entry can outlive the process that filled it. `packages/server/cache.js`'s
// header used to end by naming what it did not have — "a durable store behind
// `resolve`, which is an adapter's to provide" — and these are the promises
// that sentence turned into.
//
// Almost all of it is driven through a `Map` behind the provider seam rather
// than through a disk, on purpose: what is being checked is what the *store*
// does with a provider, and one `Map` shared by two stores is two processes
// sharing one cache with nothing else in the way. The filesystem provider gets
// its own tests, further down, because it is one implementation of the seam and
// not the seam.

describe("a durable store", () => {
  it("is not one unless a host asked for it", async () => {
    const store = createCacheStore({ now: clock().now });

    await store.resolve({ key: ["a"], lifetime: { revalidate: 60 } }, async () => "one");
    await store.settled();

    // Persistence is a second opt-in on top of the first. A project that turned
    // the route cache on and said nothing else has exactly the store it had
    // before any of this existed.
    expect(store.stats().persisted).toBe(0);
    expect(store.stats().restored).toBe(0);
  });

  it("refuses a provider with no build to key its entries by", () => {
    // The one place the store refuses rather than degrading. A host passed a
    // provider on purpose, in one line of wiring; falling back to memory would
    // turn a typo into a deployment that is not what it says it is, and
    // generating an identity per process would be worse — four servers writing
    // four copies into one store and reading none of them.
    expect(() => createCacheStore({ provider: fakeProvider() })).toThrow(/build/);
    expect(() => createCacheStore({ provider: fakeProvider(), build: "" })).toThrow(/build/);
  });

  it("refuses a provider that cannot answer the whole seam", () => {
    const provider: $FlowFixMe = fakeProvider();
    delete provider.invalidateTag;

    // Checked where it is wired rather than at the first call, because the
    // symptom otherwise is a cache that answers every request perfectly and
    // silently stops invalidating.
    expect(() => createCacheStore({ provider, build: "b" })).toThrow(/invalidateTag/);
    expect(() => createCacheStore({ provider: { name: "half", read() {} }, build: "b" })).toThrow(
      /read|write/,
    );
  });

  it("keeps an entry across a restart", async () => {
    const provider = fakeProvider();
    const time = clock();
    const before = durableStore(provider, { now: time.now });

    await before.resolve({ key: ["route", "/posts"], lifetime: { revalidate: 60 } }, async () => ({
      title: "one",
    }));
    await before.settled();

    // A second store over the same provider and nothing else: a process that
    // restarted, or the fourth server behind the load balancer.
    const after = durableStore(provider, { now: time.now });
    let rendered = 0;
    const result = await after.resolve(
      { key: ["route", "/posts"], lifetime: { revalidate: 60 } },
      async () => {
        rendered += 1;
        return { title: "two" };
      },
    );

    expect(result.outcome).toBe("hit");
    expect(result.value).toEqual({ title: "one" });
    expect(rendered).toBe(0);
    expect(after.stats().restored).toBe(1);
  });

  it("keys entries by the build, so a deploy does not read the last one's", async () => {
    const provider = fakeProvider();
    const time = clock();
    const old = durableStore(provider, { now: time.now, build: "build-one" });
    await old.resolve(
      { key: ["route", "/posts"], lifetime: { revalidate: 600 } },
      async () => "old",
    );
    await old.settled();

    const deployed = durableStore(provider, { now: time.now, build: "build-two" });
    const result = await deployed.resolve(
      { key: ["route", "/posts"], lifetime: { revalidate: 600 } },
      async () => "new",
    );
    await deployed.settled();

    // The whole of `internal/cache-key.js`'s argument, as one assertion: the
    // entry is still in the store and under a different name, so a deploy is a
    // cold cache rather than the previous build's documents answering the new
    // build's URLs.
    expect(result.value).toBe("new");
    expect(deployed.stats().restored).toBe(0);
    expect(provider.entries.size).toBe(2);
  });

  it("stores nothing durably that it would not store in memory", async () => {
    const provider = fakeProvider();
    const store = durableStore(provider, { now: clock().now });

    await store.resolve({ key: ["a"] }, async () => "no lifetime");
    await store.resolve({ key: ["b"], lifetime: { revalidate: 60 } }, async () => {
      noStore("this one asked not to be");
      return "denied";
    });
    await store.settled();

    // "No entry is stored without a stated lifetime" is a rule about the cache,
    // not about where the cache keeps things, so it holds on both sides of the
    // seam and there is no configuration that opens a hole in it.
    expect(provider.entries.size).toBe(0);
    expect(store.stats().persisted).toBe(0);
  });

  it("brings a document's bytes back as bytes", async () => {
    const provider = fakeProvider();
    const time = clock();
    const before = durableStore(provider, { now: time.now });
    const body = new TextEncoder().encode("<!doctype html><p>hello</p>");

    await before.resolve({ key: ["route", "/"], lifetime: { revalidate: 60 } }, async () => ({
      status: 200,
      headers: { "x-thing": "1" },
      body,
    }));
    await before.settled();

    const after = durableStore(provider, { now: time.now });
    const result = await after.resolve(
      { key: ["route", "/"], lifetime: { revalidate: 60 } },
      async () => ({ status: 500, headers: {}, body: new Uint8Array() }),
    );

    // A rendered document is a `Uint8Array`, and plain JSON turns one into an
    // object of numeric keys with no error anywhere. That is why the encoding
    // is uf's and not each provider's.
    expect(result.value.body instanceof Uint8Array).toBe(true);
    expect(new TextDecoder().decode(result.value.body)).toBe("<!doctype html><p>hello</p>");
    expect(result.value.headers).toEqual({ "x-thing": "1" });
  });

  it("refuses a value it cannot bring back unchanged, and keeps it in memory", async () => {
    const provider = fakeProvider();
    const failures = [];
    const store = createCacheStore({
      now: clock().now,
      provider: (provider: $FlowFixMe),
      build: "b",
      onError: (error) => {
        failures.push(error);
      },
    });

    const first = await store.resolve({ key: ["a"], lifetime: { revalidate: 60 } }, async () => ({
      at: new Date(0),
    }));
    await store.settled();
    const second = await store.resolve({ key: ["a"], lifetime: { revalidate: 60 } }, async () => ({
      at: new Date(1),
    }));

    // A `Date` goes out through `toJSON` and comes back a string, which is a
    // different answer wearing the same name. Refused rather than stored — and
    // refused is not failed: the request got its value and the entry is in
    // memory exactly as it always was.
    expect(provider.entries.size).toBe(0);
    expect(failures.length).toBe(1);
    expect(String(failures[0])).toMatch(/Date/);
    expect(second.outcome).toBe("hit");
    expect(second.value).toBe(first.value);
  });

  it("answers from the render when the provider is broken, and says so", async () => {
    const provider = fakeProvider();
    provider.broken = "the disk is gone";
    const failures = [];
    const store = createCacheStore({
      now: clock().now,
      provider: (provider: $FlowFixMe),
      build: "b",
      onError: (error) => {
        failures.push(error);
      },
    });

    const result = await store.resolve(
      { key: ["a"], lifetime: { revalidate: 60 } },
      async () => "rendered",
    );
    await store.settled();

    // Slower, never wrong: a store that cannot reach its provider costs a
    // render and reports both halves of why, and nothing a request can see
    // changed.
    expect(result.value).toBe("rendered");
    expect(result.outcome).toBe("miss");
    expect(failures.length).toBe(2);
  });

  it("serves a durable entry inside its window and refreshes behind it", async () => {
    const provider = fakeProvider();
    const time = clock();
    const before = durableStore(provider, { now: time.now });
    await before.resolve(
      { key: ["a"], lifetime: { revalidate: 60, expire: 600 } },
      async () => "old",
    );
    await before.settled();

    time.advance(120);
    const after = durableStore(provider, { now: time.now });
    let rendered = 0;
    const result = await after.resolve(
      { key: ["a"], lifetime: { revalidate: 60, expire: 600 } },
      async () => {
        rendered += 1;
        return "new";
      },
    );
    await settled();
    await after.settled();

    // Staleness is decided from the entry's own timestamps whichever store
    // wrote them, so a process that has just started reads an entry another
    // process filled two minutes ago as exactly two minutes old.
    expect(result.outcome).toBe("stale");
    expect(result.value).toBe("old");
    expect(rendered).toBe(1);
    expect(after.peek(["a"])?.value).toBe("new");
  });

  it("cannot serve a durable entry past expire", async () => {
    const provider = fakeProvider();
    const time = clock();
    const before = durableStore(provider, { now: time.now });
    await before.resolve({ key: ["a"], lifetime: { revalidate: 60 } }, async () => "old");
    await before.settled();

    time.advance(61);
    const after = durableStore(provider, { now: time.now });
    const result = await after.resolve(
      { key: ["a"], lifetime: { revalidate: 60 } },
      async () => "new",
    );
    await after.settled();

    expect(result.value).toBe("new");
    expect(after.stats().restored).toBe(0);
    // Dropped on the read that found it, which is the rule memory has, pointed
    // at the provider — then rewritten by the fill, so there is one entry and
    // it is this one.
    expect(provider.entries.size).toBe(1);
  });

  it("goes to the provider once for two callers who both missed memory", async () => {
    const provider = fakeProvider();
    const store = durableStore(provider, { now: clock().now });
    let rendered = 0;
    const produce = async () => {
      rendered += 1;
      return "one";
    };

    const [first, second] = await Promise.all([
      store.resolve({ key: ["a"], lifetime: { revalidate: 60 } }, produce),
      store.resolve({ key: ["a"], lifetime: { revalidate: 60 } }, produce),
    ]);

    // The answer that made the durable read asynchronous: the key is claimed
    // before the first `await`, so a second caller joins rather than making its
    // own trip to the disk and then its own render.
    expect(rendered).toBe(1);
    expect(provider.reads).toBe(1);
    expect(first.outcome).toBe("miss");
    expect(second.outcome).toBe("coalesced");
  });

  it("round-trips what an entry may hold, and nothing it may not", () => {
    const value = { list: [1, "two", null, true], nested: { bytes: new Uint8Array([1, 2, 3]) } };
    const back: $FlowFixMe = decodeCacheValue(encodeCacheValue(value));

    expect(back.list).toEqual([1, "two", null, true]);
    expect(Array.from(back.nested.bytes)).toEqual([1, 2, 3]);
    // An ordinary object entitled to a field called `$uf`. Escaped rather than
    // rejected, because a JSON API is allowed to use that name.
    expect(decodeCacheValue(encodeCacheValue({ $uf: "mine" }))).toEqual({ $uf: "mine" });
    expect(decodeCacheValue(encodeCacheValue(undefined))).toBe(undefined);
    expect(() => encodeCacheValue({ go: () => {} })).toThrow(/function/);
    expect(() => encodeCacheValue(new Map())).toThrow(/Map/);
  });
});

describe("invalidating a durable store", () => {
  it("takes the entry out of the store every process fills from", async () => {
    const provider = fakeProvider();
    const time = clock();
    const one = durableStore(provider, { now: time.now });
    const two = durableStore(provider, { now: time.now });

    await one.resolve(
      { key: ["route", "/posts"], lifetime: { revalidate: 600 }, tags: ["posts"] },
      async () => "old",
    );
    await one.settled();

    // The mutation happens in the process that did not fill the entry, which is
    // the case that could not work at all before a shared store existed.
    two.revalidateTag("posts");
    await two.settled();

    // A third process, which never held a copy: what it reads is what the
    // shared store holds, and the shared store no longer holds the entry.
    const three = durableStore(provider, { now: time.now });
    let rendered = 0;
    const result = await three.resolve(
      { key: ["route", "/posts"], lifetime: { revalidate: 600 }, tags: ["posts"] },
      async () => {
        rendered += 1;
        return "new";
      },
    );

    expect(provider.entries.size).toBe(1);
    expect(result.value).toBe("new");
    expect(rendered).toBe(1);
    expect(three.stats().restored).toBe(0);
  });

  it("reaches the shared store by path too", async () => {
    const provider = fakeProvider();
    const time = clock();
    const one = durableStore(provider, { now: time.now });
    const two = durableStore(provider, { now: time.now });

    await one.resolve(
      { key: ["route", "/posts"], lifetime: { revalidate: 600 }, path: "/posts" },
      async () => "old",
    );
    await one.settled();

    // Nothing in this process's memory carries the path, so the count is zero
    // and the invalidation is entirely the shared half.
    expect(two.revalidatePath("/posts")).toBe(0);
    await two.settled();

    expect(provider.entries.size).toBe(0);
  });

  it("answers with what went here, having taken out what is shared", async () => {
    const provider = fakeProvider();
    const time = clock();
    const one = durableStore(provider, { now: time.now });
    const two = durableStore(provider, { now: time.now });

    for (const key of [["a"], ["b"], ["c"]]) {
      await one.resolve({ key, lifetime: { revalidate: 600 }, tags: ["posts"] }, async () => "v");
    }
    await one.settled();
    // Two of the three are read into the second process as well, so the two
    // stores hold different amounts of the same shared set.
    await two.resolve(
      { key: ["a"], lifetime: { revalidate: 600 }, tags: ["posts"] },
      async () => "v",
    );
    await two.resolve(
      { key: ["b"], lifetime: { revalidate: 600 }, tags: ["posts"] },
      async () => "v",
    );

    const dropped = two.revalidateTag("posts");
    await two.settled();

    // The number is this process's memory, synchronously, because a mutation
    // handler should not wait on a disk to learn an integer it is going to put
    // in a log. The invalidation is the larger, shared thing beside it.
    expect(dropped).toBe(2);
    expect(provider.entries.size).toBe(0);
  });
});

describe("the route cache, on a disk", () => {
  it("answers a restarted process from what the last one rendered", async () => {
    const directory = tempDirectory();
    const build = "build-one";
    const first = appWith({
      render: () => {
        cacheLife({ revalidate: 60 });
      },
    });
    const before = createApplicationHandler({
      entry: first,
      assets,
      root: directory,
      build,
      cache: { route: true, store: "filesystem", storeDir: directory },
    });
    const cold = await serve(before, first, "/posts");

    // A different handler over a different store and the same directory: the
    // process restarted, or this is the second of four behind a load balancer.
    const second = appWith({
      render: () => {
        cacheLife({ revalidate: 60 });
      },
    });
    const after = createApplicationHandler({
      entry: second,
      assets,
      root: directory,
      build,
      cache: { route: true, store: "filesystem", storeDir: directory },
    });
    const warm = await serve(after, second, "/posts");

    expect(cold.headers.get("x-uf-cache")).toBe("MISS");
    expect(warm.headers.get("x-uf-cache")).toBe("HIT");
    expect(second.renders.length).toBe(0);
    // Byte for byte the document the first process produced, which is what
    // `Uint8Array` support in the encoding exists to make true.
    expect(await warm.text()).toBe("<!doctype html><p>/posts</p><b>1</b>");
    fs.rmSync(directory, { recursive: true, force: true });
  });

  it("does not answer the next build from the last build's documents", async () => {
    const directory = tempDirectory();
    const render = () => {
      cacheLife({ revalidate: 600 });
    };
    const first = appWith({ render });
    await serve(
      createApplicationHandler({
        entry: first,
        assets,
        root: directory,
        build: "build-one",
        cache: { route: true, store: "filesystem", storeDir: directory },
      }),
      first,
      "/posts",
    );

    const second = appWith({ render });
    const deployed = await serve(
      createApplicationHandler({
        entry: second,
        assets,
        root: directory,
        build: "build-two",
        cache: { route: true, store: "filesystem", storeDir: directory },
      }),
      second,
      "/posts",
    );

    expect(deployed.headers.get("x-uf-cache")).toBe("MISS");
    expect(second.renders.length).toBe(1);
    fs.rmSync(directory, { recursive: true, force: true });
  });

  it("keeps two searches apart on disk as it does in memory", async () => {
    const directory = tempDirectory();
    const provider = createFilesystemCache({ directory });
    const store = createCacheStore({ provider, build: "b", now: clock().now });

    await store.resolve(
      { key: ["route", "/posts", "?page=1"], lifetime: { revalidate: 60 } },
      async () => "one",
    );
    await store.resolve(
      { key: ["route", "/posts", "?page=2"], lifetime: { revalidate: 60 } },
      async () => "two",
    );
    await store.settled();

    const restarted = createCacheStore({ provider, build: "b", now: clock().now });
    const page2 = await restarted.resolve(
      { key: ["route", "/posts", "?page=2"], lifetime: { revalidate: 60 } },
      async () => "rendered again",
    );

    expect(page2.value).toBe("two");
    expect(provider.name).toBe("filesystem");
    fs.rmSync(directory, { recursive: true, force: true });
  });

  it("takes a tag out of the directory, not only out of this process", async () => {
    const directory = tempDirectory();
    const provider = createFilesystemCache({ directory });
    const one = createCacheStore({ provider, build: "b", now: clock().now, onError: () => {} });
    const two = createCacheStore({ provider, build: "b", now: clock().now, onError: () => {} });

    await one.resolve(
      { key: ["route", "/posts"], lifetime: { revalidate: 600 }, tags: ["posts"], path: "/posts" },
      async () => "old",
    );
    await one.settled();
    expect(fs.readdirSync(directory).filter((name) => name.endsWith(".json")).length).toBe(1);

    two.revalidateTag("posts");
    await two.settled();

    // Both halves of the entry, gone from the directory the other three
    // processes fill from.
    expect(fs.readdirSync(directory)).toEqual([]);
    fs.rmSync(directory, { recursive: true, force: true });
  });

  it("bounds the directory rather than growing forever", async () => {
    const directory = tempDirectory();
    const provider = createFilesystemCache({ directory, maxEntries: 2 });
    const store = createCacheStore({ provider, build: "b", now: clock().now, onError: () => {} });

    for (const key of ["a", "b", "c", "d"]) {
      await store.resolve({ key: [key], lifetime: { revalidate: 600 } }, async () => key);
      await store.settled();
    }

    // Nothing in a content-addressed cache ever removes an entry, so a
    // directory only grows unless something bounds it — the lesson
    // ubugeeei-prod/uf#218 records about the transform cache, applied here.
    const records = fs.readdirSync(directory).filter((name) => name.endsWith(".json"));
    const bodies = fs.readdirSync(directory).filter((name) => name.endsWith(".bin"));
    expect(records.length).toBe(2);
    expect(bodies.length).toBe(2);
    fs.rmSync(directory, { recursive: true, force: true });
  });
});

describe("what rendering.cache.store reaches", () => {
  it("refuses a durable store with no build identity to key it by", async () => {
    const directory = tempDirectory();
    const app = appWith({});
    const handle = createApplicationHandler({
      entry: app,
      assets,
      root: directory,
      cache: { route: true, store: "filesystem" },
    });

    // A host saying so rather than degrading quietly, which is what the
    // deployment rules require of a target that cannot provide a durable store.
    await expect(serve(handle, app, "/posts")).rejects.toThrow(/build identity/);
    fs.rmSync(directory, { recursive: true, force: true });
  });

  it("takes a module specifier, so a provider need not be one uf ships", async () => {
    const directory = tempDirectory();
    const module = nodePath.join(directory, "provider.mjs");
    // A provider written by a project, in twenty lines, against the type
    // `@uniflowed/server/cache` exports. Named in `uf.config.js` and imported
    // by name — which is red line 3's "a name it can write", and is how a Redis
    // or a KV namespace goes behind this seam without uf shipping either.
    fs.writeFileSync(
      module,
      `const entries = new Map();
export function createCacheProvider() {
  return {
    name: "in-a-module",
    async read(key) { return entries.get(key) ?? null; },
    async write(key, entry) { entries.set(key, entry); },
    async remove(key) { entries.delete(key); },
    async invalidateTag() { return 0; },
    async invalidatePath() { return 0; },
    async clear() { entries.clear(); },
  };
}
`,
    );

    const app = appWith({
      render: () => {
        cacheLife({ revalidate: 60 });
      },
    });
    const handle = createApplicationHandler({
      entry: app,
      assets,
      root: directory,
      build: "build-one",
      // Relative to the *project*, which is what somebody writing this in
      // `uf.config.js` means and is not what `import()` from inside
      // `@uniflowed/vite` would do on its own.
      cache: { route: true, store: "./provider.mjs" },
    });

    await serve(handle, app, "/posts");
    const second = await serve(handle, app, "/posts");

    expect(second.headers.get("x-uf-cache")).toBe("HIT");
    expect(app.renders.length).toBe(1);
    expect(providerSpecifier(directory, "./provider.mjs")).toBe(pathToFileURL(module).href);
    // A package name is Node's to resolve and is left exactly as written.
    expect(providerSpecifier(directory, "@acme/uf-cache-redis")).toBe("@acme/uf-cache-redis");
    fs.rmSync(directory, { recursive: true, force: true });
  });

  it("keeps memory as the default, so nothing persists unasked", async () => {
    const app = appWith({
      render: () => {
        cacheLife({ revalidate: 60 });
      },
    });
    const directory = tempDirectory();
    const handle = createApplicationHandler({
      entry: app,
      assets,
      root: directory,
      build: "build-one",
      cache: { route: true },
    });

    await serve(handle, app, "/posts");
    await serve(handle, app, "/posts");

    // The store still works — one render for two requests — and left nothing
    // anywhere. Persistence is opted into by name and by nothing else.
    expect(app.renders.length).toBe(1);
    expect(fs.readdirSync(directory)).toEqual([]);
    fs.rmSync(directory, { recursive: true, force: true });
  });
});
