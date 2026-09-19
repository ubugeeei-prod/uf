// @flow
import { describe, expect, it } from "@uniflowed/test";
import { cacheFunction, cacheTag, createCacheStore, updateTag } from "./cache.js";
import { cookies } from "./index.js";
import { beginRequest } from "./host.js";
import { dataKey } from "./internal/data-key.js";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { createFilesystemCache } from "./cache-filesystem.js";

describe("function data caches", () => {
  it("restores function values from the selected durable provider after a restart", async () => {
    const directory = fs.mkdtempSync(path.join(os.tmpdir(), "uf-function-cache-"));
    try {
      let calls = 0;
      const first = createCacheStore({
        provider: createFilesystemCache({ directory }),
        build: "one",
      });
      const options = { lifetime: { revalidate: 60 }, store: first };
      expect(await cacheFunction("public", async () => ++calls, options)()).toBe(1);
      await first.settled();
      const second = createCacheStore({
        provider: createFilesystemCache({ directory }),
        build: "one",
      });
      expect(
        await cacheFunction("public", async () => ++calls, { ...options, store: second })(),
      ).toBe(1);
      const nextBuild = createCacheStore({
        provider: createFilesystemCache({ directory }),
        build: "two",
      });
      expect(
        await cacheFunction("public", async () => ++calls, { ...options, store: nextBuild })(),
      ).toBe(2);
      await nextBuild.settled();
    } finally {
      fs.rmSync(directory, { recursive: true, force: true });
    }
  });
  it("shares one function across routes, keys arguments, and expires by its lifetime", async () => {
    let now = 0;
    let calls = 0;
    const store = createCacheStore({ now: () => now });
    const product = cacheFunction("product", async (id: string) => `${id}:${++calls}`, {
      lifetime: { revalidate: 1 },
    });
    async function from(path: string, id: string) {
      const request = beginRequest(new Request(`https://app.test${path}`));
      request.context.cache = { store, data: true };
      return request.run(() => product(id));
    }
    expect(await from("/catalog", "a")).toBe("a:1");
    expect(await from("/search", "a")).toBe("a:1");
    expect(await from("/search", "b")).toBe("b:2");
    now = 1000;
    expect(await from("/catalog", "a")).toBe("a:3");
  });

  it("uses an explicit provider store and honors the data switch otherwise", async () => {
    let calls = 0;
    const first = createCacheStore();
    const second = createCacheStore();
    const options = { lifetime: { revalidate: 60 } };
    const implicit = cacheFunction("implicit", async () => ++calls, options);
    const explicit = cacheFunction("explicit", async () => ++calls, { ...options, store: second });
    const request = beginRequest(new Request("https://app.test"));
    request.context.cache = { store: first, data: false };
    await request.run(async () => {
      expect(await implicit()).toBe(1);
      expect(await implicit()).toBe(2);
      expect(await explicit()).toBe(3);
      expect(await explicit()).toBe(3);
    });
    expect(first.size()).toBe(0);
    expect(second.size()).toBe(1);
  });

  it("keeps concurrent callers coalesced and function identities separate", async () => {
    const store = createCacheStore();
    let calls = 0;
    const options = { lifetime: { revalidate: 60 }, store };
    const first = cacheFunction("first", async () => ++calls, options);
    const second = cacheFunction("second", async () => ++calls, options);
    expect(await Promise.all([first(), first(), second()])).toEqual([1, 1, 2]);
  });

  it("invalidates declared and dynamically collected tags", async () => {
    const store = createCacheStore();
    let value = 1;
    const read = cacheFunction(
      "catalog",
      async () => {
        cacheTag("stock");
        return value;
      },
      { lifetime: { revalidate: 60 }, tags: ["catalog"], store },
    );
    expect(await read()).toBe(1);
    value = 2;
    expect(store.revalidateTag("stock")).toBe(1);
    expect(await read()).toBe(2);
    value = 3;
    const request = beginRequest(new Request("https://app.test"));
    request.context.cache = { store, data: true };
    await request.run(() => updateTag("catalog"));
    expect(await read()).toBe(3);
  });

  it("does not reuse or retain a fill that started before a mutation", async () => {
    const store = createCacheStore();
    let release = () => {};
    const waiting = new Promise<void>((resolve) => {
      release = resolve;
    });
    let started = () => {};
    const ready = new Promise<void>((resolve) => {
      started = resolve;
    });
    let value = 1;
    const read = cacheFunction(
      "catalog",
      async () => {
        const captured = value;
        started();
        if (captured === 1) await waiting;
        return captured;
      },
      { lifetime: { revalidate: 60 }, tags: ["catalog"], store },
    );
    const old = read();
    await ready;
    value = 2;
    store.revalidateTag("catalog");
    expect(await read()).toBe(2);
    release();
    expect(await old).toBe(1);
    expect(await read()).toBe(2);
  });

  it("rejects indirect ambient request reads before coalescing can expose private data", async () => {
    const store = createCacheStore();
    const helper = () => cookies().get("session");
    const read = cacheFunction("private", async () => helper(), {
      lifetime: { revalidate: 60 },
      store,
    });
    async function from(session: string) {
      const request = beginRequest(
        new Request("https://app.test", { headers: { cookie: `session=${session}` } }),
      );
      return request.run(() => read());
    }
    const results = await Promise.allSettled([from("alice"), from("bob")]);
    expect(results.every((result) => result.status === "rejected")).toBe(true);
    await expect(from("eve")).rejects.toThrow('cached function "private" cannot read cookies()');
    expect(store.size()).toBe(0);
  });
});

describe("cached arguments", () => {
  it("canonicalizes object order without collapsing distinct values", () => {
    expect(dataKey([{ a: 1, b: 2 }])).toBe(dataKey([{ b: 2, a: 1 }]));
    const values = [[], [null], [undefined], [0], [-0], ["0"], [{}], [[]], [false]];
    expect(new Set(values.map(dataKey)).size).toBe(values.length);
  });

  it("rejects cycles, accessors and non-data without evaluating user code", () => {
    let calls = 0;
    const accessor = {};
    Object.defineProperty(accessor, "secret", {
      enumerable: true,
      get: () => {
        calls += 1;
        return 1;
      },
    });
    const cycle: Array<mixed> = [];
    cycle.push(cycle);
    const inherited = [];
    Object.setPrototypeOf(inherited, { 0: "private" });
    for (const value of [accessor, cycle, inherited, new Date(), () => 1, Infinity, Symbol("x")]) {
      expect(() => dataKey([value])).toThrow();
    }
    expect(calls).toBe(0);
  });
});
