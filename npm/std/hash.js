// @flow
//
// `@uniflowed/std/hash`: non-cryptographic hashes JavaScript does not ship.
//
// WebCrypto owns cryptographic digests. This module is the other bucket:
// checksums and table hashes used by file formats, wire protocols and caches.
// The input is either bytes or text; text is hashed as UTF-8, so the result is
// the same on every host and does not depend on JavaScript's UTF-16 storage.

/** Bytes, or text that will be encoded as UTF-8 before hashing. */
export type HashInput = string | Uint8Array;

/** Go-compatible Adler-32, suitable for checksums rather than security. */
export function adler32(input: HashInput, seed?: number): number {
  const data = bytes(input);
  let checksum = uint32(seed ?? 1, "adler32 seed");
  let a = checksum & 0xffff;
  let b = checksum >>> 16;
  let index = 0;

  while (index < data.length) {
    const end = Math.min(index + ADLER_CHUNK, data.length);
    for (; index < end; index += 1) {
      a += data[index];
      b += a;
    }
    a %= ADLER_MOD;
    b %= ADLER_MOD;
  }

  return ((b << 16) | a) >>> 0;
}

/**
 * IEEE CRC-32, the variant used by zip, gzip, PNG and Go's `crc32.ChecksumIEEE`.
 *
 * Pass the previous result as `seed` to continue hashing the next chunk.
 */
export function crc32(input: HashInput, seed?: number): number {
  const data = bytes(input);
  const table = crc32Table();
  let crc = uint32(seed ?? 0, "crc32 seed") ^ 0xffffffff;

  for (const byte of data) {
    crc = (crc >>> 8) ^ table[(crc ^ byte) & 0xff];
  }

  return (crc ^ 0xffffffff) >>> 0;
}

/** FNV-1a over 32 bits. Fast, stable, and not a cryptographic hash. */
export function fnv1a32(input: HashInput, seed?: number): number {
  let hash = uint32(seed ?? FNV1A32_OFFSET, "fnv1a32 seed");

  for (const byte of bytes(input)) {
    hash ^= byte;
    hash = Math.imul(hash, FNV1A32_PRIME) >>> 0;
  }

  return hash;
}

/** FNV-1a over 64 bits. Returns a `bigint` because the result does not fit a JS number. */
export function fnv1a64(input: HashInput, seed?: bigint): bigint {
  let hash = uint64(seed ?? FNV1A64_OFFSET, "fnv1a64 seed");

  for (const byte of bytes(input)) {
    hash ^= BigInt(byte);
    hash = (hash * FNV1A64_PRIME) & UINT64_MASK;
  }

  return hash;
}

/** Coerce text to UTF-8 bytes without accepting accidental inputs at runtime. */
function bytes(input: HashInput): Uint8Array {
  if (typeof input === "string") {
    return textEncoder().encode(input);
  }
  if (input instanceof Uint8Array) {
    return input;
  }
  throw new TypeError("@uniflowed/std/hash: input must be a string or Uint8Array");
}

/** Lazily allocate the UTF-8 encoder. */
function textEncoder(): TextEncoder {
  const cached = ENCODER;
  if (cached != null) {
    return cached;
  }
  const encoder = new TextEncoder();
  ENCODER = encoder;
  return encoder;
}

/** Validate a 32-bit unsigned seed. */
function uint32(value: number, name: string): number {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffffffff) {
    throw new RangeError(`@uniflowed/std/hash: ${name} must be a uint32`);
  }
  return value >>> 0;
}

/** Validate a 64-bit unsigned seed. */
function uint64(value: bigint, name: string): bigint {
  if (value < 0n || value > UINT64_MASK) {
    throw new RangeError(`@uniflowed/std/hash: ${name} must be a uint64`);
  }
  return value;
}

/** Build the reflected IEEE CRC-32 table lazily. */
function crc32Table(): Int32Array {
  const cached = CRC32_TABLE;
  if (cached != null) {
    return cached;
  }

  const table = new Int32Array(256);
  for (let n = 0; n < table.length; n += 1) {
    let crc = n;
    for (let bit = 0; bit < 8; bit += 1) {
      crc = (crc & 1) === 1 ? (crc >>> 1) ^ CRC32_POLY : crc >>> 1;
    }
    table[n] = crc;
  }
  CRC32_TABLE = table;
  return table;
}

const ADLER_MOD = 65521;
const ADLER_CHUNK = 5552;
const CRC32_POLY = 0xedb88320;
const FNV1A32_OFFSET = 0x811c9dc5;
const FNV1A32_PRIME = 0x01000193;
const FNV1A64_OFFSET = 0xcbf29ce484222325n;
const FNV1A64_PRIME = 0x100000001b3n;
const UINT64_MASK = 0xffffffffffffffffn;

let ENCODER: TextEncoder | null = null;
let CRC32_TABLE: Int32Array | null = null;
