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

import { describe, expect, it } from "@uniflowed/test";

import {
  OutsideCacheScopeError,
  cacheLife,
  cacheTag,
  createCacheStore,
  createCachedFetch,
  noStore,
  revalidatePath,
  revalidateTag,
} from "@uniflowed/server/cache";
import { cookies, headers } from "@uniflowed/server";
import { createFetchHandler } from "@uniflowed/server/fetch";
import { beginRequest } from "@uniflowed/server/host";

// Not a package export, deliberately — `serve.test.js` says why. It is what
// `uf preview` and `uf start` build their handler with, and it is the only
// place `rendering.cache` becomes a store for those two commands, so it is
// reached by path here for the same reason it is there.
import { createApplicationHandler } from "../../packages/vite/internal/serve.js";

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
