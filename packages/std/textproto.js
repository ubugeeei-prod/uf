// @flow
//
// `@uniflowed/std/textproto`: MIME-style text protocol headers.
//
// Fetch gives every runtime a `Headers` object, but a lot of protocol work
// starts one layer lower: parse a header block out of bytes or text, keep all
// repeated values, canonicalise keys the way Go's `net/textproto` does, and
// leave the message body for the caller. This module is that small layer. It is
// deliberately string-only and runtime-agnostic; multipart readers and specific
// HTTP adapters can build on the parsed block rather than re-learning line
// folding and repeated header values.

export type HeaderMap = { +[string]: $ReadOnlyArray<string> };

export type HeaderBlock = {
  readonly headers: HeaderMap,
  readonly rest: string,
};

export type StringifyOptions = {
  /** Header line separator. Defaults to `\r\n`, the wire format shape. */
  readonly lineTerminator?: "\n" | "\r\n",
};

type MutableHeaderMap = { [string]: Array<string> };
type HeaderMapBuilder = { [string]: $ReadOnlyArray<string> };

/** A malformed text-protocol header block, with a one-based line and column. */
export class InvalidHeaderError extends Error {
  line: number;
  column: number;

  constructor(message: string, line: number, column: number) {
    super(`${message} at ${String(line)}:${String(column)}`);
    this.name = "InvalidHeaderError";
    this.line = line;
    this.column = column;
  }
}

/**
 * Canonicalise a MIME header key.
 *
 * This follows Go's `CanonicalMIMEHeaderKey` shape: `content-type` becomes
 * `Content-Type`, and each hyphen starts a new word. Invalid token characters
 * are rejected rather than silently producing a key no wire protocol accepts.
 */
export function canonicalHeaderKey(key: string): string {
  validateHeaderKey(key, 1, 1);
  let canonical = "";
  let upperNext = true;
  for (let index = 0; index < key.length; index += 1) {
    const character = key[index];
    if (character === "-") {
      canonical += "-";
      upperNext = true;
      continue;
    }
    canonical += upperNext ? character.toUpperCase() : character.toLowerCase();
    upperNext = false;
  }
  return canonical;
}

/** Parse a complete header block and discard any body text after the blank line. */
export function parseHeaders(source: string): HeaderMap {
  return parseHeaderBlock(source).headers;
}

/**
 * Parse a MIME-style header block.
 *
 * Lines may end in `\n`, `\r\n` or bare `\r`. The first blank line terminates
 * the block and `rest` returns everything after it. A line beginning with a
 * space or tab folds into the previous header value with one separating space.
 */
export function parseHeaderBlock(source: string): HeaderBlock {
  const headers = emptyMutableHeaders();
  let index = 0;
  let line = 1;
  let currentKey: string | null = null;

  while (index < source.length) {
    const start = index;
    while (index < source.length && source[index] !== "\n" && source[index] !== "\r") {
      index += 1;
    }
    const rawLine = source.slice(start, index);
    const next = consumeLineTerminator(source, index);
    index = next.index;

    if (rawLine === "") {
      return { headers: freezeHeaders(headers), rest: source.slice(index) };
    }
    if (rawLine[0] === " " || rawLine[0] === "\t") {
      if (currentKey == null) {
        throw new InvalidHeaderError(
          "@uniflowed/std/textproto: continuation line without a header",
          line,
          1,
        );
      }
      validateHeaderValue(rawLine, line, 1);
      const values = headers[currentKey];
      const lastIndex = values.length - 1;
      values[lastIndex] = `${values[lastIndex]} ${trimHeaderValue(rawLine)}`;
      line += 1;
      continue;
    }

    const colon = rawLine.indexOf(":");
    if (colon <= 0) {
      throw new InvalidHeaderError(
        "@uniflowed/std/textproto: expected header field name followed by ':'",
        line,
        colon < 0 ? rawLine.length + 1 : colon + 1,
      );
    }
    const rawKey = rawLine.slice(0, colon);
    validateHeaderKey(rawKey, line, 1);
    const key = canonicalHeaderKey(rawKey);
    const value = trimHeaderValue(rawLine.slice(colon + 1));
    validateHeaderValue(value, line, colon + 2);
    appendMutable(headers, key, value);
    currentKey = key;
    line += 1;

    if (next.done) {
      break;
    }
  }

  return { headers: freezeHeaders(headers), rest: "" };
}

/** Serialize headers as a terminated header block. */
export function stringifyHeaderBlock(headers: HeaderMap, options?: StringifyOptions): string {
  const lineTerminator = options?.lineTerminator ?? "\r\n";
  const lines = [];
  for (const rawKey of Object.keys(headers)) {
    const key = canonicalHeaderKey(rawKey);
    for (const value of headers[rawKey]) {
      validateHeaderValue(value, 1, key.length + 3);
      lines.push(`${key}: ${value}`);
    }
  }
  if (lines.length === 0) {
    return lineTerminator;
  }
  return `${lines.join(lineTerminator)}${lineTerminator}${lineTerminator}`;
}

/** Return every value stored under `key`, preserving order. */
export function values(headers: HeaderMap, key: string): $ReadOnlyArray<string> {
  const canonical = canonicalHeaderKey(key);
  return hasHeader(headers, canonical) ? headers[canonical] : [];
}

/** Return the first value stored under `key`, or `null` when it is absent. */
export function get(headers: HeaderMap, key: string): string | null {
  const found = values(headers, key);
  return found.length === 0 ? null : found[0];
}

/** Return a new map where `key` has exactly one value. */
export function set(headers: HeaderMap, key: string, value: string): HeaderMap {
  const canonical = canonicalHeaderKey(key);
  validateHeaderValue(value, 1, canonical.length + 3);
  const next = without(headers, canonical);
  next[canonical] = [value];
  return next;
}

/** Return a new map with `value` appended under `key`. */
export function append(headers: HeaderMap, key: string, value: string): HeaderMap {
  const canonical = canonicalHeaderKey(key);
  validateHeaderValue(value, 1, canonical.length + 3);
  const next = without(headers, canonical);
  next[canonical] = values(headers, canonical).concat([value]);
  return next;
}

/** Return a new map without `key`. */
export function remove(headers: HeaderMap, key: string): HeaderMap {
  return without(headers, canonicalHeaderKey(key));
}

function consumeLineTerminator(
  source: string,
  index: number,
): { readonly index: number, readonly done: boolean } {
  if (index >= source.length) {
    return { index, done: true };
  }
  if (source[index] === "\r" && source[index + 1] === "\n") {
    return { index: index + 2, done: false };
  }
  return { index: index + 1, done: false };
}

function appendMutable(headers: MutableHeaderMap, key: string, value: string): void {
  if (!hasHeader(headers, key)) {
    headers[key] = [value];
    return;
  }
  const existing = headers[key];
  existing.push(value);
}

function freezeHeaders(headers: MutableHeaderMap): HeaderMap {
  const frozen = emptyHeaderMap();
  for (const key of Object.keys(headers)) {
    frozen[key] = headers[key].slice();
  }
  return frozen;
}

function without(headers: HeaderMap, key: string): HeaderMap {
  const next = emptyHeaderMap();
  for (const existing of Object.keys(headers)) {
    if (canonicalHeaderKey(existing) !== key) {
      next[existing] = headers[existing].slice();
    }
  }
  return next;
}

function emptyMutableHeaders(): MutableHeaderMap {
  return (Object.create(null): MutableHeaderMap);
}

function emptyHeaderMap(): HeaderMapBuilder {
  return (Object.create(null): HeaderMapBuilder);
}

function hasHeader(headers: HeaderMap | MutableHeaderMap, key: string): boolean {
  return Object.prototype.hasOwnProperty.call(headers, key);
}

function trimHeaderValue(value: string): string {
  return value.replace(/^[ \t]+|[ \t]+$/g, "").replace(/[ \t]+/g, " ");
}

function validateHeaderKey(key: string, line: number, column: number): void {
  if (key === "") {
    throw new InvalidHeaderError("@uniflowed/std/textproto: empty header field name", line, column);
  }
  for (let index = 0; index < key.length; index += 1) {
    if (!isTokenCharacter(key.charCodeAt(index))) {
      throw new InvalidHeaderError(
        "@uniflowed/std/textproto: invalid header field name",
        line,
        column + index,
      );
    }
  }
}

function validateHeaderValue(value: string, line: number, column: number): void {
  for (let index = 0; index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    if ((code < 0x20 && code !== 0x09) || code === 0x7f) {
      throw new InvalidHeaderError(
        "@uniflowed/std/textproto: invalid control character in header value",
        line,
        column + index,
      );
    }
  }
}

function isTokenCharacter(code: number): boolean {
  return (
    (code >= 0x30 && code <= 0x39) ||
    (code >= 0x41 && code <= 0x5a) ||
    (code >= 0x61 && code <= 0x7a) ||
    code === 0x21 ||
    code === 0x23 ||
    code === 0x24 ||
    code === 0x25 ||
    code === 0x26 ||
    code === 0x27 ||
    code === 0x2a ||
    code === 0x2b ||
    code === 0x2d ||
    code === 0x2e ||
    code === 0x5e ||
    code === 0x5f ||
    code === 0x60 ||
    code === 0x7c ||
    code === 0x7e
  );
}
