// @flow
//
// A breadcrumb trail, read as a trail rather than as punctuation.
//
// It is `pagination.js`'s shape — a named `<nav>` around a list, with one item
// marked `aria-current="page"` — and it is a separate module for the same
// reason those two are separate entries: a paginated table and a trail through
// a hierarchy are different things to a reader, and the parts are named after
// what they mean rather than after what they render.
//
// Three decisions, each of which is invisible when it is missing:
//
//   * **It is navigation, so it is a `<nav>` with a name.** A page has more
//     than one `nav` and an unnamed one is announced as "navigation", with
//     nothing to tell it from the site's menu. `aria-label="Breadcrumb"` is
//     what puts it in a screen reader's landmark list under a useful name, and
//     it is the name assistive technology's own documentation tells readers to
//     look for.
//   * **The last item is `aria-current="page"`, and it is not a link.** It is
//     where the reader already is. A trail whose last entry is a link that
//     leads to the page it is on is a link that does nothing, and announcing
//     "link" for it is a promise the page does not keep — so `Breadcrumb.Page`
//     is a `<span>`. The `role="link"` with `aria-disabled` that this component
//     is usually copied with says "link, dimmed", which is a *control the
//     reader cannot use* rather than a place they have arrived at.
//   * **The separators are `aria-hidden`.** Otherwise the trail is read as
//     "Home slash Settings slash Billing", and the slashes are the loudest
//     thing in it. They are `<li>` elements because an `<ol>` may only contain
//     `<li>`, and they carry `role="presentation"` as well so that the count a
//     reader is given — "list, three items" — is the number of places and not
//     the number of places plus the punctuation between them.
//
// # What the type says that the markup cannot
//
// `Breadcrumb.List` declares `renders* (Breadcrumb.Item | Breadcrumb.Separator)`,
// so a `<div>` between two crumbs is a type error rather than an `<ol>` a
// validator would reject and a screen reader would count wrong.
// `Pagination.Content` states the same constraint for the same element and the
// same reason.
//
// # No `"use client"`
//
// Nothing here holds state, listens to anything or moves focus. Which crumb is
// current is the caller's, and the links are links. It renders on a server.

import * as React from "@uniflowed/react";

import type { Rest } from "./internal/merge-props.js";

/**
 * The trail, as a named landmark.
 *
 * `label` is the accessible name and has a default because there is one right
 * answer in English and it is the one readers are taught to look for. Pass it
 * to translate; there is no case for leaving it off, which is why it is not
 * optional in the sense of being absent.
 */
export component BreadcrumbRoot(
  children: React.Node,
  label?: string = "Breadcrumb",
  ...rest: Rest
) {
  return (
    <nav {...rest} aria-label={label}>
      {children}
    </nav>
  );
}

/**
 * The crumbs, in order.
 *
 * An ordered list rather than a row of links, because the order is the whole
 * information: a reader is told how many levels there are before walking them,
 * and can skip the lot in one keystroke.
 */
export component BreadcrumbList(
  children: renders* (BreadcrumbItem | BreadcrumbSeparator),
  ...rest: Rest
) {
  return <ol {...rest}>{children}</ol>;
}

/** One level of the trail. Holds a `Breadcrumb.Link` or a `Breadcrumb.Page`. */
export component BreadcrumbItem(children: React.Node, ...rest: Rest) {
  return <li {...rest}>{children}</li>;
}

/** A level you can go back to. */
export component BreadcrumbLink(children: React.Node, ...rest: Rest) {
  return <a {...rest}>{children}</a>;
}

/**
 * The level you are on.
 *
 * `aria-current="page"` is the whole of it, and it is on this part rather than
 * being a `current` prop on `Breadcrumb.Link` so that the last crumb cannot be
 * a link by accident. See the module header for why announcing it as a disabled
 * link is worse than announcing it as text.
 */
export component BreadcrumbPage(children: React.Node, ...rest: Rest) {
  return (
    <span {...rest} aria-current="page">
      {children}
    </span>
  );
}

/**
 * The mark between two crumbs.
 *
 * The glyph is the caller's — a slash, a chevron, an icon — because it is a
 * design decision and this package makes none. What is not the caller's is that
 * it is announced to nobody.
 */
export component BreadcrumbSeparator(children?: React.Node, ...rest: Rest) {
  return (
    <li {...rest} aria-hidden="true" role="presentation">
      {children}
    </li>
  );
}
