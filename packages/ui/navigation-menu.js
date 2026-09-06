// @flow
//
// Site navigation, which is not a menu.
//
// This component exists mostly to prevent one mistake, and the mistake is
// reaching for `role="menu"` because the thing is called a navigation menu.
// `menu`, `menubar` and `menuitem` are for *application commands* — Cut, Paste,
// Export as PNG — and using them for a site's navigation costs a reader three
// things at once:
//
//   * A screen reader announces "menu, five items" where the reader expected a
//     list of links, and a list of links is a thing they know how to read.
//   * The whole menu keyboard map comes with the role, and a reader who knows
//     it will use it: `Tab` should leave the set in one press, arrow keys
//     should move between items, typing a letter should jump. Claiming the role
//     and not implementing the map is worse than not claiming it.
//   * `menuitem` is not a link. It is not announced as one, it is not in the
//     list of links a reader can pull up, and "open in a new tab" is not
//     obviously available on it.
//
// The WAI-ARIA practices have a pattern for exactly this and it is the
// *Disclosure Navigation Menu*: a `<nav>` containing a list, where an
// expandable entry is a `button` with `aria-expanded` controlling a list of
// ordinary links. Radix's NavigationMenu makes the same choice. So: `Tab` walks
// the links, because they are links; `Escape` closes the open group and gives
// focus back to the button that opened it; choosing a link closes the group,
// because the reader is leaving.
//
// # One group open at a time
//
// Opening one closes the others, which is what the practices' example does and
// what a site's navigation looks like everywhere. It is also why the root holds
// which entry is open rather than each entry holding its own state: "the open
// one" is a fact about the set.
//
// # The closed group is not rendered
//
// `accordion.js` keeps its closed panels in the document with
// `hidden="until-found"` so that find-in-page can reach the text in them. This
// deliberately does not, and the difference is what the content is *for*. An
// accordion panel is prose a reader might be searching. A closed navigation
// group is a list of destinations, and having Ctrl+F unfold the site's
// navigation on the way to a word in the article would be a surprise with
// nothing to show for it.
//
// # Name the landmark
//
// `<nav>` is a landmark, and a page with two unnamed ones gives a reader a
// choice between "navigation" and "navigation". Pass `aria-label`.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useId, useMemo, useState } from "@uniflowed/react";

import type { Rest } from "./internal/merge-props.js";
import { composeHandlers, withoutComposed } from "./internal/merge-props.js";
import { usePresence } from "./internal/disclosure.js";
import { useControlled } from "./internal/controlled-state.js";

type NavigationMenuState = {|
  /** The entry whose group is open, or null for none. */
  readonly open: string | null,
  readonly setOpen: (value: string | null) => void,
|};

const NavigationMenuContext: React.Context<NavigationMenuState | null> = createContext(null);

type NavigationMenuItemState = {|
  readonly triggerId: string,
  readonly bodyId: string,
  readonly expanded: boolean,
  readonly toggle: () => void,
  readonly close: () => void,
  /** Whether a `NavigationMenu.Body` is rendered, so the trigger names one that exists. */
  readonly present: boolean,
  readonly registerBody: (present: boolean) => void,
|};

const NavigationMenuItemContext: React.Context<NavigationMenuItemState | null> =
  createContext(null);

hook useNavigationMenu(part: string): NavigationMenuState {
  const state = useContext(NavigationMenuContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a NavigationMenu.Root`);
  }
  return state;
}

hook useNavigationMenuItem(part: string): NavigationMenuItemState {
  const state = useContext(NavigationMenuItemContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a NavigationMenu.Item`);
  }
  return state;
}

/**
 * The landmark, and the one place `Escape` is handled.
 *
 * `Escape` is here rather than on each group because the reader may be anywhere
 * inside the open one when they press it, and because the button to give focus
 * back to is found the same way everything else in this package finds things:
 * by asking the document at the moment of the press. The alternative — every
 * trigger writing itself into a ref — is a registry that has to be kept in step
 * with a document that already knows the answer.
 */
export component NavigationMenuRoot(
  children: renders* NavigationMenuList,
  defaultValue?: string | null = null,
  value?: string | null,
  onValueChange?: (value: string | null) => void,
  ...rest: Rest
) {
  const [open, setOpen] = useControlled<string | null>(value, defaultValue, onValueChange);
  const state = useMemo(() => ({ open, setOpen }), [open, setOpen]);
  const passed = withoutComposed(rest, ["onKeyDown"]);

  return (
    <NavigationMenuContext.Provider value={state}>
      <nav
        {...passed}
        onKeyDown={composeHandlers(rest.onKeyDown, (event) => {
          if (event.key !== "Escape" || open == null) {
            return;
          }
          event.preventDefault();
          // This navigation menu, not a dialog around it.
          event.stopPropagation();
          const nav: $FlowFixMe = event.currentTarget;
          const trigger = nav.querySelector('[aria-expanded="true"]');
          setOpen(null);
          // After closing, and synchronously: the trigger is not the element
          // being removed — the group inside it is — so it is still there to
          // take focus, and leaving focus on a `<li>` that no longer contains
          // anything focusable drops the reader at the top of the page.
          trigger?.focus?.();
        })}
      >
        {children}
      </nav>
    </NavigationMenuContext.Provider>
  );
}

/** The list of entries. A `<ul>`, because a reader is told how many there are. */
export component NavigationMenuList(children: renders* NavigationMenuItem, ...rest: Rest) {
  useNavigationMenu("NavigationMenu.List");
  return <ul {...rest}>{children}</ul>;
}

/** One entry: a link on its own, or a button and the group it opens. */
export component NavigationMenuItem(value: string, children: React.Node, ...rest: Rest) {
  const menu = useNavigationMenu("NavigationMenu.Item");
  const base = useId();
  const [present, setPresent] = useState(false);
  const setOpen = menu.setOpen;
  const expanded = menu.open === value;

  const state = useMemo(
    () => ({
      triggerId: `${base}-trigger`,
      bodyId: `${base}-body`,
      expanded,
      toggle: () => setOpen(expanded ? null : value),
      close: () => setOpen(null),
      present,
      registerBody: setPresent,
    }),
    [base, expanded, setOpen, value, present],
  );

  return (
    <NavigationMenuItemContext.Provider value={state}>
      <li {...rest}>{children}</li>
    </NavigationMenuItemContext.Provider>
  );
}

/** The button that opens an entry's group. Not a `menuitem`; see the header. */
export component NavigationMenuTrigger(children: React.Node, ...rest: Rest) {
  const item = useNavigationMenuItem("NavigationMenu.Trigger");
  const passed = withoutComposed(rest, ["onClick"]);

  return (
    <button
      {...passed}
      aria-controls={item.present ? item.bodyId : undefined}
      aria-expanded={item.expanded ? "true" : "false"}
      id={item.triggerId}
      onClick={composeHandlers(rest.onClick, item.toggle)}
      type="button"
    >
      {children}
    </button>
  );
}

/**
 * The group of links an entry opens.
 *
 * Named after its trigger, so a reader who lands in it by `Tab` is told which
 * entry they are inside rather than hearing an unnamed list of four links.
 */
export component NavigationMenuBody(children: renders* NavigationMenuLink, ...rest: Rest) {
  const item = useNavigationMenuItem("NavigationMenu.Body");
  // Registered only while the group is actually in the document, which for this
  // component means only while it is open — see the module header for why a
  // closed group is removed rather than hidden. The register has to be handed
  // over conditionally rather than the hook called conditionally, because a
  // hook that runs on some renders and not others is a different bug.
  usePresence(item.expanded ? item.registerBody : undefined);

  if (!item.expanded) {
    return null;
  }

  return (
    <ul {...rest} aria-labelledby={item.triggerId} id={item.bodyId}>
      {children}
    </ul>
  );
}

/**
 * A destination.
 *
 * It renders its own `<li>` around the `<a>`, which is worth knowing before it
 * surprises somebody: a `<ul>` may only contain `<li>`, and a `Link` that
 * rendered a bare anchor would put this package's own markup outside what HTML
 * allows in the list it sits in. The caller's props go on the anchor, which is
 * the element they are about.
 *
 * Choosing one closes the group, because the reader is leaving. That is done
 * without preventing anything: the click still navigates, and the group is shut
 * behind them so that coming back — or a router that never unmounted the page —
 * does not leave it hanging open.
 */
export component NavigationMenuLink(children: React.Node, ...rest: Rest) {
  const item = useNavigationMenuItem("NavigationMenu.Link");
  const passed = withoutComposed(rest, ["onClick"]);

  return (
    <li>
      <a {...passed} onClick={composeHandlers(rest.onClick, item.close)}>
        {children}
      </a>
    </li>
  );
}
