// @flow
//
// `@uniflowed/loader`.
//
// `Cell` comes from `@uniflowed/cell` rather than `@uniflowed/state`: the loader needs
// the cell type, not the React hook, and importing the primitive module keeps
// the React binding out of a loader-only bundle.

import type { FetchClient } from "@uniflowed/fetch";
import type { Cell } from "@uniflowed/cell";
import type { NativeHandleInvariant } from "@uniflowed/core/native";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

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
