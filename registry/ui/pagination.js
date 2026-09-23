"use client";
// @flow
//
// Pagination: links to the pages of a long list, with the current page marked
// and a change of page announced.
//
// `uf ui add pagination` wrote this file into the project, and it is the
// project's from then on. `uf ui diff pagination` shows how it has moved away
// from the registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The row of page links, their targets, and the current page's look.
// `@uniflowed/ui`'s `Pagination` owns the markup a reader relies on: a named
// `<nav>`, one `aria-current="page"`, previous and next named in words rather
// than in chevrons, a disabled end that is not a link, and a polite status that
// says "Page 3 of 12" when the page changes. The current page is drawn from
// `aria-current` through `:is([aria-current=page])`.
//
// # What to keep true when you change it
//
// * **Pass `page` and `pageCount` to `Pagination`,** so the change of page is
//   announced rather than only drawn.
// * **Previous and next keep their words.** The chevrons are decoration;
//   "Previous page" is the name.
// * **A row holds pages, previous and next.** `PaginationContent` takes
//   `renders* (PaginationItem | PaginationPrevious | PaginationNext)`, so an
//   ellipsis is a gap in the numbers rather than an item, and anything else in
//   the list is a Flow error.
// * **Every link is a target a finger can hit:** 36px, over the 24px WCAG
//   2.5.8 asks for.
// * **Colour comes from tokens, in measured pairs:** `ink` on `surface` and
//   `surfaceHover`, which `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1
//   in both themes.

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
  content: {
    display: "flex",
    flexWrap: "wrap",
    alignItems: "center",
    gap: ufTokens.space1,
    margin: 0,
    padding: 0,
    listStyle: "none",
  },
  link: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    gap: ufTokens.space1,
    boxSizing: "border-box",
    minWidth: "36px",
    minHeight: "36px",
    paddingInline: ufTokens.space2,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    fontWeight: {
      default: ufTokens.weightRegular,
      ":is([aria-current=page])": ufTokens.weightBold,
    },
    lineHeight: ufTokens.leadingTight,
    color: ufTokens.ink,
    textDecorationLine: "none",
    backgroundColor: {
      default: "transparent",
      ":hover": ufTokens.surfaceHover,
      ":is([aria-disabled=true])": "transparent",
    },
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: { default: "transparent", ":is([aria-current=page])": ufTokens.border },
    borderRadius: ufTokens.radiusMd,
    cursor: { default: "pointer", ":is([aria-disabled=true])": "not-allowed" },
    opacity: { default: 1, ":is([aria-disabled=true])": 0.55 },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
});

/**
 * The navigation. `page` and `pageCount` are what a change of page is
 * announced with.
 */
export component Pagination(
  children: React.Node,
  page?: number | null = null,
  pageCount?: number | null = null,
  label?: string = "Pagination",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.Pagination.Root
      {...forwarded(rest)}
      className={classNames(props(xstyle).className, className)}
      label={label}
      page={page}
      pageCount={pageCount}
    >
      {children}
    </Primitive.Pagination.Root>
  );
}

/** The row of pages, previous and next. */
export component PaginationContent(
  children: renders* (PaginationItem | PaginationPrevious | PaginationNext),
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.Pagination.Content
      {...forwarded(rest)}
      className={classNames(props(styles.content, xstyle).className, className)}
    >
      {children}
    </Primitive.Pagination.Content>
  );
}

/** A link to one page; `current` for the page the reader is on. */
export component PaginationItem(
  children: React.Node,
  current?: boolean = false,
  disabled?: boolean = false,
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.Pagination.Item {
  return (
    <Primitive.Pagination.Item
      {...forwarded(rest)}
      className={classNames(props(styles.link, xstyle).className, className)}
      current={current}
      disabled={disabled}
      render={render}
    >
      {children}
    </Primitive.Pagination.Item>
  );
}

/** The link to the page before, named "Previous page" unless `label` says otherwise. */
export component PaginationPrevious(
  label?: string = "Previous page",
  disabled?: boolean = false,
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.Pagination.Previous {
  return (
    <Primitive.Pagination.Previous
      {...forwarded(rest)}
      className={classNames(props(styles.link, xstyle).className, className)}
      disabled={disabled}
      label={label}
      render={render}
    >
      <svg
        aria-hidden="true"
        fill="none"
        focusable="false"
        height="16"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2"
        viewBox="0 0 24 24"
        width="16"
      >
        <path d="m15 18-6-6 6-6" />
      </svg>
    </Primitive.Pagination.Previous>
  );
}

/** The link to the page after, named "Next page" unless `label` says otherwise. */
export component PaginationNext(
  label?: string = "Next page",
  disabled?: boolean = false,
  render?: RenderProp,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) renders Primitive.Pagination.Next {
  return (
    <Primitive.Pagination.Next
      {...forwarded(rest)}
      className={classNames(props(styles.link, xstyle).className, className)}
      disabled={disabled}
      label={label}
      render={render}
    >
      <svg
        aria-hidden="true"
        fill="none"
        focusable="false"
        height="16"
        stroke="currentColor"
        strokeLinecap="round"
        strokeLinejoin="round"
        strokeWidth="2"
        viewBox="0 0 24 24"
        width="16"
      >
        <path d="m9 18 6-6-6-6" />
      </svg>
    </Primitive.Pagination.Next>
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
