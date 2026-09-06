// @flow
//
// Flexbox, in whole cells.
//
// OpenTUI lays a terminal out with Yoga, and its documented defaults are
// flexbox's with one change: `flexDirection` starts at `"column"`, because a
// terminal is a stack of lines and a row of them is the special case. Those
// defaults are reproduced here exactly — `justifyContent: "flex-start"`,
// `alignItems: "stretch"`, `flexGrow: 0`, `flexBasis: "auto"`, and a
// `flexShrink` that is `0` when a dimension was given as a number and `1`
// otherwise — so a tree written against OpenTUI's documentation lays out the
// same way here.
//
// # Why not Yoga
//
// Yoga is a C++ library compiled to WebAssembly. Loading it costs a WASM
// instantiation before the first frame, which is the cost a terminal program
// can least afford: the whole visible difference between a fast CLI and a slow
// one is what happens before the first paint. It also solves a problem this
// does not have. Yoga resolves fractional CSS pixels, percentages of
// percentages, aspect ratios, and writing directions; a terminal has integer
// columns, one writing direction, and a tree whose depth is bounded by what
// fits on a screen.
//
// So this implements the subset a terminal uses, in whole cells. What it does
// *not* implement is written down at the bottom of this file rather than
// discovered: no wrapping, no absolute positioning, no aspect ratio, no `auto`
// margins. Those are ubugeeei-prod/uf#314.
//
// # Whole cells, and where the remainder goes
//
// Distributing 10 free columns between three growing children gives each of
// them 3⅓, and a terminal has no third of a column. Rounding each child
// independently loses a column when three of them round down, and a row that
// is one column short of its container is exactly the artefact that makes a
// hand-written terminal UI look broken. So the remainder is *carried*: each
// child gets the floor of its running exact total minus what has already been
// handed out, which means the sum of the parts is always the whole, and the
// column that could not be divided lands on one specific child rather than
// vanishing.

/**
 * A length: a number of cells, a percentage of the containing block, or
 * `"auto"` for "as large as the content needs".
 */
export type Dimension = number | string;

/** Main-axis direction. `"column"` is the default, as in OpenTUI. */
export type FlexDirection = "row" | "row-reverse" | "column" | "column-reverse";

/** Main-axis distribution. */
export type JustifyContent =
  | "flex-start"
  | "center"
  | "flex-end"
  | "space-between"
  | "space-around"
  | "space-evenly";

/** Cross-axis alignment of every child. */
export type AlignItems = "flex-start" | "center" | "flex-end" | "stretch";

/** Cross-axis alignment of one child, or `"auto"` to follow the parent. */
export type AlignSelf = "auto" | AlignItems;

/**
 * What happens to content larger than its box.
 *
 * `"scroll"` is `"hidden"` plus an offset: the children are stacked at their
 * own heights, the box shows a window onto them, and `scrollTop` says which
 * rows. It is the only value that changes how children are *placed* rather
 * than only what is drawn, which is why the scroll layout is a branch of
 * {@link layout} rather than a flag the painter reads.
 */
export type Overflow = "visible" | "hidden" | "scroll";

/**
 * Everything layout reads off a node.
 *
 * Every field is optional and every default is flexbox's, so a node with no
 * style at all is a column that grows to fit its content — which is what a
 * caller who wrote `<Box>` meant.
 */
export type LayoutStyle = {
  readonly flexDirection?: FlexDirection,
  readonly justifyContent?: JustifyContent,
  readonly alignItems?: AlignItems,
  readonly alignSelf?: AlignSelf,
  readonly flexGrow?: number,
  readonly flexShrink?: number,
  readonly flexBasis?: Dimension,
  readonly width?: Dimension,
  readonly height?: Dimension,
  readonly minWidth?: Dimension,
  readonly minHeight?: Dimension,
  readonly maxWidth?: Dimension,
  readonly maxHeight?: Dimension,
  readonly padding?: number,
  readonly paddingTop?: number,
  readonly paddingRight?: number,
  readonly paddingBottom?: number,
  readonly paddingLeft?: number,
  readonly margin?: number,
  readonly marginTop?: number,
  readonly marginRight?: number,
  readonly marginBottom?: number,
  readonly marginLeft?: number,
  readonly gap?: number,
  readonly rowGap?: number,
  readonly columnGap?: number,
  readonly overflow?: Overflow,
  /**
   * The first content row a scrolling box shows.
   *
   * Read only when `overflow` is `"scroll"`, and clamped by layout to the
   * range the content actually has — so `Number.MAX_SAFE_INTEGER` means "the
   * bottom" and needs no separate prop, and a caller that has just appended a
   * line to a log does not have to know how long the log is to follow it.
   */
  readonly scrollTop?: number,
};

/**
 * A node laid out by this module.
 *
 * `measure` is how a leaf that knows its own size — a run of text, whose
 * height depends on the width it is given — participates without layout
 * knowing what text is. It is Yoga's measure callback under a shorter name.
 *
 * The four geometry fields are written *by* layout and read by the painter.
 * They are the node's border box in absolute frame coordinates.
 */
export type LayoutNode = {
  style: LayoutStyle,
  children: Array<LayoutNode>,
  /** Cells the node's own frame occupies on each edge; a border is 1. */
  borderWidth: number,
  measure: ((availableWidth: number, availableHeight: number) => Size) | null,
  x: number,
  y: number,
  width: number,
  height: number,
  /**
   * Whether this node is outside a scrolling ancestor's window.
   *
   * Written by layout and read by the painter. Zeroing the geometry would not
   * be enough: a box zero cells wide draws nothing itself, and the painter
   * would still walk into its children, whose geometry is whatever the last
   * frame left there. A skipped subtree has to be skipped as a subtree.
   */
  hidden: boolean,
  /** Rows of content a scrolling box holds. Written by layout. */
  scrollHeight: number,
  /** The first row it is actually showing, after clamping. Written by layout. */
  scrollOffset: number,
  /**
   * Where the window is, in frame coordinates: its first row, how many rows it
   * has, and the column the bar goes in.
   *
   * Written by layout because layout is what resolved the padding, and the
   * painter must not resolve it a second time — a bar drawn against the border
   * box rather than the content box is a bar over the content on any box with
   * padding on it.
   */
  scrollViewTop: number,
  scrollViewRows: number,
  scrollBarColumn: number,
  ...
};

/** A resolved size, in whole cells. */
export type Size = { readonly width: number, readonly height: number };

const clamp = (value: number, low: number, high: number): number =>
  Math.min(Math.max(value, low), high);

/** Whether the main axis is horizontal. */
const isRow = (direction: FlexDirection): boolean =>
  direction === "row" || direction === "row-reverse";

const direction = (style: LayoutStyle): FlexDirection => style.flexDirection ?? "column";

/**
 * Resolve a length against the space its containing block offers.
 *
 * Returns `null` for `"auto"` and for anything unrecognised, which is layout's
 * word for "ask the content". Percentages round to the nearest cell: half a
 * column does not exist, and the alternative — truncating — makes `50%` of an
 * odd width lose a column that `50%` on the other side does not gain.
 */
function resolve(value: Dimension | void, basis: number): number | null {
  if (typeof value === "number") {
    return Math.max(0, Math.round(value));
  }
  if (typeof value === "string" && value.endsWith("%")) {
    const percent = Number.parseFloat(value.slice(0, -1));
    return Number.isFinite(percent) ? Math.max(0, Math.round((basis * percent) / 100)) : null;
  }
  return null;
}

/** Padding on each edge, with the shorthand applied first. */
function padding(style: LayoutStyle): [number, number, number, number] {
  const all = style.padding ?? 0;
  return [
    style.paddingTop ?? all,
    style.paddingRight ?? all,
    style.paddingBottom ?? all,
    style.paddingLeft ?? all,
  ];
}

/** Margin on each edge, with the shorthand applied first. */
function margin(style: LayoutStyle): [number, number, number, number] {
  const all = style.margin ?? 0;
  return [
    style.marginTop ?? all,
    style.marginRight ?? all,
    style.marginBottom ?? all,
    style.marginLeft ?? all,
  ];
}

/** The cells between two children along the main axis. */
function gapOf(style: LayoutStyle, row: boolean): number {
  const specific = row ? style.columnGap : style.rowGap;
  return specific ?? style.gap ?? 0;
}

/**
 * The cells a node's own frame and padding take out of its border box.
 *
 * Returned as `[top, right, bottom, left]`, border and padding summed,
 * because every caller wants the total and none of them wants to add it up
 * again.
 */
function insets(node: LayoutNode): [number, number, number, number] {
  const [top, right, bottom, left] = padding(node.style);
  const border = node.borderWidth;
  return [top + border, right + border, bottom + border, left + border];
}

/**
 * `flexShrink`'s default, which is not a constant.
 *
 * CSS says `1`; OpenTUI says `0` for a child whose main-axis dimension was
 * given as a number, and it is right to. A caller who wrote `width: 40` on a
 * terminal box meant forty columns, and a default that quietly narrows it when
 * the row is full produces a box whose width is 40 on one terminal and 37 on
 * another. A caller who wrote nothing has expressed no such intent.
 */
function shrinkOf(style: LayoutStyle, row: boolean): number {
  if (style.flexShrink != null) {
    return style.flexShrink;
  }
  const main = row ? style.width : style.height;
  return typeof main === "number" ? 0 : 1;
}

/**
 * The size a node wants when nothing constrains it.
 *
 * `available` is what the parent can offer, and it is passed down rather than
 * ignored because a text leaf's height is a function of the width it is given:
 * the same paragraph is one line at 80 columns and four at 20. This is the
 * pass Yoga calls "measure", and it is separate from layout because a parent
 * has to know how large its children want to be before it can decide how large
 * they get to be.
 */
export function intrinsicSize(
  node: LayoutNode,
  availableWidth: number,
  availableHeight: number,
): Size {
  const style = node.style;
  const [insetTop, insetRight, insetBottom, insetLeft] = insets(node);
  const innerAvailableWidth = Math.max(0, availableWidth - insetLeft - insetRight);
  const innerAvailableHeight = Math.max(0, availableHeight - insetTop - insetBottom);

  const fixedWidth = resolve(style.width, availableWidth);
  const fixedHeight = resolve(style.height, availableHeight);

  let contentWidth = 0;
  let contentHeight = 0;

  if (node.measure != null) {
    const measured = node.measure(
      fixedWidth != null ? Math.max(0, fixedWidth - insetLeft - insetRight) : innerAvailableWidth,
      innerAvailableHeight,
    );
    contentWidth = measured.width;
    contentHeight = measured.height;
  } else {
    const row = isRow(direction(node.style));
    const gap = gapOf(style, row);
    let main = 0;
    let cross = 0;
    let counted = 0;
    for (const child of node.children) {
      const [marginTop, marginRight, marginBottom, marginLeft] = margin(child.style);
      const size = intrinsicSize(
        child,
        Math.max(0, innerAvailableWidth - marginLeft - marginRight),
        Math.max(0, innerAvailableHeight - marginTop - marginBottom),
      );
      const outerWidth = size.width + marginLeft + marginRight;
      const outerHeight = size.height + marginTop + marginBottom;
      main += row ? outerWidth : outerHeight;
      cross = Math.max(cross, row ? outerHeight : outerWidth);
      counted += 1;
    }
    main += Math.max(0, counted - 1) * gap;
    contentWidth = row ? main : cross;
    contentHeight = row ? cross : main;
  }

  const width = fixedWidth ?? contentWidth + insetLeft + insetRight;
  const height = fixedHeight ?? contentHeight + insetTop + insetBottom;
  return {
    width: clampDimension(width, style.minWidth, style.maxWidth, availableWidth),
    height: clampDimension(height, style.minHeight, style.maxHeight, availableHeight),
  };
}

/** Apply `min*`/`max*` to a resolved length. */
function clampDimension(
  value: number,
  min: Dimension | void,
  max: Dimension | void,
  basis: number,
): number {
  const low = resolve(min, basis) ?? 0;
  const high = resolve(max, basis) ?? Number.MAX_SAFE_INTEGER;
  return clamp(Math.max(0, Math.round(value)), low, high);
}

/**
 * Hand out `total` cells in the proportions `weights` asks for, losing none.
 *
 * The carry is the whole point. Each recipient gets the floor of the running
 * exact total minus everything already handed out, so the parts sum to the
 * whole for every input rather than for the inputs that happen to divide.
 */
function distribute(total: number, weights: $ReadOnlyArray<number>): Array<number> {
  const sum = weights.reduce((a, b) => a + b, 0);
  const out = new Array(weights.length).fill(0);
  if (sum <= 0 || total === 0) {
    return out;
  }
  let exact = 0;
  let handed = 0;
  for (let i = 0; i < weights.length; i += 1) {
    exact += (total * weights[i]) / sum;
    const next = Math.round(exact);
    out[i] = next - handed;
    handed = next;
  }
  return out;
}

/**
 * Lay `node` out into the border box at `x`, `y`, `width` by `height`.
 *
 * Writes `x`, `y`, `width` and `height` onto every node in the subtree. The
 * caller decides the root's box, which for a terminal is the whole screen.
 */
export function layout(
  node: LayoutNode,
  x: number,
  y: number,
  width: number,
  height: number,
): void {
  node.x = x;
  node.y = y;
  node.width = width;
  node.height = height;
  node.hidden = false;

  if (node.children.length === 0) {
    return;
  }

  const style = node.style;
  const [insetTop, insetRight, insetBottom, insetLeft] = insets(node);
  const contentX = x + insetLeft;
  const contentY = y + insetTop;
  const contentWidth = Math.max(0, width - insetLeft - insetRight);
  const contentHeight = Math.max(0, height - insetTop - insetBottom);

  if (style.overflow === "scroll") {
    layoutScroll(node, contentX, contentY, contentWidth, contentHeight);
    return;
  }

  const flexDirection = direction(style);
  const row = isRow(flexDirection);
  const reverse = flexDirection === "row-reverse" || flexDirection === "column-reverse";
  const mainSpace = row ? contentWidth : contentHeight;
  const crossSpace = row ? contentHeight : contentWidth;
  const gap = gapOf(style, row);
  const children = node.children;

  // Pass one: every child's base main size, and the outer margins around it.
  const margins = children.map((child) => margin(child.style));
  const bases = children.map((child, index) => {
    const [marginTop, marginRight, marginBottom, marginLeft] = margins[index];
    const availableWidth = Math.max(0, contentWidth - marginLeft - marginRight);
    const availableHeight = Math.max(0, contentHeight - marginTop - marginBottom);
    const basis = resolve(child.style.flexBasis, mainSpace);
    if (basis != null) {
      return basis;
    }
    const fixed = resolve(row ? child.style.width : child.style.height, mainSpace);
    if (fixed != null) {
      return fixed;
    }
    const size = intrinsicSize(child, availableWidth, availableHeight);
    return row ? size.width : size.height;
  });

  const outerMain = (index: number): number => {
    const [marginTop, marginRight, marginBottom, marginLeft] = margins[index];
    return bases[index] + (row ? marginLeft + marginRight : marginTop + marginBottom);
  };

  const used =
    children.reduce((total, _, index) => total + outerMain(index), 0) +
    Math.max(0, children.length - 1) * gap;
  const free = mainSpace - used;

  // Pass two: grow into the space left over, or shrink to fit into what there
  // is. Shrinking is weighted by the base size as CSS specifies, so a wide
  // child gives up more columns than a narrow one with the same `flexShrink`.
  const mainSizes = bases.slice();
  if (free > 0) {
    const grow = children.map((child) => Math.max(0, child.style.flexGrow ?? 0));
    const shares = distribute(free, grow);
    for (let i = 0; i < children.length; i += 1) {
      mainSizes[i] += shares[i];
    }
  } else if (free < 0) {
    const weights = children.map((child, index) => shrinkOf(child.style, row) * bases[index]);
    const shares = distribute(-free, weights);
    for (let i = 0; i < children.length; i += 1) {
      mainSizes[i] = Math.max(0, mainSizes[i] - shares[i]);
    }
  }

  // Whatever main-axis space the children did not take, `justifyContent`
  // decides what to do with.
  const consumed =
    mainSizes.reduce(
      (total, size, index) =>
        total +
        size +
        (row ? margins[index][3] + margins[index][1] : margins[index][0] + margins[index][2]),
      0,
    ) +
    Math.max(0, children.length - 1) * gap;
  const slack = Math.max(0, mainSpace - consumed);
  const justify = style.justifyContent ?? "flex-start";
  let cursor = 0;
  let between = gap;
  if (justify === "center") {
    cursor = Math.floor(slack / 2);
  } else if (justify === "flex-end") {
    cursor = slack;
  } else if (justify === "space-between" && children.length > 1) {
    between = gap + Math.floor(slack / (children.length - 1));
  } else if (justify === "space-around" && children.length > 0) {
    const each = Math.floor(slack / children.length);
    cursor = Math.floor(each / 2);
    between = gap + each;
  } else if (justify === "space-evenly" && children.length > 0) {
    const each = Math.floor(slack / (children.length + 1));
    cursor = each;
    between = gap + each;
  }

  const order = reverse ? children.map((_, i) => i).reverse() : children.map((_, i) => i);
  const parentAlign = style.alignItems ?? "stretch";

  for (const index of order) {
    const child = children[index];
    const [marginTop, marginRight, marginBottom, marginLeft] = margins[index];
    const mainMarginStart = row ? marginLeft : marginTop;
    const mainMarginEnd = row ? marginRight : marginBottom;
    const crossMarginStart = row ? marginTop : marginLeft;
    const crossMarginEnd = row ? marginBottom : marginRight;

    const align = (() => {
      const own = child.style.alignSelf ?? "auto";
      return own === "auto" ? parentAlign : own;
    })();

    const crossAvailable = Math.max(0, crossSpace - crossMarginStart - crossMarginEnd);
    const fixedCross = resolve(row ? child.style.height : child.style.width, crossSpace);
    let crossSize: number;
    if (fixedCross != null) {
      crossSize = fixedCross;
    } else if (align === "stretch") {
      crossSize = crossAvailable;
    } else {
      const size = intrinsicSize(
        child,
        row ? mainSizes[index] : crossAvailable,
        row ? crossAvailable : mainSizes[index],
      );
      crossSize = row ? size.height : size.width;
    }
    crossSize = Math.min(crossSize, crossAvailable);

    let crossOffset = crossMarginStart;
    if (align === "center") {
      crossOffset += Math.floor((crossAvailable - crossSize) / 2);
    } else if (align === "flex-end") {
      crossOffset += crossAvailable - crossSize;
    }

    const mainStart = cursor + mainMarginStart;
    const childWidth = row ? mainSizes[index] : crossSize;
    const childHeight = row ? crossSize : mainSizes[index];
    const childX = row ? contentX + mainStart : contentX + crossOffset;
    const childY = row ? contentY + crossOffset : contentY + mainStart;

    layout(
      child,
      childX,
      childY,
      clampDimension(childWidth, child.style.minWidth, child.style.maxWidth, contentWidth),
      clampDimension(childHeight, child.style.minHeight, child.style.maxHeight, contentHeight),
    );

    cursor = mainStart + mainSizes[index] + mainMarginEnd + between;
  }
}

/**
 * Lay a scrolling box's children out, and skip the ones nobody can see.
 *
 * A scrolling box is a column, always. `flexDirection`, `justifyContent` and
 * growth do not apply inside one and are ignored rather than half-honoured: a
 * child that grew to fill a viewport it is meant to scroll past is a child
 * whose height depends on where it has been scrolled to. Margins do apply, and
 * for the opposite reason — they are a fixed number of cells around a child,
 * so they say the same thing at every offset.
 *
 * # What this costs, exactly
 *
 * Every child is **measured** — its height at this width — because the total
 * is what `scrollTop` is clamped against, and a box that guessed its own
 * content height would let `Number.MAX_SAFE_INTEGER` scroll into empty space.
 * Measuring one line of text is measuring one line of text.
 *
 * Only the children that intersect the window are **laid out**: the rest get
 * no position, no size, and no walk into their subtrees, and {@link
 * LayoutNode.hidden} keeps the painter out of them too. So a ten-thousand-line
 * log costs one measure per line and a screenful of everything else — which is
 * the property the whole component exists for, and the reason this is not
 * `overflow: "hidden"` with a margin on top.
 */
function layoutScroll(node: LayoutNode, x: number, y: number, width: number, height: number): void {
  const children = node.children;
  // What a scrolling box offers a child that has not been given a height:
  // nothing in particular. That is what scrolling means — a child is as tall as
  // its content and the window decides how much is seen — and the only thing
  // this basis is read for is a percentage `minHeight` or `maxHeight` on the
  // child, which inside a scroll region is a question with no good answer.
  const unbounded = Number.MAX_SAFE_INTEGER;
  const gap = gapOf(node.style, false);

  // Margins are part of the stack, exactly as they are in the flex path and
  // in `intrinsicSize`: a child's outer height is what the next one starts
  // after and what the scroll range is made of. Leaving them out of any one
  // of those makes the content shorter than it is drawn, which is a bottom
  // the offset clamps to too early and a last row nobody can scroll to.
  const margins = children.map((child) => margin(child.style));

  const heights = new Array<number>(children.length);
  let content = 0;
  for (let index = 0; index < children.length; index += 1) {
    const child = children[index];
    const [marginTop, marginRight, marginBottom, marginLeft] = margins[index];
    const available = Math.max(0, width - marginLeft - marginRight);
    const fixed = resolve(child.style.height, height);
    heights[index] = fixed ?? intrinsicSize(child, available, unbounded).height;
    content += heights[index] + marginTop + marginBottom + (index > 0 ? gap : 0);
  }

  const offset = clamp(Math.floor(node.style.scrollTop ?? 0), 0, Math.max(0, content - height));
  node.scrollHeight = content;
  node.scrollOffset = offset;
  node.scrollViewTop = y;
  node.scrollViewRows = height;
  // The column immediately right of the content, which is the one `ScrollBox`
  // reserved by adding one to its right padding.
  node.scrollBarColumn = x + width;

  let cursor = 0;
  for (let index = 0; index < children.length; index += 1) {
    const child = children[index];
    const [marginTop, marginRight, marginBottom, marginLeft] = margins[index];
    const childHeight = heights[index];
    const childY = y + cursor + marginTop - offset;
    cursor += marginTop + childHeight + marginBottom + gap;
    // The border box, not the outer one: a margin draws nothing, so a child
    // whose own box has left the window has left it.
    if (childY + childHeight <= y || childY >= y + height) {
      hide(child);
      continue;
    }
    const available = Math.max(0, width - marginLeft - marginRight);
    const childWidth = Math.min(available, resolve(child.style.width, width) ?? available);
    layout(child, x + marginLeft, childY, childWidth, childHeight);
  }
}

/** Take a node and everything under it out of this frame. */
function hide(node: LayoutNode): void {
  node.hidden = true;
  node.width = 0;
  node.height = 0;
}

// Not implemented here, on purpose, and tracked rather than discovered:
//
// - `flexWrap`. OpenTUI's default is `"no-wrap"` and a terminal layout that
//   wraps its flex line is rare enough that guessing at the semantics would be
//   worse than not having them.
// - `position: "absolute"`. It needs a containing-block concept that nothing
//   in this package has yet, and every use of it so far has been better served
//   by a box that grows.
// - `auto` margins, which are how CSS centres a single child, and which
//   `justifyContent: "center"` already covers here.
//
// All three are ubugeeei-prod/uf#314.
