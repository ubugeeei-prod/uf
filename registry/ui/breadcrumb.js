// @flow
//
// Breadcrumb: the trail from the top of a site to the page a reader is on, as
// a row of links.
//
// `uf ui add breadcrumb` wrote this file into the project, and it is the
// project's from then on. `uf ui diff breadcrumb` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The row, the links' look, the chevrons between them and the current page.
// `@uniflowed/ui`'s `Breadcrumb` owns the markup a reader navigates: a named
// `<nav>` around an ordered list, `aria-current="page"` on the last crumb,
// which is not a link, and separators that are `aria-hidden` so the trail is
// not read as "Home slash Settings slash Billing".
//
// # What to keep true when you change it
//
// * **The last crumb is the page, not a link to it.** Use `BreadcrumbPage`.
// * **A list holds crumbs and separators.** `BreadcrumbList` takes
//   `renders* (BreadcrumbItem | BreadcrumbSeparator)`, so anything else in the
//   `<ol>` is a Flow error.
// * **A link looks like a link on hover and on focus,** and the page it leads
//   to is named by its words.
// * **Colour comes from tokens, in measured pairs:** `muted` and `ink` on
//   `canvas` and `surface`, which `crates/uf_stylex/src/tests/preset.rs` holds
//   to 4.5:1 in both themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

/** A caller's own element in place of the one a part renders. */
type RenderProp = (props: Rest) => React.Node;

const styles = stylex.create({
  list: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: ufTokens.space1,
    margin: 0,
    padding: 0,
    listStyle: "none",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingTight,
    color: ufTokens.muted,
  },
  item: {
    display: "inline-flex",
    alignItems: "center",
  },
  link: {
    color: { default: ufTokens.muted, ":hover": ufTokens.ink },
    textDecorationLine: { default: "none", ":hover": "underline" },
    textUnderlineOffset: "3px",
    borderRadius: ufTokens.radiusSm,
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  page: {
    color: ufTokens.ink,
    fontWeight: ufTokens.weightMedium,
  },
  separator: {
    display: "inline-flex",
    alignItems: "center",
    color: ufTokens.muted,
  },
});

/** The trail. `label` names the `<nav>`, "Breadcrumb" unless it is given. */
export component Breadcrumb(
  children: React.Node,
  label?: string = "Breadcrumb",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.BreadcrumbRoot
      {...forwarded(rest)}
      className={classNames(props(xstyle).className, className)}
      label={label}
    >
      {children}
    </Primitive.BreadcrumbRoot>
  );
}

/** The ordered list of crumbs and the separators between them. */
export component BreadcrumbList(
  children: renders* (BreadcrumbItem | BreadcrumbSeparator),
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.BreadcrumbList
      {...forwarded(rest)}
      className={classNames(props(styles.list, xstyle).className, className)}
    >
      {children}
    </Primitive.BreadcrumbList>
  );
}

/** One crumb. */
export component BreadcrumbItem(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.BreadcrumbItem {
  return (
    <Primitive.BreadcrumbItem
      {...forwarded(rest)}
      className={classNames(props(styles.item, xstyle).className, className)}
    >
      {children}
    </Primitive.BreadcrumbItem>
  );
}

/** A crumb that leads somewhere. `render` for the router's own link. */
export component BreadcrumbLink(
  children: React.Node,
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.BreadcrumbLink
      {...forwarded(rest)}
      className={classNames(props(styles.link, xstyle).className, className)}
      render={render}
    >
      {children}
    </Primitive.BreadcrumbLink>
  );
}

/** The page the reader is on, which is not a link. */
export component BreadcrumbPage(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.BreadcrumbPage
      {...forwarded(rest)}
      className={classNames(props(styles.page, xstyle).className, className)}
    >
      {children}
    </Primitive.BreadcrumbPage>
  );
}

/** The mark between two crumbs, a chevron unless it is given; decoration. */
export component BreadcrumbSeparator(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.BreadcrumbSeparator {
  return (
    <Primitive.BreadcrumbSeparator
      {...forwarded(rest)}
      className={classNames(props(styles.separator, xstyle).className, className)}
    >
      {children ?? (
        <svg
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
          <path d="m9 18 6-6-6-6" />
        </svg>
      )}
    </Primitive.BreadcrumbSeparator>
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
