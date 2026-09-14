// @flow
//
// `@uniflowed/web/time`: rendering an instant.
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
//
// # Temporal, not `Date`
//
// This component is built on `@uniflowed/core/temporal` and the choice is not
// cosmetic. `Date` has one time zone — whichever the machine is set to — and
// the two machines here are set to different ones, which is the entire problem
// restated. There is no way to write "six in the morning UTC, shown as three in
// the afternoon in Tokyo" with a `Date` without going through the host's zone
// on the way, and going through the host's zone is what makes the two renders
// disagree. A `Temporal.Instant` carries no zone and a `Temporal.ZonedDateTime`
// carries the zone it was asked for, so the server's text is a function of the
// instant and a zone name rather than of the machine.
//
// The zone the deterministic text is rendered in comes from the render itself —
// `RenderProvider` fixes it and the markup carries it — so the browser
// reproduces the server's string exactly, and *then* moves to the reader's own
// zone. Without a provider the deterministic zone is UTC, which is unambiguous
// and the same everywhere, rather than the host's, which is neither.
//
// # What belongs in this module
//
// Anything whose difficulty is that the two renders are in different places or
// at different moments: a formatted date, a duration, a countdown, a
// timezone-aware label. The test is whether hydration would notice a
// difference.
//
// Not here: a hook that reads a clock to schedule work — `useInterval` and
// `useTimeout` are `@uniflowed/hooks`'s, and this module renders a value
// rather than driving one. Locale-formatted numbers and currency have the same
// hydration problem and would be a module of their own rather than a second
// subject inside this one, because nothing about a price is about an instant.

import * as React from "@uniflowed/react";
import type { Instant } from "@uniflowed/core/temporal";
import { Temporal } from "@uniflowed/core/temporal";
import { useRenderEnvelope } from "@uniflowed/hooks/render";

/** Whatever a caller has an instant written as. */
export type TimeValue = Instant | Date | string | number;

/** What to show. Everything but `local` and `relative` is the same everywhere. */
export type TimeFormat =
  /** `2026-09-04T06:00:00Z`. Unambiguous, and the same on every machine. */
  | "iso"
  /** `2026-09-04`. The calendar date in the render's zone. */
  | "date"
  /** `2026-09-04 15:00 +09:00`. The wall clock in the render's zone. */
  | "zoned"
  /** The reader's own locale and zone, applied after hydration. */
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

/**
 * Whatever the caller passed, as a `Temporal.Instant`.
 *
 * A string must carry an offset, because Temporal refuses one that does not and
 * this component must not be more permissive than the standard it is built on:
 * `"2026-09-04"` is a date and not an instant, and every implementation that
 * has guessed which midnight it meant has guessed differently.
 */
export function asInstant(value: TimeValue): Instant {
  if (value instanceof Date) {
    return Temporal.Instant.fromEpochMilliseconds(value.getTime());
  }
  if (typeof value === "number") {
    return Temporal.Instant.fromEpochMilliseconds(value);
  }
  if (typeof value === "string") {
    return Temporal.Instant.from(value);
  }
  return value;
}

/** The text that is the same on a server and in a browser. */
function stable(at: Instant, format: TimeFormat, zone: string): string {
  if (format === "iso") {
    return at.toString();
  }
  const zoned = at.toZonedDateTimeISO(zone);
  if (format === "date") {
    return zoned.toPlainDate().toString();
  }
  // `local` and `relative` start here too: it is the most readable form that
  // still says exactly which instant it is, which is what a reader with no
  // JavaScript and a crawler both end up with.
  const time = `${String(zoned.hour).padStart(2, "0")}:${String(zoned.minute).padStart(2, "0")}`;
  return `${zoned.toPlainDate().toString()} ${time} ${zoned.offset}`;
}

/**
 * "3 minutes ago", or "in 3 minutes".
 *
 * `from` defaults to the injected clock rather than to `new Date()`, so a test
 * can decide what "now" is and a server render is reproducible. Both arguments
 * take anything `Time` takes.
 */
export function relative(at: TimeValue, from?: TimeValue): string {
  const target = asInstant(at);
  const origin = from === undefined ? Temporal.Now.instant() : asInstant(from);
  const difference = target.epochMilliseconds - origin.epochMilliseconds;
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
 * `iso`, `date` and `zoned` render the same string on both sides and never
 * change. `local` and `relative` render the deterministic form first and
 * replace it after hydration — the text is there for the first paint and for
 * anything that does not run JavaScript, and it becomes the reader's own format
 * once it can.
 *
 * `zone` is the zone the deterministic forms are written in. It defaults to the
 * one the render fixed, so a page under a `RenderProvider` shows its server's
 * zone until it can show the reader's; with no provider it is UTC, because a
 * default that reads the host would be a different string on each side and
 * would defeat the entire component.
 */
export component Time(
  value: TimeValue,
  format?: TimeFormat = "iso",
  zone?: string,
  locale?: string,
  className?: string,
) renders React.Node {
  const at = asInstant(value);
  const machine = at.toString();
  const rendered = useRenderEnvelope();
  const server = stable(at, format, zone ?? rendered?.timeZone ?? "UTC");

  // Starts at the deterministic text on both sides, so hydration matches; the
  // effect below is what makes it the reader's.
  const [text, setText] = React.useState(server);

  React.useEffect(() => {
    if (format === "local") {
      setText(at.toZonedDateTimeISO(Temporal.Now.timeZoneId()).toLocaleString(locale));
    } else if (format === "relative") {
      setText(relative(at));
    }
    // `machine` rather than `at`: a caller passing a string builds a new instant
    // every render, and depending on the object would re-run this forever.
  }, [machine, format, locale]);

  return (
    <time dateTime={machine} className={className}>
      {text}
    </time>
  );
}
