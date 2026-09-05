// @flow
//
// `@uniflowed/validator/parse`: the four ways to run a schema.
//
//   safeParse(User, body)        // a Result: failure is a value
//   parse(User, body)            // the value, or a thrown ValidationError
//   await safeParseAsync(…)      // the same, for a schema that has to wait
//   await parseAsync(…)
//
// Two axes, and both of them are a real choice rather than a style.
//
// # Throwing or not
//
// A boundary that already has a failure path — an HTTP handler building a 422,
// a form collecting field errors — wants [`safeParse`], because a `throw` there
// is a control-flow detour to reach a value it was going to inspect anyway.
// Code with no failure path — a configuration file the process cannot start
// without, a fixture in a test — wants [`parse`], because the alternative is an
// `if (!result.ok) throw` at every call site.
//
// [`parse`] raises [`ValidationError`], which carries the structured issues
// rather than only a joined message. An HTTP handler needs the field paths to
// build a response body, and re-parsing them out of a string is not a thing an
// API should make anybody do.
//
// # Waiting or not
//
// A schema is asynchronous, or it is not, and it knew which when it was built
// — `schema.js` says why. So the synchronous entry points do not return a
// promise sometimes: given a schema with a `checkAsync` in it they throw an
// `Error` naming the asynchronous pair, which is a programmer's mistake
// reported as one. It is not a [`ValidationError`], because nothing was
// invalid.
//
// The asynchronous entry points accept both kinds. A schema with nothing to
// wait for resolves on the first microtask, so a caller that does not know
// which it has — a generic resolver, a request handler taking a schema from a
// route table — can always use them.

import type { Result, Schema } from "./schema.js";
import { runAsync, run } from "./schema.js";
import { ValidationError } from "./issue.js";

/** Parse into a result, so failure is a value rather than control flow. */
export function safeParse<TOutput>(schema: Schema<TOutput, mixed>, value: mixed): Result<TOutput> {
  return run(schema, value, []);
}

/** Parse, or raise a [`ValidationError`] carrying every issue found. */
export function parse<TOutput>(schema: Schema<TOutput, mixed>, value: mixed): TOutput {
  const result = safeParse(schema, value);
  if (result.ok) {
    return result.value;
  }
  throw new ValidationError(result.issues);
}

/** [`safeParse`], for a schema with something to wait for. */
export function safeParseAsync<TOutput>(
  schema: Schema<TOutput, mixed>,
  value: mixed,
): Promise<Result<TOutput>> {
  return runAsync(schema, value, []);
}

/** [`parse`], for a schema with something to wait for. */
export async function parseAsync<TOutput>(
  schema: Schema<TOutput, mixed>,
  value: mixed,
): Promise<TOutput> {
  const result = await safeParseAsync(schema, value);
  if (result.ok) {
    return result.value;
  }
  throw new ValidationError(result.issues);
}

/**
 * Whether `value` would parse.
 *
 * A `boolean` and not a type guard. Flow's `value is T` needs the predicate to
 * be provable from the function's body, and here the proof is a closure the
 * checker cannot see through; a guard would be a claim rather than a check.
 * Narrow with [`safeParse`] and read `result.value`, which is the same
 * information with the value attached.
 */
export function is(schema: Schema<mixed, mixed>, value: mixed): boolean {
  return safeParse(schema, value).ok;
}

/**
 * A schema as a standalone function.
 *
 * `safeParse(schema, value)` needs both halves at the call site, which is fine
 * where the schema is in scope and useless where it is not — a boundary that
 * wants to validate what arrives takes a *function*, not a schema and an
 * import of this package. `parser(User)` is that function, and it is why
 * `@uniflowed/fetch` can check a response body without depending on the
 * validator at all.
 */
export function parser<TOutput>(schema: Schema<TOutput, mixed>): (value: mixed) => Result<TOutput> {
  return (value: mixed) => safeParse(schema, value);
}

/**
 * Validate a value during render.
 *
 * A hook rather than a plain call so the React Compiler memoises it with the
 * rest of the component: re-rendering for an unrelated reason does not re-walk
 * the payload. It is here rather than in a module of its own because it is
 * [`safeParse`] at a render boundary and not a second idea — and because
 * nothing in this package imports React to provide it.
 */
export hook useValidation<TOutput>(schema: Schema<TOutput, mixed>, value: mixed): Result<TOutput> {
  return safeParse(schema, value);
}
