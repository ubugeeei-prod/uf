// @flow
//
// Reading positions out of a stack trace.
//
// Two things need this: registration, which records where `it(` was written,
// and failure reporting, which records where the assertion was. Both want the
// same answer — the first frame that belongs to the person's code — so both
// ask here.
//
// The parsing is by hand rather than by regular expression. A frame's tail is
// `…:<line>:<column>` with an optional `)`, and scanning backwards for that is
// shorter to read than the pattern that matches it, exact about what it
// accepts, and cannot be surprised by a path containing something
// regex-shaped.

/** A position in a source file, one-based line and column. */
export type Site = {| readonly line: number, readonly column: number |};

/**
 * Frames belonging to the testing libraries, which no test author wrote.
 *
 * `@uniflowed/react-testing` is here for exactly the reason `@uniflowed/test`
 * is, and was missing because it is a different package. Every `getBy…`
 * failure is *constructed* inside it, so the first surviving frame of the
 * commonest failure a component test can produce was `internal/queries.js` —
 * and since a site is reported as a line of the file being run, the reader was
 * sent to a line their own file does not have. That is ubugeeei-prod/uf#319.
 * A library that raises on the caller's behalf is the runner as far as the
 * report is concerned.
 */
const INTERNAL_MARKERS = [
  "/packages/test/internal/",
  "/packages/test/worker.js",
  "/packages/react-testing/",
  "/@uniflowed/test/",
  "/@uniflowed/react-testing/",
  "node:internal/",
];

/** Whether `frame` is the runner's own rather than the caller's. */
export function isInternalFrame(frame: string): boolean {
  return INTERNAL_MARKERS.some((marker) => frame.includes(marker));
}

/**
 * The `:line:column` a stack frame ends with, or `null`.
 *
 * A frame ends either `…:12:34` or `…:12:34)`. Anything else — a native
 * frame, a bare function name — has no position, and saying so is better than
 * inventing line one.
 */
export function frameSite(frame: string): Site | null {
  let end = frame.length;
  while (end > 0 && (frame[end - 1] === " " || frame[end - 1] === ")")) {
    end -= 1;
  }

  const column = digitsBefore(frame, end);
  if (column == null || column.start === 0 || frame[column.start - 1] !== ":") {
    return null;
  }
  const line = digitsBefore(frame, column.start - 1);
  if (line == null || line.start === 0 || frame[line.start - 1] !== ":") {
    return null;
  }
  return { line: line.value, column: column.value };
}

/**
 * The file a stack frame names, or `null`.
 *
 * The same scan as [`frameSite`], stopping one step earlier: everything before
 * the `:line:column` is where the code is, and what that is depends on how V8
 * wrote the frame — `at name (/path:1:2)` when it has a function name, and
 * `at /path:1:2` or `at async file:///path:1:2` when it does not.
 *
 * What comes back is whatever the frame said, a path or a URL, because those
 * are the two things it can be and a caller resolving a module specifier
 * against it has to tell them apart anyway.
 */
export function frameFile(frame: string): string | null {
  let end = frame.length;
  while (end > 0 && (frame[end - 1] === " " || frame[end - 1] === ")")) {
    end -= 1;
  }

  const column = digitsBefore(frame, end);
  if (column == null || column.start === 0 || frame[column.start - 1] !== ":") {
    return null;
  }
  const line = digitsBefore(frame, column.start - 1);
  if (line == null || line.start === 0 || frame[line.start - 1] !== ":") {
    return null;
  }

  let text = frame.slice(0, line.start - 1);
  const open = text.lastIndexOf("(");
  if (open !== -1) {
    text = text.slice(open + 1);
  } else {
    const at = text.lastIndexOf(" at ");
    text = at === -1 ? text : text.slice(at + 4);
  }
  text = text.trim();
  // `at async /path:1:2` — the marker belongs to the frame, not to the file.
  if (text.startsWith("async ")) {
    text = text.slice("async ".length).trim();
  }
  return text === "" ? null : text;
}

/** The run of digits ending at `end`, with where it starts. */
function digitsBefore(
  text: string,
  end: number,
): {| readonly value: number, readonly start: number |} | null {
  let start = end;
  while (start > 0 && text[start - 1] >= "0" && text[start - 1] <= "9") {
    start -= 1;
  }
  if (start === end) {
    return null;
  }
  return { value: Number(text.slice(start, end)), start };
}

/**
 * The first position in `stack` outside the runner, or `null`.
 *
 * `skipInternal` is false when the caller has already trimmed the runner's own
 * frames and wants the first frame whatever it is.
 */
export function firstUserSite(
  stack: string | null | void,
  skipInternal: boolean = true,
): Site | null {
  if (stack == null) {
    return null;
  }
  for (const frame of stack.split("\n").slice(1)) {
    if (skipInternal && isInternalFrame(frame)) {
      continue;
    }
    const site = frameSite(frame);
    if (site != null) {
      return site;
    }
  }
  return null;
}

/**
 * The first position in `stack` that is in `file`, or `null`.
 *
 * The reporter draws a failure's position as `path:line:column`, and it takes
 * the path from the file it asked the worker to run rather than from the
 * frame. So a number lifted from any other file is not a vaguer answer than
 * none — it is a wrong one, naming a line the reader can open and that has
 * nothing to do with what failed. Restricting the search to the file that will
 * be named makes the two halves of that position come from the same place.
 *
 * `null` when the file is on no frame, which is a failure raised from work the
 * case left behind: no line of it is the one that failed, and printing none is
 * the honest answer.
 */
export function siteInFile(stack: string | null | void, file: string): Site | null {
  if (stack == null) {
    return null;
  }
  for (const frame of stack.split("\n").slice(1)) {
    if (!frame.includes(file)) {
      continue;
    }
    const site = frameSite(frame);
    if (site != null) {
      return site;
    }
  }
  return null;
}

/**
 * `stack` with the runner's own frames removed.
 *
 * A stack that starts inside the matcher buries the one line that matters
 * under eight that never do. The message stays as the first line, so the
 * result still reads as a trace.
 */
export function userFrames(stack: string | null | void): string | null {
  if (stack == null) {
    return null;
  }
  const lines = stack.split("\n");
  const head = lines[0] ?? "";
  const frames = lines.slice(1).filter((frame) => !isInternalFrame(frame));
  return frames.length === 0 ? head : [head, ...frames].join("\n");
}
