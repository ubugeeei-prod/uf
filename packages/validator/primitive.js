// @flow
//
// `@uniflowed/validator/primitive`: the leaves.
//
// A schema that does not contain another schema. Every one of them is the same
// four lines — test the value, hand it back or say what was wanted — and they
// are together because that sameness is the whole subject: this is the list of
// things uf will recognise without being told how.
//
// Each is a function rather than a constant so that a project ships only the
// checks it called. `string` is not a value in a table somewhere that a
// bundler has to keep because the table is reachable.

import { plainRecord } from "./plain-object.js";
import type { Description, Schema } from "./schema.js";
import { fail, makeSchema, ok } from "./schema.js";

const stringly = (): Description => ({ kind: "string" });

export function string(): Schema<string, string> {
  return makeSchema(
    (value, path) =>
      typeof value === "string" ? ok(value) : fail("type", "expected string", path),
    stringly,
  );
}

/**
 * A finite number.
 *
 * `NaN` and the infinities are rejected. They are numbers to `typeof` and
 * disasters to arithmetic, and a validator that lets `NaN` through has not
 * validated anything — every comparison downstream silently answers `false`.
 */
export function number(): Schema<number, number> {
  return makeSchema(
    (value, path) =>
      typeof value === "number" && Number.isFinite(value)
        ? ok(value)
        : fail("type", "expected number", path),
    () => ({ kind: "number" }),
  );
}

/**
 * A `bigint`.
 *
 * Separate from [`number`] because the two do not mix: `1n === 1` is false,
 * `1n + 1` throws, and `JSON.stringify` refuses. A schema that accepted either
 * would hand its caller a value whose arithmetic depends on the payload.
 */
export function bigint(): Schema<bigint, bigint> {
  return makeSchema(
    (value, path) =>
      typeof value === "bigint" ? ok(value) : fail("type", "expected bigint", path),
    () => ({ kind: "bigint" }),
  );
}

export function boolean(): Schema<boolean, boolean> {
  return makeSchema(
    (value, path) =>
      typeof value === "boolean" ? ok(value) : fail("type", "expected boolean", path),
    () => ({ kind: "boolean" }),
  );
}

/** Anything at all, unexamined. The identity of this package. */
export function unknown(): Schema<mixed, mixed> {
  return makeSchema(
    (value) => ok(value),
    () => ({ kind: "unknown" }),
  );
}

/**
 * Nothing at all.
 *
 * For the branch of a union that must not be reachable, and for a shape whose
 * field is being removed: `never()` says so at the boundary instead of leaving
 * a field that quietly still works.
 */
export function never(): Schema<empty, empty> {
  return makeSchema(
    (value, path) => fail("never", "expected nothing here", path),
    () => ({ kind: "never" }),
  );
}

/** Exactly `null`. Distinct from a missing key, which is [`optional`]. */
export function null_(): Schema<null, null> {
  return makeSchema(
    (value, path) => (value === null ? ok(null) : fail("type", "expected null", path)),
    () => ({ kind: "null" }),
  );
}

/** Exactly `undefined`. */
export function undefined_(): Schema<void, void> {
  return makeSchema(
    (value, path) =>
      value === undefined ? ok(undefined) : fail("type", "expected undefined", path),
    () => ({ kind: "undefined" }),
  );
}

export function literal<TValue extends string | number | boolean | null>(
  expected: TValue,
): Schema<TValue, TValue> {
  return makeSchema(
    (value, path) =>
      value === expected ? ok(expected) : fail("literal", `expected ${String(expected)}`, path),
    () => ({ kind: "literal", value: expected }),
  );
}

/**
 * One of a fixed list of strings.
 *
 * The message names every option, because a rejected enum is almost always a
 * typo and the fix is in the list the caller could not see.
 */
export function enum_<TValue extends string>(
  values: $ReadOnlyArray<TValue>,
): Schema<TValue, TValue> {
  const message = `expected one of ${values.join(", ")}`;
  return makeSchema(
    (value, path) => {
      for (const option of values) {
        if (value === option) {
          return ok(option);
        }
      }
      return fail("enum", message, path);
    },
    () => ({ kind: "enum", values: values.slice() }),
  );
}

/** A `Date` that is a date, rather than the `Invalid Date` a bad string makes. */
export function date(): Schema<Date, Date> {
  return makeSchema(
    (value, path) =>
      value instanceof Date && Number.isFinite(value.getTime())
        ? ok(value)
        : fail("type", "expected Date", path),
    () => ({ kind: "date" }),
  );
}

/** An instance of `ClassValue`, by `instanceof`. */
export function instance<TValue>(ClassValue: Class<TValue>): Schema<TValue, TValue> {
  const named = plainRecord(ClassValue).name;
  const name = typeof named === "string" ? named : "instance";
  return makeSchema(
    (value, path) =>
      value instanceof ClassValue ? ok(value as TValue) : fail("type", `expected ${name}`, path),
    () => ({ kind: "instance", name }),
  );
}

/**
 * A leaf this package has no name for.
 *
 * The escape hatch, and the one place a caller's word is taken for a type:
 * `accepts` returning true is what makes the value a `TValue`, and nothing
 * checks that claim. Use it for a value with a shape of its own — a
 * `Uint8Array`, a branded id from another library — and reach for [`check`]
 * instead when the value is an ordinary type with a rule attached, because
 * that keeps the type honest.
 */
export function custom<TValue>(
  accepts: (value: mixed) => boolean,
  message: string,
  name: string = "custom",
): Schema<TValue, TValue> {
  return makeSchema(
    (value, path) =>
      // $FlowFixMe[incompatible-type] `accepts` is the caller's promise, not a proof.
      accepts(value) ? ok(value as TValue) : fail("custom", message, path),
    () => ({ kind: "custom", name }),
  );
}
