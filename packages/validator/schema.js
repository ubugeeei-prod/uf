// @flow
//
// `@uniflowed/validator/schema`: what a schema *is*.
//
// A schema is two things and no more: a function that turns `mixed` into a
// [`Result`], and a function that says what it accepts. Nothing here is a
// class, a registry, or an interpreter walking a description at run time. The
// engine that validates a `string()` is the four-line closure `string()`
// returned, which a JIT can inline into the object parser that calls it — and
// it is why the package tree-shakes, because no table holds a reference to a
// check nobody imported.
//
// # Why a description, and why it is a thunk
//
// The second function is new, and it is the price of `json-schema.js`: a
// closure cannot be read, so a schema that is only a closure can never be
// exported as anything. `describe` is a thunk rather than a value so that
// building a schema allocates nothing extra, and so that a recursive schema
// can describe itself at all — `lazy.js` returns a description that contains a
// function returning its own description, which a value could not do without
// looping forever.
//
// # Why the path is a mutable buffer, and why the async walk copies it
//
// Issues report where they happened, and the obvious way to carry that is a
// fresh array per field: `path.concat(key)`. That allocates once per field per
// parse, on the *successful* path, to produce a value almost every parse
// throws away. So the synchronous walk pushes and pops one array as it
// descends, and only an actual issue copies it. A thousand-row payload that
// validates cleanly allocates no paths at all.
//
// The asynchronous walk cannot do that. Its whole point is that an object's
// fields are checked at the same time — a form with two fields that each hit a
// database should cost one round trip, not two — and two parses interleaved on
// one buffer would report each other's paths. So [`runAtAsync`] hands each
// child its own array. A parse that is already waiting on IO is not the parse
// where an allocation per field matters.
//
// # Why asynchrony is decided when the schema is built
//
// A kernel has a `parse` or a `parseAsync` and, in the ordinary case, not
// both. A composite asks its children which they have and builds the matching
// one, so `object({ name: pipe(string(), checkAsync(isFree)) })` is an
// asynchronous schema from the moment it exists, and [`safeParse`] can refuse
// it with a message instead of returning a promise where a caller expected a
// result. The alternative — a kernel that returns "a result or a promise of
// one" — puts a `typeof result.then` test on every node of every synchronous
// parse to pay for a feature most schemas never use.
//
// The one place that cannot be decided at construction is a recursive schema,
// which does not exist yet when its parent is built. That is the whole reason
// `lazy.js` has two constructors.

import type { Issue, Path, PathBuffer } from "./issue.js";
import { issue } from "./issue.js";

/**
 * What a parse produced, or why it did not.
 *
 * Covariant in `T`, which is what lets `fail` return a single `Result<empty>`
 * and every caller accept it.
 */
export type Result<out T> =
  | {| readonly ok: true, readonly value: T |}
  | {| readonly ok: false, readonly issues: $ReadOnlyArray<Issue> |};

/**
 * What a schema says about itself, for a consumer that has to write it down
 * somewhere else.
 *
 * Structural rather than nominal: a description is plain data with no schema
 * inside it, so `json-schema.js` — or a generator that has not been written
 * yet — can walk one without being able to run a parse. `lazy` is the
 * exception and has to be, because a recursive value has no finite spelling;
 * its `id` is the identity a converter keys its `$defs` on.
 */
export type Description =
  | {| readonly kind: "unknown" |}
  | {| readonly kind: "never" |}
  | {| readonly kind: "string" |}
  | {| readonly kind: "number" |}
  | {| readonly kind: "bigint" |}
  | {| readonly kind: "boolean" |}
  | {| readonly kind: "null" |}
  | {| readonly kind: "undefined" |}
  | {| readonly kind: "date" |}
  | {| readonly kind: "instance", readonly name: string |}
  | {| readonly kind: "custom", readonly name: string |}
  | {| readonly kind: "literal", readonly value: string | number | boolean | null |}
  | {| readonly kind: "enum", readonly values: $ReadOnlyArray<string> |}
  | {| readonly kind: "array", readonly item: Description |}
  | {| readonly kind: "tuple", readonly items: $ReadOnlyArray<Description> |}
  | {| readonly kind: "record", readonly value: Description |}
  | {| readonly kind: "map", readonly key: Description, readonly value: Description |}
  | {| readonly kind: "set", readonly item: Description |}
  | {|
      readonly kind: "object",
      readonly entries: $ReadOnlyArray<[string, Description]>,
      readonly unknownKeys: "strip" | "reject" | "keep",
    |}
  | {| readonly kind: "union", readonly options: $ReadOnlyArray<Description> |}
  | {|
      readonly kind: "variant",
      readonly key: string,
      readonly branches: $ReadOnlyArray<[string, Description]>,
    |}
  | {| readonly kind: "intersect", readonly parts: $ReadOnlyArray<Description> |}
  | {| readonly kind: "optional", readonly inner: Description |}
  | {| readonly kind: "nullable", readonly inner: Description |}
  | {| readonly kind: "nullish", readonly inner: Description |}
  | {| readonly kind: "default", readonly inner: Description |}
  | {| readonly kind: "fallback", readonly inner: Description |}
  | {| readonly kind: "lazy", readonly id: symbol, readonly inner: () => Description |}
  | {| readonly kind: "transformed", readonly inner: Description |}
  | {|
      readonly kind: "constrained",
      readonly inner: Description,
      readonly constraint: Constraint,
    |};

/**
 * What one `pipe` step narrowed.
 *
 * `opaque` is the honest answer for [`check`]: an arbitrary predicate has no
 * spelling in any export format, and saying so is better than emitting a
 * schema that claims the value is unconstrained.
 */
export type Constraint =
  | {| readonly kind: "minLength", readonly value: number |}
  | {| readonly kind: "maxLength", readonly value: number |}
  | {| readonly kind: "length", readonly value: number |}
  | {| readonly kind: "minItems", readonly value: number |}
  | {| readonly kind: "maxItems", readonly value: number |}
  | {| readonly kind: "min", readonly value: number |}
  | {| readonly kind: "max", readonly value: number |}
  | {| readonly kind: "integer" |}
  | {| readonly kind: "multipleOf", readonly value: number |}
  | {| readonly kind: "pattern", readonly source: string |}
  | {| readonly kind: "format", readonly name: string |}
  | {| readonly kind: "brand", readonly name: string |}
  | {| readonly kind: "opaque", readonly label: string |};

type SchemaKernel<out T> = {|
  readonly parse: null | ((mixed, PathBuffer) => Result<T>),
  readonly parseAsync: null | ((mixed, PathBuffer) => Promise<Result<T>>),
  readonly describe: () => Description,
|};

/**
 * The object a `Schema` is.
 *
 * `__input` is a phantom. `TInput` — the type a *valid input* has, which is
 * `string` for `pipe(string(), transform(Number))` whose output is `number` —
 * is never consumed at run time, because every parse takes `mixed`. It needs a
 * place in the type to be a parameter at all, and carrying it in return
 * position keeps it covariant, which is what lets `Schema<string, string>` be
 * passed where `Schema<mixed, mixed>` is wanted.
 */
type SchemaCarrier<out TOutput, out TInput> = {|
  readonly __kind: "Schema",
  readonly __input: () => TInput,
  readonly __kernel: SchemaKernel<TOutput>,
|};

/**
 * A parser from `mixed` to `TOutput`, whose valid inputs are `TInput`.
 *
 * Opaque with a supertype bound rather than fully opaque, and the difference
 * is what makes this package more than one file. Fully opaque, `object.js`
 * could not read the kernel out of a schema `primitive.js` built, so every
 * builder would have to live beside the type — which is the argument
 * `@uniflowed/effect` makes for staying in one module, and it is a real one.
 * The bound splits the guarantee in two: any module may *read* a schema, and
 * only this one may *mint* one, because `SchemaCarrier` is not exported and
 * [`makeSchema`] is the only thing that returns the opaque type. An
 * application still cannot forge a schema, hand-write a kernel, or depend on
 * the carrier's shape.
 *
 * `TInput` defaults to `mixed` so that `Schema<User>` keeps meaning "a schema
 * that produces a `User`" for the callers that only care about the output —
 * `@uniflowed/form`'s resolver, `@uniflowed/fetch`'s response parser — and
 * keeps compiling unchanged.
 */
export opaque type Schema<out TOutput, out TInput = mixed>: SchemaCarrier<
  TOutput,
  TInput,
> = SchemaCarrier<TOutput, TInput>;

const ASYNC_MESSAGE =
  "@uniflowed/validator: this schema has an asynchronous step in it; use parseAsync or safeParseAsync";

/**
 * The one value every schema's `__input` holds.
 *
 * It is never called. `TInput` exists so that [`InferInput`] has something to
 * read, and a thrown error is how that is enforced rather than asserted.
 */
const phantomInput = (): empty => {
  throw new Error("@uniflowed/validator: __input is a type-level marker and has no value");
};

/** Mint a synchronous schema. The only way a `Schema` comes into existence. */
export function makeSchema<TOutput, TInput>(
  parse: (mixed, PathBuffer) => Result<TOutput>,
  description: () => Description,
): Schema<TOutput, TInput> {
  return {
    __kind: "Schema",
    __input: phantomInput,
    __kernel: { parse, parseAsync: null, describe: description },
  };
}

/**
 * Mint an asynchronous schema.
 *
 * Its `parse` is null, which is what [`run`] refuses and what a composite
 * reads to decide it is asynchronous too.
 */
export function makeAsyncSchema<TOutput, TInput>(
  parseAsync: (mixed, PathBuffer) => Promise<Result<TOutput>>,
  description: () => Description,
): Schema<TOutput, TInput> {
  return {
    __kind: "Schema",
    __input: phantomInput,
    __kernel: { parse: null, parseAsync, describe: description },
  };
}

/** Whether `schema` needs [`safeParseAsync`]. Decided when it was built. */
export function isAsync(schema: Schema<mixed, mixed>): boolean {
  return schema.__kernel.parse == null;
}

/** What `schema` accepts, as data. See [`Description`]. */
export function describe(schema: Schema<mixed, mixed>): Description {
  return schema.__kernel.describe();
}

/** Run `schema` where the walk currently is. Throws if `schema` is async. */
export function run<T>(schema: Schema<T, mixed>, value: mixed, path: PathBuffer): Result<T> {
  const parse = schema.__kernel.parse;
  if (parse == null) {
    throw new Error(ASYNC_MESSAGE);
  }
  return parse(value, path);
}

/**
 * Run `schema` one step deeper in the path.
 *
 * The push/pop pair is why the buffer stays balanced even when a nested schema
 * returns early: nothing between them can throw except user code inside a
 * `transform` or a `check`, and a schema that raised has already failed the
 * whole parse.
 */
export function runAt<T>(
  schema: Schema<T, mixed>,
  value: mixed,
  path: PathBuffer,
  key: string,
): Result<T> {
  path.push(key);
  const result = run(schema, value, path);
  path.pop();
  return result;
}

/** Run `schema`, awaiting it if it is asynchronous and calling it if it is not. */
export function runAsync<T>(
  schema: Schema<T, mixed>,
  value: mixed,
  path: PathBuffer,
): Promise<Result<T>> {
  const kernel = schema.__kernel;
  const parseAsync = kernel.parseAsync;
  if (parseAsync != null) {
    return parseAsync(value, path);
  }
  return Promise.resolve(run(schema, value, path));
}

/**
 * Run `schema` several steps deeper.
 *
 * `map` is the caller that needs more than one segment: an entry's key and its
 * value are two different places to fail, and `["3", "key"]` says which
 * without inventing a punctuation the rest of the package would have to parse.
 */
export function runUnder<T>(
  schema: Schema<T, mixed>,
  value: mixed,
  path: PathBuffer,
  keys: Path,
): Result<T> {
  for (const key of keys) {
    path.push(key);
  }
  const result = run(schema, value, path);
  for (let depth = 0; depth < keys.length; depth += 1) {
    path.pop();
  }
  return result;
}

/** Run `schema` deeper, on a path of its own. See the module docs. */
export function runAtAsync<T>(
  schema: Schema<T, mixed>,
  value: mixed,
  path: PathBuffer,
  keys: Path,
): Promise<Result<T>> {
  return runAsync(schema, value, path.concat(keys));
}

/** One child of a composite, for [`collectAsync`]. */
export type Job = {|
  readonly keys: Path,
  readonly schema: Schema<mixed, mixed>,
  readonly value: mixed,
|};

/** Every child's outcome, in the order the jobs were given. */
export type Collected =
  | {| readonly ok: true, readonly values: $ReadOnlyArray<mixed> |}
  | {| readonly ok: false, readonly issues: $ReadOnlyArray<Issue> |};

/**
 * Run every child at once, and report all of their issues.
 *
 * The one function that keeps `object`, `array`, `tuple`, `record`, `map` and
 * `set` from each growing a second copy of the same loop: their asynchronous
 * halves differ only in how they name their children and what they build out
 * of the answers.
 *
 * `Promise.all` rather than a loop of `await`s, because a form whose two
 * fields each ask a server should cost one round trip. Issues still come back
 * in child order, because the results are folded in the order they were
 * requested rather than the order they settled.
 */
export async function collectAsync(
  jobs: $ReadOnlyArray<Job>,
  path: PathBuffer,
): Promise<Collected> {
  const results = await Promise.all(
    jobs.map((job) => runAtAsync(job.schema, job.value, path, job.keys)),
  );
  const values: Array<mixed> = [];
  let issues: null | Array<Issue> = null;
  for (const result of results) {
    match (result) {
      {ok: true, value: const value} => {
        values.push(value);
      }
      {ok: false, issues: const found} => {
        const into = issues ?? [];
        for (const entry of found) {
          into.push(entry);
        }
        issues = into;
      }
    }
  }
  return issues == null ? { ok: true, values } : { ok: false, issues };
}

/** A successful result. */
export function ok<T>(value: T): Result<T> {
  return { ok: true, value };
}

/** A result carrying one issue, at wherever the walk is. */
export function fail(code: string, message: string, path: Path): Result<empty> {
  return { ok: false, issues: [issue(code, message, path)] };
}

/** Append `result`'s issues to `issues`, if it has any. */
export function mergeIssues(issues: Array<Issue>, result: Result<mixed>): void {
  match (result) {
    {ok: false, issues: const found} => {
      for (const entry of found) {
        issues.push(entry);
      }
    }
    _ => {}
  }
}
