// @flow
//
// `@uniflowed/std/errors`: Go's `errors`, over JavaScript's `cause`.
//
// JavaScript has had error chaining since ES2022 — `new Error(msg, { cause })`
// — and nothing that reads one. So every codebase writes the same walk, badly:
//
// ```js
// // The version everyone writes, and the three things wrong with it.
// let e = error;
// while (e) {
//   if (e instanceof TimeoutError) return true;
//   e = e.cause;
// }
// ```
//
// It loops forever on a cycle, it cannot see into an `AggregateError`, and it
// is written once per predicate. Go solved this in `errors` and the answer
// ports cleanly, because the two languages disagree about almost nothing here:
// a wrapped error is an error that holds another, and the question a caller
// asks is "is this *about* X", not "is this X".
//
// # The two questions, and why they are different functions
//
// `is` asks whether something in the chain **is a particular value** — a
// sentinel. `as` asks whether something in the chain **is a particular class**,
// and hands it back at that type so its fields can be read. Go splits them for
// the same reason, and the split is what makes the second one typed:
//
// ```js
// if (is(error, NOT_FOUND)) return null;              // a sentinel
// const http = as(error, HttpError);                  // HttpError | null
// if (http != null) retryAfter(http.status);          // .status is a number
// ```
//
// `as` infers its result from the class it is handed. There is no annotation
// at that call site and no `any` behind it: `tests/type-tests/std-errors.js`
// fails the build if `as(error, HttpError)` ever starts typing as something a
// caller has to cast.
//
// # What is walked
//
// The chain is a *tree*, not a list, because `join` exists. Each node
// contributes its `cause` and, when it is an `AggregateError`, every entry of
// its `errors` — which is what makes `is` work through a `Promise.all` that
// collected several failures. The walk is breadth-first over that tree, it
// visits every node once, and a cycle is a node it has already seen rather
// than a hang. Depth is capped as a second line of defence; see `MAX_DEPTH`.
//
// # What this deliberately does not do
//
// It does not format. Go's `fmt.Errorf` is a formatter that happens to wrap,
// and JavaScript already has template literals — `wrap(\`loading ${id}\`, e)`
// needs no `%w`. It does not install itself on `Error.prototype`, so nothing
// here changes how an error from somebody else's library behaves. And it does
// not stringify the chain into the message: the cause is a value the chain
// still holds, so a reporter can render it however it likes, and a message
// that had already flattened its cause could not be walked at all.

/**
 * How deep a chain is followed before the walk gives up.
 *
 * The `seen` set already makes a cycle terminate, so this is not the cycle
 * guard — it is the guard against a chain that is merely *enormous*, which is
 * what a retry loop that wraps its own last failure produces. A thousand is
 * far past any chain a person would write and far short of a stack anyone
 * would notice.
 */
const MAX_DEPTH = 1000;

/**
 * An error that decides for itself what it matches.
 *
 * Go spells this `Is(error) bool`, and it exists for the case identity cannot
 * cover: an error carrying an OS errno matches a sentinel for that errno
 * without being that object. Implementing it is optional and nothing here
 * requires it — a class with no `matches` is compared by identity, which is
 * what a sentinel wants.
 *
 * It is `matches` rather than `is` because `is` is the name of the function
 * asking the question, and an error whose method and the caller's verb are the
 * same word reads as recursion in every stack trace that shows both.
 */
export interface Matcher {
  matches(target: mixed): boolean;
}

/** Anything that could be an error: this module never assumes it was given one. */
export type Chainable = mixed;

/**
 * Wrap `cause` in a new error that says what was being attempted.
 *
 * The equivalent of Go's `fmt.Errorf("...: %w", err)`, with the formatting left
 * to the language that already has it. The result is an ordinary `Error`, so
 * anything that logs errors logs this one, and `is`, `as` and `chain` see
 * through it to what it holds.
 *
 * ```js
 * try {
 *   await readConfig(path);
 * } catch (cause) {
 *   throw wrap(`reading ${path}`, cause);
 * }
 * ```
 */
export function wrap(message: string, cause: Chainable): Error {
  return new Error(message, { cause });
}

/**
 * The error one level under `error`, or `undefined` when there is none.
 *
 * Go's `errors.Unwrap`. Rarely what a caller wants — `is` and `as` walk the
 * whole chain, and a hand-written loop over `unwrap` is the code this module
 * exists to delete — but it is here because a reporter that renders one frame
 * at a time needs exactly this.
 *
 * `undefined` rather than `null`, because that is what reading `.cause` off an
 * error without one gives, and a second spelling of "nothing" is a second
 * check every caller has to write.
 */
export function unwrap(error: Chainable): mixed {
  if (error == null || typeof error !== "object") {
    return undefined;
  }
  // Reading a property off `mixed` needs the object to be typed as one that
  // may have it; `cause` is optional on every error and absent on most values.
  const holder = error as { readonly cause?: mixed, ... };
  return holder.cause;
}

/**
 * Whether anything in `error`'s chain is `target`.
 *
 * Identity, and then the error's own opinion: a node implementing
 * [`Matcher`] is asked, which is how a class of errors matches a sentinel it
 * merely carries. Both are tried at every node in the tree, so a sentinel
 * three wraps down inside one branch of a `join` is still found.
 *
 * ```js
 * export const CANCELLED: Error = new Error("cancelled");
 * // ...
 * if (is(failure, CANCELLED)) return;
 * ```
 *
 * A `null` or `undefined` target matches nothing, deliberately: `is(e, e.cause)`
 * on an error with no cause would otherwise be true for every unwrapped error
 * in the program, which is the kind of accident that turns a `catch` into a
 * silent `return`.
 */
export function is(error: Chainable, target: Chainable): boolean {
  if (target == null) {
    return false;
  }
  return walk(error, (node) => {
    if (node === target) {
      return true;
    }
    const matcher = asMatcher(node);
    return matcher != null && matcher.matches(target) === true;
  });
}

/**
 * The first error in `error`'s chain that is an instance of `kind`, or `null`.
 *
 * Go's `errors.As`, which needs a pointer to a typed variable because Go has
 * no way to return one; Flow does, so this returns the value at the type the
 * class names and there is no out-parameter.
 *
 * ```js
 * class HttpError extends Error {
 *   status: number;
 *   constructor(status: number) {
 *     super(`HTTP ${status}`);
 *     this.status = status;
 *   }
 * }
 *
 * const http = as(failure, HttpError);
 * if (http != null && http.status === 429) {
 *   // `http.status` is a number here, inferred from HttpError alone.
 * }
 * ```
 *
 * The first match in breadth-first order, which matters when a `join` holds two
 * errors of the same class: the one that was joined first is the one returned,
 * and that order is the order the caller wrote.
 */
export function as<T>(error: Chainable, kind: Class<T>): T | null {
  let found: T | null = null;
  walk(error, (node) => {
    if (node instanceof kind) {
      found = node;
      return true;
    }
    return false;
  });
  return found;
}

/**
 * One error standing for several, or `null` when there are none.
 *
 * Go's `errors.Join`. `null` for an empty list rather than an empty aggregate,
 * and every nullish entry is dropped, so the shape a caller collects errors in
 * — push on failure, nothing on success — needs no filtering before it gets
 * here:
 *
 * ```js
 * const failures = results.map((r) => r.error); // some are undefined
 * const failed = join(...failures);
 * if (failed != null) throw failed;
 * ```
 *
 * The result is an `AggregateError`, which is the platform's own name for this
 * and what `Promise.any` already throws — so a reporter that knows how to
 * render one renders this too. `is` and `as` walk into every branch of it.
 *
 * A single error is returned as itself rather than wrapped in an aggregate of
 * one. An aggregate of one says the caller collected a list, which is true, and
 * costs every reader of the result a layer to see through, which is not worth
 * it.
 */
export function join(...errors: $ReadOnlyArray<Chainable>): Error | null {
  const present = errors.filter((error) => error != null);
  if (present.length === 0) {
    return null;
  }
  if (present.length === 1) {
    const only = present[0];
    if (only instanceof Error) {
      return only;
    }
  }
  return new AggregateError(present, `${String(present.length)} errors occurred`);
}

/**
 * Every error reachable from `error`, in the order the walk finds them.
 *
 * `error` itself first, then its cause and its aggregate members, then theirs.
 * For a chain with no branches this is exactly the list a hand-written
 * `while (e) e = e.cause` produces, and unlike that loop it terminates on a
 * cycle.
 *
 * For reporting, not for control flow: reaching into this array to decide what
 * to do is `is` or `as` written out longhand, and both of those stop as soon as
 * they have an answer.
 */
export function chain(error: Chainable): $ReadOnlyArray<mixed> {
  const found: Array<mixed> = [];
  walk(error, (node) => {
    found.push(node);
    return false;
  });
  return found;
}

/**
 * `node` as a [`Matcher`], or `null` when it does not implement one.
 *
 * A property named `matches` that is not callable is not a matcher, and a
 * value that reached here from JSON could easily have one — so the check is on
 * the function, not on the name.
 */
function asMatcher(node: mixed): Matcher | null {
  if (node == null || typeof node !== "object") {
    return null;
  }
  const holder = node as { readonly matches?: mixed, ... };
  if (typeof holder.matches !== "function") {
    return null;
  }
  // The property is callable and takes the one argument the interface names;
  // nothing weaker than a cast can say that about a `mixed`.
  // $FlowFixMe[incompatible-type]
  return node as Matcher;
}

/**
 * Visit every error reachable from `error` until `found` says to stop.
 *
 * Breadth-first, because the two callers that stop early both want the
 * *nearest* match: an `as` that returned the deepest `HttpError` in a chain
 * would hand back the transport failure rather than the response the caller
 * asked about.
 *
 * The queue holds nodes not yet visited and `seen` holds those already
 * enqueued, which is what makes a cycle finite: `a.cause = b; b.cause = a` is
 * two nodes and two visits. `seen` is keyed by identity, so two structurally
 * identical errors are two nodes, which is right — they are two failures.
 */
function walk(error: Chainable, found: (node: mixed) => boolean): boolean {
  const seen = new Set<mixed>();
  let frontier: Array<mixed> = [error];
  seen.add(error);
  for (let depth = 0; depth < MAX_DEPTH && frontier.length > 0; depth += 1) {
    const next: Array<mixed> = [];
    for (const node of frontier) {
      if (node == null) {
        continue;
      }
      if (found(node)) {
        return true;
      }
      for (const child of childrenOf(node)) {
        if (child != null && !seen.has(child)) {
          seen.add(child);
          next.push(child);
        }
      }
    }
    frontier = next;
  }
  return false;
}

/**
 * What hangs below one node: its cause, and an aggregate's members.
 *
 * Both, rather than one or the other. `AggregateError` accepts a `cause` like
 * every other error, and an aggregate built by `join` inside a `catch` that
 * itself wrapped something has both a list and a cause — dropping either is a
 * branch of the tree that `is` would silently not search.
 */
function childrenOf(node: mixed): $ReadOnlyArray<mixed> {
  if (node == null || typeof node !== "object") {
    return [];
  }
  const holder = node as { readonly cause?: mixed, readonly errors?: mixed, ... };
  const children: Array<mixed> = [];
  if (holder.cause != null) {
    children.push(holder.cause);
  }
  // `errors` on an `AggregateError`, and on anything else shaped like one:
  // duck-typed rather than an `instanceof`, because an aggregate that crossed a
  // realm — a worker, an iframe, a `structuredClone` — is not an instance of
  // *this* realm's `AggregateError` and is still the same tree.
  if (Array.isArray(holder.errors)) {
    for (const child of holder.errors) {
      children.push(child);
    }
  }
  return children;
}
