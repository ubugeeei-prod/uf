// @flow
//
// `@uniflowed/query/key`: what makes two requests the same request.
//
// Everything else in this package is downstream of one decision: when two
// components ask for `["user", 1]`, is that one entry or two? The answer has
// to be a *value* comparison — the arrays are written inline in two different
// files and will never be the same object — so a key is reduced to a string
// and the string is the identity.
//
// # Why the hash is order-stable
//
// `JSON.stringify` of the array would be enough if every caller wrote object
// members in the same order. They do not: `["users", {page: 1, size: 20}]` and
// `["users", {size: 20, page: 1}]` are the same request written by two people,
// and hashing them apart means two cache entries, two requests, and two
// answers that can disagree on screen. So object keys are sorted as the walk
// serialises them.
//
// Arrays are *not* sorted — order is meaning there — and neither is anything
// else. A `Date` or a class instance in a key serialises through its own
// `toJSON`, which is a reasonable default and a bad idea to rely on; keys
// should be built from strings, numbers and plain records.
//
// # Why `1` and `"1"` are different keys
//
// They come from different code paths — a route parameter is a string, a
// database id is a number — and treating them as one entry means a component
// reading `["user", 1]` is shown the answer fetched for `["user", "1"]`. That
// is a bug that surfaces as *the wrong data*, never as an exception, which is
// the worst failure mode a cache has. `JSON.stringify` keeps them apart for
// free, so the cheap behaviour is also the correct one.
//
// # Why matching is by prefix
//
// Invalidation is written as `["users"]` and has to reach `["users", 1]` and
// `["users", 2]` without the caller enumerating them, because after creating a
// user the caller does not know which ids are cached. [`matchesKey`] is that
// rule, and it is structural rather than a string `startsWith` on the hash:
// the string form has no idea where one member ends and the next begins, so
// `["user"]` would match `["users", 1]` for a large enough coincidence of
// punctuation. Comparing the arrays cannot make that mistake.

import { isPlainObject } from "./structural.js";

/** A key, as a caller writes it: `["user", id]`. */
export type QueryKey = $ReadOnlyArray<mixed>;

/**
 * A key as a string, stable across member order inside object members.
 *
 * Two keys with the same hash are the same request by definition. That is the
 * contract every other module in this package relies on, including the one
 * that decides a component does not need to resubscribe.
 */
export function hashKey(key: QueryKey): string {
  return JSON.stringify(key, (_name, value) =>
    isPlainObject(value)
      ? Object.keys(value)
          .sort()
          .reduce((sorted: $FlowFixMe, name: string) => {
            sorted[name] = (value as $FlowFixMe)[name];
            return sorted;
          }, {})
      : value,
  );
}

/**
 * Whether `key` is described by `pattern`.
 *
 * By default `pattern` is a prefix: `["users"]` matches `["users", 1]`, and
 * `["users", {done: true}]` matches `["users", {done: true, page: 2}]` —
 * object members match partially, so a filter can name the parts of a
 * parameter record it cares about and ignore the rest.
 *
 * With `exact`, the two must hash identically. That is the option for "this
 * one entry and not the list it belongs to", which is the difference between
 * refreshing a row and refetching a table.
 */
export function matchesKey(key: QueryKey, pattern: QueryKey, exact: boolean = false): boolean {
  if (exact) {
    return hashKey(key) === hashKey(pattern);
  }
  if (pattern.length > key.length) {
    return false;
  }
  for (let index = 0; index < pattern.length; index += 1) {
    if (!partiallyMatches(key[index], pattern[index])) {
      return false;
    }
  }
  return true;
}

/**
 * Whether `value` satisfies everything `pattern` states about it.
 *
 * Object patterns are partial — they constrain the members they name — while
 * arrays must line up member for member. The asymmetry is deliberate: a record
 * in a key is a bag of parameters and naming one of them is a useful filter,
 * whereas a shorter array is a different list, not a laxer description of one.
 */
function partiallyMatches(value: mixed, pattern: mixed): boolean {
  if (Object.is(value, pattern)) {
    return true;
  }
  if (Array.isArray(pattern)) {
    if (!Array.isArray(value) || value.length !== pattern.length) {
      return false;
    }
    return pattern.every((member, index) => partiallyMatches(value[index], member));
  }
  if (isPlainObject(pattern)) {
    if (!isPlainObject(value)) {
      return false;
    }
    const record = value as $FlowFixMe;
    return Object.keys(pattern as $FlowFixMe).every((name) =>
      partiallyMatches(record[name], (pattern as $FlowFixMe)[name]),
    );
  }
  return false;
}
