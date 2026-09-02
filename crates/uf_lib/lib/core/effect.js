// @flow
//
// `@uniflowed/effect`.

import type { NativeHandleCovariant } from "./internal/native-runtime.js";
import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/effect";

/**
 * An effect that yields a `T`.
 *
 * Covariant: the carrier only ever produces a `T`, never consumes one, so
 * `Effect<Dog>` is assignable to `Effect<Animal>`.
 */
export opaque type Effect<+T> = NativeHandleCovariant<
  "@uniflowed/core/effect#Effect",
  T,
>;

/** A forked effect. Covariant for the same reason as `Effect`. */
export opaque type Task<+T> = NativeHandleCovariant<
  "@uniflowed/core/effect#Task",
  T,
>;

export function effect<T>(
  body: () => Generator<Effect<mixed>, T, mixed>,
): Effect<T> {
  return nativeRuntimeRequired(MODULE, "effect");
}

export function call<TArgs: $ReadOnlyArray<mixed>, TReturn>(
  fn: (...TArgs) => TReturn | Promise<TReturn>,
  ...args: TArgs
): Effect<TReturn> {
  return nativeRuntimeRequired(MODULE, "call");
}

export function fork<T>(body: () => Effect<T>): Effect<Task<T>> {
  return nativeRuntimeRequired(MODULE, "fork");
}

export function all<T>(
  effects: $ReadOnlyArray<Effect<T>>,
): Effect<$ReadOnlyArray<T>> {
  return nativeRuntimeRequired(MODULE, "all");
}

export function race<T>(effects: { +[string]: Effect<T> }): Effect<T> {
  return nativeRuntimeRequired(MODULE, "race");
}

export function take<T>(channel: string): Effect<T> {
  return nativeRuntimeRequired(MODULE, "take");
}

export function put<T>(channel: string, value: T): Effect<void> {
  return nativeRuntimeRequired(MODULE, "put");
}
