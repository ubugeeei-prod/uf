"use client";
// @flow
//
// Navigation menu: a site's top-level links, some of which open a panel of
// further links, as along the top of a documentation site.
//
// `uf ui add navigation-menu` wrote this file into the project, and it is the
// project's from then on. `uf ui diff navigation-menu` shows how it has moved
// away from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The bar, its triggers and plain links, the arrow beside a trigger, and the
// panel of links a trigger opens. `@uniflowed/ui`'s `NavigationMenu` owns the
// pattern: a `<nav>` of list items, a disclosure button for each panel with
// `aria-expanded` and `aria-controls`, a panel named by its trigger that is not
// in the page while it is closed, one panel open at a time, and a link that
// closes its panel when it is followed. A trigger's arrow turns from
// `aria-expanded`.
//
// # What to keep true when you change it
//
// * **Name the navigation.** An `aria-label` on `NavigationMenu` tells a reader
//   which navigation this is when a page has more than one.
// * **Links stay links.** A panel holds `<a>` elements with real `href`s, so a
//   reader can open one in a new tab and a crawler can follow it.
// * **Keep the ring.** Triggers and links take focus, and their outline is how
//   a sighted keyboard user follows it.
// * **Text stays on measured pairs.** `ink` on `surface`, and `accent` on
//   `accentSoft` while highlighted, which `crates/uf_stylex/src/tests/preset.rs`
//   holds to 4.5:1 in both themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    position: "relative",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
  },
  list: {
    display: "flex",
    alignItems: "center",
    gap: ufTokens.space1,
    margin: 0,
    padding: 0,
    listStyle: "none",
  },
  item: {
    position: "relative",
  },
  trigger: {
    // Read by the arrow inside, which cannot see this button's state.
    "--uf-navigation-arrow-turn": { default: "0deg", ":is([aria-expanded=true])": "180deg" },
    display: "inline-flex",
    alignItems: "center",
    gap: ufTokens.space1,
    boxSizing: "border-box",
    minHeight: "36px",
    margin: 0,
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    borderWidth: 0,
    borderRadius: ufTokens.radiusSm,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightMedium,
    lineHeight: ufTokens.leadingTight,
    textDecoration: "none",
    cursor: "pointer",
    color: {
      default: ufTokens.ink,
      ":hover": ufTokens.accent,
      ":focus-visible": ufTokens.accent,
      ":is([aria-expanded=true])": ufTokens.accent,
    },
    backgroundColor: {
      default: "transparent",
      ":hover": ufTokens.accentSoft,
      ":focus-visible": ufTokens.accentSoft,
      ":is([aria-expanded=true])": ufTokens.accentSoft,
    },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  arrow: {
    flexShrink: 0,
    transform: "rotate(var(--uf-navigation-arrow-turn, 0deg))",
  },
  content: {
    position: "absolute",
    zIndex: 50,
    insetBlockStart: "calc(100% + 4px)",
    insetInlineStart: 0,
    display: "grid",
    gap: "2px",
    boxSizing: "border-box",
    minWidth: "14rem",
    margin: 0,
    padding: ufTokens.space2,
    listStyle: "none",
    backgroundColor: ufTokens.surface,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
  },
  link: {
    display: "block",
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    borderRadius: ufTokens.radiusSm,
    textDecoration: "none",
    color: { default: ufTokens.ink, ":hover": ufTokens.accent, ":focus-visible": ufTokens.accent },
    backgroundColor: {
      default: "transparent",
      ":hover": ufTokens.accentSoft,
      ":focus-visible": ufTokens.accentSoft,
    },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
});

/** The navigation. Uncontrolled unless `value`, the open item's, is given. */
export component NavigationMenu(
  children: renders* NavigationMenuList,
  defaultValue?: string | null = null,
  value?: string | null,
  onValueChange?: (value: string | null) => void,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.NavigationMenuRoot
      {...forwarded(rest)}
      className={classNames(props(styles.root, xstyle).className, className)}
      defaultValue={defaultValue}
      onValueChange={onValueChange}
      value={value}
    >
      {children}
    </Primitive.NavigationMenuRoot>
  );
}

/** The row of items. */
export component NavigationMenuList(
  children: renders* NavigationMenuItem,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.NavigationMenuList {
  return (
    <Primitive.NavigationMenuList
      {...forwarded(rest)}
      className={classNames(props(styles.list, xstyle).className, className)}
    >
      {children}
    </Primitive.NavigationMenuList>
  );
}

/**
 * One item: a `NavigationMenuTrigger` and the `NavigationMenuContent` it opens,
 * or a `NavigationMenuTopLink`.
 */
export component NavigationMenuItem(
  value: string,
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.NavigationMenuItem {
  return (
    <Primitive.NavigationMenuItem
      {...forwarded(rest)}
      className={classNames(props(styles.item, xstyle).className, className)}
      value={value}
    >
      {children}
    </Primitive.NavigationMenuItem>
  );
}

/** The button that opens an item's panel, with an arrow that turns while it is open. */
export component NavigationMenuTrigger(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.NavigationMenuTrigger
      {...forwarded(rest)}
      className={classNames(props(styles.trigger, xstyle).className, className)}
    >
      {children}
      <svg
        {...props(styles.arrow)}
        aria-hidden="true"
        fill="none"
        focusable="false"
        height="14"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2"
        viewBox="0 0 24 24"
        width="14"
      >
        <path d="m6 9 6 6 6-6" />
      </svg>
    </Primitive.NavigationMenuTrigger>
  );
}

/** The panel of links under an item, named by its trigger. */
export component NavigationMenuContent(
  children: renders* NavigationMenuLink,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.NavigationMenuBody
      {...forwarded(rest)}
      className={classNames(props(styles.content, xstyle).className, className)}
    >
      {children}
    </Primitive.NavigationMenuBody>
  );
}

/** A link in a panel, which closes the panel when it is followed. */
export component NavigationMenuLink(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.NavigationMenuLink {
  return (
    <Primitive.NavigationMenuLink
      {...forwarded(rest)}
      className={classNames(props(styles.link, xstyle).className, className)}
    >
      {children}
    </Primitive.NavigationMenuLink>
  );
}

/** A link on the bar itself, drawn like a trigger, for an item with no panel. */
export component NavigationMenuTopLink(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <a {...rest} className={classNames(props(styles.trigger, xstyle).className, className)}>
      {children}
    </a>
  );
}

/**
 * A caller's props on their way into a part rather than onto an element. Flow
 * checks that spread against the part's own `...rest`, whose `key` is `empty`
 * where this file's indexer says `mixed`; `@uniflowed/ui` papers over the same
 * hole the same way, and nothing checked is lost, because the elements its
 * parts render have `any`-typed props in uf's library today.
 */
function forwarded(rest: Rest): $FlowFixMe {
  return rest;
}

/** The classes this file chose, then the caller's. */
function classNames(...names: $ReadOnlyArray<?string>): string | void {
  const present = names.filter((name) => name != null && name !== "");
  return present.length === 0 ? undefined : present.join(" ");
}
