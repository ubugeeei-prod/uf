// @flow
//
// Internal to `@uniflowed/server`: what makes two lookups the same lookup.
//
// A cache key here is a list of **strings**, reduced to one string that is the
// entry's identity. That is narrower than the two keys this repository already
// has, and the narrowing is the whole design decision, so it is argued rather
// than asserted.
//
// # Where it departs from `@uniflowed/query/key`, and why
//
// `hashKey` there takes `$ReadOnlyArray<mixed>` and serialises it with object
// members sorted, because a *client* key is written inline in two components
// by two people and `["users", {page: 1, size: 20}]` has to be the same entry
// as `["users", {size: 20, page: 1}]`. Its own header then spends a paragraph
// on what that admits: a `Date` or a class instance serialises through
// whatever `toJSON` it happens to have, `1` and `"1"` are deliberately
// different entries, and both are traps you are told to avoid rather than
// prevented from writing.
//
// A server cache key is not written that way. It is built by uf — a method, a
// pathname and a query string — or by a caller naming a URL it is about to
// fetch. Every one of those is already a string, so admitting `mixed` would
// buy nothing and would import the whole `toJSON` question into a cache whose
// wrong answers are *other people's data*. Strings only, and a key that is not
// a list of strings is a `TypeError` at the call rather than an entry filed
// under a name nobody can predict.
//
// Prefix matching does not come across either, and for a reason rather than an
// omission: invalidation here is by **tag**, not by key shape. A caller that
// wants "everything about users" gone says `cacheTag("users")` when the entry
// is filled and `revalidateTag("users")` when it changes, which is a statement
// about meaning; `matchesKey`'s structural prefix is a statement about
// spelling, and the two only agree while one person is writing both ends.
//
// # Where it departs from the two disk caches, and why
//
// `crates/uf_check`'s cache and `@uniflowed/host`'s transform cache both put
// **the identity of the `uf` that produced the entry** in the key, and both
// headers say the same thing about it: a content-addressed cache has no
// invalidation to get wrong only if every other input is a constant, and the
// largest one is the compiler. Neither of them can leave it out, because both
// outlive the process that wrote them — the entry on disk is read by the next
// build, which may be a different build.
//
// This one cannot outlive the process, so the identity of the code that
// produced an entry is fixed for the whole life of the store and there is
// nothing to put in the key. That is not a shortcut around their lesson; it is
// the same lesson pointing the other way, and it is the reason the store is in
// memory and stays there until something durable exists to hold it. The day an
// adapter offers a store that survives a restart, this key gains a generation
// — the build id — before that store is written to, because on that day the
// entry outlives the build and every word of those two headers applies.

/** A cache key, as a caller writes it: `["route", "GET", "/posts"]`. */
export type CacheKey = $ReadOnlyArray<string>;

/**
 * A key as one string, exactly and totally.
 *
 * `JSON.stringify` over an array of strings is unambiguous — the quoting and
 * escaping are what separate the members — so two different keys cannot
 * collide and one key cannot spell itself two ways. Joining with a separator
 * would do neither: a path containing the separator is a second key wearing
 * the first one's name, and on a route cache that is one URL served the
 * document of another.
 */
export function hashCacheKey(key: CacheKey): string {
  for (let index = 0; index < key.length; index += 1) {
    if (typeof key[index] !== "string") {
      throw new TypeError(
        `@uniflowed/server: a cache key is a list of strings, and member ${index} is ` +
          `${typeof key[index]}. Spell it out — String(id) — rather than leaving the ` +
          "entry's name to whatever serialisation the value happens to have.",
      );
    }
  }
  return JSON.stringify(key);
}
