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

/** Frames belonging to the runner itself, which no test author wrote. */
const INTERNAL_MARKERS = [
  "/packages/test/internal/",
  "/packages/test/worker.js",
  "/@uniflowed/test/",
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
