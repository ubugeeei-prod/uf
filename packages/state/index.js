// @flow
//
// `@uniflowed/state`: the cell primitives plus their React binding.

import type { Cell, CellScope } from "@uniflowed/cell";
import { cell, computed, read, resource, subscribe, update, write } from "@uniflowed/cell";

export type { Cell, CellScope };
export { cell, computed, read, resource, subscribe, update, write };

export hook useCell<T>(source: Cell<T>): T {
  return read(source);
}
