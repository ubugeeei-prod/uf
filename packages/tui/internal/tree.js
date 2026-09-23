// @flow
//
// The node tree: the thing React mutates and the renderer reads.
//
// # Internal to `@uniflowed/tui`
//
// Absent from `package.json#exports`, and the reason is the one below: props
// are turned into a layout style *here*, once, and a consumer who could reach
// `styleFromProps` could apply a different rule to a node the renderer will
// then lay out by this one. A tree whose nodes disagree about what `padding`
// means is not a tree anything can draw.
//
//
// There are exactly three participants and this is the boundary between them.
// React's reconciler creates, moves and updates nodes and knows nothing else
// about them. `layout.js` reads `style`, `children` and `borderWidth` and
// writes geometry. `paint.js`, beside this file, reads geometry and `props`
// and writes cells.
// None of the three imports another, which is what makes each of them
// testable on a tree built by hand.
//
// # Props are read once, not on every frame
//
// A node's layout style is derived when its props are set, not when layout
// runs. Layout runs on every frame and props change on a commit, so deriving
// once per commit rather than once per frame is the obvious trade — but the
// reason it is written down is the other half: it means `style` is a plain
// object with resolved numbers in it, and a layout test can build one without
// going anywhere near React.
//
// # Both spellings of a prop
//
// OpenTUI accepts `<box padding={2}>` and `<box style={{ padding: 2 }}>` and
// treats them as the same thing, so this does too. The direct prop wins when
// both are present, which is the rule a reader guesses.

import type {
  AlignContent,
  AlignItems,
  AlignSelf,
  FlexDirection,
  FlexWrap,
  JustifyContent,
  LayoutStyle,
  Overflow,
  Position,
  ScrollIndex,
} from "../layout.js";
import type { Color, Style } from "../cells.js";
import { Attributes, INHERIT, PLAIN, parseColor } from "../cells.js";
import type { BorderStyle } from "../capability.js";

/**
 * What kind of node this is.
 *
 * `"chars"` is a run of literal text — what React calls a text instance, what
 * a caller wrote as `{name}` inside a `<Text>`. It is a node rather than a
 * string on the parent because React inserts, moves and deletes them
 * individually, and a parent holding a concatenated string cannot express
 * "the second of my three children changed".
 */
export type TuiNodeType = "root" | "box" | "text" | "chars";

/**
 * The three kinds of box that draw themselves: a `Select`, a `TabSelect` and a
 * `Textarea`. `widgets.js` draws them and says why they are drawn rather than
 * composed.
 */
export type Widget = "select" | "tab-select" | "textarea";

/** Anything a component put on a node. Read by `paint.js`, not by layout. */
export type TuiProps = { readonly [string]: mixed };

/** One node of the tree. Mutable: React owns its shape, layout owns its geometry. */
export type TuiNode = {
  type: TuiNodeType,
  props: TuiProps,
  children: Array<TuiNode>,
  parent: TuiNode | null,
  /** The literal text of a `"chars"` node; `""` for every other kind. */
  text: string,
  /** Derived from `props` whenever they are set. */
  style: LayoutStyle,
  /** 1 when the node draws a border, 0 otherwise. Layout needs it; paint draws it. */
  borderWidth: number,
  /** How a leaf reports the size it wants. Only text nodes have one. */
  measure:
    | ((
        availableWidth: number,
        availableHeight: number,
      ) => { readonly width: number, readonly height: number })
    | null,
  x: number,
  y: number,
  width: number,
  height: number,
  /** Which of its children a scrolling box laid out, as a range. */
  scrollFirst: number,
  scrollCount: number,
  /** Rows of content this node holds, when it scrolls. */
  scrollHeight: number,
  /** The first row it shows, after clamping, when it scrolls. */
  scrollOffset: number,
  /** Where its window is: first row, how many rows, and the bar's column. */
  scrollViewTop: number,
  scrollViewRows: number,
  scrollBarColumn: number,
  /** The intrinsic size this node last reported, and what it was offered. */
  measuredForWidth: number,
  measuredForHeight: number,
  measuredWidth: number,
  measuredHeight: number,
  /** The first child of a scrolling box that changed, or `-1`. See {@link invalidate}. */
  scrollDirtyFrom: number,
  /** A scrolling box's stack of child heights; see `layout.js`. */
  scrollIndex: ScrollIndex | null,
  /**
   * What this box draws in place of children, when it is a `Select`, a
   * `TabSelect` or a `Textarea`; `null` for every other node. See
   * `widgets.js`, which is also the only reader of the two fields below.
   */
  widget: Widget | null,
  /**
   * The first line and column a `Textarea` shows, kept between frames so the
   * window moves only as far as the cursor makes it. Written by the painter.
   */
  viewTop: number,
  viewLeft: number,
};

/** Read a prop, preferring the direct spelling over the one inside `style`. */
function prop(props: TuiProps, name: string): mixed {
  const direct = props[name];
  if (direct !== undefined) {
    return direct;
  }
  const style = props.style;
  if (style != null && typeof style === "object") {
    return style[name];
  }
  return undefined;
}

const asNumber = (value: mixed): number | void =>
  typeof value === "number" && Number.isFinite(value) ? value : undefined;

const asMargin = (value: mixed): number | "auto" | void =>
  value === "auto" ? "auto" : asNumber(value);

const asDimension = (value: mixed): number | string | void => {
  if (typeof value === "number" && Number.isFinite(value)) {
    return value;
  }
  return typeof value === "string" ? value : undefined;
};

const asString = (value: mixed): string | void => (typeof value === "string" ? value : undefined);

/**
 * Whether a node draws a border, and in which style.
 *
 * `border` is a boolean and `borderStyle` names one of four, exactly as
 * OpenTUI documents them: `border` alone means `"single"`, and naming a style
 * implies the border. The two spellings exist because `border` is what a
 * caller reaches for first and `borderStyle` is what they reach for second,
 * and making the second one imply the first saves the bug where a box has a
 * `borderStyle` and no border.
 */
export function borderOf(props: TuiProps): BorderStyle | null {
  const style = asString(prop(props, "borderStyle"));
  if (style === "single" || style === "double" || style === "rounded" || style === "heavy") {
    return style;
  }
  return prop(props, "border") === true ? "single" : null;
}

/** The style keys whose value is a cell count or a percentage. */
const DIMENSION_KEYS = [
  "width",
  "height",
  "minWidth",
  "minHeight",
  "maxWidth",
  "maxHeight",
  "flexBasis",
  "top",
  "right",
  "bottom",
  "left",
] as const;

/** The style keys whose value is a plain number. */
const NUMBER_KEYS = [
  "flexGrow",
  "flexShrink",
  "padding",
  "paddingTop",
  "paddingRight",
  "paddingBottom",
  "paddingLeft",
  "gap",
  "rowGap",
  "columnGap",
  "scrollTop",
] as const;

/** The style keys whose value is a margin: cells, or `"auto"`. */
const MARGIN_KEYS = ["margin", "marginTop", "marginRight", "marginBottom", "marginLeft"] as const;

const FLEX_DIRECTIONS: $ReadOnlyArray<FlexDirection> = [
  "row",
  "row-reverse",
  "column",
  "column-reverse",
];
const JUSTIFY_CONTENTS: $ReadOnlyArray<JustifyContent> = [
  "flex-start",
  "center",
  "flex-end",
  "space-between",
  "space-around",
  "space-evenly",
];
const ALIGN_ITEMS: $ReadOnlyArray<AlignItems> = ["flex-start", "center", "flex-end", "stretch"];
const ALIGN_SELVES: $ReadOnlyArray<AlignSelf> = ["auto", ...ALIGN_ITEMS];
const OVERFLOWS: $ReadOnlyArray<Overflow> = ["visible", "hidden", "scroll"];
const POSITIONS: $ReadOnlyArray<Position> = ["relative", "absolute"];
const FLEX_WRAPS: $ReadOnlyArray<FlexWrap> = ["no-wrap", "wrap", "wrap-reverse"];
const ALIGN_CONTENTS: $ReadOnlyArray<AlignContent> = [
  "flex-start",
  "center",
  "flex-end",
  "stretch",
  "space-between",
  "space-around",
  "space-evenly",
];

/** `value` when it is one of `words`, and `undefined` for anything else. */
function oneOf<T>(value: mixed, words: $ReadOnlyArray<T>): T | void {
  return words.find((word) => word === value);
}

/**
 * The layout style a node's props describe.
 *
 * Only the keys that were actually given are set, so `layout.js`'s defaults —
 * which are OpenTUI's — apply to everything else. Writing `flexDirection:
 * props.flexDirection ?? "column"` here instead would move the defaults into
 * two places and make them disagree the first time one of them changed.
 */
export function styleFromProps(props: TuiProps): LayoutStyle {
  // Writable while it is being built, and the same keys and value types as the
  // `LayoutStyle` it is returned as.
  const style: { [K in $Keys<LayoutStyle>]?: LayoutStyle[K] } = {};
  for (const name of DIMENSION_KEYS) {
    const value = asDimension(prop(props, name));
    if (value !== undefined) {
      style[name] = value;
    }
  }
  for (const name of NUMBER_KEYS) {
    const value = asNumber(prop(props, name));
    if (value !== undefined) {
      style[name] = value;
    }
  }
  for (const name of MARGIN_KEYS) {
    const value = asMargin(prop(props, name));
    if (value !== undefined) {
      style[name] = value;
    }
  }

  // The keyword props, each read against the words `layout.js` knows. A word
  // it does not know is left unset, so layout's default applies — which is
  // what a misspelt keyword already did there, now said here instead.
  const flexDirection = oneOf(prop(props, "flexDirection"), FLEX_DIRECTIONS);
  if (flexDirection !== undefined) {
    style.flexDirection = flexDirection;
  }
  const justifyContent = oneOf(prop(props, "justifyContent"), JUSTIFY_CONTENTS);
  if (justifyContent !== undefined) {
    style.justifyContent = justifyContent;
  }
  const alignItems = oneOf(prop(props, "alignItems"), ALIGN_ITEMS);
  if (alignItems !== undefined) {
    style.alignItems = alignItems;
  }
  const alignSelf = oneOf(prop(props, "alignSelf"), ALIGN_SELVES);
  if (alignSelf !== undefined) {
    style.alignSelf = alignSelf;
  }
  const overflow = oneOf(prop(props, "overflow"), OVERFLOWS);
  if (overflow !== undefined) {
    style.overflow = overflow;
  }
  const position = oneOf(prop(props, "position"), POSITIONS);
  if (position !== undefined) {
    style.position = position;
  }
  const flexWrap = oneOf(prop(props, "flexWrap"), FLEX_WRAPS);
  if (flexWrap !== undefined) {
    style.flexWrap = flexWrap;
  }
  const alignContent = oneOf(prop(props, "alignContent"), ALIGN_CONTENTS);
  if (alignContent !== undefined) {
    style.alignContent = alignContent;
  }

  // `paddingX` and `paddingY`, which OpenTUI documents and which are the two
  // shorthands a terminal layout actually reaches for: a box is padded a
  // column on each side far more often than it is padded on all four.
  const paddingX = asNumber(prop(props, "paddingX"));
  if (paddingX !== undefined) {
    style.paddingLeft = style.paddingLeft ?? paddingX;
    style.paddingRight = style.paddingRight ?? paddingX;
  }
  const paddingY = asNumber(prop(props, "paddingY"));
  if (paddingY !== undefined) {
    style.paddingTop = style.paddingTop ?? paddingY;
    style.paddingBottom = style.paddingBottom ?? paddingY;
  }

  return style;
}

/** The text style a node's props describe, layered onto what it inherited. */
export function textStyleFromProps(props: TuiProps, inherited: Style): Style {
  const fg = colorProp(props, ["fg", "color", "foregroundColor"], inherited.fg);
  const bg = colorProp(props, ["bg", "backgroundColor"], inherited.bg);
  let attributes = inherited.attributes;
  const set = (name: string, bit: number) => {
    const value = prop(props, name);
    if (value === true) {
      attributes |= bit;
    } else if (value === false) {
      attributes &= ~bit;
    }
  };
  set("bold", Attributes.BOLD);
  set("dim", Attributes.DIM);
  set("italic", Attributes.ITALIC);
  set("underline", Attributes.UNDERLINE);
  set("blink", Attributes.BLINK);
  set("inverse", Attributes.INVERSE);
  set("strikethrough", Attributes.STRIKETHROUGH);
  const explicit = asNumber(prop(props, "attributes"));
  if (explicit !== undefined) {
    attributes |= explicit;
  }
  return { fg, bg, attributes };
}

function colorProp(props: TuiProps, names: $ReadOnlyArray<string>, fallback: Color): Color {
  for (const name of names) {
    const value = prop(props, name);
    if (typeof value === "string" || typeof value === "number") {
      const parsed = parseColor(value);
      if (parsed !== INHERIT) {
        return parsed;
      }
    }
  }
  return fallback;
}

/** One stretch of text that shares a style. */
export type TextRun = {
  readonly text: string,
  readonly style: Style,
};

/**
 * Flatten a text subtree into runs.
 *
 * Nesting is how a caller writes `<Text>ready in <Text bold>{ms}ms</Text></Text>`,
 * and the inner node inherits the outer node's colour while overriding its
 * weight — the same rule CSS has, because it is the rule a reader expects and
 * because the alternative is repeating the colour on every fragment.
 *
 * A `<Box>` inside a `<Text>` is dropped rather than laid out. A box has a
 * geometry and a run of text has a position in a line; there is no answer to
 * what the two mean together that is better than refusing.
 */
export function textRuns(node: TuiNode, inherited: Style): Array<TextRun> {
  const out: Array<TextRun> = [];
  const walk = (current: TuiNode, style: Style) => {
    for (const child of current.children) {
      if (child.type === "chars") {
        if (child.text !== "") {
          out.push({ text: child.text, style });
        }
      } else if (child.type === "text") {
        walk(child, textStyleFromProps(child.props, style));
      }
    }
  };
  walk(node, inherited);
  return out;
}

/** A fresh node of the given kind. */
export function createNode(type: TuiNodeType, props: TuiProps): TuiNode {
  const node: TuiNode = {
    type,
    props,
    children: [],
    parent: null,
    text: "",
    style: {},
    borderWidth: 0,
    measure: null,
    x: 0,
    y: 0,
    width: 0,
    height: 0,
    scrollFirst: 0,
    scrollCount: 0,
    scrollHeight: 0,
    scrollOffset: 0,
    scrollViewTop: 0,
    scrollViewRows: 0,
    scrollBarColumn: 0,
    measuredForWidth: -1,
    measuredForHeight: -1,
    measuredWidth: 0,
    measuredHeight: 0,
    scrollDirtyFrom: 0,
    scrollIndex: null,
    widget: null,
    viewTop: 0,
    viewLeft: 0,
  };
  applyProps(node, props);
  return node;
}

/** Re-derive everything layout reads from a node's props. */
export function applyProps(node: TuiNode, props: TuiProps): void {
  node.props = props;
  node.style = styleFromProps(props);
  node.borderWidth = node.type === "box" && borderOf(props) != null ? 1 : 0;
  invalidate(node);
}

/**
 * Say that something under `node` changed, so layout may not reuse what it
 * measured last time.
 *
 * Layout keeps two answers between frames: what a node's intrinsic size came
 * out as, and — for a scrolling box — where each of its children sits in the
 * stack. Both are only wrong when the tree changed, and React is the only
 * participant that knows when it did; a layout that worked it out for itself
 * would have to compare this frame's tree with the last one's, which is the
 * walk over every child that the stack exists to avoid.
 *
 * The walk goes to the root because an intrinsic size is a fact about a
 * subtree: a character added to a `"chars"` node can widen the `<Text>` above
 * it, which can lengthen the box above that. It is bounded by the depth of the
 * tree, and a terminal's tree is as deep as what fits on a screen.
 *
 * `from` is the first child index of `node` that changed. Appending a line to
 * a log leaves every line above it where it was, which is what makes appending
 * cost one measurement rather than the log; above `node` nothing is known that
 * precisely, so every scrolling ancestor is invalidated whole.
 *
 * `null` — the default, and what a change to the node's *own* props or text
 * means — leaves the node's own stack alone, because a scrolling box's props
 * cannot move its children except through the width, the viewport height and
 * the gap it gives them, and all three are part of what the stack is keyed on.
 * That exemption is not a nicety: `scrollTop` is a prop on that box, so
 * without it every scroll would invalidate the very thing that makes scrolling
 * cheap, and a hundred thousand rows would rebuild their stack on each notch.
 */
export function invalidate(node: TuiNode, from: number | null = null): void {
  let current: TuiNode | null = node;
  let first = from;
  while (current != null) {
    current.measuredForWidth = -1;
    current.measuredForHeight = -1;
    if (first != null) {
      current.scrollDirtyFrom =
        current.scrollDirtyFrom < 0 ? first : Math.min(current.scrollDirtyFrom, first);
    }
    first = 0;
    current = current.parent;
  }
}

/** The style a text node's runs start from when nothing above it said otherwise. */
export const ROOT_TEXT_STYLE: Style = PLAIN;
