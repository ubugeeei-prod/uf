// @flow
//
// The server-only half of the fixture: content the home page renders and the
// browser has no use for.
//
// A private directory (`_content`) rather than a route, and imported by
// `app/$page.js` alone. No client boundary is reachable from it, so
// `crates/uf_rsc` marks it `isolated` and the client route table has no
// `import()` that reaches it — which is what the build assertion in
// `crates/uf_cli/tests/vite.rs` looks for. The marker is a string literal
// rather than an identifier because a production bundle renames identifiers
// and keeps strings.

/** The string that proves this module is in a bundle. */
export const ALMANAC_MARKER: string = "almanac-marker-only-the-server-reads-this";

/** One dated note. */
export type Entry = {| readonly day: string, readonly note: string |};

const ENTRIES: $ReadOnlyArray<Entry> = [
  { day: "1582-10-15", note: "the first day of the Gregorian calendar" },
  { day: "1752-09-14", note: "the day Britain rejoined it, eleven days later" },
  { day: "1918-02-14", note: "the day Russia did, thirteen days later" },
  { day: "1928-01-01", note: "the day the last holdout in Europe did" },
];

/** Every note, oldest first. */
export function entries(): $ReadOnlyArray<Entry> {
  return ENTRIES;
}

/** One note, rendered as a line. */
export function line(entry: Entry): string {
  return `${entry.day} — ${entry.note}`;
}
