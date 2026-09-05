// @flow
//
// `@uniflowed/form/resolver`: the contract between a form and a schema.
//
// A resolver is one function. It takes the form's current values and answers
// with either the parsed output or a map of errors keyed by field path:
//
//   const resolve: Resolver<Draft, Account> = async (values) => {
//     const result = safeParse(accountSchema, values);
//     return result.ok
//       ? { values: result.value, errors: {} }
//       : { errors: errorsFromIssues(result.issues) };
//   };
//
// That is the whole interface, and it is deliberately small enough that a
// schema library uf has never heard of can be wired in from application code.
// `@uniflowed/form/validator` is one such adapter, written against nothing but
// this file and `@uniflowed/validator`'s public `safeParse`.
//
// # Why a resolver replaces the inline rules rather than joining them
//
// A schema is a description of the whole value, so it already has an opinion
// about every field — including the ones whose `register` call also carries a
// `required`. Running both means two sources of truth for one field and an
// answer that depends on which ran last. So when a form has a resolver, the
// rules passed to `register` are not consulted for validation at all; they
// still describe how to *read* the control (`valueAsNumber`, `setValueAs`),
// because that is a different question and the schema cannot answer it.
//
// # Why the errors are keyed by path
//
// `{ "address.city": { type, message } }`, with the same string `register`
// took. A nested error object mirrors the values and reads well in a
// screenshot, but it forces every consumer to walk it — and Flow cannot type
// the walk, because expressing "the error for `items.0.name`" needs a mapped
// type over a path that Flow has no way to spell. Flat keys make
// `errors[name]` the answer, at the cost of a shape that is not the shape of
// the data. That trade is described once here and once in `index.js` rather
// than being discovered.
//
// # Why the input and output types are separate
//
// `Resolver<TIn, TOut>` is generic in both because a schema is usually a
// parser, not a predicate: a form collects `{ age: "42" }` from a text box and
// the schema produces `{ age: 42 }`. `handleSubmit` hands `onValid` the
// resolver's *output*, so the coercion survives all the way to the submit
// handler and to a Server Action beyond it, instead of being redone by hand
// at the boundary.

import type { FieldError } from "./rules.js";
import type { FieldValues } from "./internal/field-path.js";

/** Errors by field path — the same strings `register` was given. */
export type ResolverErrors = { readonly [string]: FieldError, ... };

/**
 * What a resolver answers with.
 *
 * `values` is present only when there were no errors, and a resolver that
 * reports errors need not produce a value at all. The union is exact so that
 * checking `errors` narrows `values` for the caller.
 */
export type ResolverResult<TOut> =
  | {| readonly values: TOut, readonly errors?: void |}
  | {| readonly values?: void, readonly errors: ResolverErrors |};

/**
 * Validate a whole form.
 *
 * Given every value the form holds, plus whatever `useForm({ context })` was
 * passed — a locale, a tenant, a set of already-taken names — so that a schema
 * that depends on something outside the form does not have to close over it at
 * module scope.
 *
 * May be synchronous or return a promise. Both are supported because a schema
 * check is usually the former and a uniqueness check is always the latter, and
 * forcing the sync case through a promise costs a microtask on every keystroke
 * in `onChange` mode.
 */
export type Resolver<TIn extends FieldValues, TOut = TIn> = (
  values: TIn,
  context: mixed,
) => ResolverResult<TOut> | Promise<ResolverResult<TOut>>;

/**
 * Run a resolver and normalise its answer.
 *
 * Awaiting a value that is not a promise is a microtask a form in `onChange`
 * mode pays on every keystroke, so a synchronous resolver is returned
 * synchronously and the caller decides whether it has to wait. That is the
 * only reason this is not a one-line `await`.
 */
export function runResolver<TIn extends FieldValues, TOut>(
  resolver: Resolver<TIn, TOut>,
  values: TIn,
  context: mixed,
): ResolverResult<TOut> | Promise<ResolverResult<TOut>> {
  return resolver(values, context);
}

/** The errors a result carries, as a plain map. */
export function errorsOf<TOut>(result: ResolverResult<TOut>): ResolverErrors {
  return result.errors ?? {};
}

/**
 * Build resolver errors from `path -> message`, keeping the first per path.
 *
 * An adapter's job is nearly always this: a schema reports a list of issues,
 * several of which can land on the same field, and a form shows one message per
 * field. Keeping the *first* rather than the last matches how schema libraries
 * order their issues — outermost check first — so `"expected string"` wins over
 * a length complaint about a value that is not a string.
 */
export function collectErrors(
  issues: $ReadOnlyArray<{|
    readonly path: string,
    readonly type: string,
    readonly message: string,
  |}>,
): ResolverErrors {
  const errors: { [string]: FieldError, ... } = {};
  for (const issue of issues) {
    if (!Object.prototype.hasOwnProperty.call(errors, issue.path)) {
      // `defineProperty` rather than assignment: a field path comes from a
      // schema, and a schema describes data, and data can contain the key
      // `__proto__`. Assigning to it runs a setter instead of adding a key.
      Object.defineProperty(errors, issue.path, {
        value: { type: issue.type, message: issue.message },
        writable: true,
        enumerable: true,
        configurable: true,
      });
    }
  }
  return errors;
}
