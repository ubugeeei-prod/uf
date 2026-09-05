// @flow
//
// `@uniflowed/effect`.
//
// A typed effect system written entirely in `.js` with Flow. The Rust
// toolchain may lint, format, test, and bundle it, but user code does not need
// a native binding to construct or run these effects on Node.js, Deno, or Bun.
//
// # What Flow cannot express, and what is here instead
//
// Effect-TS is built on a higher-kinded encoding: `Effect` is a type
// constructor a typeclass can be parameterised over, which is how one `pipe`,
// one `Traversable` and one `Do` notation serve every effect-like type. Flow
// has no higher-kinded types — a type parameter cannot itself take type
// arguments — so none of that is expressible here.
//
// The three things that encoding buys are bought differently:
//
//   - *Composition* is monomorphic functions over `Effect` rather than
//     typeclass instances. `map`, `flatMap`, `all` and the rest are ordinary
//     exports, so there is no `pipe`-with-inference to lose and nothing to
//     dispatch at run time.
//   - *Error accumulation* is a union. `flatMap(a, b)` is
//     `Effect<B, E1 | E2, R1 | R2>`, written out in every signature, and a
//     generator pipeline accumulates the same way because Flow infers a
//     generator's `Yield` type as the union of everything yielded.
//   - *Requirement subtraction* has no equivalent at all. Flow has no type
//     level set difference, so `provide` is typed as
//     `(Effect<A, E, R | Out>, Layer<Out, …>) => Effect<A, E, R>` and the
//     checker solves for `R` from the union. That works when the provided
//     service is a distinct member of the union and silently leaves it in `R`
//     when it is not. See Readiness.
//
// There is no `any` in this package. There are exactly four places where the
// checker cannot follow the runtime, each a one-line coded suppression with
// the argument for it written beside it, and each provable from something a
// few lines away rather than from a claim about the world:
//
//   1. the value the generator driver sends back into `yield*` — the driver
//      sends the effect's own success value, and `Next` has to be `mixed` for
//      a generator whose steps have different types to typecheck at all;
//   2. the `value` of a finished `IteratorResult`, which Flow types as
//      optional because `return;` with no argument is legal;
//   3. reading a service back out of the `Map<string, mixed>` a context is,
//      under the tag that put it there;
//   4. the non-promise arm of `call`'s `A | Promise<A>`, after the check that
//      excluded the other one.
//
// A suppression is a comment above one error, so none of these leaks: unlike
// an `any` value, nothing downstream of them stops being checked. The file
// carried forty-three `as any` casts before, and the reason it no longer needs
// them is `EffectKernel` — see below.
//
// # Readiness
//
// **Implemented.** The `Effect<A, E, R>` value and its three channels;
// generator syntax with both `yield` and `yield*`, over effects and over other
// generators; `runSync`, `runPromise` and the `Exit`-returning `runSyncExit`,
// `runPromiseExit`, `exit`; typed failures kept distinct from defects and from
// interruption, with `catchAll`, `catchTag`, `orElse`, `either` and `orDie`;
// `retry` over a `Schedule`; `timeout`; `acquireRelease` with `scoped`, and
// `ensuring`, both of which release on success, failure, defect and
// interruption; `all` and `forEach` with a concurrency limit and a synchronous
// form when every element has one, `race`, and `fork`/`join`/`interrupt`;
// `Tag` and `Layer` for services.
//
// **Experimental.** Requirement subtraction, for the reason above: `provide`,
// `provideService` and `scoped` state the service they remove and let Flow
// solve for the rest, which is weaker than Effect-TS's `Exclude`. `catchTag`
// reads a `kind` (or `tag`) string off the error at run time and does not
// narrow `E` for the recovery function, because Flow cannot narrow a type
// variable by a string compared at run time. `fork` is detached rather than
// scoped to its parent: cancelling the parent does not cancel the child, which
// is `forkDaemon`'s semantics rather than `fork`'s.
//
// **Not implemented.** `Ref`, `Deferred`, `Queue`, `Hub`, `Semaphore` and STM;
// streams; a fiber scheduler of its own (this runs on the host's microtask
// queue and its `sleep` is `setTimeout`); tracing, spans, metrics and the
// logging layer; `Layer` memoisation and dependency resolution, so a layer
// passed to two `provide`s is built twice; a typed defect channel; a
// heterogeneous `all`, which in Effect-TS keeps a tuple's element types and
// here takes and returns one array type; and Effect's `"inherit"` concurrency,
// because no enclosing limit is tracked to inherit.
//
// # Why the runtime is one file
//
// Everything below is one runtime. `Effect`, `Fiber`, `Tag`, `Layer` and
// `Scope` are opaque types over carriers this module defines, and every
// combinator reaches through `readKernel` into the same `Context` — the
// interruption flag a `sleep` registers a waker on, the service map a `Tag`
// reads, the finalizer list a `Scope` closes. A constructor, a combinator and
// a runner are three moments in the life of one value, not three subjects.
//
// Splitting them would mean handing `makeEffect` and `readKernel` to siblings,
// which is the opaque type's whole guarantee given away inside the package to
// buy a directory listing. And Flow's opacity is per module: the moment
// `Effect` is declared in one file and constructed in another, the second file
// needs the carrier's shape, so the split does not merely cost indirection —
// it costs the invariant.
//
// What was here before was the other failure: an `index.js` that re-exported
// every name from an `internal/runtime.js` holding all of it. A reader opening
// the package found a list of sixty names and had to open a second file, named
// after nothing narrower than the package itself, to see any code.
//
// `./schedule.js` is the one thing that is genuinely separable, and it is
// separate: a retry policy is arithmetic over an attempt count that never sees
// a `Context`, an `Exit` or a fiber, and it explains itself there.
//
// That split costs one thing today, and it is uf's rather than Flow's:
// `uf check` does not yet resolve types across modules, so it reports the
// imported `Schedule` as an any-typed value and `retry`'s parameter is
// unchecked by it. `flow` itself checks it, and `@uniflowed/form`,
// `@uniflowed/hooks` and `@uniflowed/immer` are in the same position for the
// same reason. It is a checker gap to close, not a reason to put a retry
// policy back inside a runtime.

import { scheduleDelay } from "./schedule.js";
import type { Schedule } from "./schedule.js";

export type { Schedule };

/**
 * How one effect takes its step.
 *
 * `run` is the general form and every effect has one. `runSync` is present
 * only on effects that can produce an outcome without yielding to the event
 * loop; `runSync` and `runSyncExit` need it, and an effect without one has no
 * synchronous answer to give.
 *
 * Both are typed in `A` and `E`, which is what removes the casts this file
 * used to be full of: because the kernel mentions them in return position
 * only, `EffectCarrier` is covariant in them without a phantom field, and a
 * runner gets an `Exit<A, E>` back rather than an `Exit<mixed, mixed>` it has
 * to swear about.
 */
type EffectKernel<out A, out E> = {
  readonly run: (Context) => Promise<Exit<A, E>>,
  readonly runSync?: (Context) => Exit<A, E>,
};

/**
 * The object an `Effect` is.
 *
 * Inexact on purpose: a `Tag` is an effect with a name attached, and exact
 * object types would make the tag's extra property a different type rather
 * than a subtype.
 *
 * `__requires` is the one phantom left. `A` and `E` are carried for real by
 * `__kernel`, but nothing at run time depends on `R`, so it needs a place in
 * the type to be a parameter at all — and putting it in a shallow field rather
 * than burying it inside the iterator's type argument keeps the checker's job
 * easy when it has to solve `R | Service` for `R`.
 */
type EffectCarrier<out A, out E, out R> = {
  readonly __kind: "Effect",
  readonly __requires: () => R,
  readonly __kernel: EffectKernel<A, E>,
  readonly @@iterator: () => $IteratorProtocol<Effect<mixed, E, R>, A, mixed>,
  ...
};

/** A `Tag` is an `Effect` that reads its own service, plus the name to read. */
type TagCarrier<out Service> = {
  readonly __kind: "Effect",
  readonly __requires: () => Service,
  readonly __kernel: EffectKernel<Service, empty>,
  readonly @@iterator: () => $IteratorProtocol<Effect<mixed, empty, Service>, Service, mixed>,
  readonly identifier: string,
  ...
};

type FiberCarrier<out A, out E> = {
  readonly __kind: "Fiber",
  readonly __promise: Promise<Exit<A, E>>,
  readonly __fiber: FiberState,
};

type LayerKernel<out E> = (Context) => Promise<Exit<$ReadOnlyMap<string, mixed>, E>>;

type LayerCarrier<out Out, out E, out In> = {
  readonly __kind: "Layer",
  readonly __out: () => Out,
  readonly __in: () => In,
  readonly __layer: LayerKernel<E>,
};

type ScopeState = {
  readonly finalizers: Array<() => Effect<void, mixed, empty>>,
};

/**
 * The interruption state one fiber shares with everything running inside it.
 *
 * `interrupted` is the flag combinators check between steps. `wakers` is how a
 * pending `sleep` finds out, because a timer that has already been scheduled
 * cannot be talked out of firing and a fiber that waited for it would stay
 * alive for the full delay after being cancelled. `children` is how a group —
 * the fibers `all`, `race` and `timeout` open — hears about an interruption
 * aimed at the fiber above it.
 */
type FiberState = {
  interrupted: boolean,
  readonly wakers: Set<() => void>,
  readonly children: Set<FiberState>,
};

type Context = {
  readonly services: $ReadOnlyMap<string, mixed>,
  readonly scope: ?ScopeState,
  readonly fiber: FiberState,
};

/**
 * Work that produces an `A`, may fail with an `E`, and needs an `R`.
 *
 * `E` defaults to `empty`, Flow's bottom type, so `Effect<number>` reads as
 * cannot fail. `R` defaults to `empty` for an effect that needs no services.
 *
 * The `$Iterable` bound is what makes `yield* someEffect` legal inside
 * `effect(function* () { … })` *and* typed: the delegate's `Return` type is
 * `A`, so the checker gives the yielded expression the effect's success type
 * rather than the `mixed` a bare `yield` produces.
 */
export opaque type Effect<out A, out E = empty, out R = empty>: $Iterable<
  Effect<mixed, E, R>,
  A,
  mixed,
> = EffectCarrier<A, E, R>;

/** A running effect, addressable so it can be awaited or cancelled. */
export opaque type Fiber<out A, out E = empty> = FiberCarrier<A, E>;

/**
 * Identifies one service inside a context, and is the effect that reads it.
 *
 * The supertype bound is the whole point: a tag can be handed to any
 * combinator that takes an effect, and `yield* Clock` inside a generator
 * produces the service, so there is no second accessor function to learn.
 */
export opaque type Tag<out Service>: Effect<Service, empty, Service> = TagCarrier<Service>;

/** A recipe for building services, itself possibly failing. */
export opaque type Layer<out Out, out E = empty, out In = empty> = LayerCarrier<Out, E, In>;

/**
 * The lifetime a resource is released at.
 *
 * Never constructed: it exists to appear in an effect's `R` so that a value
 * built with `acquireRelease` cannot be run until something has said where its
 * release belongs.
 */
export opaque type Scope = { readonly __kind: "Scope" };

/**
 * Why an effect did not produce a value.
 *
 * Three leaves, and the distinction between them is the point of the package:
 * `fail` is the typed error the signature promised and the only one recovery
 * and retry act on, `die` is a bug in the program, and `interrupt` is a
 * decision somebody already took. `sequential` and `parallel` hold the causes
 * of several effects that failed together — `race` produces a `parallel` when
 * every entrant failed.
 */
export type Cause<out E> =
  | { readonly kind: "empty" }
  | { readonly kind: "fail", readonly error: E }
  | { readonly kind: "die", readonly defect: string }
  | { readonly kind: "interrupt" }
  | { readonly kind: "sequential", readonly causes: $ReadOnlyArray<Cause<E>> }
  | { readonly kind: "parallel", readonly causes: $ReadOnlyArray<Cause<E>> };

/** How an effect ended: a value, or the reason there is none. */
export type Exit<out A, out E> =
  | { readonly kind: "success", readonly value: A }
  | { readonly kind: "failure", readonly cause: Cause<E> };

/** What `timeout` adds to an effect's error channel. */
export type TimeoutError = { readonly kind: "timeout", readonly millis: number };

/**
 * How many effects `all` and `forEach` may have in flight.
 *
 * Effect's `"inherit"` is deliberately absent: there is no enclosing limit
 * recorded to inherit, and an option that silently means something else is
 * worse than one that is not offered.
 */
export type Concurrency = number | "unbounded";

/**
 * The generator an `effect` body is.
 *
 * `Yield` is `Effect<mixed, E, R>` and not `Effect<A, E, R>` because the steps
 * of a pipeline produce different types; `E` and `R` accumulate as the union
 * of everything yielded, which is where a pipeline's failure type comes from.
 * `Next` is `mixed` for the same reason, so a bare `yield` produces `mixed` and
 * `yield*` produces the effect's own `A`.
 */
export type EffectGenerator<A, E, R> = Generator<Effect<mixed, E, R>, A, mixed>;

/**
 * Stands in for a value the type system requires and the run time never has.
 *
 * Throws rather than returning, because every call site is one that has
 * already been shown unreachable, and a phantom that quietly returned
 * `undefined` would let a mistake about that pass unnoticed.
 */
function absurd(): empty {
  throw Error("@uniflowed/effect phantom value");
}

function newFiber(): FiberState {
  return { interrupted: false, wakers: new Set(), children: new Set() };
}

function context(): Context {
  return { services: new Map(), scope: null, fiber: newFiber() };
}

function withScope(parent: Context): Context {
  return { services: parent.services, scope: { finalizers: [] }, fiber: parent.fiber };
}

function withService<Service>(
  parent: Context,
  serviceTag: Tag<Service>,
  service: Service,
): Context {
  const services = new Map(parent.services);
  services.set(readTag(serviceTag), service);
  return { services, scope: parent.scope, fiber: parent.fiber };
}

/**
 * A context whose interruption is nobody else's.
 *
 * Two callers, for opposite reasons. `fork` starts work the caller intends to
 * be able to stop on its own, so a child sharing its parent's flag could not be
 * cancelled separately. A finalizer runs detached because a cleanup that is
 * itself cancelled is not a cleanup — that is what makes a release run when
 * the fiber holding the resource was interrupted.
 */
function detachedContext(parent: Context): Context {
  return { services: parent.services, scope: parent.scope, fiber: newFiber() };
}

/**
 * A context that is cancelled when its parent is.
 *
 * This is what `all`, `race` and `timeout` open, and it is what lets them stop
 * their own siblings without stopping the fiber that called them. A child born
 * to an already-interrupted parent starts interrupted, so a group opened after
 * the decision does not get a free pass.
 */
function childContext(parent: Context): Context {
  const fiber = newFiber();
  fiber.interrupted = parent.fiber.interrupted;
  parent.fiber.children.add(fiber);
  return { services: parent.services, scope: parent.scope, fiber };
}

/**
 * Forget a finished child.
 *
 * Without this a long-lived fiber calling `all` in a loop holds every group it
 * ever opened, and the leak is invisible because nothing reads the set except
 * an interruption that never comes.
 */
function releaseChild(parent: Context, child: Context): void {
  parent.fiber.children.delete(child.fiber);
}

/**
 * Cancel a fiber and everything under it.
 *
 * Waking is what makes cancellation take effect now rather than whenever a
 * pending timer happens to fire. Deleting a waker from inside the loop is safe:
 * a `Set` iteration tolerates removal of entries it has reached, and a waker
 * only ever resolves a promise, which cannot add another before this returns.
 */
function interruptFiber(state: FiberState): void {
  if (state.interrupted) {
    return;
  }
  state.interrupted = true;
  for (const wake of state.wakers) {
    wake();
  }
  for (const child of state.children) {
    interruptFiber(child);
  }
}

/** Whether the fiber this context belongs to has been interrupted. */
function isInterrupted(runContext: Context): boolean {
  return runContext.fiber.interrupted;
}

function interruptedExit<A, E>(): Exit<A, E> {
  return failure({ kind: "interrupt" });
}

/**
 * Build an effect over `kernel`.
 *
 * Every effect is one object literal with the same four keys, so every effect
 * has the same hidden class and a combinator reading `__kernel` off one is
 * reading it off a shape the engine has already seen.
 */
function makeEffect<A, E, R>(kernel: EffectKernel<A, E>): Effect<A, E, R> {
  return {
    __kind: "Effect",
    __requires: absurd,
    __kernel: kernel,
    [Symbol.iterator]: iterateEffect,
  };
}

/**
 * What `yield* someEffect` reaches, shared by every effect ever built.
 *
 * One shared function with an annotated `this`, and not the closure this
 * started as, because the closure cost eight times the throughput of the whole
 * package. `[Symbol.iterator]: () => yieldOnce(self)` makes every effect escape
 * into a closure the moment it is built, which stops the engine from proving
 * that the intermediate effects in a `map`/`flatMap` chain are dead as soon as
 * they are run.
 *
 * Measured on Node 25.8: building and running a `map`-then-`flatMap` chain
 * under `runSync`, two hundred thousand times, best of five. With the closure,
 * 1.07M runs a second; with this, 8.4M. Effect-TS 3.22.1 does the same work at
 * 1.3M, so the closure was the whole difference between losing to it by a
 * sixth and beating it by six and a half times.
 *
 * Flow refuses `this` inside a computed *method* of an object literal, since
 * such a method may be unbound; a standalone function with a declared `this`
 * parameter is how the same thing is said in a form the checker accepts.
 */
function iterateEffect<A, E, R>(
  this: Effect<A, E, R>,
): $IteratorProtocol<Effect<mixed, E, R>, A, mixed> {
  return yieldOnce(this);
}

/**
 * The iterator `yield* someEffect` drives: hand the effect out once, then
 * finish with whatever the driver sent back.
 *
 * This is the shape Effect-TS gives its own effects, and it is what makes the
 * generator form read the same in both libraries.
 */
function yieldOnce<A, E, R>(
  self: Effect<A, E, R>,
): $IteratorProtocol<Effect<mixed, E, R>, A, mixed> {
  let delivered = false;
  return {
    next(sent?: mixed): IteratorResult<Effect<mixed, E, R>, A> {
      if (delivered) {
        // The driver in `effect` sends back the success value of the effect it
        // was handed, so `sent` is this effect's `A`. `Next` has to be `mixed`
        // for a generator whose steps have different types to typecheck at
        // all, so the checker cannot see that and this is where it is told.
        // $FlowFixMe[incompatible-type] the driver sends this effect's own value
        return { done: true, value: sent };
      }
      delivered = true;
      return { done: false, value: self };
    },
  };
}

/**
 * The value a finished generator returned.
 *
 * Flow types the done branch of `IteratorResult` as `{ done: true, +value?: R }`
 * — optional, because `return;` with no argument is legal and produces
 * `undefined`. A generator declared to return `A` and reached at its `return`
 * produced an `A`, and `EffectGenerator`'s declared `Return` type is what makes
 * that true. There is no narrowing that says so, so it is asserted once, here.
 */
function finishedValue<A>(step: { readonly value?: A, ... }): A {
  // $FlowFixMe[incompatible-type] a generator declared to return A returned one
  return step.value;
}

function readKernel<A, E, R>(self: Effect<A, E, R>): EffectKernel<A, E> {
  return self.__kernel;
}

function makeFiber<A, E>(promise: Promise<Exit<A, E>>, fiber: FiberState): Fiber<A, E> {
  return { __kind: "Fiber", __promise: promise, __fiber: fiber };
}

/**
 * A tag, which is also the effect that reads the service it names.
 *
 * Reading a service that was never provided is a defect rather than a typed
 * failure. Flow's `R` parameter already tracks which services an effect
 * requires, so reaching this at run time means the type was bypassed, and that
 * is a bug in the program rather than a condition it should recover from.
 */
function makeTag<Service>(identifier: string): Tag<Service> {
  const kernel: EffectKernel<Service, empty> = {
    run: (runContext) => Promise.resolve(readService<Service>(runContext, identifier)),
    runSync: (runContext) => readService<Service>(runContext, identifier),
  };
  return {
    __kind: "Effect",
    __requires: absurd,
    __kernel: kernel,
    identifier,
    [Symbol.iterator]: iterateEffect,
  };
}

function readService<Service>(runContext: Context, identifier: string): Exit<Service, empty> {
  const services = runContext.services;
  if (!services.has(identifier)) {
    return defect(`service ${identifier} was not provided`);
  }
  // A `Map<string, mixed>` cannot promise the value under this key is a
  // `Service`; `provideService` and `layerSucceed` are what make it one, and
  // both take the tag that names it.
  const service = services.get(identifier);
  return success(unsafeService<Service>(service));
}

/**
 * Read back a service under the tag that stored it.
 *
 * The only place in this module where a value's type is taken on trust, and it
 * is narrow by construction: the map is keyed by tag identifier, and the only
 * writers are `provideService` and the `layer*` functions, each of which takes
 * the `Tag<Service>` and the `Service` together.
 */
function unsafeService<Service>(value: mixed): Service {
  // $FlowFixMe[incompatible-type] the tag that keyed this entry typed it
  return value;
}

function readTag<Service>(serviceTag: Tag<Service>): string {
  return serviceTag.identifier;
}

function makeLayer<Out, E, In>(kernel: LayerKernel<E>): Layer<Out, E, In> {
  return { __kind: "Layer", __out: absurd, __in: absurd, __layer: kernel };
}

function readLayer<Out, E, In>(layer: Layer<Out, E, In>): LayerKernel<E> {
  return layer.__layer;
}

function success<A, E>(value: A): Exit<A, E> {
  return { kind: "success", value };
}

function failure<A, E>(cause: Cause<E>): Exit<A, E> {
  return { kind: "failure", cause };
}

function failCause<E>(error: E): Cause<E> {
  return { kind: "fail", error };
}

function dieCause(defectValue: mixed): Cause<empty> {
  return { kind: "die", defect: String(defectValue) };
}

function defect<A>(defectValue: mixed): Exit<A, empty> {
  return failure(dieCause(defectValue));
}

/**
 * Whether a failure is the kind another attempt could settle.
 *
 * A typed failure describes a condition — a request that timed out, a row that
 * was not there — and those come and go. A defect is a bug in the program and
 * an interruption is a decision already taken; repeating either just repeats
 * it. A composite cause is retriable when any leaf in it is.
 */
function isRetriable(cause: Cause<mixed>): boolean {
  switch (cause.kind) {
    case "fail":
      return true;
    case "sequential":
    case "parallel":
      return cause.causes.some(isRetriable);
    default:
      return false;
  }
}

/**
 * The first typed failure in a cause, as its node rather than its error.
 *
 * The node, because `fail(null)` is a legitimate typed failure and a function
 * returning `?E` cannot tell it from "there was no typed failure at all" —
 * which is the difference between recovering and letting a defect through.
 */
function failureNode<E>(cause: Cause<E>): ?{ readonly kind: "fail", readonly error: E } {
  switch (cause.kind) {
    case "fail":
      return cause;
    case "sequential":
    case "parallel":
      for (const entry of cause.causes) {
        const found = failureNode(entry);
        if (found != null) {
          return found;
        }
      }
      return null;
    default:
      return null;
  }
}

/**
 * Read a cause's tag with `switch` rather than `match`.
 *
 * A `match` object pattern over a generic `Cause<E>` binds `error` as
 * `unknown` rather than as `E`, so `mapCause`'s `transform` cannot be called
 * with it and `firstFailure` cannot return it. `switch` on the tag refines
 * correctly, so that is what these use until the checker catches up.
 */
function mapCause<E, F>(cause: Cause<E>, transform: (error: E) => F): Cause<F> {
  switch (cause.kind) {
    case "fail":
      return { kind: "fail", error: transform(cause.error) };
    case "sequential":
      return { kind: "sequential", causes: cause.causes.map((one) => mapCause(one, transform)) };
    case "parallel":
      return { kind: "parallel", causes: cause.causes.map((one) => mapCause(one, transform)) };
    default:
      return cause;
  }
}

/**
 * Re-type a cause that carries no typed failure.
 *
 * `catchAll` changes an effect's error channel from `E` to `F`, and a cause it
 * did not catch — a defect, an interruption — still has to come out the other
 * side. Such a cause holds no `E` anywhere, so nothing is converted; `absurd`
 * is the conversion precisely because reaching it would mean a `fail` node was
 * there after all, and a loud throw beats a quiet lie about the error type.
 * Only ever called where `failureNode` has already returned null.
 */
function untypedCause<E, F>(cause: Cause<E>): Cause<F> {
  return mapCause(cause, absurd);
}

function causeMessage<E>(cause: Cause<E>): string {
  switch (cause.kind) {
    case "fail":
      return String(cause.error);
    case "die":
      return cause.defect;
    case "interrupt":
      return "effect interrupted";
    case "sequential":
    case "parallel":
      return cause.causes.map(causeMessage).join("; ");
    default:
      return "empty effect failure";
  }
}

/** What `runSync` and `runPromise` raise: the typed error, or a described cause. */
function throwable<E>(cause: Cause<E>): mixed {
  const found = failureNode(cause);
  return found == null ? Error(causeMessage(cause)) : found.error;
}

/**
 * A finalizer's failure, as a defect.
 *
 * A scope's error channel is the body's `E`, and a release that fails is not
 * one of the conditions that type promised — it is a bug in the release. Given
 * back as a typed failure it would be retried and caught as if the body had
 * produced it.
 */
function releaseDefect<A, E>(cause: Cause<mixed>): Exit<A, E> {
  return failure(dieCause(`effect finalizer failed: ${causeMessage(cause)}`));
}

/**
 * Take one step, converting a kernel that raises into a defect.
 *
 * Not `async`: an extra async frame costs a microtask on every step of every
 * pipeline, and the kernels below already return promises. The `try` is for a
 * kernel that throws before returning one — which is what happens when a
 * generator body yields something that is not an effect.
 */
function runKernel<A, E, R>(self: Effect<A, E, R>, runContext: Context): Promise<Exit<A, E>> {
  try {
    return readKernel(self).run(runContext);
  } catch (error) {
    return Promise.resolve(defect<A>(error));
  }
}

function runSyncKernel<A, E, R>(self: Effect<A, E, R>, runContext: Context): Exit<A, E> {
  let step;
  try {
    step = readKernel(self).runSync;
  } catch (error) {
    return defect(error);
  }
  if (step == null) {
    return failure(dieCause("effect is asynchronous"));
  }
  try {
    return step(runContext);
  } catch (error) {
    return defect(error);
  }
}

function isThenable(value: mixed): boolean {
  if (value instanceof Promise) {
    return true;
  }
  return typeof value === "object" && value !== null && "then" in value;
}

/**
 * Wait, and stop waiting early if the fiber is interrupted.
 *
 * A bare `setTimeout` cannot be talked out of firing, so a cancelled fiber
 * sleeping for a minute would keep the process alive for a minute after nobody
 * wanted its answer. Registering a waker is what makes cancellation take
 * effect now rather than at the end of the delay.
 */
function pause(millis: number, runContext: Context): Promise<void> {
  return new Promise((resolve) => {
    const fiber = runContext.fiber;
    if (fiber.interrupted) {
      resolve();
      return;
    }
    let timer = null;
    const finish = () => {
      if (timer != null) {
        clearTimeout(timer);
        timer = null;
      }
      fiber.wakers.delete(finish);
      resolve();
    };
    fiber.wakers.add(finish);
    timer = setTimeout(finish, Math.max(0, millis));
  });
}

function concurrencyLimit(
  length: number,
  options?: { readonly concurrency?: Concurrency },
): number {
  const requested = options == null ? null : options.concurrency;
  if (requested == null || requested === "unbounded") {
    return Math.max(1, length);
  }
  return Math.max(1, Math.min(length, Math.floor(requested)));
}

/** An effect that has already produced `value`. */
export function succeed<A>(value: A): Effect<A> {
  const settled: Exit<A, empty> = success(value);
  return makeEffect({
    run: () => Promise.resolve(settled),
    runSync: () => settled,
  });
}

/** An effect that has already failed with the typed error `error`. */
export function fail<E>(error: E): Effect<empty, E> {
  const settled: Exit<empty, E> = failure(failCause(error));
  return makeEffect({
    run: () => Promise.resolve(settled),
    runSync: () => settled,
  });
}

/**
 * An effect that has already failed with a defect.
 *
 * Not the same thing as `fail`: a defect is outside the error channel, so
 * `catchAll` will not see it, `retry` will not repeat it, and `either` will not
 * reify it. That is the whole distinction the package exists to keep.
 */
export function die(defectValue: mixed): Effect<empty> {
  const settled: Exit<empty, empty> = defect(defectValue);
  return makeEffect({
    run: () => Promise.resolve(settled),
    runSync: () => settled,
  });
}

/**
 * An effect that never produces anything — until it is interrupted.
 *
 * Interruptible on purpose. A `never` that ignored the flag would make
 * `interrupt(runFork(never()))` hang for the life of the process, which is the
 * one thing a caller reaching for `never` is most likely to do next.
 */
export function never(): Effect<empty> {
  return makeEffect({
    run: (runContext) =>
      new Promise((resolve) => {
        if (isInterrupted(runContext)) {
          resolve(interruptedExit());
          return;
        }
        const wake = () => {
          runContext.fiber.wakers.delete(wake);
          resolve(interruptedExit());
        };
        runContext.fiber.wakers.add(wake);
      }),
  });
}

/** Run `body` when the effect runs. A throw becomes a defect, not a failure. */
export function sync<A>(body: () => A): Effect<A> {
  const step = (): Exit<A, empty> => {
    try {
      return success(body());
    } catch (error) {
      return defect(error);
    }
  };
  return makeEffect({
    run: () => Promise.resolve(step()),
    runSync: step,
  });
}

/** Build the effect when it runs, not when it is described. */
export function suspend<A, E, R>(body: () => Effect<A, E, R>): Effect<A, E, R> {
  return makeEffect({
    run: (runContext) => {
      try {
        return runKernel(body(), runContext);
      } catch (error) {
        return Promise.resolve(defect<A>(error));
      }
    },
    runSync: (runContext) => {
      try {
        return runSyncKernel(body(), runContext);
      } catch (error) {
        return defect(error);
      }
    },
  });
}

/** Adopt a promise. A rejection is a defect: nothing said what it could be. */
export function promise<A>(body: () => Promise<A>): Effect<A> {
  return makeEffect({
    run: async () => {
      try {
        return success(await body());
      } catch (error) {
        return defect(error);
      }
    },
  });
}

/** Adopt a promise, naming what a rejection means as a typed failure. */
export function tryPromise<A, E>(options: {
  readonly try: () => Promise<A>,
  readonly catch: (error: mixed) => E,
}): Effect<A, E> {
  return makeEffect({
    run: async () => {
      try {
        return success(await options.try());
      } catch (error) {
        return failure(failCause(options.catch(error)));
      }
    },
  });
}

/** Run `body`, which may or may not be asynchronous. */
export function call<A>(body: () => A | Promise<A>): Effect<A> {
  return makeEffect({
    run: async () => {
      try {
        return success(await body());
      } catch (error) {
        return defect(error);
      }
    },
    runSync: () => {
      try {
        const value = body();
        if (isThenable(value)) {
          return defect("call returned a promise in a synchronous run");
        }
        // `isThenable` has ruled out the promise arm; nothing narrows a union
        // by a helper's return value, so the cast says what the check proved.
        return success(unsafeSettled<A>(value));
      } catch (error) {
        return defect(error);
      }
    },
  });
}

/** The non-promise half of `A | Promise<A>`, once `isThenable` has said so. */
function unsafeSettled<A>(value: mixed): A {
  // $FlowFixMe[incompatible-type] isThenable has excluded the promise arm
  return value;
}

/**
 * Write a pipeline as a generator.
 *
 * `yield* effect` is the typed form: the yielded expression has the effect's
 * success type, and the body's failure type is the union of every step's. A
 * bare `yield effect` works too and produces `mixed`, which is occasionally
 * what a caller wants and never what it should reach for first.
 *
 * Both a synchronous and an asynchronous driver, because a pipeline of
 * synchronous steps has a synchronous answer and `runSync` should be able to
 * ask for it. The synchronous driver refuses at the first step that has no
 * synchronous kernel, with the same defect any other such effect gives.
 */
export function effect<A, E, R>(body: () => EffectGenerator<A, E, R>): Effect<A, E, R> {
  return makeEffect({
    run: async (runContext) => {
      let iterator;
      try {
        iterator = body();
      } catch (error) {
        return defect(error);
      }
      let sent: mixed = undefined;
      let settled: ?Exit<A, E> = null;
      while (settled == null) {
        // The checkpoint is between steps, never inside one. An effect that has
        // started runs to its own end; interruption decides whether the *next*
        // one starts, which is the only point where stopping is safe without
        // knowing what the body was in the middle of.
        if (isInterrupted(runContext)) {
          return interruptedExit();
        }
        let step;
        try {
          step = iterator.next(sent);
        } catch (error) {
          return defect(error);
        }
        if (step.done === true) {
          settled = success(finishedValue(step));
        } else {
          const stepped = await runKernel(step.value, runContext);
          if (stepped.kind === "failure") {
            return failure(stepped.cause);
          }
          sent = stepped.value;
        }
      }
      return settled;
    },
    runSync: (runContext) => {
      let iterator;
      try {
        iterator = body();
      } catch (error) {
        return defect(error);
      }
      let sent: mixed = undefined;
      let settled: ?Exit<A, E> = null;
      while (settled == null) {
        if (isInterrupted(runContext)) {
          return interruptedExit();
        }
        let step;
        try {
          step = iterator.next(sent);
        } catch (error) {
          return defect(error);
        }
        if (step.done === true) {
          settled = success(finishedValue(step));
        } else {
          const stepped = runSyncKernel(step.value, runContext);
          if (stepped.kind === "failure") {
            return failure(stepped.cause);
          }
          sent = stepped.value;
        }
      }
      return settled;
    },
  });
}

/** Change a success, leaving the failure channel alone. */
export function map<A, B, E, R>(
  self: Effect<A, E, R>,
  transform: (value: A) => B,
): Effect<B, E, R> {
  const apply = (settled: Exit<A, E>): Exit<B, E> => {
    if (settled.kind === "failure") {
      return failure(settled.cause);
    }
    try {
      return success(transform(settled.value));
    } catch (error) {
      return defect(error);
    }
  };
  return makeEffect({
    run: async (runContext) => apply(await runKernel(self, runContext)),
    runSync: (runContext) => apply(runSyncKernel(self, runContext)),
  });
}

/** Change a typed failure, leaving a defect and an interruption alone. */
export function mapError<A, E, F, R>(
  self: Effect<A, E, R>,
  transform: (error: E) => F,
): Effect<A, F, R> {
  const apply = (settled: Exit<A, E>): Exit<A, F> =>
    settled.kind === "failure"
      ? failure(mapCause(settled.cause, transform))
      : success(settled.value);
  return makeEffect({
    run: async (runContext) => apply(await runKernel(self, runContext)),
    runSync: (runContext) => apply(runSyncKernel(self, runContext)),
  });
}

/**
 * Run `next` on the success of `self`.
 *
 * The failure channel is the union of both, which is what makes a pipeline's
 * error type the sum of what its steps can fail with rather than whatever the
 * last step happened to declare.
 */
export function flatMap<A, B, E1, E2, R1, R2>(
  self: Effect<A, E1, R1>,
  next: (value: A) => Effect<B, E2, R2>,
): Effect<B, E1 | E2, R1 | R2> {
  return makeEffect({
    run: async (runContext) => {
      const settled = await runKernel(self, runContext);
      if (settled.kind === "failure") {
        return failure(settled.cause);
      }
      if (isInterrupted(runContext)) {
        return interruptedExit();
      }
      try {
        return await runKernel(next(settled.value), runContext);
      } catch (error) {
        return defect(error);
      }
    },
    runSync: (runContext) => {
      const settled = runSyncKernel(self, runContext);
      if (settled.kind === "failure") {
        return failure(settled.cause);
      }
      if (isInterrupted(runContext)) {
        return interruptedExit();
      }
      try {
        return runSyncKernel(next(settled.value), runContext);
      } catch (error) {
        return defect(error);
      }
    },
  });
}

/** Run `next` after `self`, discarding `self`'s value. */
export function andThen<A, B, E1, E2, R1, R2>(
  self: Effect<A, E1, R1>,
  next: Effect<B, E2, R2>,
): Effect<B, E1 | E2, R1 | R2> {
  return flatMap(self, () => next);
}

/** Both values as a pair, `self` first and sequentially. */
export function zip<A, B, E1, E2, R1, R2>(
  self: Effect<A, E1, R1>,
  other: Effect<B, E2, R2>,
): Effect<[A, B], E1 | E2, R1 | R2> {
  return flatMap(self, (left) => map(other, (right) => [left, right]));
}

/**
 * Every effect, with at most `concurrency` of them in flight.
 *
 * Fails fast, and states exactly what that costs: the first failure — first in
 * *completion* order, not in index order — is the result. Its siblings are
 * interrupted and then awaited, so `all` does not return until nothing it
 * started is still running. That is the difference between a combinator and a
 * leak: a sibling left in flight would go on writing to the world after the
 * caller had already handled the failure.
 *
 * Values come back in the order of the input, whatever order they finished in.
 */
export function all<A, E, R>(
  effects: $ReadOnlyArray<Effect<A, E, R>>,
  options?: { readonly concurrency?: Concurrency },
): Effect<$ReadOnlyArray<A>, E, R> {
  return makeEffect({
    run: async (runContext) => {
      const group = childContext(runContext);
      const results: Array<A> = new Array(effects.length);
      let nextIndex = 0;
      let failed: ?Exit<empty, E> = null;

      const worker = async (): Promise<void> => {
        while (failed == null && nextIndex < effects.length) {
          // The checkpoint a group needs of its own. An element that never
          // yields — a `succeed`, a `sync` — has no checkpoint inside it, so
          // without this an interrupted `all` would run the rest of the array
          // to the end before noticing.
          if (isInterrupted(group)) {
            if (failed == null) {
              failed = interruptedExit();
            }
            return;
          }
          const index = nextIndex;
          nextIndex += 1;
          const settled = await runKernel(effects[index], group);
          if (settled.kind === "failure") {
            if (failed == null) {
              failed = failure(settled.cause);
              interruptFiber(group.fiber);
            }
            return;
          }
          results[index] = settled.value;
        }
      };

      const workers: Array<Promise<void>> = [];
      const limit = concurrencyLimit(effects.length, options);
      for (let started = 0; started < limit; started += 1) {
        workers.push(worker());
      }
      await Promise.all(workers);
      releaseChild(runContext, group);

      const outcome = failed;
      if (outcome != null) {
        return outcome;
      }
      return isInterrupted(runContext) ? interruptedExit() : success(results);
    },
    // A wholly synchronous `all` has a synchronous answer, and `runSync` should
    // be able to ask for it. Concurrency has no meaning here — synchronous work
    // cannot overlap — so the elements run in the order they were given, and
    // the first one with no synchronous kernel ends the run with the same
    // defect it would give anywhere else.
    runSync: (runContext) => {
      const results: Array<A> = new Array(effects.length);
      for (let index = 0; index < effects.length; index += 1) {
        if (isInterrupted(runContext)) {
          return interruptedExit();
        }
        const settled = runSyncKernel(effects[index], runContext);
        if (settled.kind === "failure") {
          return failure(settled.cause);
        }
        results[index] = settled.value;
      }
      return success(results);
    },
  });
}

/** `all` over the effects `body` builds from `items`. */
export function forEach<A, B, E, R>(
  items: $ReadOnlyArray<A>,
  body: (item: A, index: number) => Effect<B, E, R>,
  options?: { readonly concurrency?: Concurrency },
): Effect<$ReadOnlyArray<B>, E, R> {
  return all(
    items.map((item, index) => body(item, index)),
    options,
  );
}

/**
 * The first entrant to *succeed*.
 *
 * A failure removes that entrant from the race rather than ending it, because
 * "whichever settles first" is not what a caller wants when the fastest answer
 * is an error. If every entrant fails, the result is a `parallel` cause holding
 * all of them, so nothing is hidden and `catchAll` still finds the first typed
 * error among them. An empty list behaves as `never`.
 *
 * The losers are interrupted and *not* awaited. Waiting for the slower entrant
 * to reach its next checkpoint would hand back the latency the race exists to
 * avoid; the cost is that a loser may run a little past the point `race`
 * returned, so a loser with side effects has to be written to survive that.
 */
export function race<A, E, R>(effects: $ReadOnlyArray<Effect<A, E, R>>): Effect<A, E, R> {
  return makeEffect({
    run: (runContext) => {
      if (effects.length === 0) {
        return runKernel(never(), runContext);
      }
      const group = childContext(runContext);
      return new Promise((resolve) => {
        const causes: Array<Cause<E>> = [];
        let pending = effects.length;
        let decided = false;
        const settle = (settled: Exit<A, E>) => {
          decided = true;
          releaseChild(runContext, group);
          resolve(settled);
        };
        for (const entry of effects) {
          runKernel(entry, group).then((settled) => {
            pending -= 1;
            if (decided) {
              return;
            }
            if (settled.kind === "success") {
              interruptFiber(group.fiber);
              settle(settled);
              return;
            }
            causes.push(settled.cause);
            if (pending === 0) {
              settle(
                isInterrupted(runContext)
                  ? interruptedExit()
                  : failure({ kind: "parallel", causes }),
              );
            }
          });
        }
      });
    },
  });
}

/**
 * Recover from a typed failure.
 *
 * A defect and an interruption pass straight through: neither is in the error
 * channel `recover` was written against, and catching them here is how a bug
 * ends up reported as a handled condition.
 */
export function catchAll<A, B, E, F, R1, R2>(
  self: Effect<A, E, R1>,
  recover: (error: E) => Effect<B, F, R2>,
): Effect<A | B, F, R1 | R2> {
  return makeEffect({
    run: async (runContext) => {
      const settled = await runKernel(self, runContext);
      if (settled.kind === "success") {
        return success(settled.value);
      }
      const found = failureNode(settled.cause);
      if (found == null) {
        return failure(untypedCause(settled.cause));
      }
      return await runKernel(recover(found.error), runContext);
    },
    runSync: (runContext) => {
      const settled = runSyncKernel(self, runContext);
      if (settled.kind === "success") {
        return success(settled.value);
      }
      const found = failureNode(settled.cause);
      if (found == null) {
        return failure(untypedCause(settled.cause));
      }
      return runSyncKernel(recover(found.error), runContext);
    },
  });
}

/**
 * Recover from one kind of tagged failure, re-failing the rest.
 *
 * The bound is what makes this typed rather than a probe: an error has to say
 * which kind it is before it can be caught by kind. Both `kind` — the
 * discriminant Flow's `match` and the rest of uf use — and `tag`, which is what
 * an error ported from Effect-TS will be carrying, are read.
 *
 * `recover` still sees the whole `E`. Flow cannot narrow a type variable by a
 * string compared at run time, so the narrowing Effect-TS gets from a literal
 * `_tag` is not available; see Readiness.
 */
export function catchTag<
  A,
  B,
  E extends { readonly kind?: string, readonly tag?: string, ... },
  F,
  R1,
  R2,
>(
  self: Effect<A, E, R1>,
  tagName: string,
  recover: (error: E) => Effect<B, F, R2>,
): Effect<A | B, E | F, R1 | R2> {
  return catchAll(self, (error) =>
    (error.kind ?? error.tag) === tagName ? recover(error) : fail(error),
  );
}

/** Fall back to another effect on any typed failure. */
export function orElse<A, B, E, F, R1, R2>(
  self: Effect<A, E, R1>,
  fallback: () => Effect<B, F, R2>,
): Effect<A | B, F, R1 | R2> {
  return catchAll(self, () => fallback());
}

/**
 * Reify a typed failure as a value, so the effect itself cannot fail.
 *
 * Only the typed failure. A defect and an interruption stay failures, which is
 * why the error channel of the result is `empty` and not a lie: what comes back
 * as `{ ok: false }` is exactly what `E` said could happen.
 */
export function either<A, E, R>(
  self: Effect<A, E, R>,
): Effect<
  { readonly ok: true, readonly value: A } | { readonly ok: false, readonly error: E },
  empty,
  R,
> {
  const apply = (
    settled: Exit<A, E>,
  ): Exit<
    { readonly ok: true, readonly value: A } | { readonly ok: false, readonly error: E },
    empty,
  > => {
    if (settled.kind === "success") {
      return success({ ok: true, value: settled.value });
    }
    const found = failureNode(settled.cause);
    return found == null
      ? failure(untypedCause(settled.cause))
      : success({ ok: false, error: found.error });
  };
  return makeEffect({
    run: async (runContext) => apply(await runKernel(self, runContext)),
    runSync: (runContext) => apply(runSyncKernel(self, runContext)),
  });
}

/**
 * Try again on a typed failure, on the schedule's timetable.
 *
 * The first run is not a retry, so `{ kind: "recurs", times: 2 }` runs the
 * effect three times. Only a typed failure is worth another attempt: a defect
 * is a bug, so running it again runs the bug again, and an interruption is a
 * decision already taken.
 */
export function retry<A, E, R>(self: Effect<A, E, R>, schedule: Schedule): Effect<A, E, R> {
  return makeEffect({
    run: async (runContext) => {
      let attempt = 0;
      let settled = await runKernel(self, runContext);
      while (
        settled.kind === "failure" &&
        isRetriable(settled.cause) &&
        !isInterrupted(runContext)
      ) {
        const millis = scheduleDelay(schedule, attempt);
        if (millis == null) {
          return settled;
        }
        attempt += 1;
        await pause(millis, runContext);
        if (isInterrupted(runContext)) {
          return settled;
        }
        settled = await runKernel(self, runContext);
      }
      return settled;
    },
  });
}

/**
 * Give up on `self` after `millis`.
 *
 * The effect runs in a child fiber, so the timer expiring cancels it rather
 * than leaving it running with nobody waiting, and `timeout` does not return
 * until it has stopped. An interruption of the *calling* fiber is reported as
 * an interruption and not as a timeout: reporting it as a timeout would make it
 * a typed failure, and `retry` would then run the effect again after somebody
 * asked for it to stop.
 */
export function timeout<A, E, R>(
  self: Effect<A, E, R>,
  millis: number,
): Effect<A, E | TimeoutError, R> {
  return makeEffect({
    run: async (runContext) => {
      const child = childContext(runContext);
      const running = runKernel(self, child);
      let elapsed = false;
      const timer = pause(millis, runContext).then(() => {
        elapsed = true;
      });
      await Promise.race([running, timer]);
      if (!elapsed) {
        releaseChild(runContext, child);
        const settled = await running;
        return settled.kind === "success" ? success(settled.value) : failure(settled.cause);
      }
      interruptFiber(child.fiber);
      await running;
      releaseChild(runContext, child);
      return isInterrupted(runContext)
        ? interruptedExit()
        : failure(failCause({ kind: "timeout", millis }));
    },
  });
}

/**
 * Acquire something, and register how to give it back.
 *
 * The `Scope` in the requirement channel is what stops this being run without
 * somebody saying where the release belongs; `scoped` is what discharges it.
 * Reaching this without a scope is a defect rather than a silent skip, because
 * a release that never runs is the failure this combinator exists to prevent.
 *
 * There is no interruption window between acquiring and registering: the flag
 * is only read at the checkpoints this file writes, and there is none between
 * the `await` below and the `push` after it.
 */
export function acquireRelease<A, E, R>(
  acquire: Effect<A, E, R>,
  release: (resource: A) => Effect<void>,
): Effect<A, E, R | Scope> {
  return makeEffect({
    run: async (runContext) => {
      const scope = runContext.scope;
      if (scope == null) {
        return defect("acquireRelease needs a Scope; wrap the effect in scoped()");
      }
      const settled = await runKernel(acquire, runContext);
      if (settled.kind === "failure") {
        return failure(settled.cause);
      }
      const resource = settled.value;
      scope.finalizers.push(() => release(resource));
      return success(resource);
    },
  });
}

/**
 * Give the effect a scope, and close it however the effect ends.
 *
 * Finalizers run in reverse order of acquisition, and in a detached fiber: a
 * scope closing because its fiber was interrupted still has to release what it
 * took, and a finalizer running under the interrupted fiber would stop at its
 * own first checkpoint.
 *
 * A finalizer that fails turns into a defect and only replaces a success — a
 * body that already failed keeps its own failure, which is the more useful
 * half of the news.
 */
export function scoped<A, E, R>(self: Effect<A, E, R | Scope>): Effect<A, E, R> {
  return makeEffect({
    run: async (runContext) => {
      const scopedContext = withScope(runContext);
      const state = scopedContext.scope;
      const settled = await runKernel(self, scopedContext);
      const finalizers = state == null ? [] : state.finalizers;
      let broken: ?Cause<mixed> = null;
      for (let index = finalizers.length - 1; index >= 0; index -= 1) {
        const released = await runKernel(finalizers[index](), detachedContext(runContext));
        if (released.kind === "failure" && broken == null) {
          broken = released.cause;
        }
      }
      if (settled.kind === "success" && broken != null) {
        return releaseDefect(broken);
      }
      return settled.kind === "success" ? success(settled.value) : failure(settled.cause);
    },
  });
}

/**
 * Run `finalizer` however `self` ends: success, failure, defect, interruption.
 *
 * `acquireRelease` needs a `Scope`; this does not, which makes it the right
 * tool for the ordinary "close this when the block is done" case that would
 * otherwise force a scope onto the caller's type to run one cleanup.
 */
export function ensuring<A, E, R>(
  self: Effect<A, E, R>,
  finalizer: () => Effect<mixed, mixed, empty>,
): Effect<A, E, R> {
  const combine = (settled: Exit<A, E>, released: Exit<mixed, mixed>): Exit<A, E> => {
    if (released.kind === "failure" && settled.kind === "success") {
      return releaseDefect(released.cause);
    }
    return settled.kind === "success" ? success(settled.value) : failure(settled.cause);
  };
  return makeEffect({
    // The finalizer runs detached, for the same reason `scoped`'s does: a
    // cleanup that is itself cancelled is not a cleanup.
    run: async (runContext) => {
      const settled = await runKernel(self, runContext);
      return combine(settled, await runKernel(finalizer(), detachedContext(runContext)));
    },
    // A synchronous body with a synchronous finalizer is an ordinary case, and
    // without this it degraded to a defect saying the effect was asynchronous.
    runSync: (runContext) => {
      const settled = runSyncKernel(self, runContext);
      return combine(settled, runSyncKernel(finalizer(), detachedContext(runContext)));
    },
  });
}

/** Name a service, in a value that is also the effect reading it. */
export function tag<Service>(identifier: string): Tag<Service> {
  return makeTag(identifier);
}

/** Satisfy one requirement with a value already in hand. */
export function provideService<A, E, R, Service>(
  self: Effect<A, E, R | Service>,
  serviceTag: Tag<Service>,
  service: Service,
): Effect<A, E, R> {
  return makeEffect({
    run: (runContext) => runKernel(self, withService(runContext, serviceTag, service)),
    runSync: (runContext) => runSyncKernel(self, withService(runContext, serviceTag, service)),
  });
}

/**
 * Satisfy requirements by building them.
 *
 * The layer's own failure joins the effect's error channel, because a service
 * that could not be built is a way the whole thing can fail.
 */
export function provide<A, E, R, Out, LayerError, In>(
  self: Effect<A, E, R | Out>,
  layer: Layer<Out, LayerError, In>,
): Effect<A, E | LayerError, R | In> {
  return makeEffect({
    run: async (runContext) => {
      const built = await readLayer(layer)(runContext);
      if (built.kind === "failure") {
        return failure(built.cause);
      }
      const services = new Map(runContext.services);
      for (const [key, value] of built.value) {
        services.set(key, value);
      }
      // The fiber has to come through with the services. Every interruption
      // check reads `runContext.fiber`, so a context assembled without it makes
      // anything that can be interrupted throw instead — which nothing noticed
      // while no effect under a layer ever checked.
      const settled = await runKernel(self, {
        services,
        scope: runContext.scope,
        fiber: runContext.fiber,
      });
      return settled.kind === "success" ? success(settled.value) : failure(settled.cause);
    },
  });
}

/** A layer holding one service that is already built. */
export function layerSucceed<Service>(serviceTag: Tag<Service>, service: Service): Layer<Service> {
  const built: $ReadOnlyMap<string, mixed> = new Map([[readTag(serviceTag), service]]);
  const settled: Exit<$ReadOnlyMap<string, mixed>, empty> = success(built);
  return makeLayer(() => Promise.resolve(settled));
}

/** A layer that builds its service with an effect, which may itself fail. */
export function layerEffect<Service, E, R>(
  serviceTag: Tag<Service>,
  build: Effect<Service, E, R>,
): Layer<Service, E, R> {
  return makeLayer(async (runContext) => {
    const settled = await runKernel(build, runContext);
    if (settled.kind === "failure") {
      return failure(settled.cause);
    }
    return success(new Map([[readTag(serviceTag), settled.value]]));
  });
}

/** Both layers' services, left built first so the right may fail after it. */
export function layerMerge<Out1, Out2, E1, E2, In1, In2>(
  left: Layer<Out1, E1, In1>,
  right: Layer<Out2, E2, In2>,
): Layer<Out1 | Out2, E1 | E2, In1 | In2> {
  return makeLayer(async (runContext) => {
    const leftBuilt = await readLayer(left)(runContext);
    if (leftBuilt.kind === "failure") {
      return failure(leftBuilt.cause);
    }
    const rightBuilt = await readLayer(right)(runContext);
    if (rightBuilt.kind === "failure") {
      return failure(rightBuilt.cause);
    }
    const merged = new Map(leftBuilt.value);
    for (const [key, value] of rightBuilt.value) {
      merged.set(key, value);
    }
    return success(merged);
  });
}

/**
 * Start `self` beside the current fiber and hand back a handle to it.
 *
 * The child gets its own interruption state, so cancelling it does not cancel
 * the fiber that forked it, and cancelling the parent does not silently take
 * the child down with it. That is Effect's `forkDaemon` rather than its `fork`,
 * and it is what makes this usable for the case it exists for: starting work
 * you intend to be able to stop. Work that should die with its parent is what
 * `all`, `race` and `timeout` open a child fiber for.
 */
export function fork<A, E, R>(self: Effect<A, E, R>): Effect<Fiber<A, E>, empty, R> {
  return makeEffect({
    run: (runContext) => {
      const child = detachedContext(runContext);
      const started: Exit<Fiber<A, E>, empty> = success(
        makeFiber(runKernel(self, child), child.fiber),
      );
      return Promise.resolve(started);
    },
  });
}

/** Wait for a fiber and take its result as this effect's result. */
export function join<A, E>(fiber: Fiber<A, E>): Effect<A, E> {
  return makeEffect({
    run: async () => {
      const settled = await fiber.__promise;
      return settled.kind === "success" ? success(settled.value) : failure(settled.cause);
    },
  });
}

/**
 * Cancel a fiber and wait for it to actually stop.
 *
 * Interruption is cooperative — the flag is read between steps, and a `sleep`
 * is woken — so this resolves once the fiber has reached its next checkpoint,
 * not once the request was filed. Returning the `Exit` rather than `void` is
 * what makes that observable: a fiber that had already finished reports the
 * value it produced, and one that stopped reports `interrupt`, so a caller can
 * tell "cancelled in time" from "too late, it was done".
 */
export function interrupt<A, E>(fiber: Fiber<A, E>): Effect<Exit<A, E>> {
  return makeEffect({
    run: async () => {
      interruptFiber(fiber.__fiber);
      return success(await fiber.__promise);
    },
  });
}

/**
 * Wait, doing nothing.
 *
 * Interruptible: cancelling the fiber ends the wait now, rather than leaving a
 * timer holding the process open until it fires.
 */
export function sleep(millis: number): Effect<void> {
  return makeEffect({
    run: async (runContext) => {
      await pause(millis, runContext);
      return isInterrupted(runContext) ? interruptedExit() : success(undefined);
    },
  });
}

/** Run `self` after waiting, keeping its result. */
export function delay<A, E, R>(self: Effect<A, E, R>, millis: number): Effect<A, E, R> {
  return andThen(sleep(millis), self);
}

/**
 * Look at a success without changing it.
 *
 * The point of a `tap` is that it cannot alter the value by accident: logging a
 * result inside a `map` means one careless edit turns the log's return value
 * into the pipeline's value, and this shape makes that impossible.
 */
export function tap<A, E1, E2, R1, R2>(
  self: Effect<A, E1, R1>,
  body: (value: A) => Effect<mixed, E2, R2>,
): Effect<A, E1 | E2, R1 | R2> {
  return flatMap(self, (value) => map(body(value), () => value));
}

/** Look at a typed failure without recovering from it. */
export function tapError<A, E, R1, R2>(
  self: Effect<A, E, R1>,
  body: (error: E) => Effect<mixed, mixed, R2>,
): Effect<A, E, R1 | R2> {
  return catchAll(self, (error) => andThen(orDie(body(error)), fail(error)));
}

/**
 * Turn any typed failure into a defect, so the error channel becomes `empty`.
 *
 * For the boundary where a failure is no longer a condition anybody is going to
 * handle — a logging tap, a fire-and-forget notification — and letting it stay
 * in the error channel would only invite a `catchAll` that pretends to.
 */
export function orDie<A, E, R>(self: Effect<A, E, R>): Effect<A, empty, R> {
  const convert = (settled: Exit<A, E>): Exit<A, empty> =>
    settled.kind === "success"
      ? success(settled.value)
      : failure(dieCause(causeMessage(settled.cause)));

  return makeEffect({
    run: async (runContext) => convert(await runKernel(self, runContext)),
    // Without this, `runSyncExit` on anything built out of `tapError` — which
    // is `catchAll(self, error => andThen(orDie(body(error)), fail(error)))` —
    // reached an effect with no synchronous kernel and returned "effect is
    // asynchronous" as a defect, losing both the tap and the original failure.
    runSync: (runContext) => convert(runSyncKernel(self, runContext)),
  });
}

/**
 * Reify the outcome, so a failure is a value instead of a short circuit.
 *
 * `either` narrows to the typed error and leaves defects and interruption as
 * failures; this keeps the whole `Exit`, which is what a supervisor or a test
 * that asserts on *how* something failed actually needs.
 */
export function exit<A, E, R>(self: Effect<A, E, R>): Effect<Exit<A, E>, empty, R> {
  return makeEffect({
    run: async (runContext) => success(await runKernel(self, runContext)),
    runSync: (runContext) => success(runSyncKernel(self, runContext)),
  });
}

/**
 * Keep a success only when it passes `predicate`, failing with `error` if not.
 *
 * The alternative is a `flatMap` whose body is an `if` returning `succeed` or
 * `fail`, written out at every call site.
 */
export function filterOrFail<A, E1, E2, R>(
  self: Effect<A, E1, R>,
  predicate: (value: A) => boolean,
  error: (value: A) => E2,
): Effect<A, E1 | E2, R> {
  return flatMap(self, (value) => (predicate(value) ? succeed(value) : fail(error(value))));
}

/** Replace a success with a constant, keeping the failure channel. */
export function as<A, B, E, R>(self: Effect<A, E, R>, value: B): Effect<B, E, R> {
  return map(self, () => value);
}

/** Run an effect, raising whatever it failed with. */
export async function runPromise<A, E>(self: Effect<A, E>): Promise<A> {
  const settled = await runKernel(self, context());
  if (settled.kind === "success") {
    return settled.value;
  }
  throw throwable(settled.cause);
}

/** Run an effect, returning its outcome rather than raising. */
export function runPromiseExit<A, E>(self: Effect<A, E>): Promise<Exit<A, E>> {
  return runKernel(self, context());
}

/** Run a synchronous effect, returning its outcome rather than raising. */
export function runSyncExit<A, E>(self: Effect<A, E>): Exit<A, E> {
  return runSyncKernel(self, context());
}

/**
 * Run a synchronous effect, raising whatever it failed with.
 *
 * The counterpart of `runPromise` for effects with no asynchronous step. An
 * effect that turns out to have one fails as a defect rather than returning a
 * promise nobody is awaiting, because a silently-unawaited promise is how a
 * synchronous-looking call ends up returning `undefined`.
 */
export function runSync<A, E>(self: Effect<A, E>): A {
  const settled = runSyncExit(self);
  if (settled.kind === "success") {
    return settled.value;
  }
  throw throwable(settled.cause);
}

/** Start an effect from outside the runtime and keep a handle on it. */
export function runFork<A, E>(self: Effect<A, E>): Fiber<A, E> {
  const runContext = context();
  return makeFiber(runKernel(self, runContext), runContext.fiber);
}
