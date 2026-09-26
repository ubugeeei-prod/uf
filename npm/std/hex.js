// @flow
//
// `@uniflowed/std/hex`: Go's `encoding/hex`.
//
// Hexadecimal is how bytes get written down — a digest in a lockfile, an ETag,
// a key in a URL, a packet in a log — and JavaScript's answer is still
// `Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("")`. That
// line is correct and it is on a wiki somewhere in every company, because
// `Uint8Array.prototype.toHex` is very new: Firefox 133 and Safari 18.2 at the
// end of 2024, Chrome 140 and Deno 2.5 in September 2025, and **Node 25.0 in
// October 2025** — one major version after the Node this repository's own suite
// runs on. A module that has to run everywhere cannot call it yet, and this one
// is what to import until it can.
//
// # Decoding is the half that needs care
//
// Encoding cannot fail. Decoding can, four ways, and the version people write
// by hand — `parseInt(text.slice(i, i + 2), 16)` — reports none of them:
// `parseInt("zz", 16)` is `NaN`, which becomes `0` on its way into a
// `Uint8Array`, so a corrupted digest decodes to a *different valid-looking*
// digest instead of an error. `parseInt("1z", 16)` is `1`, silently. So
// [`decode`] rejects odd lengths and any byte that is not a hex digit, and says
// which offset was wrong — the one piece of information that turns "the token
// is bad" into "the token was truncated at 32 characters".
//
// # Case
//
// [`encode`] produces lowercase, which is what every checksum format, every Git
// object id and Go itself produce. [`decode`] accepts either case, because the
// inputs come from other people.
//
// # Cost
//
// Encoding has two paths and picks by length — see `DECODER_WINS_ABOVE`, which
// is where appending to a string stops beating one trip through `TextDecoder`.
// Decoding reads a 128-entry table rather than comparing ranges. Importing this
// module allocates one `TextDecoder` and nothing else: both tables are built by
// the first call that needs them, so a program that imports [`dump`] for a
// debug path it never takes pays for nothing.
//
// That is about 200 MB/s encoding a 32-byte digest and 620 MB/s on a megabyte,
// and about 200 to 250 MB/s decoding. Node's native `Buffer` is roughly six
// times faster than either at a megabyte and about twice as fast on a digest,
// which is a real gap and still not a reason to reach for a binding: see
// `docs/app/reference/std` for the whole table and the argument.

/**
 * A hexadecimal string that could not be decoded.
 *
 * Carries the offset so a caller can say where, which is the whole reason this
 * is a class rather than a message. Go's `hex.InvalidByteError` reports the
 * byte; this reports the position too, because a malformed digest is usually
 * truncated rather than mistyped and the position is what says so.
 */
export class InvalidHexError extends Error {
  /** Index into the input string where decoding stopped. */
  offset: number;

  constructor(message: string, offset: number) {
    super(message);
    this.name = "InvalidHexError";
    this.offset = offset;
  }
}

/** How many characters `n` bytes encode to. Exactly `2n`. */
export function encodedLength(n: number): number {
  return n * 2;
}

/**
 * How many bytes `n` characters decode to.
 *
 * `n / 2`, rounded down — but an odd `n` never decodes, so this rounding is for
 * sizing a buffer before validating rather than a licence to ignore the
 * remainder.
 */
export function decodedLength(n: number): number {
  return n >> 1;
}

/**
 * `bytes` as lowercase hexadecimal.
 *
 * ```js
 * encode(new Uint8Array([0xde, 0xad, 0xbe, 0xef]));   // "deadbeef"
 * ```
 */
export function encode(bytes: Uint8Array): string {
  const table = tables();
  if (bytes.length < DECODER_WINS_ABOVE) {
    // Short input: appending to a string beats going through `TextDecoder`,
    // because the decoder costs about 500 ns before it looks at a byte and this
    // path costs about 6 ns per byte. A 32-byte digest — the common case, and
    // the one every ETag and content hash hits — is 150 ns here and 520 ns
    // through the decoder.
    let out = "";
    for (let index = 0; index < bytes.length; index += 1) {
      out += table.text[bytes[index]];
    }
    return out;
  }
  // Long input: write the digits as ASCII bytes and decode once, so the only
  // string allocated is the answer. `TextDecoder` is UTF-8, and every byte
  // written here is below 0x80, where UTF-8 and ASCII are the same encoding —
  // so this needs no `latin1` decoder, which is the corner of the Encoding
  // standard the edge runtimes do not all implement.
  const out = new Uint8Array(bytes.length * 2);
  for (let index = 0; index < bytes.length; index += 1) {
    const pair = table.codes[bytes[index]];
    out[index * 2] = pair >> 8;
    out[index * 2 + 1] = pair & 0xff;
  }
  return DECODER.decode(out);
}

/**
 * Decode hexadecimal text, or throw [`InvalidHexError`].
 *
 * Either case, no separators, no `0x` prefix, no whitespace — a decoder that
 * strips things is a decoder that accepts two spellings of the same value, and
 * a caller who wants to strip can strip.
 *
 * ```js
 * decode("deadbeef");    // Uint8Array [222, 173, 190, 239]
 * decode("dead beef");   // InvalidHexError at offset 4
 * ```
 */
export function decode(text: string): Uint8Array {
  if (text.length % 2 !== 0) {
    throw new InvalidHexError(
      `@uniflowed/std/hex: odd-length input (${String(text.length)} characters)`,
      text.length,
    );
  }
  const table = values();
  const out = new Uint8Array(text.length >> 1);
  for (let index = 0; index < out.length; index += 1) {
    const high = digit(table, text.charCodeAt(index * 2));
    const low = digit(table, text.charCodeAt(index * 2 + 1));
    if (high < 0) {
      throw invalid(text, index * 2);
    }
    if (low < 0) {
      throw invalid(text, index * 2 + 1);
    }
    out[index] = (high << 4) | low;
  }
  return out;
}

/**
 * Whether `text` would decode.
 *
 * For validating input at a boundary — a route parameter, a header — where the
 * answer is a 400 rather than a decoded value, and where building an exception
 * to throw it away is the expensive part.
 */
export function isValid(text: string): boolean {
  if (text.length % 2 !== 0) {
    return false;
  }
  const table = values();
  for (let index = 0; index < text.length; index += 1) {
    if (digit(table, text.charCodeAt(index)) < 0) {
      return false;
    }
  }
  return true;
}

/**
 * `bytes` as a `hexdump -C` listing: offset, hex columns, and printable text.
 *
 * Go's `hex.Dump`, byte for byte, which means it is the format every
 * `hexdump -C` and `xxd` reader already knows how to read:
 *
 * ```text
 * 00000000  47 45 54 20 2f 20 48 54  54 50 2f 31 2e 31 0d 0a  |GET / HTTP/1.1..|
 * 00000010  48 6f 73 74 3a 20 61 2e  62 0d 0a                 |Host: a.b..|
 * ```
 *
 * The reason to have it rather than logging [`encode`]'s output is the right
 * column: a protocol bug is almost always visible as text, and sixteen bytes
 * per line makes an offset something a person can count to. It ends with a
 * newline when there is anything to print, and is empty otherwise.
 */
export function dump(bytes: Uint8Array): string {
  const table = tables().text;
  const lines: Array<string> = [];
  for (let start = 0; start < bytes.length; start += 16) {
    const row = bytes.subarray(start, Math.min(start + 16, bytes.length));
    const columns: Array<string> = [];
    for (let index = 0; index < 16; index += 1) {
      // The gap after eight columns is what makes a 16-byte line countable at a
      // glance, and it is why the short last row still pads to full width.
      columns.push(index < row.length ? table[row[index]] : "  ");
      if (index === 7) {
        columns.push("");
      }
    }
    let text = "";
    for (const byte of row) {
      // Printable ASCII only. Anything else is a dot, including the bytes a
      // terminal would act on — a dump that could move the cursor is a dump
      // that can lie about what is in the buffer.
      text += byte >= 0x20 && byte < 0x7f ? String.fromCharCode(byte) : ".";
    }
    lines.push(`${offset(start)}  ${columns.join(" ")}  |${text}|`);
  }
  return lines.length === 0 ? "" : `${lines.join("\n")}\n`;
}

/**
 * Every byte's two hex digits, in the two shapes the two encode paths want.
 *
 * `text` is the pair as a string, which the short path appends. `codes` is the
 * pair packed into one 16-bit value — first character in the high half — which
 * the long path writes with two shifts and no string at all.
 *
 * Built lazily, so importing this module does no work: a shipped module that
 * computes at import time is one a bundler cannot drop, and a program that
 * imports [`dump`] for a debug path it never takes should pay for nothing. One
 * null check per call, not per byte.
 */
function tables(): { readonly text: $ReadOnlyArray<string>, readonly codes: Uint16Array } {
  const built = TABLES;
  if (built != null) {
    return built;
  }
  const text = new Array<string>(256);
  const codes = new Uint16Array(256);
  for (let value = 0; value < 256; value += 1) {
    const high = DIGITS.charCodeAt(value >> 4);
    const low = DIGITS.charCodeAt(value & 15);
    text[value] = DIGITS[value >> 4] + DIGITS[value & 15];
    codes[value] = (high << 8) | low;
  }
  const table = { text, codes };
  TABLES = table;
  return table;
}

/**
 * The value of every ASCII char code as a hex digit, or `-1`, built once.
 *
 * A table rather than three range comparisons per character: decoding is the
 * half that runs on untrusted input, so it is the half that runs on the long
 * strings, and a lookup is one load where the comparisons are up to six
 * branches the predictor cannot help with on mixed-case input.
 */
function values(): Int8Array {
  const built = VALUES;
  if (built != null) {
    return built;
  }
  const table = new Int8Array(128).fill(-1);
  for (let value = 0; value < 16; value += 1) {
    table[DIGITS.charCodeAt(value)] = value;
    table[UPPER.charCodeAt(value)] = value;
  }
  VALUES = table;
  return table;
}

/**
 * The value of one hex digit's char code, or `-1` when it is not one.
 *
 * The bound is checked rather than left to the typed array, because indexing a
 * `Int8Array` past its end gives `undefined`, and `undefined < 0` is `false` —
 * which would let every character above U+007F through as a valid digit.
 */
function digit(table: Int8Array, code: number): number {
  return code < 128 ? table[code] : -1;
}

/** The error for a bad character, quoting it and naming where it was. */
function invalid(text: string, at: number): InvalidHexError {
  return new InvalidHexError(
    `@uniflowed/std/hex: ${JSON.stringify(text[at])} at offset ${String(at)} is not a hex digit`,
    at,
  );
}

/** An eight-digit offset column, the width `hexdump -C` uses. */
function offset(at: number): string {
  return at.toString(16).padStart(8, "0");
}

/** One decoder for the module: the ASCII bytes [`encode`] builds go through it. */
const DECODER: TextDecoder = new TextDecoder();

/**
 * Where appending to a string stops beating a trip through `TextDecoder`.
 *
 * Measured, not guessed: the decoder costs about 500 ns per call whatever the
 * length, and the appending loop about 6 ns per byte, so they cross at around
 * 128 bytes on Node 24 and Bun 1.3 on an M-series laptop. The exact point moves
 * between engines; being within a factor of two of it is what matters, because
 * the curve is flat on both sides of the crossing.
 */
const DECODER_WINS_ABOVE = 128;

/** The encoding tables, or `null` until the first call builds them. */
let TABLES: { readonly text: $ReadOnlyArray<string>, readonly codes: Uint16Array } | null = null;

/** The decoding table, or `null` until the first call builds it. */
let VALUES: Int8Array | null = null;

/** Lowercase hex digits: the alphabet both tables are built from, and the output. */
const DIGITS = "0123456789abcdef";

/** The same digits uppercase, which [`decode`] accepts and never produces. */
const UPPER = "0123456789ABCDEF";
