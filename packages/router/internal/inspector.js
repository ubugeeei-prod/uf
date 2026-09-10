// @flow
//
// Internal to `@uniflowed/router`: what left, in what order, and what each
// chunk built.
//
// ubugeeei-prod/uf#520 asks for two things. The first — which DOM subtree each
// of the three boundaries owns — is `./boundaries.js` for Suspense and error
// and ubugeeei-prod/uf#636 for client/server. The second is *an inspector for
// the payload*: what arrived, in what order, and which part of the tree each
// chunk built. This is that one.
//
// # There is no Flight payload, and this is not pretending there is
//
// uf does not have one. `../client.js` hydrates by re-rendering the matched
// tree from the same modules the server rendered it from, `internal/rsc.js`
// says so from the other side, and ubugeeei-prod/uf#519 is where the payload
// is being worked out. Nothing here reads one, invents one, or decides
// anything about its shape.
//
// What uf *does* stream, today, on every `uf dev` request, is a document whose
// Suspense boundaries resolve independently — the shell goes out with the
// fallbacks in it and each boundary's content follows in its own chunk when it
// is ready. The three questions the issue asks are questions about that
// stream, they are unanswered today, and they are answerable from the bytes
// that are already going out. So they are answered about the stream that
// exists rather than about the payload that does not, and when the payload
// lands this is the recorder it is fed to: `inspectStream` takes chunks and
// knows nothing about who produced them.
//
// # It reads React's own markers, and no others
//
// Fizz writes a suspended boundary into the shell as an empty
// `<template id="B:0">` followed by the fallback and closed by a `<!--/$-->`
// comment, and completes it later with the content inside a hidden element
// carrying `id="S:0"` and a `$RC("B:0","S:0")` call that pairs the two. Both
// halves name the boundary, so a chunk can be attributed to the boundary it
// filled without uf marking anything, without a second render, and without any
// agreement with the client.
//
// Reading them is reading React's markup, which is the same licence `hoisted`
// takes in `./stream.js` and rests on the same fact: React escapes `>` in an
// attribute value and `<` in text, so the first `>` after an opening tag ends
// it. This is not an HTML parser and must never be handed markup from anywhere
// else. It is also written as hand-rolled `indexOf` scans rather than regular
// expressions — `docs/security.md`'s rule for text uf did not write, and the
// content of these chunks is the application's own — with a step limit on every
// loop for the same reason `./boundaries.js` has a `WALK_LIMIT`.
//
// A marker that a chunk boundary happened to split is not attributed rather
// than guessed at: the chunk is still counted, still timed, and says so. The
// alternative is a report that is confidently wrong about which part of the
// tree arrived, which is worse than one that is quiet about it.
//
// # Where the report goes, and how loud it is
//
// The terminal, on the same `diagnostic` channel `./boundaries.js` and
// `./hydration.js` use — ubugeeei-prod/uf#583's argument, made again by
// ubugeeei-prod/uf#636: a diagnostic that exists only in a browser window has
// to be noticed by somebody who does not know to look. Unlike those two it is
// produced on the server, because that is where the chunks are; `uf dev` hands
// `renderDocument` a reporter and nothing else does, so a built application
// neither reports nor records.
//
// It speaks the first time a path streams and whenever the *shape* of its
// stream changes, and is silent otherwise. The first time is the interesting
// one: you have just written the `$loading.js`, or the `await` that made the
// page suspend, and "it streamed, in three parts" is the answer to the question
// you asked by writing it. Reloading the same page unchanged is not that
// question, and `./boundaries.js` is right that a map printed on every reload
// is a banner nobody reads.
//
// The shape deliberately excludes the timings. A boundary that got slower has
// not changed shape, and putting the milliseconds in the signature would make
// this fire on every reload — which is the same trade `./boundaries.js` makes
// by leaving what a boundary owns out of its own signature.
//
// # A document that did not stream says nothing at all
//
// Most do not. A page with no suspended boundary is one chunk, there is no
// order to report and no part of the tree that arrived separately, and the
// honest report is silence. That is also what keeps this from being a request
// log: it fires for the pages the feature is about and for no others.

/** How many chunks of one document are recorded before the rest are counted. */
const CHUNK_LIMIT = 64;

/** How far a scan will walk through one chunk's markup before giving up. */
const SCAN_LIMIT = 4096;

/** How many elements one boundary's line names before it counts them. */
const NAMED_LIMIT = 3;

/** The longest prefix of a chunk the boundary labels are read out of. */
const LABEL_SCAN_BYTES = 262144;

/** How many characters of a label survive into a line of the report. */
const LABEL_LIMIT = 60;

/** How many paths the "has this changed" memory keeps. */
const MEMORY_LIMIT = 64;

/** What Fizz opens a suspended boundary with; the id follows. */
const OPEN_MARKER = '<template id="B:';

/** What closes the fallback of a suspended boundary. */
const CLOSE_MARKER = "<!--/$-->";

/** What Fizz calls to swap a completed boundary in; the two ids follow. */
const COMPLETE_MARKER = '$RC("';

/**
 * One boundary, as the chunk that completed it named it.
 *
 * `id` is React's — `"B:0"` — and not one of `./boundaries.js`'s. The two
 * vocabularies are not joinable: React numbers boundaries as it meets them
 * while rendering, `suspenseId` numbers a route's `$loading.js` entries, and
 * a `<Suspense>` a component wrote itself is in the first sequence and not the
 * second. Printing React's id and refusing to translate it is the honest
 * option; the two labels below are what actually identifies the boundary for a
 * reader, and both come off the page rather than out of a table.
 *
 * `fallback` is what the shell showed in its place and `built` what the
 * completion put there, both as the short selectors `./boundaries.js` names an
 * element with. `fallback` is `null` for a boundary whose opening marker this
 * never saw — one past [`LABEL_SCAN_BYTES`], or split across two chunks.
 */
export type StreamedBoundary = {|
  readonly id: string,
  readonly fallback: ?string,
  readonly built: $ReadOnlyArray<string>,
  readonly more: number,
|};

/** One chunk of one document, as it went out. */
export type StreamedChunk = {|
  /** Its place in the order, from one. */
  readonly index: number,
  /** Milliseconds after the first chunk. Zero for the first. */
  readonly at: number,
  /** Its size in bytes of UTF-8, which is what the socket carried. */
  readonly bytes: number,
  /** The boundaries it completed, in the order it completed them. */
  readonly boundaries: $ReadOnlyArray<StreamedBoundary>,
|};

/** One document, as a stream. */
export type StreamRecord = {|
  readonly chunks: $ReadOnlyArray<StreamedChunk>,
  /** Chunks past [`CHUNK_LIMIT`], which are counted and not described. */
  readonly more: number,
  readonly bytes: number,
|};

/** A recorder, over one document. */
export type StreamRecorder = {|
  readonly chunk: (text: string) => void,
  readonly finish: () => StreamRecord,
|};

/** What the reporter hands `uf dev` to print. */
export type StreamDiagnostic = {|
  readonly message: string,
  readonly detail: $ReadOnlyArray<string>,
|};

/**
 * A recorder for one document.
 *
 * `now` is a parameter rather than a reach for `performance.now`, so a test can
 * assert the numbers in the report instead of asserting that there are some.
 * The same reason `boundaryFindings` takes a document.
 */
export function inspectStream(now: () => number): StreamRecorder {
  const chunks: Array<StreamedChunk> = [];
  const labels: Map<string, string> = new Map();
  const encoder = new TextEncoder();
  let started: number | null = null;
  let count = 0;
  let bytes = 0;

  return {
    chunk(text: string): void {
      const at = now();
      // Read into a local before the branch below can assign it, so the
      // subtraction is over a `number` rather than over a binding the checker
      // still has to consider null.
      const first = started;
      if (first == null) {
        started = at;
        // The shell, and the only chunk the fallbacks are read out of: it is
        // where React writes every boundary it has suspended, and a scan that
        // continued into the completions would be scanning the page.
        readFallbacks(text, labels);
      }
      count += 1;
      const size = encoder.encode(text).length;
      bytes += size;
      if (chunks.length >= CHUNK_LIMIT) {
        return;
      }
      chunks.push({
        index: count,
        at: first == null ? 0 : Math.max(0, Math.round(at - first)),
        bytes: size,
        boundaries: completedIn(text, labels),
      });
    },
    finish(): StreamRecord {
      return { chunks, more: Math.max(0, count - chunks.length), bytes };
    },
  };
}

/**
 * `chunks`, unchanged, with `report` told what went past.
 *
 * A pass-through generator rather than a hook inside `./stream.js`'s assembly,
 * because what this is about is the bytes that actually left — after the head
 * has been put in and after `transformHead` has had the opening chunk. A
 * recorder further up would be describing a document nobody received.
 *
 * `finally`, so a render that was abandoned still reports what it managed to
 * send. A consumer that gives up is one of the things worth seeing.
 */
export async function* inspected(
  chunks: AsyncGenerator<string, void, void>,
  report: (record: StreamRecord) => void,
  now: () => number,
): AsyncGenerator<string, void, void> {
  const recorder = inspectStream(now);
  try {
    for await (const chunk of chunks) {
      recorder.chunk(chunk);
      yield chunk;
    }
  } finally {
    report(recorder.finish());
  }
}

/**
 * The fallback each suspended boundary in the shell is showing.
 *
 * Fills `labels` in place, keyed by React's boundary id. Bounded twice — by
 * [`LABEL_SCAN_BYTES`] of the chunk and by [`SCAN_LIMIT`] boundaries — because
 * the thing being scanned is a page.
 */
function readFallbacks(chunk: string, labels: Map<string, string>): void {
  const text = chunk.length > LABEL_SCAN_BYTES ? chunk.slice(0, LABEL_SCAN_BYTES) : chunk;
  let from = 0;
  for (let step = 0; step < SCAN_LIMIT; step += 1) {
    const open = text.indexOf(OPEN_MARKER, from);
    if (open === -1) {
      return;
    }
    const idAt = open + OPEN_MARKER.length;
    const idEnd = text.indexOf('"', idAt);
    if (idEnd === -1) {
      return;
    }
    // The fallback begins after the empty `<template>` React opened with and
    // ends at the comment that closes the boundary. A boundary nested inside
    // this one closes first, so the run taken here can be short — which costs a
    // less specific label and never a wrong one, since the label is only ever
    // the first element of it.
    const tagEnd = text.indexOf(">", idEnd);
    if (tagEnd === -1) {
      return;
    }
    const close = text.indexOf(CLOSE_MARKER, tagEnd);
    const fallback = text.slice(tagEnd + 1, close === -1 ? text.length : close);
    const named = firstElement(fallback);
    if (named != null) {
      labels.set(`B:${text.slice(idAt, idEnd)}`, named);
    }
    from = close === -1 ? tagEnd + 1 : close + CLOSE_MARKER.length;
  }
}

/**
 * The boundaries `chunk` completed, in the order it completed them.
 *
 * Driven by `$RC("B:0","S:0")` rather than by the hidden element, because that
 * call is React's own statement that the two are the same boundary — and
 * because it is the last thing written for a completion, so a chunk that
 * contains it contains the content too.
 */
function completedIn(chunk: string, labels: Map<string, string>): $ReadOnlyArray<StreamedBoundary> {
  const found: Array<StreamedBoundary> = [];
  let from = 0;
  for (let step = 0; step < SCAN_LIMIT; step += 1) {
    const call = chunk.indexOf(COMPLETE_MARKER, from);
    if (call === -1) {
      return found;
    }
    from = call + COMPLETE_MARKER.length;
    const ids = argumentPair(chunk, from);
    if (ids == null) {
      continue;
    }
    const built = builtBy(chunk, ids.content);
    found.push({
      id: ids.boundary,
      fallback: labels.get(ids.boundary) ?? null,
      built: built.slice(0, NAMED_LIMIT),
      more: Math.max(0, built.length - NAMED_LIMIT),
    });
  }
  return found;
}

/** The two quoted ids of a `$RC(…)` call whose first quote has been passed. */
function argumentPair(
  chunk: string,
  from: number,
): {| readonly boundary: string, readonly content: string |} | null {
  const boundaryEnd = chunk.indexOf('"', from);
  if (boundaryEnd === -1 || !chunk.startsWith(',"', boundaryEnd + 1)) {
    return null;
  }
  const contentAt = boundaryEnd + 3;
  const contentEnd = chunk.indexOf('"', contentAt);
  if (contentEnd === -1) {
    return null;
  }
  return {
    boundary: chunk.slice(from, boundaryEnd),
    content: chunk.slice(contentAt, contentEnd),
  };
}

/**
 * The elements a completion put on the page, as short selectors.
 *
 * The content arrives inside a hidden element carrying the id `$RC`'s second
 * argument names, and what the boundary owns afterwards is that element's own
 * children — so this finds the element by its id, walks past its opening tag,
 * and collects the tags that sit at depth zero under it.
 */
function builtBy(chunk: string, contentId: string): $ReadOnlyArray<string> {
  const marker = `id="${contentId}"`;
  const at = chunk.indexOf(marker);
  if (at === -1) {
    return [];
  }
  const tagEnd = chunk.indexOf(">", at + marker.length);
  if (tagEnd === -1) {
    return [];
  }
  return topLevelElements(chunk, tagEnd + 1);
}

/**
 * The elements at depth zero of the run beginning at `from`, until it closes.
 *
 * A depth walk rather than "every tag", because a boundary that built one
 * `<article>` with forty `<p>` in it built one thing, and naming the forty
 * would say less than naming the one. Void elements are self-closed by React,
 * which is what makes a single counter enough here — see the header.
 */
function topLevelElements(markup: string, from: number): $ReadOnlyArray<string> {
  const found: Array<string> = [];
  let depth = 0;
  let index = from;
  for (let step = 0; step < SCAN_LIMIT; step += 1) {
    const open = markup.indexOf("<", index);
    if (open === -1) {
      return found;
    }
    if (markup.startsWith("<!--", open)) {
      const end = markup.indexOf("-->", open);
      if (end === -1) {
        return found;
      }
      index = end + 3;
      continue;
    }
    const end = markup.indexOf(">", open);
    if (end === -1) {
      return found;
    }
    index = end + 1;
    if (markup.startsWith("</", open)) {
      // The close of the element the walk began inside: the run is over.
      if (depth === 0) {
        return found;
      }
      depth -= 1;
      continue;
    }
    if (depth === 0) {
      found.push(describeTag(markup.slice(open, end + 1)));
    }
    if (markup[end - 1] !== "/") {
      depth += 1;
    }
  }
  return found;
}

/**
 * One opening tag, short enough to sit in a line of a report.
 *
 * The same three rules `describeElement` in `./boundaries.js` applies to a live
 * element — the id if it has one, otherwise the first class, otherwise the tag
 * — because the two reports are about the same page and a reader should not
 * have to learn that `article.post` there and `article.post` here mean the same
 * thing. They cannot share an implementation: that one is handed a DOM node and
 * this one a string of markup, in a module that has no DOM. `inspector.test.js`
 * holds the two spellings against each other over the same elements.
 */
export function describeTag(tag: string): string {
  const space = tag.search(/[\s/>]/);
  const name = tag.slice(1, space === -1 ? tag.length : space).toLowerCase();
  const id = attribute(tag, "id");
  if (id != null && id !== "") {
    return `${name}#${id}`;
  }
  const className = attribute(tag, "class");
  const first = className == null ? "" : className.trim().split(/\s+/)[0];
  return first === "" ? name : `${name}.${first}`;
}

/** One double-quoted attribute of an opening tag, which is how React writes them. */
function attribute(tag: string, name: string): ?string {
  const marker = ` ${name}="`;
  const at = tag.indexOf(marker);
  if (at === -1) {
    return null;
  }
  const from = at + marker.length;
  const end = tag.indexOf('"', from);
  return end === -1 ? null : tag.slice(from, end);
}

/** The first element of a run of markup, as a short selector. */
function firstElement(markup: string): ?string {
  for (let index = 0, step = 0; step < SCAN_LIMIT; step += 1) {
    const open = markup.indexOf("<", index);
    if (open === -1) {
      return null;
    }
    if (markup.startsWith("<!--", open)) {
      const end = markup.indexOf("-->", open);
      if (end === -1) {
        return null;
      }
      index = end + 3;
      continue;
    }
    const end = markup.indexOf(">", open);
    if (end === -1) {
      return null;
    }
    if (markup.startsWith("</", open)) {
      index = end + 1;
      continue;
    }
    return describeTag(markup.slice(open, end + 1));
  }
  return null;
}

/**
 * Whether this document streamed at all.
 *
 * One chunk is a document that was finished before it was sent, and there is
 * nothing about order or arrival to say about it. See the header.
 */
export function didStream(record: StreamRecord): boolean {
  return record.chunks.some((chunk) => chunk.boundaries.length > 0);
}

/**
 * The stream as one comparable string.
 *
 * Which boundaries completed, in which chunk, and what each replaced with what
 * — everything a reader would notice — and not the timings or the byte counts,
 * which differ on every request. See the header.
 */
export function streamSignature(record: StreamRecord): string {
  const parts: Array<string> = [];
  for (const chunk of record.chunks) {
    for (const boundary of chunk.boundaries) {
      parts.push(
        `${chunk.index}:${boundary.id}:${boundary.fallback ?? ""}>${boundary.built.join(",")}`,
      );
    }
  }
  return parts.join("|");
}

/**
 * The report, as the terminal will print it.
 *
 * Separated from the sending so a test can pin the wording, which is the part
 * worth pinning: somebody reading this in a terminal has to be able to act on
 * it without opening this file.
 */
export function formatStream(path: string, record: StreamRecord): StreamDiagnostic {
  const described = record.chunks.filter((chunk) => chunk.boundaries.length > 0);
  const count = described.reduce((total, chunk) => total + chunk.boundaries.length, 0);
  const last = described[described.length - 1];
  const total = record.chunks.length + record.more;
  const detail: Array<string> = [
    `the shell went out in ${size(record.chunks[0]?.bytes ?? 0)}, with ${
      count === 1 ? "1 fallback" : `${count} fallbacks`
    } in it`,
  ];
  for (const chunk of described) {
    for (const boundary of chunk.boundaries) {
      detail.push(describeBoundary(chunk, boundary, total));
    }
  }
  return {
    // The boundaries and not the chunks, because the chunk count answers a
    // question nobody has: a document is cut wherever React happened to flush,
    // and two of the pieces are uf's own closing markup. What a reader wants
    // from one line is that the page streamed, how much of it arrived late, and
    // how late. The chunk each boundary rode in on is in its own line below.
    message:
      `${path} streamed ${count === 1 ? "1 boundary" : `${count} boundaries`} after its shell` +
      `, the last ${last == null ? 0 : last.at} ms in`,
    detail,
  };
}

/** One completed boundary as one line: which, when, how big, and what it built. */
function describeBoundary(chunk: StreamedChunk, boundary: StreamedBoundary, total: number): string {
  // Every name on this line came off the page — a class an application chose,
  // an id it generated — so every one of them goes through `label`.
  const named =
    boundary.built.length === 0 ? "nothing of its own" : boundary.built.map(label).join(", ");
  const more = boundary.more === 0 ? "" : ` and ${boundary.more} more`;
  const replaced = boundary.fallback == null ? "its fallback" : label(boundary.fallback);
  return (
    `+${chunk.at} ms  ${boundary.id} replaced ${replaced} with ${named}${more}` +
    ` (chunk ${chunk.index} of ${total}, ${size(chunk.bytes)})`
  );
}

/**
 * A label off the page, short enough and quiet enough for a terminal.
 *
 * Nothing the application wrote may move a cursor or set a colour. The browser
 * channel scrubs what it receives in `@uniflowed/vite`'s
 * `internal/diagnostics.js`; this report never goes through it, so it scrubs
 * its own.
 *
 * By code unit rather than by code point, which is the same answer and a
 * cheaper one: every control character is below `0x20` or is `0x7f`, and both
 * halves of a surrogate pair are far above either — so a pair is copied
 * through unchanged, one half at a time.
 */
function label(text: string): string {
  let out = "";
  for (let index = 0; index < text.length; index += 1) {
    if (out.length >= LABEL_LIMIT) {
      return `${out}…`;
    }
    const code = text.charCodeAt(index);
    out += code < 0x20 || code === 0x7f ? " " : text[index];
  }
  return out;
}

/** A byte count, in the units a person reads. */
function size(bytes: number): string {
  return bytes < 1024 ? `${bytes} B` : `${Math.round(bytes / 102.4) / 10} kB`;
}

/** The last stream shape seen for each path. */
const seen: Map<string, string> = new Map();

/**
 * A reporter for one request, or `null` when nothing is listening.
 *
 * `uf dev` supplies `send`; every other host supplies nothing, and the whole of
 * this module is then unreachable from the render — `./stream.js` never
 * constructs a recorder, so a production stream is byte for byte and generator
 * for generator what it was.
 */
export function streamReporter(
  path: string,
  send: (diagnostic: StreamDiagnostic) => void,
): (record: StreamRecord) => void {
  return (record: StreamRecord) => {
    if (!didStream(record)) {
      return;
    }
    const next = streamSignature(record);
    const previous = seen.get(path);
    if (seen.size >= MEMORY_LIMIT && previous === undefined) {
      const oldest = seen.keys().next();
      if (!oldest.done) {
        seen.delete(oldest.value);
      }
    }
    seen.set(path, next);
    if (previous === next) {
      return;
    }
    send(formatStream(path, record));
  };
}

/**
 * Forget every path this module has seen.
 *
 * For tests, which share one module registry across files and would otherwise
 * inherit a path's history from whichever file rendered it first.
 */
export function forgetStreams(): void {
  seen.clear();
}
