// @flow
//
// What a drag over the frame selected: two cells, and the order between them.
//
// A selection here is geometry and nothing else. It is not a range of
// characters inside a string, and it is not a pair of nodes with offsets into
// them — it is the cell the pointer went down on and the cell it has reached,
// in the coordinates every other part of this package already uses.
//
// # Why two points on the screen rather than an offset into the text
//
// The obvious model is the DOM's: a start node and an offset, an end node and
// an offset. It is the wrong one here for the same reason the hit test is a
// grid rather than a walk over the tree. What a reader is selecting is what
// they can *see*, and what they can see is the frame: a row scrolled out of a
// `ScrollBox` has last frame's geometry on it, `overflow: "hidden"` cut a
// child off somewhere its geometry does not admit to, and a box painted later
// erased the text under it. Two screen cells are true about the picture in
// front of the reader; a node and an offset are true about a tree that
// disagrees with it.
//
// It is also what a terminal's own selection is, which matters more than it
// looks: a reader who drags across this renderer's output and a reader who
// drags across `cat`'s output are performing the same gesture, and the second
// one has taught them what to expect from the first.
//
// The cost of the model is stated rather than hidden. A selection survives a
// re-render, because two cells are still two cells; if the content under them
// moved, the selection now covers whatever moved into those cells. That is
// again what a terminal does, and the alternative — dropping the selection on
// every commit — would drop it on the keystroke that scrolled the window a
// reader was selecting from.
//
// # Reading order, not a rectangle
//
// `start` and `end` are the two points sorted by row and then by column, which
// is the order text is read in and the order OpenTUI documents
// `getSelectedText()` as joining in: top to bottom, left to right. A selection
// from the middle of one line to the middle of the line below it therefore
// covers the end of the first line, and not a rectangle standing on the two
// columns. Column selection is a different gesture and this is not it.

/** One cell of the frame, counted from zero at the top left. */
export type SelectionPoint = {
  readonly x: number,
  readonly y: number,
};

/**
 * One selection, as the renderer holds it.
 *
 * There is at most one per renderer, which is OpenTUI's rule and a terminal's:
 * a second selection would have to be shown, and the frame has one way of
 * showing a cell is selected.
 *
 * `anchor` and `focus` are the gesture — where the press landed and where the
 * pointer has reached — and are kept because that is what an application
 * asking "which way is this drag going" needs. `start` and `end` are the same
 * two points in reading order, which is what everything that walks the
 * selection needs, and they are stored rather than recomputed so that no two
 * readers of one selection can sort it differently.
 */
export type Selection = {
  /** The cell the press that began this selection landed on. */
  readonly anchor: SelectionPoint,
  /** The cell the pointer has reached. Moves as the drag continues. */
  readonly focus: SelectionPoint,
  /** The earlier of the two, by row and then by column. */
  readonly start: SelectionPoint,
  /** The later of the two. Both ends are *inside* the selection. */
  readonly end: SelectionPoint,
};

/** Whether `a` is at or before `b` in reading order. */
function atOrBefore(a: SelectionPoint, b: SelectionPoint): boolean {
  return a.y < b.y || (a.y === b.y && a.x <= b.x);
}

/**
 * The selection a drag from `anchor` to `focus` describes.
 *
 * Both ends are inclusive: a press and a release on the same cell select that
 * one cell rather than nothing. A reader who drags across a single character
 * expects to have selected it, and a half-open range would make the shortest
 * possible selection the empty one.
 */
export function selectionBetween(anchor: SelectionPoint, focus: SelectionPoint): Selection {
  const forwards = atOrBefore(anchor, focus);
  return {
    anchor,
    focus,
    start: forwards ? anchor : focus,
    end: forwards ? focus : anchor,
  };
}

/** Whether a cell is inside a selection, in reading order. */
export function selectionContains(selection: Selection, x: number, y: number): boolean {
  const point = { x, y };
  return atOrBefore(selection.start, point) && atOrBefore(point, selection.end);
}
