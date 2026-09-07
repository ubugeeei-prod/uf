// @flow
//
// A row of menus that behaves as one control: File, Edit, View.
//
// It is not a row of `Menu.Root`s, and the reason is that the *set* has a
// keyboard map of its own — one that only exists because the menus are next to
// each other:
//
//   * The whole bar takes **one** stop in the page's tab order, so `Tab` past an
//     application menu is one press rather than six.
//   * `ArrowLeft` / `ArrowRight` move between the top-level menus, mirrored in a
//     right-to-left page because they are the inline axis.
//   * `ArrowDown` opens the menu under the cursor and lands on its first item;
//     `ArrowUp` opens it onto its last, which is the same argument `Menu.Trigger`
//     makes about the destructive command at the bottom of a long menu.
//   * `Home` / `End` go to the first and last menu.
//   * And the part that is always missing: `ArrowLeft` / `ArrowRight` **while a
//     menu is open** close it and open the adjacent one, so a reader can walk
//     File → Edit → View without pressing `Escape` between them. A menubar
//     without it makes the arrow keys mean two different things depending on
//     whether a menu happens to be showing.
//   * `Escape` closes the open menu and leaves focus on its trigger, in the bar,
//     which `Menu.Body` already does — a menubar trigger is the menu's trigger.
//
// Everything inside a menu is `menu.js`: the arrow keys within it, typeahead,
// submenus, the checkable items and the roving tab stop of the menu itself.
// `Menubar.Body` is `Menu.Body` itself rather than a wrapper around it, because
// a bar's menu *is* a root menu — it hangs off a button and drops from it — and
// `Menu.Body` already places a root menu on the bottom, aligned to the start. A
// wrapper would have been a second component with the same defaults written out
// again, and a second place for them to drift.
//
// # How the bar finds its own triggers
//
// `internal/roving-focus.js`'s `itemsOf(container, item, owner)` takes the owner
// selector as a parameter for exactly this: a set says what owns it. A menu
// passes `[role="menu"]`, and a menubar has to pass **both** — an open menu is a
// DOM descendant of the bar, and its items are `role="menuitem"` too, so a bar
// that only asked "menu items inside me" would step into the open menu's items
// with `ArrowRight`. Naming the two owners makes `closest` stop at the menu for
// an item inside one and at the bar for a trigger, which is the distinction, and
// it is settled by the roles the two containers already carry.
//
// Turning a trigger *element* back into the menu it opens is the one thing the
// roles cannot say, and `data-uf-menubar-value` is that and nothing more. The
// alternative is a registry of refs, which `roving-focus.js` explains at length
// is a second opinion about document order.
//
// # Which menu is open is the bar's state
//
// One value, `string | null`, rather than a boolean per menu. Two menus open at
// once is the state this component exists to prevent, and a per-menu boolean is
// a set of booleans somebody has to keep exclusive; `accordion.js` makes the
// same argument about `single`. It is also what makes "close this one and open
// the next" a single assignment rather than a pair of them that render twice.

"use client";

import * as React from "@uniflowed/react";
import {
  createContext,
  useCallback,
  useContext,
  useMemo,
  useRef,
  useState,
} from "@uniflowed/react";

import type { Rest } from "./internal/merge-props.js";
import { composeHandlers, composeRefs, withoutComposed } from "./internal/merge-props.js";
import type { RovingSet } from "./internal/roving-focus.js";
import {
  directionOf,
  indexOfActive,
  itemsOf,
  movementFor,
  moveTo,
  useFirstItem,
} from "./internal/roving-focus.js";
import { MenuLevel, useMenu } from "./internal/menu-tree.js";

/**
 * The bar's own items, and the two things that may own one.
 *
 * See the module header: an open menu is inside the bar and its items wear the
 * same role, so the owner selector names both and `closest` settles it.
 */
const TRIGGERS: RovingSet = {
  item: '[role="menuitem"]',
  owner: '[role="menu"], [role="menubar"]',
  orientation: "horizontal",
  wrap: true,
  skipDisabled: true,
};

type MenubarState = {|
  /** Which menu is showing, by the `value` its `Menubar.Menu` was given. */
  readonly open: string | null,
  readonly setOpen: (value: string | null) => void,
  /** Which trigger holds the bar's single tab stop, or null for "the first". */
  readonly active: string | null,
  readonly setActive: (value: string) => void,
  readonly firstId: string | null,
|};

const MenubarContext: React.Context<MenubarState | null> = createContext(null);

/** The `value` of the `Menubar.Menu` a trigger belongs to. */
const MenubarMenuContext: React.Context<string | null> = createContext(null);

hook useMenubar(part: string): MenubarState {
  const state = useContext(MenubarContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Menubar.Root`);
  }
  return state;
}

/**
 * The bar: `role="menubar"`, one tab stop, and the arrows between the menus.
 *
 * `aria-label` is the caller's and matters more here than on most containers —
 * a page with an application menubar and a formatting toolbar has two, and
 * "menu bar" twice tells a reader nothing about which is which.
 */
export component MenubarRoot(children: renders* MenubarMenu, ...rest: Rest) {
  const barRef = useRef<HTMLElement | null>(null);
  const [open, setOpenValue] = useState<string | null>(null);
  const [active, setActive] = useState<string | null>(null);
  // Only while nothing has been focused or opened. Once a trigger holds the tab
  // stop, asking the document which one comes first is work with no reader.
  const firstId = useFirstItem(barRef, TRIGGERS, active == null);

  const setOpen = useCallback((value: string | null) => {
    setOpenValue(value);
    if (value != null) {
      setActive(value);
    }
  }, []);

  const state = useMemo(
    () => ({ open, setOpen, active, setActive, firstId }),
    [open, setOpen, active, firstId],
  );
  const passed = withoutComposed(rest, ["onKeyDown", "ref"]);

  return (
    <MenubarContext.Provider value={state}>
      <div
        {...passed}
        aria-orientation="horizontal"
        onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
          const bar: $FlowFixMe = event.currentTarget;
          const movement = movementFor(event.key, "horizontal", directionOf(bar));
          if (movement == null) {
            return;
          }
          const triggers = itemsOf(bar, TRIGGERS.item, TRIGGERS.owner);
          // With a menu open, focus is on one of *its* items rather than on a
          // trigger, so "where am I in the bar" is the expanded trigger. This
          // is the whole of walking File → Edit → View without pressing Escape.
          const focused = indexOfActive(triggers, bar.ownerDocument?.activeElement);
          const at =
            focused >= 0
              ? focused
              : triggers.findIndex((each) => each.getAttribute("aria-expanded") === "true");
          const next = moveTo(triggers, at, movement, TRIGGERS.wrap, TRIGGERS.skipDisabled);
          if (next == null) {
            return;
          }
          // Claimed before focus moves, or the browser scrolls the page under
          // the trigger that has just taken it; `moveOnKey` says the same.
          event.preventDefault();
          event.stopPropagation();
          const value = next.getAttribute("data-uf-menubar-value");
          if (open != null && value != null) {
            // Swap which menu is showing. Focus lands on the new menu's first
            // item through `Menu.Body`'s own opening effect, so nothing here
            // moves it: focusing the trigger as well would be two focus moves
            // in one commit and the reader would see the second.
            setOpen(value);
            return;
          }
          next.focus();
          if (value != null) {
            setActive(value);
          }
        })}
        ref={composeRefs(rest.ref, (element) => {
          barRef.current = element;
        })}
        role="menubar"
      >
        {children}
      </div>
    </MenubarContext.Provider>
  );
}

/**
 * One menu of the bar, named by the `value` the bar opens and closes it with.
 *
 * Renders no element of its own, for the reason `Menu.Root` gives: a trigger and
 * its body are siblings in whatever layout the caller wrote.
 */
export component MenubarMenu(children: React.Node, value: string) {
  const bar = useMenubar("Menubar.Menu");
  const setOpen = bar.setOpen;
  const onOpenChange = useCallback(
    (next: boolean) => setOpen(next ? value : null),
    [setOpen, value],
  );

  return (
    <MenubarMenuContext.Provider value={value}>
      <MenuLevel
        defaultOpen={false}
        onOpenChange={onOpenChange}
        open={bar.open === value}
        parent={null}
      >
        {children}
      </MenuLevel>
    </MenubarMenuContext.Provider>
  );
}

/**
 * The button that opens one of the bar's menus.
 *
 * `role="menuitem"` rather than a plain button, because it *is* an item of the
 * menubar — a reader is told "File, menu item, has popup, 1 of 3" — and it is
 * what makes the bar's arrow keys agree with what they were told is in it.
 */
export component MenubarTrigger(children: React.Node, ...rest: Rest) {
  const bar = useMenubar("Menubar.Trigger");
  const menu = useMenu("Menubar.Trigger");
  const value = useContext(MenubarMenuContext);
  if (value == null) {
    throw new Error("Menubar.Trigger must be rendered inside a Menubar.Menu");
  }
  const passed = withoutComposed(rest, ["onClick", "onFocus", "onKeyDown", "ref"]);
  const id = `${menu.base}-trigger`;
  // The bar's single tab stop. Before anything has been focused or opened it
  // belongs to the first trigger, which is a fact about the document rather
  // than about this component — `useFirstItem` reads it in the bar.
  const stop = bar.active == null ? bar.firstId === id : bar.active === value;

  return (
    <button
      {...passed}
      aria-controls={menu.open ? `${menu.base}-body` : undefined}
      aria-expanded={menu.open ? "true" : "false"}
      aria-haspopup="menu"
      // How the bar's keyboard turns a trigger element back into the menu it
      // opens; the module header says why this is an attribute and the rest of
      // the bar's arithmetic is not.
      data-uf-menubar-value={value}
      id={id}
      onClick={composeHandlers(rest.onClick, () => bar.setOpen(menu.open ? null : value))}
      onFocus={composeHandlers(rest.onFocus, () => bar.setActive(value))}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        const end = match (event.key) {
          "ArrowDown" => "first",
          "ArrowUp" => "last",
          _ => null,
        };
        if (end == null) {
          return;
        }
        event.preventDefault();
        menu.pendingFocus.current = end;
        bar.setOpen(value);
      })}
      ref={composeRefs(rest.ref, (element) => {
        menu.triggerRef.current = element;
      })}
      role="menuitem"
      tabIndex={stop ? 0 : -1}
      type="button"
    >
      {children}
    </button>
  );
}
