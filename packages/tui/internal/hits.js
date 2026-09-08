// @flow
//
// What is under the pointer.
//
// # Internal to `@uniflowed/tui`
//
// Absent from `package.json#exports`, and for the same reason `tree.js` is:
// the answer here is only correct for the frame that produced it. A grid held
// across a commit points at nodes React has already replaced, and a consumer
// that could keep one would be asking "what was under the pointer one frame
// ago" while believing it asked something else.
//
// # Why a grid rather than walking the tree
//
// The obvious implementation of a hit test is to walk the tree looking for the
// deepest node whose box contains the point. It gives the wrong answer here,
// twice. A node that a scrolling ancestor put outside its window still has
// last frame's geometry on it, so a walk finds boxes that are not on the
// screen; and `overflow: "hidden"` clips a child to its parent, so a box whose
// geometry contains the point may have been drawn nowhere near it.
//
// Both of those are already solved once — by the painter, whose clip rectangle
// is exactly "the cells this node was allowed to write". So the hit test is
// recorded *during* the paint that answers those questions, and reading it is
// one array index. Overlap resolves the way the picture does: the node painted
// last is the node on top, because it wrote the cell last.
//
// The grid costs one entry per cell of the frame — 1,920 of them on an 80×24
// terminal — and is built only when a renderer has mouse reporting on, so an
// application that does not use the mouse pays nothing for it.

import type { Rect } from "../cells.js";
import { intersect } from "../cells.js";
import type { TuiNode } from "./tree.js";

/** The node that owns each cell of one frame. */
export type HitGrid = {
  readonly width: number,
  readonly height: number,
  /** One entry per cell, row-major, `null` where nothing was drawn. */
  readonly nodes: Array<TuiNode | null>,
};

/** An empty grid the size of a frame. */
export function createHitGrid(width: number, height: number): HitGrid {
  return { width, height, nodes: new Array(width * height).fill(null) };
}

/**
 * Claim every cell of `area` that `clip` allows for `node`.
 *
 * Called by the painter as it descends, so a child overwrites its parent and
 * a later sibling overwrites an earlier one — which is the order they are
 * drawn in, and therefore the order a reader sees them stacked.
 */
export function recordHit(grid: HitGrid, node: TuiNode, area: Rect, clip: Rect): void {
  const box = intersect(clip, area);
  const right = Math.min(grid.width, box.x + box.width);
  const bottom = Math.min(grid.height, box.y + box.height);
  for (let y = Math.max(0, box.y); y < bottom; y += 1) {
    const row = y * grid.width;
    for (let x = Math.max(0, box.x); x < right; x += 1) {
      grid.nodes[row + x] = node;
    }
  }
}

/** The topmost node at a cell, or `null` when the pointer is over nothing. */
export function hitAt(grid: HitGrid, x: number, y: number): TuiNode | null {
  if (x < 0 || y < 0 || x >= grid.width || y >= grid.height) {
    return null;
  }
  return grid.nodes[y * grid.width + x];
}
