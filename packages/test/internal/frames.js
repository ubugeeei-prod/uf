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

/**
 * A V8 call site, as much of it as [`callerSite`] reads.
 *
 * Written out rather than imported: there is no library definition for V8's
 * structured stack API, and these four methods are all it asks of one.
 */
type CallSite = interface {
  getFileName(): ?string,
  getLineNumber(): ?number,
  getColumnNumber(): ?number,
};

/** The piece of `node:module`'s source-map API [`callerSite`] needs. */
type SourceMapEntry = {|
  readonly originalLine?: number,
  readonly originalColumn?: number,
  readonly originalSource?: string,
|};
type FindSourceMap = (
  file: string,
) => ?interface { findEntry(line: number, column: number): SourceMapEntry };

/**
 * `node:module`'s `findSourceMap` when the host is Node with source maps on;
 * `false` when the structured path must not be taken; `undefined` until asked.
 *
 * Asked once per process: the host does not change under a running worker.
 */
let nodeSourceMaps: FindSourceMap | false | void;

/**
 * How to map a generated position the way Node's own stack traces do, or
 * `false` when this host's stacks cannot be reproduced from call sites.
 *
 * Node only. Bun and Deno implement the call-site API too, but whether their
 * call sites carry the generated position or the mapped one is theirs to
 * decide and has changed between releases, so they keep the string path,
 * whose answer is by construction the one their `.stack` prints. A browser
 * has no `process` at all.
 */
function sourceMapsForCallSites(): FindSourceMap | false {
  if (nodeSourceMaps !== undefined) {
    return nodeSourceMaps;
  }
  const host: $FlowFixMe = globalThis;
  const process = host.process;
  const isNode =
    typeof process?.versions?.node === "string" &&
    process.versions.bun == null &&
    host.Deno == null &&
    typeof process.getBuiltinModule === "function" &&
    typeof Error.captureStackTrace === "function";
  if (!isNode) {
    nodeSourceMaps = false;
    return false;
  }
  // A `uf test` worker whose loader maps lazily holds the maps Node would
  // have, and answers for them the way `findSourceMap` would; see
  // `@uniflowed/host`'s `internal/lazy-source-maps.js`.
  const lazy = host[Symbol.for("@uniflowed/host/source-maps")];
  if (typeof lazy?.findSourceMap === "function") {
    nodeSourceMaps = (file) => lazy.findSourceMap(file);
    return nodeSourceMaps;
  }
  const findSourceMap = process.getBuiltinModule("node:module")?.findSourceMap;
  nodeSourceMaps =
    typeof findSourceMap === "function"
      ? // Only while Node applies source maps to its own stacks: with them off
        // a `.stack` prints the generated position, and so must this.
        (file) => (process.sourceMapsEnabled === true ? findSourceMap(file) : null)
      : false;
  return nodeSourceMaps;
}

/**
 * The first position outside the runner on the stack of the call to `skip`,
 * as `firstUserSite(new Error().stack)` would read it — or `undefined` when
 * this host cannot answer that way, and the caller should build the string.
 *
 * # Why not simply read `.stack`
 *
 * Because it is the most expensive line in registering a test. `describe` and
 * `it` ask where they were called from, once per case, and on Node with
 * `--enable-source-maps` — which every `uf test` worker runs with — the string
 * `.stack` is built by mapping *every* frame through its module's source map
 * and printing each one, to read back one line and column from the first that
 * is not the runner's. On a suite of 50 files and 1,000 cases that was about a
 * sixth of a worker's CPU.
 *
 * V8 hands the same frames over unprinted to a `prepareStackTrace` installed
 * for the one capture, and only the frame the answer comes from is mapped,
 * with the lookup Node's printer itself uses — `findSourceMap(file)` then
 * `findEntry(line - 1, column - 1)`, falling back to the generated position
 * when there is no map or no entry — so the number is the one the string would
 * have carried. `packages/test/registration-site.test.js` holds the two paths
 * to that.
 */
export function callerSite(skip: (...args: $ReadOnlyArray<empty>) => mixed): Site | null | void {
  const findSourceMap = sourceMapsForCallSites();
  if (findSourceMap === false) {
    return undefined;
  }
  const errors: $FlowFixMe = Error;
  const limit: mixed = errors.stackTraceLimit;
  const full = typeof limit === "number" ? limit : 0;
  // A few frames first. The caller of a registration is two or three frames
  // above it — `it`, the modifier or `each` wrapper, `addCase` — and V8's cost
  // is per frame it materialises, so the whole default ten is walked only for
  // the rare caller that is deeper than that.
  const shallow = Math.min(full, SHALLOW_FRAMES);
  const first = readCallSites(skip, shallow);
  if (first === undefined) {
    return undefined;
  }
  const found = firstUserCallSite(first, findSourceMap);
  if (found != null || first.length < shallow || shallow === full) {
    return found;
  }
  const again = readCallSites(skip, full);
  return again === undefined ? undefined : firstUserCallSite(again, findSourceMap);
}

/** How many frames [`callerSite`] asks for before it asks for all of them. */
const SHALLOW_FRAMES = 4;

/** Hands V8's call sites back unprinted; one function, so none is made per capture. */
function unprinted(_error: mixed, sites: $ReadOnlyArray<CallSite>): $ReadOnlyArray<CallSite> {
  return sites;
}

/**
 * Up to `limit` call sites above `skip`, or `undefined` when the host does not
 * hand them over.
 */
function readCallSites(
  skip: (...args: $ReadOnlyArray<empty>) => mixed,
  limit: number,
): $ReadOnlyArray<CallSite> | void {
  const errors: $FlowFixMe = Error;
  const prepare = errors.prepareStackTrace;
  const before = errors.stackTraceLimit;
  const holder: $FlowFixMe = {};
  try {
    errors.prepareStackTrace = unprinted;
    errors.stackTraceLimit = limit;
    errors.captureStackTrace(holder, skip);
    // Read inside the `try`: V8 formats `stack` lazily, on first access, and
    // with whatever `prepareStackTrace` is installed *then*. What comes back is
    // what `unprinted` returned, which V8 does not type.
    const sites: $FlowFixMe = holder.stack;
    return Array.isArray(sites) ? sites : undefined;
  } finally {
    errors.prepareStackTrace = prepare;
    errors.stackTraceLimit = before;
  }
}

/** The first of `sites` outside the runner, mapped as Node maps a printed frame. */
function firstUserCallSite(
  sites: $ReadOnlyArray<CallSite>,
  findSourceMap: FindSourceMap,
): Site | null {
  for (const site of sites) {
    const file = site.getFileName();
    if (file == null || isInternalFrame(file)) {
      continue;
    }
    const line = site.getLineNumber();
    const column = site.getColumnNumber();
    if (line == null || column == null) {
      continue;
    }
    const entry = findSourceMap(file)?.findEntry(line - 1, column - 1);
    if (
      entry?.originalSource != null &&
      entry.originalSource !== "" &&
      entry.originalLine != null &&
      entry.originalColumn != null
    ) {
      return { line: entry.originalLine + 1, column: entry.originalColumn + 1 };
    }
    return { line, column };
  }
  return null;
}
