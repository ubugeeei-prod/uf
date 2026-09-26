// @flow
//
// `@uniflowed/validator/optional`: a value that might not be there.
//
// Four wrappers and one rule each, and they are together because the question
// they answer is one question — what counts as "not there", and what should
// the parse produce when it happens.
//
//   optional(string())          // undefined is fine, and stays undefined
//   nullable(string())          // null is fine, and stays null
//   nullish(string())           // either is fine
//   withDefault(string(), "")   // undefined becomes ""
//   fallback(number(), 8080)    // *anything* that fails becomes 8080
//
// `undefined` and `null` are kept apart on purpose. A missing key and a key
// explicitly set to null mean different things in every wire format uf reads —
// a PATCH body, a GraphQL response, a form that cleared a field — and a
// validator that folded them together would hand the application a value it
// could no longer tell apart. `nullish` is there for the boundary that really
// does not care.
//
// # Where the input type stops matching the output type
//
// [`withDefault`] and [`fallback`] are the two schemas whose `InferInput` is
// genuinely wider than their `InferOutput`: a default's input may be missing
// and its output never is, and a fallback accepts literally anything. That is
// not a quirk of the typing, it is what those two are for, and it is the case
// `InferInput` exists to describe.

import type { Description, Result, Schema } from "./schema.js";
import { describe, isAsync, makeAsyncSchema, makeSchema, ok, run, runAsync } from "./schema.js";

/**
 * Build a wrapper that answers for some inputs itself and delegates the rest.
 *
 * `shortcut` returns a result to use as-is, or null to mean "ask the inner
 * schema". Every wrapper in this module is that shape, and writing it once is
 * also what keeps the asynchronous variants from being four more copies of the
 * same three lines.
 */
function wrap<TOutput, TInput>(
  inner: Schema<TOutput, mixed>,
  shortcut: (value: mixed) => null | Result<TOutput>,
  description: () => Description,
): Schema<TOutput, TInput> {
  if (isAsync(inner)) {
    return makeAsyncSchema((value, path) => {
      const answered = shortcut(value);
      return answered == null ? runAsync(inner, value, path) : Promise.resolve(answered);
    }, description);
  }
  return makeSchema((value, path) => {
    const answered = shortcut(value);
    return answered == null ? run(inner, value, path) : answered;
  }, description);
}

/** `undefined` passes through; anything else goes to `schema`. */
export function optional<TOutput, TInput>(
  schema: Schema<TOutput, TInput>,
): Schema<void | TOutput, void | TInput> {
  return wrap(
    schema,
    (value) => (value === undefined ? ok(undefined) : null),
    () => ({ kind: "optional", inner: describe(schema) }),
  );
}

/** `null` passes through; anything else goes to `schema`. */
export function nullable<TOutput, TInput>(
  schema: Schema<TOutput, TInput>,
): Schema<null | TOutput, null | TInput> {
  return wrap(
    schema,
    (value) => (value === null ? ok(null) : null),
    () => ({ kind: "nullable", inner: describe(schema) }),
  );
}

/** Either `null` or `undefined` passes through, unchanged. */
export function nullish<TOutput, TInput>(
  schema: Schema<TOutput, TInput>,
): Schema<null | void | TOutput, null | void | TInput> {
  return wrap(
    schema,
    (value) => (value == null ? ok(value === null ? null : undefined) : null),
    () => ({ kind: "nullish", inner: describe(schema) }),
  );
}

/**
 * `undefined` becomes `value`; anything else goes to `schema`.
 *
 * The default is not validated. It is a value the program wrote, in the
 * program's own types, and running it back through the parser would only be a
 * chance for the two to disagree.
 */
export function withDefault<TOutput, TInput>(
  schema: Schema<TOutput, TInput>,
  value: TOutput,
): Schema<TOutput, void | TInput> {
  return wrap(
    schema,
    (input) => (input === undefined ? ok(value) : null),
    () => ({ kind: "default", inner: describe(schema) }),
  );
}

/**
 * A schema that never fails, substituting `value` when the inner one does.
 *
 * For the boundary where a bad field should not sink the whole payload — a
 * cached response, a user preference — and where the alternative is a
 * `safeParse` and a hand-written `if` at every call site. Its input type is
 * `mixed`, because that is the truth: it accepts everything.
 */
export function fallback<TOutput>(
  schema: Schema<TOutput, mixed>,
  value: TOutput,
): Schema<TOutput, mixed> {
  const description = (): Description => ({ kind: "fallback", inner: describe(schema) });
  if (isAsync(schema)) {
    return makeAsyncSchema(async (input, path) => {
      const result = await runAsync(schema, input, path);
      return result.ok ? result : ok(value);
    }, description);
  }
  return makeSchema((input, path) => {
    const result = run(schema, input, path);
    return result.ok ? result : ok(value);
  }, description);
}
