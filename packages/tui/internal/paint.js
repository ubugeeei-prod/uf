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
import {
  Attributes,
  INHERIT,
  PLAIN,
  fillRect,
  intersect,
  parseColor,
  writeGrapheme,
} from "../cells.js";
import type { Selection } from "../selection.js";
import type { HitGrid } from "./hits.js";
import { recordHit, recordText } from "./hits.js";
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
 *
 * `selectable` descends the same way `clip` does, and for the same reason it
 * is a parameter rather than something read back off a node: whether a cell
 * may be selected is a fact about the whole chain above it, and walking up
 * from each `<Text>` to find out would ask the same question of the same
 * ancestors once per leaf. It starts `true`, which is OpenTUI's default for
 * text, and a `selectable={false}` anywhere on the way down turns it off for
 * everything under that node.
 */
export function paint(
  node: TuiNode,
  frame: Frame,
  capabilities: Capabilities,
  clip: Rect,
  hits: HitGrid | null = null,
  selectable: boolean = true,
): void {
  // `hideInstance` — React's for a Suspense fallback and for `<Activity>` —
  // sets `width: 0, height: 0, hidden: true`. The zero size is not enough on
  // its own: `overflow` defaults to `"visible"`, so a child laid out inside a
  // 0x0 box still draws over its edge, and the walk would also record hits for
  // a subtree the reader cannot see. The flag is the thing that says the
  // subtree is not here; read it before anything is drawn.
  if (node.props.hidden === true) {
    return;
  }
  const inherited = selectableOf(node, selectable);
  switch (node.type) {
    case "root":
      for (const child of node.children) {
        paint(child, frame, capabilities, clip, hits, inherited);
      }
      return;
    case "box":
      paintBox(node, frame, capabilities, clip, hits, inherited);
      return;
    case "text":
      paintText(node, frame, clip, hits, inherited);
      return;
    default:
      // A `"chars"` node is only ever reached through its `"text"` parent,
      // which paints it as part of a wrapped line. One outside a `<Text>` has
      // no style, no wrap mode and no line to belong to, so it draws nothing —
      // deliberately, rather than by omission.
      return;
  }
}

/**
 * Whether text under `node` may be selected.
 *
 * A boolean prop wins over what was inherited; anything else — including the
 * prop being absent — leaves the answer where its ancestors put it. Only the
 * direct prop is read, and not `style.selectable`: `style` is where OpenTUI
 * puts the things that *paint* a node, and whether a reader may copy a line
 * out of it is not one of them.
 */
function selectableOf(node: TuiNode, inherited: boolean): boolean {
  const own = node.props.selectable;
  return typeof own === "boolean" ? own : inherited;
}

function paintBox(
  node: TuiNode,
  frame: Frame,
  capabilities: Capabilities,
  clip: Rect,
  hits: HitGrid | null,
  selectable: boolean,
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

  if (node.style.overflow === "scroll") {
    // The children a scrolling box laid out, and only those. The rest were
    // never given a position this frame, so their geometry is from whichever
    // frame last showed them and drawing it would put those rows back on the
    // screen. Reading the range rather than a flag per child is also what
    // keeps the walk proportional to the window: a box holding ten thousand
    // rows is not visited ten thousand times to be told nine thousand nine
    // hundred and seventy-six of them are elsewhere.
    const end = node.scrollFirst + node.scrollCount;
    for (let index = node.scrollFirst; index < end; index += 1) {
      paint(node.children[index], frame, capabilities, childClip, hits, selectable);
    }
  } else {
    for (const child of node.children) {
      paint(child, frame, capabilities, childClip, hits, selectable);
    }
  }

  if (node.style.overflow === "scroll" && node.props.scrollbar === true) {
    // `childClip` rather than `clip`: the bar belongs to this box and must be
    // cut by the same rectangle its rows are.
    paintScrollbar(node, frame, capabilities, childClip, style, hits);
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
  hits: HitGrid | null,
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
    // The bar is drawn after the children, so a line with `wrap="none"` that
    // ran into this column has already claimed it. It is not that line any
    // more, and a selection dragged over the bar must not copy one.
    if (hits != null) {
      recordText(hits, null, column, top + row, 1, clip);
    }
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

function paintText(
  node: TuiNode,
  frame: Frame,
  clip: Rect,
  hits: HitGrid | null,
  selectable: boolean,
): void {
  const own = textStyleFromProps(node.props, ROOT_TEXT_STYLE);
  const runs = textRuns(node, own);
  const lines = wrapRuns(runs, node.width, wrapModeOf(node));
  // The outermost `<Text>` of a nest is the one recorded, because it is the
  // one that paints: `textRuns` has already flattened its children into runs,
  // so a `<Text bold>` inside it never reaches the frame under its own name.
  // That is the right owner anyway — `selectionBg` is inherited like every
  // other text style, and a nested run has no separate existence to select.
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
      if (hits != null && selectable) {
        recordText(hits, node, column, node.y + index, cluster.width, clip);
      }
      column += cluster.width;
    }
  }
}

/**
 * One row of a selection: the cells of it a reader would read across.
 *
 * `from` is after `to` for a row the selection covers but that holds no
 * selectable text — a gap between two paragraphs, the padding of a box, the
 * blank half of a half-filled screen. Those rows are still rows of the
 * selection, which is why they are reported rather than dropped: the text
 * copied out of a selection has a line for each of them, the same way dragging
 * across a blank line in a terminal gives you the blank line.
 */
type SelectedRow = {
  readonly y: number,
  readonly from: number,
  readonly to: number,
};

/**
 * Which cells of each row a selection covers.
 *
 * A row's span runs from its first selectable cell to its last, and *includes
 * whatever is between them*, selectable or not. That is the one rule this
 * module applies twice — once to draw the highlight and once to read the text
 * back out — and it exists because of what the alternative does to a layout.
 * Two `<Text>`s in a row with a gap between them are `left` and `right` on the
 * screen; taking only the cells they own would copy `leftright`, and drawing
 * the highlight only over them would leave a hole in the middle of a selection
 * a reader dragged straight through. Cells before the first and after the last
 * are not part of it: trailing blanks are the shape of the box, not something
 * anyone selected.
 *
 * Spans are snapped outwards onto whole graphemes. A selection that begins on
 * the right-hand cell of a two-column character would otherwise style half of
 * it, and the two halves would then differ in a comparison the diff makes per
 * cell — which is a repaint of a character nobody selected.
 */
function selectedRows(frame: Frame, grid: HitGrid, selection: Selection): Array<SelectedRow> {
  const rows: Array<SelectedRow> = [];
  const top = Math.max(0, selection.start.y);
  const bottom = Math.min(frame.height - 1, selection.end.y);
  for (let y = top; y <= bottom; y += 1) {
    const base = y * frame.width;
    const last = frame.width - 1;
    const left = y === selection.start.y ? Math.max(0, selection.start.x) : 0;
    const right = y === selection.end.y ? Math.min(last, selection.end.x) : last;
    let from = -1;
    let to = -2;
    for (let x = left; x <= right; x += 1) {
      if (grid.text[base + x] != null) {
        if (from < 0) {
          from = x;
        }
        to = x;
      }
    }
    if (from >= 0) {
      while (from > 0 && frame.chars[base + from] === "") {
        from -= 1;
      }
      while (to + 1 < frame.width && frame.chars[base + to + 1] === "") {
        to += 1;
      }
    }
    rows.push({ y, from, to });
  }
  return rows;
}

/**
 * Show a selection in a frame that has already been painted.
 *
 * A pass over the selected rows rather than something the walk above knows
 * about, and that is the whole reason it is cheap and the reason it is
 * correct. A selection is two cells of the *frame*; the tree does not have it
 * and could not apply it without every node asking whether each of its cells
 * is selected. Here the answer is already on the screen.
 *
 * The default is inverse video, toggled rather than set. A terminal with no
 * colour at all still has it — this is `SGR 7`, not a palette entry — and
 * toggling is what makes a selection dragged over something already inverse,
 * such as the cell an `Input` draws its cursor in, show as a hole in the
 * highlight instead of vanishing into it. A `selectionBg` or `selectionFg` on
 * the text, or on anything above it, replaces that with the colours it names
 * and leaves the attributes alone: an application that has said how a
 * selection looks has said it.
 */
export function paintSelection(frame: Frame, grid: HitGrid, selection: Selection): void {
  for (const row of selectedRows(frame, grid, selection)) {
    if (row.from > row.to) {
      // A row of the selection with no selectable text on it. It is a line in
      // what gets copied and nothing at all in what gets drawn.
      continue;
    }
    const base = row.y * frame.width;
    let owner = grid.text[base + row.from];
    let style = selectionStyleOf(owner);
    for (let x = row.from; x <= row.to; x += 1) {
      const at = grid.text[base + x];
      if (at != null && at !== owner) {
        owner = at;
        style = selectionStyleOf(owner);
      }
      const index = base + x;
      if (style.fg === INHERIT && style.bg === INHERIT) {
        frame.attributes[index] ^= Attributes.INVERSE;
        continue;
      }
      if (style.fg !== INHERIT) {
        frame.fg[index] = style.fg;
      }
      if (style.bg !== INHERIT) {
        frame.bg[index] = style.bg;
      }
    }
  }
}

/**
 * The colours a node's selection is drawn in, or `INHERIT` for both.
 *
 * `selectionFg` and `selectionBg` inherit the way `fg` and `bg` do, and
 * independently of each other, so one prop on a panel covers everything inside
 * it. They are resolved by walking *up* from the node that owns the cell,
 * rather than threaded down the paint the way `selectable` is, because the two
 * are asked about at different times: `selectable` decides whether a cell is
 * recorded at all and so is needed for every cell of every frame, while these
 * are needed only for the handful of cells a selection covers — and only when
 * there is one. A walk bounded by the depth of a terminal's tree, taken once
 * per run of one owner, is cheaper than a lookup nothing usually reads.
 */
function selectionStyleOf(node: TuiNode | null): { fg: number, bg: number } {
  let fg = INHERIT;
  let bg = INHERIT;
  let current = node;
  while (current != null && (fg === INHERIT || bg === INHERIT)) {
    if (fg === INHERIT) {
      fg = parseColor(readColor(current.props, ["selectionFg"]));
    }
    if (bg === INHERIT) {
      bg = parseColor(readColor(current.props, ["selectionBg"]));
    }
    current = current.parent;
  }
  return { fg, bg };
}

/**
 * The text a selection covers, as a reader would copy it.
 *
 * Read out of the frame rather than out of the tree, which is what makes it
 * agree with the highlight down to the cell: a wide grapheme contributes its
 * cluster once and its continuation cell contributes the empty string, a line
 * that was wrapped comes back wrapped, and a row that was clipped comes back
 * clipped. Rows are joined top to bottom with `\n`, which is OpenTUI's
 * documented order and the only thing a clipboard can do with two rows.
 */
export function selectionText(frame: Frame, grid: HitGrid, selection: Selection): string {
  const lines: Array<string> = [];
  for (const row of selectedRows(frame, grid, selection)) {
    const base = row.y * frame.width;
    let line = "";
    for (let x = row.from; x <= row.to; x += 1) {
      line += frame.chars[base + x];
    }
    lines.push(line);
  }
  return lines.join("\n");
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
