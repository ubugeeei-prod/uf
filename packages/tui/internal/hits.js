// @flow
//
// What is under the pointer, and what a drag over it would select.
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
//
// # Two answers, one walk
//
// A second array rides along, and it answers a second question: which cells
// hold text a reader is allowed to select. It is here rather than in a grid of
// its own because it is the same question asked of the same walk — *who wrote
// this cell* — and because both answers are only true of the frame that
// produced them, which is the whole reason this module is internal. Two grids
// would be two allocations and two chances to keep one of them a frame too
// long.
//
// They are separate arrays rather than one, because the two answers are about
// different nodes and resolve differently. A hit is the innermost *box*, and a
// box claims its whole rectangle whether it painted anything into it. A
// selectable cell is the `<Text>` whose grapheme is actually in that cell, and
// only where one is: a box's background is not text, so `recordHit` clears the
// text array over the area it claims, and the painter fills it back in for
// each cluster it writes. Last writer wins, exactly as it does in the frame.

import type { Rect } from "../cells.js";
import { intersect } from "../cells.js";
import type { TuiNode } from "./tree.js";

/** The node that owns each cell of one frame. */
export type HitGrid = {
  readonly width: number,
  readonly height: number,
  /** One entry per cell, row-major, `null` where nothing was drawn. */
  readonly nodes: Array<TuiNode | null>,
  /**
   * The selectable `<Text>` whose grapheme is in each cell, or `null`.
   *
   * `null` covers three different cells and deliberately does not distinguish
   * them: one nothing was drawn in, one a box's background or border owns, and
   * one holding text that said `selectable={false}`. Nothing routed by this
   * array cares which — a drag selects a cell or it does not.
   */
  readonly text: Array<TuiNode | null>,
};

/** An empty grid the size of a frame. */
export function createHitGrid(width: number, height: number): HitGrid {
  const size = width * height;
  return {
    width,
    height,
    nodes: new Array(size).fill(null),
    text: new Array(size).fill(null),
  };
}

/**
 * Claim every cell of `area` that `clip` allows for `node`.
 *
 * Called by the painter as it descends, so a child overwrites its parent and
 * a later sibling overwrites an earlier one — which is the order they are
 * drawn in, and therefore the order a reader sees them stacked.
 *
 * It also *un*claims those cells for selection, because a box is about to
 * paint over them: the text array is filled in afterwards by the clusters this
 * box's descendants write. Without it a panel dropped over a paragraph would
 * leave the paragraph selectable through its background — text a reader can no
 * longer see, in a copy they did not expect it in.
 */
export function recordHit(grid: HitGrid, node: TuiNode, area: Rect, clip: Rect): void {
  const box = intersect(clip, area);
  const right = Math.min(grid.width, box.x + box.width);
  const bottom = Math.min(grid.height, box.y + box.height);
  for (let y = Math.max(0, box.y); y < bottom; y += 1) {
    const row = y * grid.width;
    for (let x = Math.max(0, box.x); x < right; x += 1) {
      grid.nodes[row + x] = node;
      grid.text[row + x] = null;
    }
  }
}

/**
 * Say that `node`'s text occupies `width` columns from `x`, `y`.
 *
 * The two guards are `writeGrapheme`'s, repeated rather than shared, and the
 * reason is what this array means: a cell a clip refused is a cell the node
 * did not write, so it is not a cell the node owns. Deriving that from the
 * write itself would be better, but `writeGrapheme` reports the columns a
 * caller should advance by whether or not it wrote anything — it has to, or a
 * clipped line would come out with its remaining clusters bunched up.
 *
 * `node` may be `null`, which is how the painter takes a cell back for
 * something that is not selectable text — a scrollbar drawn over a line that
 * ran into its column.
 */
export function recordText(
  grid: HitGrid,
  node: TuiNode | null,
  x: number,
  y: number,
  width: number,
  clip: Rect,
): void {
  if (y < clip.y || y >= clip.y + clip.height || y < 0 || y >= grid.height) {
    return;
  }
  if (x < clip.x || x + width > clip.x + clip.width || x < 0 || x + width > grid.width) {
    return;
  }
  const row = y * grid.width;
  for (let column = x; column < x + width; column += 1) {
    grid.text[row + column] = node;
  }
}

/** The topmost node at a cell, or `null` when the pointer is over nothing. */
export function hitAt(grid: HitGrid, x: number, y: number): TuiNode | null {
  if (x < 0 || y < 0 || x >= grid.width || y >= grid.height) {
    return null;
  }
  return grid.nodes[y * grid.width + x];
}

/** The selectable text node at a cell, or `null` when there is none. */
export function textAt(grid: HitGrid, x: number, y: number): TuiNode | null {
  if (x < 0 || y < 0 || x >= grid.width || y >= grid.height) {
    return null;
  }
  return grid.text[y * grid.width + x];
}
