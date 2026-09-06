// @flow
//
// The cell grid: what a frame *is*.
//
// A terminal is a rectangle of cells, and this module is the only description
// of one in the package. Everything above it produces a frame — layout decides
// where boxes go, `internal/paint.js` fills them in — and everything below it consumes
// one: `diff.js` compares two frames and writes the difference, the test
// renderer reads a frame back as text. Neither end knows how a cell is stored,
// which is what let the storage change once already without either end
// noticing.
//
// # Why parallel arrays rather than objects
//
// The obvious representation is `Array<{ char, fg, bg, attributes }>`, and it
// was the first one. It is also the representation that makes the diff — the
// operation this package exists to make cheap — allocate: comparing two frames
// of a 200×60 terminal means 12,000 property loads on 12,000 objects that the
// garbage collector has to keep alive between frames, and a frame is produced
// on every keystroke.
//
// So a frame is four arrays of the same length: three typed, one not. The
// three numeric ones make the common comparison — "is this cell unchanged?" —
// three unboxed integer loads, and the string array carries the one field that
// cannot be a number, because a cell holds a *grapheme cluster* and not a code
// point. A family emoji is a dozen scalars in one cell.
//
// # The empty string means something
//
// `chars[i] === ""` is not a blank cell. A blank cell is `" "`. The empty
// string is a **continuation cell**: the right-hand column of a two-column
// grapheme whose cluster is stored in the cell to its left. The distinction
// exists because a terminal advances its cursor two columns for `界` and
// writing anything into the second of them corrupts the first, so both the
// painter and the diff have to know that a cell is not independently
// writable. Losing this distinction is the bug that makes a table of Japanese
// paths look fine until one row contains an emoji.

import { graphemeWidth } from "./widths.js";

/**
 * A colour, as a packed `0xRRGGBB`, or `INHERIT`.
 *
 * Colours are numbers rather than strings or objects because they sit in a
 * typed array beside every cell, and because the comparison the diff performs
 * a million times a second is `fg[i] === fg[i]`.
 */
export type Color = number;

/**
 * "Whatever the terminal's default is."
 *
 * Distinct from black: a reader with a light terminal theme and a renderer
 * that resolved `INHERIT` to `0x000000` gets black text that stops being
 * readable the moment the reader switches themes, and the terminal's own
 * default is the only value that follows them.
 */
export const INHERIT: Color = -1;

/**
 * Text attributes, as a bit mask.
 *
 * The bits are OpenTUI's `TextAttributes`, in its order, so a value that
 * crosses between the two libraries means the same thing.
 */
export const Attributes = {
  NONE: 0,
  BOLD: 1,
  DIM: 2,
  ITALIC: 4,
  UNDERLINE: 8,
  BLINK: 16,
  INVERSE: 32,
  STRIKETHROUGH: 64,
};

/** How one cell is painted. */
export type Style = {
  /** Foreground colour, or `INHERIT`. */
  readonly fg: Color,
  /** Background colour, or `INHERIT`. */
  readonly bg: Color,
  /** A mask of `Attributes`. */
  readonly attributes: number,
};

/** The style of a cell nobody has painted. */
export const PLAIN: Style = { fg: INHERIT, bg: INHERIT, attributes: Attributes.NONE };

/**
 * The sixteen colours every terminal has, by the names people write.
 *
 * The values are the widely-used xterm defaults rather than a standard,
 * because there is no standard: a terminal is free to render `red` as whatever
 * its theme says, and it will. These exist so that a caller who writes
 * `fg="red"` gets a colour on a truecolour terminal that a reader recognises,
 * and so that the downgrade back to `SGR 31` on a sixteen-colour terminal
 * lands on the same name it started as.
 */
const NAMED: { [string]: Color } = {
  black: 0x000000,
  red: 0xcd0000,
  green: 0x00cd00,
  yellow: 0xcdcd00,
  blue: 0x0000ee,
  magenta: 0xcd00cd,
  cyan: 0x00cdcd,
  white: 0xe5e5e5,
  gray: 0x7f7f7f,
  grey: 0x7f7f7f,
  brightblack: 0x7f7f7f,
  brightred: 0xff0000,
  brightgreen: 0x00ff00,
  brightyellow: 0xffff00,
  brightblue: 0x5c5cff,
  brightmagenta: 0xff00ff,
  brightcyan: 0x00ffff,
  brightwhite: 0xffffff,
};

/**
 * Read a colour written as a name, a hex string, or a packed number.
 *
 * Anything unrecognised resolves to `INHERIT` rather than raising. A colour is
 * decoration: a typo in one should leave the interface readable in the
 * terminal's own colours, not stop the program that was drawing it.
 */
export function parseColor(value: string | number | void | null): Color {
  if (value == null) {
    return INHERIT;
  }
  if (typeof value === "number") {
    return Number.isFinite(value) ? value & 0xffffff : INHERIT;
  }
  const text = value.trim().toLowerCase();
  if (text.startsWith("#")) {
    const digits = text.slice(1);
    if (digits.length === 3) {
      const r = Number.parseInt(digits[0] + digits[0], 16);
      const g = Number.parseInt(digits[1] + digits[1], 16);
      const b = Number.parseInt(digits[2] + digits[2], 16);
      return Number.isNaN(r + g + b) ? INHERIT : (r << 16) | (g << 8) | b;
    }
    if (digits.length === 6) {
      const packed = Number.parseInt(digits, 16);
      return Number.isNaN(packed) ? INHERIT : packed;
    }
    return INHERIT;
  }
  return NAMED[text] ?? INHERIT;
}

/**
 * One rendered frame.
 *
 * Mutable on purpose. A frame is filled in by one painter pass and then read
 * by one diff, and copying it to keep it immutable would double the allocation
 * this representation exists to avoid. The rule that keeps that safe is that a
 * frame belongs to exactly one owner at a time: the painter owns it until
 * `paint()` returns, the renderer owns it afterwards and never writes again.
 */
export type Frame = {
  /** Columns. */
  readonly width: number,
  /** Rows. */
  readonly height: number,
  /** One grapheme cluster per cell, or `""` for a wide cluster's second cell. */
  readonly chars: Array<string>,
  /** Foreground per cell. */
  readonly fg: Int32Array,
  /** Background per cell. */
  readonly bg: Int32Array,
  /** Attribute mask per cell. */
  readonly attributes: Uint8Array,
};

/** A rectangle in frame coordinates; `x`/`y` are the top-left cell. */
export type Rect = {
  readonly x: number,
  readonly y: number,
  readonly width: number,
  readonly height: number,
};

/** A frame of blanks, `width` by `height`. */
export function createFrame(width: number, height: number): Frame {
  const size = Math.max(0, width * height);
  const chars = new Array(size);
  for (let i = 0; i < size; i += 1) {
    chars[i] = " ";
  }
  return {
    width,
    height,
    chars,
    fg: new Int32Array(size).fill(INHERIT),
    bg: new Int32Array(size).fill(INHERIT),
    attributes: new Uint8Array(size),
  };
}

/** Whether two frames describe the same rectangle. */
export function sameSize(a: Frame, b: Frame): boolean {
  return a.width === b.width && a.height === b.height;
}

/**
 * Blank the cell at `index`, and any cell its content spans.
 *
 * A wide grapheme owns two cells and only one of them holds the cluster, so
 * clearing "the cell at x" is not a single-index operation. Overwriting the
 * *second* half of `界` and leaving the first half in place leaves a terminal
 * printing half a character it has no way to draw; overwriting the first half
 * and leaving the continuation marker leaves the diff convinced the second
 * column is not writable. Both were bugs before this existed, and both were
 * only visible with a non-ASCII string in the frame.
 */
function clearSpan(frame: Frame, index: number): void {
  const row = Math.floor(index / frame.width);
  let start = index;
  if (frame.chars[index] === "") {
    // A continuation cell: its cluster is to the left, on the same row.
    start = index - 1;
    if (start < row * frame.width) {
      start = index;
    }
  }
  const width = frame.chars[start] === "" ? 1 : graphemeWidth(frame.chars[start]);
  for (let i = start; i < start + Math.max(1, width) && i < (row + 1) * frame.width; i += 1) {
    frame.chars[i] = " ";
  }
}

/**
 * Write one grapheme cluster at `x`, `y`, in `style`.
 *
 * Returns the number of columns consumed, which is what a caller advances by:
 * zero when the write fell outside the frame or outside `clip`, one or two
 * otherwise. A two-column cluster that would straddle the right edge is
 * written as a space instead of being split, because half of a wide character
 * is not a character.
 */
export function writeGrapheme(
  frame: Frame,
  x: number,
  y: number,
  cluster: string,
  width: number,
  style: Style,
  clip: Rect,
): number {
  if (y < clip.y || y >= clip.y + clip.height || y < 0 || y >= frame.height) {
    return width;
  }
  if (x < clip.x || x + width > clip.x + clip.width || x < 0 || x + width > frame.width) {
    return width;
  }
  const index = y * frame.width + x;
  clearSpan(frame, index);
  if (width === 2) {
    clearSpan(frame, index + 1);
  }
  frame.chars[index] = cluster;
  frame.fg[index] = style.fg;
  frame.bg[index] = style.bg;
  frame.attributes[index] = style.attributes;
  if (width === 2) {
    frame.chars[index + 1] = "";
    frame.fg[index + 1] = style.fg;
    frame.bg[index + 1] = style.bg;
    frame.attributes[index + 1] = style.attributes;
  }
  return width;
}

/**
 * Fill a rectangle with `style`, leaving the characters in it blank.
 *
 * Used for a box's background. It clears rather than preserving what is under
 * it: a background is opaque, and a box drawn over another box's text has to
 * hide that text or the terminal shows both.
 */
export function fillRect(frame: Frame, area: Rect, style: Style, clip: Rect): void {
  const left = Math.max(area.x, clip.x, 0);
  const top = Math.max(area.y, clip.y, 0);
  const right = Math.min(area.x + area.width, clip.x + clip.width, frame.width);
  const bottom = Math.min(area.y + area.height, clip.y + clip.height, frame.height);
  for (let y = top; y < bottom; y += 1) {
    for (let x = left; x < right; x += 1) {
      const index = y * frame.width + x;
      clearSpan(frame, index);
      frame.chars[index] = " ";
      frame.fg[index] = style.fg;
      frame.bg[index] = style.bg;
      frame.attributes[index] = style.attributes;
    }
  }
}

/** The intersection of two rectangles, empty when they do not overlap. */
export function intersect(a: Rect, b: Rect): Rect {
  const x = Math.max(a.x, b.x);
  const y = Math.max(a.y, b.y);
  const right = Math.min(a.x + a.width, b.x + b.width);
  const bottom = Math.min(a.y + a.height, b.y + b.height);
  return { x, y, width: Math.max(0, right - x), height: Math.max(0, bottom - y) };
}

/**
 * One row of a frame, as the text a reader would see.
 *
 * Continuation cells contribute nothing, because their cluster was already
 * emitted by the cell to their left. Trailing blanks are kept: a test that
 * asserts on a row is asserting on a rectangle, and trimming would make
 * "painted a space here" and "painted nothing here" indistinguishable.
 */
export function frameRow(frame: Frame, y: number): string {
  let out = "";
  for (let x = 0; x < frame.width; x += 1) {
    out += frame.chars[y * frame.width + x];
  }
  return out;
}

/** Every row of a frame, newline separated. Snapshots and `toContain` read this. */
export function frameText(frame: Frame): string {
  const rows = [];
  for (let y = 0; y < frame.height; y += 1) {
    rows.push(frameRow(frame, y));
  }
  return rows.join("\n");
}
