// @flow
//
// `@uniflowed/validator/union`: several schemas over one value.
//
// Three ways to combine schemas that all look at the *same* value, rather than
// at different parts of one. [`union`] accepts if any of them does, [`variant`]
// is the same thing when the value says which one to use, and [`intersect`] is
// the dual: accept only if all of them do. They are together because the walk
// is the same walk and the acceptance rule is the only line that differs.
//
// # Why `variant` exists when `union` would work
//
// It would, and its errors would be useless. A four-branch union of shapes,
// given `{ kind: "circle", radius: "big" }`, reports every reason the value is
// not a square, not a triangle and not a line, on top of the one reason that
// matters. A discriminated union knows which branch was meant before it starts
// — that is what the discriminant is for — so it runs that one and reports
// `expected number at radius`. An unmatched discriminant names the ones that
// exist, which is the other half of the error a union cannot give.
//
// So: reach for `variant` whenever the branches share a tag, and leave `union`
// for the cases that genuinely have none, like `string | number`.
//
// # Why the union runs its branches in order, even when it is asynchronous
//
// "The first schema that accepts" is the contract, and a branch is allowed to
// have a `checkAsync` in it that talks to a server. Running the branches at
// once would ask every server on every parse, including the ones whose branch
// an earlier one had already made irrelevant. Sequential is slower on a value
// that only the last branch accepts and correct on every value; the parallel
// version is faster and wrong.

import type { InferInput, InferOutput, Options, Shape } from "./infer.js";
import type { Issue } from "./issue.js";
import { isPlainObject, ownValue, plainRecord, put } from "./plain-object.js";
import type { Description, Result, Schema } from "./schema.js";
import {
  describe,
  fail,
  isAsync,
  makeAsyncSchema,
  makeSchema,
  mergeIssues,
  ok,
  run,
  runAsync,
} from "./schema.js";

function buildUnion<TOutput, TInput>(options: Options): Schema<TOutput, TInput> {
  const description = (): Description => ({
    kind: "union",
    options: options.map((option) => describe(option)),
  });

  if (options.some((option) => isAsync(option))) {
    return makeAsyncSchema(async (value, path) => {
      const issues: Array<Issue> = [];
      for (const option of options) {
        const result = await runAsync(option, value, path);
        if (result.ok) {
          // $FlowFixMe[incompatible-type] the branch that accepted produced the output type.
          return result as Result<TOutput>;
        }
        mergeIssues(issues, result);
      }
      return { ok: false, issues };
    }, description);
  }

  return makeSchema((value, path) => {
    const issues: Array<Issue> = [];
    for (const option of options) {
      const result = run(option, value, path);
      if (result.ok) {
        // $FlowFixMe[incompatible-type] the branch that accepted produced the output type.
        return result as Result<TOutput>;
      }
      mergeIssues(issues, result);
    }
    return { ok: false, issues };
  }, description);
}

/**
 * The first schema that accepts the value.
 *
 * When none do, every branch's issues are reported, because there is no way to
 * know which branch the author meant. That is also why [`variant`] exists.
 */
export function union<TOptions extends Options>(
  options: TOptions,
): Schema<InferOutput<TOptions[number]>, InferInput<TOptions[number]>> {
  return buildUnion(options);
}

/** Which branch a value asked for, or the issue that says it named none. */
type Chosen =
  | {| readonly found: true, readonly branch: Schema<mixed, mixed> |}
  | {| readonly found: false, readonly failure: Result<empty> |};

function buildVariant<TOutput, TInput>(key: string, branches: Shape): Schema<TOutput, TInput> {
  const known = Object.keys(branches);
  const message = `expected one of ${known.join(", ")}`;
  const description = (): Description => ({
    kind: "variant",
    key,
    branches: known.map((name) => [name, describe(branches[name])]),
  });

  /** The branch the value asked for, or the issue saying it asked for nothing. */
  function choose(value: mixed, path: Array<string>): Chosen {
    if (!isPlainObject(value)) {
      return { found: false, failure: fail("type", "expected object", path) };
    }
    const discriminant = ownValue(plainRecord(value), key);
    if (typeof discriminant !== "string" || !Object.hasOwn(branches, discriminant)) {
      path.push(key);
      const failure = fail("variant", message, path);
      path.pop();
      return { found: false, failure };
    }
    return { found: true, branch: branches[discriminant] };
  }

  if (known.some((name) => isAsync(branches[name]))) {
    return makeAsyncSchema(async (value, path) => {
      const chosen = choose(value, path);
      if (!chosen.found) {
        return chosen.failure;
      }
      // $FlowFixMe[incompatible-type] a branch's output type is the variant's.
      return (await runAsync(chosen.branch, value, path)) as Result<TOutput>;
    }, description);
  }

  return makeSchema((value, path) => {
    const chosen = choose(value, path);
    if (!chosen.found) {
      return chosen.failure;
    }
    // $FlowFixMe[incompatible-type] a branch's output type is the variant's.
    return run(chosen.branch, value, path) as Result<TOutput>;
  }, description);
}

/**
 * A union chosen by the value of one key.
 *
 * The discriminant is read first and the matching branch is the only one run.
 * A discriminant that is missing, is not a string, or names no branch is
 * reported at the discriminant's own path, so a form can put the message on
 * the control that chooses it.
 */
export function variant<TBranches extends Shape>(
  key: string,
  branches: TBranches,
): Schema<InferOutput<TBranches[keyof TBranches]>, InferInput<TBranches[keyof TBranches]>> {
  return buildVariant(key, branches);
}

/**
 * Both schemas, over the same value.
 *
 * Binary rather than variadic: `intersect(intersect(a, b), c)` is the third
 * one, the type is `A & B` with nothing for the checker to fold, and there is
 * no arity table to keep in step with an implementation.
 *
 * Both sides run, and both sides' issues are reported, for the same reason
 * `object` does not stop at the first bad field.
 *
 * What the result *is* depends on what the two produced. Two plain objects are
 * merged, with the right-hand side winning a shared key — which is what makes
 * `intersect(object(base), object(extra))` mean what it looks like. Two
 * identical values are that value. Anything else is a `intersect` issue rather
 * than a guess, because there is no defensible way to merge a `Date` with a
 * string and pretend the result satisfies both.
 */
export function intersect<TLeftOut, TLeftIn, TRightOut, TRightIn>(
  left: Schema<TLeftOut, TLeftIn>,
  right: Schema<TRightOut, TRightIn>,
): Schema<TLeftOut & TRightOut, TLeftIn & TRightIn> {
  const description = (): Description => ({
    kind: "intersect",
    parts: [describe(left), describe(right)],
  });

  function combine(
    first: Result<TLeftOut>,
    second: Result<TRightOut>,
    path: Array<string>,
  ): Result<TLeftOut & TRightOut> {
    if (!first.ok || !second.ok) {
      const issues: Array<Issue> = [];
      mergeIssues(issues, first);
      mergeIssues(issues, second);
      return { ok: false, issues };
    }
    if (isPlainObject(first.value) && isPlainObject(second.value)) {
      const merged: { [string]: mixed, ... } = {};
      for (const source of [plainRecord(first.value), plainRecord(second.value)]) {
        for (const key of Object.keys(source)) {
          put(merged, key, ownValue(source, key));
        }
      }
      // $FlowFixMe[incompatible-type] both sides' own keys are in the merged object.
      return ok(merged as TLeftOut & TRightOut);
    }
    if (Object.is(first.value, second.value)) {
      // $FlowFixMe[incompatible-type] one value that both schemas accepted.
      return ok(first.value as TLeftOut & TRightOut);
    }
    return fail("intersect", "expected both sides to agree on one value", path);
  }

  if (isAsync(left) || isAsync(right)) {
    return makeAsyncSchema(async (value, path) => {
      const [first, second] = await Promise.all([
        runAsync(left, value, path),
        runAsync(right, value, path),
      ]);
      return combine(first, second, path);
    }, description);
  }

  return makeSchema(
    (value, path) => combine(run(left, value, path), run(right, value, path), path),
    description,
  );
}
