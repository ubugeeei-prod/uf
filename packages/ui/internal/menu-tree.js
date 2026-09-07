// @flow
//
// What every menu in this package is a menu *of*.
//
// `menu.js` was one module because there was one menu. shadcn ships four
// components on this behaviour — a dropdown menu, a context menu, a menubar and
// the checkable items all three of them share — and the three that are not the
// dropdown differ from it in exactly two places: what opens the menu, and where
// the menu goes. Everything between those two — the tree of open levels, what
// `Escape` closes, what "choosing an item" dismisses, which selector finds an
// item and which finds its owner — is one set of rules, and this is it.
//
// Written down here rather than exported from `menu.js` for the reason every
// other module in `internal/` gives: these are relationships the components
// build, not a menu-building kit. `MENU_SELECTOR` is only true because
// `Menu.Body` renders `role="menu"`; handed to a consumer it would be a
// selector that happens to work.
//
// # A menu is a chain, not a flag
//
// A submenu is a menu whose parent is another menu, and almost every rule that
// distinguishes the two is a statement about that chain:
//
//   * `Escape` closes *one* level, so it needs to know which one it is in.
//   * Choosing an item closes the whole chain, because leaving the parent menu
//     open after a command has run is a state no native menu has been in.
//   * Only the outermost level listens for a press outside, because a submenu
//     closes with the tree and two listeners would each answer one press.
//
// So the chain is the shape the context carries, and `parent` is what a level
// is given rather than something it works out.
//
// # Where a menu goes, when it is not against its trigger
//
// A context menu opens at the pointer, which is a *point* and not an element.
// `MenuAnchorContext` is how a level says so: a rectangle that replaces the
// trigger's own measurement inside `internal/anchor.js`, and nothing else — the
// trigger element is still what the writing direction is read from and what
// focus goes back to. A level with no provider above it measures its trigger,
// which is every menu that hangs off a button.

"use client";

import * as React from "@uniflowed/react";
import {
  createContext,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "@uniflowed/react";

import type { Rect } from "./anchor.js";
import type { Direction } from "./roving-focus.js";
import { useControlled } from "./controlled-state.js";

/**
 * Anything that plays the part of a menu item, including the two checkable
 * kinds. The keyboard has to move between all of them, so the selector names
 * all of them rather than only the plain command.
 */
export const ITEM_SELECTOR: string =
  '[role="menuitem"], [role="menuitemcheckbox"], [role="menuitemradio"]';

/** What owns an item: the nearest menu, so a submenu keeps its own. */
export const MENU_SELECTOR: string = '[role="menu"]';

/**
 * Which arrow key opens a submenu, and which closes it.
 *
 * The WAI-ARIA menu pattern puts a submenu on the *inline end*, so it opens to
 * the right of a left-to-right menu and to the left of a right-to-left one, and
 * the key that opens it is the one pointing at it. Written out as
 * `ArrowRight` to open and `ArrowLeft` to close, an RTL reader pressed the key
 * aimed at the submenu and closed the menu they were standing in — which is
 * worse than nothing happening, because it loses their place.
 */
export function submenuKeys(direction: Direction): {|
  readonly open: string,
  readonly close: string,
|} {
  return direction === "rtl"
    ? { open: "ArrowLeft", close: "ArrowRight" }
    : { open: "ArrowRight", close: "ArrowLeft" };
}

export type MenuState = {|
  readonly base: string,
  readonly open: boolean,
  readonly setOpen: (open: boolean) => void,
  /** What opened this menu, and what focus goes back to when it closes. */
  readonly triggerRef: { current: HTMLElement | null },
  /**
   * Which end the menu should open onto, written by whatever opened it.
   *
   * A ref rather than state because it is an instruction for the next commit,
   * not a value anything renders: `ArrowUp` on a closed menu opens it *and*
   * lands on the last item, and re-rendering the trigger to say so would be a
   * render whose only purpose is to carry a message to an effect.
   */
  readonly pendingFocus: { current: "first" | "last" | null },
  /** The menu this one hangs off, or null for the outermost. */
  readonly parent: MenuState | null,
  /**
   * Whether a trigger is rendered *and* is something worth naming the menu
   * after, so the body only claims a name that exists.
   *
   * A menu opened by `defaultOpen` in a page that never renders a trigger is a
   * real arrangement, and an `aria-labelledby` pointing at the id that trigger
   * *would* have had makes a screen reader announce nothing at all. A context
   * menu's trigger is arbitrary content rather than a label, and registers
   * itself as a trigger without registering itself as a name.
   */
  readonly triggered: boolean,
  readonly registerTrigger: (present: boolean) => void,
|};

export const MenuContext: React.Context<MenuState | null> = createContext(null);

/**
 * The roving tab stop of one open menu.
 *
 * Provided by `Menu.Body` rather than by the root, because a submenu is a
 * second list with a tab stop of its own: nesting the provider is what stops
 * the parent menu and the submenu from fighting over which item is `tabindex=0`.
 */
export type MenuListState = {|
  readonly activeId: string | null,
  readonly setActiveId: (id: string | null) => void,
|};

export const MenuListContext: React.Context<MenuListState | null> = createContext(null);

/**
 * A box a menu body is placed against instead of its trigger's own.
 *
 * `null`, and no provider at all, both mean "measure the trigger". See the
 * module header, and `AnchorRequest.anchorRect` for what it replaces.
 */
export const MenuAnchorContext: React.Context<Rect | null> = createContext(null);

export hook useMenu(part: string): MenuState {
  const state = useContext(MenuContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Menu.Root`);
  }
  return state;
}

/**
 * Tell the menu that something worth naming it after is in the document.
 *
 * `Menu.Body` names its trigger with `aria-labelledby`, and it may only do that
 * while there is one to name — a menu opened by `defaultOpen` in a page with no
 * trigger would otherwise point at an id nothing has, and a screen reader given
 * a dangling `aria-labelledby` announces nothing at all rather than falling back
 * to the element's own content.
 */
export hook useTriggerRegistration(menu: MenuState): void {
  const register = menu.registerTrigger;
  useEffect(() => {
    register(true);
    return () => register(false);
  }, [register]);
}

/** Every menu from `menu` outwards, innermost first. */
export function ancestry(menu: MenuState): Array<MenuState> {
  const chain = [];
  let at: MenuState | null = menu;
  while (at != null) {
    chain.push(at);
    at = at.parent;
  }
  return chain;
}

/**
 * Close this menu and every menu it hangs off.
 *
 * Choosing an item in a submenu dismisses the whole thing — leaving the parent
 * menu open after a command has run is a state no native menu has ever been in,
 * and it leaves the reader looking at a menu whose action already happened.
 */
export function closeTree(menu: MenuState): void {
  for (const each of ancestry(menu)) {
    each.setOpen(false);
  }
}

/**
 * One level of the menu tree.
 *
 * Shared by `Menu.Root`, `Menu.Sub`, `ContextMenu.Root` and `Menubar.Menu`,
 * which differ only in what they hand it: a submenu knows its parent, and a
 * menubar's menu is opened and closed by the bar rather than by itself.
 */
export component MenuLevel(
  children: React.Node,
  parent: MenuState | null,
  defaultOpen: boolean,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  const base = useId();
  const [isOpen, setOpen] = useControlled(open, defaultOpen, onOpenChange);
  const triggerRef = useRef<HTMLElement | null>(null);
  const pendingFocus = useRef<"first" | "last" | null>(null);
  const [triggered, setTriggered] = useState(false);

  const state = useMemo(
    () => ({
      base,
      open: isOpen,
      setOpen,
      triggerRef,
      pendingFocus,
      parent,
      triggered,
      registerTrigger: setTriggered,
    }),
    [base, isOpen, setOpen, parent, triggered],
  );

  return <MenuContext.Provider value={state}>{children}</MenuContext.Provider>;
}
