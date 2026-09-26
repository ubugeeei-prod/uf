// @flow
//
// `@uniflowed/validator/issue`: what a failure is, and where it happened.
//
// A validator that answers "no" has told the caller nothing. A form needs to
// put a message under one input, an HTTP handler needs to name the field in
// its 422 body, and a log needs to say which row of a thousand was wrong. All
// three want the same thing: the path from the root of the value to the place
// that failed, as data.
//
// So an issue is `{ code, message, path }` — `["users", "2", "email"]`, not
// `"users[2].email"`. Segments compose without a grammar: joining them with a
// dot is one line at the boundary that wants a string, and splitting a string
// back into segments is a parser nobody should have to write. Array indices
// are their decimal spelling, because a path is a path whether the container
// was an object or an array, and a consumer that has to branch on the segment
// type gains nothing from the distinction.
//
// # Why this is separate from the schemas
//
// Because nothing here knows what a schema is. `issue.js` is a leaf: it is
// imported by the kernel, by every combinator that reports a failure, and by
// `@uniflowed/form`, and it imports one thing itself. Keeping it that way is
// what lets a consumer translate issues — into field errors, into a response
// body — without resolving the schema engine at all.

import { put } from "./plain-object.js";

/** Where an issue happened, as object keys and array indices from the root. */
export type Path = $ReadOnlyArray<string>;

/**
 * The mutable buffer the synchronous walk descends with.
 *
 * `schema.js` explains why one array is pushed and popped rather than a fresh
 * array being allocated per field. The type is separate from [`Path`] so that
 * the distinction between "the buffer, which is being mutated right now" and
 * "a path, which is a value" is visible in every signature.
 */
export type PathBuffer = Array<string>;

/**
 * One reason a value was rejected.
 *
 * `code` is for programs — `"type"`, `"min_length"`, `"unknown_key"` — and is
 * stable across message changes, so a caller can tell "this is not an email
 * address" from "we need an email address" without matching on prose.
 *
 * `path` is absent rather than empty when the issue is about the whole value,
 * because the overwhelmingly common case is a successful parse and an object
 * with one fewer field is one fewer allocation on the path that matters.
 */
export type Issue = {|
  readonly code: string,
  readonly message: string,
  readonly path?: Path,
|};

/**
 * An issue at wherever the walk currently is.
 *
 * The `slice` is the only copy of a path the package makes, and it happens
 * exactly when a value was going to be rejected anyway.
 */
export function issue(code: string, message: string, path: Path): Issue {
  return path.length === 0 ? { code, message } : { code, message, path: path.slice() };
}

/** An issue at `path` with `keys` appended: a cross-field rule's landing spot. */
export function issueUnder(code: string, message: string, path: Path, keys: Path): Issue {
  const at = path.concat(keys);
  return at.length === 0 ? { code, message } : { code, message, path: at };
}

function describeIssue(entry: Issue): string {
  const at = entry.path == null || entry.path.length === 0 ? "" : ` at ${entry.path.join(".")}`;
  return `${entry.message}${at}`;
}

/**
 * What [`parse`] raises.
 *
 * A real `Error` subclass so it survives `instanceof`, logging and a `catch`
 * that only knows about errors, and it carries `issues` so a caller can build
 * a field-by-field response without parsing the message back apart.
 */
export class ValidationError extends Error {
  readonly issues: $ReadOnlyArray<Issue>;

  constructor(issues: $ReadOnlyArray<Issue>) {
    super(issues.map(describeIssue).join("; "));
    this.name = "ValidationError";
    this.issues = issues;
  }
}

/**
 * Issues grouped the way a form renders them.
 *
 * `root` is everything that was about the value as a whole; `nested` is keyed
 * by the dotted path, which is the same string `@uniflowed/form`'s `register`
 * was given. Written through [`put`] because a payload's own `__proto__` key
 * reaches this function as a path segment.
 */
export type FlatIssues = {|
  readonly root: $ReadOnlyArray<string>,
  readonly nested: { readonly [string]: $ReadOnlyArray<string>, ... },
|};

/** Group `issues` by their path, for a caller that renders per field. */
export function flatten(issues: $ReadOnlyArray<Issue>): FlatIssues {
  const root: Array<string> = [];
  const nested: { [string]: Array<string>, ... } = {};
  for (const entry of issues) {
    const path = entry.path;
    if (path == null || path.length === 0) {
      root.push(entry.message);
      continue;
    }
    const key = path.join(".");
    const already = Object.hasOwn(nested, key) ? nested[key] : null;
    if (already == null) {
      put(nested, key, [entry.message]);
    } else {
      already.push(entry.message);
    }
  }
  return { root, nested };
}
