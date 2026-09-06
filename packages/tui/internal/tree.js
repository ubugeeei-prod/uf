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

import type { LayoutStyle } from "../layout.js";
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
  /** Whether a scrolling ancestor put this node outside its window. */
  hidden: boolean,
  /** Rows of content this node holds, when it scrolls. */
  scrollHeight: number,
  /** The first row it shows, after clamping, when it scrolls. */
  scrollOffset: number,
  /** Where its window is: first row, how many rows, and the bar's column. */
  scrollViewTop: number,
  scrollViewRows: number,
  scrollBarColumn: number,
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

/**
 * The layout style a node's props describe.
 *
 * Only the keys that were actually given are set, so `layout.js`'s defaults —
 * which are OpenTUI's — apply to everything else. Writing `flexDirection:
 * props.flexDirection ?? "column"` here instead would move the defaults into
 * two places and make them disagree the first time one of them changed.
 */
export function styleFromProps(props: TuiProps): LayoutStyle {
  const style: { [string]: mixed } = {};
  const copy = (name: string, read: (mixed) => mixed) => {
    const value = read(prop(props, name));
    if (value !== undefined) {
      style[name] = value;
    }
  };
  for (const name of [
    "width",
    "height",
    "minWidth",
    "minHeight",
    "maxWidth",
    "maxHeight",
    "flexBasis",
  ]) {
    copy(name, asDimension);
  }
  for (const name of [
    "flexGrow",
    "flexShrink",
    "padding",
    "paddingTop",
    "paddingRight",
    "paddingBottom",
    "paddingLeft",
    "margin",
    "marginTop",
    "marginRight",
    "marginBottom",
    "marginLeft",
    "gap",
    "rowGap",
    "columnGap",
    "scrollTop",
  ]) {
    copy(name, asNumber);
  }
  for (const name of ["flexDirection", "justifyContent", "alignItems", "alignSelf", "overflow"]) {
    copy(name, asString);
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
    hidden: false,
    scrollHeight: 0,
    scrollOffset: 0,
    scrollViewTop: 0,
    scrollViewRows: 0,
    scrollBarColumn: 0,
  };
  applyProps(node, props);
  return node;
}

/** Re-derive everything layout reads from a node's props. */
export function applyProps(node: TuiNode, props: TuiProps): void {
  node.props = props;
  node.style = styleFromProps(props);
  node.borderWidth = node.type === "box" && borderOf(props) != null ? 1 : 0;
}

/** The style a text node's runs start from when nothing above it said otherwise. */
export const ROOT_TEXT_STYLE: Style = PLAIN;
