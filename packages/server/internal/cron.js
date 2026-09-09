// @flow
//
// A five-field cron expression, and whether a given minute matches it.
//
// The matching half of ubugeeei-prod/uf#531. `crates/uf_std`'s `parse_cron` is
// the other reading of the same string and stays where it is: it splits five
// fields for a build that is *emitting* a platform's own cron configuration,
// and never asks what they mean. This one has to know, because on a host that
// keeps a process uf runs the schedule itself.
//
// # The syntax, and only this syntax
//
// Per field: `*`, `n`, `a-b`, `*/n`, `a-b/n`, and a comma-separated list of
// those. No names (`JAN`, `MON`), no `@daily`, no seconds field, no `L`/`W`/`#`
// — every one of those is spelled differently by Vixie cron, by Quartz and by
// each cloud, and a schedule uf accepted and a platform read differently is
// the failure this feature has everywhere it exists. What is here is the
// intersection every one of them agrees on.
//
// # Which day wins
//
// When day-of-month and day-of-week are *both* restricted, a time matches when
// **either** does — not both. That is POSIX's rule and it surprises everybody
// once: `0 0 1 * 1` is "the first of the month, and every Monday", not "Mondays
// that fall on the first". When only one of the two is restricted it simply has
// to match, which is the reading people expect and the reason the surprise is
// rare enough to be worth a comment rather than a different syntax.
//
// # UTC, and said out loud
//
// Fields are matched against UTC. A deployment's local time is not something
// uf can know — the container has one zone, the platform's own scheduler has
// another, and a schedule that meant different minutes in the two would be
// worse than one that always means the same minute. Every cloud's cron
// configuration is UTC for the same reason.

import type { Instant } from "@uniflowed/core/temporal";

/** One parsed field: the set of values that match, or `null` for `*`. */
type Field = $ReadOnlySet<number> | null;

/** A parsed five-field expression. */
export type Cron = {|
  readonly minute: Field,
  readonly hour: Field,
  readonly dayOfMonth: Field,
  readonly month: Field,
  readonly dayOfWeek: Field,
  /** The expression as written, for an error message and for a manifest. */
  readonly source: string,
|};

/** What a field may hold, and what to call it when it does not. */
const FIELDS: $ReadOnlyArray<{|
  readonly name: string,
  readonly min: number,
  readonly max: number,
|}> = [
  { name: "minute", min: 0, max: 59 },
  { name: "hour", min: 0, max: 23 },
  { name: "day-of-month", min: 1, max: 31 },
  { name: "month", min: 1, max: 12 },
  { name: "day-of-week", min: 0, max: 7 },
];

/**
 * Parse `source`, or throw saying which field and why.
 *
 * Throws rather than returning `null`, because every caller is a
 * `defineSchedule` at module scope: an expression that does not parse is a
 * program that cannot run, and the sooner it says so the better.
 */
export function parseCron(source: string): Cron {
  const parts = source.trim().split(/\s+/);
  if (parts.length !== 5) {
    throw new SyntaxError(
      `a cron expression has five fields (minute hour day-of-month month day-of-week); ` +
        `${JSON.stringify(source)} has ${String(parts.length)}`,
    );
  }
  const fields = parts.map((part, index) => parseField(part, FIELDS[index], source));
  return {
    minute: fields[0],
    hour: fields[1],
    dayOfMonth: fields[2],
    month: fields[3],
    // Both spellings of Sunday collapse to 0 here, so `dayOfWeekOf` below has
    // one number to compare against rather than two.
    dayOfWeek: fields[4] == null ? null : new Set([...fields[4]].map((day) => day % 7)),
    source,
  };
}

/** Whether `instant` falls in a minute this expression names. */
export function cronMatches(cron: Cron, instant: Instant): boolean {
  const at = instant.toZonedDateTimeISO("UTC");
  if (!inField(cron.minute, at.minute)) return false;
  if (!inField(cron.hour, at.hour)) return false;
  if (!inField(cron.month, at.month)) return false;

  const day = inField(cron.dayOfMonth, at.day);
  const weekday = inField(cron.dayOfWeek, dayOfWeekOf(at.dayOfWeek));
  // The POSIX rule, and the whole reason these two are not just two more
  // `inField` calls above.
  if (cron.dayOfMonth != null && cron.dayOfWeek != null) {
    return day || weekday;
  }
  return day && weekday;
}

/** Cron's day-of-week from Temporal's, which counts from Monday. */
function dayOfWeekOf(isoDayOfWeek: number): number {
  // Temporal is 1 = Monday … 7 = Sunday; cron is 0 = Sunday … 6 = Saturday.
  return isoDayOfWeek % 7;
}

function inField(field: Field, value: number): boolean {
  return field == null || field.has(value);
}

function parseField(
  part: string,
  spec: {| readonly name: string, readonly min: number, readonly max: number |},
  source: string,
): Field {
  if (part === "*") return null;

  const values: Set<number> = new Set();
  for (const term of part.split(",")) {
    if (term === "") {
      throw fieldError(spec.name, part, source, "an empty term between commas");
    }
    const [range, step] = splitStep(term, spec, part, source);
    const [from, to] = splitRange(range, spec, part, source);
    for (let value = from; value <= to; value += step) {
      values.add(value);
    }
  }
  // Unreachable through the parsing above — every term adds at least `from` —
  // and checked anyway, because a field that matches nothing is a schedule
  // that silently never runs, which is the one outcome worse than an error.
  if (values.size === 0) {
    throw fieldError(spec.name, part, source, "it matches no value");
  }
  return values;
}

/** `a-b/n` into `["a-b", n]`, with `n` defaulting to one. */
function splitStep(
  term: string,
  spec: {| readonly name: string, readonly min: number, readonly max: number |},
  part: string,
  source: string,
): [string, number] {
  const at = term.indexOf("/");
  if (at === -1) return [term, 1];
  const step = Number(term.slice(at + 1));
  if (!Number.isInteger(step) || step < 1) {
    throw fieldError(spec.name, part, source, `a step must be a whole number above zero`);
  }
  const range = term.slice(0, at);
  // `5/15` is the one form that would otherwise be read here as `5` with the
  // step quietly dropped — an hourly-looking schedule that runs once a day.
  // Vixie cron reads it as `5-59/15`, and this module's whole rule is that it
  // accepts only what every cron agrees on; `n/step` is not on that list, so
  // it is refused with the spelling that is.
  if (range !== "*" && !range.includes("-")) {
    throw fieldError(
      spec.name,
      part,
      source,
      `a step needs \`*\` or a range before it — write \`${range}-${String(spec.max)}/${String(step)}\``,
    );
  }
  return [range, step];
}

/** `a-b` or `a` or `*` into the pair of numbers it covers. */
function splitRange(
  range: string,
  spec: {| readonly name: string, readonly min: number, readonly max: number |},
  part: string,
  source: string,
): [number, number] {
  if (range === "*") return [spec.min, spec.max];

  const at = range.indexOf("-");
  if (at === -1) {
    const only = numberIn(range, spec, part, source);
    return [only, only];
  }
  const from = numberIn(range.slice(0, at), spec, part, source);
  const to = numberIn(range.slice(at + 1), spec, part, source);
  if (to < from) {
    // No wrap-around. `22-2` is the kind of thing somebody writes meaning "late
    // at night" and every cron reads as empty; refusing is better than either
    // reading, because the two readings differ by twenty hours.
    throw fieldError(spec.name, part, source, `${String(from)}-${String(to)} counts backwards`);
  }
  return [from, to];
}

function numberIn(
  text: string,
  spec: {| readonly name: string, readonly min: number, readonly max: number |},
  part: string,
  source: string,
): number {
  const value = Number(text);
  if (text.trim() === "" || !Number.isInteger(value)) {
    throw fieldError(spec.name, part, source, `${JSON.stringify(text)} is not a whole number`);
  }
  if (value < spec.min || value > spec.max) {
    throw fieldError(
      spec.name,
      part,
      source,
      `${String(value)} is outside ${String(spec.min)}-${String(spec.max)}`,
    );
  }
  return value;
}

function fieldError(name: string, part: string, source: string, why: string): SyntaxError {
  return new SyntaxError(
    `the ${name} field ${JSON.stringify(part)} of cron ${JSON.stringify(source)}: ${why}`,
  );
}
