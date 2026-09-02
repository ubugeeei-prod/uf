// @flow
//
// `@uniflowed/state`: the cell primitives plus their React binding.

import type { Cell } from "./flow-cell.js";
import { nativeRuntimeRequired } from "./internal/native-runtime.js";

const MODULE = "@uniflowed/core/state";

export type { Cell, CellScope } from "./flow-cell.js";
export { cell, computed, resource } from "./flow-cell.js";

export function useCell<T>(cell: Cell<T>): T {
  return nativeRuntimeRequired(MODULE, "useCell");
}
