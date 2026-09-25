// @flow
//
// Internal to `@uniflowed/router`: a page's `searchParams` schema, run against
// the query string.
//
// A page that exports
//
//     export const searchParams = object({ page: number(), tag: array(string()) });
//
// is handed the schema's output as its `searchParams` prop instead of the
// string map every other page gets. Two things stand between a query string
// and that schema, and both are this file.
//
// **Shape.** A query is a list of pairs, and a key may repeat. Which repeats
// are a list and which are one value is not something the query says — it is
// what the schema says, so the schema is asked: a field described as an array
// (or a set, or a tuple) gets every value in order, and any other field gets
// the last one, which is what `parseSearch` has always kept. A field that is a
// bare array and whose key is absent gets `[]`, because no `tag` in the query
// is no tags rather than a malformed request.
//
// **Type.** Every value in a query is a string, and `number()` refuses a string.
// A field described as a number, a boolean, a bigint or a date is given the
// value the string spells when it spells one — `"2"` is `2`, `"true"` is
// `true` — and the string itself when it does not, so the schema's own message
// is what a malformed value is reported with. A field the schema reads as a
// string is left alone, which is why `pipe(string(), transform(Number))` still
// sees the string it asked for.
//
// Everything past that is the schema's: optionality, defaults, refinements,
// transforms. A query that does not fit is a `SearchParamsError`, which the
// resolver turns into the error boundary with a `400` — the request's fault,
// not the page's.

// The two subpaths rather than the package: a browser bundle of the router
// carries the parser and the description reader, and none of the builders.
import { safeParseAsync } from "@uniflowed/validator/parse";
import { describe } from "@uniflowed/validator/schema";
import type { Description, Schema } from "@uniflowed/validator/schema";

import { SearchParamsError, parseSearchAll } from "./routing.js";
import type { SearchParamsAll } from "./routing.js";

/**
 * The value a page's `searchParams` schema produces for `search`.
 *
 * Throws a [`SearchParamsError`] carrying every issue when the query does not
 * fit, rather than returning them: the resolver's catch is what turns a thrown
 * router error into a boundary, and this is one.
 */
export async function parseSearchParams(
  schema: Schema<mixed, mixed>,
  search: string,
): Promise<mixed> {
  const input = searchParamsInput(describe(schema), parseSearchAll(search));
  const result = await safeParseAsync(schema, input);
  if (!result.ok) {
    throw new SearchParamsError(result.issues);
  }
  return result.value;
}

/**
 * The object a schema described by `description` is given for a query.
 *
 * Exported for the tests, which hold the shape and the coercion to what the
 * header says without a route table around them.
 */
export function searchParamsInput(
  description: Description,
  query: SearchParamsAll,
): { readonly [string]: mixed } {
  const fields = objectFields(description);
  // Built as a `Map` and turned into an object once, by `Object.fromEntries`,
  // which defines own properties: `?__proto__=x` is a key like any other.
  const input = new Map<string, mixed>();
  for (const key of Object.keys(query)) {
    const values = query[key];
    const field = fields.get(key);
    if (field == null) {
      // A key the schema does not name: an `object()` strips it and a
      // `strictObject()` refuses it, and a `looseObject()` keeps it as the
      // query had it — one value, or every value of a repeated key.
      input.set(key, values.length === 1 ? values[0] : values);
    } else {
      input.set(key, fieldValue(field, values));
    }
  }
  for (const [key, field] of fields) {
    if (!input.has(key) && isList(plain(field))) {
      input.set(key, []);
    }
  }
  return Object.fromEntries(input);
}

/** The fields a schema names, when it is an object or an intersection of them. */
function objectFields(description: Description): Map<string, Description> {
  const found = new Map<string, Description>();
  const visit = (node: Description): void => {
    const unwrapped = unwrap(node);
    if (unwrapped.kind === "object") {
      for (const [key, field] of unwrapped.entries) {
        found.set(key, field);
      }
    } else if (unwrapped.kind === "intersect") {
      for (const part of unwrapped.parts) {
        visit(part);
      }
    }
  };
  visit(description);
  return found;
}

/** What one field is given: every value for a list, the last for anything else. */
function fieldValue(field: Description, values: $ReadOnlyArray<string>): mixed {
  const described = unwrap(field);
  if (described.kind === "array" || described.kind === "set") {
    const item = described.item;
    return values.map((value) => coerce(item, value));
  }
  if (described.kind === "tuple") {
    const items = described.items;
    return values.map((value, index) =>
      index < items.length ? coerce(items[index], value) : value,
    );
  }
  return coerce(described, values[values.length - 1]);
}

/** The value `raw` spells for a field described by `description`, or `raw`. */
function coerce(description: Description, raw: string): mixed {
  const described = unwrap(description);
  switch (described.kind) {
    case "number":
      return numeric(raw) ?? raw;
    case "bigint":
      return /^[-+]?\d+$/.test(raw.trim()) ? BigInt(raw.trim()) : raw;
    case "boolean":
      return raw === "true" ? true : raw === "false" ? false : raw;
    case "date": {
      const date = new Date(raw);
      return Number.isNaN(date.getTime()) ? raw : date;
    }
    case "literal": {
      const value = described.value;
      if (typeof value === "number") return numeric(raw) === value ? value : raw;
      if (typeof value === "boolean") return raw === String(value) ? value : raw;
      if (value === null) return raw === "null" ? null : raw;
      return raw;
    }
    case "union": {
      // The string itself when some option takes it as it is, so
      // `union([literal("all"), number()])` reads `all` as `"all"` and `3` as 3.
      const options = described.options.map(unwrap);
      if (options.some((option) => acceptsAsIs(option, raw))) {
        return raw;
      }
      for (const option of options) {
        const coerced = coerce(option, raw);
        if (coerced !== raw) return coerced;
      }
      return raw;
    }
    default:
      return raw;
  }
}

/** Whether a field described this way takes `raw` without coercion. */
function acceptsAsIs(described: Description, raw: string): boolean {
  switch (described.kind) {
    case "string":
    case "unknown":
      return true;
    case "enum":
      return described.values.includes(raw);
    case "literal":
      return described.value === raw;
    default:
      return false;
  }
}

/** A finite number `raw` spells, or `null`. `""` is not zero. */
function numeric(raw: string): ?number {
  if (raw.trim() === "") return null;
  const value = Number(raw);
  return Number.isFinite(value) ? value : null;
}

/**
 * A description with every wrapper that does not change what the value *is*
 * taken off: optionality, defaults, `pipe` steps and `lazy`.
 */
function unwrap(description: Description): Description {
  const inner = innerOf(description, true);
  return inner == null ? description : unwrap(inner);
}

/**
 * The same, keeping optionality and defaults: whether a field that is absent
 * from the query should be an empty list is a question about the field as it
 * was written, and `optional(array(…))` answers it differently from
 * `array(…)`.
 */
function plain(description: Description): Description {
  const inner = innerOf(description, false);
  return inner == null ? description : plain(inner);
}

/** What a wrapper wraps, or `null` for a description that is not one. */
function innerOf(description: Description, optionality: boolean): ?Description {
  switch (description.kind) {
    case "transformed":
    case "constrained":
      return description.inner;
    case "lazy":
      return description.inner();
    case "optional":
    case "nullable":
    case "nullish":
    case "default":
    case "fallback":
      return optionality ? description.inner : null;
    default:
      return null;
  }
}

function isList(description: Description): boolean {
  return description.kind === "array" || description.kind === "set";
}
