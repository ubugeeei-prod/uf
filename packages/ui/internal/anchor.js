// @flow
//
// Where an overlay goes, given where its trigger is.
//
// A popover, a tooltip and a hover card are the same three sentences: a box
// appears next to another box, it stays on the screen when there is no room
// where it was asked to go, and it says where it ended up so a stylesheet can
// point an arrow at the trigger. Written into each component, those three
// sentences are three copies that agree until one of them is fixed.
//
// # Flip *and* shift, because a flip alone is not enough
//
// There are two answers to "it does not fit", and a positioning layer needs
// both:
//
//   * **Flip** — the main axis. A menu whose trigger is near the bottom of the
//     viewport opens upwards instead of off the page, and reports
//     `data-side="top"` so the arrow moves with it.
//   * **Shift** — the cross axis. A menu wider than its trigger, aligned to a
//     trigger near the right edge, has nowhere to flip *to*: both alignments
//     overflow when the overlay is wider than the room on either side of the
//     anchor. Sliding it back along the trigger is the only answer that keeps
//     it on screen, and it is what Floating UI calls `shift`.
//
// Flipping the *alignment* — `start` becomes `end` when the aligned edge would
// leave the viewport — is the thing that looks like a shift and is not. It
// moves the overlay by its own width, which is a jump the reader sees, and it
// still overflows in exactly the case above. So the alignment never flips here:
// the side flips, the cross axis slides, and `data-align` keeps saying what the
// caller asked for.
//
// # One tier, measured, and why there is no declarative second one
//
// The platform grew this capability: CSS anchor positioning places an element
// against an `anchor-name` in the compositor, with `position-try-fallbacks` for
// the collision case and no JavaScript on the scroll path. It is the better
// mechanism and it is deliberately not used here, because it cannot express
// half of what this module promises:
//
//   * `position-area` places a box in one of a grid of regions around the
//     anchor. There is no spelling that says "and then move it back inside the
//     viewport": the property chooses a region, it does not clamp a coordinate.
//   * `position-try-fallbacks`, with the built-in `flip-block` / `flip-inline`
//     or a hand-written `@position-try` block, tries another *placement* when
//     the first overflows. Every one of those is a flip. None is a slide.
//   * `position-area: span-all` with `justify-self: anchor-center` centres the
//     overlay on the anchor and then needs a `margin-inline` to pull it back
//     inside — and that margin is a number only a measurement produces, so the
//     declarative path would be a measured path wearing a stylesheet.
//
// A tier that flips and a tier that flips *and* slides put the same overlay in
// two different places, and which one a reader gets depends on their browser.
// Shipping one tier that both engines run is worth more than shipping the
// faster one to some of them, so this module measures. It is the reason
// `packages/ui` has no `CSS.supports` in it: there is nothing to branch on.
//
// The measuring is kept cheap rather than clever. A placement is computed on
// open, on a scroll anywhere in the page, on a viewport resize and on a resize
// of either box — and each of those is one `getBoundingClientRect` per box
// followed by the arithmetic below, which reads nothing else.
//
// # Why the arithmetic is a function and not a hook
//
// `placeOverlay` takes three rectangles and returns a placement. It touches no
// element, no window and no React, so the whole of the collision behaviour —
// every flip, every slide, right-to-left alignment, the overlay that is wider
// than the viewport — is testable by calling it with numbers. That matters more
// here than in most modules: the DOM these components are tested in computes no
// layout at all, so a test that went through the hook could only assert *that*
// a position was applied, never that it was the right one.
//
// # Why an overlay is `position: fixed`
//
// Because the coordinates are the viewport's. It also gets most of what a
// portal is usually reached for: a fixed box is not clipped by an ancestor's
// `overflow: hidden`, so a menu in a table row or a scroll container is not cut
// in half — without moving the element away from its trigger in the
// accessibility tree, which is the trade `dialog.js`'s header refuses to make.
// The exception is an ancestor with `transform`, `filter` or `will-change`,
// which becomes the containing block for a fixed descendant; that is the CSS
// the top layer would answer, and answering it is ubugeeei-prod/uf#256's
// remaining half rather than this module's.
//
// # Why this is `internal/` and not a subpath
//
// The same reason `roving-focus.js` gives. This is not a positioning library,
// it is the relationship the overlay parts build: a `data-side` that agrees
// with the coordinates, one set of custom-property names for a stylesheet to
// read, and one definition of what `start` means in a right-to-left page.
// Exported, a consumer could build a part that positions itself differently and
// still calls itself an overlay.

import { useEffect, useState } from "@uniflowed/react";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";

import type { Direction } from "./roving-focus.js";
import { directionOf } from "./roving-focus.js";

/**
 * Which side of its trigger an overlay opens onto.
 *
 * Physical rather than logical, which is the opposite of `Align` below and is
 * deliberate: `side` answers "above or below", and the two horizontal ones are
 * named after the screen because a design that puts a popover to the right of
 * a toolbar means the right of the toolbar in any writing direction. What the
 * writing direction changes is the *alignment*, which is where the reading
 * order actually lives.
 */
export type Side = "top" | "right" | "bottom" | "left";

/**
 * A side a caller may ask for, which is the four above plus the two that mean
 * "the way the reader reads".
 *
 * The WAI-ARIA menu pattern puts a submenu on the inline end - to the right of a
 * left-to-right menu and to the left of a right-to-left one - and `menu.js`
 * already spells the *keys* that way, so a physical-only `side` would have left
 * one half of that pattern mirrored and the other half not. `Placement` and
 * `placeOverlay` stay physical: a logical side is resolved once, against the
 * trigger's own direction, before any arithmetic sees it.
 */
export type LogicalSide = Side | "inline-start" | "inline-end";

/**
 * Where the overlay sits along the trigger's other axis.
 *
 * Logical: `start` is the left edge in a left-to-right page and the right edge
 * in a right-to-left one, so a menu aligned to the start of its trigger opens
 * the way the reader reads in both.
 */
export type Align = "start" | "center" | "end";

/** A box, in viewport coordinates: what `getBoundingClientRect` reports. */
export type Rect = {|
  readonly x: number,
  readonly y: number,
  readonly width: number,
  readonly height: number,
|};

/** Everything `placeOverlay` needs, and nothing it could read for itself. */
export type Anchoring = {|
  /** The trigger. */
  readonly anchor: Rect,
  /** The overlay. Only its size is used; where it is now does not matter. */
  readonly overlay: Rect,
  /** What it has to stay inside, which is the viewport for a fixed element. */
  readonly viewport: Rect,
  readonly side: Side,
  readonly align: Align,
  /** The gap between the trigger and the overlay, in pixels. */
  readonly sideOffset: number,
  /** A nudge along the cross axis, in the direction the page reads. */
  readonly alignOffset: number,
  /** Whether to flip and slide at all. `false` places it exactly as asked. */
  readonly avoidCollisions: boolean,
  /** How close to the viewport edge the overlay may come. */
  readonly collisionPadding: number,
  readonly direction: Direction,
|};

/** Where the overlay goes, and what the placement had to do to get there. */
export type Placement = {|
  readonly x: number,
  readonly y: number,
  /** The side it ended up on, which is the requested one unless it flipped. */
  readonly side: Side,
  /** The requested alignment. It never changes; see the module header. */
  readonly align: Align,
  /** How far it slid along the cross axis, so an arrow can be moved back. */
  readonly shift: number,
  /** The room between the trigger and the viewport edge, and across it. */
  readonly availableWidth: number,
  readonly availableHeight: number,
|};

/** What `useAnchor` reports back for `data-side` and `data-align`. */
export type Anchored = {|
  readonly side: Side,
  readonly align: Align,
|};

/** What `useAnchor` is told, on top of the geometry `placeOverlay` needs. */
export type AnchorRequest = {|
  readonly anchorRef: { current: HTMLElement | null },
  /**
   * A box to place against instead of the anchor element's own.
   *
   * A context menu opens *at a point* rather than against an element: the
   * pointer coordinates the reader right-clicked at, which is a zero-sized
   * rectangle no element in the document has. The element is still needed —
   * it is what the writing direction is read from, what a `ResizeObserver`
   * watches, and what focus returns to — so this replaces the *measurement*
   * and nothing else.
   *
   * `null` (and absent) means "measure the element", which is every other
   * overlay in this package.
   *
   * It has to be stable between renders for the same position, because it is
   * one of the things the placement effect re-runs for; a fresh object each
   * render would re-measure on every render of the page around it.
   */
  readonly anchorRect?: Rect | null,
  readonly overlayRef: { current: HTMLElement | null },
  /** Nothing is measured while it is closed: there is nothing to measure. */
  readonly open: boolean,
  /** Resolved against the trigger's writing direction; see `LogicalSide`. */
  readonly side: LogicalSide,
  readonly align: Align,
  readonly sideOffset: number,
  readonly alignOffset: number,
  readonly avoidCollisions: boolean,
  readonly collisionPadding: number,
|};

/** The side a flip goes to. */
const OPPOSITE: { readonly [Side]: Side } = {
  top: "bottom",
  bottom: "top",
  left: "right",
  right: "left",
};

/**
 * The side on the screen that `side` names for a reader reading `direction`.
 *
 * The four physical ones pass through unchanged, because a design that puts a
 * popover to the right of a toolbar means the right of the toolbar in Arabic
 * too; `Side`'s own documentation says why that is the useful default and why
 * alignment is the axis that mirrors.
 */
export function physicalSide(side: LogicalSide, direction: Direction): Side {
  // Written out rather than left to a wildcard, so that a sixth side added to
  // `LogicalSide` one day is a checker error here instead of a value that falls
  // through unresolved.
  return match (side) {
    "inline-start" => direction === "rtl" ? "right" : "left",
    "inline-end" => direction === "rtl" ? "left" : "right",
    "top" => "top",
    "right" => "right",
    "bottom" => "bottom",
    "left" => "left",
  };
}

/** Whether a side stacks the overlay above the trigger or beside it. */
function isVertical(side: Side): boolean {
  return side === "top" || side === "bottom";
}

/**
 * The space between each edge of the anchor and the edge of the viewport,
 * already minus the padding the overlay must not come inside.
 *
 * Negative for an anchor that is itself off the screen, which is a real state
 * — a trigger scrolled halfway out of a container — and is why nothing below
 * assumes these are positive.
 */
function roomAround(
  anchor: Rect,
  viewport: Rect,
  collisionPadding: number,
): { readonly [Side]: number } {
  return {
    top: anchor.y - (viewport.y + collisionPadding),
    bottom: viewport.y + viewport.height - collisionPadding - (anchor.y + anchor.height),
    left: anchor.x - (viewport.x + collisionPadding),
    right: viewport.x + viewport.width - collisionPadding - (anchor.x + anchor.width),
  };
}

/**
 * The side the overlay actually fits on.
 *
 * The requested side wins whenever it fits, because a component that moved an
 * overlay it did not have to move is one a designer cannot lay out against.
 * When it does not fit and the opposite side does, it flips. When *neither*
 * fits — a tall overlay next to a trigger in the middle of a short viewport —
 * it takes the side with more room, so the reader sees as much of it as the
 * page allows and `availableHeight` tells the stylesheet how much that was.
 */
function sideThatFits(anchoring: Anchoring, room: { readonly [Side]: number }): Side {
  const { overlay, side, sideOffset } = anchoring;
  const needed = (isVertical(side) ? overlay.height : overlay.width) + sideOffset;
  const opposite = OPPOSITE[side];
  if (room[side] >= needed) {
    return side;
  }
  if (room[opposite] >= needed) {
    return opposite;
  }
  return room[opposite] > room[side] ? opposite : side;
}

/**
 * Where the overlay's leading edge goes along the cross axis, before sliding.
 *
 * `start` is the anchor's leading edge, `end` puts the overlay's trailing edge
 * on the anchor's, and `center` splits the difference. Which edge is "leading"
 * is the writing direction's business, and only on a horizontal cross axis: a
 * page that reads right to left still reads top to bottom, so an overlay beside
 * its trigger aligns to the top for `start` either way.
 */
function alignedAt(
  align: Align,
  mirrored: boolean,
  anchorStart: number,
  anchorSize: number,
  overlaySize: number,
): number {
  const resolved = mirrored ? mirror(align) : align;
  return match (resolved) {
    "start" => anchorStart,
    "center" => anchorStart + anchorSize / 2 - overlaySize / 2,
    "end" => anchorStart + anchorSize - overlaySize,
  };
}

/** `start` and `end` swapped, for a right-to-left page. */
function mirror(align: Align): Align {
  return match (align) {
    "start" => "end",
    "center" => "center",
    "end" => "start",
  };
}

/**
 * Place `overlay` against `anchor` inside `viewport`.
 *
 * Pure arithmetic over three rectangles: no element, no window, no React. See
 * the module header for why that is the point rather than a convenience.
 */
export function placeOverlay(anchoring: Anchoring): Placement {
  const {
    align,
    alignOffset,
    anchor,
    avoidCollisions,
    collisionPadding,
    direction,
    overlay,
    sideOffset,
    viewport,
  } = anchoring;

  const room = roomAround(anchor, viewport, collisionPadding);
  const side = avoidCollisions ? sideThatFits(anchoring, room) : anchoring.side;
  const vertical = isVertical(side);

  // The main axis: hard against the trigger's edge, plus the gap. The overlay
  // is never pushed along this axis, because pushing it would slide it over
  // the trigger it is meant to be pointing at.
  const main = match (side) {
    "top" => anchor.y - overlay.height - sideOffset,
    "bottom" => anchor.y + anchor.height + sideOffset,
    "left" => anchor.x - overlay.width - sideOffset,
    "right" => anchor.x + anchor.width + sideOffset,
  };

  // The cross axis: aligned, nudged, then slid back inside if it has to be.
  const anchorStart = vertical ? anchor.x : anchor.y;
  const anchorSize = vertical ? anchor.width : anchor.height;
  const overlaySize = vertical ? overlay.width : overlay.height;
  const viewportStart = vertical ? viewport.x : viewport.y;
  const viewportSize = vertical ? viewport.width : viewport.height;
  // A positive `alignOffset` moves the overlay the way the page reads, so the
  // same number nudges a menu in the same visual direction as its own text.
  const reading = vertical && direction === "rtl" ? -1 : 1;
  const wanted =
    alignedAt(align, vertical && direction === "rtl", anchorStart, anchorSize, overlaySize) +
    alignOffset * reading;

  const lowest = viewportStart + collisionPadding;
  const highest = viewportStart + viewportSize - collisionPadding - overlaySize;
  // `Math.max` last, so an overlay *wider* than the viewport — where `highest`
  // is below `lowest` and no position satisfies both — is pinned to the leading
  // edge rather than pushed off the far one. `availableWidth` is what a
  // stylesheet reads to stop it being that wide in the first place.
  const cross = avoidCollisions ? Math.max(lowest, Math.min(wanted, highest)) : wanted;

  const alongSide = Math.max(0, room[side] - sideOffset);
  const acrossSide = Math.max(0, viewportSize - collisionPadding * 2);

  return {
    align,
    availableHeight: vertical ? alongSide : acrossSide,
    availableWidth: vertical ? acrossSide : alongSide,
    shift: cross - wanted,
    side,
    x: vertical ? cross : main,
    y: vertical ? main : cross,
  };
}

/**
 * Write a placement onto the overlay.
 *
 * Imperatively, and only these seven properties. They change on every scroll
 * frame while an overlay is open, and putting them in state would re-render the
 * overlay and everything in it sixty times a second to move a box — which is
 * exactly the cost `index.js`'s header says this package does not pay. React
 * owns nothing written here: it never sets `left`, `top` or a custom property
 * on these elements, and a caller who passes a `style` of their own keeps every
 * property in it except the ones this component's position is made of.
 *
 * The two measurements are the ones a caller cannot compute for themselves. A
 * `Select` popup that must match its trigger's width needs the trigger
 * measured; a menu near the bottom of the page needs to know how much room is
 * left before it can decide to scroll rather than overflow.
 */
function write(overlay: HTMLElement, anchor: Rect, placement: Placement): void {
  const style = overlay.style;
  style.position = "fixed";
  style.left = `${placement.x}px`;
  style.top = `${placement.y}px`;
  style.setProperty("--uf-anchor-trigger-width", `${anchor.width}px`);
  style.setProperty("--uf-anchor-trigger-height", `${anchor.height}px`);
  style.setProperty("--uf-anchor-available-width", `${placement.availableWidth}px`);
  style.setProperty("--uf-anchor-available-height", `${placement.availableHeight}px`);
  // How far the slide moved it, so an arrow drawn by a stylesheet can be moved
  // back the other way and keep pointing at the trigger.
  style.setProperty("--uf-anchor-shift", `${placement.shift}px`);
}

/** The rectangle of an element, as the arithmetic above wants it. */
function rectOf(element: HTMLElement): Rect {
  const box = element.getBoundingClientRect();
  return { height: box.height, width: box.width, x: box.left, y: box.top };
}

/**
 * Keep an overlay against its trigger for as long as it is open.
 *
 * Returns the side and alignment it settled on, which the part renders as
 * `data-side` and `data-align`. Those are state — they change rarely, a
 * stylesheet has to see them, and a flip is exactly the moment an arrow has to
 * move — while the coordinates are written straight to the element; `write`
 * says why.
 */
export hook useAnchor(request: AnchorRequest): Anchored {
  const {
    align,
    alignOffset,
    anchorRect,
    anchorRef,
    avoidCollisions,
    collisionPadding,
    open,
    overlayRef,
    side,
    sideOffset,
  } = request;
  // Absent and `null` are one answer here — "measure the element" — so the two
  // spellings become one value before anything depends on it.
  const virtual = anchorRect ?? null;
  // The left-to-right reading of a logical side, which is what `Anchored`
  // reports until something has been measured. In a right-to-left page a
  // submenu's `inline-end` is the *left*, and the first `reflow` says so - one
  // commit later, exactly as a flip does, and for the same reason: the direction
  // is a fact about the document, and a render may not read one.
  const [settled, setSettled] = useState<Anchored>({ align, side: physicalSide(side, "ltr") });

  const reflow = useStableCallback(() => {
    const anchor = anchorRef.current;
    const overlay = overlayRef.current;
    const view = overlay?.ownerDocument?.defaultView;
    if (anchor == null || overlay == null || view == null) {
      return;
    }
    const box = virtual ?? rectOf(anchor);
    const placement = placeOverlay({
      align,
      alignOffset,
      anchor: box,
      avoidCollisions,
      collisionPadding,
      direction: directionOf(anchor),
      overlay: rectOf(overlay),
      side: physicalSide(side, directionOf(anchor)),
      sideOffset,
      // The viewport of a fixed element, which is the whole of it: a fixed box
      // is positioned against the viewport rather than against whatever is
      // scrolled around it.
      viewport: { height: view.innerHeight, width: view.innerWidth, x: 0, y: 0 },
    });
    write(overlay, box, placement);
    setSettled((current) =>
      current.side === placement.side && current.align === placement.align
        ? // The same object, so a scroll that changes nothing renders nothing.
          current
        : { align: placement.align, side: placement.side },
    );
  });

  useEffect(() => {
    const anchor = anchorRef.current;
    const overlay = overlayRef.current;
    const view = overlay?.ownerDocument?.defaultView;
    if (!open || anchor == null || overlay == null || view == null) {
      // Forget the measurement when the overlay closes. It is only read while
      // `open`, but a body that stays mounted across a close — `PopoverBody`
      // does — reopens in a commit where `open` is already `true`, and would
      // report the *previous* opening's side until `reflow` corrects it from
      // an effect, which runs after paint. That is one frame of an arrow
      // drawn from `data-side` pointing the wrong way, after a reopen that
      // follows a flip.
      if (!open) {
        const asked = physicalSide(side, anchor == null ? "ltr" : directionOf(anchor));
        setSettled((current) =>
          current.side === asked && current.align === align ? current : { align, side: asked },
        );
      }
      return;
    }
    reflow();

    const document = overlay.ownerDocument;
    const moved = () => reflow();
    // Capture, because a scroll event does not bubble: a trigger inside a
    // scrolling panel would otherwise move under an overlay that never heard
    // about it. Passive, because this never prevents the scroll it is watching.
    document.addEventListener("scroll", moved, { capture: true, passive: true });
    view.addEventListener("resize", moved);
    // A trigger that grows — a button whose label changed, a field that gained
    // a second line — moves the overlay without any scroll or resize event
    // being fired at all.
    //
    // Read off the window rather than through a local: a capitalised name
    // holding a constructor is read as a React component by `uf lint`, and it
    // is right to — the rule cannot tell this one from a component, and the
    // window's own property is the thing being asked about anyway.
    const host: $FlowFixMe = view;
    const sizes = typeof host.ResizeObserver === "function" ? new host.ResizeObserver(moved) : null;
    sizes?.observe(anchor);
    sizes?.observe(overlay);

    return () => {
      document.removeEventListener("scroll", moved, true);
      view.removeEventListener("resize", moved);
      sizes?.disconnect();
    };
    // The measurements are read through `reflow`, which is stable and always
    // has the latest of them; they are named here so that changing one — a
    // caller flipping `side` on a breakpoint — measures again.
  }, [
    align,
    alignOffset,
    anchorRect,
    anchorRef,
    avoidCollisions,
    collisionPadding,
    open,
    overlayRef,
    reflow,
    side,
    sideOffset,
  ]);

  // What was asked for until there is something measured to report, so the
  // first commit — and the server's markup, which measures nothing at all —
  // says the requested side rather than the last one some other opening
  // happened to settle on.
  return open ? settled : { align, side: physicalSide(side, "ltr") };
}
