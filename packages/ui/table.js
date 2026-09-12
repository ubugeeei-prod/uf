// @flow
//
// A table, and the four things about one a caller cannot get right by hand.
//
// The markup is not one of them. A `<table>` with `<th scope="col">` and a
// `<caption>` is already accessible, and a component that only renames those
// elements has added a dependency and no behaviour. What this owns is the part
// that is invisible until somebody uses a screen reader on it:
//
//   * **Sorting.** `aria-sort` on exactly one header, the header's content in a
//     button so the sort is reachable at all, and the re-order *announced* —
//     because the rows change places and a screen reader is told nothing.
//   * **Selection.** A header checkbox that is `mixed` when some rows are
//     chosen, and a row checkbox whose name says which row.
//   * **Counting.** `aria-rowcount` and `aria-rowindex`, so a reader on page
//     four is not told "row 3 of 10".
//   * **Pagination**, which is `pagination.js` next door.
//
// # `table`, not `grid`, and no opt-in
//
// "Make it a `role="grid"`" is the advice that circulates and it is usually
// wrong. `grid` takes the arrow keys away from the reader and gives them to
// the component: in a grid, arrows move between cells, which is right for a
// spreadsheet and wrong for a list of records — because a screen reader
// already has its own table-reading commands, they work perfectly on a plain
// `<table>`, and readers rely on them.
//
// So this is a real `<table>` and there is no `role="grid"` flag. A flag would
// be a stub: a grid is not an attribute, it is a two-dimensional keyboard
// contract — focus in the cells, `Ctrl+Home` to the first cell, `PageUp` and
// `PageDown` by a screenful — and `role="grid"` without it is strictly worse
// than what it replaced, because the reader is told the arrow keys will do
// something and they do nothing. An editable grid is a component of its own and
// is worth writing as one when somebody needs it.
//
// # `DataTable` is not here either, and that is the same decision shadcn made
//
// The benchmark ships a *guide* rather than a component — "instead of a
// data-table component, I thought it would be more helpful to provide a guide
// on how to build your own" — and composes TanStack Table with its `Table`. uf
// has no TanStack Table equivalent: `@uniflowed/query` is the fetching layer,
// not table state. So shipping a `DataTable` would mean first shipping a
// headless table-state library, which is a real project and a different one.
//
// This module is the half that is uf's to own: the accessibility of a table
// whose state somebody else holds. What is above is deliberate rather than
// unfinished, and `crates/uf_lib/src/ui.rs` says so beside the entry.
//
// # Where the announcement lives
//
// `Table.Root` renders the `<table>` *and* a live region after it, as
// siblings, and the region is there from the first render holding nothing.
// That is the rule `combobox.js` states and `toast.js` is built on: a live
// region added in the same commit as its text is not announced, because the
// technology watching it had nothing to watch.
//
// It is rendered by the root rather than offered as a part a caller places,
// because a sort that announces nothing is the failure this component exists
// to prevent and a part is a thing somebody forgets. The cost is one extra
// element in the caller's layout, and a sentence that is visible until they
// style it — which is not a bad thing to see, and a table that shows its sort
// status in words is a table more people can use.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useEffect, useId, useMemo, useState } from "@uniflowed/react";
import { useStableCallback } from "@uniflowed/hooks/lifecycle";

import { Checkbox } from "./checkbox.js";
import type { RenderProp, Rest } from "./internal/merge-props.js";
import {
  composeHandlers,
  composeRefs,
  withProps,
  withoutComposed,
} from "./internal/merge-props.js";
import { useControlled } from "./internal/controlled-state.js";

/** Which column a table is sorted by, and which way. */
export type Sort = {|
  readonly column: string,
  readonly direction: "ascending" | "descending",
|};

type TableState = {|
  readonly sort: Sort | null,
  readonly setSort: (sort: Sort | null) => void,
  /** How many rows the whole set has, which is not how many are rendered. */
  readonly rowCount: number | null,
  /** Where the rendered rows start in that set, counting from zero. */
  readonly rowOffset: number,
  readonly headerRows: number,
  readonly registerHeader: (present: boolean) => void,
  readonly captionId: string,
  readonly captioned: boolean,
  readonly registerCaption: (present: boolean) => void,
  /** What each sortable column is called, for the announcement. */
  readonly labels: { readonly [string]: string },
  readonly registerLabel: (column: string, label: string) => void,
|};

const TableContext: React.Context<TableState | null> = createContext(null);

/** Whether the rows below are header rows, which decides `th` versus `td`. */
const HeaderContext: React.Context<boolean> = createContext(false);

hook useTable(part: string): TableState {
  const state = useContext(TableContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Table.Root`);
  }
  return state;
}

/**
 * The table, and the region that announces what happens to it.
 *
 * `rowCount` is how many rows the *whole* set has — five hundred people, not
 * the ten on this page — and the component adds the header row to reach
 * `aria-rowcount`, which ARIA defines as every row in the table. Doing that
 * arithmetic here rather than asking the caller for `501` is the point of
 * having a component: `aria-rowcount` is the single most commonly missing
 * attribute in data tables, and the reason is that nobody wants to think about
 * whether the header counts.
 *
 * `announceSort` is the wording of the announcement, for an application with a
 * translation table. The default is English.
 */
export component TableRoot(
  children: React.Node,
  sort?: Sort | null,
  defaultSort?: Sort | null = null,
  onSortChange?: (sort: Sort | null) => void,
  rowCount?: number | null = null,
  rowOffset?: number = 0,
  announceSort?: (column: string, direction: "ascending" | "descending") => string,
  render?: RenderProp,
  ...rest: Rest
) {
  const base = useId();
  const [current, setCurrent] = useControlled(sort, defaultSort, onSortChange);
  const [headerRows, setHeaderRows] = useState(0);
  const [captioned, setCaptioned] = useState(false);
  const [labels, setLabels] = useState<{ readonly [string]: string }>({});

  const registerHeader = useStableCallback((present: boolean) => {
    setHeaderRows(present ? 1 : 0);
  });

  // Only ever added to. A column that has been rendered once keeps its name,
  // so a sort applied from outside — a URL, a saved preference — is announced
  // with the column's own words rather than with its key.
  const registerLabel = useStableCallback((column: string, label: string) => {
    setLabels((held) => (held[column] === label ? held : { ...held, [column]: label }));
  });

  const state = useMemo(
    () => ({
      sort: current,
      setSort: setCurrent,
      rowCount,
      rowOffset,
      headerRows,
      registerHeader,
      captionId: `${base}-caption`,
      captioned,
      registerCaption: setCaptioned,
      labels,
      registerLabel,
    }),
    [
      base,
      current,
      setCurrent,
      rowCount,
      rowOffset,
      headerRows,
      registerHeader,
      captioned,
      labels,
      registerLabel,
    ],
  );

  const message =
    current == null
      ? ""
      : (announceSort ?? defaultAnnouncement)(
          labels[current.column] ?? current.column,
          current.direction,
        );
  const props = withProps(rest, {
    "aria-labelledby": captioned ? `${base}-caption` : undefined,
    "aria-rowcount": rowCount == null ? undefined : rowCount + headerRows,
    children,
  });

  return (
    <TableContext.Provider value={state}>
      {render == null ? <table {...props} /> : render(withProps(props, { role: "table" }))}
      {/*
        Beside the table rather than inside it, because a `<table>` may only
        contain a caption, column groups and row groups — and mounted from the
        first render, holding nothing, because a live region that appears with
        its text is a live region that says nothing.
      */}
      <div aria-atomic="true" aria-live="polite" data-uf-table-status="" role="status">
        {message}
      </div>
    </TableContext.Provider>
  );
}

/**
 * The table's name.
 *
 * A real `<caption>`, which is what gives a `<table>` its accessible name and
 * what a screen reader reads when a reader lands on it. A heading above the
 * table looks the same and is not the table's name.
 */
export component TableCaption(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const table = useTable("Table.Caption");
  const register = table.registerCaption;

  useEffect(() => {
    register(true);
    return () => register(false);
  }, [register]);

  const props = withProps(rest, { children, id: table.captionId });
  if (render != null) {
    return render(withProps(props, { role: "caption" }));
  }
  return <caption {...props} />;
}

/**
 * The header rows.
 *
 * It tells the root that it exists, because `aria-rowcount` and every row's
 * `aria-rowindex` count header rows and a table without one counts differently.
 */
export component TableHeader(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const table = useTable("Table.Header");
  const register = table.registerHeader;

  useEffect(() => {
    register(true);
    return () => register(false);
  }, [register]);

  const props = withProps(rest, { children });

  return (
    <HeaderContext.Provider value={true}>
      {render == null ? <thead {...props} /> : render(withProps(props, { role: "rowgroup" }))}
    </HeaderContext.Provider>
  );
}

/** The data rows. */
export component TableBody(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const props = withProps(rest, { children });
  return (
    <HeaderContext.Provider value={false}>
      {render == null ? <tbody {...props} /> : render(withProps(props, { role: "rowgroup" }))}
    </HeaderContext.Provider>
  );
}

/**
 * One row, and its real position in the whole set.
 *
 * `index` counts from zero within the rows that are rendered — the index a
 * caller already has from mapping this page — and the component turns it into
 * `aria-rowindex`, which counts from one across every row of the table
 * including the header. Ten rows of five hundred on page ten are rows 92 to
 * 101 and the component works that out; a caller who had to would get it wrong
 * once and never find out, because a reader on page four being told "row 3 of
 * 10" looks exactly like a reader being told the truth.
 *
 * Only when the table was given a `rowCount`. A table showing everything it
 * has needs neither attribute — the browser counts the rows itself — and
 * adding them anyway is a second source of truth that can disagree with the
 * document.
 */
export component TableRow(
  children: React.Node,
  index?: number | null = null,
  render?: RenderProp,
  ...rest: Rest
) {
  const table = useTable("Table.Row");
  const header = useContext(HeaderContext);
  const counted = table.rowCount != null;

  // An `if` chain rather than a `match`, because matching on a boolean is a
  // `match` whose subject carries none of the information.
  let rowIndex;
  if (!counted) {
    rowIndex = undefined;
  } else if (header) {
    rowIndex = 1;
  } else {
    rowIndex = table.headerRows + table.rowOffset + (index ?? 0) + 1;
  }

  const props = withProps(rest, { "aria-rowindex": rowIndex, children });
  if (render != null) {
    return render(withProps(props, { role: "row" }));
  }
  return <tr {...props} />;
}

/**
 * A column header, and the sort control when the column has one.
 *
 * `scope="col"` always: it is what tells a screen reader which cells this
 * header names, and it is one attribute that turns a grid of text into a table
 * a reader can navigate.
 *
 * Given a `column`, the header's content becomes a `button`, because a sort a
 * reader cannot reach with the keyboard is a sort half the readers do not have.
 * `aria-sort` then appears on **this header only when it is the sorted one**.
 * Not `"none"` on the others: eleven headers each announcing "not sorted" is
 * eleven announcements of nothing, on every pass through the table.
 */
export component TableHead(
  children: React.Node,
  column?: string | null = null,
  render?: RenderProp,
  ...rest: Rest
) {
  const table = useTable("Table.Head");
  const passed = withoutComposed(rest, column == null ? [] : ["onClick", "ref"]);
  const sorted = column != null && table.sort?.column === column;
  const register = table.registerLabel;
  const [element, setElement] = useState<HTMLElement | null>(null);

  // What the column is called, for the announcement — read from the element
  // rather than from `children`, which may be an icon beside a word or a
  // caller's own component and is not a string anybody can rely on.
  useEffect(() => {
    if (column == null || element == null) {
      return;
    }
    const label = (element.textContent ?? "").replace(/\s+/g, " ").trim();
    if (label !== "") {
      register(column, label);
    }
  });

  if (column == null) {
    const props = withProps(rest, { children, scope: "col" });
    if (render != null) {
      return render(withProps(props, { role: "columnheader" }));
    }
    return <th {...props} />;
  }

  const button = (
    <button
      onClick={composeHandlers(rest.onClick, () => {
        // Two states, not three. A sort that cycles back to "unsorted" gives
        // a reader a third press whose result is a table in an order nobody
        // asked for.
        table.setSort({
          column,
          direction: sorted && table.sort?.direction === "ascending" ? "descending" : "ascending",
        });
      })}
      type="button"
    >
      {children}
    </button>
  );
  const props = withProps(passed, {
    "aria-sort": sorted ? table.sort?.direction : undefined,
    children: button,
    ref: composeRefs(rest.ref, setElement),
    scope: "col",
  });

  if (render != null) {
    return render(withProps(props, { role: "columnheader" }));
  }
  return <th {...props} />;
}

/** One cell. */
export component TableCell(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const props = withProps(rest, { children });
  if (render != null) {
    return render(withProps(props, { role: "cell" }));
  }
  return <td {...props} />;
}

/**
 * A row header: the cell that says which row this is.
 *
 * `scope="row"` is the other half of `scope="col"`, and it is what lets a
 * screen reader say "Ada Lovelace, born 1815" instead of "1815" when a reader
 * moves down the year column. A table of records usually has one and almost
 * never marks it.
 */
export component TableRowHeader(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const props = withProps(rest, { children, scope: "row" });
  if (render != null) {
    return render(withProps(props, { role: "rowheader" }));
  }
  return <th {...props} />;
}

/**
 * The "select all" checkbox.
 *
 * `checked` is `boolean | "mixed"` and that is the whole reason this part
 * exists. `checkbox.js` was written for it:
 *
 * > a half-selected "select all" that clears itself on the first click is the
 * > behaviour every table in every application gets wrong.
 *
 * A `boolean` prop would let a caller pass `false` for "two of three rows are
 * selected", and a reader would be told nothing is selected while three
 * checkboxes below say otherwise. The union makes the third state a case the
 * caller has to answer rather than one they can fail to notice, and choosing a
 * mixed box reports `true` — select all, which is what a reader expects it to
 * move to.
 *
 * # Why this takes named props and not `...rest: Rest`
 *
 * Every other part in this package ends with `...rest: Rest` and spreads it
 * onto an intrinsic. This one renders a `Checkbox`, which is a *typed*
 * component, and `Rest`'s `mixed` indexer cannot promise that `indeterminate`
 * is a boolean — `uf check` says so, and it is right to. `Rest` is the type of
 * props on their way onto an element whose own props are unchecked, which
 * `merge-props.js` explains; it is not a way to pass anything to anything.
 *
 * So the props are written out. `className` is here because styling is what a
 * caller actually needs to pass; anything more than that is a sign the caller
 * wants a `Checkbox` of their own, which they should write — this part exists
 * for the type of `checked`, not for the markup.
 */
export component TableSelectAll(
  checked: boolean | "mixed",
  onCheckedChange: (checked: boolean) => void,
  label?: string = "Select all rows",
  className?: string,
  disabled?: boolean = false,
  render?: RenderProp,
) {
  return (
    <Checkbox
      aria-label={label}
      checked={checked === "mixed" ? false : checked}
      className={className}
      disabled={disabled}
      indeterminate={checked === "mixed"}
      onCheckedChange={onCheckedChange}
      render={render}
    />
  );
}

/**
 * One row's checkbox.
 *
 * `label` is required, and requiring it is the point. "Select row" repeated
 * forty times is forty identical announcements, and a reader moving through
 * the column hears the same three words with no way to tell which row they are
 * on. `label={`Select ${person.name}`}` is the difference, and it is the sort
 * of thing a component can require and a guide can only suggest.
 *
 * Named props rather than `...rest: Rest`, for the reason `Table.SelectAll`
 * gives above.
 */
export component TableRowSelect(
  label: string,
  checked: boolean,
  onCheckedChange: (checked: boolean) => void,
  className?: string,
  disabled?: boolean = false,
  render?: RenderProp,
) {
  return (
    <Checkbox
      aria-label={label}
      checked={checked}
      className={className}
      disabled={disabled}
      onCheckedChange={onCheckedChange}
      render={render}
    />
  );
}

/** The wording used when the caller supplies none. */
function defaultAnnouncement(column: string, direction: "ascending" | "descending"): string {
  return `Sorted by ${column}, ${direction}.`;
}
