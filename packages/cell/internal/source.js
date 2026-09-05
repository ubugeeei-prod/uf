// @flow
//
// Source cells: the roots of the graph, and the only nodes anything writes to.
//
// A source has no `evaluate`, which is what makes it a root — it is never
// stale, because there is nothing it could be stale with respect to. Every
// version stamp in the graph ultimately comes from one of these.
//
// # Why a write of the same value is dropped
//
// Equality is `Object.is` rather than `===`, so writing `NaN` over `NaN` is
// also a no-op, and because `Object.is` is what React compares state with — a
// cell that disagreed with `useState` about what "changed" means would be a
// subtle source of extra renders. A cell whose values are structural rather
// than referential can say so with `equals`.

import type { Cell, CellOptions } from "./graph.js";
import { createNode } from "./graph.js";

/**
 * A cell holding a value directly.
 *
 * Nothing about it is React-aware or environment-aware: the same cell is read
 * in a server action, a worker and a component, which is why the scope it
 * reports is `"client"` only in the sense of "wherever the application is".
 */
export function cell<T>(value: T, options?: CellOptions<T>): Cell<T> {
  return createNode({ kind: "source", scope: "client", value, options });
}
