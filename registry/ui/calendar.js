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
// The month's frame, the buttons to the months either side, and each day: the
// chosen day filled, today ringed, and a day that cannot be chosen struck
// through. `@uniflowed/ui/calendar` owns the grid: a `<table role="grid">` named
// by its caption, weekday headings that give a reader the whole weekday name,
// one day in the tab order with the keyboard moving between days and months,
// and a polite announcement of the month shown. A day is drawn from
// `aria-selected`, `aria-current="date"` and `aria-disabled`.
//
// The caption and the weekday headings are rendered by the part with no prop for
// a class, so they take this file's type by inheritance. Every prop of the
// part's root, such as `value`, `locale` or `isDateDisabled`, passes through
// `Calendar` unchanged.
//
// # What to keep true when you change it
//
// * **Name the month buttons.** They show only an arrow; `label` is what a
//   reader hears.
// * **Chosen is more than a colour.** The chosen day is filled, and a reader
//   hears it as selected.
// * **Keep the ring.** One day takes focus at a time, and its outline is how a
//   sighted keyboard user follows it.
// * **Text stays on measured pairs.** `ink` and `muted` on `surface`, `accent`
//   on `accentSoft`, and `accentInk` on `accent` for the chosen day, which
//   `crates/uf_stylex/src/tests/preset.rs` holds to 4.5:1 in both themes.

import * as React from "@uniflowed/react";
import type { StyleArgument } from "@uniflowed/stylex";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";
import * as Primitive from "@uniflowed/ui/calendar";

export type { DateValue } from "@uniflowed/ui/calendar";

/** Every prop a caller passes that this file does not name, for the part. */
type Rest = { readonly key?: empty, readonly [string]: mixed };

const styles = stylex.create({
  root: {
    display: "inline-grid",
    gap: ufTokens.space2,
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
  header: {
    display: "flex",
    alignItems: "center",
    justifyContent: "space-between",
    gap: ufTokens.space2,
  },
  step: {
    display: "inline-flex",
    alignItems: "center",
    justifyContent: "center",
    width: "32px",
    height: "32px",
    margin: 0,
    padding: 0,
    color: { default: ufTokens.ink, ":hover": ufTokens.accent },
    backgroundColor: { default: "transparent", ":hover": ufTokens.accentSoft },
    borderWidth: "1px",
    borderStyle: "solid",
    borderColor: ufTokens.border,
    borderRadius: ufTokens.radiusSm,
    cursor: { default: "pointer", ":disabled": "not-allowed" },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "2px",
  },
  month: {
    borderCollapse: "separate",
    borderSpacing: "2px",
    fontSize: ufTokens.textSm,
    textAlign: "center",
  },
  day: {
    boxSizing: "border-box",
    width: "36px",
    height: "36px",
    padding: 0,
    textAlign: "center",
    verticalAlign: "middle",
    borderRadius: ufTokens.radiusSm,
    cursor: { default: "pointer", ":is([aria-disabled=true])": "not-allowed" },
    color: {
      default: ufTokens.ink,
      ":hover": ufTokens.accent,
      ":is([aria-selected=true])": ufTokens.accentInk,
      ":is([aria-disabled=true])": ufTokens.muted,
    },
    backgroundColor: {
      default: "transparent",
      ":hover": ufTokens.accentSoft,
      ":is([aria-selected=true])": ufTokens.accent,
      ":is([aria-disabled=true])": "transparent",
    },
    textDecoration: { default: "none", ":is([aria-disabled=true])": "line-through" },
    fontWeight: {
      default: ufTokens.weightRegular,
      ":is([aria-current=date])": ufTokens.weightMedium,
    },
    boxShadow: { default: "none", ":is([aria-current=date])": "inset 0 0 0 1px currentColor" },
    outlineWidth: { default: "0", ":focus-visible": "2px" },
    outlineStyle: "solid",
    outlineColor: ufTokens.focus,
    outlineOffset: "-2px",
  },
});

/**
 * The calendar. Every prop of `Calendar.Root` passes through: `value` or
 * `defaultValue`, `onValueChange`, `locale`, `today`, `isDateDisabled` and the
 * rest.
 */
export component Calendar(
  children: React.Node,
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.CalendarRoot
      {...forwarded(rest)}
      className={classNames(props(styles.root, xstyle).className, className)}
    >
      {children}
    </Primitive.CalendarRoot>
  );
}

/** The row over the month that holds the buttons to the months either side. */
export component CalendarHeader(children: React.Node, xstyle?: StyleArgument, className?: string) {
  return (
    <div className={classNames(props(styles.header, xstyle).className, className)}>{children}</div>
  );
}

/** The button to the month before. */
export component CalendarPrevious(
  label?: string = "Previous month",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.CalendarPrevious
      {...forwarded(rest)}
      aria-label={label}
      className={classNames(props(styles.step, xstyle).className, className)}
    >
      <Chevron path="m15 18-6-6 6-6" />
    </Primitive.CalendarPrevious>
  );
}

/** The button to the month after. */
export component CalendarNext(
  label?: string = "Next month",
  xstyle?: StyleArgument,
  className?: string,
  ...rest: Rest
) {
  return (
    <Primitive.CalendarNext
      {...forwarded(rest)}
      aria-label={label}
      className={classNames(props(styles.step, xstyle).className, className)}
    >
      <Chevron path="m9 18 6-6-6-6" />
    </Primitive.CalendarNext>
  );
}

/** The month shown: its caption, its weekday headings, and a cell for every day. */
export component CalendarMonth(xstyle?: StyleArgument, className?: string, ...rest: Rest) {
  const day = props(styles.day).className;
  return (
    <Primitive.CalendarMonth
      {...forwarded(rest)}
      className={classNames(props(styles.month, xstyle).className, className)}
    >
      {(date) => <Primitive.CalendarDay className={day} date={date} />}
    </Primitive.CalendarMonth>
  );
}

/** An arrow for a month button. */
component Chevron(path: string) {
  return (
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
