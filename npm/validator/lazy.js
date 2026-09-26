// @flow
//
// `@uniflowed/validator/lazy`: a schema that does not exist yet.
//
// A comment has replies, and a reply is a comment. There is no order in which
// to write that down, because the initialiser of `Comment` would have to name
// `Comment`, and at that moment it is still `undefined`. So the schema is
// wrapped in a function, and the function is not called until the first parse:
//
//   type Comment = {| text: string, replies: $ReadOnlyArray<Comment> |};
//
//   const Comment: Schema<Comment> = lazy(() =>
//     object({ text: string(), replies: array(Comment) }),
//   );
//
// The annotation is the other half, and it is not optional. Flow will infer
// the type of almost every schema in this package, but it will not solve a
// type that mentions itself; the `Schema<Comment>` on the binding is where the
// cycle is cut, and `infer.js` says so among the other limits.
//
// # Why the result is memoised
//
// Without it, every node of the tree calls `build()` and gets a fresh object
// graph — a thousand-comment thread would construct a thousand schemas, none
// of which is reused, on a parse that should have allocated nothing. With it,
// recursion costs one closure for the whole schema.
//
// # Why there are two of these
//
// Everything else in the package works out whether it is asynchronous when it
// is built: `object` asks its fields, `array` asks its item. A lazy schema
// cannot ask, because the thing it would ask does not exist yet, and building
// it to find out is the infinite loop the laziness was there to avoid.
//
// So the answer is declared instead of discovered. [`lazy`] is synchronous and
// [`lazyAsync`] is not, and a recursive schema with a `checkAsync` anywhere
// inside it must use the second — otherwise the objects and arrays above it
// build their synchronous halves, and the first `await` that never happens
// surfaces as a thrown "use safeParseAsync" from the middle of a parse rather
// than as a type the caller could have seen.

import type { Description, Schema } from "./schema.js";
import { describe, makeAsyncSchema, makeSchema, run, runAsync } from "./schema.js";

/** Build once, then hand back the same schema for the life of the process. */
function memoise<TOutput, TInput>(
  build: () => Schema<TOutput, TInput>,
): () => Schema<TOutput, TInput> {
  let built: null | Schema<TOutput, TInput> = null;
  return () => {
    const already = built;
    if (already != null) {
      return already;
    }
    const made = build();
    built = made;
    return made;
  };
}

/**
 * A synchronous schema built on first use.
 *
 * `id` is what a converter keys its definitions on: `json-schema.js` sees the
 * same symbol every time it walks back around the cycle, which is how a
 * recursive schema becomes a `$ref` rather than a stack overflow.
 */
export function lazy<TOutput, TInput = mixed>(
  build: () => Schema<TOutput, TInput>,
): Schema<TOutput, TInput> {
  const resolve = memoise(build);
  const id = Symbol("lazy");
  const description = (): Description => ({
    kind: "lazy",
    id,
    inner: () => describe(resolve()),
  });
  return makeSchema((value, path) => run(resolve(), value, path), description);
}

/** A recursive schema with an asynchronous step somewhere inside it. */
export function lazyAsync<TOutput, TInput = mixed>(
  build: () => Schema<TOutput, TInput>,
): Schema<TOutput, TInput> {
  const resolve = memoise(build);
  const id = Symbol("lazyAsync");
  const description = (): Description => ({
    kind: "lazy",
    id,
    inner: () => describe(resolve()),
  });
  return makeAsyncSchema((value, path) => runAsync(resolve(), value, path), description);
}
