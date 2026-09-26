"use client";
// @flow
//
// Calendar: a month of days to choose one from, moved through from the keyboard
// and paged a month at a time.
//
// `uf ui add calendar` wrote this file into the project, and it is the project's
// from then on. `uf ui diff calendar` shows how it has moved away from the
// registry in the uf you are running.
//
// # What this file owns, and what it does not
//
// The month's frame, the buttons to the months either side, and each day:
// today in the accent's numeral, the chosen day filled, a range's middle as a
// pale band between its filled ends, and a day that cannot be chosen muted and
// struck through. `@uniflowed/ui`'s `Calendar` owns the grid: a `<table
// role="grid">` named by its caption, weekday headings that give a reader the
// whole weekday name, one day in the tab order with the keyboard moving between
// days and months, and a polite announcement of the month shown. A day is drawn
// from `aria-selected`, `aria-current="date"`, `aria-disabled`, and
// `data-selection-start` / `data-selection-end` on the ends of a chosen run.
//
// The caption and the weekday headings are the part's own elements and take
// this file's classes through `captionClassName` and `columnHeaderClassName`.
// The month buttons hang on the caption's line, at its inline ends. Every prop
// of the part's root, such as `value`, `locale` or `isDateDisabled`, passes
// through `Calendar` unchanged.
//
// # What to keep true when you change it
//
// * **Name the month buttons.** They show only an arrow; `label` is what a
//   reader hears.
// * **Chosen is more than a colour.** The chosen day is filled, and a reader
//   hears it as selected. Today is bold as well as coloured.
// * **Numerals line up.** Days are `tabular-nums` and each weekday heading is
//   a day's width, so the columns are columns in any locale.
// * **Keep the ring.** One day takes focus at a time, and its outline is how a
//   sighted keyboard user follows it.
// * **Text stays on measured pairs.** `ink`, `muted` and `accent` on `surface`
//   and `surfaceHover`, `ink` and `accent` on `accentSoft` inside a range, and
//   `accentInk` on `accent` for the chosen day, which
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import { Calendar } from "@uniflowed/ui";

export type { DateValue } from "@uniflowed/ui";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  // A block rather than a grid: the header is a zero-height row the month
  // slides under (see `header`), and a grid's gap would open a space between
  // the two that pushed the caption off the buttons' line.
  root: {
    display: "inline-block",
    boxSizing: "border-box",
    padding: ufTokens.space3,
    fontFamily: ufTokens.fontSans,
    fontSize: ufTokens.textSm,
    color: ufTokens.ink,
    backgroundColor: ufTokens.surface,
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusMd,
  },
  // The month buttons sit on the caption's line, one at each end, with the
  // month's name between them. The caption belongs to the grid — it is the
  // grid's accessible name — so it cannot move into this row; instead this row
  // takes no height, and its buttons hang over the caption's ends. `inline`
  // ends, so a right-to-left page puts "previous" on the right.
  header: {
    position: "relative",
    zIndex: 1,
    display: "flex",
    alignItems: "flex-start",
    justifyContent: "space-between",
    height: 0,
  },
  step: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    boxSizing: "border-box",
    width: "32px",
    height: "32px",
    margin: 0,
    padding: 0,
    color: { default: ufTokens.muted, ":hover": ufTokens.ink },
    backgroundColor: { default: "transparent", ":hover": ufTokens.surfaceHover },
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: "transparent",
    borderRadius: ufTokens.radiusSm,
    cursor: { default: "pointer", ":disabled": "not-allowed" },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "-2px",
  },
  // Mirrored in a right-to-left page, where "previous" points right.
  chevron: {
    display: "block",
    transform: { default: "none", ":dir(rtl)": "scaleX(-1)" },
  },
  month: {
    // No space between columns, so a range reads as one band; a little
    // between weeks, so the rows stay rows.
    borderCollapse: "separate",
    borderSpacing: "0 2px",
    // Columns a day wide whatever the headings say: some locales' short
    // weekday names are whole words, and an automatic layout widened their
    // columns and pulled the numerals out of line.
    tableLayout: "fixed",
    margin: 0,
    fontSize: ufTokens.textSm,
    textAlign: "center",
  },
  // The month and year, on the buttons' line and clear of them.
  caption: {
    captionSide: "top",
    boxSizing: "border-box",
    height: "32px",
    paddingInline: "36px",
    marginBottom: ufTokens.space1,
    overflow: "hidden",
    fontSize: ufTokens.textSm,
    fontWeight: ufTokens.weightMedium,
    lineHeight: "32px",
    whiteSpace: "nowrap",
    textOverflow: "ellipsis",
    color: ufTokens.ink,
  },
  // As wide as a day, so each initial stands over its column.
  weekday: {
    boxSizing: "border-box",
    width: "36px",
    height: "28px",
    padding: 0,
    fontSize: ufTokens.textXs,
    fontWeight: ufTokens.weightMedium,
    lineHeight: 1,
    color: ufTokens.muted,
    textAlign: "center",
    verticalAlign: "middle",
    // A long short name is clipped rather than allowed to widen its column;
    // the heading's `aria-label` still gives the reader the whole name.
    overflow: "hidden",
    whiteSpace: "nowrap",
  },
  day: {
    boxSizing: "border-box",
    width: "36px",
    height: "36px",
    padding: 0,
    fontSize: ufTokens.textSm,
    lineHeight: 1,
    // Every numeral the same width, so 11 does not sit narrower than 18.
    fontVariantNumeric: "tabular-nums",
    textAlign: "center",
    verticalAlign: "middle",
    cursor: { default: "pointer", ":is([aria-disabled=true])": "not-allowed" },
    // Today is the accent's numeral in bold, not a ring or a fill: a mark you
    // find when you look for it. The chosen day is the one fill in the month,
    // and the days between a range's ends are a pale band.
    //
    // The states overlap — today can be chosen, or unavailable, or inside a
    // range — and a key with more conditions sorts later, so each overlap is
    // written as its own key rather than left to the order of two equal ones.
    color: {
      default: ufTokens.ink,
      ":is([aria-current=date])": ufTokens.accent,
      ":is([aria-disabled=true]):not([aria-selected=true])": ufTokens.muted,
      ":is([aria-selected=true]):not([aria-disabled=true])": ufTokens.accentInk,
      ":is([aria-selected=true]):not([data-selection-start]):not([data-selection-end])":
        ufTokens.ink,
      ":is([aria-current=date]):is([aria-selected=true]):not([data-selection-start]):not([data-selection-end])":
        ufTokens.accent,
    },
    backgroundColor: {
      default: "transparent",
      ":hover": ufTokens.surfaceHover,
      ":is([aria-disabled=true])": "transparent",
      ":is([aria-selected=true])": ufTokens.accent,
      ":is([aria-selected=true]):not([data-selection-start]):not([data-selection-end])":
        ufTokens.accentSoft,
    },
    textDecorationLine: { default: "none", ":is([aria-disabled=true])": "line-through" },
    fontWeight: {
      default: ufTokens.weightRegular,
      ":is([aria-selected=true])": ufTokens.weightMedium,
      ":is([aria-current=date]):not([aria-disabled=true])": ufTokens.weightBold,
    },
    borderWidth: 0,
    // A range's ends are square where they meet the band, named by inline
    // side so the join is right in a right-to-left page too.
    borderStartStartRadius: {
      default: ufTokens.radiusSm,
      ":is([aria-selected=true]):not([data-selection-start])": 0,
    },
    borderEndStartRadius: {
      default: ufTokens.radiusSm,
      ":is([aria-selected=true]):not([data-selection-start])": 0,
    },
    borderStartEndRadius: {
      default: ufTokens.radiusSm,
      ":is([aria-selected=true]):not([data-selection-end])": 0,
    },
    borderEndEndRadius: {
      default: ufTokens.radiusSm,
      ":is([aria-selected=true]):not([data-selection-end])": 0,
    },
    // Inside the cell, so neighbours never cover it; light on the filled day,
    // where the focus colour would vanish into the accent.
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: {
      default: ufTokens.focus,
      ":is([aria-selected=true])": ufTokens.accentInk,
      ":is([aria-selected=true]):not([data-selection-start]):not([data-selection-end])":
        ufTokens.focus,
    },
    outlineOffset: "-3px",
  },
});

/**
 * The calendar. Every prop of `Calendar.Root` passes through: `value` or
 * `defaultValue`, `onValueChange`, `locale`, `today`, `isDateDisabled` and the
 * rest.
 */
component CalendarRoot(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Calendar.Root
      {...forwarded(rest)}
      className={classNames(props(styles.root, xstyle).className, className)}
    >
      {children}
    </Calendar.Root>
  );
}

/** The row over the month that holds the buttons to the months either side. */
component CalendarHeader(children: React.Node, xstyle?: StyleArgument, className?: string) {
  return (
    <div className={classNames(props(styles.header, xstyle).className, className)}>{children}</div>
  );
}

/** The button to the month before. */
component CalendarPrevious(
  label?: string = "Previous month",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Calendar.Previous
      {...forwarded(rest)}
      aria-label={label}
      className={classNames(props(styles.step, xstyle).className, className)}
    >
      <Chevron path="m15 18-6-6 6-6" />
    </Calendar.Previous>
  );
}

/** The button to the month after. */
component CalendarNext(
  label?: string = "Next month",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Calendar.Next
      {...forwarded(rest)}
      aria-label={label}
      className={classNames(props(styles.step, xstyle).className, className)}
    >
      <Chevron path="m9 18 6-6-6-6" />
    </Calendar.Next>
  );
}

/** The month shown: its caption, its weekday headings, and a cell for every day. */
component CalendarMonth(xstyle?: StyleArgument, className?: string, ...rest: Rest) {
  const day = props(styles.day).className;
  return (
    <Calendar.Month
      {...forwarded(rest)}
      captionClassName={props(styles.caption).className}
      className={classNames(props(styles.month, xstyle).className, className)}
      columnHeaderClassName={props(styles.weekday).className}
    >
      {(date) => <Calendar.Day className={day} date={date} />}
    </Calendar.Month>
  );
}

/** An arrow for a month button. */
component Chevron(path: string) {
  return (
    <svg
      {...props(styles.chevron)}
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
      <path d={path} />
    </svg>
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
 * The parts, under the names `import * as Calendar from "./calendar.js"` gives them.
 *
 * One name per component (ubugeeei-prod/uf#1453): a page writes `<Calendar.Root>`
 * and `<Calendar.Header>`, the way it writes `@uniflowed/ui`'s own parts. Each
 * is declared under its full name, so React DevTools and an error say
 * `CalendarRoot` rather than `Root`.
 */
export {
  CalendarRoot as Root,
  CalendarHeader as Header,
  CalendarPrevious as Previous,
  CalendarNext as Next,
  CalendarMonth as Month,
};
