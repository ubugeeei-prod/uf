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
//     closes it and comes back to the item that opened it — and the two swap in
//     a right-to-left page, because a submenu opens onto the *inline end*.
//   * `Tab` closes the menu and carries on through the page, rather than
//     walking the reader through thirty items they have already dismissed.
//
// # This is shadcn's Dropdown Menu
//
// Under that name it is a fourth component; here it is this one. A dropdown
// menu is a menu whose trigger is a button, which is what `Menu.Trigger` is, so
// there is no second module and no alias export — a second spelling of a
// component is a second surface to keep in step, and `index.js` argues against
// one at greater length. `context-menu.js` and `menubar.js` are the two that
// genuinely differ, and each of their headers says in what.
//
// # Choosing an item, and the item that should not close the menu
//
// `onSelect` receives the click and may answer it. `preventDefault()` means "I
// handled this and the menu stays open", which is the contract
// `internal/merge-props.js` already uses between a caller's handler and a
// component's, and the one Radix settled on for this exact question. A
// `closeOnSelect` prop is the same answer given once for a part rather than per
// press.
//
// The defaults differ between the item kinds because the platform's do. A
// command closes the menu — running it and leaving the menu open is a state no
// native menu has been in. A *checkable* item does not: "show hidden files"
// toggled three times is one visit to the menu everywhere except in a component
// library, and a checkbox that closed the menu would make checking three boxes
// mean opening the menu three times.
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
// # Where the menu goes
//
// `internal/anchor.js`, the same module `Popover.Body` uses, and adopting it
// here rather than writing a second one is most of ubugeeei-prod/uf#256. Two
// things about a menu were wrong before it and are worth naming, because
// neither looks like a positioning bug:
//
//   * A menu in a table row, a card, or anything else with `overflow: hidden`
//     was cut off at that box's edge. It is `position: fixed` now, so the
//     clipping ancestor is not its business.
//   * A menu whose trigger sat near the bottom of the page opened downwards,
//     off the screen, and the reader saw nothing at all.
//
// A submenu asks for `side="inline-end"` rather than `right`, which is the same
// answer `submenuKeys` gives about the *keys*: the submenu opens the way the
// page reads, and the arrow that opens it points at where it went.
//
// A context menu opens at a point instead, which is the one thing the
// positioner did not do; `internal/menu-tree.js` carries that rectangle and
// `internal/anchor.js` says what it replaces.
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
  useCallback,
  useContext,
  useEffect,
  useId,
  useMemo,
  useRef,
  useState,
} from "@uniflowed/react";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";

import type { Align, LogicalSide } from "./internal/anchor.js";
import { useAnchor } from "./internal/anchor.js";
import type { Rest } from "./internal/merge-props.js";
import { composeHandlers, composeRefs, withoutComposed } from "./internal/merge-props.js";
import {
  directionOf,
  indexOfActive,
  isTypeaheadKey,
  itemsOf,
  movementFor,
  moveTo,
  useTypeahead,
} from "./internal/roving-focus.js";
import { useControlled } from "./internal/controlled-state.js";
import {
  ITEM_SELECTOR,
  MENU_SELECTOR,
  MenuAnchorContext,
  MenuContext,
  MenuLevel,
  MenuListContext,
  closeTree,
  submenuKeys,
  useMenu,
  useTriggerRegistration,
} from "./internal/menu-tree.js";

export type { Align, LogicalSide, Side } from "./internal/anchor.js";

/**
 * The part of a click a menu item's `onSelect` may read and answer.
 *
 * Inexact, because what arrives is React's synthetic event and this names only
 * the two members the contract is about: calling `preventDefault()` keeps the
 * menu open, and the component reads `defaultPrevented` afterwards to find out.
 * A caller who wants the rest of the event has it — this is the promise, not
 * the object.
 */
export type MenuSelect = {
  readonly defaultPrevented: boolean,
  readonly preventDefault: () => mixed,
  ...
};

/** The id of a group's label, so `Menu.Group` only claims one that exists. */
type MenuGroupState = {|
  readonly labelId: string,
  readonly registerLabel: (present: boolean) => void,
|};

const MenuGroupContext: React.Context<MenuGroupState | null> = createContext(null);

/** What a `Menu.RadioGroup` tells the items inside it. */
type MenuRadioState = {|
  readonly value: string | null,
  readonly choose: (value: string) => void,
|};

const MenuRadioContext: React.Context<MenuRadioState | null> = createContext(null);

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

/** The button that opens the menu. */
export component MenuTrigger(children: React.Node, ...rest: Rest) {
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
  children: renders* (
    | MenuItem
    | MenuCheckboxItem
    | MenuRadioGroup
    | MenuSeparator
    | MenuGroup
    | MenuSub
  ),
  align?: Align = "start",
  alignOffset?: number = 0,
  avoidCollisions?: boolean = true,
  collisionPadding?: number = 0,
  side?: LogicalSide,
  sideOffset?: number = 0,
  ...rest: Rest
) {
  const menu = useMenu("Menu.Body");
  const bodyRef = useRef<HTMLElement | null>(null);
  const [activeId, setActiveId] = useState<string | null>(null);
  const typeahead = useTypeahead();
  // A point to open at, when whatever opened this menu was a pointer rather
  // than a button. Null for every menu that hangs off a trigger.
  const point = useContext(MenuAnchorContext);

  // Pulled out because they are stable for the life of the menu, which is what
  // lets the effect below depend on `open` alone. Keyed on the context object
  // it re-ran on every parent render and re-took focus each time, dragging the
  // reader back to the first item while they were arrowing.
  const triggerRef = menu.triggerRef;
  const pendingFocus = menu.pendingFocus;
  const isRoot = menu.parent == null;
  const closeAll = useStableCallback(() => closeTree(menu));
  // A root menu drops from its button; a submenu comes out of the side of the
  // item that opened it, on the side the page reads towards. The default cannot
  // be a parameter default because it is not a constant: it is the answer to
  // "is this the outermost menu", which only this component knows.
  const placement = side ?? (isRoot ? "bottom" : "inline-end");
  const anchored = useAnchor({
    align,
    alignOffset,
    anchorRect: point,
    anchorRef: triggerRef,
    avoidCollisions,
    collisionPadding,
    open: menu.open,
    overlayRef: bodyRef,
    side: placement,
    sideOffset,
  });
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
        data-align={anchored.align}
        data-side={anchored.side}
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

          // Asked once, here, and used for both questions below: which key
          // closes this submenu, and — for a menu a caller has laid out
          // horizontally one day — which way the arrows run.
          const direction = directionOf(body);

          if (!isRoot && event.key === submenuKeys(direction).close) {
            event.preventDefault();
            event.stopPropagation();
            menu.setOpen(false);
            return;
          }

          const movement = movementFor(event.key, "vertical", direction);
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
 * Everything an item of any of the three kinds needs from the menu around it.
 *
 * One hook rather than three copies, because the three differ in their role and
 * their state and in nothing else: the same id, the same roving tab stop, the
 * same "a disabled item is announced and stepped over", and the same rule about
 * when a press closes the tree.
 */
hook useMenuItem(
  part: string,
  disabled: boolean,
  closeOnSelect: boolean,
  onSelect: ((event: MenuSelect) => mixed) | void,
  act: (() => void) | void,
): {|
  readonly id: string,
  readonly onClick: (event: MenuSelect) => void,
  readonly onFocus: () => void,
  readonly tabIndex: number,
|} {
  const menu = useMenu(part);
  const list = useContext(MenuListContext);
  const id = useId();
  const setActiveId = list?.setActiveId;

  return {
    id,
    onClick: (event: MenuSelect) => {
      if (disabled) {
        return;
      }
      act?.();
      onSelect?.(event);
      // The caller's answer, read after they have had the event: a
      // `preventDefault()` in `onSelect` is "I handled this, leave the menu
      // open", which is the same sentence `composeHandlers` reads between a
      // caller's handler and this package's.
      if (closeOnSelect && !event.defaultPrevented) {
        closeTree(menu);
      }
    },
    onFocus: () => setActiveId?.(id),
    tabIndex: list?.activeId === id ? 0 : -1,
  };
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
  closeOnSelect?: boolean = true,
  onSelect?: (event: MenuSelect) => mixed,
  ...rest: Rest
) {
  const item = useMenuItem("Menu.Item", disabled, closeOnSelect, onSelect, undefined);
  const passed = withoutComposed(rest, ["onClick", "onFocus"]);

  return (
    <button
      {...passed}
      aria-disabled={disabled ? "true" : undefined}
      id={item.id}
      onClick={composeHandlers(rest.onClick, item.onClick)}
      // The roving tab stop follows real focus rather than leading it, so a
      // pointer that moves focus and a key that moves focus agree without the
      // two of them having to be kept in step by hand.
      onFocus={composeHandlers(rest.onFocus, item.onFocus)}
      role="menuitem"
      tabIndex={item.tabIndex}
      type="button"
    >
      {children}
    </button>
  );
}

/**
 * An item that carries a state of its own: "show hidden files".
 *
 * `role="menuitemcheckbox"` with `aria-checked`, which is the role the arrow
 * keys and the typeahead have always stepped across — `ITEM_SELECTOR` named it
 * before there was a component that rendered it. What a caller could not
 * hand-roll on `Menu.Item` is the rest: the controlled-and-uncontrolled
 * contract `internal/controlled-state.js` states for everything here, and a
 * press that does *not* dismiss the menu.
 *
 * There is no third state. `aria-checked="mixed"` belongs to a checkbox that
 * summarises other checkboxes — `checkbox.js` has it, and a menu item is a
 * command rather than a summary of a table's rows.
 */
export component MenuCheckboxItem(
  children: React.Node,
  checked?: boolean,
  defaultChecked?: boolean = false,
  onCheckedChange?: (checked: boolean) => void,
  disabled?: boolean = false,
  // A menu the reader is still ticking boxes in stays open; see the module
  // header for why this default is the opposite of `Menu.Item`'s.
  closeOnSelect?: boolean = false,
  onSelect?: (event: MenuSelect) => mixed,
  ...rest: Rest
) {
  const [on, setOn] = useControlled(checked, defaultChecked, onCheckedChange);
  const toggle = useCallback(() => setOn(!on), [on, setOn]);
  const item = useMenuItem("Menu.CheckboxItem", disabled, closeOnSelect, onSelect, toggle);
  const passed = withoutComposed(rest, ["onClick", "onFocus"]);

  return (
    <button
      {...passed}
      aria-checked={on ? "true" : "false"}
      aria-disabled={disabled ? "true" : undefined}
      id={item.id}
      onClick={composeHandlers(rest.onClick, item.onClick)}
      onFocus={composeHandlers(rest.onFocus, item.onFocus)}
      role="menuitemcheckbox"
      tabIndex={item.tabIndex}
      type="button"
    >
      {children}
    </button>
  );
}

/**
 * A set of items of which exactly one is chosen.
 *
 * The *group* owns the value, which is what makes this a component rather than
 * a convention: `aria-checked="true"` has to be on one item and `"false"` on
 * the others, and a caller holding a value per item gets two checked ones the
 * first time a render is skipped. `role="group"` is what ties them together for
 * a reader — the items are `menuitemradio`, and a reader is told "2 of 3".
 *
 * `onValueChange` promises a `string` while the state is `string | null`, for
 * the reason `radio-group.js` gives at greater length: "nothing chosen yet" is
 * a state the group starts in and never an event it reports, because no gesture
 * inside it unchooses an answer.
 */
export component MenuRadioGroup(
  children: renders* (MenuRadioItem | MenuLabel | MenuSeparator),
  defaultValue?: string | null = null,
  value?: string | null,
  onValueChange?: (value: string) => void,
  ...rest: Rest
) {
  const base = useId();
  const [labelled, setLabelled] = useState(false);
  const report = useCallback(
    (next: string | null) => {
      if (next != null) {
        onValueChange?.(next);
      }
    },
    [onValueChange],
  );
  const [selected, select] = useControlled<string | null>(value, defaultValue, report);

  const group = useMemo(() => ({ labelId: `${base}-label`, registerLabel: setLabelled }), [base]);
  const radio = useMemo(
    () => ({ value: selected, choose: (next: string) => select(next) }),
    [selected, select],
  );

  return (
    <MenuGroupContext.Provider value={group}>
      <MenuRadioContext.Provider value={radio}>
        <div {...rest} aria-labelledby={labelled ? group.labelId : undefined} role="group">
          {children}
        </div>
      </MenuRadioContext.Provider>
    </MenuGroupContext.Provider>
  );
}

/**
 * One answer in a `Menu.RadioGroup`.
 *
 * Choosing it reports the group's new value and leaves the menu open, which is
 * what a sort order or a zoom level in a native menu does; `closeOnSelect` is
 * the way to say otherwise for a choice that ends the visit.
 */
export component MenuRadioItem(
  children: React.Node,
  value: string,
  disabled?: boolean = false,
  closeOnSelect?: boolean = false,
  onSelect?: (event: MenuSelect) => mixed,
  ...rest: Rest
) {
  const group = useContext(MenuRadioContext);
  if (group == null) {
    throw new Error("Menu.RadioItem must be rendered inside a Menu.RadioGroup");
  }
  const choose = group.choose;
  const pick = useCallback(() => choose(value), [choose, value]);
  const item = useMenuItem("Menu.RadioItem", disabled, closeOnSelect, onSelect, pick);
  const passed = withoutComposed(rest, ["onClick", "onFocus"]);

  return (
    <button
      {...passed}
      aria-checked={group.value === value ? "true" : "false"}
      aria-disabled={disabled ? "true" : undefined}
      id={item.id}
      onClick={composeHandlers(rest.onClick, item.onClick)}
      onFocus={composeHandlers(rest.onFocus, item.onFocus)}
      role="menuitemradio"
      tabIndex={item.tabIndex}
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
export component MenuSubTrigger(children: React.Node, ...rest: Rest) {
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
        const trigger: $FlowFixMe = event.currentTarget;
        if (event.key !== submenuKeys(directionOf(trigger)).open) {
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
export component MenuSeparator(...rest: Rest) {
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
export component MenuGroup(children: React.Node, ...rest: Rest) {
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
 * The heading of a `Menu.Group` or a `Menu.RadioGroup`.
 *
 * `role="presentation"` because the group already carries the name: leaving it
 * as ordinary content would have a reader hear the heading once as the group's
 * name and again as a stray line of text between the items.
 */
export component MenuLabel(children: React.Node, ...rest: Rest) {
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
