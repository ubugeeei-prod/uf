// @flow
//
// `@uniflowed/std/context`: Go's `context`, over `AbortSignal`.
//
// `AbortSignal` is the web platform's answer to cancellation and it is a good
// one, so this does not replace it — every context here *has* one, and
// `ctx.signal()` is what goes to `fetch`. What the platform does not have is
// the three things Go's `context` adds around it:
//
// * **a tree.** Cancelling a request cancels everything it started, at any
//   depth, without each layer having to remember what it handed down.
// * **deadlines that compose.** A child may shorten a deadline and may not
//   extend one, so a five-second handler cannot be made to wait ten by a
//   library it called.
// * **typed request-scoped values.** A trace id that reaches the bottom of the
//   call stack without being a parameter on every function in between, and
//   without being a global that two concurrent requests share.
//
// `AbortSignal.any` and `AbortSignal.timeout` cover parts of the first two.
// They are also the two newest things on the interface — Deno, Bun, workerd and
// Node gained them at different times — so a module that needs to run on all
// of them cannot yet assume either. Nothing here uses them; a deadline is a
// `setTimeout` and a tree is a listener, both of which have worked everywhere
// for a decade.
//
// # The shape
//
// ```js
// const [ctx, cancel] = withTimeout(background(), 5_000);
// try {
//   const response = await fetch(url, { signal: ctx.signal() });
//   await handle(withValue(ctx, TRACE, id));
// } finally {
//   cancel();   // releases the timer; see "Timers" below
// }
// ```
//
// Every constructor returns `[context, cancel]` the way Go returns
// `(ctx, cancel)`, because the two are always needed together and a `cancel`
// that hangs off the context is a `cancel` any callee can reach.
//
// # Values are typed by their key
//
// ```js
// const TRACE = key<string>("trace-id");
// const traced = withValue(ctx, TRACE, requestId);
// const id = traced.value(TRACE);       // string | void — inferred from TRACE
// ```
//
// A `Key<T>` carries `T` and nothing else; two keys with the same name are
// still different keys, because a key is compared by identity. That is what
// makes this safe to use from a library: a key you did not export is a key
// nobody else can read or overwrite, which is the property a bare string key on
// a shared object does not have.
//
// # Timers
//
// `withTimeout` and `withDeadline` arm a `setTimeout`. On Node a pending timer
// keeps the process alive, and there is no runtime-agnostic `unref` — Deno and
// browsers do not have one — so **the `cancel` a deadline hands back has to be
// called**, in a `finally`, even on the path where nothing went wrong. A
// context that has been cancelled or has already fired holds no timer.
//
// This is the one place where this module asks something of the caller that Go
// does not, and it is why `cancel` is returned rather than hidden.

/**
 * A key for one request-scoped value, carrying the type of that value.
 *
 * Opaque, so the only way to make one is [`key`], and invariant in `T` — `T`
 * appears in both an argument and a return position of the carrier — because a
 * `Key<Dog>` used to read a context that stored an `Animal` would be a lie in
 * one direction and a `Key<Animal>` used to write a `Dog` would be a lie in the
 * other.
 */
export opaque type Key<T> = { readonly name: string, readonly carrier: (T) => T };

/**
 * A cancellation scope: a signal, a deadline, and the values under it.
 *
 * An interface rather than a class, so the only contexts that exist are the
 * ones the constructors below return. `new Context()` is not a thing a caller
 * can write, which keeps "every context has a parent or is the root" true by
 * construction rather than by convention.
 */
export interface Context {
  /**
   * The signal for this scope, aborted when it is cancelled.
   *
   * This is what goes to `fetch`, to an `addEventListener`, and to anything
   * else that already speaks the platform's cancellation. It is aborted with
   * the same error [`err`] reports, so `signal.reason` and `ctx.err()` never
   * disagree.
   */
  signal(): AbortSignal;

  /**
   * Why this scope ended, or `null` while it is still live.
   *
   * [`CANCELLED`] or [`DEADLINE_EXCEEDED`] for the two ordinary endings, and
   * whatever was passed to `cancel(reason)` when a caller supplied one. The
   * two sentinels are values, so `errors.is(failure, CANCELLED)` finds one
   * however deeply a caller wrapped it.
   */
  err(): mixed;

  /**
   * Resolves when this scope ends, and never if it does not.
   *
   * Go's `<-ctx.Done()`. A context with no cancellation — [`background`] — never
   * resolves this, exactly as Go's nil channel never fires, so awaiting it
   * alone is a hang. It is for racing: `Promise.race([work, ctx.done()])`.
   */
  done(): Promise<void>;

  /**
   * When this scope expires, as epoch milliseconds, or `null` for no deadline.
   *
   * The *effective* deadline, which is the earliest of this scope's and every
   * scope above it.
   */
  deadline(): number | null;

  /**
   * The value stored under `key`, at the type the key names, or `undefined`.
   *
   * Looks up the chain: the nearest scope that stored something under this key
   * wins, which is what lets a middleware shadow a value for the work below it
   * without touching what its own caller sees.
   */
  value<T>(key: Key<T>): T | void;
}

/** How a cancellable scope is handed back: the context, and how to end it. */
export type Cancellable = [Context, (reason?: mixed) => void];

/**
 * The reason a scope that was cancelled reports.
 *
 * A value rather than a class, so identity is the whole comparison and
 * `errors.is` finds it through any amount of wrapping. Go's `context.Canceled`
 * is a sentinel for the same reason.
 */
export const CANCELLED: Error = new Error("context cancelled");

/** The reason a scope that ran out of time reports. Go's `DeadlineExceeded`. */
export const DEADLINE_EXCEEDED: Error = new Error("context deadline exceeded");

/**
 * Make a key for a value of type `T`.
 *
 * ```js
 * const TRACE = key<string>("trace-id");
 * ```
 *
 * The name is for reading in a debugger and is not the identity: two keys made
 * with the same name are different keys, and neither can read the other's
 * value. Give the type argument explicitly — there is nothing else in the call
 * for Flow to infer it from, and a key whose `T` was inferred as `empty` reads
 * back as `empty` everywhere.
 */
export function key<T>(name: string): Key<T> {
  return { name, carrier: (value) => value };
}

/**
 * The root scope: never cancelled, no deadline, no values.
 *
 * Go's `context.Background()`, and used the same way — at the top of a request,
 * a job, a `main`. One object, because it holds nothing that could differ
 * between two of them.
 */
export function background(): Context {
  return ROOT;
}

/**
 * A scope that ends when the returned function is called, or when `parent` does.
 *
 * ```js
 * const [ctx, cancel] = withCancel(parent);
 * request.on("close", () => cancel());
 * ```
 *
 * `cancel` is idempotent and safe to call after the scope has already ended,
 * which is what makes it correct in a `finally`. Calling it with a reason ends
 * the scope with that reason instead of [`CANCELLED`]; calling it after the
 * scope has ended changes nothing, because the first ending is the one that
 * explains what happened.
 */
export function withCancel(parent: Context): Cancellable {
  const scope = new Scope(parent, parent.deadline());
  return [scope, (reason?: mixed) => scope.end(reason ?? CANCELLED)];
}

/**
 * A scope that ends `ms` from now, or when `parent` does, or on `cancel`.
 *
 * `withDeadline(parent, Date.now() + ms)`, which is how Go defines it too. See
 * the module header on timers: the returned `cancel` releases the timer, and on
 * Node a timer nobody released holds the process open until it fires.
 */
export function withTimeout(parent: Context, ms: number): Cancellable {
  return withDeadline(parent, Date.now() + ms);
}

/**
 * A scope that ends at `at`, or when `parent` does, or on `cancel`.
 *
 * `at` is epoch milliseconds — `Date.now()`'s units, and `Date.prototype`'s
 * through `date.getTime()`.
 *
 * A deadline later than the parent's is ignored: the effective deadline is the
 * earliest in the chain, so a callee cannot buy itself more time than its
 * caller allowed. A deadline already in the past ends the scope immediately,
 * rather than on the next turn of the event loop, so the code after it sees a
 * context that is already done.
 */
export function withDeadline(parent: Context, at: number): Cancellable {
  const above = parent.deadline();
  const effective = above == null ? at : Math.min(above, at);
  const scope = new Scope(parent, effective);
  const remaining = effective - Date.now();
  if (remaining <= 0) {
    scope.end(DEADLINE_EXCEEDED);
  } else {
    scope.arm(remaining);
  }
  return [scope, (reason?: mixed) => scope.end(reason ?? CANCELLED)];
}

/**
 * A scope like `parent` with one more value in it.
 *
 * No cancellation of its own: it ends exactly when `parent` does, and there is
 * nothing to release, which is why this one returns a context rather than a
 * pair. Storing is not mutation — `parent` cannot see the new value — so a
 * middleware that adds a value does not change what its caller reads.
 *
 * Values are for things that belong to the *request* rather than to the
 * function: a trace id, an authenticated user, a deadline-aware logger. A
 * parameter is better for everything else, because a parameter is checked at
 * every call and a context value is checked where it is read.
 */
export function withValue<T>(parent: Context, key: Key<T>, value: T): Context {
  return new Scope(parent, parent.deadline(), { key, value });
}

/**
 * Adopt an `AbortSignal` that somebody else owns.
 *
 * The bridge inwards: a server hands a handler `request.signal`, and this makes
 * it the root of a context tree without the handler having to own the
 * cancellation. There is no `cancel` because the signal's owner has it.
 *
 * An already-aborted signal produces an already-ended context, with the
 * signal's own `reason` as the error.
 */
export function fromSignal(signal: AbortSignal): Context {
  const scope = new Scope(ROOT, null);
  if (signal.aborted) {
    scope.end(signal.reason ?? CANCELLED);
  } else {
    signal.addEventListener("abort", () => scope.end(signal.reason ?? CANCELLED), { once: true });
  }
  return scope;
}

/**
 * One node of the context tree.
 *
 * Every context except the root is one of these, whether it was made by
 * `withCancel`, `withDeadline` or `withValue` — the three differ in what they
 * arm and what they store, not in what they are, and one class is what keeps
 * the lookup walk and the cancellation walk from drifting apart.
 */
class Scope implements Context {
  #parent: Context | null;
  #controller: AbortController = new AbortController();
  #deadline: number | null;
  /**
   * The one value this scope stores, if it stores one.
   *
   * The key is held as `mixed` rather than as a `Key<something>`: `Key` is
   * invariant, so there is no type a chain of scopes with different value types
   * could all be, and a `Key<mixed>` is not comparable to the `Key<T>` a lookup
   * arrives with. Identity is the whole comparison, and identity does not need
   * a type — the type comes back at the `return`, which is where the phantom
   * parameter is cashed in.
   */
  #entry: { readonly key: mixed, readonly value: mixed } | null;
  #error: mixed = null;
  #timer: TimeoutID | null = null;
  #detach: (() => void) | null = null;
  #done: Promise<void>;
  #settle: () => void = () => {};

  constructor(
    parent: Context,
    deadline: number | null,
    entry?: { readonly key: mixed, readonly value: mixed },
  ) {
    this.#parent = parent;
    this.#deadline = deadline;
    this.#entry = entry ?? null;
    this.#done = new Promise((resolve) => {
      this.#settle = resolve;
    });

    const above = parent.signal();
    if (above.aborted) {
      this.end(above.reason ?? CANCELLED);
      return;
    }
    // The listener is what makes cancellation reach the whole subtree, and
    // removing it is what keeps a long-lived parent from accumulating one per
    // short-lived child — the leak that makes a per-request context tree a
    // memory profile shaped like a staircase.
    const propagate = () => {
      this.end(above.reason ?? CANCELLED);
    };
    above.addEventListener("abort", propagate, { once: true });
    this.#detach = () => {
      above.removeEventListener("abort", propagate);
    };
  }

  signal(): AbortSignal {
    return this.#controller.signal;
  }

  err(): mixed {
    return this.#error;
  }

  done(): Promise<void> {
    return this.#done;
  }

  deadline(): number | null {
    return this.#deadline;
  }

  value<T>(wanted: Key<T>): T | void {
    let scope: Context | null = this;
    while (scope instanceof Scope) {
      const entry = scope.#entry;
      if (entry != null && entry.key === wanted) {
        // The key is invariant in `T` and the only way to have stored under
        // this key was `withValue<T>`, so the value is a `T`. Flow cannot carry
        // that through a heterogeneous chain of scopes, which is exactly what
        // the key's phantom type exists to make safe.
        // $FlowFixMe[incompatible-type]
        return entry.value as T;
      }
      scope = scope.#parent;
    }
    return undefined;
  }

  /**
   * Arm the deadline timer.
   *
   * Separate from the constructor because only two of the three constructors
   * want one, and because a deadline already in the past must end the scope
   * synchronously rather than on the next tick.
   */
  arm(ms: number): void {
    this.#timer = setTimeout(() => {
      this.end(DEADLINE_EXCEEDED);
    }, ms);
  }

  /**
   * End this scope, once.
   *
   * Idempotent, because three things race to call it — the caller's `cancel`,
   * the deadline, and the parent — and the first ending is the true one. It
   * releases the timer and the parent listener before aborting, so the
   * listeners this scope's own children run in are the only work left.
   */
  end(reason: mixed): void {
    if (this.#error != null) {
      return;
    }
    this.#error = reason;
    if (this.#timer != null) {
      clearTimeout(this.#timer);
      this.#timer = null;
    }
    if (this.#detach != null) {
      this.#detach();
      this.#detach = null;
    }
    this.#controller.abort(reason);
    this.#settle();
  }
}

/**
 * The root: no parent, no deadline, and a signal that is never aborted.
 *
 * A module-level constant rather than a fresh object per `background()` call,
 * because every one of them would be identical and every context tree would
 * otherwise hold its own dead root.
 */
const ROOT: Context = {
  signal: () => ROOT_CONTROLLER.signal,
  err: () => null,
  deadline: () => null,
  // Never resolves, which is what "the root is never cancelled" means. Awaiting
  // it alone hangs; it is here to be raced against.
  done: () => NEVER,
  value: <T>(_key: Key<T>): T | void => undefined,
};

/** The root's controller, never aborted, so `ROOT.signal()` is a real signal. */
const ROOT_CONTROLLER: AbortController = new AbortController();

/** The promise `ROOT.done()` hands back. One, because it never settles. */
const NEVER: Promise<void> = new Promise(() => {});
