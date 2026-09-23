// @flow
//
// Internal to `@uniflowed/router`: a finished Flight payload, with the rows a
// partial prerender left for the request taken out.
//
// While `uf build` prerenders a page's static shell, a read of the request
// throws (`@uniflowed/server`'s `internal/partial.js` says why a throw), and
// React's Flight renderer writes an error row where that part of the tree would
// have been, carrying the digest the renderer's `onError` returned for it. Fed
// to the HTML renderer as it is, each of those rows is an error, and a
// `<Suspense>` boundary that meets an error in a prerender is written as one the
// browser has to render — not as a hole a server can finish.
//
// So the rows go. A row that never arrives is a part of the tree React's Flight
// client is still waiting for, and a boundary waiting on it when the prerender
// is stopped is exactly what React leaves as a hole in the shell. Everything
// else in the payload arrived, so everything else is rendered.
//
// # Reading the rows
//
// The wire format is React's, and this reads the framing of it and nothing
// more, following the state machine in React's own client
// (`react-server-dom-*/client`, `processBinaryChunk`): a row is a hexadecimal
// id, a colon, and then either a tag with a hexadecimal length, a comma and
// that many raw bytes — text and typed arrays, which may hold a newline — or
// everything up to the next newline. Only error rows are ever parsed further,
// and only as the JSON React wrote them.

/** The tags whose row is a length and raw bytes rather than a line. */
const SIZED: $ReadOnlySet<number> = new Set(
  ["T", "A", "O", "o", "b", "U", "S", "s", "L", "l", "G", "g", "M", "m", "V"].map((tag) =>
    tag.charCodeAt(0),
  ),
);

const COLON = 0x3a;
const COMMA = 0x2c;
const NEWLINE = 0x0a;
const ERROR_TAG = "E".charCodeAt(0);

/** One row's place in the payload. */
type Row = {| +start: number, +end: number, +tag: number, +body: number |};

/**
 * Every row of a finished payload, in order.
 *
 * A payload that ends partway through a row is not one React finished writing,
 * and is returned whole as its last row rather than guessed at.
 */
function rowsOf(bytes: Uint8Array): Array<Row> {
  const rows: Array<Row> = [];
  let at = 0;
  while (at < bytes.length) {
    const start = at;
    const colon = bytes.indexOf(COLON, at);
    if (colon === -1) {
      rows.push({ start, end: bytes.length, tag: 0, body: bytes.length });
      break;
    }
    const tag = bytes[colon + 1];
    if (SIZED.has(tag)) {
      const comma = bytes.indexOf(COMMA, colon + 2);
      if (comma === -1) {
        rows.push({ start, end: bytes.length, tag: 0, body: bytes.length });
        break;
      }
      const length = Number.parseInt(
        new TextDecoder().decode(bytes.subarray(colon + 2, comma)),
        16,
      );
      const end = Math.min(comma + 1 + length, bytes.length);
      rows.push({ start, end, tag, body: comma + 1 });
      at = end;
      continue;
    }
    const newline = bytes.indexOf(NEWLINE, colon + 1);
    const end = newline === -1 ? bytes.length : newline + 1;
    // A tag is one upper-case letter, `#`, `r` or `x`; anything else is the
    // first byte of a model row, which has no tag.
    const tagged = (tag > 64 && tag < 91) || tag === 0x23 || tag === 0x72 || tag === 0x78;
    rows.push({ start, end, tag: tagged ? tag : 0, body: tagged ? colon + 2 : colon + 1 });
    at = end;
  }
  return rows;
}

/**
 * `payload` without the error rows whose digest `left` recognises.
 *
 * Returns the payload itself, unchanged, when there is no such row.
 */
export function withoutErrorRows(
  payload: Uint8Array,
  left: (digest: string) => boolean,
): {| +payload: Uint8Array, +removed: number |} {
  const decoder = new TextDecoder();
  const kept: Array<Uint8Array> = [];
  let removed = 0;
  let length = 0;
  for (const row of rowsOf(payload)) {
    if (
      row.tag === ERROR_TAG &&
      left(digestOf(decoder.decode(payload.subarray(row.body, row.end))))
    ) {
      removed += 1;
      continue;
    }
    const bytes = payload.subarray(row.start, row.end);
    kept.push(bytes);
    length += bytes.byteLength;
  }
  if (removed === 0) {
    return { payload, removed };
  }
  const joined = new Uint8Array(length);
  let at = 0;
  for (const bytes of kept) {
    joined.set(bytes, at);
    at += bytes.byteLength;
  }
  return { payload: joined, removed };
}

/** The digest an error row carries, or `""`. */
function digestOf(json: string): string {
  try {
    const parsed: mixed = JSON.parse(json);
    if (parsed != null && typeof parsed === "object" && typeof parsed.digest === "string") {
      return parsed.digest;
    }
  } catch {
    // Not React's JSON, so not a row this module took out.
  }
  return "";
}
