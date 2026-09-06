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
// # What a fiber owns
//
// A fiber owns the fibers it starts. `fork` gives back a handle to a child,
// and the child's lifetime is contained in its parent's: interrupting the
// parent interrupts the child, and a parent that simply *ends* interrupts
// whatever it still had running. Neither half is optional — a tree that only
// propagates cancellation downwards leaks every child of a fiber that returned
// normally, and a tree that only cleans up on return leaks every child of a
// fiber that was cancelled.
//
// That is a statement about the runtime and not about one combinator, so it is
// stated once here and enforced in three places: `childContext` links a new
// fiber to its parent, `interruptFiber` walks the link downwards, and
// `releaseChild` — which every group and `fork` end at — walks it upwards.
// `all`, `race` and `timeout` were already written against it; before this,
// `fork` was the one hole in it. The two deliberate exceptions are named below,
// and each is a name a reader has to have typed.
//
// `forkDaemon` is the deliberate way out, and it is a separate name rather
// than an option because the two answers fail in opposite directions. A caller
// who wanted a daemon and got a child sees their work stop, and goes looking.
// A caller who wanted a child and got a daemon sees nothing at all, until a
// cancelled request is still writing to a socket nobody is reading, or the
// process will not exit. The silent failure is the one that must be spelled
// out at the call site.
//
// `forkScoped` is the third answer, and the one a request handler usually
// means: a child tied to a `Scope` rather than to a fiber, which outlives the
// fiber that forked it and dies when the scope closes. That is the lifetime
// neither of the other two has — a background refresh should finish its own
// work and stop when the *request* is over — and `Scope` was already an
// ordinary member of `R`, so saying it costs nothing new in the types.
//
// `forkIn` — the same thing against a named scope rather than the enclosing
// one — is deliberately absent, and the reason is that it needs something this
// package does not have: a `Scope` that is a *value*. `Scope` here is a
// phantom that is never constructed, which is what lets `acquireRelease` state
// a lifetime in the type and makes `scoped` the only thing that discharges it.
// A constructible handle means a public `close`, and then two ways to close a
// scope — the combinator and the handle — with nothing stopping the second
// from closing a scope an `acquireRelease` is still registering into. A
// `forkScoped` inside a nested `scoped` says what `forkIn` says, with the
// nesting visible in the code rather than in a value somebody has to have
// passed to the right place. It stays out until something concrete needs a
// scope it cannot get by nesting.
//
// # Readiness
//
// **Implemented.** The `Effect<A, E, R>` value and its three channels;
// generator syntax with both `yield` and `yield*`, over effects and over other
// generators; `runSync`, `runPromise` and the `Exit`-returning `runSyncExit`,
// `runPromiseExit`, `exit`; typed failures kept distinct from defects and from
// interruption, with `catchAll`, `catchTag`, `orElse`, `either` and `orDie`.
//
// `retry` and `repeat` over a `Schedule` — a state machine with an input, an
// output and a decision, so one policy drives both, `repeat` gives back the
// number its schedule reached, and a policy can decide on the error or on the
// value rather than only on how many attempts have been made; `timeout`;
// `acquireRelease` with `scoped`, and `ensuring`, both of which release on
// success, failure, defect and interruption and both of which keep a
// synchronous program synchronous; `all` and `forEach` with a concurrency
// limit and a synchronous form when every element has one, `race`, and
// `fork`/`forkDaemon`/`forkScoped`/`join`/`interrupt` over fibers whose
// lifetimes nest.
//
// `Tag` and `Layer` for services, with `layerProvide` feeding one layer into
// another, `layerScoped` for a layer that acquires something, and one memoised
// build per `provide`, so a layer reached twice in one graph is built once and
// `runSync` still answers for a program that uses one — with `managedRuntime`
// for the other half, a layer built once for an application, many effects run
// against it, and a `dispose` that closes what it acquired.
//
// `Ref`, `Deferred`, `Semaphore`, `Queue` and `PubSub` for the things two
// fibers have to share, each of which a blocked fiber can be interrupted out
// of, and the last two of which put a bound on a producer that outruns its
// consumer. And a pull-based `Stream` in `./stream.js`, whose traversal closes
// what it opened on success, on failure and on interruption.
//
// **Experimental.** Requirement subtraction, for the reason above: `provide`,
// `provideService` and `scoped` state the service they remove and let Flow
// solve for the rest, which is weaker than Effect-TS's `Exclude`. `catchTag`
// reads a `kind` (or `tag`) string off the error at run time and does not
// narrow `E` for the recovery function, because Flow cannot narrow a type
// variable by a string compared at run time. `layerProvide` and `layerScoped`
// subtract requirements the same way and carry the same caveat.
//
// **Not implemented.** STM — a transaction log and a retry-on-conflict
// scheduler, which is larger than everything above it put together and has no
// use that a serialised `Ref` cannot serve at a cost worth measuring first;
// `Channel`, `Sink`, `GroupBy` and `Chunk`, which `./stream.js` explains being
// without rather than pending; `forkIn` and a schedule's output as a type
// parameter, each argued against where it would have gone; a fiber scheduler
// of its own (this runs on the host's microtask queue and its `sleep` is
// `setTimeout`); tracing, spans, metrics and the logging layer; a typed defect
// channel; a heterogeneous `all`, which in Effect-TS keeps a tuple's element
// types and here takes and returns one array type; and Effect's `"inherit"`
// concurrency, because no enclosing limit is tracked to inherit.
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
// `./schedule.js` is the one thing that is genuinely separable on those terms,
// and it is separate: a policy is a state machine over a state and an input
// that never sees a `Context`, an `Exit` or a fiber — it is handed the clock
// and the random factor rather than reading either — and it explains itself
// there.
//
// `./stream.js` is separate on different terms, and finding out which was the
// first task of writing it. A stream does reach the runtime — it produces
// effects and runs them — so the argument above looked like it applied, and
// the choice looked like "put it in this file or give the runtime a seam".
// Neither: a `Stream` needs the `Effect` *interface* and never the `Effect`
// *carrier*, so it is written entirely with this file's public exports and
// there is no seam here for it. The rule that generalises is worth keeping —
// "this needs the runtime" is usually "this needs the runtime's public API",
// and only the second one costs an invariant.
//
// That split costs one thing today, and it is uf's rather than Flow's:
// `uf check` does not yet resolve types across modules, so it reports the
// imported `Schedule` as an any-typed value and `retry`'s parameter is
// unchecked by it. `flow` itself checks it, and `@uniflowed/form`,
// `@uniflowed/hooks` and `@uniflowed/immer` are in the same position for the
// same reason. It is a checker gap to close, not a reason to put a retry
// policy back inside a runtime.

import { scheduleStart, scheduleStep } from "./schedule.js";
import type { Schedule, ScheduleDecision, ScheduleState } from "./schedule.js";

export type { Schedule, ScheduleDecision, ScheduleState };

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

/**
 * How one layer builds its services.
 *
 * The same two-kernel shape as `EffectKernel`, for the same reason. `build` is
 * the general form and every layer has one; `buildSync` is present on the
 * layers whose services can be produced without yielding to the event loop,
 * and `provide` has a synchronous kernel because of it. Before this, every
 * layer was a `Promise` by construction, so "use a layer" and "use `runSync`"
 * were mutually exclusive — including in a test, which is where `runSync`
 * earns its keep.
 *
 * Both take the memo for the build they belong to. A layer graph is a graph
 * and not a tree — `Database` and `Logger` both wanting `Config` is the
 * ordinary shape — so a builder that does not remember what it has already
 * built walks the graph once per path and opens two connection pools.
 */
type LayerKernel<out E> = {
  readonly build: (Context, LayerMemo) => Promise<Exit<$ReadOnlyMap<string, mixed>, E>>,
  readonly buildSync?: (Context, LayerMemo) => Exit<$ReadOnlyMap<string, mixed>, E>,
};

/**
 * What one build pass has already built, keyed by layer identity.
 *
 * Successes only, and that is not a shortcut: a failed build ends the pass, so
 * nothing can ask for a second layer after one has failed. Holding the
 * services rather than the `Exit` is also what keeps the memo's value type
 * free of `E` — a `Map<Layer<…>, Exit<…, E>>` holding layers with different
 * error types cannot be read back at the type it was written at, and that
 * would have cost this file a fifth suppression to buy nothing.
 *
 * The memo lives for one `provide` and is keyed by the layer object, so a
 * layer that appears twice in one graph is built once and a layer handed to
 * two `provide`s is built twice. That second half is what `managedRuntime` is
 * for: one build for a whole application, and a `dispose` rather than the end
 * of one effect.
 */
type LayerMemo = Map<Layer<mixed, mixed, mixed>, $ReadOnlyMap<string, mixed>>;

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
 * A layer already built, and the scope it was built in.
 *
 * `disposed` is on the carrier rather than inferred from the scope, because a
 * scope with nothing to close and a scope that has been closed look the same
 * from outside and mean opposite things.
 */
type RuntimeCarrier<R> = {
  readonly __kind: "Runtime",
  readonly __provides: () => R,
  readonly services: $ReadOnlyMap<string, mixed>,
  readonly scope: ScopeState,
  disposed: boolean,
};

type RefCarrier<A> = {
  readonly __kind: "Ref",
  value: A,
};

/**
 * A one-shot handshake: a value that has not been produced yet, and everyone
 * waiting for it.
 *
 * `settled` is the `Exit` once there is one and `null` before, which is why
 * it is not a plain `?A`: a `Deferred<?string>` completed with `null` is done,
 * and telling that from "not yet" is the whole job.
 */
type DeferredState<A, E> = {
  settled: ?Exit<A, E>,
  readonly waiters: Set<(Exit<A, E>) => void>,
};

type DeferredCarrier<A, E> = {
  readonly __kind: "Deferred",
  readonly state: DeferredState<A, E>,
};

/** One fiber queued for permits, and how to tell it whether it got them. */
type SemaphoreWaiter = {
  readonly permits: number,
  readonly settle: (taken: boolean) => void,
};

/**
 * Permits, and the fibers queued for them in the order they asked.
 *
 * `capacity` is kept so that a request for more permits than exist can be a
 * defect rather than a fiber that waits for ever: nothing will ever release
 * enough, and a hang is the least debuggable way to say so.
 *
 * The queue is served strictly from the head. Letting a later small request
 * past a waiting large one would raise throughput and starve the large one,
 * and a semaphore nobody can rely on for the widest request is not a bound.
 */
type SemaphoreState = {
  available: number,
  readonly capacity: number,
  readonly waiters: Array<SemaphoreWaiter>,
};

type SemaphoreCarrier = {
  readonly __kind: "Semaphore",
  readonly state: SemaphoreState,
};

/**
 * What a full queue does with one more value.
 *
 * `bounded` is the only one of the three that is back pressure: the producer
 * waits, and a producer that waits is a producer that cannot outrun its
 * consumer. The other two keep the producer running and lose a value instead —
 * `dropping` loses the value being offered, `sliding` loses the oldest one
 * already waiting — which is what a metrics feed or a live cursor position
 * wants, where the newest reading is the only interesting one.
 */
export type QueueStrategy = "bounded" | "dropping" | "sliding";

/**
 * What one attempt to take a value found, without waiting for another.
 *
 * `empty` is the only one that means "wait"; it never leaves this module,
 * because every caller either waits on it or reports it as the reason a
 * synchronous run could not answer.
 */
type QueueTaken<A> =
  | { readonly kind: "taken", readonly value: A }
  | { readonly kind: "stopped" }
  | { readonly kind: "empty" };

/** What one attempt to offer a value did, `full` being the one that waits. */
type QueueOffered = "accepted" | "dropped" | "stopped" | "full";

/** One fiber waiting for a value, and how to give it one or tell it to stop. */
type QueueTaker<A> = {
  readonly settle: (QueueTaken<A>) => void,
};

/** One fiber waiting for room, still holding the value it could not put down. */
type QueueOfferer<A> = {
  readonly value: A,
  readonly settle: (accepted: boolean) => void,
};

/**
 * The buffer, and the fibers queued at each end of it.
 *
 * Two invariants hold for every capacity of at least one, and everything below
 * is written to keep them: a fiber waits in `takers` only while `items` is
 * empty, and in `offerers` only while `items` is full. They cannot both be
 * non-empty, which is why an offer never has to choose between handing its
 * value to a waiting taker and appending it behind a waiting offerer.
 *
 * Both queues are served strictly from the head, for the reason
 * `releasePermits` records: a later taker that overtook a waiting one would
 * raise throughput and starve the one that asked first, and an order nobody
 * can predict is not an order.
 */
type QueueState<A> = {
  readonly capacity: number,
  readonly strategy: QueueStrategy,
  readonly items: Array<A>,
  readonly takers: Array<QueueTaker<A>>,
  readonly offerers: Array<QueueOfferer<A>>,
  shutdown: boolean,
};

type QueueCarrier<A> = {
  readonly __kind: "Queue",
  readonly state: QueueState<A>,
};

/**
 * The subscribers, and the buffer each of them gets.
 *
 * A `PubSub` owns no buffer of its own: it owns the *shape* of one, and hands
 * every subscriber a queue of that shape. That is the whole implementation of
 * "the same buffer with several subscribers", and it is why a subscription is
 * an ordinary `Queue` that `queueTake` and `queueTakeAll` already work on
 * rather than a second family of readers.
 */
type PubSubState<A> = {
  readonly capacity: number,
  readonly strategy: QueueStrategy,
  readonly subscribers: Set<QueueState<A>>,
  shutdown: boolean,
};

type PubSubCarrier<A> = {
  readonly __kind: "PubSub",
  readonly state: PubSubState<A>,
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
 * Services built once, for an application rather than for one effect.
 *
 * `provide` builds a layer, runs one effect against it and closes the scope
 * when that effect ends, which is right for what `provide` is and wrong for
 * what a server is: a request handler is an `Effect` per request against a
 * connection pool opened once at boot. Building the layer per request opens two
 * pools a second; assembling a `provideService` chain by hand at the entry point
 * is the thing `Layer` exists to replace.
 *
 * Invariant in `R`, unlike `Effect` and `Layer`, and that is what makes the
 * requirement check mean something here. `Effect` is covariant in `R`, so with
 * a covariant `Runtime` the checker could satisfy `runtimeRunPromise` by
 * widening *both* sides to a union containing a service the runtime does not
 * have. Pinned to exactly what the layer produced, the effect has to be a
 * subtype of it, which is the ordinary subtyping the issue this came from
 * expected and not another instance of the requirement-subtraction caveat.
 */
export opaque type Runtime<R> = RuntimeCarrier<R>;

/**
 * A place two fibers can both read and write.
 *
 * Invariant in `A`, unlike `Effect` and `Fiber`: a `Ref` is written as well as
 * read, so a `Ref<string>` is not a `Ref<mixed>` — writing a number through
 * the second would break the first.
 *
 * The operations are flat monomorphic functions — `refGet`, `refUpdate` — and
 * not methods or a `Ref` namespace object. That is this package's convention
 * rather than a preference expressed once more: there is no `pipe` with
 * inference to lose, nothing to dispatch at run time, and a bundler can drop
 * the eleven of these a program does not call. It is stated here because
 * `Ref` is the first type where a namespace would have looked natural.
 */
export opaque type Ref<A> = RefCarrier<A>;

/**
 * A value one fiber will produce and others are waiting for.
 *
 * `Fiber` covers "wait for the thing this fiber returns". This covers the
 * other one: waiting for a value nobody has promised yet — a one-shot
 * handshake, a lazy singleton, "the first caller does the work and the rest
 * wait".
 */
export opaque type Deferred<A, E = empty> = DeferredCarrier<A, E>;

/**
 * Permission to run, in a fixed number of copies.
 *
 * `all`'s `concurrency` bounds one call. This bounds a budget shared across
 * call sites, which is what a rate-limited API needs: two independent
 * `forEach`es against the same host can hold one of these between them.
 */
export opaque type Semaphore = SemaphoreCarrier;

/**
 * A bounded place to hand work from one fiber to another.
 *
 * The type the rest of this package was waiting on. `all` collects into an
 * array, so a producer that outruns its consumer grows one until the process
 * dies and there is no seam to put a limit in; a `Queue` is that seam. A
 * `bounded` queue makes the producer wait, which is the only one of the three
 * strategies that is back pressure rather than a way of losing values politely.
 *
 * Invariant in `A` for the reason `Ref` is: a queue is written as well as read,
 * so a `Queue<string>` is not a `Queue<mixed>`.
 *
 * Every operation but `queueShutdown` and `queueIsShutdown` reports an
 * *interruption* on a queue that has been shut down, waiting or not. That is
 * one rule rather than two, and it is the one `Cause` already has a word for:
 * a shutdown is a decision somebody took, which is exactly what `interrupt`
 * means and exactly what `fail` does not.
 */
export opaque type Queue<A> = QueueCarrier<A>;

/**
 * One value, delivered to everybody who is listening when it is published.
 *
 * `Queue` is one-to-one — a value taken by one consumer is gone. This is the
 * many-to-many case: every subscriber gets its own buffer, so a slow one falls
 * behind rather than stealing from a fast one, and with `bounded` a slow one
 * makes the publisher wait.
 *
 * A subscription is an ordinary `Queue`, which is what makes this a small type
 * rather than a second family of readers: `queueTake`, `queueTakeAll` and
 * `queueSize` are already the operations a subscriber needs.
 */
export opaque type PubSub<A> = PubSubCarrier<A>;

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
 * A context with a built layer's services added to it.
 *
 * The fiber comes through unchanged, and that is load-bearing rather than
 * tidy: every interruption check reads `runContext.fiber`, so a context
 * assembled without it makes anything underneath a layer uninterruptible.
 */
function withServices(parent: Context, added: $ReadOnlyMap<string, mixed>): Context {
  const services = new Map(parent.services);
  for (const [key, value] of added) {
    services.set(key, value);
  }
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
 * A fiber has settled: stop whatever it still had running.
 *
 * The half of structured concurrency that is not about cancellation. A fiber
 * that returns normally is as finished as one that was interrupted, and its
 * children are as orphaned either way — nobody is left holding a handle to
 * them, so nothing will ever stop them. Effect ties a `fork`ed child to the
 * parent's scope, which closes when the parent terminates however it
 * terminates; this is the same rule said in terms of the link `childContext`
 * already builds.
 *
 * Called wherever a fiber's run is known to have settled: both forks, the two
 * asynchronous runners, and `releaseChild`. The synchronous runners need no
 * call — `fork` has no `runSync` kernel, so a program `runSync` will answer for
 * has no children to end.
 */
function endFiber(state: FiberState): void {
  for (const child of state.children) {
    interruptFiber(child);
  }
}

/**
 * Forget a finished child, having first stopped whatever it started.
 *
 * Two failures, and one call site fixes both. Without the `delete`, a
 * long-lived fiber calling `all` in a loop holds every group it ever opened,
 * and that leak is invisible because nothing reads the set except an
 * interruption that never comes. Without the `endFiber`, the `delete` *causes*
 * a leak rather than closing one: cutting a finished fiber out of the tree
 * takes its own still-running children with it, and they are then unreachable
 * from any root. Removing a subtree is only safe once the subtree is empty.
 */
function releaseChild(parent: Context, child: Context): void {
  endFiber(child.fiber);
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

/**
 * Build a layer once per pass.
 *
 * The memo is consulted here rather than inside a layer's own kernel because
 * a kernel has no name for the carrier it belongs to, and identity is the key.
 * Every composite — `layerMerge`, `layerProvide` — reaches its children
 * through this rather than through `readLayer`, which is what makes a diamond
 * build its shared node once.
 *
 * A memoised layer keeps the services it was first built with. Handing the
 * same layer to two different `layerProvide`s in one graph therefore gives it
 * one of the two outers rather than each in turn; that is Effect's rule too,
 * and the answer is two layers rather than one used twice.
 */
function buildLayer<Out, E, In>(
  layer: Layer<Out, E, In>,
  runContext: Context,
  memo: LayerMemo,
): Promise<Exit<$ReadOnlyMap<string, mixed>, E>> {
  const alreadyBuilt = memo.get(layer);
  if (alreadyBuilt != null) {
    const settled: Exit<$ReadOnlyMap<string, mixed>, E> = success(alreadyBuilt);
    return Promise.resolve(settled);
  }
  return readLayer(layer)
    .build(runContext, memo)
    .then((built) => {
      if (built.kind === "success") {
        memo.set(layer, built.value);
      }
      return built;
    });
}

/**
 * Build a layer once per pass, without yielding to the event loop.
 *
 * A layer with no synchronous kernel ends the run the same way an effect with
 * none does, and by the same route: a defect naming what was asked for, rather
 * than a promise nobody is awaiting.
 */
function buildLayerSync<Out, E, In>(
  layer: Layer<Out, E, In>,
  runContext: Context,
  memo: LayerMemo,
): Exit<$ReadOnlyMap<string, mixed>, E> {
  const alreadyBuilt = memo.get(layer);
  if (alreadyBuilt != null) {
    return success(alreadyBuilt);
  }
  const buildSync = readLayer(layer).buildSync;
  if (buildSync == null) {
    return failure(dieCause("layer is asynchronous"));
  }
  const built = buildSync(runContext, memo);
  if (built.kind === "success") {
    memo.set(layer, built.value);
  }
  return built;
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
 *
 * Filed as #205. The defect is in the vendored Flow port rather than in uf's
 * embedding of it, so the wait is on upstream. The reproduction, and the line
 * that drops the type, are in `crates/uf_check/tests/known_bugs.rs`.
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
 * Which typed failures another attempt is worth making for.
 *
 * A separate parameter rather than a `{ schedule, while, until }` union in
 * `retry`'s second position, which is what Effect takes. Flow's objects are
 * exact, so a union of a `Schedule` and an options object cannot be refined by
 * reading a property one of them does not have, and the trick that makes it
 * possible would be a worse thing to explain than a third parameter. Effect's
 * `times` is not here either: it is `intersect` with `recurs`, which the
 * schedule union already says, and two ways to say one thing is the cost of
 * copying an API rather than reading it.
 *
 * A `Schedule<E>` can now read the error itself, so `whileInput` says the same
 * thing inside a policy. That is not a duplicate: this is the condition the
 * *call site* knows, and a policy is the one a config file could name. Both
 * have to allow an attempt for one to happen.
 *
 * The predicates see the first typed failure in the cause, which is the same
 * error `catchAll` would hand a recovery function and the same one the schedule
 * is stepped with.
 */
export type RetryOptions<in E> = {
  readonly while?: (error: E) => boolean,
  readonly until?: (error: E) => boolean,
};

/** The same, over the value a `repeat` produced. */
export type RepeatOptions<in A> = {
  readonly while?: (value: A) => boolean,
  readonly until?: (value: A) => boolean,
};

/**
 * Whether another attempt is worth making.
 *
 * `isRetriable` draws the line the runtime knows about — a defect is a bug and
 * an interruption is a decision already taken — and the predicates draw the
 * line only the application knows: a 429 is worth retrying and a 400 is not,
 * and both are typed failures.
 *
 * Total apart from the caller's predicate, which is why the call site catches:
 * a predicate that throws is a bug in the predicate, and it becomes a defect
 * rather than an extra attempt or a silent stop.
 */
function worthRetrying<E>(cause: Cause<E>, options: ?RetryOptions<E>): boolean {
  if (!isRetriable(cause)) {
    return false;
  }
  if (options == null) {
    return true;
  }
  const found = failureNode(cause);
  if (found == null) {
    return false;
  }
  return allowedBy(found.error, options.while, options.until);
}

/** The same two predicates over a success, for `repeat`. */
function worthRepeating<A>(value: A, options: ?RepeatOptions<A>): boolean {
  return options == null || allowedBy(value, options.while, options.until);
}

/** `while` must hold and `until` must not, and an absent one is no opinion. */
function allowedBy<T>(
  subject: T,
  whilePredicate: ?(T) => boolean,
  untilPredicate: ?(T) => boolean,
): boolean {
  if (whilePredicate != null && !whilePredicate(subject)) {
    return false;
  }
  return untilPredicate == null || !untilPredicate(subject);
}

/**
 * Try again on a typed failure, on the schedule's timetable.
 *
 * The first run is not a retry, so `{ kind: "recurs", times: 2 }` runs the
 * effect three times. Only a typed failure is worth another attempt: a defect
 * is a bug, so running it again runs the bug again, and an interruption is a
 * decision already taken.
 *
 * The schedule's input is the error, so a policy can decide on it —
 * `{ kind: "whileInput", schedule: backoff, predicate: (e) => e.kind !== "forbidden" }`
 * is a policy that gives up on a rejection. `options` says the same thing at
 * the call site; both have to allow an attempt.
 *
 * `Date.now()` is read once per decision and passed to the schedule, which is
 * what lets `fixed` subtract the time the attempt itself took, and what keeps
 * `scheduleStep` a pure function of its arguments. The random factor a
 * `jittered` schedule needs is drawn in the same place and for the same reason.
 */
export function retry<A, E, R>(
  self: Effect<A, E, R>,
  schedule: Schedule<E>,
  options?: RetryOptions<E>,
): Effect<A, E, R> {
  return makeEffect({
    run: async (runContext) => {
      let state = scheduleStart(Date.now());
      let settled = await runKernel(self, runContext);
      while (settled.kind === "failure" && !isInterrupted(runContext)) {
        const cause = settled.cause;
        let decision;
        try {
          if (!worthRetrying(cause, options)) {
            return settled;
          }
          const found = failureNode(cause);
          if (found == null) {
            // `worthRetrying` said yes, which means `isRetriable` found a typed
            // failure in this cause, so there is one here to find. Written as a
            // branch rather than asserted, because a schedule with no input to
            // step on has no answer and giving up is the total one.
            return settled;
          }
          decision = scheduleStep(schedule, state, found.error, Date.now(), Math.random());
        } catch (error) {
          return defect(error);
        }
        if (decision.kind === "done") {
          return settled;
        }
        state = decision.state;
        await pause(decision.delayMillis, runContext);
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
 * Run again on *success*, on the schedule's timetable: a poll, a heartbeat, a
 * cache refresh.
 *
 * The other half of what a schedule is for. The first run is not a repetition,
 * so `{ kind: "recurs", times: 2 }` runs the effect three times, and
 * `{ kind: "spaced", millis: 1000 }` runs it until something stops it.
 *
 * The value is the schedule's *input*, so "poll until the job reports finished"
 * is a schedule rather than a loop:
 * `{ kind: "untilInput", schedule: everySecond, predicate: (job) => job.done }`.
 * `options` says the same thing at the call site, the way `retry`'s does.
 *
 * The result is the schedule's output — the number of repetitions, the elapsed
 * time, whichever number the schedule's last decision reached — which is what
 * Effect's `repeat` gives back and what this could not give back while a
 * schedule was arithmetic over an attempt count. The effect's own last value is
 * one `tap` or one `Ref` away and was never the interesting half: a poll that
 * ran eleven times wants to say eleven.
 *
 * A failure ends the repetition and is the result. That is not a policy choice:
 * an effect that failed produced no value to repeat *from*, and swallowing the
 * failure to keep polling would hide the outage the poll exists to notice.
 * `retry` is what wraps an unreliable step, and the two compose —
 * `repeat(retry(poll, backoff), everySecond)` is a poll that tolerates a blip
 * and stops on a real failure.
 *
 * Interruption is checked before each wait and after it, so a fiber polling
 * once a minute stops when it is cancelled rather than at the top of the next
 * minute, and reports an interruption rather than the last number it happened
 * to have.
 */
export function repeat<A, E, R>(
  self: Effect<A, E, R>,
  schedule: Schedule<A>,
  options?: RepeatOptions<A>,
): Effect<number, E, R> {
  return makeEffect({
    run: async (runContext) => {
      let state = scheduleStart(Date.now());
      let output = state.output;
      let settled = await runKernel(self, runContext);
      while (settled.kind === "success") {
        if (isInterrupted(runContext)) {
          return interruptedExit();
        }
        const value = settled.value;
        let decision;
        try {
          if (!worthRepeating(value, options)) {
            return success(output);
          }
          decision = scheduleStep(schedule, state, value, Date.now(), Math.random());
        } catch (error) {
          return defect(error);
        }
        output = decision.output;
        if (decision.kind === "done") {
          return success(output);
        }
        state = decision.state;
        await pause(decision.delayMillis, runContext);
        if (isInterrupted(runContext)) {
          return interruptedExit();
        }
        settled = await runKernel(self, runContext);
      }
      return failure(settled.cause);
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
    // Acquiring is not inherently asynchronous — a file handle, a prepared
    // statement, an object taken out of a pool — and there is no reason a
    // program made of synchronous steps should lose `runSync` for using a
    // resource. `ensuring` already had this arm for the same reason.
    runSync: (runContext) => {
      const scope = runContext.scope;
      if (scope == null) {
        return defect("acquireRelease needs a Scope; wrap the effect in scoped()");
      }
      const settled = runSyncKernel(acquire, runContext);
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
      const broken = state == null ? null : await closeScope(state, runContext);
      if (settled.kind === "success" && broken != null) {
        return releaseDefect(broken);
      }
      return settled.kind === "success" ? success(settled.value) : failure(settled.cause);
    },
    // The other half of `acquireRelease`'s synchronous arm. Without it a scope
    // could be opened without waiting and never closed without waiting, which
    // is an asymmetry with no argument behind it.
    runSync: (runContext) => {
      const scopedContext = withScope(runContext);
      const state = scopedContext.scope;
      const settled = runSyncKernel(self, scopedContext);
      const broken = state == null ? null : closeScopeSync(state, runContext);
      if (settled.kind === "success" && broken != null) {
        return releaseDefect(broken);
      }
      return settled.kind === "success" ? success(settled.value) : failure(settled.cause);
    },
  });
}

/**
 * Run a scope's finalizers, newest first, and report the first that failed.
 *
 * Detached, because a scope closing *because* its fiber was interrupted still
 * has to release what it took, and a finalizer running under the interrupted
 * fiber would stop at its own first checkpoint.
 *
 * Shared by `scoped` and `provide`: a layer that acquires a resource has the
 * same lifetime problem as an effect that does, and it would be a poor answer
 * to solve it twice slightly differently.
 */
async function closeScope(state: ScopeState, runContext: Context): Promise<?Cause<mixed>> {
  const finalizers = state.finalizers;
  let broken: ?Cause<mixed> = null;
  for (let index = finalizers.length - 1; index >= 0; index -= 1) {
    const released = await runKernel(finalizers[index](), detachedContext(runContext));
    if (released.kind === "failure" && broken == null) {
      broken = released.cause;
    }
  }
  return broken;
}

/**
 * The same, for a run that has promised not to yield to the event loop.
 *
 * A finalizer with no synchronous kernel fails the way any other asynchronous
 * effect does under `runSync`, and that failure is reported rather than
 * swallowed: a release that did not happen is exactly the news this returns.
 */
function closeScopeSync(state: ScopeState, runContext: Context): ?Cause<mixed> {
  const finalizers = state.finalizers;
  let broken: ?Cause<mixed> = null;
  for (let index = finalizers.length - 1; index >= 0; index -= 1) {
    const released = runSyncKernel(finalizers[index](), detachedContext(runContext));
    if (released.kind === "failure" && broken == null) {
      broken = released.cause;
    }
  }
  return broken;
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
 *
 * One build pass, with one memo, so a layer reached twice in the graph is
 * built once. The pass also gets a scope of its own — separate from the body's
 * — which is what a `layerScoped` resource is released at: the layer that
 * opened a connection pool has somewhere to close it, and it closes after the
 * body that was using it has finished, however it finished.
 *
 * That scope is deliberately not the body's. Handing the body a scope here
 * would silently discharge the `Scope` an `acquireRelease` inside it requires,
 * and the requirement channel would go on saying otherwise; `scoped` is still
 * the only thing that answers for an effect's own resources.
 */
export function provide<A, E, R, Out, LayerError, In>(
  self: Effect<A, E, R | Out>,
  layer: Layer<Out, LayerError, In>,
): Effect<A, E | LayerError, R | In> {
  return makeEffect({
    run: async (runContext) => {
      const layerScope: ScopeState = { finalizers: [] };
      const buildContext = {
        services: runContext.services,
        scope: layerScope,
        fiber: runContext.fiber,
      };
      const built = await buildLayer(layer, buildContext, new Map());
      if (built.kind === "failure") {
        // A merge whose left side acquired and whose right side failed has
        // something to give back, and the build's own failure is the more
        // useful half of the news, so it survives the release.
        await closeScope(layerScope, runContext);
        return failure(built.cause);
      }
      const settled = await runKernel(self, withServices(runContext, built.value));
      const broken = await closeScope(layerScope, runContext);
      if (settled.kind === "success" && broken != null) {
        return releaseDefect(broken);
      }
      return settled.kind === "success" ? success(settled.value) : failure(settled.cause);
    },
    // Nothing about a layer is inherently asynchronous: `layerSucceed` holds a
    // value that is already built, and a `layerEffect` over a `sync` has an
    // answer to give. This is the arm that was missing, and without it
    // `runSync` and `provide` could not appear in the same program.
    runSync: (runContext) => {
      const layerScope: ScopeState = { finalizers: [] };
      const buildContext = {
        services: runContext.services,
        scope: layerScope,
        fiber: runContext.fiber,
      };
      const built = buildLayerSync(layer, buildContext, new Map());
      if (built.kind === "failure") {
        closeScopeSync(layerScope, runContext);
        return failure(built.cause);
      }
      const settled = runSyncKernel(self, withServices(runContext, built.value));
      const broken = closeScopeSync(layerScope, runContext);
      if (settled.kind === "success" && broken != null) {
        return releaseDefect(broken);
      }
      return settled.kind === "success" ? success(settled.value) : failure(settled.cause);
    },
  });
}

/** A layer holding one service that is already built. */
export function layerSucceed<Service>(serviceTag: Tag<Service>, service: Service): Layer<Service> {
  const built: $ReadOnlyMap<string, mixed> = new Map([[readTag(serviceTag), service]]);
  const settled: Exit<$ReadOnlyMap<string, mixed>, empty> = success(built);
  return makeLayer({
    build: () => Promise.resolve(settled),
    buildSync: () => settled,
  });
}

/** A layer that builds its service with an effect, which may itself fail. */
export function layerEffect<Service, E, R>(
  serviceTag: Tag<Service>,
  build: Effect<Service, E, R>,
): Layer<Service, E, R> {
  const identifier = readTag(serviceTag);
  const collect = (settled: Exit<Service, E>): Exit<$ReadOnlyMap<string, mixed>, E> =>
    settled.kind === "failure"
      ? failure(settled.cause)
      : success(new Map([[identifier, settled.value]]));
  return makeLayer({
    build: async (runContext) => collect(await runKernel(build, runContext)),
    buildSync: (runContext) => collect(runSyncKernel(build, runContext)),
  });
}

/**
 * A layer that acquires something, released when the `provide` using it ends.
 *
 * The difference from `layerEffect` is entirely in the type, and that is the
 * point rather than an admission: `provide` gives every build a scope, so a
 * `layerEffect` over an `acquireRelease` would already release — but its
 * `Scope` requirement would sit in the layer's `In` for ever, and a `Layer`
 * has no `scoped` of its own to discharge it with. This is that discharge, in
 * the same shape and with the same caveat `scoped` carries: the service being
 * removed is stated and Flow solves for the rest.
 *
 * The resource outlives the build and dies with the `provide`, which is the
 * whole reason a pool belongs in a layer rather than in the body.
 */
export function layerScoped<Service, E, R>(
  serviceTag: Tag<Service>,
  build: Effect<Service, E, R | Scope>,
): Layer<Service, E, R> {
  const identifier = readTag(serviceTag);
  const collect = (settled: Exit<Service, E>): Exit<$ReadOnlyMap<string, mixed>, E> =>
    settled.kind === "failure"
      ? failure(settled.cause)
      : success(new Map([[identifier, settled.value]]));
  return makeLayer({
    build: async (runContext) => collect(await runKernel(build, runContext)),
    buildSync: (runContext) => collect(runSyncKernel(build, runContext)),
  });
}

/** Both layers' services, left built first so the right may fail after it. */
export function layerMerge<Out1, Out2, E1, E2, In1, In2>(
  left: Layer<Out1, E1, In1>,
  right: Layer<Out2, E2, In2>,
): Layer<Out1 | Out2, E1 | E2, In1 | In2> {
  return makeLayer({
    build: async (runContext, memo) => {
      const leftBuilt = await buildLayer(left, runContext, memo);
      if (leftBuilt.kind === "failure") {
        return failure(leftBuilt.cause);
      }
      const rightBuilt = await buildLayer(right, runContext, memo);
      if (rightBuilt.kind === "failure") {
        return failure(rightBuilt.cause);
      }
      return success(mergedServices(leftBuilt.value, rightBuilt.value));
    },
    buildSync: (runContext, memo) => {
      const leftBuilt = buildLayerSync(left, runContext, memo);
      if (leftBuilt.kind === "failure") {
        return failure(leftBuilt.cause);
      }
      const rightBuilt = buildLayerSync(right, runContext, memo);
      if (rightBuilt.kind === "failure") {
        return failure(rightBuilt.cause);
      }
      return success(mergedServices(leftBuilt.value, rightBuilt.value));
    },
  });
}

/**
 * Build `inner` with `outer`'s services in scope, discharging what it needed.
 *
 * This is what makes a layer's `In` mean something. Before it, `In` was
 * carried through every signature and could never be satisfied: a
 * `Layer<Database, ConfigError, Config>` could only be used by an effect that
 * still required `Config`, so the requirement was a label rather than a debt
 * anything could pay.
 *
 * The same requirement-subtraction caveat as `provide`, in the same words:
 * Flow has no type-level set difference, so the discharged service is stated
 * — `inner` is typed as needing `In1 | Out2` — and the checker solves for
 * `In1`. That works when `Out2` is a distinct member of the union and silently
 * leaves it in `In1` when it is not. See Readiness.
 *
 * `outer` is built first and through the pass's memo, so a `Config` that two
 * layers both provide into is built once.
 */
export function layerProvide<Out, E1, In1, Out2, E2, In2>(
  inner: Layer<Out, E1, In1 | Out2>,
  outer: Layer<Out2, E2, In2>,
): Layer<Out, E1 | E2, In1 | In2> {
  return makeLayer({
    build: async (runContext, memo) => {
      const outerBuilt = await buildLayer(outer, runContext, memo);
      if (outerBuilt.kind === "failure") {
        return failure(outerBuilt.cause);
      }
      const innerBuilt = await buildLayer(inner, withServices(runContext, outerBuilt.value), memo);
      return innerBuilt.kind === "failure" ? failure(innerBuilt.cause) : success(innerBuilt.value);
    },
    buildSync: (runContext, memo) => {
      const outerBuilt = buildLayerSync(outer, runContext, memo);
      if (outerBuilt.kind === "failure") {
        return failure(outerBuilt.cause);
      }
      const innerBuilt = buildLayerSync(inner, withServices(runContext, outerBuilt.value), memo);
      return innerBuilt.kind === "failure" ? failure(innerBuilt.cause) : success(innerBuilt.value);
    },
  });
}

/**
 * `layerProvide`, keeping the outer layer's services in the result.
 *
 * For the ordinary case where `Config` is wanted by the application as well as
 * by the `Database` it was built for. Free, because the merge is the one line
 * that differs.
 */
export function layerProvideMerge<Out, E1, In1, Out2, E2, In2>(
  inner: Layer<Out, E1, In1 | Out2>,
  outer: Layer<Out2, E2, In2>,
): Layer<Out | Out2, E1 | E2, In1 | In2> {
  return makeLayer({
    build: async (runContext, memo) => {
      const outerBuilt = await buildLayer(outer, runContext, memo);
      if (outerBuilt.kind === "failure") {
        return failure(outerBuilt.cause);
      }
      const innerBuilt = await buildLayer(inner, withServices(runContext, outerBuilt.value), memo);
      return innerBuilt.kind === "failure"
        ? failure(innerBuilt.cause)
        : success(mergedServices(outerBuilt.value, innerBuilt.value));
    },
    buildSync: (runContext, memo) => {
      const outerBuilt = buildLayerSync(outer, runContext, memo);
      if (outerBuilt.kind === "failure") {
        return failure(outerBuilt.cause);
      }
      const innerBuilt = buildLayerSync(inner, withServices(runContext, outerBuilt.value), memo);
      return innerBuilt.kind === "failure"
        ? failure(innerBuilt.cause)
        : success(mergedServices(outerBuilt.value, innerBuilt.value));
    },
  });
}

/** Two built layers' services in one map, the second winning a collision. */
function mergedServices(
  first: $ReadOnlyMap<string, mixed>,
  second: $ReadOnlyMap<string, mixed>,
): $ReadOnlyMap<string, mixed> {
  const merged = new Map(first);
  for (const [key, value] of second) {
    merged.set(key, value);
  }
  return merged;
}

/**
 * Build a layer once, and hand back something many effects can be run against.
 *
 * Effect-TS calls this `ManagedRuntime.make`. The build is one pass with one
 * memo, exactly as `provide`'s is; the difference is entirely in who closes the
 * scope and when. `provide` closes it when its one effect ends, and this does
 * not close it at all — `runtimeDispose` does, whenever the application is
 * over.
 *
 * That makes this the one effect in the package that deliberately leaves
 * something open when it returns, which is why `dispose` is not optional and
 * why it is a named function rather than a finalizer somebody might not have
 * registered. A runtime built and never disposed holds whatever its layers
 * acquired for the life of the process, which for a process-lifetime pool is
 * the point and for anything shorter is a leak.
 *
 * The runtime keeps the services in scope where it was built as well as the
 * ones the layer produced, so a runtime built inside a `provideService` sees
 * both. A build that fails releases whatever the build had already acquired
 * and reports the failure, as `provide`'s does.
 */
export function managedRuntime<Out, E, In>(layer: Layer<Out, E, In>): Effect<Runtime<Out>, E, In> {
  return makeEffect({
    run: async (runContext) => {
      const scope: ScopeState = { finalizers: [] };
      const built = await buildLayer(layer, runtimeBuildContext(runContext, scope), new Map());
      if (built.kind === "failure") {
        await closeScope(scope, runContext);
        return failure(built.cause);
      }
      return success(makeRuntime<Out>(mergedServices(runContext.services, built.value), scope));
    },
    // A layer with a synchronous build gives a synchronous runtime, for the
    // reason `provide`'s own synchronous kernel exists: nothing about a layer
    // is inherently asynchronous, and a test that cannot use `runSync` because
    // it used a layer is a test paying for a limitation that is not there.
    runSync: (runContext) => {
      const scope: ScopeState = { finalizers: [] };
      const built = buildLayerSync(layer, runtimeBuildContext(runContext, scope), new Map());
      if (built.kind === "failure") {
        closeScopeSync(scope, runContext);
        return failure(built.cause);
      }
      return success(makeRuntime<Out>(mergedServices(runContext.services, built.value), scope));
    },
  });
}

/**
 * Run an effect against a built runtime, raising whatever it failed with.
 *
 * `runPromise` with the runtime's services in scope. Each run gets a root fiber
 * of its own, so two effects running against one runtime do not own each other:
 * one of them ending stops what *it* forked and nothing of the other's.
 *
 * The runtime's own scope is not the effect's scope, for the reason `provide`
 * does not hand the body one either — doing so would silently discharge the
 * `Scope` an `acquireRelease` in the body requires while the requirement
 * channel went on saying otherwise.
 */
export function runtimeRunPromise<A, E, R>(self: Runtime<R>, body: Effect<A, E, R>): Promise<A> {
  return runtimeRunPromiseExit(self, body).then((settled) => {
    if (settled.kind === "success") {
      return settled.value;
    }
    throw throwable(settled.cause);
  });
}

/** The same, returning the outcome rather than raising. */
export function runtimeRunPromiseExit<A, E, R>(
  self: Runtime<R>,
  body: Effect<A, E, R>,
): Promise<Exit<A, E>> {
  const closed = disposedExit<A, E, R>(self);
  if (closed != null) {
    return Promise.resolve(closed);
  }
  const runContext = runtimeContext(self);
  return runKernel(body, runContext).then((settled) => {
    endFiber(runContext.fiber);
    return settled;
  });
}

/** Run a synchronous effect against a built runtime, raising its failure. */
export function runtimeRunSync<A, E, R>(self: Runtime<R>, body: Effect<A, E, R>): A {
  const settled = runtimeRunSyncExit(self, body);
  if (settled.kind === "success") {
    return settled.value;
  }
  throw throwable(settled.cause);
}

/** The same, returning the outcome rather than raising. */
export function runtimeRunSyncExit<A, E, R>(self: Runtime<R>, body: Effect<A, E, R>): Exit<A, E> {
  const closed = disposedExit<A, E, R>(self);
  return closed == null ? runSyncKernel(body, runtimeContext(self)) : closed;
}

/** Start an effect against a built runtime and keep a handle on it. */
export function runtimeRunFork<A, E, R>(self: Runtime<R>, body: Effect<A, E, R>): Fiber<A, E> {
  const runContext = runtimeContext(self);
  const closed = disposedExit<A, E, R>(self);
  if (closed != null) {
    return makeFiber(Promise.resolve(closed), runContext.fiber);
  }
  const running = runKernel(body, runContext).then((settled) => {
    endFiber(runContext.fiber);
    return settled;
  });
  return makeFiber(running, runContext.fiber);
}

/**
 * Close what the runtime's layers acquired.
 *
 * Idempotent, so a shutdown path that runs twice is not a second release of a
 * connection pool. Every run against a disposed runtime is a defect rather than
 * a typed failure: the services are gone, so an effect that asks for one is
 * reaching for a resource somebody has already closed, and that is a bug in the
 * program's shutdown order rather than a condition it should be recovering
 * from — the same line `readService` draws for a service that was never
 * provided.
 */
export function runtimeDispose<R>(self: Runtime<R>): Effect<void> {
  return makeEffect({
    run: async (runContext) => {
      if (self.disposed) {
        return success(undefined);
      }
      self.disposed = true;
      const broken = await closeScope(self.scope, runContext);
      return broken == null ? success(undefined) : releaseDefect<void, empty>(broken);
    },
    runSync: (runContext) => {
      if (self.disposed) {
        return success(undefined);
      }
      self.disposed = true;
      const broken = closeScopeSync(self.scope, runContext);
      return broken == null ? success(undefined) : releaseDefect<void, empty>(broken);
    },
  });
}

function makeRuntime<R>(services: $ReadOnlyMap<string, mixed>, scope: ScopeState): Runtime<R> {
  return { __kind: "Runtime", __provides: absurd, services, scope, disposed: false };
}

/** The context a layer is built in: the caller's fiber, and the runtime's scope. */
function runtimeBuildContext(runContext: Context, scope: ScopeState): Context {
  return { services: runContext.services, scope, fiber: runContext.fiber };
}

/** A root context over the runtime's services, with a fiber of its own. */
function runtimeContext<R>(self: Runtime<R>): Context {
  return { services: self.services, scope: null, fiber: newFiber() };
}

/** The answer a disposed runtime gives every run, or `null` if it is still open. */
function disposedExit<A, E, R>(self: Runtime<R>): ?Exit<A, E> {
  return self.disposed ? defect("runtime has been disposed; nothing can be run against it") : null;
}

/**
 * Start `self` beside the current fiber and hand back a handle to it.
 *
 * The child gets its own interruption state, so cancelling it does not cancel
 * the fiber that forked it. The link runs the other way: the child is
 * registered as the caller's, so the caller's interruption reaches it, and the
 * caller ending reaches it too. See *What a fiber owns* in the header — this
 * is where the rule stated there is entered.
 *
 * The handle is the fiber's own promise with the bookkeeping attached in
 * front, so a caller that has `join`ed or `interrupt`ed a child is looking at
 * a tree the child has already left. The promise has no rejection arm because
 * no kernel in this file has one: `runKernel` turns a throw into a defect and
 * every asynchronous kernel below settles its own errors into an `Exit`.
 *
 * This used to be `detachedContext`, which is `forkDaemon` under this name.
 * The bug that made was not that a cancelled child kept running — nobody
 * cancels a fiber they cannot see — but that cancelling a *request* left the
 * work it had started writing to a connection that was already closed, with no
 * handle anywhere that could have stopped it.
 */
export function fork<A, E, R>(self: Effect<A, E, R>): Effect<Fiber<A, E>, empty, R> {
  return makeEffect({
    run: (runContext) => {
      const child = childContext(runContext);
      const running = runKernel(self, child).then((settled) => {
        releaseChild(runContext, child);
        return settled;
      });
      const started: Exit<Fiber<A, E>, empty> = success(makeFiber(running, child.fiber));
      return Promise.resolve(started);
    },
  });
}

/**
 * Start `self` in a fiber that outlives the one that forked it.
 *
 * The escape from the rule `fork` keeps, for work whose lifetime is genuinely
 * not the caller's: a cache warmer, a metrics flush, a supervisor started from
 * a request that has no business owning it. Nothing but the returned handle
 * can stop a daemon, so dropping that handle is dropping the work — which is
 * why this is a name a reader can look up rather than an option on `fork`.
 *
 * A daemon is detached from its parent, not from its own children: it still
 * ends the fibers it started, or the escape would be inherited by everything
 * below it.
 */
export function forkDaemon<A, E, R>(self: Effect<A, E, R>): Effect<Fiber<A, E>, empty, R> {
  return makeEffect({
    run: (runContext) => {
      const child = detachedContext(runContext);
      const running = runKernel(self, child).then((settled) => {
        endFiber(child.fiber);
        return settled;
      });
      const started: Exit<Fiber<A, E>, empty> = success(makeFiber(running, child.fiber));
      return Promise.resolve(started);
    },
  });
}

/**
 * Start `self` beside the current fiber, tied to the enclosing `Scope`.
 *
 * The third lifetime, and the one a request handler usually means. `fork` stops
 * the child when the fiber that forked it returns, which is too early for a
 * background refresh that should finish its own work; `forkDaemon` never stops
 * it, which is a leak with one more step. The lifetime the caller means is
 * neither fiber's — it is the request's, and a request is a `Scope`.
 *
 * The child is registered on the scope and deliberately *not* on the forking
 * fiber's `children`. Being on both would let `endFiber` stop it the moment
 * the handler returned, which is the behaviour this exists to avoid.
 *
 * The child gets a scope of its own, and that is an ordering rather than a
 * detail: finalizers run newest first, so a resource the child acquired after
 * the fork would be released *before* the finalizer that stops the child, out
 * from under a fiber still using it. Its own scope closes when it settles,
 * however it settles.
 *
 * The scope's finalizer interrupts the child and waits for it to stop, so a
 * `scoped` block does not return until what it started has actually finished
 * and released — the same promise `interrupt` makes, made by the scope.
 */
export function forkScoped<A, E, R>(self: Effect<A, E, R>): Effect<Fiber<A, E>, empty, R | Scope> {
  return makeEffect({
    run: (runContext) => {
      const scope = runContext.scope;
      if (scope == null) {
        const missing: Exit<Fiber<A, E>, empty> = defect(
          "forkScoped needs a Scope; wrap the effect in scoped()",
        );
        return Promise.resolve(missing);
      }
      const childScope: ScopeState = { finalizers: [] };
      const child: Context = {
        services: runContext.services,
        scope: childScope,
        fiber: newFiber(),
      };
      const running = runKernel(self, child).then(async (settled) => {
        endFiber(child.fiber);
        const broken = await closeScope(childScope, runContext);
        if (settled.kind === "success" && broken != null) {
          return releaseDefect<A, E>(broken);
        }
        return settled;
      });
      scope.finalizers.push(() =>
        makeEffect({
          run: async () => {
            interruptFiber(child.fiber);
            await running;
            return success(undefined);
          },
        }),
      );
      const started: Exit<Fiber<A, E>, empty> = success(makeFiber(running, child.fiber));
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
 * A place two fibers can both read and write, starting at `initial`.
 *
 * An `Effect` rather than a value, so that making one is part of the program:
 * a `Ref` built at module scope is shared by every run of that program, which
 * is rarely what anybody wants and never what they meant to write.
 *
 * Every operation has a synchronous kernel, so a program that uses a `Ref` can
 * still be answered by `runSync`.
 */
export function ref<A>(initial: A): Effect<Ref<A>> {
  return sync(() => {
    const made: RefCarrier<A> = { __kind: "Ref", value: initial };
    return made;
  });
}

/** What the ref holds now. */
export function refGet<A>(self: Ref<A>): Effect<A> {
  const step = (): Exit<A, empty> => success(self.value);
  return makeEffect({
    run: () => Promise.resolve(step()),
    runSync: step,
  });
}

/** Replace what the ref holds. */
export function refSet<A>(self: Ref<A>, value: A): Effect<void> {
  const step = (): Exit<void, empty> => {
    self.value = value;
    return success(undefined);
  };
  return makeEffect({
    run: () => Promise.resolve(step()),
    runSync: step,
  });
}

/**
 * Read, compute a new value and an answer, and write, without yielding.
 *
 * The one place a `Ref` is read and written, and the reason the rest of these
 * are one line each. It needs no lock: `transform` runs between two property
 * accesses in one step, and nothing in this runtime interleaves fibers except
 * at an `await`. That is also the guarantee's boundary — a transform that
 * returned an `Effect` would yield, and serialising *that* is what a semaphore
 * is for. See `refUpdateEffect`.
 *
 * A transform that throws leaves the ref alone and becomes a defect, because
 * half of a read-modify-write is worse than none of it.
 */
export function refModify<A, B>(self: Ref<A>, transform: (value: A) => [B, A]): Effect<B> {
  const step = (): Exit<B, empty> => {
    try {
      const [answer, next] = transform(self.value);
      self.value = next;
      return success(answer);
    } catch (error) {
      return defect(error);
    }
  };
  return makeEffect({
    run: () => Promise.resolve(step()),
    runSync: step,
  });
}

/** Apply a function to what the ref holds. */
export function refUpdate<A>(self: Ref<A>, transform: (value: A) => A): Effect<void> {
  return refModify(self, (value) => [undefined, transform(value)]);
}

/** Apply a function, and give back what it produced. */
export function refUpdateAndGet<A>(self: Ref<A>, transform: (value: A) => A): Effect<A> {
  return refModify(self, (value) => {
    const next = transform(value);
    return [next, next];
  });
}

/** Apply a function, and give back what was there before it. */
export function refGetAndUpdate<A>(self: Ref<A>, transform: (value: A) => A): Effect<A> {
  return refModify(self, (value) => [value, transform(value)]);
}

/** Replace what the ref holds, and give back what was there before. */
export function refGetAndSet<A>(self: Ref<A>, value: A): Effect<A> {
  return refModify(self, (current) => [current, value]);
}

/**
 * Update a ref with an effect, one fiber at a time.
 *
 * This is Effect's `SynchronizedRef`, and it is a `Ref` and a `Semaphore` held
 * together rather than a third opaque type. Passing the lock in is what makes
 * the serialisation visible at the call site, and it lets two refs that must
 * move together share one — which a bundled lock could not express.
 *
 * `refModify` needs no lock because it cannot yield. This can, so it must
 * have one: without it, two fibers read the same value, both compute from it,
 * and the second write silently discards the first.
 */
export function refUpdateEffect<A, E, R>(
  self: Ref<A>,
  lock: Semaphore,
  transform: (value: A) => Effect<A, E, R>,
): Effect<A, E, R> {
  // Written with `flatMap` rather than the generator form: the runtime does
  // not otherwise use its own `effect`, and a combinator that did would be the
  // one place where a bug in the driver could not be debugged with the driver.
  return withPermits(
    lock,
    1,
    flatMap(refGet(self), (current) =>
      flatMap(transform(current), (next) => as(refSet(self, next), next)),
    ),
  );
}

/**
 * A value that has not been produced yet, and can be waited for.
 *
 * Completed at most once: the first `deferredSucceed` or `deferredFail` wins
 * and says so by returning `true`, and every later one returns `false` rather
 * than overwriting an answer somebody may already have acted on.
 */
export function deferred<A, E = empty>(): Effect<Deferred<A, E>> {
  return sync(() => {
    const made: DeferredCarrier<A, E> = {
      __kind: "Deferred",
      state: { settled: null, waiters: new Set() },
    };
    return made;
  });
}

/**
 * Wait for the value, and take its outcome as this effect's outcome.
 *
 * Interruptible, by the protocol `pause` uses for a `sleep`: a waker goes on
 * the fiber's list, and cancelling the fiber ends the wait now. Getting this
 * wrong is how a handshake becomes a fiber `interrupt` cannot stop, which is
 * the whole reason a hand-written `Promise` and a `let` are not good enough
 * for this.
 *
 * No synchronous kernel: waiting for a value nobody has produced is what this
 * is, and an effect that pretended otherwise would have to answer for a value
 * that does not exist. `deferredIsDone` is the question with a synchronous
 * answer.
 */
export function deferredAwait<A, E>(self: Deferred<A, E>): Effect<A, E> {
  return makeEffect({
    run: (runContext) =>
      new Promise((resolve) => {
        const state = self.state;
        const already = state.settled;
        if (already != null) {
          resolve(already);
          return;
        }
        if (isInterrupted(runContext)) {
          resolve(interruptedExit());
          return;
        }
        const finish = (outcome: Exit<A, E>) => {
          state.waiters.delete(deliver);
          runContext.fiber.wakers.delete(wake);
          resolve(outcome);
        };
        const deliver = (outcome: Exit<A, E>) => finish(outcome);
        const wake = () => finish(interruptedExit());
        state.waiters.add(deliver);
        runContext.fiber.wakers.add(wake);
      }),
  });
}

/** Complete it with a value. `true` if this call was the one that did. */
export function deferredSucceed<A, E>(self: Deferred<A, E>, value: A): Effect<boolean> {
  const settled: Exit<A, E> = success(value);
  const step = (): Exit<boolean, empty> => success(completeDeferred(self.state, settled));
  return makeEffect({
    run: () => Promise.resolve(step()),
    runSync: step,
  });
}

/** Complete it with a typed failure. `true` if this call was the one that did. */
export function deferredFail<A, E>(self: Deferred<A, E>, error: E): Effect<boolean> {
  const settled: Exit<A, E> = failure(failCause(error));
  const step = (): Exit<boolean, empty> => success(completeDeferred(self.state, settled));
  return makeEffect({
    run: () => Promise.resolve(step()),
    runSync: step,
  });
}

/** Whether it has been completed, without waiting to find out. */
export function deferredIsDone<A, E>(self: Deferred<A, E>): Effect<boolean> {
  const step = (): Exit<boolean, empty> => success(self.state.settled != null);
  return makeEffect({
    run: () => Promise.resolve(step()),
    runSync: step,
  });
}

/**
 * Settle a deferred and hand the outcome to everyone waiting.
 *
 * Deleting from the set inside the loop is safe for the reason `interruptFiber`
 * gives: a `Set` iteration tolerates removal of entries it has reached, and a
 * waiter only resolves a promise, which cannot add another before this
 * returns. The `clear` afterwards is for waiters that were added and never
 * reached, which cannot happen today and costs one call to keep true.
 */
function completeDeferred<A, E>(state: DeferredState<A, E>, outcome: Exit<A, E>): boolean {
  if (state.settled != null) {
    return false;
  }
  state.settled = outcome;
  for (const waiter of state.waiters) {
    waiter(outcome);
  }
  state.waiters.clear();
  return true;
}

/**
 * A budget of permits, shared by whoever holds this.
 *
 * `permits` is the capacity and the starting count. Asking for more than the
 * capacity later is a defect rather than a wait, because nothing will ever
 * release enough and a permanent hang is the least debuggable way to say so.
 */
export function semaphore(permits: number): Effect<Semaphore> {
  return sync(() => {
    const capacity = Math.max(0, Math.floor(permits));
    const made: SemaphoreCarrier = {
      __kind: "Semaphore",
      state: { available: capacity, capacity, waiters: [] },
    };
    return made;
  });
}

/** Run `body` holding one permit. */
export function withPermit<A, E, R>(self: Semaphore, body: Effect<A, E, R>): Effect<A, E, R> {
  return withPermits(self, 1, body);
}

/**
 * Run `body` holding `permits` of them, and give them back however it ends.
 *
 * The same guarantee `ensuring` gives, and for the same reason: a permit that
 * is not returned when the fiber holding it is interrupted is a budget that
 * shrinks every time somebody cancels a request, until nothing can run at all.
 * `finally` rather than a finalizer effect, because releasing is a counter and
 * an array splice — it cannot fail, and it must not be interruptible.
 *
 * A fiber interrupted while *queued* never took a permit, so it returns
 * without releasing one it does not hold.
 */
export function withPermits<A, E, R>(
  self: Semaphore,
  permits: number,
  body: Effect<A, E, R>,
): Effect<A, E, R> {
  return makeEffect({
    run: async (runContext) => {
      const wanted = Math.max(0, Math.floor(permits));
      if (wanted > self.state.capacity) {
        return defect(
          `withPermits asked for ${wanted} permits of a semaphore that has ${self.state.capacity}`,
        );
      }
      const taken = await acquirePermits(self.state, wanted, runContext);
      if (!taken) {
        return interruptedExit();
      }
      try {
        return await runKernel(body, runContext);
      } finally {
        releasePermits(self.state, wanted);
      }
    },
  });
}

/**
 * Take `wanted` permits, or wait for them. `false` means interrupted instead.
 *
 * A fiber that could be served immediately still queues when anybody is ahead
 * of it, which is what keeps the order the one people asked in.
 */
function acquirePermits(
  state: SemaphoreState,
  wanted: number,
  runContext: Context,
): Promise<boolean> {
  return new Promise((resolve) => {
    if (isInterrupted(runContext)) {
      resolve(false);
      return;
    }
    if (state.waiters.length === 0 && state.available >= wanted) {
      state.available -= wanted;
      resolve(true);
      return;
    }
    const waiter: SemaphoreWaiter = {
      permits: wanted,
      settle: (taken: boolean) => {
        const queued = state.waiters.indexOf(waiter);
        if (queued >= 0) {
          state.waiters.splice(queued, 1);
        }
        runContext.fiber.wakers.delete(wake);
        resolve(taken);
      },
    };
    const wake = () => waiter.settle(false);
    state.waiters.push(waiter);
    runContext.fiber.wakers.add(wake);
  });
}

/**
 * Give permits back, and serve whoever the queue owes them to.
 *
 * Strictly from the head. Serving a later small request that happens to fit
 * would raise throughput and starve a large one for ever, and a bound that the
 * widest caller cannot rely on is not a bound.
 */
function releasePermits(state: SemaphoreState, permits: number): void {
  state.available += permits;
  while (state.waiters.length > 0 && state.available >= state.waiters[0].permits) {
    const next = state.waiters[0];
    state.available -= next.permits;
    next.settle(true);
  }
}

/**
 * A place to hand values from one fiber to another, `capacity` deep.
 *
 * `capacity` is at least one: a queue of nothing has no buffer for a `sliding`
 * strategy to slide and no room for a `dropping` one to drop into, so the two
 * would silently mean "hand over directly or lose it", which is a rendezvous
 * and not a queue. A `Deferred` is the rendezvous.
 *
 * `strategy` defaults to `bounded`, which is the one that is back pressure:
 * the default should be the answer that cannot lose a value.
 */
export function queue<A>(capacity: number, strategy?: QueueStrategy): Effect<Queue<A>> {
  return sync(() => {
    const made: QueueCarrier<A> = {
      __kind: "Queue",
      state: {
        capacity: Math.max(1, Math.floor(capacity)),
        strategy: strategy == null ? "bounded" : strategy,
        items: [],
        takers: [],
        offerers: [],
        shutdown: false,
      },
    };
    return made;
  });
}

/**
 * Put a value in, waiting for room if the queue is `bounded` and full.
 *
 * `true` when the queue took the value, `false` when a `dropping` queue that
 * was full threw it away. A `sliding` queue always answers `true`, because it
 * took the value — what it lost was an older one.
 *
 * A fiber waiting for room can be interrupted out of the wait, by the protocol
 * `deferredAwait` and `acquirePermits` use, and a fiber interrupted while
 * waiting never enqueues the value it was holding: it was never in the queue,
 * and putting it there on the way out would deliver work from a request that
 * had already been cancelled.
 */
export function queueOffer<A>(self: Queue<A>, value: A): Effect<boolean> {
  const state = self.state;
  const answer = (offered: QueueOffered): Exit<boolean, empty> =>
    offered === "stopped" ? interruptedExit() : success(offered === "accepted");
  return makeEffect({
    run: (runContext) => offerToQueue(state, value, runContext).then(answer),
    // An offer that does not have to wait is a push onto an array, and there
    // is no reason a synchronous program should lose `runSync` for making one.
    // An offer that *would* wait ends the run the way any other asynchronous
    // effect does, and under `runSync` that is not a race it might have won:
    // nothing else is running, so nothing will ever drain the queue.
    runSync: (runContext) => {
      const offered = offerNow(state, value, runContext);
      return offered === "full"
        ? failure(dieCause("queueOffer would wait for room; a bounded queue is asynchronous"))
        : answer(offered);
    },
  });
}

/**
 * Take the next value, waiting for one if there is none.
 *
 * Interruptible while waiting, and a taker woken by an interruption leaves the
 * queue exactly as it found it: it is removed from the queue of takers before
 * anything can hand it a value, so the value it did not receive is still there
 * for the next one. Getting that wrong is how a cancelled request eats a piece
 * of work that nobody then does.
 *
 * A taker that has already been handed a value keeps it even if its fiber is
 * interrupted a moment later. The alternative is dropping a value that has
 * left the queue, and interruption is checked at the caller's next step
 * anyway — a cancelled fiber stops there rather than one step earlier, and the
 * work does not vanish in between.
 *
 * Unlike `deferredAwait` this has a synchronous kernel, and the difference is
 * real rather than an inconsistency: taking from a queue that has something in
 * it is reading a buffer, not waiting for a value nobody has produced.
 */
export function queueTake<A>(self: Queue<A>): Effect<A> {
  const state = self.state;
  const answer = (taken: QueueTaken<A>): Exit<A, empty> =>
    taken.kind === "taken" ? success(taken.value) : interruptedExit();
  return makeEffect({
    run: (runContext) => takeFromQueue(state, runContext).then(answer),
    runSync: (runContext) => {
      const taken = takeNow(state, runContext);
      return taken.kind === "empty"
        ? failure(dieCause("queueTake would wait for a value; an empty queue is asynchronous"))
        : answer(taken);
    },
  });
}

/** Everything waiting, without waiting. An empty queue answers with `[]`. */
export function queueTakeAll<A>(self: Queue<A>): Effect<$ReadOnlyArray<A>> {
  return takeManyEffect(self.state, Number.POSITIVE_INFINITY);
}

/** At most `max` of what is waiting, without waiting for more. */
export function queueTakeUpTo<A>(self: Queue<A>, max: number): Effect<$ReadOnlyArray<A>> {
  return takeManyEffect(self.state, Math.max(0, Math.floor(max)));
}

/**
 * How many values are waiting to be taken.
 *
 * The buffer's length, and not Effect's signed count. A fiber blocked in
 * `queueOffer` is holding a value that is not in the queue, and a fiber blocked
 * in `queueTake` is not a negative value; a number that means three different
 * things depending on its sign is a worse answer than one that means the one
 * thing its name says.
 */
export function queueSize<A>(self: Queue<A>): Effect<number> {
  const state = self.state;
  const step = (): Exit<number, empty> =>
    state.shutdown ? interruptedExit() : success(state.items.length);
  return makeEffect({
    run: () => Promise.resolve(step()),
    runSync: step,
  });
}

/**
 * Stop the queue, and stop everybody waiting on it.
 *
 * Waiting takers and waiting offerers are all interrupted, and what was in the
 * buffer is dropped: a shutdown is a decision that the work is over, and
 * handing out three more values on the way down would be that decision half
 * taken.
 *
 * Idempotent, and the one operation a shut-down queue still answers normally —
 * along with `queueIsShutdown`, which has to stay answerable for anything to be
 * able to tell a shutdown from an interruption of its own fiber.
 */
export function queueShutdown<A>(self: Queue<A>): Effect<void> {
  const state = self.state;
  const step = (): Exit<void, empty> => {
    shutdownQueue(state);
    return success(undefined);
  };
  return makeEffect({
    run: () => Promise.resolve(step()),
    runSync: step,
  });
}

/** Whether the queue has been shut down, which a shut-down queue still answers. */
export function queueIsShutdown<A>(self: Queue<A>): Effect<boolean> {
  const state = self.state;
  const step = (): Exit<boolean, empty> => success(state.shutdown);
  return makeEffect({
    run: () => Promise.resolve(step()),
    runSync: step,
  });
}

/**
 * `queueTakeAll` and `queueTakeUpTo`, which differ only in how many.
 *
 * Neither of them ever waits, and both still refuse to run in an interrupted
 * fiber — unlike `refGet`, which also never waits. The difference is that these
 * *remove* what they read: a cancelled fiber that drained a queue on its way
 * out would take the work with it, which is the same loss `queueTake` avoids by
 * leaving a value for the next taker.
 */
function takeManyEffect<A>(state: QueueState<A>, wanted: number): Effect<$ReadOnlyArray<A>> {
  const step = (runContext: Context): Exit<$ReadOnlyArray<A>, empty> =>
    state.shutdown || isInterrupted(runContext)
      ? interruptedExit()
      : success(drainQueue(state, wanted));
  return makeEffect({
    run: (runContext) => Promise.resolve(step(runContext)),
    runSync: step,
  });
}

/**
 * Put a value down without waiting, and say what became of it.
 *
 * `full` is the one answer that is not an answer: it means only a `bounded`
 * queue's back pressure is left, which is the caller's to wait on.
 */
function offerNow<A>(state: QueueState<A>, value: A, runContext: Context): QueueOffered {
  if (state.shutdown || isInterrupted(runContext)) {
    return "stopped";
  }
  const taker = state.takers[0];
  if (taker != null) {
    // Handed straight over. The invariant says the buffer is empty whenever
    // anybody is waiting for one, so there is nothing this could jump ahead of.
    taker.settle({ kind: "taken", value });
    return "accepted";
  }
  if (state.items.length < state.capacity) {
    state.items.push(value);
    return "accepted";
  }
  switch (state.strategy) {
    case "dropping":
      return "dropped";
    case "sliding":
      state.items.shift();
      state.items.push(value);
      return "accepted";
    default:
      return "full";
  }
}

/** `offerNow`, and then the wait a `bounded` queue asks for. */
function offerToQueue<A>(
  state: QueueState<A>,
  value: A,
  runContext: Context,
): Promise<QueueOffered> {
  return new Promise((resolve) => {
    const immediate = offerNow(state, value, runContext);
    if (immediate !== "full") {
      resolve(immediate);
      return;
    }
    const offerer: QueueOfferer<A> = {
      value,
      settle: (accepted: boolean) => {
        const queued = state.offerers.indexOf(offerer);
        if (queued >= 0) {
          state.offerers.splice(queued, 1);
        }
        runContext.fiber.wakers.delete(wake);
        resolve(accepted ? "accepted" : "stopped");
      },
    };
    const wake = () => offerer.settle(false);
    state.offerers.push(offerer);
    runContext.fiber.wakers.add(wake);
  });
}

/** Take the head without waiting, or say that waiting is what is left. */
function takeNow<A>(state: QueueState<A>, runContext: Context): QueueTaken<A> {
  if (state.shutdown || isInterrupted(runContext)) {
    return { kind: "stopped" };
  }
  if (state.takers.length > 0 || state.items.length === 0) {
    return { kind: "empty" };
  }
  return { kind: "taken", value: takeOne(state) };
}

/** `takeNow`, and then the wait an empty queue asks for. */
function takeFromQueue<A>(state: QueueState<A>, runContext: Context): Promise<QueueTaken<A>> {
  return new Promise((resolve) => {
    const immediate = takeNow(state, runContext);
    if (immediate.kind !== "empty") {
      resolve(immediate);
      return;
    }
    const taker: QueueTaker<A> = {
      settle: (taken: QueueTaken<A>) => {
        const queued = state.takers.indexOf(taker);
        if (queued >= 0) {
          state.takers.splice(queued, 1);
        }
        runContext.fiber.wakers.delete(wake);
        resolve(taken);
      },
    };
    const wake = () => taker.settle({ kind: "stopped" });
    state.takers.push(taker);
    runContext.fiber.wakers.add(wake);
  });
}

/** Remove the head, and let whoever was waiting for room put a value down. */
function takeOne<A>(state: QueueState<A>): A {
  const value = state.items.shift();
  admitOfferers(state);
  return value;
}

/** The head of the buffer, up to `wanted` of it, and then the same admission. */
function drainQueue<A>(state: QueueState<A>, wanted: number): $ReadOnlyArray<A> {
  const drained = state.items.splice(0, Math.min(wanted, state.items.length));
  admitOfferers(state);
  return drained;
}

/**
 * Move waiting offerers' values into the room that has just appeared.
 *
 * From the head, and only as far as the capacity allows, so a producer that has
 * been waiting since before a faster one arrived goes first. `settle` removes
 * the offerer it is called on, which is what makes the loop terminate.
 */
function admitOfferers<A>(state: QueueState<A>): void {
  while (state.offerers.length > 0 && state.items.length < state.capacity) {
    const offerer = state.offerers[0];
    state.items.push(offerer.value);
    offerer.settle(true);
  }
}

/** Shut a queue down and stop everybody waiting at either end of it. */
function shutdownQueue<A>(state: QueueState<A>): void {
  if (state.shutdown) {
    return;
  }
  state.shutdown = true;
  state.items.length = 0;
  while (state.takers.length > 0) {
    state.takers[0].settle({ kind: "stopped" });
  }
  while (state.offerers.length > 0) {
    state.offerers[0].settle(false);
  }
}

/**
 * A place to publish values every subscriber sees, `capacity` deep each.
 *
 * The capacity and the strategy describe one *subscriber's* buffer, because
 * that is where the choice bites: with `bounded`, the slowest subscriber is
 * what a publisher waits for, and with `sliding` a subscriber that falls behind
 * loses its oldest values rather than holding the publisher up.
 */
export function pubSub<A>(capacity: number, strategy?: QueueStrategy): Effect<PubSub<A>> {
  return sync(() => {
    const made: PubSubCarrier<A> = {
      __kind: "PubSub",
      state: {
        capacity: Math.max(1, Math.floor(capacity)),
        strategy: strategy == null ? "bounded" : strategy,
        subscribers: new Set(),
        shutdown: false,
      },
    };
    return made;
  });
}

/**
 * Listen, until the enclosing scope closes.
 *
 * The `Scope` is not decoration: a subscription that nothing unsubscribes is a
 * buffer that a publisher keeps filling and nobody keeps draining, which with
 * `bounded` stops the publisher for ever and with the other two is a leak. So
 * the lifetime is stated in the type, in the same shape `acquireRelease` states
 * it, and `scoped` discharges both.
 *
 * A subscriber sees what is published after it subscribes and nothing that came
 * before, because the buffer it is handed is its own and starts empty.
 */
export function pubSubSubscribe<A>(self: PubSub<A>): Effect<Queue<A>, empty, Scope> {
  const state = self.state;
  const step = (runContext: Context): Exit<Queue<A>, empty> => {
    const scope = runContext.scope;
    if (scope == null) {
      return defect("pubSubSubscribe needs a Scope; wrap the effect in scoped()");
    }
    if (state.shutdown) {
      return interruptedExit();
    }
    const subscription: QueueState<A> = {
      capacity: state.capacity,
      strategy: state.strategy,
      items: [],
      takers: [],
      offerers: [],
      shutdown: false,
    };
    state.subscribers.add(subscription);
    scope.finalizers.push(() =>
      sync(() => {
        state.subscribers.delete(subscription);
        shutdownQueue(subscription);
      }),
    );
    const made: QueueCarrier<A> = { __kind: "Queue", state: subscription };
    return success(made);
  };
  return makeEffect({
    run: (runContext) => Promise.resolve(step(runContext)),
    runSync: step,
  });
}

/**
 * Hand a value to every subscriber, waiting for the slowest one that has back
 * pressure.
 *
 * `true` when every subscriber took it, and `false` when one of them did not —
 * a `dropping` subscriber that was full, or one whose scope closed while the
 * publish was waiting for it. A pub-sub with no subscribers answers `true`,
 * because nobody failed to take it.
 *
 * No synchronous kernel, unlike `queueOffer`. A publish to a `bounded`
 * subscriber waits by design, and one that reported "would wait" for the fast
 * subscribers and not the slow one would be an answer about which subscribers
 * happened to be behind.
 */
export function pubSubPublish<A>(self: PubSub<A>, value: A): Effect<boolean> {
  const state = self.state;
  return makeEffect({
    run: async (runContext) => {
      if (state.shutdown || isInterrupted(runContext)) {
        return interruptedExit();
      }
      const delivered = await Promise.all(
        Array.from(state.subscribers, (subscription) =>
          offerToQueue(subscription, value, runContext),
        ),
      );
      // A `stopped` answer is either this fiber being cancelled — in which case
      // the publish is cancelled — or one subscriber having unsubscribed while
      // the offer waited, which is that subscriber missing a value and not the
      // publisher's failure.
      if (isInterrupted(runContext)) {
        return interruptedExit();
      }
      return success(delivered.every((outcome) => outcome === "accepted"));
    },
  });
}

/**
 * Stop the pub-sub, and every subscription with it.
 *
 * Every subscriber is interrupted where it waits, by the rule a shut-down queue
 * already has. A subscription's scope still runs its own finalizer afterwards,
 * which finds the queue already down and says so by doing nothing.
 */
export function pubSubShutdown<A>(self: PubSub<A>): Effect<void> {
  const state = self.state;
  const step = (): Exit<void, empty> => {
    state.shutdown = true;
    for (const subscription of state.subscribers) {
      shutdownQueue(subscription);
    }
    state.subscribers.clear();
    return success(undefined);
  };
  return makeEffect({
    run: () => Promise.resolve(step()),
    runSync: step,
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

/**
 * Run an effect, raising whatever it failed with.
 *
 * The root fiber ends when this returns, which is what stops a program that
 * forked and did not wait from leaving the fork behind. `forkDaemon` is how a
 * caller says the work should outlive the run.
 */
export async function runPromise<A, E>(self: Effect<A, E>): Promise<A> {
  const runContext = context();
  const settled = await runKernel(self, runContext);
  endFiber(runContext.fiber);
  if (settled.kind === "success") {
    return settled.value;
  }
  throw throwable(settled.cause);
}

/**
 * Run an effect, returning its outcome rather than raising.
 *
 * `.then` rather than `async`, because an extra async frame costs a microtask
 * on a function whose whole body is one call, and the root fiber has to be
 * ended after the run either way.
 */
export function runPromiseExit<A, E>(self: Effect<A, E>): Promise<Exit<A, E>> {
  const runContext = context();
  return runKernel(self, runContext).then((settled) => {
    endFiber(runContext.fiber);
    return settled;
  });
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

/**
 * Start an effect from outside the runtime and keep a handle on it.
 *
 * The handle is a root fiber, so it owns what it forks in the same way any
 * other fiber does: when it settles, the children it still has are stopped.
 */
export function runFork<A, E>(self: Effect<A, E>): Fiber<A, E> {
  const runContext = context();
  const running = runKernel(self, runContext).then((settled) => {
    endFiber(runContext.fiber);
    return settled;
  });
  return makeFiber(running, runContext.fiber);
}
