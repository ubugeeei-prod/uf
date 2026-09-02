// @flow
//
// `@uniflowed/query`.

import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/query";

export type QueryKey = $ReadOnlyArray<string | number | boolean>;

export interface QueryState<T> {
  value: null | T,
  error: null | Error,
  pending: boolean,
  refetch(): Promise<T>,
}

export interface QueryResource<T> {
  key: QueryKey,
  use(): QueryState<T>,
  prefetch(): Promise<T>,
}

export interface Mutation<TInput, TOutput> {
  run(input: TInput): Promise<TOutput>,
}

export function createQuery<T>(config: {
  +key: QueryKey,
  +query: () => T | Promise<T>,
}): QueryResource<T> {
  return nativeRuntimeRequired(MODULE, "createQuery");
}

export function createMutation<TInput, TOutput>(config: {
  +mutation: (input: TInput) => TOutput | Promise<TOutput>,
}): Mutation<TInput, TOutput> {
  return nativeRuntimeRequired(MODULE, "createMutation");
}
