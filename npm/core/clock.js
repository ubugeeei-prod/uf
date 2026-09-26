// @flow
//
// `@uniflowed/core/clock`: the one place uf reads a clock.
//
// A server render and the hydration that follows it happen at two different
// instants, on two different machines, in two different time zones. Anything
// derived from "now" is therefore different on the two sides, React compares
// the markup and reports a mismatch, and the application did nothing wrong —
// it called `Date.now()`, which is the only thing there was to call.
//
// The fix is not to stop reading a clock. It is to stop reading the *host's*
// clock from the middle of a render. This module is the seam: every uf package
// that needs the time asks `currentClock()`, and what that returns is a
// decision the application, the server or a test has already taken. A test that
// wants a deterministic "3 minutes ago" installs a fixed clock instead of
// mocking a global; a server that wants two requests to render reproducibly
// gives each one a clock of its own.
//
// # Why a number, and not a `Date`
//
// `Date` is the wrong currency for a seam. It is mutable, so handing one out
// hands out something the caller can change under you; it is millisecond-only;
// and it carries exactly one time zone, the host's, which is the half of the
// hydration problem that the instant alone does not solve. So the seam carries
// an epoch millisecond count and an IANA zone name — two immutable primitives
// that serialize as themselves — and `@uniflowed/core/temporal` builds the
// calendar arithmetic on top. A clock that returned `Temporal.Instant` would
// have been the more expressive choice and the wrong layering: `Temporal` is
// built from this, so it cannot also be underneath it.
//
// Milliseconds rather than nanoseconds for the same reason. `Date.now()` is
// what every host can actually promise, and inventing three zeroes of
// precision at the seam would be a lie told in the one place that exists to
// stop lies about the time.
//
// # Why the current clock is module state
//
// The alternative was a `clock` parameter on `useInterval`, `useNow`,
// `useTimeAgo`, every `Schedule` decision and every component that formats a
// date — a parameter almost every caller would pass the same value for, and
// forgetting it would be a hydration mismatch rather than a type error. An
// ambient default that a host installs once is the shape that makes the
// correct call the shortest one.
//
// What this is *not* is a per-request store. A server rendering two requests
// concurrently shares this module, so a clock installed here is the process's
// and not the request's. That is the right granularity for what it is for —
// freezing time in a test, and giving a runtime a monotonic source — and the
// wrong granularity for "the instant this page was rendered at", which is why
// that value travels through the render itself in `@uniflowed/hooks/render`
// rather than through here.

/** A source of the current time, and of the zone it is reported in. */
export type Clock = {
  /** Milliseconds since the Unix epoch. */
  readonly now: () => number,
  /** The IANA name of the zone this clock reports in, for example `Asia/Tokyo`. */
  readonly timeZone: () => string,
};

/**
 * `Intl.DateTimeFormat`, narrowed to the one call this module makes.
 *
 * Flow's vendored `intl.js` types `resolvedOptions().timeZone` as optional and
 * declares no `Intl$DateTimeFormatOptions`, and a host with no ICU at all has
 * no `Intl` to ask. Declaring the shape here keeps both cases a branch rather
 * than a crash — the same move `@uniflowed/hooks/timing` makes for
 * `Intl.RelativeTimeFormat`.
 */
declare class ResolvedDateTimeFormat {
  constructor(locale?: void, options?: void): void;
  resolvedOptions(): { timeZone?: string, ... };
}

declare var Intl: {
  DateTimeFormat?: Class<ResolvedDateTimeFormat>,
  ...
};

/**
 * The zone uf falls back to when the host cannot name its own.
 *
 * UTC rather than a guess from the offset: an offset is not a zone — it does
 * not know when the offset changes — and a wrong zone name produces text that
 * is confidently wrong twice a year.
 */
export const UTC: string = "UTC";

/** Whatever the host says, which is what an application gets by default. */
export function systemClock(): Clock {
  return {
    now: () => Date.now(),
    timeZone: () => hostTimeZone(),
  };
}

/**
 * The host's own zone, or UTC where it has none.
 *
 * Read on every call rather than once at import: a Node process can be started
 * with `TZ` unset and have it set later, and a test that installs a zone would
 * otherwise be fighting a value captured before it ran.
 */
export function hostTimeZone(): string {
  const format = Intl.DateTimeFormat;
  if (format == null) {
    return UTC;
  }
  try {
    return new format().resolvedOptions().timeZone ?? UTC;
  } catch {
    // A host with a partial ICU build throws here rather than returning
    // nothing. Either way it cannot name its zone, and UTC is the answer that
    // is at least unambiguous.
    return UTC;
  }
}

/**
 * A clock stopped at one instant.
 *
 * This is what a server render and a test both want, for the same reason: the
 * value must not change between the first read and the last, or two components
 * on the same page disagree about what "today" is.
 */
export function fixedClock(epochMilliseconds: number, timeZone: string = UTC): Clock {
  return {
    now: () => epochMilliseconds,
    timeZone: () => timeZone,
  };
}

/** A clock and the two handles that move it. */
export type ManualClock = {
  readonly clock: Clock,
  /** Move forward by `millis`, which may be negative. */
  readonly advance: (millis: number) => void,
  /** Move to an exact instant. */
  readonly set: (epochMilliseconds: number) => void,
};

/**
 * A clock a test drives by hand.
 *
 * Separate from `fixedClock` because the two answer different questions. A
 * fixed clock proves that a value does not depend on when it was read; a manual
 * one proves that something *does* move when time does — that "in 5 minutes"
 * becomes "in 4 minutes" — without waiting five minutes to find out.
 */
export function manualClock(epochMilliseconds: number, timeZone: string = UTC): ManualClock {
  let at = epochMilliseconds;
  return {
    clock: { now: () => at, timeZone: () => timeZone },
    advance: (millis: number) => {
      at += millis;
    },
    set: (next: number) => {
      at = next;
    },
  };
}

/**
 * The clock installed for this process, if any.
 *
 * `null` rather than a system clock, so `currentClock` can tell "nobody chose"
 * from "somebody chose the host's" — the second is a decision a test may want
 * to make explicitly, and it must survive `setClock(null)` being called by
 * something else.
 */
let installed: Clock | null = null;

/** The default, built once: it holds no state, so there is no reason to rebuild it. */
const SYSTEM: Clock = systemClock();

/** The clock every uf package reads. The host's, unless something installed one. */
export function currentClock(): Clock {
  return installed ?? SYSTEM;
}

/**
 * Install `clock`, and return the call that puts back whatever was there.
 *
 * The undo is returned rather than left to the caller to reconstruct, because
 * the caller reconstructing it is how nested installs go wrong: two tests that
 * each restore "the system clock" instead of "what I found" leave the second
 * one's clock installed for everything that runs after them. `null` restores
 * the host's.
 */
export function setClock(clock: Clock | null): () => void {
  const previous = installed;
  installed = clock;
  return () => {
    installed = previous;
  };
}

/** Milliseconds since the epoch, from the installed clock. */
export function now(): number {
  return currentClock().now();
}

/** The zone the installed clock reports in. */
export function timeZone(): string {
  return currentClock().timeZone();
}
