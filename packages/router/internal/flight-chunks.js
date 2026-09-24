// @flow
//
// Internal to `@uniflowed/router`: a Flight payload, written into a document
// and read back out of one.
//
// A document a server streams carries the payload the browser hydrates from,
// beside the HTML rendered from it: every chunk React's Flight renderer wrote
// is copied into a `<script type="application/json" data-uf-flight>` element
// as the HTML streams, and the browser reads the elements back into the byte
// stream React's Flight client consumes. This module is the encoding of one
// chunk in each direction, and nothing else, so the writer (`./stream.js`) and
// the reader (`./flight-browser.js`) cannot disagree about it.
//
// # `application/json`, and not a script that runs
//
// The obvious mechanism is an inline script that pushes each chunk into a
// global. `./runtime.js` and `./payload-rows.js` have already made the case
// against it for uf's documents: a script that runs is a script a content
// security policy has to allow, and application data in a script element with
// no type is application data handed to the JavaScript parser. A JSON script
// runs nothing, and a `MutationObserver` notices each one as it lands.
//
// # Text, and bytes that are not text
//
// A payload is bytes. Almost all of it is UTF-8 text — rows of JSON — and it is
// written as a JSON string, which is what makes it readable in a document. A
// typed array a server component passed as a prop is a row of raw bytes that
// need not be valid UTF-8, and a decoder that replaced them would hand React a
// different array. So a run of bytes that does not decode strictly is written
// as base64 instead, and the reader turns each element back into exactly the
// bytes the writer was given.
//
// A chunk boundary can fall inside a multi-byte character, which is not an
// invalid sequence and must not be treated as one: the writer keeps an
// incomplete tail back and prepends it to the next chunk.

/** The attribute every payload chunk element carries. */
export const FLIGHT_CHUNK_ATTRIBUTE: string = "data-uf-flight";

/** What one element holds: text, bytes that are not text, or the end. */
export type FlightChunk = string | {| readonly bytes: string |} | null;

/**
 * An element's text: a JSON value with the three escapes an inline element
 * needs.
 *
 * `<` so a payload holding `</script>` cannot close the element early, and
 * U+2028 and U+2029 because JSON is sometimes read as JavaScript. The same
 * escape `./payload.js` applies, for the same reasons.
 */
function elementJson(value: FlightChunk): string {
  return JSON.stringify(value)
    .replace(/</g, "\\u003c")
    .replace(/\u2028/g, "\\u2028")
    .replace(/\u2029/g, "\\u2029");
}

/**
 * The element for one chunk, or the end marker for `null`.
 *
 * `nonce` is this response's, and it is carried even though the element is
 * `application/json` and therefore a data block the browser never executes.
 * Two reasons, and neither is "in case the spec changes". A uf document's
 * invariant is that every `<script>` in it carries the nonce — that is what
 * `packages/router/streaming.test.js` pins, and an invariant with an exception
 * in it is one a reader has to check every new script against. And these are
 * written as text into a stream rather than rendered by React, so unlike the
 * payload rows in `./runtime.js` there is no second render on the client to
 * disagree with; the attribute costs a comparison nobody makes.
 */
export function flightChunkElement(chunk: FlightChunk, nonce?: string | null): string {
  const carried = nonce == null ? "" : ` nonce="${escapeNonce(nonce)}"`;
  return `<script type="application/json"${carried} ${FLIGHT_CHUNK_ATTRIBUTE}>${elementJson(chunk)}</script>`;
}

/**
 * A nonce, as an attribute value.
 *
 * uf's own nonces are base64 and hold none of these, so this is about the one a
 * project supplied: the value reaches a document that a browser parses as
 * HTML, and a `"` in it would end the attribute and open whatever follows.
 */
function escapeNonce(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/"/g, "&quot;").replace(/</g, "&lt;");
}

/**
 * A writer's state: the bytes of an incomplete character, carried to the next
 * chunk.
 *
 * One per document, made by [`createChunkEncoder`], because the carry is a fact
 * about the stream and two documents must never share one.
 */
export type ChunkEncoder = {|
  /** The elements for one chunk of payload bytes; possibly none. */
  readonly encode: (bytes: Uint8Array) => string,
  /** The elements for whatever is carried, and the end marker. */
  readonly end: () => string,
|};

export function createChunkEncoder(nonce?: string | null): ChunkEncoder {
  const strict = new TextDecoder("utf-8", { fatal: true });
  let carry: Uint8Array = new Uint8Array(0);

  function encode(bytes: Uint8Array): string {
    const joined = concat(carry, bytes);
    const complete = completeLength(joined);
    carry = joined.slice(complete);
    if (complete === 0) {
      return "";
    }
    const body = joined.subarray(0, complete);
    try {
      return flightChunkElement(strict.decode(body), nonce);
    } catch {
      return flightChunkElement({ bytes: base64(body) }, nonce);
    }
  }

  function end(): string {
    const rest = carry;
    carry = new Uint8Array(0);
    const tail = rest.length === 0 ? "" : flightChunkElement({ bytes: base64(rest) }, nonce);
    return `${tail}${flightChunkElement(null, nonce)}`;
  }

  return { encode, end };
}

/**
 * The bytes one element holds, or `null` for the end marker.
 *
 * Refuses anything that is not one of the three shapes the writer produces,
 * rather than guessing: a document carrying some other value in an element
 * with this attribute was not written by this module.
 */
export function flightChunkBytes(text: string): Uint8Array | null {
  const value: mixed = JSON.parse(text);
  if (value === null) {
    return null;
  }
  if (typeof value === "string") {
    return new TextEncoder().encode(value);
  }
  if (typeof value === "object" && !Array.isArray(value)) {
    // Read once into a local: a refinement of `value.bytes` does not survive
    // to the call, a `const` does.
    const bytes = value.bytes;
    if (typeof bytes === "string" && Object.keys(value).length === 1) {
      return fromBase64(bytes);
    }
  }
  throw new Error(`@uniflowed/router: a ${FLIGHT_CHUNK_ATTRIBUTE} element holds no payload chunk`);
}

/**
 * How many leading bytes of `bytes` end on a character boundary.
 *
 * Walks back over at most three continuation bytes to the lead byte of the last
 * character and keeps it back when the character is not yet whole. A byte that
 * is not a valid lead at all is left in — the strict decoder is what says so,
 * and holding it back would carry it forever.
 */
function completeLength(bytes: Uint8Array): number {
  const length = bytes.length;
  let at = length - 1;
  let continuation = 0;
  while (at >= 0 && continuation < 3 && (bytes[at] & 0xc0) === 0x80) {
    at -= 1;
    continuation += 1;
  }
  if (at < 0) {
    return length;
  }
  const lead = bytes[at];
  const needed = lead >= 0xf0 ? 4 : lead >= 0xe0 ? 3 : lead >= 0xc0 ? 2 : 1;
  return length - at >= needed ? length : at;
}

function concat(first: Uint8Array, second: Uint8Array): Uint8Array {
  if (first.length === 0) {
    return second;
  }
  const joined = new Uint8Array(first.length + second.length);
  joined.set(first, 0);
  joined.set(second, first.length);
  return joined;
}

function base64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) {
    binary += String.fromCharCode(byte);
  }
  return btoa(binary);
}

function fromBase64(text: string): Uint8Array {
  const binary = atob(text);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index += 1) {
    bytes[index] = binary.charCodeAt(index);
  }
  return bytes;
}
