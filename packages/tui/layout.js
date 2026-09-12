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
// discovered: no wrapping, no absolute positioning, no aspect ratio. Those are
// ubugeeei-prod/uf#314.
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

/** A margin: a number of cells, or `"auto"` inside a flex container. */
export type Margin = number | "auto";

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
  readonly margin?: Margin,
  readonly marginTop?: Margin,
  readonly marginRight?: Margin,
  readonly marginBottom?: Margin,
  readonly marginLeft?: Margin,
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
   * The index of this node's first child that is inside a scrolling window,
   * and how many of them are. Written by layout, read by the painter.
   *
   * Zero and zero for everything that does not scroll, and for a scrolling box
   * whose window has reached past the end of its content. The painter walks
   * this range instead of the whole child list, which is the half of "only the
   * visible window" that paint is responsible for: a child outside the range
   * has geometry from whichever frame last showed it, and drawing that would
   * put last frame's rows on top of this one's.
   */
  scrollFirst: number,
  scrollCount: number,
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
  /**
   * The last intrinsic size this node reported, and what was offered for it.
   *
   * `measuredFor*` is `-1` when there is nothing cached, which is what
   * `invalidate` in `internal/tree.js` writes when anything under the node
   * changes. Layout never invalidates this itself: a cache that layout could
   * clear would be cleared on the frame that most needs it.
   */
  measuredForWidth: number,
  measuredForHeight: number,
  measuredWidth: number,
  measuredHeight: number,
  /**
   * The first child index whose height may have changed since the last frame,
   * or `-1` when none has.
   *
   * Written by whoever changes the tree — `internal/tree.js`, which is to say
   * React — and cleared by layout once it has acted on it. It is a number
   * rather than a call into this module because the three participants named
   * at the top of `internal/tree.js` do not import each other; a node's fields
   * are the whole of what they say to one another.
   */
  scrollDirtyFrom: number,
  /** A scrolling box's stack of child heights. Owned by {@link layout}. */
  scrollIndex: ScrollIndex | null,
  ...
};

/**
 * Where each child of a scrolling box sits in its content, kept between frames.
 *
 * This is what makes scrolling cost the window rather than the content. The
 * stack of child heights does not change when the offset does, so rebuilding
 * it on every frame would be recomputing the answer to a question nobody
 * asked — and it is the only part of a scrolling box that is proportional to
 * how many children it has.
 *
 * `from` is the first index that has to be rebuilt, and it is written from
 * outside layout: `internal/tree.js` sets it when React mutates the tree, and
 * sets it to the old child count when the mutation was an append, which is the
 * shape a log has. A frame that changed nothing leaves it at the child count
 * and rebuilds none of it.
 *
 * `tops[i]` is where child `i`'s border box starts, measured from the top of
 * the content and including every margin and gap above it; `heights[i]` is how
 * tall it is.
 */
export type ScrollIndex = {
  /** The content width, viewport height and gap the stack was built for. */
  width: number,
  view: number,
  gap: number,
  /** The first index whose height is not known to be current. */
  from: number,
  tops: Array<number>,
  heights: Array<number>,
  /** Rows of content the whole stack adds up to. */
  content: number,
};

/** A resolved size, in whole cells. */
export type Size = { readonly width: number, readonly height: number };

const clamp = (value: number, low: number, high: number): number =>
  Math.min(Math.max(value, low), high);

type Edges<T> = [T, T, T, T];

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
function padding(style: LayoutStyle): Edges<number> {
  const all = style.padding ?? 0;
  return [
    style.paddingTop ?? all,
    style.paddingRight ?? all,
    style.paddingBottom ?? all,
    style.paddingLeft ?? all,
  ];
}

/** Margin on each edge, with the shorthand applied first. */
function margin(style: LayoutStyle): Edges<Margin> {
  const all = style.margin ?? 0;
  return [
    style.marginTop ?? all,
    style.marginRight ?? all,
    style.marginBottom ?? all,
    style.marginLeft ?? all,
  ];
}

/** The fixed part of a margin. `auto` is resolved only by the flex parent. */
function marginCells(value: Margin): number {
  return value === "auto" ? 0 : value;
}

/** A margin tuple with every `auto` edge treated as zero cells. */
function fixedMargins(edges: Edges<Margin>): Edges<number> {
  return [
    marginCells(edges[0]),
    marginCells(edges[1]),
    marginCells(edges[2]),
    marginCells(edges[3]),
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
  // The same offer twice is the same answer twice, and the answer is only
  // stale when something under the node changed — which is a fact React knows
  // and layout does not, so `internal/tree.js` is what clears this. Without
  // it, a scrolling box that has not changed re-measures every child on every
  // frame, and measuring a line of text means walking it grapheme by grapheme.
  if (node.measuredForWidth === availableWidth && node.measuredForHeight === availableHeight) {
    return { width: node.measuredWidth, height: node.measuredHeight };
  }
  const size = measureIntrinsic(node, availableWidth, availableHeight);
  node.measuredForWidth = availableWidth;
  node.measuredForHeight = availableHeight;
  node.measuredWidth = size.width;
  node.measuredHeight = size.height;
  return size;
}

function measureIntrinsic(node: LayoutNode, availableWidth: number, availableHeight: number): Size {
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
  } else if (style.overflow === "scroll") {
    // A scrolling box's height is its content's, which the stack already
    // knows; and its width is whatever it was offered, because there is no
    // horizontal scrolling and so nothing about a child can widen it. Asking
    // the children for a width would also be asking ten thousand of them, and
    // the answer would be the width of a row that may be nowhere near the
    // window — which is the coupling this whole component exists to break.
    contentWidth = innerAvailableWidth;
    // A box that was given a height has already answered this, and the answer
    // below is thrown away — so the stack is not consulted for it. That also
    // avoids keying one on a viewport this pass can only guess at, which
    // `layout` would then have to rebuild whole at the height it settles on.
    contentHeight =
      fixedHeight != null
        ? 0
        : scrollStack(node, innerAvailableWidth, innerAvailableHeight).content;
  } else {
    const row = isRow(direction(node.style));
    const gap = gapOf(style, row);
    let main = 0;
    let cross = 0;
    let counted = 0;
    for (const child of node.children) {
      const [marginTop, marginRight, marginBottom, marginLeft] = fixedMargins(margin(child.style));
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
  // Cleared before the branch that may set it, so that a box which has run out
  // of children does not leave the painter a range into a list that no longer
  // has those rows in it.
  node.scrollFirst = 0;
  node.scrollCount = 0;

  if (node.children.length === 0) {
    // A scrolling box that has lost its content has nothing to say about where
    // in it the window is, and a bar drawn from what it said last frame is a
    // control pointing into rows that are gone.
    node.scrollHeight = 0;
    node.scrollOffset = 0;
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
  const resolvedMargins = margins.map((edges) => fixedMargins(edges));
  const bases = children.map((child, index) => {
    const [marginTop, marginRight, marginBottom, marginLeft] = resolvedMargins[index];
    const availableWidth = Math.max(0, contentWidth - marginLeft - marginRight);
    const availableHeight = Math.max(0, contentHeight - marginTop - marginBottom);
    const basis = resolve(child.style.flexBasis, mainSpace);
    if (basis != null) {
      return basis;
    }
    const fixedMain = resolve(row ? child.style.width : child.style.height, mainSpace);
    if (fixedMain != null) {
      return fixedMain;
    }
    const size = intrinsicSize(child, availableWidth, availableHeight);
    return row ? size.width : size.height;
  });

  const outerMain = (index: number): number => {
    const [marginTop, marginRight, marginBottom, marginLeft] = resolvedMargins[index];
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
        (row
          ? resolvedMargins[index][3] + resolvedMargins[index][1]
          : resolvedMargins[index][0] + resolvedMargins[index][2]),
      0,
    ) +
    Math.max(0, children.length - 1) * gap;
  const slack = Math.max(0, mainSpace - consumed);

  const placed = resolvedMargins.map((edges) => edges.slice());
  const autoMain: Array<[number, number]> = [];
  if (slack > 0) {
    for (let index = 0; index < margins.length; index += 1) {
      const edges = margins[index];
      if (row) {
        if (edges[3] === "auto") autoMain.push([index, 3]);
        if (edges[1] === "auto") autoMain.push([index, 1]);
      } else {
        if (edges[0] === "auto") autoMain.push([index, 0]);
        if (edges[2] === "auto") autoMain.push([index, 2]);
      }
    }
  }
  const justifySlack = autoMain.length === 0 ? slack : 0;
  const autoMainShares = distribute(
    slack,
    autoMain.map(() => 1),
  );
  for (let index = 0; index < autoMain.length; index += 1) {
    const [childIndex, edge] = autoMain[index];
    placed[childIndex][edge] += autoMainShares[index];
  }

  const justify = style.justifyContent ?? "flex-start";
  let cursor = 0;
  let between = gap;
  if (justify === "center") {
    cursor = Math.floor(justifySlack / 2);
  } else if (justify === "flex-end") {
    cursor = justifySlack;
  } else if (justify === "space-between" && children.length > 1) {
    between = gap + Math.floor(justifySlack / (children.length - 1));
  } else if (justify === "space-around" && children.length > 0) {
    const each = Math.floor(justifySlack / children.length);
    cursor = Math.floor(each / 2);
    between = gap + each;
  } else if (justify === "space-evenly" && children.length > 0) {
    const each = Math.floor(justifySlack / (children.length + 1));
    cursor = each;
    between = gap + each;
  }

  const order = reverse ? children.map((_, i) => i).reverse() : children.map((_, i) => i);
  const parentAlign = style.alignItems ?? "stretch";

  for (const index of order) {
    const child = children[index];
    const [marginTop, marginRight, marginBottom, marginLeft] = placed[index];
    const mainMarginStart = row ? marginLeft : marginTop;
    const mainMarginEnd = row ? marginRight : marginBottom;
    const crossMarginStart = row ? marginTop : marginLeft;
    const crossMarginEnd = row ? marginBottom : marginRight;
    const crossStartEdge = row ? 0 : 3;
    const crossEndEdge = row ? 2 : 1;
    const autoCrossStart = margins[index][crossStartEdge] === "auto";
    const autoCrossEnd = margins[index][crossEndEdge] === "auto";
    const hasAutoCross = autoCrossStart || autoCrossEnd;

    const align = (() => {
      const own = child.style.alignSelf ?? "auto";
      return own === "auto" ? parentAlign : own;
    })();

    const crossAvailable = Math.max(0, crossSpace - crossMarginStart - crossMarginEnd);
    const fixedCross = resolve(row ? child.style.height : child.style.width, crossSpace);
    let crossSize: number;
    if (fixedCross != null) {
      crossSize = fixedCross;
    } else if (align === "stretch" && !hasAutoCross) {
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
    if (hasAutoCross) {
      const remaining = Math.max(0, crossSpace - crossSize - crossMarginStart - crossMarginEnd);
      const autoCrossShares = distribute(remaining, [autoCrossStart ? 1 : 0, autoCrossEnd ? 1 : 0]);
      crossOffset += autoCrossShares[0];
    } else if (align === "center") {
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
 * A child is **measured** once — its height at this width — and the height is
 * kept on the child until something under it changes. The total is what
 * `scrollTop` is clamped against, so a box that guessed at its own content
 * height would let `Number.MAX_SAFE_INTEGER` scroll into empty space; keeping
 * the answer is how that total stays exact without being recomputed.
 *
 * Only the children that intersect the window are **laid out**, and the
 * painter is given their range rather than a flag per child, so the rest get
 * no position, no size, no walk into their subtrees and no visit at all. What
 * remains proportional to the number of children is the stack of heights, and
 * {@link ScrollIndex} rebuilds only the part of it that changed.
 *
 * So moving the window over ten thousand rows touches the rows in the window
 * and nothing else, whatever the other nine thousand nine hundred and
 * seventy-six are — which is the property the whole component exists for, and
 * the reason this is not `overflow: "hidden"` with a margin on top.
 */
function layoutScroll(node: LayoutNode, x: number, y: number, width: number, height: number): void {
  const children = node.children;
  const stack = scrollStack(node, width, height);
  const content = stack.content;

  const offset = clamp(Math.floor(node.style.scrollTop ?? 0), 0, Math.max(0, content - height));
  node.scrollHeight = content;
  node.scrollOffset = offset;
  node.scrollViewTop = y;
  node.scrollViewRows = height;
  // The column immediately right of the content, which is the one `ScrollBox`
  // reserved by adding one to its right padding.
  node.scrollBarColumn = x + width;

  // The bottom of a child's border box only ever moves down the list, so the
  // first child the window reaches can be found without looking at the ones
  // above it. This is the step that would otherwise make the offset — the one
  // thing about a scrolling box that changes every frame — cost the content.
  const first = firstVisible(stack, offset);
  let count = 0;
  for (let index = first; index < children.length; index += 1) {
    const top = stack.tops[index];
    // The border box, not the outer one: a margin draws nothing, so a child
    // whose own box has left the window has left it.
    if (top - offset >= height) {
      break;
    }
    const child = children[index];
    const [, marginRight, , marginLeft] = fixedMargins(margin(child.style));
    const available = Math.max(0, width - marginLeft - marginRight);
    const childWidth = Math.min(available, resolve(child.style.width, width) ?? available);
    layout(child, x + marginLeft, y + top - offset, childWidth, stack.heights[index]);
    count += 1;
  }
  node.scrollFirst = first;
  node.scrollCount = count;
}

/**
 * The first child whose border box has not finished above `offset`.
 *
 * A binary search rather than a scan, because a scan is the thing this is
 * replacing. `tops[i] + heights[i]` never decreases as `i` grows — every term
 * between two children is a height, a margin or a gap, and none of those is
 * negative — which is exactly the ordering a binary search needs.
 */
function firstVisible(stack: ScrollIndex, offset: number): number {
  let low = 0;
  let high = stack.heights.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    if (stack.tops[middle] + stack.heights[middle] > offset) {
      high = middle;
    } else {
      low = middle + 1;
    }
  }
  return low;
}

/**
 * Where each child of a scrolling box sits, rebuilding only what moved.
 *
 * Margins are part of the stack, exactly as they are in the flex path and in
 * {@link intrinsicSize}: a child's outer height is what the next one starts
 * after and what the scroll range is made of. Leaving them out of any one of
 * those makes the content shorter than it is drawn, which is a bottom the
 * offset clamps to too early and a last row nobody can scroll to.
 *
 * The width, the viewport height and the gap are part of the key rather than
 * inputs to a patch: all three change every height in the stack at once, so
 * there is nothing to salvage. The viewport is in there because a child may
 * give its height as a percentage, and the percentage is of the window.
 */
function scrollStack(node: LayoutNode, width: number, height: number): ScrollIndex {
  const children = node.children;
  const gap = gapOf(node.style, false);
  let stack = node.scrollIndex;
  if (stack == null || stack.width !== width || stack.view !== height || stack.gap !== gap) {
    stack = {
      width,
      view: height,
      gap,
      from: 0,
      tops: [],
      heights: [],
      content: 0,
    };
    node.scrollIndex = stack;
  }

  // What React changed since the last frame, taken rather than read: acting on
  // it twice would be harmless and leaving it set would make every later frame
  // rebuild from the same index forever.
  if (node.scrollDirtyFrom >= 0) {
    stack.from = Math.min(stack.from, node.scrollDirtyFrom);
    node.scrollDirtyFrom = -1;
  }

  // Children the tree no longer has. `from` is clamped rather than reset: a
  // removal has already put it at zero, and an unmount during a Suspense
  // fallback must not be able to leave an index pointing past the end.
  if (stack.heights.length > children.length) {
    stack.heights.length = children.length;
    stack.tops.length = children.length;
    stack.from = Math.min(stack.from, children.length);
  }

  if (children.length === 0) {
    stack.content = 0;
    stack.from = 0;
    return stack;
  }

  if (stack.from < children.length) {
    // What a scrolling box offers a child that has not been given a height:
    // nothing in particular. That is what scrolling means — a child is as tall
    // as its content and the window decides how much is seen — and the only
    // thing this basis is read for is a percentage `minHeight` or `maxHeight`
    // on the child, which inside a scroll region is a question with no good
    // answer.
    const unbounded = Number.MAX_SAFE_INTEGER;
    // Where the child before the first stale one ended. Read back out of the
    // stack rather than carried, because a rebuild may start anywhere and only
    // the entries below it are known to be current.
    let cursor = 0;
    if (stack.from > 0) {
      const previous = children[stack.from - 1];
      const [, , previousBottom] = fixedMargins(margin(previous.style));
      cursor = stack.tops[stack.from - 1] + stack.heights[stack.from - 1] + previousBottom + gap;
    }
    for (let index = stack.from; index < children.length; index += 1) {
      const child = children[index];
      const [marginTop, marginRight, marginBottom, marginLeft] = fixedMargins(margin(child.style));
      const available = Math.max(0, width - marginLeft - marginRight);
      const fixedHeight = resolve(child.style.height, height);
      const own = fixedHeight ?? intrinsicSize(child, available, unbounded).height;
      stack.tops[index] = cursor + marginTop;
      stack.heights[index] = own;
      cursor += marginTop + own + marginBottom + gap;
    }
    // `cursor` counts a gap after the last child, which the content does not
    // have. Subtracting it at the end rather than skipping the first one is
    // what makes the running total patchable from any index.
    stack.content = cursor - gap;
    stack.from = children.length;
  }

  return stack;
}

// Not implemented here, on purpose, and tracked rather than discovered:
//
// - `flexWrap`. OpenTUI's default is `"no-wrap"` and a terminal layout that
//   wraps its flex line is rare enough that guessing at the semantics would be
//   worse than not having them.
// - `position: "absolute"`. It needs a containing-block concept that nothing
//   in this package has yet, and every use of it so far has been better served
//   by a box that grows.
// - `aspectRatio`. A terminal resolves whole cells, and nothing here has a
//   fractional layout pass to lean on.
//
// All three are ubugeeei-prod/uf#314.
