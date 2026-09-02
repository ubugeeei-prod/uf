// @flow
//
// `@uniflowed/loader`.
//
// `Cell` comes from `./flow-cell.js` rather than `./state.js`: the loader needs
// the cell type, not the React hook, and importing the primitive module keeps
// the React binding out of a loader-only bundle.

import type { FetchClient } from "./fetch.js";
import type { Cell } from "./flow-cell.js";
import type { NativeHandleInvariant } from "./internal/native-runtime.js";
import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/loader";

export type LoaderState<T> =
  | { +status: "idle" }
  | { +status: "pending" }
  | { +status: "ready", +value: T }
  | { +status: "failed", +error: Error };

export opaque type Loader<T> = NativeHandleInvariant<
  "@uniflowed/core/loader#Loader",
  T,
>;

export function createLoader<T>(
  key: string,
  load: (client: FetchClient) => Promise<T>,
): Loader<T> {
  return nativeRuntimeRequired(MODULE, "createLoader");
}

export function useLoader<T>(loader: Loader<T>): LoaderState<T> {
  return nativeRuntimeRequired(MODULE, "useLoader");
}

export function loaderCell<T>(loader: Loader<T>): Cell<LoaderState<T>> {
  return nativeRuntimeRequired(MODULE, "loaderCell");
}

export function preload<T>(loader: Loader<T>): Promise<T> {
  return nativeRuntimeRequired(MODULE, "preload");
}
