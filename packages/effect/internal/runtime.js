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

type Context = {
  services: { [string]: mixed },
  scope: ?ScopeState,
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

function context(): Context {
  return { services: {}, scope: null };
}

function withScope(parent: Context): Context {
  return { services: parent.services, scope: { finalizers: [] } };
}

function withService<Service>(
  parent: Context,
  serviceTag: Tag<Service>,
  service: Service,
): Context {
  return {
    services: { ...parent.services, [readTag(serviceTag)]: service },
    scope: parent.scope,
  };
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

function makeFiber<A, E>(promise: Promise<Exit<mixed, mixed>>): Fiber<A, E> {
  return ({
    __kind: "Fiber",
    __value: absurd,
    __error: absurd,
    __promise: promise,
  }: any);
}

function readFiber<A, E>(fiber: Fiber<A, E>): FiberCarrier<A, E> {
  return (fiber: any);
}

function makeTag<Service>(identifier: string): Tag<Service> {
  return ({ __kind: "Tag", __service: absurd, identifier }: any);
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

function delay(millis: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, Math.max(0, millis)));
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
  +catch_: (error: mixed) => E,
}): Effect<A, E> {
  return makeEffect({
    run: async () => {
      try {
        return success(await options.try());
      } catch (error) {
        return failure(failCause(options.catch_(error)));
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
        const millis = scheduleDelay(schedule, attempt);
        if (millis == null) {
          return exit;
        }
        attempt += 1;
        await delay(millis);
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
        delay(millis).then(() => failure(failCause({ kind: "timeout", millis }))),
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
        services[key] = built.value[key];
      }
      return await runKernel(self, { services, scope: runContext.scope });
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

export function fork<A, E, R>(self: Effect<A, E, R>): Effect<Fiber<A, E>, empty, R> {
  return makeEffect({
    run: (runContext) => Promise.resolve(success(makeFiber(runKernel(self, runContext)))),
  });
}

export function join<A, E>(fiber: Fiber<A, E>): Effect<A, E> {
  return makeEffect({
    run: async () => (await readFiber(fiber).__promise: any),
  });
}

export function interrupt<A, E>(fiber: Fiber<A, E>): Effect<Exit<A, E>> {
  return makeEffect({
    run: async () => success(((await readFiber(fiber).__promise): any)),
  });
}

export async function runPromise<A, E>(self: Effect<A, E>): Promise<A> {
  const exit = await runKernel(self, context());
  if (exit.kind === "success") {
    return exit.value;
  }
  throw throwable(exit.cause);
}

export function runSyncExit<A, E>(self: Effect<A, E>): Exit<A, E> {
  return runSyncKernel(self, context());
}

export function runFork<A, E>(self: Effect<A, E>): Fiber<A, E> {
  return makeFiber(runKernel(self, context()));
}
