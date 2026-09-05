// @flow
//
// `@uniflowed/validator/collection`: containers whose contents are only known
// when the value arrives.
//
// `object.js` names its keys when the schema is written. These five do not:
// an array has as many items as the payload has, a record has whatever keys it
// was sent, a map and a set have whatever they were built with. So the walk is
// the same in all five — visit every child, keep the ones that parsed, collect
// the issues from the ones that did not, and report all of them — and the only
// thing that differs is what the container is called and how the result is
// rebuilt. `tuple` is here rather than beside `object` for the same reason: its
// positions are its keys.
//
// # Every child is visited, including the ones after a failure
//
// A parse stops at the first bad field in most libraries. This one does not,
// because "row 3 is wrong" followed by "row 7 is wrong" on the next attempt is
// two round trips for information that was available at once — and for a form
// bound to an array of rows it is two renders where one would do.
//
// # Where an issue lands
//
// An array or tuple item is its index, as a decimal string: `["items", "2"]`.
// A record value is its key. A set element is its position in iteration order,
// which is insertion order.
//
// A map is the one that needs two segments. An entry can fail because its key
// was wrong or because its value was, and `["3", "key"]` against `["3",
// "value"]` says which — where a single index would have left the caller
// guessing at exactly the moment it needed to know.
//
// # What crosses a wire, and what does not
//
// `map` and `set` do not survive `JSON.stringify`, and `json-schema.js`
// reports them as unrepresentable rather than pretending. They are here
// because not every boundary is JSON: a value coming out of `structuredClone`,
// out of IndexedDB, or from another module in the same process is a real value
// with a real shape, and a validator that could only describe JSON would be a
// JSON validator.

import type { ItemsInput, ItemsOutput, Options } from "./infer.js";
import type { Issue } from "./issue.js";
import { isPlainObject, ownKeys, ownValue, plainRecord, put } from "./plain-object.js";
import type { Description, Result, Schema } from "./schema.js";
import {
  collectAsync,
  describe,
  fail,
  isAsync,
  makeAsyncSchema,
  makeSchema,
  mergeIssues,
  ok,
  runAt,
  runUnder,
} from "./schema.js";

/** Every item of an array, each parsed by `item`. */
export function array<TOutput, TInput>(
  item: Schema<TOutput, TInput>,
): Schema<$ReadOnlyArray<TOutput>, $ReadOnlyArray<TInput>> {
  const description = (): Description => ({ kind: "array", item: describe(item) });

  if (isAsync(item)) {
    return makeAsyncSchema(async (value, path) => {
      if (!Array.isArray(value)) {
        return fail("type", "expected array", path);
      }
      const collected = await collectAsync(
        value.map((entry, index) => ({ keys: [String(index)], schema: item, value: entry })),
        path,
      );
      if (!collected.ok) {
        return { ok: false, issues: collected.issues };
      }
      // $FlowFixMe[incompatible-type] every element came out of `item`.
      return ok(collected.values as $ReadOnlyArray<TOutput>);
    }, description);
  }

  return makeSchema((value, path) => {
    if (!Array.isArray(value)) {
      return fail("type", "expected array", path);
    }
    const out: Array<TOutput> = [];
    let issues: null | Array<Issue> = null;
    for (let index = 0; index < value.length; index += 1) {
      const result = runAt(item, value[index], path, String(index));
      if (result.ok) {
        out.push(result.value);
      } else {
        issues = issues ?? [];
        mergeIssues(issues, result);
      }
    }
    return issues == null ? ok(out) : { ok: false, issues };
  }, description);
}

/**
 * A fixed number of positions, each with a schema of its own.
 *
 * The arity is part of the type: a payload with one item too many is rejected
 * rather than truncated, because a tuple whose length varied would be an array
 * with extra steps.
 */
export function tuple<TItems extends Options>(
  items: TItems,
): Schema<ItemsOutput<TItems>, ItemsInput<TItems>> {
  const arity = items.length;
  const message = `expected ${String(arity)} tuple items`;
  const description = (): Description => ({
    kind: "tuple",
    items: items.map((item) => describe(item)),
  });

  if (items.some((item) => isAsync(item))) {
    return makeAsyncSchema(async (value, path) => {
      if (!Array.isArray(value)) {
        return fail("type", "expected tuple", path);
      }
      if (value.length !== arity) {
        return fail("length", message, path);
      }
      const collected = await collectAsync(
        items.map((item, index) => ({ keys: [String(index)], schema: item, value: value[index] })),
        path,
      );
      if (!collected.ok) {
        return { ok: false, issues: collected.issues };
      }
      // $FlowFixMe[incompatible-type] position by position, each item's own schema built it.
      return ok(collected.values as ItemsOutput<TItems>);
    }, description);
  }

  return makeSchema((value, path) => {
    if (!Array.isArray(value)) {
      return fail("type", "expected tuple", path);
    }
    if (value.length !== arity) {
      return fail("length", message, path);
    }
    const out: Array<mixed> = [];
    let issues: null | Array<Issue> = null;
    for (let index = 0; index < arity; index += 1) {
      const result = runAt(items[index], value[index], path, String(index));
      if (result.ok) {
        out.push(result.value);
      } else {
        issues = issues ?? [];
        mergeIssues(issues, result);
      }
    }
    // $FlowFixMe[incompatible-type] position by position, each item's own schema built it.
    return issues == null ? ok(out as ItemsOutput<TItems>) : { ok: false, issues };
  }, description);
}

/**
 * An object whose keys are not known ahead of time.
 *
 * Only own enumerable keys are read, so a payload carrying `__proto__` or
 * `constructor` cannot smuggle an inherited value into the parsed result.
 */
export function record<TOutput, TInput>(
  value: Schema<TOutput, TInput>,
): Schema<{ readonly [string]: TOutput, ... }, { readonly [string]: TInput, ... }> {
  const description = (): Description => ({ kind: "record", value: describe(value) });

  if (isAsync(value)) {
    return makeAsyncSchema(async (input, path) => {
      if (!isPlainObject(input)) {
        return fail("type", "expected object", path);
      }
      const source = plainRecord(input);
      const keys = ownKeys(source);
      const collected = await collectAsync(
        keys.map((key) => ({ keys: [key], schema: value, value: ownValue(source, key) })),
        path,
      );
      if (!collected.ok) {
        return { ok: false, issues: collected.issues };
      }
      const out: { [string]: TOutput, ... } = {};
      keys.forEach((key, index) => {
        // $FlowFixMe[incompatible-type] every value came out of `value`.
        put(out, key, collected.values[index] as TOutput);
      });
      return ok(out);
    }, description);
  }

  return makeSchema((input, path) => {
    if (!isPlainObject(input)) {
      return fail("type", "expected object", path);
    }
    const source = plainRecord(input);
    const out: { [string]: TOutput, ... } = {};
    let issues: null | Array<Issue> = null;
    for (const key of ownKeys(source)) {
      const result = runAt(value, ownValue(source, key), path, key);
      if (result.ok) {
        put(out, key, result.value);
      } else {
        issues = issues ?? [];
        mergeIssues(issues, result);
      }
    }
    return issues == null ? ok(out) : { ok: false, issues };
  }, description);
}

/**
 * A `Map`, with a schema for its keys and one for its values.
 *
 * Rebuilt rather than checked in place, because a `key` schema may be a `pipe`
 * that changes the key — and a map re-keyed in place would collide with itself
 * halfway through.
 */
export function map<TKey, TKeyInput, TValue, TValueInput>(
  key: Schema<TKey, TKeyInput>,
  value: Schema<TValue, TValueInput>,
): Schema<$ReadOnlyMap<TKey, TValue>, $ReadOnlyMap<TKeyInput, TValueInput>> {
  const description = (): Description => ({
    kind: "map",
    key: describe(key),
    value: describe(value),
  });

  if (isAsync(key) || isAsync(value)) {
    return makeAsyncSchema(async (input, path) => {
      if (!(input instanceof Map)) {
        return fail("type", "expected Map", path);
      }
      const entries = Array.from(input.entries());
      const collected = await collectAsync(
        entries.flatMap(([entryKey, entryValue], index) => [
          { keys: [String(index), "key"], schema: key, value: entryKey },
          { keys: [String(index), "value"], schema: value, value: entryValue },
        ]),
        path,
      );
      if (!collected.ok) {
        return { ok: false, issues: collected.issues };
      }
      const out = new Map<TKey, TValue>();
      for (let index = 0; index < entries.length; index += 1) {
        // $FlowFixMe[incompatible-type] the jobs were pushed key-then-value, in order.
        out.set(collected.values[index * 2] as TKey, collected.values[index * 2 + 1] as TValue);
      }
      return ok(out);
    }, description);
  }

  return makeSchema((input, path) => {
    if (!(input instanceof Map)) {
      return fail("type", "expected Map", path);
    }
    const out = new Map<TKey, TValue>();
    let issues: null | Array<Issue> = null;
    let index = 0;
    for (const [entryKey, entryValue] of input.entries()) {
      const at = String(index);
      const parsedKey = runUnder(key, entryKey, path, [at, "key"]);
      const parsedValue = runUnder(value, entryValue, path, [at, "value"]);
      if (parsedKey.ok && parsedValue.ok) {
        out.set(parsedKey.value, parsedValue.value);
      } else {
        issues = issues ?? [];
        mergeIssues(issues, parsedKey);
        mergeIssues(issues, parsedValue);
      }
      index += 1;
    }
    return issues == null ? ok(out) : { ok: false, issues };
  }, description);
}

/**
 * A `Set`, every member parsed by `item`.
 *
 * A `pipe` on `item` can make two distinct inputs equal — `trim()` over
 * `"a"` and `"a "` — and the rebuilt set then has one member where the input
 * had two. That is what a set is for, and it is worth knowing before it
 * surprises somebody counting rows.
 */
export function set<TOutput, TInput>(
  item: Schema<TOutput, TInput>,
): Schema<$ReadOnlySet<TOutput>, $ReadOnlySet<TInput>> {
  const description = (): Description => ({ kind: "set", item: describe(item) });

  if (isAsync(item)) {
    return makeAsyncSchema(async (value, path) => {
      if (!(value instanceof Set)) {
        return fail("type", "expected Set", path);
      }
      const collected = await collectAsync(
        Array.from(value).map((member, index) => ({
          keys: [String(index)],
          schema: item,
          value: member,
        })),
        path,
      );
      if (!collected.ok) {
        return { ok: false, issues: collected.issues };
      }
      const out = new Set<TOutput>();
      for (const member of collected.values) {
        // $FlowFixMe[incompatible-type] every member came out of `item`.
        out.add(member as TOutput);
      }
      return ok(out);
    }, description);
  }

  return makeSchema((value, path) => {
    if (!(value instanceof Set)) {
      return fail("type", "expected Set", path);
    }
    const out = new Set<TOutput>();
    let issues: null | Array<Issue> = null;
    let index = 0;
    for (const member of value) {
      const result = runAt(item, member, path, String(index));
      if (result.ok) {
        out.add(result.value);
      } else {
        issues = issues ?? [];
        mergeIssues(issues, result);
      }
      index += 1;
    }
    return issues == null ? ok(out) : { ok: false, issues };
  }, description);
}
