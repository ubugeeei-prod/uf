// @flow
//
// Turning a laid-out tree into cells.
//
// # Internal to `@uniflowed/tui`
//
// Absent from `package.json#exports`, because line breaking has to be the
// same computation in both passes. Layout asks this module how tall a
// paragraph is and the painter asks it what is on line three; a consumer who
// could call `wrapRuns` with a different mode than the node carries would get
// a frame that disagrees with the layout that made room for it.
//
//
// Layout has already decided where every node is; this decides what is in it.
// The two are separate passes because a node's size is a question about its
// content and a node's appearance is a question about its size, and running
// them together is how a renderer ends up measuring text twice — once to find
// out how tall the box is, and again to draw it.
//
// # Line breaking lives here, not in layout
//
// Wrapping is the one thing both passes need: layout asks "how tall is this
// paragraph at 40 columns" and paint asks "what is on line three". They are
// the same computation with two different questions asked of the answer, so
// it is written once, here, and layout reaches it through the `measure`
// callback a text node carries.
//
// # Painting is destructive, and that is the point
//
// A cell holds one grapheme, so drawing a box's background over the text
// underneath it *erases* that text. This is what a terminal does and what a
// caller means by a background — but it is worth stating, because the obvious
// alternative (compositing, with transparency) is what a browser does, and a
// reader coming from CSS will assume it. A cell has no alpha channel. The
// last writer wins.

import type { BorderStyle, Capabilities } from "../capability.js";
import { borderGlyphs } from "../capability.js";
import type { Frame, Rect, Style } from "../cells.js";
import { INHERIT, PLAIN, fillRect, intersect, parseColor, writeGrapheme } from "../cells.js";
import type { HitGrid } from "./hits.js";
import { recordHit } from "./hits.js";
import type { TuiNode } from "./tree.js";
import { ROOT_TEXT_STYLE, borderOf, textRuns, textStyleFromProps } from "./tree.js";
import type { Grapheme } from "../widths.js";
import { graphemes } from "../widths.js";

/** How a run of text breaks when it does not fit. OpenTUI's three modes. */
export type WrapMode = "word" | "char" | "none";

/** One grapheme, its width, and how it is painted. */
type Cluster = {
  readonly text: string,
  readonly width: number,
  readonly style: Style,
};

/** A laid-out line of clusters, with the columns it occupies. */
type Line = {
  readonly clusters: Array<Cluster>,
  readonly width: number,
};

/**
 * Break styled runs into lines that fit `available` columns.
 *
 * A `\n` always breaks, in every mode: it is the one instruction in the text
 * itself, and a `wrapMode: "none"` that ignored it would turn a three-line
 * message into one line that runs off the screen.
 *
 * `"word"` breaks at the last space that fits and drops that space, which is
 * what makes the next line start at column zero instead of one column in.
 * A word longer than the whole line falls back to breaking mid-word, because
 * the alternative is a line that overflows no matter what, and a URL is a word.
 */
export function wrapRuns(
  runs: $ReadOnlyArray<{ readonly text: string, readonly style: Style }>,
  available: number,
  mode: WrapMode,
): Array<Line> {
  const lines: Array<Line> = [];
  let current: Array<Cluster> = [];
  let width = 0;
  // Where the last space in `current` is, so a word break can rewind to it.
  let lastBreak = -1;

  const flush = () => {
    lines.push({ clusters: current, width });
    current = [];
    width = 0;
    lastBreak = -1;
  };

  for (const run of runs) {
    const segments = run.text.split("\n");
    for (let s = 0; s < segments.length; s += 1) {
      if (s > 0) {
        flush();
      }
      for (const grapheme of graphemes(segments[s])) {
        if (mode !== "none" && available > 0 && width + grapheme.width > available) {
          if (mode === "word" && lastBreak >= 0) {
            const tail = current.slice(lastBreak + 1);
            const head = current.slice(0, lastBreak);
            const headWidth = head.reduce((total, cluster) => total + cluster.width, 0);
            lines.push({ clusters: head, width: headWidth });
            current = tail;
            width = tail.reduce((total, cluster) => total + cluster.width, 0);
            lastBreak = -1;
          } else {
            flush();
          }
        }
        if (grapheme.text === " ") {
          lastBreak = current.length;
        }
        current.push({ text: grapheme.text, width: grapheme.width, style: run.style });
        width += grapheme.width;
      }
    }
  }
  flush();
  return lines;
}

/** The size a text node wants, given the width it may use. */
export function measureText(
  node: TuiNode,
  available: number,
  mode: WrapMode,
): { readonly width: number, readonly height: number } {
  const runs = textRuns(node, textStyleFromProps(node.props, ROOT_TEXT_STYLE));
  const lines = wrapRuns(runs, available, mode);
  let width = 0;
  for (const line of lines) {
    width = Math.max(width, line.width);
  }
  return { width, height: lines.length };
}

/** The wrap mode a text node's props ask for; OpenTUI's default is `"word"`. */
export function wrapModeOf(node: TuiNode): WrapMode {
  const raw = node.props.wrap ?? node.props.wrapMode;
  return raw === "char" || raw === "none" ? raw : "word";
}

/**
 * Draw a whole tree into `frame`.
 *
 * `clip` starts as the frame itself and narrows as the walk descends through
 * boxes that hide their overflow. Nothing is ever written outside it, which is
 * both how `overflow: "hidden"` is implemented and how a child that layout
 * placed off the bottom of an 80×24 terminal fails to corrupt the frame.
 *
 * `hits` is the grid the mouse is routed with, or `null` for a renderer that
 * has no mouse. It is filled here rather than by a second walk because the
 * question it answers — which node was allowed to draw this cell — is the
 * question `clip` is already the answer to; see `hits.js`.
 */
export function paint(
  node: TuiNode,
  frame: Frame,
  capabilities: Capabilities,
  clip: Rect,
  hits: HitGrid | null = null,
): void {
  // A scrolling ancestor decided this subtree is not on screen. Its geometry
  // is deliberately not up to date, so walking into it would draw the last
  // frame's positions on top of this one's.
  if (node.hidden) {
    return;
  }
  switch (node.type) {
    case "root":
      for (const child of node.children) {
        paint(child, frame, capabilities, clip, hits);
      }
      return;
    case "box":
      paintBox(node, frame, capabilities, clip, hits);
      return;
    case "text":
      paintText(node, frame, clip);
      return;
    default:
      // A `"chars"` node is only ever reached through its `"text"` parent,
      // which paints it as part of a wrapped line. One outside a `<Text>` has
      // no style, no wrap mode and no line to belong to, so it draws nothing —
      // deliberately, rather than by omission.
      return;
  }
}

function paintBox(
  node: TuiNode,
  frame: Frame,
  capabilities: Capabilities,
  clip: Rect,
  hits: HitGrid | null,
): void {
  const area = { x: node.x, y: node.y, width: node.width, height: node.height };
  // Before the children, so that a child overwrites its parent — a click on a
  // button inside a panel is a click on the button. A box claims its whole
  // rectangle whether or not it painted anything into it: a box is a region,
  // and one without a background is still the thing a reader is pointing at.
  if (hits != null) {
    recordHit(hits, node, area, clip);
  }
  const background = parseColor(readColor(node.props, ["backgroundColor", "bg"]));
  const style = textStyleFromProps(node.props, PLAIN);
  if (background !== INHERIT) {
    fillRect(frame, area, { fg: style.fg, bg: background, attributes: 0 }, clip);
  }

  const border = borderOf(node.props);
  if (border != null && node.width >= 2 && node.height >= 1) {
    paintBorder(node, frame, capabilities, clip, border, background);
  }

  // `overflow: "hidden"` clips children to what is inside the border and
  // padding. `"visible"` — the default — lets them draw over the border,
  // which is how a badge sits on a box's top edge. `"scroll"` clips like
  // `"hidden"`: a row half in the window has to be half drawn.
  const clipped = node.style.overflow === "hidden" || node.style.overflow === "scroll";
  const childClip = clipped
    ? intersect(clip, {
        x: node.x + node.borderWidth,
        y: node.y + node.borderWidth,
        width: Math.max(0, node.width - node.borderWidth * 2),
        height: Math.max(0, node.height - node.borderWidth * 2),
      })
    : clip;

  for (const child of node.children) {
    paint(child, frame, capabilities, childClip, hits);
  }

  if (node.style.overflow === "scroll" && node.props.scrollbar === true) {
    // `childClip` rather than `clip`: the bar belongs to this box and must be
    // cut by the same rectangle its rows are.
    paintScrollbar(node, frame, capabilities, childClip, style);
  }
}

/**
 * The bar down the right-hand edge of a scrolling box.
 *
 * It goes in the column `ScrollBox` reserved for it by adding one to the box's
 * right padding, which is why wrapped content never reaches it — and why
 * turning the bar off gives that column back to the content instead of leaving
 * a gap. Text with `wrap="none"` can still run into the column, since padding
 * is not a clip anywhere in this renderer; the bar is drawn after the children
 * and wins.
 *
 * Nothing is drawn when everything fits. The bar is only reached when there is
 * more content than window, and a thumb is then always at least one row and
 * never the whole bar — a full-height thumb would say "all of it is showing",
 * which is the one thing that is not true here.
 */
function paintScrollbar(
  node: TuiNode,
  frame: Frame,
  capabilities: Capabilities,
  clip: Rect,
  style: Style,
): void {
  const top = node.scrollViewTop;
  const viewport = node.scrollViewRows;
  const column = node.scrollBarColumn;
  if (viewport <= 0 || node.scrollHeight <= viewport) {
    return;
  }

  const ascii = capabilities.glyphs === "ascii";
  const trackGlyph = ascii ? "|" : "│";
  const thumbGlyph = ascii ? "#" : "█";
  const trackStyle: Style = {
    fg: parseColor(readColor(node.props, ["scrollbarColor", "borderColor"])),
    bg: style.bg,
    attributes: 0,
  };

  const thumb = Math.max(1, Math.round((viewport / node.scrollHeight) * viewport));
  const travel = viewport - thumb;
  const scrolled = node.scrollHeight - viewport;
  const start = scrolled === 0 ? 0 : Math.round((node.scrollOffset / scrolled) * travel);

  for (let row = 0; row < viewport; row += 1) {
    const glyph = row >= start && row < start + thumb ? thumbGlyph : trackGlyph;
    writeGrapheme(frame, column, top + row, glyph, 1, trackStyle, clip);
  }
}

function paintBorder(
  node: TuiNode,
  frame: Frame,
  capabilities: Capabilities,
  clip: Rect,
  border: BorderStyle,
  background: number,
): void {
  const glyphs = borderGlyphs(border, capabilities.glyphs);
  const color = parseColor(readColor(node.props, ["borderColor"]));
  const style: Style = { fg: color, bg: background, attributes: 0 };
  const { x, y, width, height } = node;
  const right = x + width - 1;
  const bottom = y + height - 1;

  for (let column = x + 1; column < right; column += 1) {
    writeGrapheme(frame, column, y, glyphs.top, 1, style, clip);
    if (height > 1) {
      writeGrapheme(frame, column, bottom, glyphs.bottom, 1, style, clip);
    }
  }
  for (let row = y + 1; row < bottom; row += 1) {
    writeGrapheme(frame, x, row, glyphs.left, 1, style, clip);
    writeGrapheme(frame, right, row, glyphs.right, 1, style, clip);
  }
  writeGrapheme(frame, x, y, glyphs.topLeft, 1, style, clip);
  writeGrapheme(frame, right, y, glyphs.topRight, 1, style, clip);
  if (height > 1) {
    writeGrapheme(frame, x, bottom, glyphs.bottomLeft, 1, style, clip);
    writeGrapheme(frame, right, bottom, glyphs.bottomRight, 1, style, clip);
  }

  paintTitle(node, frame, clip, style, "title", "titleAlignment", y);
  if (height > 1) {
    paintTitle(node, frame, clip, style, "bottomTitle", "bottomTitleAlignment", bottom);
  }
}

/**
 * Write a title into a border row.
 *
 * Titles are never wrapped and never widen a box: a title longer than the
 * edge it sits on is truncated, because the alternative is a box whose size
 * depends on a string somebody typed. The available run is the edge minus its
 * two corners.
 */
function paintTitle(
  node: TuiNode,
  frame: Frame,
  clip: Rect,
  style: Style,
  titleProp: string,
  alignProp: string,
  row: number,
): void {
  const raw = node.props[titleProp];
  if (typeof raw !== "string" || raw === "") {
    return;
  }
  const titleStyle: Style = {
    fg:
      parseColor(readColor(node.props, ["titleColor"])) === INHERIT
        ? style.fg
        : parseColor(readColor(node.props, ["titleColor"])),
    bg: style.bg,
    attributes: style.attributes,
  };
  const available = Math.max(0, node.width - 2);
  const clusters: Array<Grapheme> = [];
  let used = 0;
  for (const grapheme of graphemes(raw)) {
    if (used + grapheme.width > available) {
      break;
    }
    clusters.push(grapheme);
    used += grapheme.width;
  }
  const alignment = node.props[alignProp];
  let start = node.x + 1;
  if (alignment === "center") {
    start += Math.floor((available - used) / 2);
  } else if (alignment === "right") {
    start += available - used;
  }
  let column = start;
  for (const grapheme of clusters) {
    writeGrapheme(frame, column, row, grapheme.text, grapheme.width, titleStyle, clip);
    column += grapheme.width;
  }
}

function paintText(node: TuiNode, frame: Frame, clip: Rect): void {
  const own = textStyleFromProps(node.props, ROOT_TEXT_STYLE);
  const runs = textRuns(node, own);
  const lines = wrapRuns(runs, node.width, wrapModeOf(node));
  for (let index = 0; index < lines.length && index < node.height; index += 1) {
    let column = node.x;
    for (const cluster of lines[index].clusters) {
      writeGrapheme(
        frame,
        column,
        node.y + index,
        cluster.text,
        cluster.width,
        cluster.style,
        clip,
      );
      column += cluster.width;
    }
  }
}

function readColor(
  props: { readonly [string]: mixed },
  names: $ReadOnlyArray<string>,
): string | number | void {
  for (const name of names) {
    const direct = props[name];
    if (typeof direct === "string" || typeof direct === "number") {
      return direct;
    }
    const style = props.style;
    if (style != null && typeof style === "object") {
      const nested = style[name];
      if (typeof nested === "string" || typeof nested === "number") {
        return nested;
      }
    }
  }
  return undefined;
}
