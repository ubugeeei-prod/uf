// @flow
//
// What a `Select`, a `TabSelect` and a `Textarea` draw, and how large they are.
//
// # Internal to `@uniflowed/tui`
//
// Absent from `package.json#exports`, for the reason `paint.js` is: the
// `Textarea` component moves its cursor up and down by the lines this module
// breaks its text into, and the painter draws the same lines. A consumer who
// could call {@link editLines} with a width the node was not given would move a
// cursor through lines nobody can see.
//
// # Why these three are painted, and not composed out of `Box` and `Text`
//
// `components.js` says most of what OpenTUI offers beyond its first four
// components is "those plus state", and for most of it that is true. For these three it is
// not, for one reason: what they show depends on how large layout made them.
//
// A `Select` keeps its selected row in the middle of the rows it has room for,
// so the first row it draws is a function of its height. A `TabSelect` shows as
// many tabs as fit across it, so the first tab it draws is a function of its
// width. A `Textarea` wraps at its width and scrolls to keep the cursor in its
// height. A component knows none of those while it renders — layout has not
// run yet — and a component that rendered twice to find out would draw one
// wrong frame every time its size changed.
//
// So each is one node, a box with nothing inside it, and this module draws
// into it after layout has sized it. That is also exactly what OpenTUI does:
// its `SelectRenderable`, `TabSelectRenderable` and `TextareaRenderable` are
// renderables that fill their own frame buffer, not trees of boxes. The
// drawing below follows theirs cell for cell — the `▶ ` in front of the
// selected row, the name one column in and the description under it, the `▬`
// under a selected tab, the `‹` and `›` when there are more tabs than fit —
// and every place it differs says so beside the difference.
//
// The React component in `components.js` still owns the state — which row is
// selected, what the text is and where its cursor sits — because that is what
// React is for and what an application reads. What lives here is only what
// depends on the size: the window, and the lines.

import type { Capabilities } from "../capability.js";
import type { Color, Frame, Rect, Style } from "../cells.js";
import { Attributes, INHERIT, fillRect, intersect, parseColor, writeGrapheme } from "../cells.js";
import type { HitGrid } from "./hits.js";
import { recordText } from "./hits.js";
import type { TuiNode, Widget } from "./tree.js";
import { graphemeWidth } from "../widths.js";

/** One entry in a `Select` or a `TabSelect`: OpenTUI's `SelectOption`. */
export type SelectOption = {
  readonly name: string,
  readonly description: string,
  readonly value?: mixed,
};

/** A `Textarea`'s wrap modes, which are OpenTUI's. */
export type EditWrapMode = "word" | "char" | "none";

// OpenTUI's defaults for the colours that say which item is selected. They are
// fixed pairs — a dark slate behind a yellow name — rather than the terminal's
// own colours, because "selected" has to be visible on a terminal whose theme
// nobody here knows, and a pair that is legible against itself is legible on
// any background.
const SELECTED_BACKGROUND = 0x334455;
const SELECTED_TEXT = 0xffff00;
const DESCRIPTION = 0x888888;
const SELECTED_DESCRIPTION = 0xcccccc;
const SCROLL_INDICATOR = 0x666666;
const SCROLL_ARROWS = 0xaaaaaa;
const PLACEHOLDER = 0x666666;

/** OpenTUI's default tab width, in cells. */
export const DEFAULT_TAB_WIDTH = 20;

const SEGMENTER = new Intl.Segmenter(undefined, { granularity: "grapheme" });

/** One grapheme of a `Textarea`'s text, and where it is in the string. */
export type EditCell = {
  readonly text: string,
  readonly width: number,
  /** The string offset it starts at. */
  readonly at: number,
};

/**
 * One line of a `Textarea` as it is drawn.
 *
 * `start` and `end` are string offsets, `end` exclusive. `wrapped` is `true`
 * when the logical line goes on in the next visual line rather than ending
 * here, which is the one thing a cursor needs to know about a line besides its
 * cells: a cursor at `end` of a wrapped line is drawn at the start of the next
 * one, so "the end of this line" has to mean the last cell instead.
 */
export type EditLine = {
  readonly start: number,
  readonly end: number,
  readonly cells: $ReadOnlyArray<EditCell>,
  readonly width: number,
  readonly wrapped: boolean,
};

/**
 * Break a `Textarea`'s text into the lines it is drawn in.
 *
 * The same three modes as `Text`, with one difference that editing forces:
 * `wrapRuns` in `paint.js` drops the space a word wrap broke at, and this keeps
 * it at the end of the line it broke. A line of `Text` is read, so the space is
 * nothing; a line of a `Textarea` has a cursor in it, and a cursor has to be
 * able to stand on every character of the text — including that one, which a
 * reader deletes by putting the cursor after it and pressing Backspace.
 *
 * A width of zero or less means "not laid out yet", and does not wrap.
 */
export function editLines(
  text: string,
  width: number,
  mode: EditWrapMode,
): $ReadOnlyArray<EditLine> {
  const lines: Array<EditLine> = [];
  const wraps = mode !== "none" && width > 0;
  let offset = 0;
  for (const logical of text.split("\n")) {
    let current: Array<EditCell> = [];
    let used = 0;
    // Where the next line begins when `current` is empty: the offset a line
    // that holds nothing is at.
    let from = offset;
    let lastSpace = -1;
    const push = (cells: Array<EditCell>, wrapped: boolean) => {
      const last = cells[cells.length - 1];
      const start = cells.length > 0 ? cells[0].at : from;
      const end = last != null ? last.at + last.text.length : start;
      let total = 0;
      for (const cell of cells) {
        total += cell.width;
      }
      lines.push({ start, end, cells, width: total, wrapped });
      from = end;
    };
    for (const segment of SEGMENTER.segment(logical)) {
      const cell = {
        text: segment.segment,
        width: graphemeWidth(segment.segment),
        at: offset + segment.index,
      };
      if (wraps && used + cell.width > width && current.length > 0) {
        if (mode === "word" && lastSpace >= 0 && cell.text !== " ") {
          const head = current.slice(0, lastSpace + 1);
          current = current.slice(lastSpace + 1);
          push(head, true);
          used = current.reduce((total, each) => total + each.width, 0);
        }
        lastSpace = -1;
        // A word longer than the whole line breaks mid-word, as it does in a
        // `Text`: the alternative is a line that overflows whatever happens.
        if (used + cell.width > width && current.length > 0) {
          push(current, true);
          current = [];
          used = 0;
        }
      }
      current.push(cell);
      used += cell.width;
      if (cell.text === " ") {
        lastSpace = current.length - 1;
      }
    }
    push(current, false);
    offset += logical.length + 1;
  }
  return lines;
}

/** Which visual line a string offset is drawn on, and in which column. */
export function locate(
  lines: $ReadOnlyArray<EditLine>,
  offset: number,
): { readonly row: number, readonly column: number } {
  // The last line that starts at or before the offset. Two lines of one
  // wrapped paragraph share a boundary — one ends where the next begins — and
  // a cursor on that boundary is drawn at the start of the second, which is
  // where the character it is in front of is.
  let row = 0;
  for (let index = 0; index < lines.length; index += 1) {
    if (lines[index].start <= offset) {
      row = index;
    } else {
      break;
    }
  }
  let column = 0;
  for (const cell of lines[row]?.cells ?? []) {
    if (cell.at >= offset) {
      break;
    }
    column += cell.width;
  }
  return { row, column };
}

/**
 * The offset on `line` closest to `column`, which is where a cursor moving up
 * or down onto it lands.
 *
 * A column in the middle of a two-column character lands in front of it. A
 * column past the end lands at the end — or, on a line that wraps, in front of
 * its last character, because the end of a wrapped line is drawn on the next.
 */
export function offsetAt(line: EditLine, column: number): number {
  let at = 0;
  for (const cell of line.cells) {
    if (at + cell.width > column) {
      return cell.at;
    }
    at += cell.width;
  }
  return lineEnd(line);
}

/** The furthest a cursor can go along a visual line and still be drawn on it. */
export function lineEnd(line: EditLine): number {
  const last = line.cells[line.cells.length - 1];
  return line.wrapped && last != null ? last.at : line.end;
}

/**
 * The inside of a node: its border box less its border and its padding.
 *
 * Read by the painter to know where to draw, and by `Textarea` — through the
 * node a ref hands it — to know how wide its lines are when a key moves the
 * cursor up or down. Layout resolved the same four numbers; they are read back
 * off the style rather than kept, because the style is what layout read them
 * from.
 */
export function contentBox(node: TuiNode): Rect {
  const style = node.style;
  const all = style.padding ?? 0;
  const top = (style.paddingTop ?? all) + node.borderWidth;
  const right = (style.paddingRight ?? all) + node.borderWidth;
  const bottom = (style.paddingBottom ?? all) + node.borderWidth;
  const left = (style.paddingLeft ?? all) + node.borderWidth;
  return {
    x: node.x + left,
    y: node.y + top,
    width: Math.max(0, node.width - left - right),
    height: Math.max(0, node.height - top - bottom),
  };
}

/** A colour prop, or `fallback` when it is absent or unreadable. */
function colorOf(node: TuiNode, name: string, fallback: Color): Color {
  const raw = node.props[name];
  if (typeof raw === "string" || typeof raw === "number") {
    const parsed = parseColor(raw);
    return parsed === INHERIT && raw !== "transparent" ? fallback : parsed;
  }
  return fallback;
}

function flag(node: TuiNode, name: string, fallback: boolean): boolean {
  const raw = node.props[name];
  return typeof raw === "boolean" ? raw : fallback;
}

function count(node: TuiNode, name: string, fallback: number): number {
  const raw = node.props[name];
  return typeof raw === "number" && Number.isFinite(raw) ? Math.max(0, Math.floor(raw)) : fallback;
}

function optionsOf(node: TuiNode): $ReadOnlyArray<SelectOption> {
  const raw = node.props.options;
  // The component typed these; a node is only ever given them by it.
  return Array.isArray(raw) ? (raw as $FlowFixMe) : [];
}

function selectedOf(node: TuiNode, length: number): number {
  const raw = count(node, "selectedIndex", 0);
  return length > 0 ? Math.min(raw, length - 1) : 0;
}

function widthOf(text: string): number {
  let width = 0;
  for (const segment of SEGMENTER.segment(text)) {
    width += graphemeWidth(segment.segment);
  }
  return width;
}

/**
 * Write `text` from `x`, one grapheme at a time.
 *
 * `bg` of `undefined` keeps whatever background the cell already has, which is
 * what OpenTUI's `drawText` does when it is given no background: a name drawn
 * over a selected row's fill stays on that fill.
 */
function drawText(
  frame: Frame,
  x: number,
  y: number,
  text: string,
  fg: Color,
  bg: Color | void,
  clip: Rect,
): void {
  let column = x;
  for (const segment of SEGMENTER.segment(text)) {
    const width = graphemeWidth(segment.segment);
    if (width === 0) {
      continue;
    }
    let background = bg;
    if (background === undefined) {
      background =
        column >= 0 && column < frame.width && y >= 0 && y < frame.height
          ? frame.bg[y * frame.width + column]
          : INHERIT;
    }
    writeGrapheme(
      frame,
      column,
      y,
      segment.segment,
      width,
      { fg, bg: background, attributes: Attributes.NONE },
      clip,
    );
    column += width;
  }
}

/**
 * `text`, cut to `max` columns with an ellipsis when it does not fit.
 *
 * OpenTUI counts UTF-16 code units here; this counts columns, which is what
 * "fits" means on a terminal and the same thing for the ASCII OpenTUI's
 * examples use.
 */
function truncate(text: string, max: number, ellipsis: string): string {
  if (widthOf(text) <= max) {
    return text;
  }
  let out = "";
  let used = 0;
  for (const segment of SEGMENTER.segment(text)) {
    const width = graphemeWidth(segment.segment);
    if (used + width > max - 1) {
      break;
    }
    out += segment.segment;
    used += width;
  }
  return out + ellipsis;
}

/** Rows one `Select` item takes: its name, its description, and the spacing. */
export function selectLinesPerItem(node: TuiNode): number {
  return (flag(node, "showDescription", true) ? 2 : 1) + count(node, "itemSpacing", 0);
}

/**
 * The first item a window of `visible` items shows, with `selected` in it.
 *
 * OpenTUI's rule for both selects: the selection sits in the middle of the
 * window, and the window stops at either end of the list rather than showing
 * empty rows past it.
 */
function windowStart(selected: number, visible: number, length: number): number {
  return Math.max(0, Math.min(selected - Math.floor(visible / 2), length - visible));
}

/**
 * The size a widget wants when nothing else decides it.
 *
 * A `Select` wants every item — OpenTUI's would be as tall as its style says
 * and no taller, but a box here that is given no height is as tall as its
 * content, and a list that shows all its items is the useful reading of that.
 * Give it a height and it scrolls. A `TabSelect` is always as tall as its rows,
 * as OpenTUI's is. A `Textarea` is as tall as its text, and at least one line.
 */
export function measureWidget(
  node: TuiNode,
  widget: Widget,
  availableWidth: number,
): { readonly width: number, readonly height: number } {
  if (widget === "select") {
    const options = optionsOf(node);
    const indicator = flag(node, "showSelectionIndicator", true) ? 2 : 0;
    let width = 0;
    for (const option of options) {
      width = Math.max(
        width,
        1 + indicator + widthOf(option.name),
        1 + indicator + widthOf(option.description),
      );
    }
    return { width, height: options.length * selectLinesPerItem(node) };
  }
  if (widget === "tab-select") {
    const options = optionsOf(node);
    return {
      width: options.length * Math.max(1, count(node, "tabWidth", DEFAULT_TAB_WIDTH)),
      height: tabSelectHeight(
        flag(node, "showUnderline", true),
        flag(node, "showDescription", true),
      ),
    };
  }
  const text = typeof node.props.value === "string" ? node.props.value : "";
  const placeholder = typeof node.props.placeholder === "string" ? node.props.placeholder : "";
  const lines = editLines(text === "" ? placeholder : text, availableWidth, wrapOf(node));
  let width = 0;
  for (const line of lines) {
    width = Math.max(width, line.width);
  }
  // One column more than the text, so a cursor after the last character has a
  // cell to stand in rather than being clipped off the edge.
  return { width: width + 1, height: Math.max(1, lines.length) };
}

/** A `TabSelect`'s height: the tabs, the underline, and the description. */
export function tabSelectHeight(showUnderline: boolean, showDescription: boolean): number {
  return 1 + (showUnderline ? 1 : 0) + (showDescription ? 1 : 0);
}

function wrapOf(node: TuiNode): EditWrapMode {
  const raw = node.props.wrapMode;
  return raw === "char" || raw === "none" ? raw : "word";
}

/**
 * Draw a widget into the content box of `node`.
 *
 * Called by `paint.js` after the box's own background and border, in place of
 * its children — a widget has none.
 */
export function paintWidget(
  node: TuiNode,
  widget: Widget,
  frame: Frame,
  capabilities: Capabilities,
  clip: Rect,
  hits: HitGrid | null,
  selectable: boolean,
): void {
  const area = contentBox(node);
  const inside = intersect(clip, area);
  const ascii = capabilities.glyphs === "ascii";
  if (widget === "select") {
    paintSelect(node, frame, area, inside, ascii);
  } else if (widget === "tab-select") {
    paintTabSelect(node, frame, area, inside, ascii);
  } else {
    paintTextarea(node, frame, area, inside, hits, selectable);
  }
}

/** The background and text colours a widget uses with and without focus. */
function baseColors(node: TuiNode): { bg: Color, fg: Color } {
  const focused = node.props.focused === true;
  const bg = colorOf(node, "backgroundColor", INHERIT);
  const fg = colorOf(node, "textColor", INHERIT);
  return focused
    ? { bg: colorOf(node, "focusedBackgroundColor", bg), fg: colorOf(node, "focusedTextColor", fg) }
    : { bg, fg };
}

function paintSelect(node: TuiNode, frame: Frame, area: Rect, clip: Rect, ascii: boolean): void {
  const { bg, fg } = baseColors(node);
  if (bg !== INHERIT) {
    fillRect(frame, area, { fg, bg, attributes: Attributes.NONE }, clip);
  }
  const options = optionsOf(node);
  if (options.length === 0) {
    return;
  }
  const selected = selectedOf(node, options.length);
  const spacing = count(node, "itemSpacing", 0);
  const perItem = selectLinesPerItem(node);
  const showDescription = flag(node, "showDescription", true);
  const showIndicator = flag(node, "showSelectionIndicator", true);
  const visible = Math.max(1, Math.floor(area.height / perItem));
  const first = windowStart(selected, visible, options.length);
  const selectedBg = colorOf(node, "selectedBackgroundColor", SELECTED_BACKGROUND);
  const selectedFg = colorOf(node, "selectedTextColor", SELECTED_TEXT);
  const description = colorOf(node, "descriptionColor", DESCRIPTION);
  const selectedDescription = colorOf(node, "selectedDescriptionColor", SELECTED_DESCRIPTION);
  const arrow = ascii ? "> " : "▶ ";

  for (let slot = 0; slot < visible && first + slot < options.length; slot += 1) {
    const index = first + slot;
    const option = options[index];
    const isSelected = index === selected;
    const top = slot * perItem;
    // An item that would not fit whole is not drawn at all, which is OpenTUI's
    // rule: half a name with no description is not an item a reader can read.
    if (top + perItem - 1 >= area.height) {
      break;
    }
    const y = area.y + top;
    if (isSelected) {
      fillRect(
        frame,
        { x: area.x, y, width: area.width, height: perItem - spacing },
        { fg: selectedFg, bg: selectedBg, attributes: Attributes.NONE },
        clip,
      );
    }
    const indicator = showIndicator ? (isSelected ? arrow : "  ") : "";
    drawText(
      frame,
      area.x + 1,
      y,
      indicator + option.name,
      isSelected ? selectedFg : fg,
      undefined,
      clip,
    );
    if (showDescription && top + 1 < area.height) {
      drawText(
        frame,
        area.x + 1 + (showIndicator ? 2 : 0),
        y + 1,
        option.description,
        isSelected ? selectedDescription : description,
        undefined,
        clip,
      );
    }
  }

  if (flag(node, "showScrollIndicator", false) && options.length > visible) {
    const percent = first / (options.length - visible);
    const track = Math.max(1, area.height - 2);
    drawText(
      frame,
      area.x + area.width - 1,
      area.y + 1 + Math.floor(percent * track),
      ascii ? "#" : "█",
      SCROLL_INDICATOR,
      undefined,
      clip,
    );
  }
}

function paintTabSelect(node: TuiNode, frame: Frame, area: Rect, clip: Rect, ascii: boolean): void {
  const { bg, fg } = baseColors(node);
  if (bg !== INHERIT) {
    fillRect(frame, area, { fg, bg, attributes: Attributes.NONE }, clip);
  }
  const options = optionsOf(node);
  if (options.length === 0) {
    return;
  }
  const selected = selectedOf(node, options.length);
  const tabWidth = Math.max(1, count(node, "tabWidth", DEFAULT_TAB_WIDTH));
  const showUnderline = flag(node, "showUnderline", true);
  const showDescription = flag(node, "showDescription", true);
  const visible = Math.max(1, Math.floor(area.width / tabWidth));
  const first = windowStart(selected, visible, options.length);
  const selectedBg = colorOf(node, "selectedBackgroundColor", SELECTED_BACKGROUND);
  const selectedFg = colorOf(node, "selectedTextColor", SELECTED_TEXT);
  const ellipsis = ascii ? "~" : "…";

  for (let slot = 0; slot < visible && first + slot < options.length; slot += 1) {
    const index = first + slot;
    const isSelected = index === selected;
    const offset = slot * tabWidth;
    if (offset >= area.width) {
      break;
    }
    const x = area.x + offset;
    const width = Math.min(tabWidth, area.width - offset);
    if (isSelected) {
      fillRect(
        frame,
        { x, y: area.y, width, height: 1 },
        { fg: selectedFg, bg: selectedBg, attributes: Attributes.NONE },
        clip,
      );
    }
    const color = isSelected ? selectedFg : fg;
    drawText(
      frame,
      x + 1,
      area.y,
      truncate(options[index].name, width - 2, ellipsis),
      color,
      undefined,
      clip,
    );
    if (isSelected && showUnderline && area.height >= 2) {
      drawText(frame, x, area.y + 1, (ascii ? "=" : "▬").repeat(width), color, selectedBg, clip);
    }
  }

  if (showDescription && area.height >= (showUnderline ? 3 : 2)) {
    drawText(
      frame,
      area.x + 1,
      area.y + (showUnderline ? 2 : 1),
      truncate(options[selected].description, area.width - 2, ellipsis),
      colorOf(node, "selectedDescriptionColor", SELECTED_DESCRIPTION),
      undefined,
      clip,
    );
  }

  if (flag(node, "showScrollArrows", true) && options.length > visible) {
    if (first > 0) {
      drawText(frame, area.x, area.y, ascii ? "<" : "‹", SCROLL_ARROWS, undefined, clip);
    }
    if (first + visible < options.length) {
      drawText(
        frame,
        area.x + area.width - 1,
        area.y,
        ascii ? ">" : "›",
        SCROLL_ARROWS,
        undefined,
        clip,
      );
    }
  }
}

/**
 * Draw a `Textarea`: its lines, its cursor, and the window onto both.
 *
 * The window is the one piece of state this module keeps, on the node, between
 * frames — `viewTop` and `viewLeft`. It moves only as far as it has to for the
 * cursor to be inside it, which is what every editor does and what OpenTUI's
 * does: a window recomputed from the cursor alone would jump to put the cursor
 * in the same place on every keystroke. It is written here rather than in
 * layout because it is a function of the lines, and the lines are this
 * module's.
 */
function paintTextarea(
  node: TuiNode,
  frame: Frame,
  area: Rect,
  clip: Rect,
  hits: HitGrid | null,
  selectable: boolean,
): void {
  const { bg, fg } = baseColors(node);
  if (bg !== INHERIT) {
    fillRect(frame, area, { fg, bg, attributes: Attributes.NONE }, clip);
  }
  const focused = node.props.focused === true;
  const text = typeof node.props.value === "string" ? node.props.value : "";
  const mode = wrapOf(node);

  if (text === "") {
    node.viewTop = 0;
    node.viewLeft = 0;
    const placeholder = typeof node.props.placeholder === "string" ? node.props.placeholder : "";
    const color = colorOf(node, "placeholderColor", PLACEHOLDER);
    const lines = editLines(placeholder, area.width, mode);
    for (let row = 0; row < lines.length && row < area.height; row += 1) {
      drawCells(
        frame,
        lines[row].cells,
        area.x,
        area.y + row,
        0,
        { fg: color, bg, attributes: 0 },
        clip,
      );
    }
    if (focused) {
      cursorCell(frame, lines[0]?.cells[0], area.x, area.y, fg, bg, clip);
    }
    return;
  }

  const cursor = Math.max(
    0,
    Math.min(typeof node.props.cursor === "number" ? node.props.cursor : 0, text.length),
  );
  const lines = editLines(text, area.width, mode);
  const { row, column: rawColumn } = locate(lines, cursor);
  // A cursor after the last character of a line that exactly fills the width
  // has no column to stand in; it stands on that character instead.
  const column =
    mode === "none" || area.width <= 0 ? rawColumn : Math.min(rawColumn, area.width - 1);

  let top = Math.min(node.viewTop, Math.max(0, lines.length - area.height));
  if (row < top) {
    top = row;
  } else if (row >= top + area.height) {
    top = row - area.height + 1;
  }
  let left = mode === "none" ? node.viewLeft : 0;
  if (column < left) {
    left = column;
  } else if (area.width > 0 && column >= left + area.width) {
    left = column - area.width + 1;
  }
  node.viewTop = Math.max(0, top);
  node.viewLeft = Math.max(0, left);

  const style: Style = { fg, bg, attributes: Attributes.NONE };
  for (let slot = 0; slot < area.height && node.viewTop + slot < lines.length; slot += 1) {
    const line = lines[node.viewTop + slot];
    const y = area.y + slot;
    drawCells(frame, line.cells, area.x, y, node.viewLeft, style, clip);
    if (hits != null && selectable) {
      let x = area.x - node.viewLeft;
      for (const cell of line.cells) {
        if (cell.width > 0) {
          recordText(hits, node, x, y, cell.width, clip);
        }
        x += cell.width;
      }
    }
  }

  if (focused) {
    const cells = lines[row]?.cells ?? [];
    let under = cells.find((cell) => cell.at === cursor) ?? null;
    let x = rawColumn;
    const last = cells[cells.length - 1];
    if (under == null && column < rawColumn && last != null) {
      // No column after the line to stand in, so the cursor stands on its
      // last character rather than off the edge of the box.
      under = last;
      x = rawColumn - last.width;
    }
    cursorCell(frame, under, area.x + x - node.viewLeft, area.y + row - node.viewTop, fg, bg, clip);
  }
}

function drawCells(
  frame: Frame,
  cells: $ReadOnlyArray<EditCell>,
  x: number,
  y: number,
  left: number,
  style: Style,
  clip: Rect,
): void {
  let column = x - left;
  for (const cell of cells) {
    if (cell.width > 0) {
      writeGrapheme(frame, column, y, cell.text, cell.width, style, clip);
    }
    column += cell.width;
  }
}

/**
 * The cursor: the character under it, or a space, in inverse video.
 *
 * The same choice `Input` makes and for the same reason — a terminal has one
 * real cursor, and an inverse cell is a property of the frame, so two of these
 * on one screen do not fight over it.
 */
function cursorCell(
  frame: Frame,
  cell: EditCell | void | null,
  x: number,
  y: number,
  fg: Color,
  bg: Color,
  clip: Rect,
): void {
  const text = cell != null && cell.width > 0 ? cell.text : " ";
  const width = cell != null && cell.width > 0 ? cell.width : 1;
  writeGrapheme(frame, x, y, text, width, { fg, bg, attributes: Attributes.INVERSE }, clip);
}
