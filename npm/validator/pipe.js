// @flow
//
// `@uniflowed/validator/pipe`: a schema with steps after it.
//
//   const Handle = pipe(string(), trim(), minLength(3), startsWith("@"));
//   const Age = pipe(string(), transform(Number), integer(), min(18));
//
// A step takes a schema and returns a schema, so a pipeline is a fold and
// nothing more. `pipe` is variadic because refinement is cumulative in
// practice — a handle is a string that is trimmed *and* long enough *and*
// starts with an at-sign — and making that `pipe(pipe(pipe(…)))` is a tax on
// the only case anybody has.
//
// # A pipeline may change the output type
//
// `transform` is the reason the type has two parameters. `pipe(string(),
// transform(Number))` accepts a `string` and produces a `number`, and both
// halves survive: `InferInput` is `string`, `InferOutput` is `number`. That is
// what lets `@uniflowed/form` type `defaultValues` as what the controls hold
// and `onValid` as what the application wanted, from one schema.
//
// A step is polymorphic in the input type — `<TInput>(Schema<TFrom, TInput>)
// => Schema<TTo, TInput>` — which is how the pipeline's input survives every
// step after the first. It is also why a step is written as a generic arrow
// rather than a plain one.
//
// # Why the overload table
//
// Flow cannot fold a type over a variadic argument list, so the relationship
// "the output of step *n* is the input of step *n+1*" has to be spelled out
// once per arity. Eight of them, which covers every pipeline this repository
// or Valibot's own examples contain; a ninth step is a type error, and the fix
// is to name the first eight and pipe the result. The implementation under the
// table is arity-agnostic and there is exactly one cast in this package to
// join the two, marked where it happens.
//
// The alternative was the previous signature, `pipe(schema, ...steps:
// Step<any, any>): Schema<any>`, which type-checked everything and knew
// nothing: it silently erased the output type of every pipeline in every
// application, and `uf lint` was right to reject it.
//
// # Where a cross-field rule reports
//
// [`check`] takes an optional path. A rule that compares two fields lives on
// the object — it needs both of them — but the message belongs under the
// control the user has to change:
//
//   pipe(
//     object({ password: string(), confirm: string() }),
//     check((form) => form.password === form.confirm, "Passwords must match", ["confirm"]),
//   );
//
// Without it the issue arrives with the object's own path and a form has
// nowhere to put it.

import type { Path } from "./issue.js";
import { issueUnder } from "./issue.js";
import type { Constraint, Description, Schema } from "./schema.js";
import { describe, isAsync, makeAsyncSchema, makeSchema, ok, run, runAsync } from "./schema.js";

/**
 * One stage of a pipeline.
 *
 * Polymorphic in `TInput` so that the type of the pipeline's *input* is
 * carried through every step rather than being flattened to `mixed` at the
 * first one.
 */
export type Step<TFrom, TTo> = <TInput>(Schema<TFrom, TInput>) => Schema<TTo, TInput>;

/**
 * `pipe`'s type: one call signature per number of steps.
 *
 * Inexact, because an exact object type with call properties cannot be
 * inhabited by a function.
 */
export type Pipe = {
  <A, AIn>(schema: Schema<A, AIn>): Schema<A, AIn>,
  <A, AIn, B>(schema: Schema<A, AIn>, a: Step<A, B>): Schema<B, AIn>,
  <A, AIn, B, C>(schema: Schema<A, AIn>, a: Step<A, B>, b: Step<B, C>): Schema<C, AIn>,
  <A, AIn, B, C, D>(
    schema: Schema<A, AIn>,
    a: Step<A, B>,
    b: Step<B, C>,
    c: Step<C, D>,
  ): Schema<D, AIn>,
  <A, AIn, B, C, D, E>(
    schema: Schema<A, AIn>,
    a: Step<A, B>,
    b: Step<B, C>,
    c: Step<C, D>,
    d: Step<D, E>,
  ): Schema<E, AIn>,
  <A, AIn, B, C, D, E, F>(
    schema: Schema<A, AIn>,
    a: Step<A, B>,
    b: Step<B, C>,
    c: Step<C, D>,
    d: Step<D, E>,
    e: Step<E, F>,
  ): Schema<F, AIn>,
  <A, AIn, B, C, D, E, F, G>(
    schema: Schema<A, AIn>,
    a: Step<A, B>,
    b: Step<B, C>,
    c: Step<C, D>,
    d: Step<D, E>,
    e: Step<E, F>,
    f: Step<F, G>,
  ): Schema<G, AIn>,
  <A, AIn, B, C, D, E, F, G, H>(
    schema: Schema<A, AIn>,
    a: Step<A, B>,
    b: Step<B, C>,
    c: Step<C, D>,
    d: Step<D, E>,
    e: Step<E, F>,
    f: Step<F, G>,
    g: Step<G, H>,
  ): Schema<H, AIn>,
  <A, AIn, B, C, D, E, F, G, H, I>(
    schema: Schema<A, AIn>,
    a: Step<A, B>,
    b: Step<B, C>,
    c: Step<C, D>,
    d: Step<D, E>,
    e: Step<E, F>,
    f: Step<F, G>,
    g: Step<G, H>,
    h: Step<H, I>,
  ): Schema<I, AIn>,
  ...
};

function applySteps<TInput>(
  schema: Schema<mixed, TInput>,
  ...steps: $ReadOnlyArray<Step<mixed, mixed>>
): Schema<mixed, TInput> {
  let piped = schema;
  for (const step of steps) {
    piped = step<TInput>(piped);
  }
  return piped;
}

/** Apply steps to a schema, left to right. */
// $FlowFixMe[incompatible-type] the overload table above is the checked surface.
export const pipe: Pipe = applySteps as Pipe;

/**
 * A schema with one more thing that must be true of its output.
 *
 * Every named step in `action.js` is a call to this. The `constraint` is what
 * makes the step visible to `json-schema.js`: a refinement that cannot say
 * what it refined is invisible to every exporter.
 */
export function refine<TOutput, TInput>(
  schema: Schema<TOutput, TInput>,
  accepts: (value: TOutput) => boolean,
  code: string,
  message: string,
  constraint: Constraint,
  at: Path = [],
): Schema<TOutput, TInput> {
  const description = (): Description => ({
    kind: "constrained",
    inner: describe(schema),
    constraint,
  });

  if (isAsync(schema)) {
    return makeAsyncSchema(async (value, path) => {
      const result = await runAsync(schema, value, path);
      if (!result.ok || accepts(result.value)) {
        return result;
      }
      return { ok: false, issues: [issueUnder(code, message, path, at)] };
    }, description);
  }

  return makeSchema((value, path) => {
    const result = run(schema, value, path);
    if (!result.ok || accepts(result.value)) {
      return result;
    }
    return { ok: false, issues: [issueUnder(code, message, path, at)] };
  }, description);
}

/**
 * An arbitrary predicate, with the message it should report.
 *
 * Every other step in the package is a special case of this one. It exists so
 * that a rule the library did not anticipate — a checksum, a business rule,
 * one field agreeing with another — is a one-liner rather than a reason to
 * abandon the schema and hand-roll validation.
 *
 * `at` is where the issue lands, relative to the value being checked. See the
 * module docs for the cross-field case it is there for.
 */
export function check<TValue>(
  accepts: (value: TValue) => boolean,
  message: string,
  at: Path = [],
): Step<TValue, TValue> {
  return <TInput>(schema: Schema<TValue, TInput>): Schema<TValue, TInput> =>
    refine(schema, accepts, "check", message, { kind: "opaque", label: message }, at);
}

/**
 * A predicate that has to ask something.
 *
 * "Is this username taken" is a question with a network on the other end, and
 * a schema containing one is asynchronous from here up: the object around it,
 * the array around that, and [`safeParse`] will refuse it and say to use
 * [`safeParseAsync`].
 */
export function checkAsync<TValue>(
  accepts: (value: TValue) => Promise<boolean>,
  message: string,
  at: Path = [],
): Step<TValue, TValue> {
  return <TInput>(schema: Schema<TValue, TInput>): Schema<TValue, TInput> =>
    makeAsyncSchema(
      async (value, path) => {
        const result = await runAsync(schema, value, path);
        if (!result.ok) {
          return result;
        }
        return (await accepts(result.value))
          ? result
          : { ok: false, issues: [issueUnder("check", message, path, at)] };
      },
      () => ({
        kind: "constrained",
        inner: describe(schema),
        constraint: { kind: "opaque", label: message },
      }),
    );
}

/**
 * Change the value, and with it the schema's output type.
 *
 * Runs after everything before it in the pipeline has accepted, so a transform
 * never sees a value the steps above rejected — which is why
 * `pipe(string(), minLength(2), transform((text) => text.length))` is safe to
 * write in that order and means something different in the other.
 */
export function transform<TFrom, TTo>(change: (value: TFrom) => TTo): Step<TFrom, TTo> {
  return <TInput>(schema: Schema<TFrom, TInput>): Schema<TTo, TInput> => {
    const description = (): Description => ({ kind: "transformed", inner: describe(schema) });
    if (isAsync(schema)) {
      return makeAsyncSchema(async (value, path) => {
        const result = await runAsync(schema, value, path);
        return result.ok ? ok(change(result.value)) : result;
      }, description);
    }
    return makeSchema((value, path) => {
      const result = run(schema, value, path);
      return result.ok ? ok(change(result.value)) : result;
    }, description);
  };
}

/** A transform that has to wait: a lookup, a hash, a decode off the main path. */
export function transformAsync<TFrom, TTo>(
  change: (value: TFrom) => Promise<TTo>,
): Step<TFrom, TTo> {
  return <TInput>(schema: Schema<TFrom, TInput>): Schema<TTo, TInput> =>
    makeAsyncSchema(
      async (value, path) => {
        const result = await runAsync(schema, value, path);
        return result.ok ? ok(await change(result.value)) : result;
      },
      () => ({ kind: "transformed", inner: describe(schema) }),
    );
}

/**
 * A name on an otherwise ordinary value.
 *
 * It checks nothing at run time and it is not pretending to. The name reaches
 * the description, so an exported schema can say "this string is a `UserId`";
 * it does not reach the Flow type, because Flow's opaque types are declared in
 * a module and cannot be produced by a call. `infer.js` says what to do
 * instead when the distinction has to be enforced.
 */
export function brand<TValue>(name: string): Step<TValue, TValue> {
  return <TInput>(schema: Schema<TValue, TInput>): Schema<TValue, TInput> => {
    const description = (): Description => ({
      kind: "constrained",
      inner: describe(schema),
      constraint: { kind: "brand", name },
    });
    if (isAsync(schema)) {
      return makeAsyncSchema((value, path) => runAsync(schema, value, path), description);
    }
    return makeSchema((value, path) => run(schema, value, path), description);
  };
}
