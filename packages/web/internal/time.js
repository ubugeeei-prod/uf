// @flow
//
// Internal to `@uniflowed/web`: rendering an instant.
//
// The whole difficulty is that a server and a browser are in different places.
// `new Date().toLocaleString()` on a server in UTC and in a browser in Tokyo
// produce different text, React notices during hydration, and the fix people
// reach for — rendering nothing until an effect runs — makes the date invisible
// to a crawler and shifts the layout when it appears.
//
// So `Time` formats deterministically by default: the same string on both
// sides, chosen by the caller rather than by the environment. A caller who
// genuinely wants the reader's locale asks for it, and gets a component that
// renders the deterministic form first and upgrades after hydration — which is
// the honest version of what the naive code was trying to do.

import * as React from "@uniflowed/react";

/** What to show. `iso` and `date` are the same on a server and in a browser. */
export type TimeFormat =
  /** `2026-09-04T06:00:00.000Z`. Unambiguous, and the same everywhere. */
  | "iso"
  /** `2026-09-04`. The UTC calendar date. */
  | "date"
  /** The reader's locale, applied after hydration. */
  | "local"
  /** "3 minutes ago", relative to now, after hydration. */
  | "relative";

/** Milliseconds in each unit, largest first, for the relative form. */
const UNITS: $ReadOnlyArray<[Intl$RelativeTimeFormatUnit, number]> = [
  ["year", 31_536_000_000],
  ["month", 2_592_000_000],
  ["day", 86_400_000],
  ["hour", 3_600_000],
  ["minute", 60_000],
  ["second", 1_000],
];

/** Parse whatever the caller passed into a `Date`. */
function asDate(value: Date | string | number): Date {
  return value instanceof Date ? value : new Date(value);
}

/** The text that is the same on a server and in a browser. */
function stable(at: Date, format: TimeFormat): string {
  return format === "date" ? at.toISOString().slice(0, 10) : at.toISOString();
}

/** "3 minutes ago", or "in 3 minutes". */
export function relative(at: Date, from: Date = new Date()): string {
  const difference = at.getTime() - from.getTime();
  const formatter = new Intl.RelativeTimeFormat(undefined, { numeric: "auto" });
  for (const [unit, span] of UNITS) {
    if (Math.abs(difference) >= span) {
      return formatter.format(Math.round(difference / span), unit);
    }
  }
  // Under a second in either direction is "now", not "in 0 seconds".
  return formatter.format(0, "second");
}

/**
 * An instant, as a `<time>` element.
 *
 * The machine-readable value is always in `dateTime`, whatever the text says,
 * so a crawler and a screen reader get the exact instant even when a reader
 * sees "3 minutes ago".
 *
 * `iso` and `date` render the same string on both sides and never change.
 * `local` and `relative` render the stable form first and replace it after
 * hydration — the text is there for the first paint and for anything that does
 * not run JavaScript, and it becomes the reader's own format once it can.
 */
export component Time(
  value: Date | string | number,
  format?: TimeFormat = "iso",
  locale?: string,
  className?: string,
) renders React.Node {
  const at = asDate(value);
  const machine = at.toISOString();
  const server = stable(at, format);

  // Starts at the deterministic text on both sides, so hydration matches; the
  // effect below is what makes it the reader's.
  const [text, setText] = React.useState(server);

  React.useEffect(() => {
    if (format === "local") {
      setText(at.toLocaleString(locale));
    } else if (format === "relative") {
      setText(relative(at));
    }
    // `machine` rather than `at`: a caller passing a string builds a new `Date`
    // every render, and depending on the object would re-run this forever.
  }, [machine, format, locale]);

  return (
    <time dateTime={machine} className={className}>
      {text}
    </time>
  );
}
