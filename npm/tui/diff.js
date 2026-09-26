// @flow
//
// The difference between two frames, as the bytes that turn one into the other.
//
// This is the module the package exists for. Everything else — the tree, the
// layout, the painter — could be replaced with something else and the result
// would still be a terminal UI; without this it would be a terminal UI that
// reprints the screen on every keystroke, which is what makes a slow one feel
// slow. The bottleneck in a terminal program is not laying out a hundred
// boxes. It is the bytes handed to a pseudo-terminal, which are copied,
// parsed, and re-rendered by an emulator that is drawing them with a GPU on
// the other side of a pipe.
//
// So the measurement that matters is bytes per frame, and the mechanism is
// this: compare the two frames cell by cell, and emit only the cells that
// differ. Changing one character of a status line in an 80×24 terminal costs
// this renderer about a dozen bytes. A renderer that diffs *lines* — which is
// what React Ink does, and what a naive implementation does — reprints from
// the first changed line to the bottom of the frame, and the same edit costs
// it however much of the screen is below it. `tui.test.js` measures both
// numbers rather than asserting the shape of the claim.
//
// # Two cursor moves cost more than four spaces
//
// A cursor move is `ESC [ row ; col H`, which is six to nine bytes. Two runs
// of changed cells separated by three unchanged ones are therefore cheaper to
// write as one run — paying for three characters that did not need writing —
// than as two runs with a jump between them. `JOIN_GAP` is where that trade
// turns over. It was not there in the first version, and a progress bar whose
// filled cells alternate with empty ones produced one cursor move per cell.
//
// # Colour is emitted only when it changes
//
// The SGR sequence for a truecolour foreground is nineteen bytes, which is
// longer than most of the runs it introduces. Tracking the terminal's current
// style across the whole frame and re-emitting only on a change turns a
// coloured row from nineteen bytes per cell into nineteen bytes per *run of
// one colour*, which for a syntax-highlighted line is an order of magnitude.

import type { Capabilities, ColorLevel } from "./capability.js";
import type { Color, Frame, Style } from "./cells.js";
import { Attributes, INHERIT, PLAIN, sameSize } from "./cells.js";

/**
 * How many unchanged cells are worth writing through rather than jumping over.
 *
 * Four: a cursor move is at least six bytes and a plain unchanged cell is one,
 * so the break-even is around six — but the cells being written through also
 * have to carry their style, so the real figure is lower. Four is measured on
 * the frames this package's own tests produce rather than derived.
 */
const JOIN_GAP = 4;

/** What one diff produced, and what it cost. */
export type Update = {
  /** The bytes to write to the terminal. Empty when nothing changed. */
  readonly output: string,
  /** How many cells were re-sent. The number the performance claim is about. */
  readonly cells: number,
};

/**
 * Move the cursor to a zero-based cell.
 *
 * Terminals count rows and columns from one, and every off-by-one bug in a
 * renderer's first week is this one. The escape is written as `\u001b` rather
 * than as a literal control byte because a literal one is invisible in every
 * editor and diff, which is how one gets deleted.
 */
const moveTo = (x: number, y: number): string => `\u001b[${y + 1};${x + 1}H`;

/** Reset every attribute and colour. */
const RESET = "\u001b[0m";

/**
 * The bytes that turn `previous` into `next`.
 *
 * A `null` previous frame, or one of a different size, means a full repaint:
 * the terminal was just entered or has just been resized, and there is nothing
 * on it this renderer can claim to know.
 */
export function diffFrames(
  previous: Frame | null,
  next: Frame,
  capabilities: Capabilities,
): Update {
  const full = previous == null || !sameSize(previous, next);
  let output = "";
  let cells = 0;
  let current: Style = PLAIN;
  let styled = false;

  for (let y = 0; y < next.height; y += 1) {
    let x = 0;
    while (x < next.width) {
      if (!full && !changed(previous, next, y * next.width + x)) {
        x += 1;
        continue;
      }
      // The end of this run: the last changed cell, plus any unchanged cells
      // close enough behind it that jumping over them would cost more.
      let end = x;
      let scan = x;
      while (scan < next.width) {
        if (full || changed(previous, next, y * next.width + scan)) {
          end = scan;
          scan += 1;
        } else {
          let gap = 0;
          while (scan + gap < next.width && !changedOrFull(full, previous, next, y, scan + gap)) {
            gap += 1;
          }
          if (gap > JOIN_GAP || scan + gap >= next.width) {
            break;
          }
          scan += gap;
        }
      }

      output += moveTo(x, y);
      for (let column = x; column <= end; column += 1) {
        const index = y * next.width + column;
        const character = next.chars[index];
        if (character === "") {
          // A continuation cell. Its cluster was emitted by the cell to its
          // left, and the terminal has already moved the cursor over it.
          continue;
        }
        const style: Style = {
          fg: next.fg[index],
          bg: next.bg[index],
          attributes: next.attributes[index],
        };
        if (!sameStyle(style, current)) {
          const sequence = sgr(style, capabilities.color);
          if (sequence !== "") {
            output += sequence;
            styled = true;
          } else if (styled) {
            output += RESET;
            styled = false;
          }
          current = style;
        }
        output += character;
        cells += 1;
      }
      x = end + 1;
    }
  }

  if (styled) {
    output += RESET;
  }
  return { output, cells };
}

const changedOrFull = (
  full: boolean,
  previous: Frame | null,
  next: Frame,
  y: number,
  x: number,
): boolean => full || changed(previous, next, y * next.width + x);

function changed(previous: Frame | null, next: Frame, index: number): boolean {
  if (previous == null) {
    return true;
  }
  return (
    previous.chars[index] !== next.chars[index] ||
    previous.fg[index] !== next.fg[index] ||
    previous.bg[index] !== next.bg[index] ||
    previous.attributes[index] !== next.attributes[index]
  );
}

const sameStyle = (a: Style, b: Style): boolean =>
  a.fg === b.fg && a.bg === b.bg && a.attributes === b.attributes;

/**
 * The escape sequence that selects `style`.
 *
 * Empty at `"none"`, and that is the whole of the no-colour answer: a terminal
 * that cannot carry colour also cannot carry bold or underline, because both
 * are the same SGR mechanism and a `TERM=dumb` terminal prints them as
 * literal text. The frame's *shape* still arrives — the layout, the borders in
 * their ASCII vocabulary, the text — which is the part a reader needs.
 */
export function sgr(style: Style, level: ColorLevel): string {
  if (level === "none") {
    return "";
  }
  const parts: Array<string> = ["0"];
  const attributes = style.attributes;
  if (attributes & Attributes.BOLD) parts.push("1");
  if (attributes & Attributes.DIM) parts.push("2");
  if (attributes & Attributes.ITALIC) parts.push("3");
  if (attributes & Attributes.UNDERLINE) parts.push("4");
  if (attributes & Attributes.BLINK) parts.push("5");
  if (attributes & Attributes.INVERSE) parts.push("7");
  if (attributes & Attributes.STRIKETHROUGH) parts.push("9");
  if (style.fg !== INHERIT) {
    parts.push(colorParts(style.fg, level, false));
  }
  if (style.bg !== INHERIT) {
    parts.push(colorParts(style.bg, level, true));
  }
  if (parts.length === 1) {
    return "";
  }
  return `\u001b[${parts.join(";")}m`;
}

/**
 * One colour, at the depth this terminal has.
 *
 * The downgrade is deliberate rather than a fallback to "no colour at all". A
 * theme written in 24-bit values still has to mean something on a
 * sixteen-colour terminal, and the nearest colour in the smaller palette is a
 * far better answer than none — this is the same ladder
 * `crates/uf_term/src/style.rs` walks for the CLI's own output.
 */
function colorParts(color: Color, level: ColorLevel, background: boolean): string {
  const r = (color >> 16) & 0xff;
  const g = (color >> 8) & 0xff;
  const b = color & 0xff;
  if (level === "truecolor") {
    return `${background ? 48 : 38};2;${r};${g};${b}`;
  }
  if (level === "ansi256") {
    return `${background ? 48 : 38};5;${cube(r, g, b)}`;
  }
  const base = nearestBasic(r, g, b);
  const offset = background ? 40 : 30;
  return base < 8 ? `${offset + base}` : `${offset + 60 + (base - 8)}`;
}

/** The 256-colour index nearest to an RGB triple. */
function cube(r: number, g: number, b: number): number {
  // The 24-step grey ramp is a better match than the colour cube whenever the
  // three channels are close, and grey text is common enough — every dimmed
  // hint in every CLI — that getting it wrong is visible.
  if (Math.abs(r - g) < 8 && Math.abs(g - b) < 8) {
    if (r < 8) return 16;
    if (r > 248) return 231;
    return 232 + Math.round(((r - 8) / 247) * 24);
  }
  const step = (value: number) => Math.round((value / 255) * 5);
  return 16 + 36 * step(r) + 6 * step(g) + step(b);
}

/** The 16 base colours, as RGB, in SGR order. */
const BASIC: $ReadOnlyArray<[number, number, number]> = [
  [0, 0, 0],
  [205, 0, 0],
  [0, 205, 0],
  [205, 205, 0],
  [0, 0, 238],
  [205, 0, 205],
  [0, 205, 205],
  [229, 229, 229],
  [127, 127, 127],
  [255, 0, 0],
  [0, 255, 0],
  [255, 255, 0],
  [92, 92, 255],
  [255, 0, 255],
  [0, 255, 255],
  [255, 255, 255],
];

/** The base colour closest to an RGB triple, by squared distance. */
function nearestBasic(r: number, g: number, b: number): number {
  let best = 0;
  let bestDistance = Number.MAX_SAFE_INTEGER;
  for (let index = 0; index < BASIC.length; index += 1) {
    const [cr, cg, cb] = BASIC[index];
    const distance = (cr - r) ** 2 + (cg - g) ** 2 + (cb - b) ** 2;
    if (distance < bestDistance) {
      bestDistance = distance;
      best = index;
    }
  }
  return best;
}
