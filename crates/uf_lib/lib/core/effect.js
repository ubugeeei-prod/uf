// @flow
//
// `@uniflowed/effect`.
//
// An effect describes work that has not happened yet, in three channels: what
// it produces, how it can fail, and what it needs from its environment. Keeping
// failure and requirements in the type is the whole point — a `catch` tells you
// nothing about what a function throws, and a `Promise` tells you nothing about
// what it needs.
//
// The runtime semantics live in Rust, in `crates/uf_effect`: `Cause` keeps the
// structure of a failure, `Exit` distinguishes cancelled from failed, and
// `Schedule` decides whether to try again. This module is the typed surface
// over them.

import type {
  NativeHandle,
  NativeHandleCovariant,
  NativeHandleCovariant2,
  NativeHandleCovariant3,
} from "./internal/native-runtime.js";
import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/effect";

/**
 * Work that produces an `A`, may fail with an `E`, and needs an `R`.
 *
 * All three parameters are covariant, and the carrier earns each sigil by
 * putting the parameter behind a function that returns it.
 *
 * `E` defaults to `empty` — Flow's bottom type — so `Effect<number>` reads as
 * "cannot fail", not "fails with anything". `R` defaults to `empty` for the same
 * reason: an effect that needs nothing. `R` is covariant rather than
 * contravariant because it is a *union of required services*: needing `Db` is
 * assignable where at most `Db | Clock` is needed.
 */
export opaque type Effect<+A, +E = empty, +R = empty> = NativeHandleCovariant3<
  "@uniflowed/core/effect#Effect",
  A,
  E,
  R,
>;

/** A running effect, addressable so it can be awaited or interrupted. */
export opaque type Fiber<+A, +E = empty> = NativeHandleCovariant2<
  "@uniflowed/core/effect#Fiber",
  A,
  E,
>;

/** Identifies one service inside a context. */
export opaque type Tag<+Service> = NativeHandleCovariant<
  "@uniflowed/core/effect#Tag",
  Service,
>;

/** A recipe for building the services in `Out`, itself possibly failing. */
export opaque type Layer<+Out, +E = empty, +In = empty> = NativeHandleCovariant3<
  "@uniflowed/core/effect#Layer",
  Out,
  E,
  In,
>;

/** The lifetime a resource is released at. */
export opaque type Scope = NativeHandle<"@uniflowed/core/effect#Scope">;

/**
 * Why an effect did not produce a value.
 *
 * Mirrors `uf_effect::Cause`. `sequential` records "this failed, then cleanup
 * also failed"; `parallel` records "these failed at once". Both hold a list
 * rather than a pair, so ten thousand concurrent failures are one wide node
 * instead of a structure ten thousand levels deep.
 */
export type Cause<+E> =
  | { +kind: "empty" }
  | { +kind: "fail", +error: E }
  | { +kind: "die", +defect: string }
  | { +kind: "interrupt" }
  | { +kind: "sequential", +causes: $ReadOnlyArray<Cause<E>> }
  | { +kind: "parallel", +causes: $ReadOnlyArray<Cause<E>> };

/** How an effect ended. A failure carries a whole `Cause`, not one error. */
export type Exit<+A, +E> =
  | { +kind: "success", +value: A }
  | { +kind: "failure", +cause: Cause<E> };

/**
 * A retry or repeat policy.
 *
 * Data rather than a callback, so a policy can be logged, serialized into a
 * build manifest, and compared. Mirrors `uf_effect::Schedule`.
 */
export type Schedule =
  | { +kind: "recurs", +times: number }
  | { +kind: "spaced", +millis: number }
  | { +kind: "exponential", +baseMillis: number, +factorPercent?: number }
  | { +kind: "fibonacci", +baseMillis: number }
  | { +kind: "upTo", +millis: number }
  | { +kind: "intersect", +left: Schedule, +right: Schedule }
  | { +kind: "union", +left: Schedule, +right: Schedule }
  | { +kind: "maxDelay", +schedule: Schedule, +millis: number };

/** How many effects `all` and `forEach` may run at once. */
export type Concurrency = number | "unbounded" | "inherit";

/** An effect that already has its value. */
export function succeed<A>(value: A): Effect<A> {
  return nativeRuntimeRequired(MODULE, "succeed");
}

/** An effect that fails in the way its type declares. */
export function fail<E>(error: E): Effect<empty, E> {
  return nativeRuntimeRequired(MODULE, "fail");
}

/** An effect that dies: a failure nobody declared. */
export function die(defect: mixed): Effect<empty> {
  return nativeRuntimeRequired(MODULE, "die");
}

/** An effect that never produces anything and never fails. */
export function never(): Effect<empty> {
  return nativeRuntimeRequired(MODULE, "never");
}

/** Defer a synchronous computation. Throwing from `body` is a defect. */
export function sync<A>(body: () => A): Effect<A> {
  return nativeRuntimeRequired(MODULE, "sync");
}

/** Defer building the effect itself, so recursion does not run eagerly. */
export function suspend<A, E, R>(body: () => Effect<A, E, R>): Effect<A, E, R> {
  return nativeRuntimeRequired(MODULE, "suspend");
}

/** Adopt a promise that is not expected to reject. Rejection is a defect. */
export function promise<A>(body: () => Promise<A>): Effect<A> {
  return nativeRuntimeRequired(MODULE, "promise");
}

/**
 * Adopt a promise that may reject, mapping the rejection into the error channel.
 *
 * `catch_` names the recovery function; `catch` is a reserved word, and a
 * trailing underscore is less surprising than a rename.
 */
export function tryPromise<A, E>(options: {
  +try: () => Promise<A>,
  +catch_: (error: mixed) => E,
}): Effect<A, E> {
  return nativeRuntimeRequired(MODULE, "tryPromise");
}

/** Rewrite the success value. */
export function map<A, B, E, R>(
  self: Effect<A, E, R>,
  transform: (value: A) => B,
): Effect<B, E, R> {
  return nativeRuntimeRequired(MODULE, "map");
}

/** Rewrite the declared error. */
export function mapError<A, E, F, R>(
  self: Effect<A, E, R>,
  transform: (error: E) => F,
): Effect<A, F, R> {
  return nativeRuntimeRequired(MODULE, "mapError");
}

/**
 * Sequence a dependent effect. The error and requirement channels union.
 *
 * This is where the three-channel type pays for itself: the result declares
 * everything either half can fail with and everything either half needs.
 */
export function flatMap<A, B, E1, E2, R1, R2>(
  self: Effect<A, E1, R1>,
  next: (value: A) => Effect<B, E2, R2>,
): Effect<B, E1 | E2, R1 | R2> {
  return nativeRuntimeRequired(MODULE, "flatMap");
}

/** Run `self`, discard its value, then run `next`. */
export function andThen<A, B, E1, E2, R1, R2>(
  self: Effect<A, E1, R1>,
  next: Effect<B, E2, R2>,
): Effect<B, E1 | E2, R1 | R2> {
  return nativeRuntimeRequired(MODULE, "andThen");
}

/** Run both and keep both values. */
export function zip<A, B, E1, E2, R1, R2>(
  self: Effect<A, E1, R1>,
  other: Effect<B, E2, R2>,
): Effect<[A, B], E1 | E2, R1 | R2> {
  return nativeRuntimeRequired(MODULE, "zip");
}

/** Run every effect, with the requested concurrency. */
export function all<A, E, R>(
  effects: $ReadOnlyArray<Effect<A, E, R>>,
  options?: { +concurrency?: Concurrency },
): Effect<$ReadOnlyArray<A>, E, R> {
  return nativeRuntimeRequired(MODULE, "all");
}

/** Apply an effectful function across a collection. */
export function forEach<A, B, E, R>(
  items: $ReadOnlyArray<A>,
  body: (item: A, index: number) => Effect<B, E, R>,
  options?: { +concurrency?: Concurrency },
): Effect<$ReadOnlyArray<B>, E, R> {
  return nativeRuntimeRequired(MODULE, "forEach");
}

/** Take the first to finish and interrupt the rest. */
export function race<A, E, R>(
  effects: $ReadOnlyArray<Effect<A, E, R>>,
): Effect<A, E, R> {
  return nativeRuntimeRequired(MODULE, "race");
}

/** Recover from any declared failure. */
export function catchAll<A, B, E, F, R1, R2>(
  self: Effect<A, E, R1>,
  recover: (error: E) => Effect<B, F, R2>,
): Effect<A | B, F, R1 | R2> {
  return nativeRuntimeRequired(MODULE, "catchAll");
}

/** Recover from one tagged member of a union error type. */
export function catchTag<A, B, E, F, R1, R2>(
  self: Effect<A, E, R1>,
  tag: string,
  recover: (error: E) => Effect<B, F, R2>,
): Effect<A | B, E | F, R1 | R2> {
  return nativeRuntimeRequired(MODULE, "catchTag");
}

/** Fall back to another effect when this one fails. */
export function orElse<A, B, E, F, R1, R2>(
  self: Effect<A, E, R1>,
  fallback: () => Effect<B, F, R2>,
): Effect<A | B, F, R1 | R2> {
  return nativeRuntimeRequired(MODULE, "orElse");
}

/** Move the failure into the value, so the caller must handle it. */
export function either<A, E, R>(
  self: Effect<A, E, R>,
): Effect<{ +ok: true, +value: A } | { +ok: false, +error: E }, empty, R> {
  return nativeRuntimeRequired(MODULE, "either");
}

/** Retry on failure according to `schedule`. */
export function retry<A, E, R>(
  self: Effect<A, E, R>,
  schedule: Schedule,
): Effect<A, E, R> {
  return nativeRuntimeRequired(MODULE, "retry");
}

/** Fail with `None` if the effect has not finished in time. */
export function timeout<A, E, R>(
  self: Effect<A, E, R>,
  millis: number,
): Effect<A, E | { +kind: "timeout", +millis: number }, R> {
  return nativeRuntimeRequired(MODULE, "timeout");
}

/** Acquire a resource and release it when the scope closes, even on failure. */
export function acquireRelease<A, E, R>(
  acquire: Effect<A, E, R>,
  release: (resource: A) => Effect<void>,
): Effect<A, E, R | Scope> {
  return nativeRuntimeRequired(MODULE, "acquireRelease");
}

/** Close the scope every `acquireRelease` inside `self` registered against. */
export function scoped<A, E, R>(
  self: Effect<A, E, R | Scope>,
): Effect<A, E, R> {
  return nativeRuntimeRequired(MODULE, "scoped");
}

/** Name a service so it can be required and provided by type. */
export function tag<Service>(identifier: string): Tag<Service> {
  return nativeRuntimeRequired(MODULE, "tag");
}

/** Satisfy one required service, removing it from the requirement channel. */
export function provideService<A, E, R, Service>(
  self: Effect<A, E, R | Service>,
  serviceTag: Tag<Service>,
  service: Service,
): Effect<A, E, R> {
  return nativeRuntimeRequired(MODULE, "provideService");
}

/** Satisfy a group of services from a layer. */
export function provide<A, E, R, Out, LayerError, In>(
  self: Effect<A, E, R | Out>,
  layer: Layer<Out, LayerError, In>,
): Effect<A, E | LayerError, R | In> {
  return nativeRuntimeRequired(MODULE, "provide");
}

/** A layer that already has its service. */
export function layerSucceed<Service>(
  serviceTag: Tag<Service>,
  service: Service,
): Layer<Service> {
  return nativeRuntimeRequired(MODULE, "layerSucceed");
}

/** A layer that builds its service with an effect. */
export function layerEffect<Service, E, R>(
  serviceTag: Tag<Service>,
  build: Effect<Service, E, R>,
): Layer<Service, E, R> {
  return nativeRuntimeRequired(MODULE, "layerEffect");
}

/** Combine two layers into one that provides both. */
export function layerMerge<Out1, Out2, E1, E2, In1, In2>(
  left: Layer<Out1, E1, In1>,
  right: Layer<Out2, E2, In2>,
): Layer<Out1 | Out2, E1 | E2, In1 | In2> {
  return nativeRuntimeRequired(MODULE, "layerMerge");
}

/** Start the effect without waiting for it. */
export function fork<A, E, R>(self: Effect<A, E, R>): Effect<Fiber<A, E>, empty, R> {
  return nativeRuntimeRequired(MODULE, "fork");
}

/** Wait for a fiber's result. */
export function join<A, E>(fiber: Fiber<A, E>): Effect<A, E> {
  return nativeRuntimeRequired(MODULE, "join");
}

/** Ask a fiber to stop and wait for its cleanup to finish. */
export function interrupt<A, E>(fiber: Fiber<A, E>): Effect<Exit<A, E>> {
  return nativeRuntimeRequired(MODULE, "interrupt");
}

/**
 * Run an effect that needs nothing, as a promise.
 *
 * The `R = empty` bound is the point: an effect with an unsatisfied requirement
 * will not type-check here, so a missing service is a compile error rather than
 * a runtime one.
 */
export function runPromise<A, E>(self: Effect<A, E>): Promise<A> {
  return nativeRuntimeRequired(MODULE, "runPromise");
}

/** Run a synchronous effect that needs nothing, returning its exit. */
export function runSyncExit<A, E>(self: Effect<A, E>): Exit<A, E> {
  return nativeRuntimeRequired(MODULE, "runSyncExit");
}

/** Start an effect that needs nothing and return its fiber. */
export function runFork<A, E>(self: Effect<A, E>): Fiber<A, E> {
  return nativeRuntimeRequired(MODULE, "runFork");
}
