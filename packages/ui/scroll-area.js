// @flow
//
// A scroll area: a scrollbar you drew yourself, and the keyboard you took away
// when you did.
//
// # What it gives that `overflow: auto` does not
//
// A `<div style="overflow: auto">` scrolls with the wheel, with a trackpad, and
// with a finger. What it does not reliably do is scroll from the keyboard,
// because it may not be focusable: Firefox makes a scrollable region focusable,
// Chromium historically does not, and Safari's answer depends on the setting
// that controls whether `Tab` reaches anything but form controls. So a region
// that must be scrolled to be read is, in most browsers, a region a keyboard
// reader can see the top of and nothing else. That is WCAG 2.1.1, and it is the
// entire reason to have this component rather than the `div`:
//
//   * **`tabindex="0"`, `role="region"` and a name.** The tab stop is what
//     makes the arrow keys and `PageDown` work; the role and the name are what
//     keep a tab stop from being a mystery — a focusable `div` with no name is
//     announced as nothing at all, which is a worse place to land than the
//     `div` was.
//   * **Nothing is intercepted.** No `onKeyDown`, no `onWheel`, no
//     `scroll-behavior` written from JavaScript. Every key that scrolls a
//     native overflow container scrolls this one, because this one *is* a
//     native overflow container and the component's whole contribution is not
//     getting in its way. A scroll area that reimplemented `PageDown` would
//     have to reimplement `Home`, `End`, the space bar, caret browsing and
//     whatever the reader's own software sends, and would get one of them
//     wrong.
//   * **`scrollIntoView({ block: "nearest" })` still works.** `combobox.js` and
//     `select.js` both call it to keep the active option visible, so a
//     `Combobox.List` inside a `ScrollArea` is a case that has to work. It does
//     because the viewport is a plain scroll container and nothing here
//     overrides `scrollTop`; the one time this module writes it is described
//     below, and it is exactly the case where the browser has already thrown
//     the position away.
//   * **The position survives a re-render.** Replacing the content of a scroll
//     container — a filtered list, a new page of results — makes the browser
//     clamp `scrollTop` to a shorter document and it does not put it back. The
//     viewport remembers where the reader actually scrolled to, from the
//     `scroll` event, and restores it after a commit that lost it. A reader who
//     scrolls to the top themselves fires a `scroll` event, so the remembered
//     position is theirs and this never fights them.
//
// # The scrollbar is a picture
//
// `ScrollArea.Scrollbar` is `aria-hidden` and holds no controls. That is the
// decision the component is made of: the *region* is the thing that scrolls and
// the keyboard is how it is operated, so a drawn scrollbar has nothing it must
// be able to do — which means it never has to answer WCAG 2.5.7's question
// about dragging, because nothing here is achievable only by dragging. It
// reports where the content is as two custom properties and stays out of the
// accessibility tree, where a second, mouse-only copy of the scroll position
// would be noise.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useEffect, useId, useMemo, useRef } from "@uniflowed/react";
import { useEventListener } from "@uniflowed/hooks/dom";

import type { Orientation } from "./internal/roving-focus.js";
import type { Rest } from "./internal/merge-props.js";
import { composeRefs, withoutComposed } from "./internal/merge-props.js";

export type { Orientation } from "./internal/roving-focus.js";

/** Where the reader last actually was, per axis. */
type Offset = {| x: number, y: number |};

type ScrollAreaState = {|
  readonly base: string,
  readonly label: string,
  readonly viewportRef: { current: HTMLElement | null },
  readonly remembered: { current: Offset },
  /** Written by the viewport, read by every scrollbar. */
  readonly report: () => void,
  readonly scrollbars: { current: Array<HTMLElement> },
|};

const ScrollAreaContext: React.Context<ScrollAreaState | null> = createContext(null);

/**
 * The scroll area a part belongs to.
 *
 * Raising rather than returning null, for the reason `useDialog` gives: a
 * `ScrollArea.Scrollbar` outside a root would draw a thumb for a viewport it
 * has never measured, and it would look correct until the content moved.
 */
hook useScrollArea(part: string): ScrollAreaState {
  const state = useContext(ScrollAreaContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a ScrollArea.Root`);
  }
  return state;
}

/**
 * The box the viewport and the scrollbars sit in.
 *
 * `label` is required and lives here rather than on the viewport, because the
 * name belongs to the whole component: it is what a reader hears when `Tab`
 * lands them in it, and a scroll area that has to be scrolled to be read and is
 * announced as "region" has told them nothing.
 */
export component ScrollAreaRoot(children: React.Node, label: string, ...rest: Rest) {
  const base = useId();
  const viewportRef = useRef<HTMLElement | null>(null);
  const remembered = useRef<Offset>({ x: 0, y: 0 });
  const scrollbars = useRef<Array<HTMLElement>>([]);

  const state = useMemo(
    () => ({
      base,
      label,
      remembered,
      report: () => {
        const viewport = viewportRef.current;
        if (viewport == null) {
          return;
        }
        for (const scrollbar of scrollbars.current) {
          write(scrollbar, viewport);
        }
      },
      scrollbars,
      viewportRef,
    }),
    [base, label],
  );

  return (
    <ScrollAreaContext.Provider value={state}>
      <div {...rest}>{children}</div>
    </ScrollAreaContext.Provider>
  );
}

/**
 * The element that actually scrolls: a named region, in the tab sequence.
 *
 * It carries no key handling at all. See the module header — every key that
 * scrolls a native overflow container scrolls this one because it is one, and
 * the component's contribution is the tab stop that lets those keys arrive.
 */
export component ScrollAreaViewport(children: React.Node, ...rest: Rest) {
  const area = useScrollArea("ScrollArea.Viewport");
  const { remembered, report, viewportRef } = area;
  const passed = withoutComposed(rest, ["ref"]);

  useEventListener(viewportRef, "scroll", () => {
    const viewport = viewportRef.current;
    if (viewport == null) {
      return;
    }
    // The reader's own position, including a deliberate scroll back to the
    // top — which is why the restore below never fights them.
    remembered.current = { x: viewport.scrollLeft, y: viewport.scrollTop };
    report();
  });

  // After every commit, because a commit is what replaces the content: a
  // shorter document makes the browser clamp the offset to fit and it does not
  // put it back when the content grows again.
  useEffect(() => {
    const viewport = viewportRef.current;
    if (viewport == null) {
      return;
    }
    const { x, y } = remembered.current;
    if (y !== 0 && viewport.scrollTop === 0) {
      viewport.scrollTop = y;
    }
    if (x !== 0 && viewport.scrollLeft === 0) {
      viewport.scrollLeft = x;
    }
    report();
  });

  return (
    <div
      {...passed}
      aria-label={area.label}
      id={`${area.base}-viewport`}
      ref={composeRefs(rest.ref, (element: HTMLElement | null) => {
        viewportRef.current = element;
      })}
      // A named region, which is what makes the tab stop below explicable
      // rather than a place a reader lands and cannot account for.
      role="region"
      // The whole component. Without it the arrow keys and `PageDown` never
      // arrive, and the bottom of this box is unreachable from a keyboard in
      // every browser that does not make scroll containers focusable.
      tabIndex={0}
    >
      {children}
    </div>
  );
}

/**
 * The drawn scrollbar: two numbers and no semantics.
 *
 * `--uf-scroll-thumb-size` is the thumb's length as a fraction of the track and
 * `--uf-scroll-thumb-offset` is where along it the thumb sits, both between 0
 * and 1, so a stylesheet can draw one with a `scale` and a `translate` and
 * measure nothing. `aria-hidden`, because the region it belongs to is already
 * the thing a reader operates.
 */
export component ScrollAreaScrollbar(
  children?: React.Node,
  orientation?: Orientation = "vertical",
  ...rest: Rest
) {
  const area = useScrollArea("ScrollArea.Scrollbar");
  const { report, scrollbars } = area;
  const passed = withoutComposed(rest, ["ref"]);

  return (
    <div
      {...passed}
      // A picture of the scroll position is not something a screen reader has
      // any use for: it cannot be operated, and the region it describes
      // announces itself.
      aria-hidden="true"
      data-orientation={orientation}
      ref={composeRefs(rest.ref, (element: HTMLElement | null) => {
        const kept = scrollbars.current.filter((each) => each !== element);
        scrollbars.current = element == null ? kept : [...kept, element];
        report();
      })}
    >
      {children}
    </div>
  );
}

/**
 * Write where the content is onto a scrollbar.
 *
 * Imperatively, and only these two properties, for the reason
 * `internal/anchor.js` gives about a placement: they change on every scroll
 * frame, and re-rendering the scroll area and everything in it sixty times a
 * second to move a thumb is the cost this package does not pay. React sets
 * neither property, so a caller's `style` keeps everything in it.
 *
 * Both axes are written on every scrollbar rather than the one its
 * `data-orientation` names, because a stylesheet reads the pair it wants and a
 * branch here would be a second place the orientation is decided.
 */
function write(scrollbar: HTMLElement, viewport: HTMLElement): void {
  const style = scrollbar.style;
  style.setProperty(
    "--uf-scroll-thumb-size",
    String(fraction(viewport.clientHeight, viewport.scrollHeight)),
  );
  style.setProperty(
    "--uf-scroll-thumb-offset",
    String(fraction(viewport.scrollTop, viewport.scrollHeight - viewport.clientHeight)),
  );
  style.setProperty(
    "--uf-scroll-thumb-size-x",
    String(fraction(viewport.clientWidth, viewport.scrollWidth)),
  );
  style.setProperty(
    "--uf-scroll-thumb-offset-x",
    String(fraction(viewport.scrollLeft, viewport.scrollWidth - viewport.clientWidth)),
  );
}

/**
 * `part / whole`, clamped, and 1 when there is no whole.
 *
 * A document that computes no layout reports every measurement as zero, and
 * `0 / 0` is `NaN` — which a stylesheet reading the custom property renders as
 * a thumb of no size at all rather than as a full-length one.
 */
function fraction(part: number, whole: number): number {
  if (!(whole > 0)) {
    return 1;
  }
  return Math.min(1, Math.max(0, part / whole));
}
