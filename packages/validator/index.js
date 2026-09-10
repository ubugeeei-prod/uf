// @flow
//
// `@uniflowed/validator`: a schema is a parser, not an assertion.
//
// ```js
// const Account = object({
//   email: pipe(string(), trim(), email()),
//   age: pipe(string(), transform(Number), integer(), min(18)),
//   tags: array(pipe(string(), nonEmpty())),
// });
//
// const result = safeParse(Account, await request.json());
// if (!result.ok) {
//   return respond(422, flatten(result.issues));
// }
// createAccount(result.value); // { email: string, age: number, tags: … }
// ```
//
// `result.value.age` is a `number` and nobody wrote that down. The schema is
// the only description of the account that exists, and both the type the form
// collects and the type the application uses are read off it — that is the
// whole promise of this package, and `infer.js` is where it is kept.
//
// Ordinary Flow-typed JavaScript with no native binding, so it behaves
// identically on Node.js, Deno and Bun, and every builder is a separate named
// export so an application ships only the checks it calls.
//
// # The four decisions everything else follows from
//
// **A schema is a closure, plus a description of itself.** No class, no
// registry, no interpreter walking a description at run time. The engine that
// validates a `string()` *is* the four-line closure `string()` returned, which
// a JIT can inline into the object parser that calls it. The description is a
// thunk beside it, allocated only when something asks — and it is what makes
// `toJsonSchema` possible at all, because a closure cannot be read.
// `schema.js`.
//
// **An issue says where it happened.** `["users", "2", "email"]`, as segments,
// not a string somebody has to parse back apart. A form binds errors to
// fields, and a field is a path. `issue.js`.
//
// **Every branch of the value is visited.** A bad third row does not hide a
// bad seventh one; one parse reports both, because the alternative is two
// round trips for information that was available at once. `collection.js`,
// `object.js`.
//
// **Asynchrony is decided when the schema is built, not when it runs.** A
// schema containing a `checkAsync` is asynchronous from there up, so
// `safeParse` refuses it with a message instead of returning a promise where
// its caller expected a result. `schema.js` sets out the alternative and what
// it would cost every synchronous parse.
//
// # How the package is laid out
//
// Fifteen modules beside this one. Nothing is under an `internal/`: each is a
// reasonable thing to import on purpose, and every one is reachable through a
// subpath so an application that only needs `parse` and `object` can say so.
//
// The leaves, which know nothing about schemas:
//
// - `plain-object.js` — reading and writing an object whose keys came from
//   outside, without touching its prototype. The package's whole answer to
//   `{"__proto__": …}`, in one place so that it is one answer.
// - `issue.js` — what a failure is, where it happened, and the error `parse`
//   throws.
//
// The kernel:
//
// - `schema.js` — what a schema *is*: the opaque type, the two-function
//   kernel, the description it carries, and the walk that runs one. Read this
//   first.
// - `infer.js` — reading a value's type off its schema. Types only; it
//   compiles to nothing.
//
// The vocabulary, one module per kind of thing a schema can be:
//
// - `primitive.js` — the leaves: `string`, `number`, `literal`, `enum_`, and
//   the rest of what uf will recognise without being told how.
// - `object.js` — keys known when the schema was written, in the three
//   flavours that differ only in what happens to an unknown key.
// - `collection.js` — arrays, tuples, records, maps and sets: containers whose
//   contents are only known when the value arrives.
// - `union.js` — several schemas over one value: `union`, the discriminated
//   `variant` that gives an error worth reading, and `intersect`.
// - `optional.js` — a value that might not be there, and what to put there
//   when it is not.
// - `lazy.js` — a schema that does not exist yet, which is how a comment tree
//   is spelled.
//
// The pipeline:
//
// - `pipe.js` — a schema with steps after it, the `Step` type, and the
//   overload table that carries the output type through a `transform`.
// - `action.js` — the steps that come ready-made, each with the constraint
//   that makes it visible to an exporter.
//
// The edges:
//
// - `parse.js` — the four ways to run a schema, and why there are four.
// - `json-schema.js` — a schema as a document somebody else can read, and an
//   honest list of what JSON Schema could not say.
// - `namespace.js` — `v`, the alias that holds every builder. Separate because
//   it is the one module that has to import all of them, and an entry point
//   that did that would make every application carry every check.
//
// # Readiness
//
// **Implemented and tested.** The schema vocabulary above, including
// discriminated unions, intersections, recursive schemas, maps and sets;
// issue paths through every composite; both entry points in both synchronous
// and asynchronous forms; `InferInput` and `InferOutput` over objects, shapes,
// tuples, unions, variants and pipelines; a `pipe` that changes the output
// type with the change surviving into the inferred type; `toJsonSchema` with
// `$defs` for recursion and a reported list of what it could not express.
// `packages/validator/validator.test.js` covers each of those, including
// `@uniflowed/form`'s resolver over both a synchronous and an asynchronous
// schema, and `packages/form/form.test.js` covers that resolver inside a real
// form.
//
// **Experimental.** [`describe`] and the [`Description`] type. The shape is
// right for `json-schema.js` and it is the shape a code generator should read,
// but no generator exists yet to prove the second half — see below — so the
// type may gain cases before it is stable.
//
// **Not implemented, deliberately.**
//
// - `pick`, `omit` and `required` over a *schema*. These take a shape here or
//   not at all, and `object.js` says why: a built schema is a closure and a
//   description, not a reified field list.
// - A nominal `brand`. `brand("UserId")` is a name in the description and
//   nothing to the checker, because Flow's opaque types are declared in a
//   module and cannot be produced by a call. `infer.js` says what to write
//   instead when the distinction has to be enforced.
// - Typed field paths. Flow has no template-literal types; an `Issue`'s `path`
//   is `$ReadOnlyArray<string>` and `@uniflowed/form` makes the same call for
//   the same reason.
// - The long tail of format checks — `creditCard`, `emoji`, `mac`, `cuid2`.
//   `action.js` says why a stale regular expression in a library is worse than
//   a `check` an application owns.
// - More than eight `pipe` steps in one call. Flow cannot fold a type over a
//   variadic list, so the arities are spelled out; a ninth step is a type
//   error and the fix is to pipe the result of a pipe.
//
// **Not implemented, and a gap.** `uf prepare` lists a
// `GenerateValidatorTypes` step and nothing implements it: no crate reads a
// schema and writes Flow types or a JSON Schema file to disk. The half that
// belongs in this package — a description complete enough to generate from —
// is here and is exercised by `toJsonSchema`. The build-time half is not
// written, and this package should not grow it: walking a repository's sources
// is Rust's job under the same rule that puts the formatter and the checker
// there.
//
// # Measured, against Valibot
//
// Valibot 1.4.2 from npm, Node 25.8.1, an Apple M2 Max (12 cores, macOS 26.5),
// best of five timed runs after two warm-up runs, both libraries given the
// same payload and schemas built out of the same pieces — an object of seven
// fields with a nested object, an array of strings, and `minLength`,
// `maxLength`, `integer`, `min` and `max` steps. The two agree on every case
// the harness runs, which it asserts before it times anything: the same
// verdict, the same number of issues, the same transformed value.
//
// | Workload | uf | Valibot | |
// | --- | --- | --- | --- |
// | 1,000-record list, all valid (per record) | **0.49 µs** | 0.65 µs | 1.34x |
// | 1,000-record list, one field bad in ten | **0.50 µs** | 0.66 µs | 1.34x |
// | One form object, four fields, one transform | **0.20 µs** | 0.35 µs | 1.73x |
// | `safeParse(string(), "ada")` | **5.0 ns** | 15.6 ns | 3.1x |
// | Building the record schema | **0.47 µs** | 6.5 µs | 14x |
//
// The harness is not in the repository — it needs Valibot from npm, and this
// repository installs no dependency it does not ship — so it lives with the
// change that produced these numbers. It runs as:
//
//   UF_PROJECT_ROOT=$PWD node --import ./packages/host/register.js \
//     bench-validator.js path/to/valibot/dist/index.cjs
//
// The construction column is the one to read first: a Valibot schema is a
// tree of objects with an `~run` method and metadata on each node, and this
// package's is a closure. That is fourteen times cheaper to build and it is
// why the leaf parse is three times cheaper to run.
//
// The row that was *worse* is worth recording too, because finding it is what
// made the others true. Writing every parsed field with
// `Object.defineProperty` — which is how this package used to keep a
// `__proto__` key out of the prototype — cost more than the entire rest of the
// walk: the list workload took 298 µs per hundred records instead of 49, and
// this package was 2.4 times *slower* than Valibot rather than 1.34 times
// faster. `plain-object.js` has the one-line fix and the argument that it
// keeps the guarantee intact.

export type { FlatIssues, Issue, Path } from "./issue.js";
export type { Constraint, Description, Result, Schema } from "./schema.js";
export type {
  Infer,
  InferInput,
  InferOutput,
  ItemsInput,
  ItemsOutput,
  Options,
  Shape,
  ShapeInput,
  ShapeOutput,
} from "./infer.js";
export type { Pipe, Step } from "./pipe.js";
export type { JsonSchemaExport, JsonSchemaNode, Unrepresentable } from "./json-schema.js";

export { ValidationError, flatten } from "./issue.js";
export { describe, isAsync } from "./schema.js";
export {
  bigint,
  boolean,
  custom,
  date,
  enum_,
  instance,
  literal,
  never,
  null_,
  number,
  string,
  undefined_,
  unknown,
} from "./primitive.js";
export { looseObject, object, partial, strictObject } from "./object.js";
export { array, map, record, set, tuple } from "./collection.js";
export { intersect, union, variant } from "./union.js";
export { fallback, nullable, nullish, optional, withDefault } from "./optional.js";
export { lazy, lazyAsync } from "./lazy.js";
export { brand, check, checkAsync, pipe, refine, transform, transformAsync } from "./pipe.js";
export {
  email,
  endsWith,
  includes,
  integer,
  isoDate,
  length,
  max,
  maxItems,
  maxLength,
  min,
  minItems,
  minLength,
  multipleOf,
  nonEmpty,
  regex,
  startsWith,
  toLowerCase,
  toUpperCase,
  trim,
  url,
  uuid,
} from "./action.js";
export {
  is,
  parse,
  parseAsync,
  parser,
  safeParse,
  safeParseAsync,
  useValidation,
} from "./parse.js";
export { toJsonSchema } from "./json-schema.js";

export { v } from "./namespace.js";
