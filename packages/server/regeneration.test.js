// @flow
//
// Incremental static regeneration, at each seam that makes it up.
//
// A prerendered page that stated a lifetime is served from the document the
// build wrote, regenerated in the background once the lifetime has passed, and
// replaced in one step. The `describe`s below take the pieces in order: the
// store's seed, entries kept until something replaces them, what a host waits
// for so a refresh is not stopped halfway, what the build reads a page to have
// declared, a regenerated page through a Node and a Worker front door, and the
// Workers KV provider a Worker keeps regenerated pages in.
//
// The whole chain — `uf build` writing a page for regeneration and a server
// regenerating it without a rebuild — is
// `a_regenerated_page_changes_after_its_lifetime_without_a_rebuild` in
// `crates/uf_cli/tests/vite.rs`, under `uf preview` and `uf start`, and
// `tools/ci/edge-worker-smoke.sh` under workerd.
//
// Time is injected rather than waited for, for the reason `cache.test.js` gives:
// a test that establishes staleness by sleeping fails on a loaded machine.

import fs from "node:fs";
import os from "node:os";
import nodePath from "node:path";

import { describe, expect, it } from "@uniflowed/test";

import {
  cacheLife,
  cacheTag,
  collectCacheDeclarations,
  createCacheStore,
  noStore,
  revalidateTag,
} from "@uniflowed/server/cache";
import { createFilesystemCache } from "@uniflowed/server/cache/filesystem";
import { KvBindingMissingError, createKvCache } from "@uniflowed/server/cache/kv";
import { createWorkerFetch } from "@uniflowed/server/edge";
import { createFetchHandler } from "@uniflowed/server/fetch";
import { beginRequest } from "@uniflowed/server/host";
import { createServeHandler } from "@uniflowed/server/node";

import { END_OF_TIME } from "./internal/cache-store.js";

const assets = { scripts: ["/assets/client.js"], styles: [], preloads: [] };

const request = (url: string, init?: mixed) => new Request(`http://localhost${url}`, init);

/** The clock a test drives: milliseconds out, seconds in. */
function clock(): {| now: () => number, advance: (seconds: number) => void |} {
  let millis = 1_000_000;
  return {
    now: () => millis,
    advance: (seconds: number) => {
      millis += seconds * 1000;
    },
  };
}

/** Let every microtask already queued run, so a background refresh can land. */
async function settled(): Promise<void> {
  for (let turn = 0; turn < 20; turn += 1) {
    await Promise.resolve();
  }
}

/** The half of a `ReadableStream` controller the fixture uses. */
type StreamController = {
  readonly enqueue: (chunk: Uint8Array) => mixed,
  readonly close: () => mixed,
  ...
};

/**
 * A server bundle whose every render states a one-minute lifetime and a tag,
 * the way a page written for regeneration does, and counts itself.
 */
function regeneratingApp() {
  const renders: Array<string> = [];
  return {
    renders,
    beginRequest,
    runMiddleware: async (_request: Request) => null,
    callAction: async (_request: Request) => null,
    dispatch: async (_request: Request) => null,
    render: async (url: string) => {
      cacheLife({ revalidate: 60 });
      cacheTag("clock");
      renders.push(url);
      const html = `<!doctype html><p>rendered ${url}</p><b>${renders.length}</b>`;
      return {
        status: 200,
        headers: undefined,
        pipe: (destination: { write: (chunk: string) => mixed, end: () => mixed, ... }) => {
          destination.write(html);
          destination.end();
        },
        stream: () =>
          new ReadableStream({
            start(controller: StreamController) {
              controller.enqueue(new TextEncoder().encode(html));
              controller.close();
            },
          }),
      };
    },
  };
}

/** Answer one request the way every host does: begin, run, settle. */
async function serve(handle, url: string): Promise<Response> {
  const asRequest = request(url);
  const { run, settle } = beginRequest(asRequest);
  try {
    return await run(() => handle(asRequest));
  } finally {
    await settle();
  }
}

/** What `uf build` records for `/clock`, rendered at `renderedAt`. */
function manifestFor(renderedAt: number) {
  return {
    pages: {
      "/clock": {
        document: "/__uf/regenerate/clock/",
        renderedAt,
        revalidate: 60,
        expire: null,
        tags: ["clock"],
      },
    },
  };
}

/** A build's output directory holding the regenerated document for `/clock`. */
function buildDirectory(): string {
  const root = fs.mkdtempSync(nodePath.join(os.tmpdir(), "uf-regenerate-"));
  const file = nodePath.join(root, "__uf", "regenerate", "clock", "index.html");
  fs.mkdirSync(nodePath.dirname(file), { recursive: true });
  fs.writeFileSync(file, "<!doctype html><p>the build's copy</p>");
  return root;
}

/** A stored entry, for a seed or a provider. */
function entryOf(value: mixed, storedAt: number, revalidateAt: number, tags: Array<string> = []) {
  return { value, storedAt, revalidateAt, expiresAt: END_OF_TIME, tags, path: null };
}

describe("a key started from a seed", () => {
  it("is the seed's entry until its lifetime passes, then a refresh replaces it", async () => {
    const time = clock();
    const store = createCacheStore({ now: time.now, onError: () => {} });
    let asked = 0;
    const seeding = {
      key: ["page"],
      staleUntilReplaced: true,
      seed: async () => {
        asked += 1;
        return entryOf("the build's copy", time.now() - 10_000, time.now() + 50_000);
      },
    };
    const render = async () => {
      cacheLife({ revalidate: 60 });
      return "rendered";
    };

    const first = await store.resolve(seeding, render);
    expect(first.value).toBe("the build's copy");
    expect(first.outcome).toBe("hit");
    expect(store.stats().seeded).toBe(1);

    time.advance(60);
    const stale = await store.resolve(seeding, render);
    expect(stale.value).toBe("the build's copy");
    expect(stale.outcome).toBe("stale");
    await settled();

    const replaced = await store.resolve(seeding, render);
    expect(replaced.value).toBe("rendered");
    expect(replaced.outcome).toBe("hit");
    // Asked once for the life of the store, however many reads there were.
    expect(asked).toBe(1);
  });

  it("is never started from the seed again once it was invalidated", async () => {
    const time = clock();
    const store = createCacheStore({ now: time.now, onError: () => {} });
    let asked = 0;
    const seeding = {
      key: ["page"],
      staleUntilReplaced: true,
      seed: async () => {
        asked += 1;
        return entryOf("the build's copy", time.now(), time.now() + 60_000, ["posts"]);
      },
    };
    const render = async () => {
      cacheLife({ revalidate: 60 });
      return "rendered";
    };

    expect((await store.resolve(seeding, render)).value).toBe("the build's copy");
    expect(store.revalidateTag("posts")).toBe(1);

    // Known wrong, so a render and not the build's copy, which is older still.
    const after = await store.resolve(seeding, render);
    expect(after.value).toBe("rendered");
    expect(after.outcome).toBe("miss");
    expect(asked).toBe(1);
  });

  it("renders when the seed fails, and says why", async () => {
    const failures: Array<mixed> = [];
    const store = createCacheStore({ onError: (error) => failures.push(error) });
    const result = await store.resolve(
      {
        key: ["page"],
        seed: async () => {
          throw new Error("the static half is gone");
        },
      },
      async () => {
        cacheLife({ revalidate: 60 });
        return "rendered";
      },
    );

    expect(result.value).toBe("rendered");
    expect(result.outcome).toBe("miss");
    expect(String(failures[0])).toContain("the static half is gone");
  });
});

describe("an entry kept until something replaces it", () => {
  it("stays servable past its lifetime unless the fill stated an expiry", async () => {
    const time = clock();
    const store = createCacheStore({ now: time.now, onError: () => {} });
    let renders = 0;
    const fill = (lifetime: {| revalidate: number, expire?: number |}) => async () => {
      renders += 1;
      cacheLife(lifetime);
      return `render ${renders}`;
    };

    await store.resolve({ key: ["kept"], staleUntilReplaced: true }, fill({ revalidate: 1 }));
    await store.resolve({ key: ["dropped"] }, fill({ revalidate: 1 }));
    await store.resolve(
      { key: ["stated"], staleUntilReplaced: true },
      fill({ revalidate: 1, expire: 5 }),
    );
    time.advance(3600);

    expect(
      (await store.resolve({ key: ["kept"], staleUntilReplaced: true }, fill({ revalidate: 1 })))
        .outcome,
    ).toBe("stale");
    expect((await store.resolve({ key: ["dropped"] }, fill({ revalidate: 1 }))).outcome).toBe(
      "miss",
    );
    expect(
      (
        await store.resolve(
          { key: ["stated"], staleUntilReplaced: true },
          fill({ revalidate: 1, expire: 5 }),
        )
      ).outcome,
    ).toBe("miss");
  });

  it("names an instant a Date can hold and JSON can carry", async () => {
    const store = createCacheStore();
    await store.resolve({ key: ["forever"], staleUntilReplaced: true }, async () => {
      cacheLife({ revalidate: END_OF_TIME / 1000 });
      return "kept";
    });

    const entry = store.peek(["forever"]);
    expect(entry?.expiresAt).toBe(END_OF_TIME);
    expect(entry?.revalidateAt).toBe(END_OF_TIME);
    expect(JSON.parse(JSON.stringify({ at: entry?.expiresAt })).at).toBe(END_OF_TIME);
    expect(Number.isNaN(new Date(END_OF_TIME).getTime())).toBe(false);
  });
});

describe("what a host waits for", () => {
  it("waits in settled() for a refresh a stale read started, and for what it writes", async () => {
    const time = clock();
    const written: Map<string, $FlowFixMe> = new Map();
    const provider = {
      name: "map",
      read: async (key: string) => written.get(key) ?? null,
      write: async (key: string, entry: mixed) => {
        written.set(key, entry);
      },
      remove: async (key: string) => {
        written.delete(key);
      },
      invalidateTag: async () => 0,
      invalidatePath: async () => 0,
      clear: async () => {
        written.clear();
      },
    };
    const store = createCacheStore({
      now: time.now,
      provider,
      build: "build-one",
      onError: () => {},
    });
    let release: () => void = () => {};
    const held = new Promise<void>((resolve) => {
      release = resolve;
    });
    let renders = 0;
    const render = async () => {
      renders += 1;
      const count = renders;
      if (count > 1) await held;
      cacheLife({ revalidate: 60 });
      return `render ${count}`;
    };
    const asked = { key: ["page"], staleUntilReplaced: true };

    await store.resolve(asked, render);
    await store.settled();
    time.advance(61);
    expect((await store.resolve(asked, render)).outcome).toBe("stale");

    let done = false;
    const waiting = store.settled().then(() => {
      done = true;
    });
    await settled();
    // A Worker hands this promise to `waitUntil` and a Lambda awaits it, so a
    // store that resolved it now would let either stop the refresh halfway.
    expect(done).toBe(false);

    release();
    await waiting;
    expect(
      Array.from(written.values()).some((entry) => String(entry.value).includes("render 2")),
    ).toBe(true);
  });
});

describe("what a prerender declared", () => {
  it("is read without storing anything, once per tag", async () => {
    const declared = await collectCacheDeclarations(async () => {
      cacheLife({ revalidate: 300 });
      cacheLife({ revalidate: 60, expire: 600 });
      cacheTag("posts", "posts");
      cacheTag("authors");
      return "<p>page</p>";
    });

    expect(declared).toEqual({
      value: "<p>page</p>",
      lifetime: { revalidate: 60, expire: 600 },
      tags: ["posts", "authors"],
      denied: null,
    });
  });

  it("carries noStore's reason, which is what keeps a page from regenerating", async () => {
    const declared = await collectCacheDeclarations(async () => {
      cacheLife({ revalidate: 60 });
      noStore("the page read the session");
      return null;
    });

    expect(declared.denied).toBe("the page read the session");
  });
});

describe("a regenerated page", () => {
  it("is the build's document through a Node front door until its lifetime passes", async () => {
    const time = clock();
    const store = createCacheStore({ now: time.now, onError: () => {} });
    const app = regeneratingApp();
    const handle = createServeHandler({
      staticDir: buildDirectory(),
      handle: createFetchHandler({
        app,
        document: assets,
        cache: { store, route: true, fetch: false },
        regeneration: manifestFor(time.now()),
      }),
    });

    const first = await serve(handle, "/clock");
    expect(await first.text()).toContain("the build's copy");
    expect(first.headers.get("x-uf-cache")).toBe("HIT");
    expect(first.headers.get("content-type")).toContain("text/html");
    // The same page, however it was spelled.
    expect(await (await serve(handle, "/clock/")).text()).toContain("the build's copy");
    expect(await (await serve(handle, "/clock?ref=feed")).text()).toContain("the build's copy");
    expect(app.renders).toEqual([]);

    time.advance(61);
    const stale = await serve(handle, "/clock");
    expect(await stale.text()).toContain("the build's copy");
    expect(stale.headers.get("x-uf-cache")).toBe("STALE");
    await settled();
    expect(app.renders).toEqual(["/clock"]);

    const regenerated = await serve(handle, "/clock");
    expect(await regenerated.text()).toContain("rendered /clock");
    expect(regenerated.headers.get("x-uf-cache")).toBe("HIT");
  });

  it("is rendered on its first request where no front door offers the build's files", async () => {
    const app = regeneratingApp();
    const handle = createFetchHandler({
      app,
      document: assets,
      cache: { store: createCacheStore(), route: true, fetch: false },
      regeneration: manifestFor(Date.now()),
    });

    const response = await serve(handle, "/clock");
    expect(await response.text()).toContain("rendered /clock");
    expect(response.headers.get("x-uf-cache")).toBe("MISS");
  });

  it("goes back to a render, not to the build's copy, when its tag is revalidated", async () => {
    const time = clock();
    const store = createCacheStore({ now: time.now, onError: () => {} });
    const app = regeneratingApp();
    const handle = createServeHandler({
      staticDir: buildDirectory(),
      handle: createFetchHandler({
        app,
        document: assets,
        cache: { store, route: true, fetch: false },
        regeneration: manifestFor(time.now()),
      }),
    });

    expect(await (await serve(handle, "/clock")).text()).toContain("the build's copy");
    expect(store.revalidateTag("clock")).toBe(1);

    const after = await serve(handle, "/clock");
    expect(await after.text()).toContain("rendered /clock");
    expect(after.headers.get("x-uf-cache")).toBe("MISS");
  });

  it("is the build's document through a Worker, and keeps what it regenerates in KV", async () => {
    const time = clock();
    const kv = fakeNamespace();
    const store = createCacheStore({
      now: time.now,
      provider: createKvCache(),
      build: "build-one",
      onError: () => {},
    });
    const app = regeneratingApp();
    const handle = createWorkerFetch({
      handle: createFetchHandler({
        app,
        document: assets,
        cache: { store, route: true, fetch: false },
        regeneration: manifestFor(time.now()),
      }),
      beginRequest,
    });
    const asked: Array<string> = [];
    const env = {
      UF_CACHE: kv,
      ASSETS: {
        fetch: async (asset: Request): Promise<Response> => {
          const pathname = new URL(asset.url).pathname;
          asked.push(pathname);
          return pathname === "/__uf/regenerate/clock/"
            ? new Response("<!doctype html><p>the build's copy</p>", {
                headers: { "content-type": "text/html" },
              })
            : new Response("not found", { status: 404 });
        },
      },
    };
    const waiting: Array<Promise<mixed>> = [];
    const context = { waitUntil: (promise: Promise<mixed>) => waiting.push(promise) };
    const answer = async (): Promise<Response> => {
      const response = await handle(request("/clock"), env, context);
      await Promise.all(waiting);
      return response;
    };

    const first = await answer();
    expect(await first.text()).toContain("the build's copy");
    expect(first.headers.get("x-uf-cache")).toBe("HIT");
    // The page's own URL first, which has no file, then the build's document.
    expect(asked).toEqual(["/clock", "/__uf/regenerate/clock/"]);

    time.advance(61);
    expect((await answer()).headers.get("x-uf-cache")).toBe("STALE");
    await settled();
    await Promise.all(waiting);
    await store.settled();

    const kept = Array.from(kv.values.keys());
    expect(kept.some((key) => key.startsWith("uf/entry/"))).toBe(true);
    expect(kept.some((key) => key.startsWith("uf/tag/clock/"))).toBe(true);
    expect(kept.some((key) => key.startsWith("uf/path/%2Fclock/"))).toBe(true);
  });
});

/**
 * A Workers KV namespace that is a `Map`, listing one key per page so the
 * cursor is followed rather than assumed away.
 */
function fakeNamespace(): $FlowFixMe {
  const values: Map<string, string> = new Map();
  const puts: Array<{| key: string, options: mixed |}> = [];
  return {
    values,
    puts,
    async get(key: string) {
      return values.get(key) ?? null;
    },
    async put(key: string, value: string, options?: mixed) {
      values.set(key, value);
      puts.push({ key, options });
    },
    async delete(key: string) {
      values.delete(key);
    },
    async list(options: {| prefix: string, cursor?: string |}) {
      const names = Array.from(values.keys())
        .filter((name) => name.startsWith(options.prefix))
        .sort();
      const at = options.cursor == null ? 0 : Number(options.cursor);
      const page = names.slice(at, at + 1).map((name) => ({ name }));
      const next = at + 1;
      return next >= names.length
        ? { keys: page, list_complete: true }
        : { keys: page, list_complete: false, cursor: String(next) };
    },
  };
}

describe("the Workers KV provider", () => {
  it("keeps an entry and an index key per tag and path, and invalidates through them", async () => {
    const kv = fakeNamespace();
    const provider = createKvCache({ namespace: kv });
    const stored = { ...entryOf("value", 1, 2, ["posts", "a/b"]), path: "/posts/a" };
    await provider.write("key-1", stored);
    await provider.write("key-2", { ...entryOf("other", 1, 2, ["posts"]), path: "/posts/b" });

    expect(await provider.read("key-1")).toEqual(stored);
    expect(kv.values.has("uf/tag/a%2Fb/key-1")).toBe(true);
    expect(kv.values.has("uf/path/%2Fposts%2Fa/key-1")).toBe(true);

    expect(await provider.invalidatePath("/posts/a")).toBe(1);
    expect(await provider.read("key-1")).toBe(null);
    expect(await provider.read("key-2")).not.toBe(null);

    // Two index keys under one tag, listed one per page.
    await provider.write("key-1", stored);
    expect(await provider.invalidateTag("posts")).toBe(2);
    expect(await provider.read("key-1")).toBe(null);
    expect(await provider.read("key-2")).toBe(null);

    await provider.write("key-3", entryOf("third", 1, 2));
    await provider.clear();
    expect(kv.values.size).toBe(0);
  });

  it("gives a finite entry an expiration a minute out at least, and a never-ending one none", async () => {
    const kv = fakeNamespace();
    const provider = createKvCache({ namespace: kv });
    await provider.write("forever", entryOf("kept", 1, 2));
    await provider.write("soon", { ...entryOf("brief", 1, 2), expiresAt: Date.now() + 1000 });

    const forever = kv.puts.find((put) => put.key === "uf/entry/forever");
    const soon = kv.puts.find((put) => put.key === "uf/entry/soon");
    expect(forever?.options).toEqual({});
    expect(soon?.options?.expiration).toBeGreaterThanOrEqual(Math.ceil(Date.now() / 1000) + 59);
  });

  it("treats a stored value that is not an entry as nothing", async () => {
    const kv = fakeNamespace();
    kv.values.set("uf/entry/garbage", '{"value": 7}');
    expect(await createKvCache({ namespace: kv }).read("garbage")).toBe(null);
  });

  it("names the binding it could not find, inside a request and outside one", async () => {
    const provider = createKvCache();
    let outside: mixed = null;
    try {
      await provider.read("key");
    } catch (error) {
      outside = error;
    }
    expect(outside).toBeInstanceOf(KvBindingMissingError);
    expect(String(outside)).toContain("outside a request");

    const inside = beginRequest(request("/"));
    let refused: mixed = null;
    try {
      await inside.run(() => provider.read("key"));
    } catch (error) {
      refused = error;
    } finally {
      await inside.settle();
    }
    expect(refused).toBeInstanceOf(KvBindingMissingError);
    expect(String(refused)).toContain("UF_CACHE");
    expect(String(refused)).toContain("kv_namespaces");
  });
});

/** The regenerating app, with the route an application invalidates the page's tag from. */
function invalidatingApp() {
  return {
    ...regeneratingApp(),
    dispatch: async (incoming: Request) =>
      new URL(incoming.url).pathname === "/revalidate"
        ? Response.json({ expired: revalidateTag("clock") })
        : null,
  };
}

/** One server process over a build's files, wired the way `uf start` wires one. */
function nodeProcess(
  time: {| now: () => number, advance: (seconds: number) => void |},
  options: {|
    staticDir: string,
    renderedAt: number,
    provider?: $FlowFixMe,
    build?: string,
    onError?: (error: mixed) => void,
  |},
) {
  const onError = options.onError ?? (() => {});
  const store =
    options.provider == null
      ? createCacheStore({ now: time.now, onError })
      : createCacheStore({
          now: time.now,
          onError,
          provider: options.provider,
          build: options.build ?? "build-one",
        });
  const handle = createServeHandler({
    staticDir: options.staticDir,
    handle: createFetchHandler({
      app: invalidatingApp(),
      document: assets,
      cache: { store, route: true, fetch: false },
      regeneration: manifestFor(options.renderedAt),
    }),
  });
  return { store, handle };
}

/** Invalidate the page's tag through the application, the way a mutation does. */
async function invalidate(handle): Promise<mixed> {
  const asRequest = request("/revalidate", { method: "POST" });
  const { run, settle } = beginRequest(asRequest);
  try {
    return await (await run(() => handle(asRequest))).json();
  } finally {
    await settle();
  }
}

/** A directory for a filesystem provider that several processes share. */
const cacheDirectory = () => fs.mkdtempSync(nodePath.join(os.tmpdir(), "uf-regenerate-cache-"));

describe("an invalidation a restart remembers", () => {
  it("renders a page invalidated before a restart, rather than the build's copy, from a disk", async () => {
    const time = clock();
    const built = time.now();
    const staticDir = buildDirectory();
    const directory = cacheDirectory();

    const before = nodeProcess(time, {
      staticDir,
      renderedAt: built,
      provider: createFilesystemCache({ directory }),
    });
    expect(await (await serve(before.handle, "/clock")).text()).toContain("the build's copy");
    time.advance(1);
    expect(await invalidate(before.handle)).toEqual({ expired: 1 });
    await before.store.settled();

    // Nothing in memory, the same directory, and a build's copy still inside
    // the lifetime it was rendered with.
    const after = nodeProcess(time, {
      staticDir,
      renderedAt: built,
      provider: createFilesystemCache({ directory }),
    });
    const answer = await serve(after.handle, "/clock");
    expect(await answer.text()).toContain("rendered /clock");
    expect(answer.headers.get("x-uf-cache")).toBe("MISS");
  });

  it("renders it in a process that never served it, beside the one that invalidated it", async () => {
    const time = clock();
    const built = time.now();
    const staticDir = buildDirectory();
    const directory = cacheDirectory();
    const one = nodeProcess(time, {
      staticDir,
      renderedAt: built,
      provider: createFilesystemCache({ directory }),
    });
    const two = nodeProcess(time, {
      staticDir,
      renderedAt: built,
      provider: createFilesystemCache({ directory }),
    });

    time.advance(1);
    expect(one.store.revalidatePath("/clock")).toBe(0);
    await one.store.settled();

    const answer = await serve(two.handle, "/clock");
    expect(await answer.text()).toContain("rendered /clock");
    expect(answer.headers.get("x-uf-cache")).toBe("MISS");
  });

  it("renders it where it was invalidated before anybody asked, with no durable store", async () => {
    const time = clock();
    const server = nodeProcess(time, { staticDir: buildDirectory(), renderedAt: time.now() });
    time.advance(1);
    expect(server.store.revalidateTag("clock")).toBe(0);

    const answer = await serve(server.handle, "/clock");
    expect(await answer.text()).toContain("rendered /clock");
    expect(answer.headers.get("x-uf-cache")).toBe("MISS");
  });

  it("weighs an invalidation against when a build rendered, whichever build wrote it down", async () => {
    const time = clock();
    const staticDir = buildDirectory();
    const directory = cacheDirectory();
    const renderedBefore = time.now();
    time.advance(1);
    const serving = nodeProcess(time, {
      staticDir,
      renderedAt: renderedBefore,
      provider: createFilesystemCache({ directory }),
    });
    serving.store.revalidateTag("clock");
    await serving.store.settled();

    // Built before the invalidation and deployed after it, as a deploy is.
    const deployed = nodeProcess(time, {
      staticDir,
      renderedAt: renderedBefore,
      provider: createFilesystemCache({ directory }),
      build: "build-two",
    });
    expect((await serve(deployed.handle, "/clock")).headers.get("x-uf-cache")).toBe("MISS");

    time.advance(1);
    const rebuilt = nodeProcess(time, {
      staticDir,
      renderedAt: time.now(),
      provider: createFilesystemCache({ directory }),
      build: "build-three",
    });
    const answer = await serve(rebuilt.handle, "/clock");
    expect(await answer.text()).toContain("the build's copy");
    expect(answer.headers.get("x-uf-cache")).toBe("HIT");
  });

  it("renders rather than serve the build's copy when the store cannot say", async () => {
    const time = clock();
    const failures: Array<mixed> = [];
    const unreachable = {
      name: "unreachable",
      read: async () => {
        throw new Error("the store is unreachable");
      },
      write: async () => {},
      remove: async () => {},
      invalidateTag: async () => 0,
      invalidatePath: async () => 0,
      clear: async () => {},
    };
    const server = nodeProcess(time, {
      staticDir: buildDirectory(),
      renderedAt: time.now(),
      provider: unreachable,
      onError: (error) => failures.push(error),
    });

    const answer = await serve(server.handle, "/clock");
    expect(await answer.text()).toContain("rendered /clock");
    expect(failures.map(String)).toContain("Error: the store is unreachable");
  });

  it("renders a page invalidated before a Worker restarted, from Workers KV", async () => {
    const time = clock();
    const built = time.now();
    const kv = fakeNamespace();
    const files = {
      fetch: async (asset: Request): Promise<Response> =>
        new URL(asset.url).pathname === "/__uf/regenerate/clock/"
          ? new Response("<!doctype html><p>the build's copy</p>", {
              headers: { "content-type": "text/html" },
            })
          : new Response("not found", { status: 404 }),
    };
    const worker = () => {
      const store = createCacheStore({
        now: time.now,
        provider: createKvCache(),
        build: "build-one",
        onError: () => {},
      });
      const handle = createWorkerFetch({
        handle: createFetchHandler({
          app: invalidatingApp(),
          document: assets,
          cache: { store, route: true, fetch: false },
          regeneration: manifestFor(built),
        }),
        beginRequest,
      });
      return async (url: string, init?: mixed): Promise<Response> => {
        const waiting: Array<Promise<mixed>> = [];
        const response = await handle(
          request(url, init),
          { UF_CACHE: kv, ASSETS: files },
          { waitUntil: (promise: Promise<mixed>) => waiting.push(promise) },
        );
        await Promise.all(waiting);
        return response;
      };
    };

    const before = worker();
    expect(await (await before("/clock")).text()).toContain("the build's copy");
    time.advance(1);
    expect(await (await before("/revalidate", { method: "POST" })).json()).toEqual({ expired: 1 });

    const after = worker();
    const answer = await after("/clock");
    expect(await answer.text()).toContain("rendered /clock");
    expect(answer.headers.get("x-uf-cache")).toBe("MISS");
  });
});
