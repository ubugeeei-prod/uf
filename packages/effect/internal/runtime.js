// @flow
//
// `@uniflowed/effect`.
//
// A small, typed Effect surface written entirely in `.js` with Flow. The Rust
// toolchain may lint, format, test, and bundle it, but user code does not need a
// native binding to construct or run these effects on Node.js, Deno, or Bun.

type EffectKernel = {
  +run: (Context) => Promise<Exit<mixed, mixed>>,
  +runSync?: (Context) => Exit<mixed, mixed>,
};

type FiberCarrier<+A, +E> = {
  +__kind: "Fiber",
  +__value: () => A,
  +__error: () => E,
  +__promise: Promise<Exit<mixed, mixed>>,
  +__fiber: FiberState,
};

type EffectCarrier<+A, +E, +R> = {
  +__kind: "Effect",
  +__value: () => A,
  +__error: () => E,
  +__requires: () => R,
  +__kernel: EffectKernel,
};

type TagCarrier<+Service> = {
  +__kind: "Tag",
  +__service: () => Service,
  +identifier: string,
};

type LayerKernel = (Context) => Promise<Exit<{ [string]: mixed }, mixed>>;

type LayerCarrier<+Out, +E, +In> = {
  +__kind: "Layer",
  +__out: () => Out,
  +__error: () => E,
  +__in: () => In,
  +__layer: LayerKernel,
};

type ScopeState = {
  finalizers: Array<() => Effect<void, mixed, empty>>,
};

/**
 * The interruption state one fiber shares with everything running inside it.
 *
 * `interrupted` is the flag combinators check between steps; `wakers` is how a
 * pending `sleep` finds out, because a timer that has already been scheduled
 * cannot be talked out of firing and a fiber that waited for it would stay
 * alive for the full delay after being cancelled.
 */
type FiberState = {
  interrupted: boolean,
  wakers: Set<() => void>,
};

type Context = {
  services: { [string]: mixed },
  scope: ?ScopeState,
  fiber: FiberState,
};

/**
 * Work that produces an `A`, may fail with an `E`, and needs an `R`.
 *
 * `E` defaults to `empty`, Flow's bottom type, so `Effect<number>` reads as
 * cannot fail. `R` defaults to `empty` for an effect that needs no services.
 */
export opaque type Effect<+A, +E = empty, +R = empty> = EffectCarrier<A, E, R>;

/** A running effect, addressable so it can be awaited. */
export opaque type Fiber<+A, +E = empty> = FiberCarrier<A, E>;

/** Identifies one service inside a context. */
export opaque type Tag<+Service> = TagCarrier<Service>;

/** A recipe for building services, itself possibly failing. */
export opaque type Layer<+Out, +E = empty, +In = empty> = LayerCarrier<Out, E, In>;

/** The lifetime a resource is released at. */
export opaque type Scope = { +__kind: "Scope" };

export type Cause<+E> =
  | { +kind: "empty" }
  | { +kind: "fail", +error: E }
  | { +kind: "die", +defect: string }
  | { +kind: "interrupt" }
  | { +kind: "sequential", +causes: $ReadOnlyArray<Cause<E>> }
  | { +kind: "parallel", +causes: $ReadOnlyArray<Cause<E>> };

export type Exit<+A, +E> =
  | { +kind: "success", +value: A }
  | { +kind: "failure", +cause: Cause<E> };

export type Schedule =
  | { +kind: "recurs", +times: number }
  | { +kind: "spaced", +millis: number }
  | { +kind: "exponential", +baseMillis: number, +factorPercent?: number }
  | { +kind: "fibonacci", +baseMillis: number }
  | { +kind: "upTo", +millis: number }
  | { +kind: "intersect", +left: Schedule, +right: Schedule }
  | { +kind: "union", +left: Schedule, +right: Schedule }
  | { +kind: "maxDelay", +schedule: Schedule, +millis: number };

export type Concurrency = number | "unbounded" | "inherit";

export type EffectGenerator<A, E, R> = Generator<Effect<mixed, E, R>, A, mixed>;

function absurd(): empty {
  throw Error("@uniflowed/effect phantom value");
}

function fiberState(): FiberState {
  return { interrupted: false, wakers: new Set() };
}

function context(): Context {
  return { services: {}, scope: null, fiber: fiberState() };
}

function withScope(parent: Context): Context {
  return { services: parent.services, scope: { finalizers: [] }, fiber: parent.fiber };
}

function withService<Service>(
  parent: Context,
  serviceTag: Tag<Service>,
  service: Service,
): Context {
  return {
    services: { ...parent.services, [readTag(serviceTag)]: service },
    scope: parent.scope,
    fiber: parent.fiber,
  };
}

/**
 * A context whose interruption is independent of its parent's.
 *
 * Forking is the only place this happens. A child that shared its parent's
 * flag could not be cancelled on its own, and a fork whose whole purpose is
 * "run this beside me and let me stop it" would be unable to stop anything.
 */
function withFiber(parent: Context): Context {
  return { services: parent.services, scope: parent.scope, fiber: fiberState() };
}

/** Whether the fiber this context belongs to has been interrupted. */
function isInterrupted(runContext: Context): boolean {
  return runContext.fiber.interrupted;
}

function interruptedExit<A, E>(): Exit<A, E> {
  return failure({ kind: "interrupt" });
}

function makeEffect<A, E, R>(kernel: EffectKernel): Effect<A, E, R> {
  return ({
    __kind: "Effect",
    __value: absurd,
    __error: absurd,
    __requires: absurd,
    __kernel: kernel,
  }: any);
}

function readKernel<A, E, R>(self: Effect<A, E, R>): EffectKernel {
  return (self: any).__kernel;
}

function makeFiber<A, E>(promise: Promise<Exit<mixed, mixed>>, fiber: FiberState): Fiber<A, E> {
  return ({
    __kind: "Fiber",
    __value: absurd,
    __error: absurd,
    __promise: promise,
    __fiber: fiber,
  }: any);
}

function readFiber<A, E>(fiber: Fiber<A, E>): FiberCarrier<A, E> {
  return (fiber: any);
}

/**
 * A tag, which is also the effect that reads the service it names.
 *
 * Both, on purpose: `provideService` and `provide` put a service into the run
 * context, and something has to take it out again. Giving the tag an effect
 * kernel means the way to ask for a service is to use the tag as one —
 * `yield Clock` inside `effect`, or `flatMap(Clock, …)` — so there is no
 * second accessor function to learn, and a tag can be handed to any combinator
 * that takes an effect.
 *
 * Reading a service that was never provided is a defect rather than a typed
 * failure. Flow's `R` parameter already tracks which services an effect
 * requires, so reaching this at run time means the type was bypassed, and that
 * is a bug in the program rather than a condition it should be recovering
 * from.
 */
function makeTag<Service>(identifier: string): Tag<Service> {
  const kernel: EffectKernel = {
    run: async (runContext) => readService(runContext, identifier),
    runSync: (runContext) => readService(runContext, identifier),
  };
  return ({
    __kind: "Tag",
    __service: absurd,
    identifier,
    // Being an Effect as well as a Tag: `readKernel` finds this.
    __kernel: kernel,
    __value: absurd,
    __error: absurd,
    __requires: absurd,
  }: any);
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
    case "die":
    case "interrupt":
    case "empty":
      return false;
    case "sequential":
    case "parallel":
      return cause.causes.some(isRetriable);
    default:
      return false;
  }
}

function readService(runContext: Context, identifier: string): Exit<mixed, mixed> {
  if (!Object.prototype.hasOwnProperty.call(runContext.services, identifier)) {
    return defect(`service ${identifier} was not provided`);
  }
  return success(runContext.services[identifier]);
}

function readTag<Service>(serviceTag: Tag<Service>): string {
  return (serviceTag: any).identifier;
}

function makeLayer<Out, E, In>(kernel: LayerKernel): Layer<Out, E, In> {
  return ({
    __kind: "Layer",
    __out: absurd,
    __error: absurd,
    __in: absurd,
    __layer: kernel,
  }: any);
}

function readLayer<Out, E, In>(layer: Layer<Out, E, In>): LayerKernel {
  return (layer: any).__layer;
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

async function runKernel<A, E, R>(
  self: Effect<A, E, R>,
  runContext: Context,
): Promise<Exit<A, E>> {
  return ((await readKernel(self).run(runContext)): any);
}

function runSyncKernel<A, E, R>(
  self: Effect<A, E, R>,
  runContext: Context,
): Exit<A, E> {
  const runSync = readKernel(self).runSync;
  if (runSync == null) {
    return failure(dieCause("effect is asynchronous"));
  }
  return (runSync(runContext): any);
}

function isPromiseLike(value: mixed): boolean {
  return (
    value != null &&
    (typeof value === "object" || typeof value === "function") &&
    typeof (value: any).then === "function"
  );
}

function mapCause<E, F>(cause: Cause<E>, transform: (E) => F): Cause<F> {
  return match (cause) {
    {kind: "fail", error: const error} => failCause(transform(error)),
    {kind: "sequential", causes: const causes} => ({
      kind: "sequential",
      causes: causes.map((entry) => mapCause(entry, transform)),
    }),
    {kind: "parallel", causes: const causes} => ({
      kind: "parallel",
      causes: causes.map((entry) => mapCause(entry, transform)),
    }),
    {kind: "die", defect: const defect} => ({ kind: "die", defect }),
    {kind: "interrupt"} => ({ kind: "interrupt" }),
    _ => ({ kind: "empty" }),
  };
}

function firstFailure<E>(cause: Cause<E>): ?E {
  return match (cause) {
    {kind: "fail", error: const error} => error,
    {kind: "sequential", causes: const causes} => firstCauseFailure(causes),
    {kind: "parallel", causes: const causes} => firstCauseFailure(causes),
    _ => null,
  };
}

function firstCauseFailure<E>(causes: $ReadOnlyArray<Cause<E>>): ?E {
  for (const entry of causes) {
    const error = firstFailure(entry);
    if (error != null) {
      return error;
    }
  }
  return null;
}

function causeMessage<E>(cause: Cause<E>): string {
  return match (cause) {
    {kind: "fail", error: const error} => String(error),
    {kind: "die", defect: const defect} => defect,
    {kind: "interrupt"} => "effect interrupted",
    {kind: "sequential", causes: const causes} => causes.map(causeMessage).join("; "),
    {kind: "parallel", causes: const causes} => causes.map(causeMessage).join("; "),
    _ => "empty effect failure",
  };
}

function throwable<E>(cause: Cause<E>): mixed {
  const error = firstFailure(cause);
  return error == null ? Error(causeMessage(cause)) : error;
}

/**
 * Wait, and stop waiting early if the fiber is interrupted.
 *
 * A bare `setTimeout` cannot be talked out of firing, so a cancelled fiber
 * sleeping for a minute would keep the process alive for a minute after
 * nobody wanted its answer. Registering a waker is what makes cancellation
 * take effect now rather than at the end of the delay.
 */
function pause(millis: number, runContext?: Context): Promise<void> {
  return new Promise((resolve) => {
    const fiber = runContext == null ? null : runContext.fiber;
    let timer = null;
    const finish = () => {
      if (timer != null) {
        clearTimeout(timer);
        timer = null;
      }
      if (fiber != null) {
        fiber.wakers.delete(finish);
      }
      resolve();
    };
    if (fiber != null) {
      if (fiber.interrupted) {
        resolve();
        return;
      }
      fiber.wakers.add(finish);
    }
    timer = setTimeout(finish, Math.max(0, millis));
  });
}

function fibonacci(index: number): number {
  let previous = 0;
  let current = 1;
  for (let position = 0; position < index; position += 1) {
    const next = previous + current;
    previous = current;
    current = next;
  }
  return current;
}

function scheduleDelay(schedule: Schedule, attempt: number): ?number {
  return match (schedule) {
    {kind: "recurs", times: const times} => (attempt < times ? 0 : null),
    {kind: "spaced", millis: const millis} => millis,
    {kind: "exponential", baseMillis: const baseMillis, factorPercent: const factorPercent} =>
      exponentialDelay(baseMillis, factorPercent, attempt),
    {kind: "fibonacci", baseMillis: const baseMillis} => baseMillis * fibonacci(attempt),
    {kind: "upTo", millis: const millis} => (attempt === 0 ? Math.max(0, millis) : null),
    {kind: "intersect", left: const left, right: const right} =>
      intersectDelay(left, right, attempt),
    {kind: "union", left: const left, right: const right} => unionDelay(left, right, attempt),
    {kind: "maxDelay", schedule: const inner, millis: const millis} =>
      capDelay(inner, millis, attempt),
  };
}

function exponentialDelay(baseMillis: number, factorPercent: ?number, attempt: number): number {
  const factor = (factorPercent == null ? 200 : factorPercent) / 100;
  return Math.round(baseMillis * Math.pow(factor, attempt));
}

function intersectDelay(left: Schedule, right: Schedule, attempt: number): ?number {
  const leftDelay = scheduleDelay(left, attempt);
  const rightDelay = scheduleDelay(right, attempt);
  return leftDelay == null || rightDelay == null ? null : Math.max(leftDelay, rightDelay);
}

function unionDelay(left: Schedule, right: Schedule, attempt: number): ?number {
  const leftDelay = scheduleDelay(left, attempt);
  const rightDelay = scheduleDelay(right, attempt);
  if (leftDelay == null) {
    return rightDelay;
  }
  if (rightDelay == null) {
    return leftDelay;
  }
  return Math.min(leftDelay, rightDelay);
}

function capDelay(schedule: Schedule, millis: number, attempt: number): ?number {
  const delayed = scheduleDelay(schedule, attempt);
  return delayed == null ? null : Math.min(delayed, millis);
}

function concurrencyLimit(length: number, options?: { +concurrency?: Concurrency }): number {
  const requested = options == null ? "unbounded" : options.concurrency;
  if (requested == null || requested === "unbounded" || requested === "inherit") {
    return Math.max(1, length);
  }
  return Math.max(1, Math.min(length, Math.floor(requested)));
}

export function succeed<A>(value: A): Effect<A> {
  return makeEffect({
    run: () => Promise.resolve(success(value)),
    runSync: () => success(value),
  });
}

export function fail<E>(error: E): Effect<empty, E> {
  return makeEffect({
    run: () => Promise.resolve(failure(failCause(error))),
    runSync: () => failure(failCause(error)),
  });
}

export function die(defectValue: mixed): Effect<empty> {
  return makeEffect({
    run: () => Promise.resolve(defect(defectValue)),
    runSync: () => defect(defectValue),
  });
}

export function never(): Effect<empty> {
  return makeEffect({
    run: () => new Promise(() => {}),
  });
}

export function sync<A>(body: () => A): Effect<A> {
  return makeEffect({
    run: () => {
      try {
        return Promise.resolve(success(body()));
      } catch (error) {
        return Promise.resolve(defect(error));
      }
    },
    runSync: () => {
      try {
        return success(body());
      } catch (error) {
        return defect(error);
      }
    },
  });
}

export function suspend<A, E, R>(body: () => Effect<A, E, R>): Effect<A, E, R> {
  return makeEffect({
    run: (runContext) => {
      try {
        return runKernel(body(), runContext);
      } catch (error) {
        return Promise.resolve(defect(error));
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

export function tryPromise<A, E>(options: {
  +try: () => Promise<A>,
  +catch: (error: mixed) => E,
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

export function call<A>(body: () => A | Promise<A>): Effect<A> {
  return makeEffect({
    run: async () => {
      try {
        return success(await Promise.resolve(body()));
      } catch (error) {
        return defect(error);
      }
    },
    runSync: () => {
      try {
        const value = body();
        if (isPromiseLike(value)) {
          return defect("call returned a promise in runSyncExit");
        }
        return success(value);
      } catch (error) {
        return defect(error);
      }
    },
  });
}

export function effect<A, E, R>(body: () => EffectGenerator<A, E, R>): Effect<A, E, R> {
  return makeEffect({
    run: async (runContext) => {
      let input;
      let iterator;
      try {
        iterator = body();
      } catch (error) {
        return defect(error);
      }
      while (true) {
        // The checkpoint is between steps, never inside one. An effect that has
        // started runs to its own end; interruption decides whether the *next*
        // one starts, which is the only point where stopping is safe without
        // knowing what the body was in the middle of.
        if (isInterrupted(runContext)) {
          return interruptedExit();
        }
        let step;
        try {
          step = iterator.next(input);
        } catch (error) {
          return defect(error);
        }
        if (step.done) {
          return success(step.value);
        }
        const exit = await runKernel((step.value: any), runContext);
        if (exit.kind === "failure") {
          return (exit: any);
        }
        input = exit.value;
      }
    },
  });
}

export function map<A, B, E, R>(
  self: Effect<A, E, R>,
  transform: (value: A) => B,
): Effect<B, E, R> {
  return makeEffect({
    run: async (runContext) => {
      const exit = await runKernel(self, runContext);
      if (exit.kind === "failure") {
        return (exit: any);
      }
      try {
        return success(transform(exit.value));
      } catch (error) {
        return defect(error);
      }
    },
    runSync: (runContext) => {
      const exit = runSyncKernel(self, runContext);
      if (exit.kind === "failure") {
        return (exit: any);
      }
      try {
        return success(transform(exit.value));
      } catch (error) {
        return defect(error);
      }
    },
  });
}

export function mapError<A, E, F, R>(
  self: Effect<A, E, R>,
  transform: (error: E) => F,
): Effect<A, F, R> {
  return makeEffect({
    run: async (runContext) => {
      const exit = await runKernel(self, runContext);
      return exit.kind === "failure" ? failure(mapCause(exit.cause, transform)) : (exit: any);
    },
    runSync: (runContext) => {
      const exit = runSyncKernel(self, runContext);
      return exit.kind === "failure" ? failure(mapCause(exit.cause, transform)) : (exit: any);
    },
  });
}

export function flatMap<A, B, E1, E2, R1, R2>(
  self: Effect<A, E1, R1>,
  next: (value: A) => Effect<B, E2, R2>,
): Effect<B, E1 | E2, R1 | R2> {
  return makeEffect({
    run: async (runContext) => {
      const exit = await runKernel(self, runContext);
      if (exit.kind === "failure") {
        return (exit: any);
      }
      if (isInterrupted(runContext)) {
        return interruptedExit();
      }
      try {
        return await runKernel(next(exit.value), runContext);
      } catch (error) {
        return defect(error);
      }
    },
    runSync: (runContext) => {
      const exit = runSyncKernel(self, runContext);
      if (exit.kind === "failure") {
        return (exit: any);
      }
      try {
        return runSyncKernel(next(exit.value), runContext);
      } catch (error) {
        return defect(error);
      }
    },
  });
}

export function andThen<A, B, E1, E2, R1, R2>(
  self: Effect<A, E1, R1>,
  next: Effect<B, E2, R2>,
): Effect<B, E1 | E2, R1 | R2> {
  return flatMap(self, () => next);
}

export function zip<A, B, E1, E2, R1, R2>(
  self: Effect<A, E1, R1>,
  other: Effect<B, E2, R2>,
): Effect<[A, B], E1 | E2, R1 | R2> {
  return flatMap(self, (left) => map(other, (right) => [left, right]));
}

export function all<A, E, R>(
  effects: $ReadOnlyArray<Effect<A, E, R>>,
  options?: { +concurrency?: Concurrency },
): Effect<$ReadOnlyArray<A>, E, R> {
  return makeEffect({
    run: async (runContext) => {
      const results = new Array(effects.length);
      let nextIndex = 0;
      let failed = null;
      async function worker(): Promise<void> {
        while (failed == null && nextIndex < effects.length) {
          if (isInterrupted(runContext)) {
            failed = interruptedExit();
            return;
          }
          const index = nextIndex;
          nextIndex += 1;
          const exit = await runKernel(effects[index], runContext);
          if (exit.kind === "failure") {
            failed = exit;
            return;
          }
          results[index] = exit.value;
        }
      }
      const workers = new Array(concurrencyLimit(effects.length, options))
        .fill(null)
        .map(() => worker());
      await Promise.all(workers);
      return failed == null ? success(results) : (failed: any);
    },
  });
}

export function forEach<A, B, E, R>(
  items: $ReadOnlyArray<A>,
  body: (item: A, index: number) => Effect<B, E, R>,
  options?: { +concurrency?: Concurrency },
): Effect<$ReadOnlyArray<B>, E, R> {
  return all(items.map((item, index) => body(item, index)), options);
}

export function race<A, E, R>(
  effects: $ReadOnlyArray<Effect<A, E, R>>,
): Effect<A, E, R> {
  return makeEffect({
    run: (runContext) => {
      if (effects.length === 0) {
        return new Promise(() => {});
      }
      return Promise.race(effects.map((entry) => runKernel(entry, runContext)));
    },
  });
}

export function catchAll<A, B, E, F, R1, R2>(
  self: Effect<A, E, R1>,
  recover: (error: E) => Effect<B, F, R2>,
): Effect<A | B, F, R1 | R2> {
  return makeEffect({
    run: async (runContext) => {
      const exit = await runKernel(self, runContext);
      if (exit.kind === "success") {
        return (exit: any);
      }
      const error = firstFailure(exit.cause);
      return error == null ? (exit: any) : await runKernel(recover(error), runContext);
    },
    runSync: (runContext) => {
      const exit = runSyncKernel(self, runContext);
      if (exit.kind === "success") {
        return (exit: any);
      }
      const error = firstFailure(exit.cause);
      return error == null ? (exit: any) : runSyncKernel(recover(error), runContext);
    },
  });
}

export function catchTag<A, B, E, F, R1, R2>(
  self: Effect<A, E, R1>,
  tagName: string,
  recover: (error: E) => Effect<B, F, R2>,
): Effect<A | B, E | F, R1 | R2> {
  return catchAll(self, (error) => {
    const actual = (error: any).kind == null ? (error: any).tag : (error: any).kind;
    return actual === tagName ? recover(error) : fail(error);
  });
}

export function orElse<A, B, E, F, R1, R2>(
  self: Effect<A, E, R1>,
  fallback: () => Effect<B, F, R2>,
): Effect<A | B, F, R1 | R2> {
  return catchAll(self, () => fallback());
}

export function either<A, E, R>(
  self: Effect<A, E, R>,
): Effect<{ +ok: true, +value: A } | { +ok: false, +error: E }, empty, R> {
  return makeEffect({
    run: async (runContext) => {
      const exit = await runKernel(self, runContext);
      if (exit.kind === "success") {
        return success({ ok: true, value: exit.value });
      }
      return success({ ok: false, error: (throwable(exit.cause): any) });
    },
    runSync: (runContext) => {
      const exit = runSyncKernel(self, runContext);
      if (exit.kind === "success") {
        return success({ ok: true, value: exit.value });
      }
      return success({ ok: false, error: (throwable(exit.cause): any) });
    },
  });
}

export function retry<A, E, R>(
  self: Effect<A, E, R>,
  schedule: Schedule,
): Effect<A, E, R> {
  return makeEffect({
    run: async (runContext) => {
      let attempt = 0;
      while (true) {
        const exit = await runKernel(self, runContext);
        if (exit.kind === "success") {
          return exit;
        }
        // Only a typed failure is worth another attempt. A defect is a bug, so
        // running it again runs the bug again; an interruption is a decision
        // that has already been taken. Retrying either would turn one wrong
        // answer into several.
        if (!isRetriable(exit.cause)) {
          return exit;
        }
        const millis = scheduleDelay(schedule, attempt);
        if (millis == null || isInterrupted(runContext)) {
          return exit;
        }
        attempt += 1;
        await pause(millis, runContext);
      }
    },
  });
}

export function timeout<A, E, R>(
  self: Effect<A, E, R>,
  millis: number,
): Effect<A, E | { +kind: "timeout", +millis: number }, R> {
  return makeEffect({
    run: (runContext) =>
      Promise.race([
        runKernel(self, runContext),
        pause(millis, runContext).then(() => failure(failCause({ kind: "timeout", millis }))),
      ]),
  });
}

export function acquireRelease<A, E, R>(
  acquire: Effect<A, E, R>,
  release: (resource: A) => Effect<void>,
): Effect<A, E, R | Scope> {
  return makeEffect({
    run: async (runContext) => {
      const exit = await runKernel(acquire, runContext);
      if (exit.kind === "success" && runContext.scope != null) {
        const resource = exit.value;
        runContext.scope.finalizers.push(() => release(resource));
      }
      return (exit: any);
    },
  });
}

export function scoped<A, E, R>(
  self: Effect<A, E, R | Scope>,
): Effect<A, E, R> {
  return makeEffect({
    run: async (runContext) => {
      const scopedContext = withScope(runContext);
      const exit = await runKernel(self, scopedContext);
      const finalizers = scopedContext.scope == null ? [] : scopedContext.scope.finalizers;
      for (let index = finalizers.length - 1; index >= 0; index -= 1) {
        const released = await runKernel(finalizers[index](), runContext);
        if (released.kind === "failure" && exit.kind === "success") {
          return (released: any);
        }
      }
      return (exit: any);
    },
  });
}

export function tag<Service>(identifier: string): Tag<Service> {
  return makeTag(identifier);
}

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

export function provide<A, E, R, Out, LayerError, In>(
  self: Effect<A, E, R | Out>,
  layer: Layer<Out, LayerError, In>,
): Effect<A, E | LayerError, R | In> {
  return makeEffect({
    run: async (runContext) => {
      const built = await readLayer(layer)(runContext);
      if (built.kind === "failure") {
        return (built: any);
      }
      const services = { ...runContext.services };
      for (const key in built.value) {
        if (Object.prototype.hasOwnProperty.call(built.value, key)) {
          services[key] = built.value[key];
        }
      }
      // The fiber has to come through with the services. Every interruption
      // check reads `runContext.fiber`, so a context assembled without it
      // makes anything that can be interrupted throw instead — which nothing
      // noticed while no effect under a layer ever checked.
      return await runKernel(self, {
        services,
        scope: runContext.scope,
        fiber: runContext.fiber,
      });
    },
  });
}

export function layerSucceed<Service>(
  serviceTag: Tag<Service>,
  service: Service,
): Layer<Service> {
  return makeLayer(() => Promise.resolve(success({ [readTag(serviceTag)]: service })));
}

export function layerEffect<Service, E, R>(
  serviceTag: Tag<Service>,
  build: Effect<Service, E, R>,
): Layer<Service, E, R> {
  return makeLayer(async (runContext) => {
    const exit = await runKernel(build, runContext);
    return exit.kind === "failure"
      ? (exit: any)
      : success({ [readTag(serviceTag)]: exit.value });
  });
}

export function layerMerge<Out1, Out2, E1, E2, In1, In2>(
  left: Layer<Out1, E1, In1>,
  right: Layer<Out2, E2, In2>,
): Layer<Out1 | Out2, E1 | E2, In1 | In2> {
  return makeLayer(async (runContext) => {
    const leftExit = await readLayer(left)(runContext);
    if (leftExit.kind === "failure") {
      return (leftExit: any);
    }
    const rightExit = await readLayer(right)(runContext);
    if (rightExit.kind === "failure") {
      return (rightExit: any);
    }
    return success({ ...leftExit.value, ...rightExit.value });
  });
}

/**
 * Start `self` beside the current fiber and hand back a handle to it.
 *
 * The child gets its own interruption state, so cancelling it does not cancel
 * the fiber that forked it, and cancelling the parent does not silently take
 * the child down with it. Anything else makes `fork` unusable for the case it
 * exists for: starting work you intend to be able to stop.
 */
export function fork<A, E, R>(self: Effect<A, E, R>): Effect<Fiber<A, E>, empty, R> {
  return makeEffect({
    run: (runContext) => {
      const child = withFiber(runContext);
      return Promise.resolve(success(makeFiber(runKernel(self, child), child.fiber)));
    },
  });
}

/** Wait for a fiber and take its result as this effect's result. */
export function join<A, E>(fiber: Fiber<A, E>): Effect<A, E> {
  return makeEffect({
    run: async () => (await readFiber(fiber).__promise: any),
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
      const carrier = readFiber(fiber);
      carrier.__fiber.interrupted = true;
      for (const wake of Array.from(carrier.__fiber.wakers)) {
        wake();
      }
      return success(((await carrier.__promise): any));
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
 * The point of a `tap` is that it cannot alter the value by accident: logging
 * a result inside a `map` means one careless edit turns the log's return value
 * into the pipeline's value, and this shape makes that impossible.
 */
export function tap<A, E1, E2, R1, R2>(
  self: Effect<A, E1, R1>,
  body: (value: A) => Effect<mixed, E2, R2>,
): Effect<A, E1 | E2, R1 | R2> {
  return flatMap(self, (value) => map(body(value), () => value));
}

/** Look at a failure without recovering from it. */
export function tapError<A, E, R1, R2>(
  self: Effect<A, E, R1>,
  body: (error: E) => Effect<mixed, mixed, R2>,
): Effect<A, E, R1 | R2> {
  return catchAll(self, (error) => andThen(orDie(body(error)), fail(error)));
}

/** Turn any failure of `self` into a defect, so its error type is `empty`. */
function orDie<A, E, R>(self: Effect<A, E, R>): Effect<A, empty, R> {
  return makeEffect({
    run: async (runContext) => {
      const settled = await runKernel(self, runContext);
      return settled.kind === "success"
        ? (settled: any)
        : failure(dieCause(causeMessage(settled.cause)));
    },
  });
}

/**
 * Run `finalizer` however `self` ends: success, failure, defect, interruption.
 *
 * `acquireRelease` needs a `Scope`; this does not, which makes it the right
 * tool for the ordinary "close this when the block is done" case that would
 * otherwise force a scope on the caller's type just to run one cleanup.
 */
export function ensuring<A, E, R>(
  self: Effect<A, E, R>,
  finalizer: () => Effect<mixed, mixed, empty>,
): Effect<A, E, R> {
  return makeEffect({
    run: async (runContext) => {
      const settled = await runKernel(self, runContext);
      // The finalizer runs in a context that is not interrupted, because a
      // cleanup that is itself cancelled is not a cleanup.
      const released = await runKernel(finalizer(), withFiber(runContext));
      if (released.kind === "failure" && settled.kind === "success") {
        return (released: any);
      }
      return (settled: any);
    },
  });
}

/**
 * Reify the outcome, so a failure is a value instead of a short circuit.
 *
 * `either` narrows to the typed error and loses defects and interruption;
 * this keeps the whole `Exit`, which is what a supervisor or a test that
 * asserts on *how* something failed actually needs.
 */
export function exit<A, E, R>(self: Effect<A, E, R>): Effect<Exit<A, E>, empty, R> {
  return makeEffect({
    run: async (runContext) => success(((await runKernel(self, runContext)): any)),
    runSync: (runContext) => success((runSyncKernel(self, runContext): any)),
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

export async function runPromise<A, E>(self: Effect<A, E>): Promise<A> {
  const exit = await runKernel(self, context());
  if (exit.kind === "success") {
    return exit.value;
  }
  throw throwable(exit.cause);
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
