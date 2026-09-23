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
// * **The last crumb is the page, not a link to it.** Use `Breadcrumb.Page`.
// * **A list holds crumbs and separators.** `Breadcrumb.List` takes
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
import { Breadcrumb } from "@uniflowed/ui";

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
component BreadcrumbRoot(
  children: React.Node,
  label?: string = "Breadcrumb",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Breadcrumb.Root
      {...forwarded(rest)}
      className={classNames(props(xstyle).className, className)}
      label={label}
    >
      {children}
    </Breadcrumb.Root>
  );
}

/** The ordered list of crumbs and the separators between them. */
component BreadcrumbList(
  children: renders* (BreadcrumbItem | BreadcrumbSeparator),
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Breadcrumb.List
      {...forwarded(rest)}
      className={classNames(props(styles.list, xstyle).className, className)}
    >
      {children}
    </Breadcrumb.List>
  );
}

/** One crumb. */
component BreadcrumbItem(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Breadcrumb.Item {
  return (
    <Breadcrumb.Item
      {...forwarded(rest)}
      className={classNames(props(styles.item, xstyle).className, className)}
    >
      {children}
    </Breadcrumb.Item>
  );
}

/** A crumb that leads somewhere. `render` for the router's own link. */
component BreadcrumbLink(
  children: React.Node,
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Breadcrumb.Link
      {...forwarded(rest)}
      className={classNames(props(styles.link, xstyle).className, className)}
      render={render}
    >
      {children}
    </Breadcrumb.Link>
  );
}

/** The page the reader is on, which is not a link. */
component BreadcrumbPage(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Breadcrumb.Page
      {...forwarded(rest)}
      className={classNames(props(styles.page, xstyle).className, className)}
    >
      {children}
    </Breadcrumb.Page>
  );
}

/** The mark between two crumbs, a chevron unless it is given; decoration. */
component BreadcrumbSeparator(
  children?: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Breadcrumb.Separator {
  return (
    <Breadcrumb.Separator
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
    </Breadcrumb.Separator>
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

/**
 * The parts, under the names `import * as Breadcrumb from "./breadcrumb.js"` gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes `<Breadcrumb.Root>`
 * and `<Breadcrumb.List>`, the way it writes `@uniflowed/ui`'s own parts. Each
 * is declared under its full name, so React DevTools and an error say
 * `BreadcrumbRoot` rather than `Root`.
 */
export {
  BreadcrumbRoot as Root,
  BreadcrumbList as List,
  BreadcrumbItem as Item,
  BreadcrumbLink as Link,
  BreadcrumbPage as Page,
  BreadcrumbSeparator as Separator,
};
