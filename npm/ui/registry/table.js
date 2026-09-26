"use client";
// @flow
//
// Table: rows of data under column headings, sortable by a heading and
// selectable a row at a time.
//
// `uf ui add table` wrote this file into the project, and it is the project's
// from then on. `uf ui diff table` shows how it has moved away from the registry
// in the uf you are running.
//
// # What this file owns, and what it does not
//
// The frame the table scrolls sideways in, the rules between rows, the headings'
// type, the sort buttons and the arrow beside a sorted heading.
// `@uniflowed/ui`'s `Table` owns the markup and what it says: a real `<table>` with
// `<th scope="col">`, a button inside every sortable heading and `aria-sort` on
// the heading the rows are sorted by, a polite announcement when the sort
// changes, and `aria-rowcount` and `aria-rowindex` for a table that shows part
// of its rows. The arrow is drawn from `aria-sort`. Rows are chosen with this
// registry's `Checkbox`, and a select-all shows mixed while only some are.
//
// The part gives a caller no prop for its sort button. This file reaches the
// button through `render`, which hands it over as the heading's child. The
// part's own select checkboxes are not used, because they take no children and
// so cannot hold a box and its marks.
//
// # What to keep true when you change it
//
// * **Give it a caption.** `Table.Caption` names the table for a reader moving
//   between tables.
// * **The page sorts.** A heading asks for a sort; the order the rows come in
//   is the page's, because the data can be remote or paged.
// * **Name every checkbox.** `Table.RowSelect` takes a `label` saying which row
//   it chooses, such as the row's name.
// * **Text stays on measured pairs.** `ink` and `muted` on `surface`, which
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import type { Sort } from "@uniflowed/ui";
import { Table } from "@uniflowed/ui";

import { Checkbox } from "./checkbox.js";

export type { Sort } from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  frame: {
    boxSizing: "border-box",
    width: "100%",
    overflowX: "auto",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.muted,
  },
  table: {
    width: "100%",
    borderCollapse: "collapse",
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    lineHeight: ufTokens.leadingBase,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
  },
  caption: {
    captionSide: "top",
    paddingBlock: ufTokens.space2,
    textAlign: "start",
    fontWeight: ufTokens.weightMedium,
    color: ufTokens.ink,
  },
  row: {
    borderBlockEndWidth: "1px",
    borderBlockEndStyle: "solid",
    borderBlockEndColor: ufTokens.border,
  },
  head: {
    // Read by the arrow inside the sort button, which cannot see this heading.
    "--uf-table-arrow-shown": { default: "0", ":is([aria-sort])": "1" },
    "--uf-table-arrow-turn": { default: "0deg", ":is([aria-sort=descending])": "180deg" },
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    textAlign: "start",
    verticalAlign: "bottom",
    fontWeight: ufTokens.weightMedium,
    whiteSpace: "nowrap",
    borderBlockEndWidth: "2px",
    borderBlockEndStyle: "solid",
    borderBlockEndColor: ufTokens.border,
  },
  sort: {
    display: "inline-flex",
    alignItems: "center",
    gap: ufTokens.space1,
    margin: 0,
    padding: 0,
    fontFamily: "inherit",
    fontSize: "inherit",
    fontWeight: "inherit",
    lineHeight: "inherit",
    color: "inherit",
    backgroundColor: "transparent",
    borderWidth: 0,
    borderRadius: ufTokens.radiusSm,
    cursor: "pointer",
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  arrow: {
    flexShrink: 0,
    opacity: "var(--uf-table-arrow-shown, 0)",
    transform: "rotate(var(--uf-table-arrow-turn, 0deg))",
  },
  cell: {
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    textAlign: "start",
    verticalAlign: "middle",
  },
  rowHeader: {
    paddingBlock: ufTokens.space2,
    paddingInline: ufTokens.space3,
    textAlign: "start",
    verticalAlign: "middle",
    fontWeight: ufTokens.weightMedium,
  },
});

/**
 * The table, in a frame it scrolls sideways in when it is wider than the page.
 * `sort` and `onSortChange` are the page's to act on.
 */
component TableRoot(
  children: React.Node,
  sort?: Sort | null,
  defaultSort?: Sort | null = null,
  onSortChange?: (sort: Sort | null) => void,
  rowCount?: number | null = null,
  rowOffset?: number = 0,
  announceSort?: (column: string, direction: "ascending" | "descending") => string,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <div {...props(styles.frame)}>
      <Table.Root
        {...forwarded(rest)}
        announceSort={announceSort}
        className={classNames(props(styles.table, xstyle).className, className)}
        defaultSort={defaultSort}
        onSortChange={onSortChange}
        rowCount={rowCount}
        rowOffset={rowOffset}
        sort={sort}
      >
        {children}
      </Table.Root>
    </div>
  );
}

/** The table's name, over it. */
component TableCaption(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Table.Caption
      {...forwarded(rest)}
      className={classNames(props(styles.caption, xstyle).className, className)}
    >
      {children}
    </Table.Caption>
  );
}

/** The rows of headings. */
component TableHeader(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Table.Header {...forwarded(rest)} className={classNames(props(xstyle).className, className)}>
      {children}
    </Table.Header>
  );
}

/** The rows of data. */
component TableBody(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Table.Body {...forwarded(rest)} className={classNames(props(xstyle).className, className)}>
      {children}
    </Table.Body>
  );
}

/** One row. `index` is its place in the whole data when the table shows part of it. */
component TableRow(
  children: React.Node,
  index?: number | null = null,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Table.Row
      {...forwarded(rest)}
      className={classNames(props(styles.row, xstyle).className, className)}
      index={index}
    >
      {children}
    </Table.Row>
  );
}

/**
 * A column heading. With a `column`, it is a button that asks for the rows to
 * be sorted by it, with an arrow while they are.
 */
component TableHead(
  children: React.Node,
  column?: string | null = null,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  const classes = classNames(props(styles.head, xstyle).className, className);
  if (column == null) {
    return (
      <Table.Head {...forwarded(rest)} className={classes}>
        {children}
      </Table.Head>
    );
  }
  return (
    <Table.Head
      {...forwarded(rest)}
      className={classes}
      column={column}
      render={(heading) => <th {...heading}>{dressed(heading.children)}</th>}
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
        <path d="m6 15 6-6 6 6" />
      </svg>
    </Table.Head>
  );
}

/** A data cell. */
component TableCell(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Table.Cell
      {...forwarded(rest)}
      className={classNames(props(styles.cell, xstyle).className, className)}
    >
      {children}
    </Table.Cell>
  );
}

/** The cell that names its row, such as a person's name. */
component TableRowHeader(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Table.RowHeader
      {...forwarded(rest)}
      className={classNames(props(styles.rowHeader, xstyle).className, className)}
    >
      {children}
    </Table.RowHeader>
  );
}

/** The checkbox that chooses every row, mixed while only some are chosen. */
component TableSelectAll(
  checked: boolean | "mixed",
  onCheckedChange: (checked: boolean) => void,
  label?: string = "Select all rows",
  disabled?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
) {
  return (
    <Checkbox
      aria-label={label}
      checked={checked === "mixed" ? false : checked}
      className={className}
      disabled={disabled}
      indeterminate={checked === "mixed"}
      onCheckedChange={onCheckedChange}
      xstyle={xstyle}
    />
  );
}

/** The checkbox that chooses one row. `label` says which. */
component TableRowSelect(
  label: string,
  checked: boolean,
  onCheckedChange: (checked: boolean) => void,
  disabled?: boolean = false,
  xstyle?: StyleArgument,
  className?: string,
) {
  return (
    <Checkbox
      aria-label={label}
      checked={checked}
      className={className}
      disabled={disabled}
      onCheckedChange={onCheckedChange}
      xstyle={xstyle}
    />
  );
}

/**
 * The part's sort button, with this file's class on it. `Table.Head` builds the
 * button itself and has no prop for it, so it is reached through `render`,
 * which hands the button over as the heading's child: ubugeeei-prod/uf#1074.
 */
function dressed(button: mixed): React.Node {
  if (!React.isValidElement(button)) {
    return button as $FlowFixMe;
  }
  return React.cloneElement(button as $FlowFixMe, { className: props(styles.sort).className });
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
 * The parts, under the names `import * as Table from "./table.js"` gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes `<Table.Root>`
 * and `<Table.Caption>`, the way it writes `@uniflowed/ui`'s own parts. Each
 * is declared under its full name, so React DevTools and an error say
 * `TableRoot` rather than `Root`.
 */
export {
  TableRoot as Root,
  TableCaption as Caption,
  TableHeader as Header,
  TableBody as Body,
  TableRow as Row,
  TableHead as Head,
  TableCell as Cell,
  TableRowHeader as RowHeader,
  TableSelectAll as SelectAll,
  TableRowSelect as RowSelect,
};
