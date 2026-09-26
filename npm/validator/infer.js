// @flow
//
// `@uniflowed/validator/infer`: reading a value's type off its schema.
//
// This module compiles to nothing. Every name in it is a type, and the reason
// they are here rather than beside the parsers is that they are the answer to
// a question the parsers do not ask: given a schema, what is the type of the
// thing it produces, and what is the type of the thing you may hand it?
//
//   const Account = object({
//     email: pipe(string(), email()),
//     age: pipe(string(), transform(Number), min(18)),
//   });
//
//   type Raw = InferInput<typeof Account>;   // {| email: string, age: string |}
//   type Account = InferOutput<typeof Account>; // {| email: string, age: number |}
//
// Neither type was written down. That is the point: a schema is the single
// source of truth, and a hand-written `type Account` beside it is a second
// truth that will disagree with the first on a Tuesday.
//
// # What Flow can do here
//
// More than this package used to assume. Conditional types with `infer` are
// what read a parameter back out of `Schema<Out, In>`; mapped types are what
// turn a shape — an object whose values are schemas — into an object whose
// values are those schemas' outputs, key by key, without naming the keys.
// `object`, `partial`, `looseObject`, `tuple`, `union`, `variant` and `record`
// all infer, so none of them takes an explicit type argument any more, and a
// field added to a shape appears in the inferred type with nothing else
// edited.
//
// The inferred object types are **exact**. `InferOutput<typeof Account>` does
// not accept an extra property, which matches what `object()` does at run time
// — it drops keys the shape does not name — and is why `looseObject` has a
// separate spelling with `...` in its result.
//
// # What it cannot
//
// **Paths are not typed.** An `Issue`'s `path` is `$ReadOnlyArray<string>`.
// Flow has no template-literal types, so there is no way to say "one of the
// paths that exist in this schema", and a `SchemaPath<S>` alias that was
// really `string` would be a type that looks like it checks something and does
// not. `@uniflowed/form` makes the same call for the same reason.
//
// **Branded outputs are not nominal.** `pipe(string(), brand("UserId"))` is
// `Schema<string, string>`. Flow's opaque types are per module and cannot be
// generated from a call, so the brand is a label in the description — real for
// `json-schema.js`, and honest about being nothing to the checker. A project
// that wants `UserId` to be a distinct type should declare
// `opaque type UserId = string` in the module that owns it and annotate there.
//
// **`InferInput` describes shape, not provenance.** It says a valid input to
// `pipe(string(), transform(Number))` is a `string`. It cannot say that the
// string has to parse as a number, because that is what the parse is for.
//
// **A recursive schema still needs one annotation.** `lazy(() => …)` builds a
// type that mentions itself, and Flow will not solve for that on its own; the
// type argument on `lazy` is where the cycle is cut. `lazy.js` has the
// example.

import type { Schema } from "./schema.js";

/** The type a schema produces. The one every consumer wants. */
export type InferOutput<TSchema> = TSchema extends Schema<infer TValue, infer TSource>
  ? TValue
  : empty;

/**
 * The type a valid input to a schema has.
 *
 * Equal to [`InferOutput`] until a `transform` is in the pipeline. Where they
 * differ, this is the one a form's `defaultValues` wants and the other is the
 * one its `onValid` receives.
 */
export type InferInput<TSchema> = TSchema extends Schema<infer TValue, infer TSource>
  ? TSource
  : empty;

/** The older name for [`InferOutput`], kept so existing annotations compile. */
export type Infer<TSchema> = InferOutput<TSchema>;

/** An object whose values are schemas: what `object` and `variant` take. */
export type Shape = { readonly [string]: Schema<mixed, mixed>, ... };

/** A list of schemas: what `union` and `tuple` take. */
export type Options = $ReadOnlyArray<Schema<mixed, mixed>>;

/** The object a shape produces, key by key. */
export type ShapeOutput<TShape extends Shape> = {
  [Key in keyof TShape]: InferOutput<TShape[Key]>,
};

/** The object a shape accepts, key by key. */
export type ShapeInput<TShape extends Shape> = {
  [Key in keyof TShape]: InferInput<TShape[Key]>,
};

/** The tuple a list of schemas produces, position by position. */
export type ItemsOutput<TItems extends Options> = {
  [Index in keyof TItems]: InferOutput<TItems[Index]>,
};

/** The tuple a list of schemas accepts, position by position. */
export type ItemsInput<TItems extends Options> = {
  [Index in keyof TItems]: InferInput<TItems[Index]>,
};
