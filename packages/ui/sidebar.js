// @flow
//
// A sidebar: the one of the four dialog-shaped components that is usually not a
// dialog at all.
//
// `alert-dialog.js`, `sheet.js` and `drawer.js` are modal, and being modal is
// the point of each. A sidebar is *part of the page*: the reader uses what is
// beside it while it is open, nothing behind it is inert, nothing is
// scroll-locked, and there is no focus trap. It is a `<nav>` landmark and a
// button that says whether it is showing — the disclosure pattern
// `internal/disclosure.js` describes, applied to a region of the layout.
//
// Then the viewport gets narrow, and it *becomes* a dialog. That transition is
// the component, and it is the reason this is not something a caller assembles
// out of `Collapsible` and a media query:
//
//   * **Collapsed is not closed.** On a wide screen the sidebar collapses to
//     its icons and stays in the page. On a narrow one it is a `Sheet`: modal,
//     over the content, gone when it is closed. One `open` describes both, and
//     `aria-expanded` on the trigger is true about both.
//   * **Focus has to move in both directions.** Opening the narrow sidebar
//     moves focus into it, because it is a modal dialog and that is what
//     `dialog.js` promises; closing it gives focus back to the trigger. Opening
//     the wide one moves focus nowhere at all, because nothing was taken away.
//     A component that got this backwards would either strand a reader in a
//     dialog they cannot leave or move their focus for no reason.
//   * **A collapsed button still has a name.** This is where the pattern goes
//     wrong in practice. Collapsed to icons, a button whose accessible name
//     came from its text is announced as "button" — so `Sidebar.Item` takes a
//     `label`, puts it in `aria-label` the moment the sidebar collapses, and
//     shows it in a `Tooltip` for readers who can see the icon and do not know
//     what it means. The name survives whatever the stylesheet does to the
//     text, which is the only way to promise it survives.
//
// # The state is the caller's, and that is a server-rendering decision
//
// `open` is uncontrolled by default like everything else here, and a real
// application will control it: shadcn persists the answer in a cookie so the
// server renders the sidebar in the state the reader left it. That matters more
// under RSC than it looks — an uncontrolled sidebar renders expanded on the
// server and collapses on hydration, which is a layout shift on every
// navigation. `open` and `onOpenChange` are how a caller reads the cookie and
// hands the answer in.
//
// # The narrow viewport is a query, not a guess
//
// `useMediaQuery` from `@uniflowed/hooks/browser`, with `false` on the server:
// there is no viewport during a prerender, and guessing "narrow" would send
// every reader markup in which the navigation is a closed dialog. The wide
// layout is the one that is still usable when the guess is wrong.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useId, useMemo } from "@uniflowed/react";
import { useMediaQuery } from "@uniflowed/hooks/browser";

import type { Rest } from "./internal/merge-props.js";
import { composeHandlers, forwarded, withProps, withoutComposed } from "./internal/merge-props.js";
import { SheetBody, SheetOverlay, SheetRoot, SheetTrigger } from "./sheet.js";
import { TooltipBody, TooltipRoot, TooltipTrigger } from "./tooltip.js";
import { useControlled } from "./internal/controlled-state.js";

/**
 * Which side of the layout the sidebar is on.
 *
 * Two members and not `sheet.js`'s four, because a sidebar is never attached to
 * the top or the bottom: a navigation rail across the top of a page is a header,
 * with different semantics and a different component. `<Sidebar.Root side="top">`
 * is a type error, which is the point of naming the union rather than reusing
 * `Edge`.
 */
export type SidebarSide = "left" | "right";

/** The breakpoint below which the sidebar is a modal sheet. */
const NARROW = "(max-width: 48rem)";

type SidebarState = {|
  readonly base: string,
  /** Expanded on a wide screen, and showing on a narrow one. */
  readonly open: boolean,
  readonly setOpen: (open: boolean) => void,
  /** In the page, showing icons only. Never true while it is a sheet. */
  readonly collapsed: boolean,
  /** Whether the viewport has made it a modal sheet. */
  readonly modal: boolean,
  readonly side: SidebarSide,
  /** Whether the navigation is in the document, so nothing names it when it is not. */
  readonly present: boolean,
|};

const SidebarContext: React.Context<SidebarState | null> = createContext(null);

/**
 * The sidebar a part belongs to.
 *
 * Raising rather than returning null, for the reason `useDialog` gives: a
 * `Sidebar.Trigger` outside a root would render a button with an
 * `aria-expanded` that never changes, and it would look correct.
 */
hook useSidebar(part: string): SidebarState {
  const state = useContext(SidebarContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Sidebar.Root`);
  }
  return state;
}

/**
 * The sidebar, expanded or collapsed — and, on a narrow viewport, a sheet.
 *
 * Renders no element of its own when it is part of the page: the trigger and
 * the navigation are siblings in whatever layout the caller wrote. When the
 * viewport makes it modal it renders a `Sheet.Root` around both, so the trigger
 * is the sheet's trigger and focus goes back to it — which is the half of the
 * transition a wrapper around only the navigation could not do.
 */
export component SidebarRoot(
  children: React.Node,
  defaultOpen?: boolean = true,
  narrowQuery?: string = NARROW,
  onOpenChange?: (open: boolean) => void,
  open?: boolean,
  side?: SidebarSide = "left",
) {
  const base = useId();
  const [isOpen, setOpen] = useControlled(open, defaultOpen, onOpenChange);
  // `false` on the server: see the module header. The wide layout is the one
  // that is still usable when there is no viewport to ask.
  const modal = useMediaQuery(narrowQuery, false);

  const state = useMemo(
    () => ({
      base,
      collapsed: !modal && !isOpen,
      modal,
      open: isOpen,
      // A sheet's navigation is in the document only while the sheet is open;
      // the page's is always there, collapsed or not.
      present: modal ? isOpen : true,
      setOpen,
      side,
    }),
    [base, isOpen, modal, setOpen, side],
  );

  return (
    <SidebarContext.Provider value={state}>
      {modal ? (
        <SheetRoot onOpenChange={setOpen} open={isOpen} side={side}>
          {children}
        </SheetRoot>
      ) : (
        children
      )}
    </SidebarContext.Provider>
  );
}

/**
 * The button that expands and collapses it.
 *
 * `aria-expanded` either way, and it is a true sentence about two different
 * things: on a wide screen it says whether the navigation is showing its
 * labels, on a narrow one whether the sheet is open. `aria-controls` names the
 * navigation only while the navigation is in the document — a reference to an
 * id nothing has tells a reader there is somewhere to go and has nowhere to
 * send them.
 */
export component SidebarTrigger(children: React.Node, ...rest: Rest) {
  const sidebar = useSidebar("Sidebar.Trigger");
  const passed = withoutComposed(rest, ["onClick"]);
  const named = sidebar.present ? `${sidebar.base}-nav` : undefined;

  // The sheet's own trigger while it is one: `Dialog.Trigger` is what records
  // where focus came from, and focus going back to this button when the sheet
  // closes is the second half of the transition.
  if (sidebar.modal) {
    // `Dialog.Trigger` names the sheet's own body while it is open, which is a
    // better `aria-controls` than the navigation inside it, so this part adds
    // nothing to it.
    return <SheetTrigger {...forwarded(rest)}>{children}</SheetTrigger>;
  }

  return (
    <button
      {...passed}
      aria-controls={named}
      aria-expanded={sidebar.open ? "true" : "false"}
      onClick={composeHandlers(rest.onClick, () => sidebar.setOpen(!sidebar.open))}
      type="button"
    >
      {children}
    </button>
  );
}

/**
 * The navigation itself: a named `<nav>` landmark, and on a narrow viewport a
 * named `<nav>` landmark inside a modal sheet.
 *
 * A `<div>` here is the mistake the component exists to prevent. A `<nav>` is
 * how a screen reader's landmark list offers "skip to the navigation", and a
 * site's main navigation that is not one is navigation a reader has to find by
 * tabbing through it.
 *
 * `label` is required rather than defaulted, because a landmark with no name is
 * announced as "navigation" — and a page with two of those has told the reader
 * there are two and which is which is a guess.
 */
export component SidebarBody(children: React.Node, label: string, ...rest: Rest) {
  const sidebar = useSidebar("Sidebar.Body");
  const nav = (
    <nav
      {...forwarded(rest)}
      aria-label={label}
      data-collapsed={sidebar.collapsed ? "true" : undefined}
      id={`${sidebar.base}-nav`}
    >
      {children}
    </nav>
  );

  if (!sidebar.modal) {
    return nav;
  }

  // Named rather than titled: a heading nobody asked for would appear in the
  // page's outline, and the sheet's name is the navigation's name.
  return (
    <>
      <SheetOverlay />
      <SheetBody aria-label={label}>{nav}</SheetBody>
    </>
  );
}

/** The top of the sidebar, as a place to put styles. See `Dialog.Header`. */
export component SidebarHeader(children: React.Node, ...rest: Rest) {
  return <div {...rest}>{children}</div>;
}

/** The bottom of the sidebar. See `Sidebar.Header`. */
export component SidebarFooter(children: React.Node, ...rest: Rest) {
  return <div {...rest}>{children}</div>;
}

/**
 * One entry in the navigation, whose name survives the collapse.
 *
 * `label` is what the reader hears. While the sidebar is expanded the entry's
 * own content is its name, so nothing is overridden and a label with an icon,
 * a count and a second line reads as written. The moment it collapses,
 * `aria-label` takes over — because at that point the text is whatever the
 * stylesheet has done to it, and a promise about the accessible name cannot
 * depend on that.
 *
 * `render` for an entry that is a link. Site navigation is links, and a
 * `<button>` that navigates is a button a reader cannot open in a new tab; see
 * `navigation-menu.js`, which is the same argument at the scale of a whole
 * menu.
 */
export component SidebarItem(
  children: React.Node,
  label: string,
  render?: (props: Rest) => React.Node,
  ...rest: Rest
) {
  const sidebar = useSidebar("Sidebar.Item");
  const passed = withoutComposed(rest, ["ref"]);
  const mine: Rest = {
    "aria-label": sidebar.collapsed ? label : undefined,
    "data-collapsed": sidebar.collapsed ? "true" : undefined,
  };

  const entry = (extra: Rest) => {
    const props = withProps(withProps(passed, mine), extra);
    return render == null ? (
      <button {...props} type="button">
        {children}
      </button>
    ) : (
      render(props)
    );
  };

  if (!sidebar.collapsed) {
    return entry({ ref: rest.ref });
  }

  // The icon's name, shown. A reader who can see the rail and not read minds
  // needs the same sentence `aria-label` gives everybody else, and a tooltip is
  // the mechanism that already satisfies WCAG 1.4.13 in this package.
  return (
    <TooltipRoot>
      <TooltipTrigger ref={rest.ref} render={(props: Rest) => entry(props)} />
      <TooltipBody side={sidebar.side === "left" ? "right" : "left"}>{label}</TooltipBody>
    </TooltipRoot>
  );
}
