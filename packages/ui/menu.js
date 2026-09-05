// @flow
//
// A menu, which is the widget whose keyboard map people know by feel and cannot
// name.
//
// Every native menu on every platform has behaved the same way for thirty
// years, so a reader arrives already knowing what the keys do — and notices
// immediately when one of them does nothing:
//
//   * `ArrowDown` / `ArrowUp` move between items and wrap at the ends.
//   * `Home` / `End` go to the first and last item.
//   * Typing letters jumps to an item by prefix, and typing the same letter
//     again cycles between the items that start with it. A thirty-item menu
//     without typeahead is thirty arrow presses.
//   * `Escape` closes *this* menu — the submenu if one is open, not the whole
//     tree — and gives focus back to what opened it.
//   * `ArrowRight` opens a submenu and lands on its first item; `ArrowLeft`
//     closes it and comes back to the item that opened it.
//   * `Tab` closes the menu and carries on through the page, rather than
//     walking the reader through thirty items they have already dismissed.
//
// # Focus moves; `aria-activedescendant` does not appear here
//
// A menu moves *real* DOM focus onto its items. That is what WAI-ARIA
// prescribes for this pattern, and it is why the items are buttons: activation,
// disabled semantics and the focus ring are the browser's rather than this
// component's. `aria-activedescendant` — a "virtual" focus that stays on the
// container — belongs to the pattern where focus cannot leave a text field,
// which is the combobox, and `combobox.js` uses it there.
//
// # Why hovering does not open a submenu
//
// It does nothing here on purpose. Opening on hover requires an intent
// heuristic — the "safe triangle" that lets the pointer travel diagonally
// across a sibling item to reach the submenu without it snapping shut — and a
// naive `onPointerEnter` that opens immediately is *worse* than no hover at
// all: it opens menus the reader was only passing over and closes the one they
// were aiming at. Keyboard and click open a submenu; a deliberate hover
// implementation is tracked work, not a line to be added carelessly.
//
// # Items are found in the document, not in a registry
//
// `internal/roving-focus.js` explains why. The short version is that mount
// order stops being document order the first time an item is conditional, and
// a submenu's items live *inside* its parent menu's element.

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
import { useStableCallback } from "@uniflowed/hooks/lifecycle";

import { composeHandlers, composeRefs, withoutComposed } from "./internal/merge-props.js";
import {
  indexOfActive,
  isTypeaheadKey,
  itemsOf,
  movementFor,
  moveTo,
  useTypeahead,
} from "./internal/roving-focus.js";
import { useControlled } from "./internal/controlled-state.js";

/**
 * Anything that plays the part of a menu item, including the two checkable
 * kinds a caller may write themselves. The keyboard has to move between all of
 * them, so the selector names all of them rather than only what this package
 * ships.
 */
const ITEM_SELECTOR = '[role="menuitem"], [role="menuitemcheckbox"], [role="menuitemradio"]';

/** What owns an item: the nearest menu, so a submenu keeps its own. */
const MENU_SELECTOR = '[role="menu"]';

type MenuState = {|
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
   * Whether a trigger is rendered, so the body only names one that exists.
   *
   * A menu opened by `defaultOpen` in a page that never renders a trigger is a
   * real arrangement, and an `aria-labelledby` pointing at the id that trigger
   * *would* have had makes a screen reader announce nothing at all.
   */
  readonly triggered: boolean,
  readonly registerTrigger: (present: boolean) => void,
|};

const MenuContext: React.Context<MenuState | null> = createContext(null);

/**
 * The roving tab stop of one open menu.
 *
 * Provided by `Menu.Body` rather than by the root, because a submenu is a
 * second list with a tab stop of its own: nesting the provider is what stops
 * the parent menu and the submenu from fighting over which item is `tabindex=0`.
 */
type MenuListState = {|
  readonly activeId: string | null,
  readonly setActiveId: (id: string | null) => void,
|};

const MenuListContext: React.Context<MenuListState | null> = createContext(null);

/** The id of a group's label, so `Menu.Group` only claims one that exists. */
type MenuGroupState = {|
  readonly labelId: string,
  readonly registerLabel: (present: boolean) => void,
|};

const MenuGroupContext: React.Context<MenuGroupState | null> = createContext(null);

hook useMenu(part: string): MenuState {
  const state = useContext(MenuContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Menu.Root`);
  }
  return state;
}

/**
 * Tell the menu that a trigger for it is in the document.
 *
 * `Menu.Body` names its trigger with `aria-labelledby`, and it may only do that
 * while there is one to name — a menu opened by `defaultOpen` in a page with no
 * trigger would otherwise point at an id nothing has, and a screen reader given
 * a dangling `aria-labelledby` announces nothing at all rather than falling back
 * to the element's own content.
 */
hook useTriggerRegistration(menu: MenuState): void {
  const register = menu.registerTrigger;
  useEffect(() => {
    register(true);
    return () => register(false);
  }, [register]);
}

/** Every menu from `menu` outwards, innermost first. */
function ancestry(menu: MenuState): Array<MenuState> {
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
function closeTree(menu: MenuState): void {
  for (const each of ancestry(menu)) {
    each.setOpen(false);
  }
}

/**
 * A menu and its trigger.
 *
 * Renders no element of its own: a menu's trigger and its body are siblings in
 * whatever layout the caller wrote, and a wrapper would put a `<div>` between
 * them that the caller then has to style around.
 */
export component MenuRoot(
  children: React.Node,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  return (
    <MenuLevel defaultOpen={defaultOpen} onOpenChange={onOpenChange} open={open} parent={null}>
      {children}
    </MenuLevel>
  );
}

/**
 * A submenu: a menu whose trigger is an item of the menu around it.
 *
 * It is the same component as a root menu with one difference — it knows its
 * parent — and that difference is what `ArrowLeft`, `Escape` and "choosing an
 * item closes everything" are all defined in terms of.
 */
export component MenuSub(
  children: React.Node,
  defaultOpen?: boolean = false,
  open?: boolean,
  onOpenChange?: (open: boolean) => void,
) {
  const parent = useContext(MenuContext);
  if (parent == null) {
    throw new Error("Menu.Sub must be rendered inside a Menu.Root");
  }
  return (
    <MenuLevel defaultOpen={defaultOpen} onOpenChange={onOpenChange} open={open} parent={parent}>
      {children}
    </MenuLevel>
  );
}

/** One level of the menu tree. Shared by `Menu.Root` and `Menu.Sub`. */
component MenuLevel(
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

/** The button that opens the menu. */
export component MenuTrigger(children: React.Node, ...rest: { readonly [string]: mixed }) {
  const menu = useMenu("Menu.Trigger");
  const passed = withoutComposed(rest, ["onClick", "onKeyDown", "ref"]);
  useTriggerRegistration(menu);

  return (
    <button
      {...passed}
      // Named only while the menu is in the document, so a reader is never told
      // to go somewhere that is not there.
      aria-controls={menu.open ? `${menu.base}-body` : undefined}
      aria-expanded={menu.open ? "true" : "false"}
      aria-haspopup="menu"
      id={`${menu.base}-trigger`}
      onClick={composeHandlers(rest.onClick, () => menu.setOpen(!menu.open))}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        // `ArrowUp` opening onto the *last* item is the behaviour that makes a
        // long menu usable: the last entry is usually the destructive one, and
        // reaching it should not mean arrowing past everything else.
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
        menu.setOpen(true);
      })}
      ref={composeRefs(rest.ref, (element) => {
        menu.triggerRef.current = element;
      })}
      type="button"
    >
      {children}
    </button>
  );
}

/**
 * The menu itself: the roving tab stop, the arrow keys, typeahead and Escape.
 *
 * The keys are handled here rather than on each item because every one of them
 * is a question about the *set* — "the next item", "the item starting with r" —
 * and only the container can answer it. Items still get their own `Enter` and
 * `Space` from being buttons.
 */
export component MenuBody(
  children: renders* (MenuItem | MenuSeparator | MenuGroup | MenuSub),
  ...rest: { readonly [string]: mixed }
) {
  const menu = useMenu("Menu.Body");
  const bodyRef = useRef<HTMLElement | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const typeahead = useTypeahead();

  // Pulled out because they are stable for the life of the menu, which is what
  // lets the effect below depend on `open` alone. Keyed on the context object
  // it re-ran on every parent render and re-took focus each time, dragging the
  // reader back to the first item while they were arrowing.
  const triggerRef = menu.triggerRef;
  const pendingFocus = menu.pendingFocus;
  const isRoot = menu.parent == null;
  const closeAll = useStableCallback(() => closeTree(menu));
  // Set when the menu was dismissed by a press somewhere else, so the cleanup
  // knows not to drag focus back to the trigger the reader just left.
  const dismissed = useRef(false);

  useEffect(() => {
    const body = bodyRef.current;
    if (!menu.open || body == null) {
      return;
    }
    const document = body.ownerDocument;
    const trigger = triggerRef.current;

    const wanted = pendingFocus.current;
    pendingFocus.current = null;
    const items = itemsOf(body, ITEM_SELECTOR, MENU_SELECTOR);
    const landing = moveTo(items, -1, wanted === "last" ? "last" : "first", false);
    // The menu itself when it holds nothing focusable, so focus is inside it
    // either way and Escape still reaches this component's handler.
    (landing ?? body).focus();
    if (landing != null) {
      setActiveId(landing.id);
    }

    const onOutsidePress = (event: Event) => {
      const target: $FlowFixMe = event.target;
      if (target == null || body.contains(target)) {
        return;
      }
      // The trigger is outside the menu and is not "outside" for this purpose:
      // closing here and letting the trigger's own click reopen made a press on
      // the trigger a no-op that flickered.
      if (trigger != null && trigger.contains(target)) {
        return;
      }
      dismissed.current = true;
      closeAll();
    };
    // Only the outermost menu listens. A submenu closes with the tree, and two
    // listeners would each answer the same press.
    if (isRoot) {
      document.addEventListener("pointerdown", onOutsidePress, true);
    }

    return () => {
      if (isRoot) {
        document.removeEventListener("pointerdown", onOutsidePress, true);
      }
      if (dismissed.current) {
        dismissed.current = false;
        return;
      }
      // Only when focus would otherwise be lost. Choosing an item in a submenu
      // closes three menus at once, and each one restoring focus to its own
      // trigger would leave it on a button that is itself being removed.
      const active = document.activeElement;
      if (active == null || active === document.body || body.contains(active)) {
        trigger?.focus?.();
      }
    };
  }, [menu.open, isRoot, triggerRef, pendingFocus, closeAll]);

  const list = useMemo(() => ({ activeId, setActiveId }), [activeId]);

  if (!menu.open) {
    return null;
  }

  const passed = withoutComposed(rest, ["onKeyDown", "ref"]);

  return (
    <MenuListContext.Provider value={list}>
      <div
        {...passed}
        aria-labelledby={menu.triggered ? `${menu.base}-trigger` : undefined}
        aria-orientation="vertical"
        id={`${menu.base}-body`}
        onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
          const body: $FlowFixMe = event.currentTarget;
          const items = itemsOf(body, ITEM_SELECTOR, MENU_SELECTOR);
          const at = indexOfActive(items, body.ownerDocument?.activeElement);

          if (event.key === "Escape") {
            event.preventDefault();
            // This menu, not the one behind it and not the dialog around it.
            // A submenu is a DOM descendant of its parent menu, so without this
            // one Escape closed the whole tree at once.
            event.stopPropagation();
            menu.setOpen(false);
            return;
          }

          if (event.key === "Tab") {
            // Not prevented: the browser should carry on to the next control,
            // which is what makes Tab a way *past* a menu rather than a way
            // through its thirty items.
            event.stopPropagation();
            closeAll();
            return;
          }

          if (!isRoot && event.key === "ArrowLeft") {
            event.preventDefault();
            event.stopPropagation();
            menu.setOpen(false);
            return;
          }

          const movement = movementFor(event.key, "vertical");
          if (movement != null) {
            // Before moving, or the arrow also scrolls the page under the item
            // that just took focus.
            event.preventDefault();
            event.stopPropagation();
            const next = moveTo(items, at, movement, true);
            if (next != null) {
              next.focus();
              setActiveId(next.id);
            }
            return;
          }

          if (isTypeaheadKey(event)) {
            const next = typeahead(items, at, event.key);
            if (next != null) {
              event.preventDefault();
              event.stopPropagation();
              next.focus();
              setActiveId(next.id);
            }
          }
        })}
        ref={composeRefs(rest.ref, (element) => {
          bodyRef.current = element;
        })}
        role="menu"
        // So the menu can hold focus itself when it is empty, and so a press on
        // its padding does not send focus to `<body>`.
        tabIndex={-1}
      >
        {children}
      </div>
    </MenuListContext.Provider>
  );
}

/**
 * One command in the menu.
 *
 * A disabled item is `aria-disabled` rather than `disabled`, so it stays in the
 * accessibility tree: a reader is told "Delete, menu item, dimmed" and learns
 * that the command exists and is unavailable, where a native `disabled` leaves
 * a silent gap they cannot ask about. The arrow keys and typeahead step over it
 * either way.
 */
export component MenuItem(
  children: React.Node,
  disabled?: boolean = false,
  onSelect?: () => mixed,
  ...rest: { readonly [string]: mixed }
) {
  const menu = useMenu("Menu.Item");
  const list = useContext(MenuListContext);
  const id = useId();
  const passed = withoutComposed(rest, ["onClick", "onFocus"]);
  const setActiveId = list?.setActiveId;

  return (
    <button
      {...passed}
      aria-disabled={disabled ? "true" : undefined}
      id={id}
      onClick={composeHandlers(rest.onClick, () => {
        if (disabled) {
          return;
        }
        onSelect?.();
        closeTree(menu);
      })}
      // The roving tab stop follows real focus rather than leading it, so a
      // pointer that moves focus and a key that moves focus agree without the
      // two of them having to be kept in step by hand.
      onFocus={composeHandlers(rest.onFocus, () => setActiveId?.(id))}
      role="menuitem"
      tabIndex={list?.activeId === id ? 0 : -1}
      type="button"
    >
      {children}
    </button>
  );
}

/**
 * The item that opens a submenu.
 *
 * It is a menu item of the *outer* menu and the trigger of the inner one, which
 * is why it reads the list context of the menu around it and the menu context
 * of the one below it.
 */
export component MenuSubTrigger(children: React.Node, ...rest: { readonly [string]: mixed }) {
  const menu = useMenu("Menu.SubTrigger");
  const list = useContext(MenuListContext);
  const passed = withoutComposed(rest, ["onClick", "onFocus", "onKeyDown", "ref"]);
  const setActiveId = list?.setActiveId;
  useTriggerRegistration(menu);
  // The submenu's own trigger id, not a fresh one: the submenu names itself
  // after it, and two ids for one element is how that link went stale.
  const id = `${menu.base}-trigger`;

  const open = () => {
    menu.pendingFocus.current = "first";
    menu.setOpen(true);
  };

  return (
    <button
      {...passed}
      aria-controls={menu.open ? `${menu.base}-body` : undefined}
      aria-expanded={menu.open ? "true" : "false"}
      aria-haspopup="menu"
      id={id}
      onClick={composeHandlers(rest.onClick, open)}
      onFocus={composeHandlers(rest.onFocus, () => setActiveId?.(id))}
      onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
        if (event.key !== "ArrowRight") {
          return;
        }
        event.preventDefault();
        // The parent menu's own `ArrowRight` does nothing, but a menu three
        // levels deep would otherwise see this key at every level.
        event.stopPropagation();
        open();
      })}
      ref={composeRefs(rest.ref, (element) => {
        menu.triggerRef.current = element;
      })}
      role="menuitem"
      tabIndex={list?.activeId === id ? 0 : -1}
      type="button"
    >
      {children}
    </button>
  );
}

/**
 * A rule between groups of items.
 *
 * `role="separator"` rather than an `<hr>` with a border, because a reader
 * moving through the menu is told the group changed. It is not focusable and
 * the arrow keys pass straight over it.
 */
export component MenuSeparator(...rest: { readonly [string]: mixed }) {
  return <div {...rest} aria-orientation="horizontal" role="separator" />;
}

/**
 * A named group of items.
 *
 * The name has to reach the group through `aria-labelledby`, and only when a
 * `Menu.Label` is actually rendered — an `aria-labelledby` pointing at an id
 * that is not in the document makes a screen reader announce *nothing*, which
 * is worse than an unnamed group.
 */
export component MenuGroup(children: React.Node, ...rest: { readonly [string]: mixed }) {
  const base = useId();
  const [labelled, setLabelled] = useState(false);

  const group = useMemo(() => ({ labelId: `${base}-label`, registerLabel: setLabelled }), [base]);

  return (
    <MenuGroupContext.Provider value={group}>
      <div {...rest} aria-labelledby={labelled ? group.labelId : undefined} role="group">
        {children}
      </div>
    </MenuGroupContext.Provider>
  );
}

/**
 * The heading of a `Menu.Group`.
 *
 * `role="presentation"` because the group already carries the name: leaving it
 * as ordinary content would have a reader hear the heading once as the group's
 * name and again as a stray line of text between the items.
 */
export component MenuLabel(children: React.Node, ...rest: { readonly [string]: mixed }) {
  const group = useContext(MenuGroupContext);
  const register = group?.registerLabel;

  useEffect(() => {
    if (register == null) {
      return;
    }
    register(true);
    return () => register(false);
  }, [register]);

  return (
    <div {...rest} id={group?.labelId} role="presentation">
      {children}
    </div>
  );
}
