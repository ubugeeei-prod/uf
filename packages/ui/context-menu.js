// @flow
//
// The same menu, opened by the right button.
//
// Everything below the trigger is `menu.js` — the arrow keys, typeahead,
// submenus, `Escape` stacking, the roving tab stop and the two checkable item
// kinds — because a context menu *is* a menu and a second implementation of one
// would be a second set of keyboard bugs. What is here is the two things that
// make it a component rather than an `oncontextmenu` handler, and both of them
// are the parts people leave out.
//
// # It has to be reachable from the keyboard
//
// `Shift+F10` and the `ContextMenu` key open a context menu, on every platform,
// and a component that only listens for `contextmenu` is a WCAG 2.1.1 failure:
// the commands in it are reachable by pointer and by nothing else. Long press
// is the touch equivalent of the same gesture, and `@uniflowed/hooks/dom`'s
// `useLongPress` already knows what a long press is — including that a press
// that moves is a drag and not a press.
//
// The trigger is therefore focusable. That is a real cost and it is stated
// rather than hidden: a list of two hundred rows with a context menu on each is
// two hundred tab stops. A caller whose trigger already *contains* something
// focusable should pass `tabIndex={-1}` and let the keys arrive from inside it,
// which they do — the handler is on the trigger and the event bubbles. What is
// not on offer is leaving the keys out, because the alternative to a tab stop
// is a command a keyboard cannot reach.
//
// # It opens at a point, and sometimes at an element
//
// A context menu opened by the pointer belongs at the pointer — the reader is
// looking at their cursor, and a menu that appeared against the top-left corner
// of a table row is a menu they have to go and find. Opened by the keyboard
// there is no pointer, and the menu belongs against the element that has focus.
//
// So the anchor is a rectangle rather than an element, and
// `internal/anchor.js`'s `anchorRect` is the seam: the trigger element is still
// what the writing direction is read from and what focus goes back to, and only
// the *measurement* is replaced. `null` — which is what the keyboard path
// leaves behind — measures the trigger, so both routes end in one code path
// rather than two placements that drift.
//
// # The body is not named after the trigger
//
// `Menu.Body` names itself with `aria-labelledby` pointing at its trigger,
// because a dropdown menu's trigger is a button with a short label — "File" —
// and that is the menu's name. A context menu's trigger is arbitrary content: a
// table row, a canvas, a paragraph. Naming the menu after it would announce the
// whole row as the menu's name. So `ContextMenu.Trigger` registers itself as
// the thing focus returns to and *not* as a name, and the caller gives
// `ContextMenu.Body` an `aria-label`. That is the one attribute this component
// cannot supply and the reference page says so.

"use client";

import * as React from "@uniflowed/react";
import { useCallback, useContext, useMemo, useRef, useState } from "@uniflowed/react";
import { useLongPress } from "@uniflowed/hooks/dom";

import type { Rect } from "./internal/anchor.js";
import type { PartEvent, RenderProp, Rest } from "./internal/merge-props.js";
import {
  composeHandlers,
  composeRefs,
  withProps,
  withoutComposed,
} from "./internal/merge-props.js";
import { MenuAnchorContext, MenuContext, MenuLevel, useMenu } from "./internal/menu-tree.js";

/**
 * Where the pointer was, or nothing when the keyboard opened the menu.
 *
 * Held by the root rather than by the trigger because the *body* is what reads
 * it, and the body is a sibling of the trigger rather than a child of it.
 */
type PointState = {|
  readonly point: Rect | null,
  readonly openAt: (point: Rect | null) => void,
|};

const PointContext: React.Context<PointState | null> = React.createContext(null);

/** A zero-sized box at a pointer's coordinates, which is what a point is. */
function pointAt(x: number, y: number): Rect {
  return { height: 0, width: 0, x, y };
}

/**
 * The trigger, the menu, and where the pointer was when it opened.
 *
 * Renders no element of its own, for the reason `Menu.Root` gives: the trigger
 * and the body are siblings in whatever layout the caller wrote.
 */
export component ContextMenuRoot(
  children: React.Node,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  // State rather than a ref, and that is load-bearing: the rectangle is one of
  // the things the placement effect re-runs for, so a second right-click
  // somewhere else has to be a new value React has committed rather than a
  // mutation nothing heard about.
  const [point, setPoint] = useState<Rect | null>(null);
  const openAt = useCallback((next: Rect | null) => setPoint(next), []);
  const state = useMemo(() => ({ point, openAt }), [point, openAt]);

  return (
    <PointContext.Provider value={state}>
      <MenuAnchorContext.Provider value={point}>
        <MenuLevel defaultOpen={defaultOpen} onOpenChange={onOpenChange} open={open} parent={null}>
          {children}
        </MenuLevel>
      </MenuAnchorContext.Provider>
    </PointContext.Provider>
  );
}

hook usePoint(part: string): PointState {
  const state = useContext(PointContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a ContextMenu.Root`);
  }
  return state;
}

/**
 * The content the menu belongs to.
 *
 * A `<div>` rather than a button, because what a context menu hangs off is a
 * region of the page. The module header says why it is in the tab order and
 * when a caller should take it out again.
 */
export component ContextMenuTrigger(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const menu = useMenu("ContextMenu.Trigger");
  const { openAt } = usePoint("ContextMenu.Trigger");
  const triggerRef = useRef<HTMLElement | null>(null);
  const passed = withoutComposed(rest, ["onContextMenu", "onKeyDown", "ref"]);

  const openHere = useCallback(() => {
    // No point: the menu goes against the element, which is where the reader's
    // focus already is.
    openAt(null);
    menu.pendingFocus.current = "first";
    menu.setOpen(true);
  }, [menu, openAt]);

  // The touch equivalent of the right button. `useLongPress` cancels itself
  // when the pointer moves, so a drag across a list is not two hundred menus.
  useLongPress(triggerRef, (event: Event) => {
    const pointer: $FlowFixMe = event;
    openAt(pointAt(pointer.clientX ?? 0, pointer.clientY ?? 0));
    menu.pendingFocus.current = "first";
    menu.setOpen(true);
  });

  const props = withProps(
    // The `tabIndex` goes *underneath* the caller's props, alone, because it is
    // the one attribute here a caller is invited to overrule: the module header
    // promises `tabIndex={-1}` to a caller whose trigger already contains
    // something focusable, and a value that won over the caller's would be a
    // documented escape hatch that does nothing. Everything in the second
    // argument is this component's own and stays on top.
    withProps({ tabIndex: 0 }, passed),
    {
      "aria-haspopup": "menu",
      children,
      id: `${menu.base}-trigger`,
      onContextMenu: composeHandlers(rest.onContextMenu, (event: PartEvent) => {
        const press: $FlowFixMe = event;
        // The browser's own menu would otherwise cover this one, and the reader
        // would be looking at the platform's Back/Reload rather than at the
        // commands the page has for what they pressed on.
        press.preventDefault();
        openAt(pointAt(press.clientX ?? 0, press.clientY ?? 0));
        menu.pendingFocus.current = "first";
        menu.setOpen(true);
      }),
      onKeyDown: composeHandlers(rest.onKeyDown, (event: PartEvent) => {
        // Both spellings. `ContextMenu` is the dedicated key on a PC keyboard;
        // `Shift+F10` is the one every platform has, and is what a laptop
        // without that key leaves a reader with.
        const asked =
          event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey === true);
        if (!asked) {
          return;
        }
        event.preventDefault();
        openHere();
      }),
      ref: composeRefs(rest.ref, (element: HTMLElement | null) => {
        triggerRef.current = element;
        // What focus goes back to when the menu closes. It is deliberately not
        // registered as the menu's *name*; see the module header.
        menu.triggerRef.current = element;
      }),
    },
  );

  if (render != null) {
    return render(props);
  }
  return <div {...props} />;
}

export type { MenuSelect } from "./menu.js";
