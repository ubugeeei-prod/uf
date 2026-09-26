// @flow
//
// `@uniflowed/validator/object`: an object whose keys are known when the
// schema is written.
//
// Three of them, and the only difference is what happens to a key the shape
// does not name:
//
// - [`object`] drops it. The right default for reading somebody else's
//   payload, where a new field appearing upstream is not your emergency.
// - [`strictObject`] rejects it. The right answer for a configuration file or
//   an internal API, where an unrecognised key is almost always a typo the
//   user would rather hear about than have ignored.
// - [`looseObject`] keeps it. For a boundary that has to forward what it did
//   not understand — a webhook that is re-signed, a document that round-trips
//   through a form and must come back whole.
//
// One walk serves all three, because "which keys does the shape name" is the
// same question in each and only the fourth line of the answer differs.
//
// # Why these take a shape and not a schema
//
// `partial({ name: string(), age: number() })`, not `partial(User)`. A schema
// here is a closure and a description, not a reified list of fields, so there
// is nothing on a built schema for `partial` to take apart — and adding one
// would mean every object schema carried its shape for the benefit of the
// callers that reshape it. Naming the shape once and passing it to both is the
// same amount of typing and keeps the built schema the size of its job:
//
//   const account = { name: string(), age: number() };
//   const Account = object(account);
//   const Draft = partial(account);
//
// Valibot's `pick`, `omit` and `required` are absent for the same reason, and
// because over a shape they are object literal manipulation the language
// already has: `object({ name: account.name })` is `pick`.
//
// # Reading and writing
//
// Every read of an untrusted key goes through `plain-object.js`, and so does
// every write. `object.js` never touches `source[key]` or `out[key] = …`
// directly, and the reason is written down there.

import type { Shape, ShapeInput, ShapeOutput } from "./infer.js";
import type { Issue, PathBuffer } from "./issue.js";
import { issue } from "./issue.js";
import { isPlainObject, ownKeys, ownValue, plainRecord, put } from "./plain-object.js";
import type { Description, Result, Schema } from "./schema.js";
import {
  collectAsync,
  describe,
  fail,
  isAsync,
  makeAsyncSchema,
  makeSchema,
  mergeIssues,
  ok,
  runAt,
} from "./schema.js";
import { optional } from "./optional.js";

/** What an object schema does with a key its shape does not name. */
type UnknownKeys = "strip" | "reject" | "keep";

/**
 * Pairs of `[key, schema]`, resolved once when the schema is built.
 *
 * `Object.keys(shape)` on every parse re-reads the same descriptors for the
 * life of the process. The shape cannot change after construction, so the walk
 * belongs at construction.
 */
function shapeEntries(shape: Shape): $ReadOnlyArray<[string, Schema<mixed, mixed>]> {
  return Object.keys(shape).map((key) => [key, shape[key]]);
}

/** Issues for every own key the shape does not name. Empty unless rejecting. */
function unknownKeyIssues(
  source: { readonly [string]: mixed, ... },
  named: Set<string>,
  path: PathBuffer,
): Array<Issue> {
  const issues: Array<Issue> = [];
  for (const key of ownKeys(source)) {
    if (!named.has(key)) {
      path.push(key);
      issues.push(issue("unknown_key", `unexpected key ${key}`, path));
      path.pop();
    }
  }
  return issues;
}

/** Copy every own key the shape does not name into the output, unchecked. */
function keepUnknownKeys(
  source: { readonly [string]: mixed, ... },
  named: Set<string>,
  out: { [string]: mixed, ... },
): void {
  for (const key of ownKeys(source)) {
    if (!named.has(key)) {
      put(out, key, ownValue(source, key));
    }
  }
}

/**
 * The one object parser.
 *
 * `TOutput` and `TInput` are supplied by the three exported spellings; nothing
 * at run time depends on them, which is why one implementation can serve an
 * exact result, a `Partial` one and an inexact one alike.
 */
function buildObject<TOutput, TInput>(
  shape: Shape,
  unknownKeys: UnknownKeys,
): Schema<TOutput, TInput> {
  const entries = shapeEntries(shape);
  const named = new Set(entries.map(([key]) => key));
  const description = (): Description => ({
    kind: "object",
    entries: entries.map(([key, schema]) => [key, describe(schema)]),
    unknownKeys,
  });

  function assemble(
    source: { readonly [string]: mixed, ... },
    out: { [string]: mixed, ... },
    found: Array<Issue>,
    path: PathBuffer,
  ): Result<TOutput> {
    const issues =
      unknownKeys === "reject" ? found.concat(unknownKeyIssues(source, named, path)) : found;
    if (issues.length > 0) {
      return { ok: false, issues };
    }
    if (unknownKeys === "keep") {
      keepUnknownKeys(source, named, out);
    }
    // $FlowFixMe[incompatible-type] every retained key was produced by the shape.
    return ok(out as TOutput);
  }

  if (entries.some(([, schema]) => isAsync(schema))) {
    return makeAsyncSchema(async (value, path) => {
      if (!isPlainObject(value)) {
        return fail("type", "expected object", path);
      }
      const source = plainRecord(value);
      const collected = await collectAsync(
        entries.map(([key, schema]) => ({ keys: [key], schema, value: ownValue(source, key) })),
        path,
      );
      const out: { [string]: mixed, ... } = {};
      const found: Array<Issue> = [];
      match (collected) {
        {ok: true, values: const values} => {
          entries.forEach(([key], index) => {
            put(out, key, values[index]);
          });
        }
        {ok: false, issues: const collectedIssues} => {
          for (const entry of collectedIssues) {
            found.push(entry);
          }
        }
      }
      return assemble(source, out, found, path);
    }, description);
  }

  return makeSchema((value, path) => {
    if (!isPlainObject(value)) {
      return fail("type", "expected object", path);
    }
    const source = plainRecord(value);
    const out: { [string]: mixed, ... } = {};
    const found: Array<Issue> = [];
    for (const [key, schema] of entries) {
      const result = runAt(schema, ownValue(source, key), path, key);
      if (result.ok) {
        put(out, key, result.value);
      } else {
        mergeIssues(found, result);
      }
    }
    return assemble(source, out, found, path);
  }, description);
}

/** An object with exactly the shape's keys; anything else is dropped. */
export function object<TShape extends Shape>(
  shape: TShape,
): Schema<ShapeOutput<TShape>, ShapeInput<TShape>> {
  return buildObject(shape, "strip");
}

/**
 * An object that rejects keys the shape does not name.
 *
 * The unknown-key scan runs whether or not the fields parsed. Returning early
 * on a field failure meant `{ name: 1, extra: true }` reported the wrong type
 * of `name` and said nothing about `extra`, so fixing the first error revealed
 * the second — which is the whole reason this validator collects issues
 * instead of stopping at one.
 */
export function strictObject<TShape extends Shape>(
  shape: TShape,
): Schema<ShapeOutput<TShape>, ShapeInput<TShape>> {
  return buildObject(shape, "reject");
}

/**
 * An object that keeps the keys the shape does not name.
 *
 * The extra keys are in the parsed value and not in its type: they are
 * `mixed`, because nothing validated them. The result type is inexact, which
 * is Flow saying exactly that.
 */
export function looseObject<TShape extends Shape>(
  shape: TShape,
): Schema<{ ...ShapeOutput<TShape>, ... }, { ...ShapeInput<TShape>, ... }> {
  return buildObject(shape, "keep");
}

/**
 * Every field of `shape`, each allowed to be missing.
 *
 * A key that was absent is present in the result holding `undefined`, rather
 * than absent from it. One shape for the parsed value means a consumer reads
 * `draft.name` without asking whether the key exists, and `Partial` says the
 * same thing to the checker.
 */
export function partial<TShape extends Shape>(
  shape: TShape,
): Schema<Partial<ShapeOutput<TShape>>, Partial<ShapeInput<TShape>>> {
  const partialShape: { [string]: Schema<mixed, mixed>, ... } = {};
  for (const key of Object.keys(shape)) {
    put(partialShape, key, optional(shape[key]));
  }
  return buildObject(partialShape, "strip");
}
