// @flow
//
// `@uniflowed/std/base32`: Go's `encoding/base32`.
//
// Base32 is the alphabet people reach for when a byte string has to be copied
// by a human: TOTP secrets, recovery tokens, DNS labels, storage keys. The web
// platform has base64 in several shapes and no base32 at all, so every project
// that needs it either ships a dependency for one loop or writes the loop
// inline.
//
// This module implements RFC 4648's standard alphabet:
// `A-Z` and `2-7`. Encoding produces uppercase. Decoding accepts uppercase and
// lowercase, with or without `=` padding, because secrets are often printed
// without padding and often pasted in lowercase. It does not accept separators
// or whitespace; a caller that wants to group text for display should remove
// the groups before decoding, at the boundary where that policy belongs.

/** Whether encoded output should include RFC 4648 `=` padding. */
export type Base32Padding = "include" | "omit";

/** Options for [`encode`] and [`encodedLength`]. */
export type Base32EncodeOptions = {
  readonly padding?: Base32Padding,
};

/**
 * A base32 string that could not be decoded.
 *
 * `offset` points at the byte in the input that made decoding impossible: the
 * invalid character, the first padding character that appears in the wrong
 * place, or the final character when the text length cannot name whole bytes.
 */
export class InvalidBase32Error extends Error {
  offset: number;

  constructor(message: string, offset: number) {
    super(message);
    this.name = "InvalidBase32Error";
    this.offset = offset;
  }
}

/** How many characters `n` bytes encode to. */
export function encodedLength(n: number, options?: Base32EncodeOptions): number {
  if (!Number.isSafeInteger(n) || n < 0) {
    throw new RangeError(
      `@uniflowed/std/base32: byte length must be a non-negative safe integer, got ${String(n)}`,
    );
  }
  if (n === 0) {
    return 0;
  }
  const unpadded = Math.ceil((n * 8) / 5);
  return options?.padding === "omit" ? unpadded : Math.ceil(unpadded / 8) * 8;
}

/** The maximum number of bytes `n` base32 characters can decode to. */
export function decodedLength(n: number): number {
  if (!Number.isSafeInteger(n) || n < 0) {
    throw new RangeError(
      `@uniflowed/std/base32: text length must be a non-negative safe integer, got ${String(n)}`,
    );
  }
  return Math.floor((n * 5) / 8);
}

/**
 * Encode bytes as RFC 4648 base32.
 *
 * Padding is included by default, matching Go's `base32.StdEncoding`. Pass
 * `{ padding: "omit" }` for TOTP-style secrets.
 */
export function encode(bytes: Uint8Array, options?: Base32EncodeOptions): string {
  if (bytes.length === 0) {
    return "";
  }

  const out = new Array<string>(encodedLength(bytes.length, options));
  let written = 0;
  let buffer = 0;
  let bits = 0;

  for (const byte of bytes) {
    buffer = (buffer << 8) | byte;
    bits += 8;
    while (bits >= 5) {
      bits -= 5;
      out[written] = ALPHABET[(buffer >> bits) & 31];
      written += 1;
      buffer &= mask(bits);
    }
  }
  if (bits > 0) {
    out[written] = ALPHABET[(buffer << (5 - bits)) & 31];
    written += 1;
  }
  if (options?.padding !== "omit") {
    while (written < out.length) {
      out[written] = "=";
      written += 1;
    }
  }
  return out.join("");
}

/**
 * Decode RFC 4648 base32 text.
 *
 * Padded and unpadded text are accepted. The alphabet is case-insensitive; any
 * other character, misplaced padding, impossible length, or non-zero trailing
 * padding bit raises [`InvalidBase32Error`].
 */
export function decode(text: string): Uint8Array {
  const bodyEnd = paddingStart(text);
  const padding = text.length - bodyEnd;
  if (padding > 0) {
    validatePadding(text, bodyEnd, padding);
  }
  validateRemainder(bodyEnd, text.length === 0 ? 0 : Math.max(0, bodyEnd - 1));

  const table = values();
  const out = new Uint8Array(decodedLength(bodyEnd));
  let written = 0;
  let buffer = 0;
  let bits = 0;

  for (let index = 0; index < bodyEnd; index += 1) {
    const value = digit(table, text.charCodeAt(index));
    if (value < 0) {
      throw invalid(text, index);
    }
    buffer = (buffer << 5) | value;
    bits += 5;
    if (bits >= 8) {
      bits -= 8;
      out[written] = (buffer >> bits) & 0xff;
      written += 1;
      buffer &= mask(bits);
    }
  }
  if (bits > 0 && buffer !== 0) {
    throw new InvalidBase32Error("@uniflowed/std/base32: non-zero trailing bits", bodyEnd - 1);
  }
  return out;
}

/** Whether `text` would decode. */
export function isValid(text: string): boolean {
  try {
    decode(text);
    return true;
  } catch (failure) {
    if (failure instanceof InvalidBase32Error) {
      return false;
    }
    throw failure;
  }
}

/** Where padding starts, or `text.length` when there is none. */
function paddingStart(text: string): number {
  const first = text.indexOf("=");
  return first < 0 ? text.length : first;
}

/** Check that every character after the first `=` is padding and in the right count. */
function validatePadding(text: string, bodyEnd: number, padding: number): void {
  for (let index = bodyEnd; index < text.length; index += 1) {
    if (text[index] !== "=") {
      throw invalid(text, index);
    }
  }
  if (text.length % 8 !== 0) {
    throw new InvalidBase32Error(
      "@uniflowed/std/base32: padded input length must be a multiple of 8",
      bodyEnd,
    );
  }
  const expected = expectedPadding(bodyEnd % 8);
  if (expected < 0) {
    throw new InvalidBase32Error("@uniflowed/std/base32: invalid encoded length", bodyEnd - 1);
  }
  if (expected !== padding) {
    throw new InvalidBase32Error(
      `@uniflowed/std/base32: invalid padding, expected ${String(expected)} characters`,
      bodyEnd,
    );
  }
}

/** How much padding a body remainder requires. */
function expectedPadding(remainder: number): number {
  switch (remainder) {
    case 0:
      return 0;
    case 2:
      return 6;
    case 4:
      return 4;
    case 5:
      return 3;
    case 7:
      return 1;
    default:
      return -1;
  }
}

/** Check that a padding-free body length can name whole bytes. */
function validateRemainder(length: number, offset: number): void {
  if (expectedPadding(length % 8) < 0) {
    throw new InvalidBase32Error("@uniflowed/std/base32: invalid encoded length", offset);
  }
}

/** Look up one base32 digit. */
function digit(table: Int16Array, code: number): number {
  return code < table.length ? table[code] : -1;
}

/** Build the ASCII lookup table lazily. */
function values(): Int16Array {
  const built = VALUES;
  if (built != null) {
    return built;
  }
  const table = new Int16Array(128);
  table.fill(-1);
  for (let value = 0; value < ALPHABET.length; value += 1) {
    const upper = ALPHABET.charCodeAt(value);
    table[upper] = value;
    const lower = lowerAscii(upper);
    if (lower !== upper) {
      table[lower] = value;
    }
  }
  VALUES = table;
  return table;
}

/** ASCII lowercase for the letters in the alphabet. */
function lowerAscii(code: number): number {
  return code >= 0x41 && code <= 0x5a ? code + 0x20 : code;
}

/** A mask with `bits` low bits set. */
function mask(bits: number): number {
  return bits === 0 ? 0 : (1 << bits) - 1;
}

/** A consistent invalid-character error. */
function invalid(text: string, offset: number): InvalidBase32Error {
  return new InvalidBase32Error(
    `@uniflowed/std/base32: invalid character ${JSON.stringify(text[offset] ?? "")} at offset ${String(offset)}`,
    offset,
  );
}

const ALPHABET = "ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

let VALUES: Int16Array | null = null;
