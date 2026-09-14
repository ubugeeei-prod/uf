// @flow
//
// `@uniflowed/server/cache/kv`: a durable cache provider over Workers KV.
//
// The store `uf build --adapter edge` links for a build that regenerates pages
// and names no `rendering.cache.store` of its own, and a store any Worker can
// name: `rendering.cache.store: "@uniflowed/server/cache/kv"` goes through the
// same module seam a Redis would, because this module exports
// `createCacheProvider` beside [`createKvCache`].
//
// # Why KV and not the Cache API
//
// A Worker has two places to keep bytes past its isolate without a database,
// and the provider seam decides between them. `./internal/cache-provider.js`
// asks for six methods, and two of them are `invalidateTag` and
// `invalidatePath`: find every entry filled under this tag, or for this path,
// and take it out. The Cache API cannot answer that. It is a map from request
// to response with no way to enumerate what it holds, so an invalidation could
// only remove keys it already knew — and a page regenerated in another isolate
// is exactly the key it does not know. It is also per data centre: an entry
// written in one location is invisible in every other, so a regenerated page
// would be regenerated again in each location that served it.
//
// KV lists by prefix, which makes tag and path invalidation a list and a
// delete, and it is one namespace for every location, so a page regenerated
// anywhere is read everywhere. The cost is the one KV documents: a write takes
// up to a minute to be seen in other locations, and a `list` is eventually
// consistent too. So a page regenerated in Tokyo may be regenerated once more
// in Frankfurt inside that minute, and an invalidation that follows a write
// within it may miss that write. Both are bounded by the platform's own number
// rather than uf's, and both err towards a render rather than a wrong page for
// longer than the page's lifetime allows.
//
// # The namespace comes from the request
//
// A Worker is handed its bindings as `env`, per request, and nowhere else. A
// provider is constructed once, at module load, before any request exists, so
// it cannot be handed the namespace: it looks it up on every call, in the
// request that is using it, through the bindings `./edge.js` puts on the
// request context. A background refresh runs inside the request that started
// it, so it finds the same binding.
//
// # How it is laid out
//
// Three kinds of key under one prefix, `uf/` unless told otherwise:
//
// * `uf/entry/<key>`: the entry, as JSON.
// * `uf/tag/<tag>/<key>`: an empty value per tag the entry was filled under.
// * `uf/path/<path>/<key>`: the same for its path.
//
// The tag and the path are percent-encoded, so a `/` in either cannot move the
// boundary between it and the key. An index key outlives the entry it points
// at when the entry is replaced or removed, and that is harmless: an
// invalidation deletes the entry it names whether or not it is still there.

import { END_OF_TIME } from "./internal/cache-store.js";
import type { CacheProvider, DurableCacheEntry } from "./internal/cache-provider.js";
import { currentContext } from "./internal/context.js";

export type { CacheProvider, DurableCacheEntry } from "./internal/cache-provider.js";

/** The binding `uf build --adapter edge` writes into `wrangler.json`. */
export const KV_BINDING = "UF_CACHE";

/**
 * The part of a Workers KV namespace this provider uses.
 *
 * Declared structurally, as `./edge.js` declares the assets binding: the
 * runtime supplies the object, and naming the whole of Cloudflare's type here
 * would be this package holding a copy of another project's types.
 */
export type KvNamespace = {
  get(key: string, type: "text"): Promise<string | null>,
  put(key: string, value: string, options?: {| expiration?: number |}): Promise<void>,
  delete(key: string): Promise<void>,
  list(options: {| prefix: string, cursor?: string |}): Promise<{
    +keys: $ReadOnlyArray<{ +name: string, ... }>,
    +list_complete: boolean,
    +cursor?: string,
    ...
  }>,
  ...
};

/** How a KV provider finds its namespace and names its keys. */
export type KvCacheOptions = {|
  /** The binding the namespace is under. [`KV_BINDING`] unless given. */
  readonly binding?: string,
  /**
   * A namespace to use instead of the request's binding.
   *
   * For a test, and for a host that is not a Worker but has a KV client of its
   * own. A Worker passes nothing and is answered from `env`.
   */
  readonly namespace?: KvNamespace,
  /** What every key starts with, so one namespace can hold more than this. */
  readonly prefix?: string,
|};

/** Raised when a Worker has no namespace under the binding a provider was told to use. */
export class KvBindingMissingError extends Error {
  /** The binding that was looked for. */
  binding: string;

  constructor(binding: string, insideRequest: boolean) {
    super(
      insideRequest
        ? `@uniflowed/server: the KV cache provider reads the namespace bound as ${binding}, and ` +
            "this Worker has no such binding. `uf build --adapter edge` writes it into " +
            'wrangler.json as `"kv_namespaces": [{ "binding": "' +
            binding +
            '" }]` for a build that regenerates pages; a wrangler.json of your own needs the same ' +
            "entry."
        : `@uniflowed/server: the KV cache provider was used outside a request. A Worker's ` +
            `bindings, ${binding} among them, exist only inside the request they were handed to.`,
    );
    this.name = "KvBindingMissingError";
    this.binding = binding;
  }
}

/** A durable cache provider over the Workers KV namespace `options` names. */
export function createKvCache(options?: KvCacheOptions): CacheProvider {
  const binding = options?.binding ?? KV_BINDING;
  const prefix = options?.prefix ?? "uf/";
  const given = options?.namespace ?? null;

  const namespace = (): KvNamespace => {
    if (given != null) return given;
    const context = currentContext();
    const bound = context?.bindings?.[binding];
    if (bound == null || typeof bound !== "object") {
      throw new KvBindingMissingError(binding, context != null);
    }
    return (bound: $FlowFixMe);
  };
  const entryKey = (key: string) => `${prefix}entry/${key}`;
  const tagPrefix = (tag: string) => `${prefix}tag/${encodeURIComponent(tag)}/`;
  const pathPrefix = (path: string) => `${prefix}path/${encodeURIComponent(path)}/`;

  /** Delete every entry an index prefix points at, and the index keys. */
  const dropIndexed = async (kv: KvNamespace, index: string): Promise<number> => {
    const names = await keysUnder(kv, index);
    await Promise.all(
      names.flatMap((name) => [kv.delete(entryKey(name.slice(index.length))), kv.delete(name)]),
    );
    return names.length;
  };

  return {
    name: `workers-kv:${binding}`,
    async read(key: string): Promise<DurableCacheEntry | null> {
      const text = await namespace().get(entryKey(key), "text");
      return text == null ? null : parseEntry(text);
    },
    async write(key: string, entry: DurableCacheEntry): Promise<void> {
      const kv = namespace();
      const expiring = expirationFor(entry.expiresAt);
      await kv.put(entryKey(key), JSON.stringify(entry), expiring);
      await Promise.all([
        ...entry.tags.map((tag) => kv.put(`${tagPrefix(tag)}${key}`, "", expiring)),
        ...(entry.path == null ? [] : [kv.put(`${pathPrefix(entry.path)}${key}`, "", expiring)]),
      ]);
    },
    async remove(key: string): Promise<void> {
      await namespace().delete(entryKey(key));
    },
    invalidateTag(tag: string): Promise<number> {
      return dropIndexed(namespace(), tagPrefix(tag));
    },
    invalidatePath(path: string): Promise<number> {
      return dropIndexed(namespace(), pathPrefix(path));
    },
    async clear(): Promise<void> {
      const kv = namespace();
      await Promise.all((await keysUnder(kv, prefix)).map((name) => kv.delete(name)));
    },
  };
}

/**
 * The module seam's spelling of [`createKvCache`].
 *
 * `rendering.cache.store` names a module exporting `createCacheProvider`, and
 * this is that export: the binding is [`KV_BINDING`] and the directory, which
 * means nothing to a namespace, is ignored.
 */
export function createCacheProvider(_options?: {|
  readonly build?: string,
  readonly directory?: string,
|}): CacheProvider {
  return createKvCache();
}

/** Every key under `prefix`, following the cursor to the end of the listing. */
async function keysUnder(kv: KvNamespace, prefix: string): Promise<Array<string>> {
  const names: Array<string> = [];
  let cursor: string | void;
  for (;;) {
    const page = await kv.list(cursor == null ? { prefix } : { prefix, cursor });
    for (const key of page.keys) names.push(key.name);
    if (page.list_complete || page.cursor == null) return names;
    cursor = page.cursor;
  }
  return names;
}

/**
 * The `expiration` a write carries, in seconds since the epoch, or nothing.
 *
 * Nothing for an entry that never expires. Otherwise the entry's own
 * `expiresAt`, and never sooner than a minute from now, which is the earliest
 * KV accepts. Letting the platform keep an entry a little past its end is
 * harmless: the store judges an entry by its timestamps, not by whether a
 * provider still had it.
 */
function expirationFor(expiresAt: number): {| expiration?: number |} {
  if (expiresAt >= END_OF_TIME) return {};
  return {
    expiration: Math.max(Math.ceil(expiresAt / 1000), Math.ceil(Date.now() / 1000) + 60),
  };
}

/** A stored entry, or `null` for text that is not one. */
function parseEntry(text: string): DurableCacheEntry | null {
  let record: mixed;
  try {
    record = JSON.parse(text);
  } catch {
    return null;
  }
  if (record == null || typeof record !== "object" || Array.isArray(record)) return null;
  const { value, storedAt, revalidateAt, expiresAt, tags, path } = record;
  if (
    typeof value !== "string" ||
    typeof storedAt !== "number" ||
    typeof revalidateAt !== "number" ||
    typeof expiresAt !== "number" ||
    !Array.isArray(tags) ||
    !tags.every((tag) => typeof tag === "string") ||
    (path != null && typeof path !== "string")
  ) {
    return null;
  }
  return {
    value,
    storedAt,
    revalidateAt,
    expiresAt,
    tags: tags.map(String),
    path: path == null ? null : path,
  };
}
