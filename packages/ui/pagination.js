// @flow
//
// Pagination: the navigation a table needs to be usable, and its four rules.
//
// It is here rather than in `table.js` because it is `crates/uf_lib`'s own
// entry and because a paginated list is not always a table — but it is written
// for the table next door, and the two are documented together.
//
// Four things, each of which is invisible when it is missing:
//
//   * **It is navigation, so it is a `<nav>` with a name.** A page has more
//     than one `nav`, and an unnamed one is announced as "navigation" with no
//     way to tell it from the site's menu. `aria-label="Pagination"` is what
//     puts it in a screen reader's landmark list under a useful name.
//   * **The current page is `aria-current="page"`.** Not a class, not bold
//     text, not `aria-selected` — `page` is the value ARIA defines for exactly
//     this, and it is the only one that tells a reader where they are.
//   * **Previous and next are named as such.** A link whose content is `‹` is
//     announced as "link, left single quotation mark", which is not a thing
//     anybody can act on. The glyph stays; the name is words.
//   * **The change is announced.** Pressing "next" replaces the rows and moves
//     nothing a reader is looking at, so a live region that was already there
//     says "Page 4 of 25". `combobox.js` states the rule about why it has to
//     have been there first.
//
// # Why the controls are links
//
// `Pagination.Item` renders an `<a>`, not a `<button>`, and that is an opinion
// worth stating because it constrains the caller: page four of a table is a
// *place*, and a reader expects to be able to open it in a new tab, copy it,
// bookmark it and come back to it. A list paginated with buttons is a list
// whose fourth page does not exist as far as the rest of the web is concerned.
//
// An application that genuinely has no URL for a page — a modal, an unsaved
// draft — is the case where this is the wrong component, and a `<button>` the
// caller writes themselves is the right answer. That is a smaller cost than
// making every well-behaved application invent its own links.
//
// # No `"use client"`
//
// Nothing here holds state, listens to anything or moves focus. Which page is
// current is the caller's, the links are links, and the announcement is a
// string in a div. It renders on a server.

import * as React from "@uniflowed/react";

import type { Rest } from "./internal/merge-props.js";
import { withoutComposed } from "./internal/merge-props.js";

/**
 * The pagination, as a named landmark, and the region that announces it.
 *
 * `page` and `pageCount` are what the announcement says. They are separate
 * from which `Pagination.Item` is marked current because the two answer
 * different questions — a reader is told "page 4 of 25" whether or not 25
 * links are on screen, and a component that showed five links out of
 * twenty-five would otherwise announce "page 4 of 5".
 */
export component PaginationRoot(
  children: React.Node,
  label?: string = "Pagination",
  page?: number | null = null,
  pageCount?: number | null = null,
  announcePage?: (page: number, pageCount: number) => string,
  ...rest: Rest
) {
  const message =
    page == null || pageCount == null ? "" : (announcePage ?? defaultAnnouncement)(page, pageCount);

  return (
    <>
      <nav {...rest} aria-label={label}>
        {children}
      </nav>
      {/*
        Beside the navigation rather than inside it, so a reader walking the
        landmark hears the links and not a sentence about them — and mounted
        from the first render holding nothing, because a live region that
        appears together with its text is not announced at all.
      */}
      <div aria-atomic="true" aria-live="polite" data-uf-pagination-status="" role="status">
        {message}
      </div>
    </>
  );
}

/**
 * The list of pages.
 *
 * A real list, so a reader is told how many there are before walking them and
 * can skip the whole thing in one keystroke.
 */
export component PaginationContent(
  children: renders* (PaginationItem | PaginationPrevious | PaginationNext),
  ...rest: Rest
) {
  return <ul {...rest}>{children}</ul>;
}

/**
 * One page.
 *
 * The `<li>` is structural and takes nothing; everything a caller passes goes
 * on the `<a>`, which is what they style and what a reader activates.
 */
export component PaginationItem(
  children: React.Node,
  current?: boolean = false,
  disabled?: boolean = false,
  ...rest: Rest
) {
  return (
    <PageLink current={current} disabled={disabled} rest={rest}>
      {children}
    </PageLink>
  );
}

/**
 * The link to the page before this one.
 *
 * `label` is its accessible name and has a default, because the content of
 * this link is a chevron every time — and "link, left single quotation mark"
 * is not something a reader can act on. Pass `label` to translate it; the
 * glyph stays whatever the caller rendered.
 */
export component PaginationPrevious(
  children?: React.Node,
  label?: string = "Previous page",
  disabled?: boolean = false,
  ...rest: Rest
) {
  return (
    <PageLink disabled={disabled} label={label} rest={rest}>
      {children}
    </PageLink>
  );
}

/** The link to the page after this one. See `Pagination.Previous`. */
export component PaginationNext(
  children?: React.Node,
  label?: string = "Next page",
  disabled?: boolean = false,
  ...rest: Rest
) {
  return (
    <PageLink disabled={disabled} label={label} rest={rest}>
      {children}
    </PageLink>
  );
}

/**
 * The `<li><a>` the three parts above all render.
 *
 * `rest` arrives as a *named prop* rather than as a spread, and that is not a
 * style choice. `Rest` is the type of props on their way onto an intrinsic —
 * `merge-props.js` explains why an intrinsic's own props are unchecked here —
 * and spreading its `mixed` indexer onto a typed component makes the checker
 * say, correctly, that `disabled` might not be a boolean. Handing the bag over
 * as one value keeps it a bag until it reaches the element it was always for.
 *
 * `disabled` drops the `href` rather than adding an attribute, because there
 * is no such thing as a disabled link: an `<a>` with no `href` is not in the
 * tab order and is not announced as a link, which is exactly what "there is no
 * previous page" means. `aria-disabled` is there too, so a reader who reaches
 * it another way is told why it does nothing.
 */
component PageLink(
  rest: Rest,
  children?: React.Node,
  current?: boolean = false,
  disabled?: boolean = false,
  label?: string,
) {
  const passed = withoutComposed(rest, disabled ? ["href"] : []);

  return (
    <li>
      <a
        {...passed}
        aria-current={current ? "page" : undefined}
        aria-disabled={disabled ? "true" : undefined}
        aria-label={label}
      >
        {children}
      </a>
    </li>
  );
}

/** The wording used when the caller supplies none. */
function defaultAnnouncement(page: number, pageCount: number): string {
  return `Page ${String(page)} of ${String(pageCount)}.`;
}
