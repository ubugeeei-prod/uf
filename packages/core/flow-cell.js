// @flow
//
// `@uniflowed/flow-cell`: the runtime-agnostic cell primitives.
//
// This module owns `Cell` and the three constructors. `./state.js` re-exports
// them by name and adds the React binding, so the two specifiers share one
// opaque `Cell` instead of forking it into two incompatible types.

import type { NativeHandleInvariant } from "./internal/native-runtime.js";
import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/flow-cell";

export type CellScope = "client" | "server" | "react-render" | "native-runtime";

/**
 * A reactive cell holding a `T`.
 *
 * Invariant: a cell is both read and written, so `Cell<Dog>` is neither a
 * subtype nor a supertype of `Cell<Animal>`.
 */
export opaque type Cell<T> = NativeHandleInvariant<
  "@uniflowed/core/flow-cell#Cell",
  T,
>;

export function cell<T>(value: T): Cell<T> {
  return nativeRuntimeRequired(MODULE, "cell");
}

export function computed<T>(derive: () => T): Cell<T> {
  return nativeRuntimeRequired(MODULE, "computed");
}

export function resource<T>(load: () => Promise<T>): Cell<?T> {
  return nativeRuntimeRequired(MODULE, "resource");
}
