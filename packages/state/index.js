// @flow
//
// `@uniflowed/state`: the cell primitives plus their React binding.

import type { Cell } from "@uniflowed/flow-cell";
import { nativeRuntimeRequired } from "@uniflowed/core/native";

const MODULE = "@uniflowed/core/state";

export type { Cell, CellScope } from "@uniflowed/flow-cell";
export { cell, computed, resource } from "@uniflowed/flow-cell";

export function useCell<T>(cell: Cell<T>): T {
  return nativeRuntimeRequired(MODULE, "useCell");
}
