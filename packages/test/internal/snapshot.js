// @flow
//
// Internal to `@uniflowed/test`: `toMatchSnapshot` and its inline sibling.
//
// A snapshot is an assertion whose expected value was written by the last run
// rather than by a person. That is the whole idea and also the whole danger:
// a snapshot nobody reads is a test that asserts whatever the code did, which
// is not a test. Two things here take that seriously.
//
// **A missing snapshot is written; a different one is a failure.** Never
// "updated because it changed" — that is how a snapshot suite becomes a diff
// nobody looks at. Rewriting on mismatch happens only when a run was explicitly
// asked to, through `UF_UPDATE_SNAPSHOTS`, which `uf test -u` sets.
//
// **The diff is in the failure.** A snapshot mismatch that says only "snapshot
// did not match" makes a reader open two files; the whole expected and received
// text is in the message, because that is what they were going to look at.
//
// # Where they live
//
// `__snapshots__/<file>.snap` beside the test file, one file per test file, in
// the format Jest and Vitest both write: a module of `exports[key] = ...`. Not
// because uf runs either, but because the format is diffable, the tooling that
// reads it already exists, and inventing a different one would buy nothing.

import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

import { render } from "./equality.js";

/**
 * A backtick, by code point.
 *
 * Written this way rather than as a literal because this module is scanned by
 * `uf_lib`'s surface tests, whose tokenizer does not model regex literals and
 * reads a stray backtick as the start of a template. One constant is cheaper
 * than teaching that scanner about regular expressions, and it is used often
 * enough here to earn a name.
 */
const BACKTICK = String.fromCharCode(96);

/** Which test is running, so a snapshot can be keyed by it. */
type Current = {
  /** Absolute path of the test file. */
  readonly file: string,
  /** The test's full name, suites included. */
  readonly name: string,
  /** How many snapshots this test has taken, so a second one gets `2`. */
  taken: number,
};

let current: Current | null = null;

/** Snapshots read from disk, by snapshot file, and whether they changed. */
const loaded: Map<string, { entries: { [string]: string }, dirty: boolean }> = new Map();

/** Whether this run may rewrite a snapshot that did not match. */
function updating(): boolean {
  const value = (globalThis: $FlowFixMe).process?.env?.UF_UPDATE_SNAPSHOTS;
  return value != null && value !== "" && value !== "0";
}

/**
 * Say which test is running.
 *
 * Called by the runner around each case. `null` between them, so a snapshot
 * taken outside a test fails with something better than a wrong key.
 */
export function enterTest(file: string, name: string): void {
  current = { file, name, taken: 0 };
}

/** Say that no test is running. */
export function exitTest(): void {
  current = null;
}

/** Raised when a snapshot is taken where it cannot be keyed or stored. */
export class SnapshotError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "SnapshotError";
  }
}

/** The snapshot file for a test file. */
function snapshotPath(file: string): string {
  return path.join(path.dirname(file), "__snapshots__", `${path.basename(file)}.snap`);
}

/**
 * Read a snapshot file, or start an empty one.
 *
 * Parsed rather than imported: the file is data, and importing it would run it
 * — which is a fine way to execute whatever a snapshot happens to contain.
 */
function entriesFor(file: string): { entries: { [string]: string }, dirty: boolean } {
  const target = snapshotPath(file);
  const already = loaded.get(target);
  if (already != null) {
    return already;
  }

  const state = { entries: (Object.create(null): $FlowFixMe), dirty: false };
  if (existsSync(target)) {
    parseInto(readFileSync(target, "utf8"), state.entries);
  }
  loaded.set(target, state);
  return state;
}

/** The literal text an entry opens with, before its key. */
const ENTRY_OPEN = "exports[" + BACKTICK;

/** The literal text between an entry's key and its value. */
const ENTRY_MIDDLE = BACKTICK + "] = " + BACKTICK;

/**
 * Read `exports[<key>] = <value>;` entries out of a snapshot file.
 *
 * A scanner rather than a regular expression, for two reasons. A snapshot's
 * value holds newlines and backticks of its own, so the terminator has to be
 * found by walking the escapes rather than by matching a pattern. And a regular
 * expression for this would have to contain a backtick, which is the one
 * character `code_only` in `uf_lib`'s surface tests cannot see past — the
 * scanner there does not model regex literals, and a backtick inside one reads
 * as the start of a template.
 */
function parseInto(source: string, into: { [string]: string }): void {
  let at = 0;
  for (;;) {
    const open = source.indexOf(ENTRY_OPEN, at);
    if (open < 0) {
      break;
    }
    const keyFrom = open + ENTRY_OPEN.length;
    const keyEnd = findClose(source, keyFrom);
    if (keyEnd < 0) {
      break;
    }
    if (!source.startsWith(ENTRY_MIDDLE, keyEnd)) {
      at = keyFrom;
      continue;
    }
    const valueFrom = keyEnd + ENTRY_MIDDLE.length;
    const valueEnd = findClose(source, valueFrom);
    if (valueEnd < 0) {
      break;
    }
    into[unescape(source.slice(keyFrom, keyEnd))] = unescape(source.slice(valueFrom, valueEnd));
    at = valueEnd + 1;
  }
}

/** The index of the unescaped backtick closing a value that starts at `from`. */
function findClose(source: string, from: number): number {
  for (let at = from; at < source.length; at += 1) {
    if (source[at] === "\\") {
      at += 1;
      continue;
    }
    if (source[at] === "`") {
      return at;
    }
  }
  return -1;
}

/** Undo what `escapeValue` did. */
function unescape(value: string): string {
  let out = "";
  for (let at = 0; at < value.length; at += 1) {
    if (value[at] === "\\" && at + 1 < value.length) {
      at += 1;
    }
    out += value[at];
  }
  return out;
}

/**
 * Make a value safe inside a template literal.
 *
 * A backtick would end it, a backslash would eat the next character, and `${`
 * would start a substitution that runs code — which matters because a snapshot
 * file is written by a test and read by whatever opens it next.
 */
function escapeValue(value: string): string {
  let out = "";
  for (let at = 0; at < value.length; at += 1) {
    const char = value[at];
    if (char === "\\" || char === BACKTICK) {
      out += "\\";
    } else if (char === "$" && value[at + 1] === "{") {
      out += "\\";
    }
    out += char;
  }
  return out;
}

/** Write a snapshot file back, in key order so a diff is readable. */
function flush(target: string, entries: { [string]: string }): void {
  const keys = Object.keys(entries).sort();
  const body = keys
    .map((key) => `exports[\`${escapeValue(key)}\`] = \`${escapeValue(entries[key])}\`;\n`)
    .join("\n");
  mkdirSync(path.dirname(target), { recursive: true });
  writeFileSync(
    target,
    `// uf snapshot file. Read the diff — a snapshot nobody reads is not a test.\n\n${body}`,
    "utf8",
  );
}

/** Write every snapshot file this run changed. */
export function writeChangedSnapshots(): void {
  for (const [target, state] of loaded) {
    if (state.dirty) {
      flush(target, state.entries);
      state.dirty = false;
    }
  }
}

/** What a snapshot comparison decided. */
export type SnapshotVerdict = {
  readonly pass: boolean,
  /** The stored snapshot, or `null` when there was none. */
  readonly expected: string | null,
  /** What this run produced. */
  readonly received: string,
  /** Whether the file was written, and why. */
  readonly wrote: "created" | "updated" | null,
};

/**
 * Compare `value` against the stored snapshot for the running test.
 *
 * A missing snapshot is written and passes — the first run of a new assertion
 * has nothing to compare against, and failing it would mean every new snapshot
 * test fails once by design. A *different* one fails unless the run was asked
 * to update.
 */
export function matchSnapshot(value: mixed, hint?: string): SnapshotVerdict {
  if (current == null) {
    throw new SnapshotError(
      "toMatchSnapshot was called outside a test, so there is nothing to key it by",
    );
  }
  current.taken += 1;
  const suffix = hint != null && hint !== "" ? `: ${hint}` : "";
  const key = `${current.name}${suffix} ${current.taken}`;
  const state = entriesFor(current.file);
  const received = render(value);

  if (!Object.hasOwn(state.entries, key)) {
    state.entries[key] = received;
    state.dirty = true;
    return { pass: true, expected: null, received, wrote: "created" };
  }

  const expected = state.entries[key];
  if (expected === received) {
    return { pass: true, expected, received, wrote: null };
  }
  if (updating()) {
    state.entries[key] = received;
    state.dirty = true;
    return { pass: true, expected, received, wrote: "updated" };
  }
  return { pass: false, expected, received, wrote: null };
}

/**
 * Compare `value` against a snapshot written in the test file itself.
 *
 * Nothing is written back: uf does not rewrite a test file, because a tool that
 * edits the file you are editing is a tool that loses work. A missing inline
 * snapshot reports what to paste in, which is the same information with the
 * decision left to a person.
 */
export function matchInlineSnapshot(value: mixed, expected?: string): SnapshotVerdict {
  const received = render(value);
  if (expected == null) {
    return { pass: false, expected: null, received, wrote: null };
  }
  // The stored form is indented to sit inside the call, so both sides are
  // compared with that indentation removed.
  return {
    pass: dedent(expected) === dedent(received),
    expected,
    received,
    wrote: null,
  };
}

/** Strip the common leading whitespace, so indentation is not the assertion. */
export function dedent(value: string): string {
  const lines = value.replace(/^\n/, "").replace(/\s+$/, "").split("\n");
  const indents = lines
    .filter((line) => line.trim() !== "")
    .map((line) => line.length - line.trimStart().length);
  const common = indents.length === 0 ? 0 : Math.min(...indents);
  return lines.map((line) => line.slice(common)).join("\n");
}
