// @flow
//
// Internal to `@uniflowed/server`: the seam a durable cache is written to.
//
// `./cache-store.js` ends by saying what it is not: "in memory, in one
// process… what would fix it is a durable store behind this same seam, which
// is an adapter's job". This file is that seam, and it is deliberately the
// smallest one that can be correct — five methods over strings, no filesystem,
// no Node types, and nothing an adapter has to import from here to implement.
//
// # What a provider is, and what it is not
//
// A provider is a **key-value store of already-encoded entries**. It is not a
// cache: it does not decide what is stale, it does not decide what to evict
// before its own limits force it to, and it never runs a fill. Every one of
// those five answers stays in `./cache-store.js`, where they are written down,
// and a provider that re-answered any of them would be a second cache
// disagreeing with the first.
//
// The division is worth stating the other way round too, because it is what
// makes a three-line Redis provider possible: uf owns *when* an entry may be
// used and *what an entry is on the wire*; the provider owns *where the bytes
// go*. So an adapter writes `GET`, `SET`, `DEL` and two index operations, and
// inherits stale-while-revalidate, coalescing and the request-state refusal
// without having heard of any of them.
//
// # Why uf owns the encoding and the provider does not
//
// The route cache's value is `{status, headers, body}` where `body` is a
// `Uint8Array`, and `JSON.stringify` turns that into `{"0":60,"1":33,…}` — a
// document that round-trips into an object with six thousand numeric keys and
// no error anywhere. Leaving the encoding to each provider would mean every
// provider discovering that separately, and two of them getting it differently
// wrong. So [`encodeCacheValue`] is the one answer, every provider stores the
// string it is given, and a value that cannot survive the trip is **refused at
// the boundary rather than mangled through it**: see [`encodeCacheValue`] for
// the list and for why `Date` is on it.
//
// A refusal is not a failed request. `./cache-store.js` reports it through
// `onError` and keeps the entry in memory — the same tolerance
// `@uniflowed/host`'s transform cache has, for the same reason: a cache that
// cannot be written is a slower answer, never a wrong one.
//
// # The build identity is not here, and that is deliberate
//
// `./cache-key.js` says where it is: "the day an adapter offers a store that
// survives a restart, this key gains a generation — the build id — before that
// store is written to". The key a provider receives has already been through
// [`hashDurableCacheKey`], so it already carries the build that produced the
// entry, and a provider cannot forget to include it because it never had the
// choice. That is why there is no `build` on this type: an input every
// implementation must handle identically is an input none of them should hold.

/** One stored answer, as it survives a process. */
export type DurableCacheEntry = {|
  /** The value, through [`encodeCacheValue`]. Opaque to the provider. */
  readonly value: string,
  readonly storedAt: number,
  readonly revalidateAt: number,
  readonly expiresAt: number,
  /** What `invalidateTag` matches on. */
  readonly tags: $ReadOnlyArray<string>,
  /** What `invalidatePath` matches on. */
  readonly path: string | null,
|};

/**
 * Somewhere entries outlive the process.
 *
 * Five methods, all asynchronous, none of them optional. Asynchronous because
 * the implementations worth having are a disk, a socket and an HTTP request,
 * and an interface that let one of them be synchronous would be an interface
 * the other two cannot satisfy. Not optional because a provider missing one is
 * a cache that silently stops invalidating, which is the failure the whole
 * seam exists to make impossible — a method that is expensive for some
 * implementation is still a method it has to answer, and answering it badly is
 * that implementation's decision to defend rather than uf's to guess.
 *
 * Nothing here throws for a miss. A `read` that finds nothing answers `null`,
 * and a `remove` for a key that is not there is not an error — both are
 * ordinary in a store several processes are writing to at once.
 *
 * A provider *may* throw for a real failure — a disk that is full, a Redis
 * that is gone — and the store treats that the way it treats a failed
 * background refresh: reported through `onError`, and the request answered
 * from the render rather than from the cache.
 */
export type CacheProvider = {
  /**
   * What this provider is, for a log and for `uf explain`.
   *
   * Required so that "the cache is durable" is a claim somebody can check from
   * outside the process rather than a promise in a config file.
   */
  readonly name: string,
  /** The entry under `key`, or `null`. Never throws for a miss. */
  read(key: string): Promise<DurableCacheEntry | null>,
  /** Put `entry` under `key`, replacing whatever was there. */
  write(key: string, entry: DurableCacheEntry): Promise<void>,
  /** Drop `key`. Not an error when there was nothing under it. */
  remove(key: string): Promise<void>,
  /** Expire every entry carrying `tag`. Answers how many went. */
  invalidateTag(tag: string): Promise<number>,
  /** Expire every entry filled for `path`. Answers how many went. */
  invalidatePath(path: string): Promise<number>,
  /** Drop everything. For a test, and for a host tearing a store down. */
  clear(): Promise<void>,
  ...
};

/** Raised when a value cannot be stored durably without being changed. */
export class UnserialisableCacheValueError extends Error {
  /** What was in the way, e.g. `Date`. */
  kind: string;

  constructor(kind: string, at: string) {
    super(
      `@uniflowed/server: a durable cache entry cannot hold ${kind}, at ${at}. ` +
        "An in-memory entry is the value itself and a durable one is a copy of it, so " +
        "anything that would come back as something else is refused here rather than " +
        "stored as whatever JSON happened to make of it. The entry stays in memory.",
    );
    this.name = "UnserialisableCacheValueError";
    this.kind = kind;
  }
}

/** The marker key a tagged member carries. */
const MARKER = "$uf";

/**
 * A value as one string, or a refusal.
 *
 * JSON, with one addition and one rule, over a walk of uf's own rather than
 * `JSON.stringify`'s replacer.
 *
 * The addition is `Uint8Array`, which is what a rendered document's body is and
 * which plain JSON turns into an object of six thousand numeric keys with no
 * error anywhere.
 *
 * The rule is that **anything JSON would change on the way through is
 * refused**. `JSON.stringify` drops a function from an object without saying
 * so, turns a `Date` into a string through its `toJSON`, and turns a `Map` into
 * `{}` — and `./cache-key.js` has already argued this exact point about
 * `@uniflowed/query`'s key: a serialisation that quietly does something is a
 * trap you are told to avoid rather than prevented from writing. In memory none
 * of it matters, because the entry *is* the value. Durably it is the difference
 * between the answer a route was cached with and a different answer wearing the
 * same name, so it throws and `./cache-store.js` keeps the entry in memory
 * instead.
 *
 * # Why the walk is written out and not a replacer
 *
 * Two reasons, and the second one was a bug before it was a reason.
 *
 * A replacer is handed a member *after* `toJSON` has run, so a `Date` arrives
 * as a string and the refusal above cannot see it. Reaching back through the
 * replacer's `this` for the original works and is the sort of thing that is
 * true until somebody tidies it.
 *
 * And escaping has to be top-down. An object that would decode as one of uf's
 * own tagged shapes is wrapped in another one — see [`tagged`] — and
 * `JSON.parse`'s reviver runs *bottom-up*, so it would unwrap the inner shape
 * before it ever saw the wrapper and hand back something that was never stored.
 * A walk that controls its own order has no such corner.
 *
 * `undefined` is not refused: it is a value a loader legitimately produces, and
 * wrapping in `{v: …}` rather than stringifying the value directly is what
 * makes it survive.
 */
export function encodeCacheValue(value: mixed): string {
  return JSON.stringify({ v: pack(value, new Set(), "the value") });
}

/** The value [`encodeCacheValue`] wrote. */
export function decodeCacheValue(text: string): mixed {
  return unpack(JSON.parse(text).v);
}

/** One value on the way out, with everything under it. */
function pack(value: mixed, open: Set<mixed>, at: string): mixed {
  if (value instanceof Uint8Array) {
    return { [MARKER]: "bytes", data: base64Of(value) };
  }
  const refused = unstorable(value);
  if (refused != null) {
    throw new UnserialisableCacheValueError(refused, at);
  }
  if (value == null || typeof value !== "object") {
    return value;
  }
  if (open.has(value)) {
    // `JSON.stringify` raises on a cycle too. Named here so the message is
    // about a cache entry rather than about converting circular structures.
    throw new UnserialisableCacheValueError("a cycle", at);
  }
  open.add(value);
  let packed: mixed;
  if (Array.isArray(value)) {
    packed = value.map((member: mixed, index: number) => pack(member, open, `${at}[${index}]`));
  } else {
    const members: { [string]: mixed } = {};
    for (const key of Object.keys(value)) {
      const member = pack(value[key], open, `${at}.${key}`);
      // What `JSON.stringify` does with an undefined property, kept: a member
      // that is not there and a member that is `undefined` are the same object
      // as far as anything reading this back is concerned.
      if (member !== undefined) members[key] = member;
    }
    packed = tagged(members) ? { [MARKER]: "escaped", data: members } : members;
  }
  open.delete(value);
  return packed;
}

/** One value on the way back in, with everything under it. */
function unpack(node: mixed): mixed {
  if (node == null || typeof node !== "object") {
    return node;
  }
  if (Array.isArray(node)) {
    return node.map(unpack);
  }
  const marker = node[MARKER];
  if (marker === "bytes" && typeof node.data === "string") {
    return bytesOf(node.data);
  }
  if (marker === "escaped" && node.data != null && typeof node.data === "object") {
    // One level, and the members of what is inside rather than the thing
    // itself: what is inside *is* a tagged shape, and looking at it again is
    // exactly the mistake this wrapper exists to prevent.
    return members(node.data);
  }
  return members(node);
}

/** Every member of `node`, unpacked, as a plain object. */
function members(node: $FlowFixMe): { [string]: mixed } {
  const out: { [string]: mixed } = {};
  for (const key of Object.keys(node)) {
    out[key] = unpack(node[key]);
  }
  return out;
}

/**
 * Whether `members` would come back as something uf put there.
 *
 * A JSON API is entitled to a field called `$uf`, and an entry containing one
 * is entitled to be cached, so an object that happens to look like one of the
 * two shapes above is wrapped in an `escaped` one on the way out and unwrapped
 * on the way in. Only the exact shapes count: `{$uf: "posts"}` is nobody's
 * marker and travels untouched.
 */
function tagged(members: { [string]: mixed }): boolean {
  const marker = members[MARKER];
  return marker === "bytes" || marker === "escaped";
}

/**
 * What is in the way of storing `value` durably, or `null`.
 *
 * Named types rather than "not a plain object", so the message says what to do
 * about it. A `Date` in a cached payload becomes an ISO string; storing it and
 * handing back the string is the bug, and `String(date)` at the call is the
 * fix — the same advice `./cache-key.js` gives about a key member.
 */
function unstorable(value: mixed): string | null {
  const type = typeof value;
  if (type === "function") return "a function";
  if (type === "symbol") return "a symbol";
  if (type === "bigint") return "a bigint";
  if (value instanceof Date) return "a Date";
  if (value instanceof Map) return "a Map";
  if (value instanceof Set) return "a Set";
  if (value instanceof RegExp) return "a RegExp";
  return null;
}

/**
 * Bytes as base64, without `Buffer`.
 *
 * `btoa` and a binary string rather than `Buffer.from`, because this module is
 * reached from a worker as readily as from Node and the whole claim of
 * `./fetch.js` is that the application half holds no Node types. Chunked so a
 * megabyte document does not become a million-argument `apply`.
 */
function base64Of(bytes: Uint8Array): string {
  let binary = "";
  const chunk = 0x8000;
  for (let at = 0; at < bytes.length; at += chunk) {
    binary += String.fromCharCode.apply(null, (bytes.subarray(at, at + chunk): $FlowFixMe));
  }
  return btoa(binary);
}

/** The bytes base64 `text` spells. */
function bytesOf(text: string): Uint8Array {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let at = 0; at < binary.length; at += 1) {
    bytes[at] = binary.charCodeAt(at);
  }
  return bytes;
}
