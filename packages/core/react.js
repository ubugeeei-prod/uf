// @flow
//
// `@uniflowed/react`.

import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/react";

export type Node = mixed;
export type SetState<S> = S | ((previous: S) => S);

export component Suspense(fallback?: Node, children?: Node) renders Node {
  return nativeRuntimeRequired(MODULE, "Suspense");
}

export function use<T>(usable: Promise<T> | T): T {
  return nativeRuntimeRequired(MODULE, "use");
}

export function useState<S>(initial: S): [S, (next: SetState<S>) => void] {
  return nativeRuntimeRequired(MODULE, "useState");
}

export function cache<TArgs: $ReadOnlyArray<mixed>, TReturn>(
  fn: (...TArgs) => TReturn,
): (...TArgs) => TReturn {
  return nativeRuntimeRequired(MODULE, "cache");
}

/**
 * Namespace object for `import React from "@uniflowed/react"`.
 *
 * Also exported by name, because `uf_lib`'s registry lists `React` among the
 * exports of this specifier and a named import must be able to reach it.
 */
const React: {
  +Suspense: typeof Suspense,
  +use: typeof use,
  +useState: typeof useState,
  +cache: typeof cache,
} = { Suspense, use, useState, cache };

export { React };
export default React;
