// @flow
//
// `@uniflowed/std/bytes`: Go's `bytes`, over `Uint8Array`.
//
// `String` has thirty-odd methods for searching, slicing and joining. Its
// binary counterpart has `set`, `subarray` and `slice`, and everything else is
// a loop somebody writes again:
//
// ```js
// // Comparing two Uint8Arrays, as found in the wild.
// if (JSON.stringify([...a]) === JSON.stringify([...b])) { ... }
// ```
//
// That is not a caricature; it is what happens when the obvious thing is
// missing. Every function here is a loop that is easy to write and easy to
// write slightly wrong: a comparison that is quadratic, a search that misses a
// match spanning the point where it restarted, a concatenation that copies the
// whole buffer once per piece.
//
// # Views, not copies
//
// `split`, `trimPrefix` and `trimSuffix` return **views** into the input, the
// way Go's `bytes.Split` returns subslices of the same backing array. Nothing
// is copied and nothing is allocated but the view objects, which is what makes
// splitting a megabyte on newlines cost the newlines rather than the megabyte.
//
// The consequence is the same as Go's: writing through one of those views
// writes through to the original, and holding one keeps the whole original
// buffer alive. Where a copy is wanted, `.slice()` on the result says so, at
// the one call site that knows.
//
// [`concat`], [`join`], [`repeat`] and [`Builder.bytes`] allocate, because
// there is nothing to be a view of.
//
// # Searching
//
// [`indexOf`] scans for the needle's first byte with `Uint8Array.prototype
// .indexOf`, which is one native call rather than one interpreted loop
// iteration per byte, and only then compares the rest. On the case that matters
// — a needle whose first byte is rare in the haystack, which is nearly every
// real delimiter — that is several times faster than the naive double loop, and
// it is never slower by more than the cost of the calls it makes. It is not a
// Boyer–Moore or a two-way search: those win on long needles, they need
// preprocessing proportional to the needle, and the needles people actually
// search binary data for are one to four bytes long.
//
// # What is not here
//
// Case folding, `Fields`, `Title`, and the rest of the half of Go's `bytes`
// that is really about text. Bytes that are text should be decoded — the
// platform has `TextDecoder` and it is faster than anything written here would
// be — and the string methods that follow are better than their byte-wise
// equivalents. This module is for the bytes that are not text.

/** Lexicographic ordering, the way Go's `bytes.Compare` reports it. */
export type Ordering = -1 | 0 | 1;

/**
 * Whether two buffers hold the same bytes.
 *
 * Length first, which settles the common case without touching a byte, and then
 * a plain loop. Not constant time: this is for parsing and dispatch, and
 * comparing a secret with it leaks where the difference is. `timingSafeEqual`
 * in `@uniflowed/std` is the one for that.
 */
export function equal(a: Uint8Array, b: Uint8Array): boolean {
  if (a === b) {
    return true;
  }
  if (a.length !== b.length) {
    return false;
  }
  for (let index = 0; index < a.length; index += 1) {
    if (a[index] !== b[index]) {
      return false;
    }
  }
  return true;
}

/**
 * Order two buffers lexicographically, by byte and then by length.
 *
 * `-1`, `0` or `+1`, which is Go's `bytes.Compare` and is also exactly what
 * `Array.prototype.sort` wants — so sorting a list of buffers is
 * `list.sort(compare)` and nothing else.
 *
 * "Then by length" is the part worth stating: a prefix sorts before what it is
 * a prefix of, so `[1, 2]` comes before `[1, 2, 0]`. Byte order is unsigned,
 * because `Uint8Array` is.
 */
export function compare(a: Uint8Array, b: Uint8Array): Ordering {
  const shared = Math.min(a.length, b.length);
  for (let index = 0; index < shared; index += 1) {
    if (a[index] !== b[index]) {
      return a[index] < b[index] ? -1 : 1;
    }
  }
  if (a.length === b.length) {
    return 0;
  }
  return a.length < b.length ? -1 : 1;
}

/**
 * The index of the first occurrence of `needle` in `haystack`, or `-1`.
 *
 * An empty needle is found at `from`, which is what `String.prototype.indexOf`
 * does and what makes `split` on an empty separator terminate instead of
 * looping.
 *
 * `from` is clamped rather than validated: a negative start means the
 * beginning, and a start past the end means there is nothing left to find. A
 * search is a question, and neither of those is a mistake worth an exception.
 */
export function indexOf(haystack: Uint8Array, needle: Uint8Array, from?: number): number {
  const start = Math.max(0, from ?? 0);
  if (needle.length === 0) {
    return start <= haystack.length ? start : -1;
  }
  if (needle.length > haystack.length) {
    return -1;
  }
  const first = needle[0];
  const last = haystack.length - needle.length;
  let at = start;
  while (at <= last) {
    // One native call finds the next candidate; the interpreted loop below runs
    // only on positions whose first byte already matched. See the module header
    // for why this beats the naive double loop on real delimiters.
    const found = haystack.indexOf(first, at);
    if (found < 0 || found > last) {
      return -1;
    }
    if (matchesAt(haystack, needle, found)) {
      return found;
    }
    at = found + 1;
  }
  return -1;
}

/**
 * The index of the last occurrence of `needle` in `haystack`, or `-1`.
 *
 * `lastIndexOf` on the first byte, walking backwards, for the same reason
 * [`indexOf`] scans forwards for it. An empty needle is found at the end.
 */
export function lastIndexOf(haystack: Uint8Array, needle: Uint8Array): number {
  if (needle.length === 0) {
    return haystack.length;
  }
  if (needle.length > haystack.length) {
    return -1;
  }
  const first = needle[0];
  let at = haystack.length - needle.length;
  while (at >= 0) {
    const found = haystack.lastIndexOf(first, at);
    if (found < 0) {
      return -1;
    }
    if (matchesAt(haystack, needle, found)) {
      return found;
    }
    at = found - 1;
  }
  return -1;
}

/** Whether `needle` occurs anywhere in `haystack`. */
export function contains(haystack: Uint8Array, needle: Uint8Array): boolean {
  return indexOf(haystack, needle) >= 0;
}

/** Whether `bytes` starts with `prefix`. An empty prefix always matches. */
export function hasPrefix(bytes: Uint8Array, prefix: Uint8Array): boolean {
  return prefix.length <= bytes.length && matchesAt(bytes, prefix, 0);
}

/** Whether `bytes` ends with `suffix`. An empty suffix always matches. */
export function hasSuffix(bytes: Uint8Array, suffix: Uint8Array): boolean {
  return suffix.length <= bytes.length && matchesAt(bytes, suffix, bytes.length - suffix.length);
}

/**
 * `bytes` without `prefix`, or `bytes` unchanged when it does not start with one.
 *
 * A view. Unchanged means the same object, so `trimPrefix(b, p) === b` answers
 * "was there a prefix" without a second comparison.
 */
export function trimPrefix(bytes: Uint8Array, prefix: Uint8Array): Uint8Array {
  return hasPrefix(bytes, prefix) && prefix.length > 0 ? bytes.subarray(prefix.length) : bytes;
}

/** `bytes` without `suffix`, or `bytes` unchanged. A view; see [`trimPrefix`]. */
export function trimSuffix(bytes: Uint8Array, suffix: Uint8Array): Uint8Array {
  return hasSuffix(bytes, suffix) && suffix.length > 0
    ? bytes.subarray(0, bytes.length - suffix.length)
    : bytes;
}

/**
 * Split `bytes` on every occurrence of `separator`.
 *
 * Views into `bytes`, so this costs one object per piece rather than a copy of
 * the input. `n` pieces for `n - 1` separators, including the empty pieces
 * around a separator at either end — splitting `,a,` on `,` is three pieces,
 * two of them empty, which is what every other `split` in the language does and
 * what makes the round trip through [`join`] exact.
 *
 * An empty separator throws. `String.prototype.split("")` answers with the
 * characters, and the byte-wise reading of that is "every byte its own piece",
 * which is `Array.from(bytes)` and is not what anybody reaching for a delimiter
 * meant. Answering the wrong question quietly is worse than refusing.
 */
export function split(bytes: Uint8Array, separator: Uint8Array): $ReadOnlyArray<Uint8Array> {
  if (separator.length === 0) {
    throw new Error("@uniflowed/std/bytes: split needs a non-empty separator");
  }
  const pieces: Array<Uint8Array> = [];
  let start = 0;
  let found = indexOf(bytes, separator, start);
  while (found >= 0) {
    pieces.push(bytes.subarray(start, found));
    start = found + separator.length;
    found = indexOf(bytes, separator, start);
  }
  // The tail, which is the whole input when there was no separator at all.
  pieces.push(bytes.subarray(start));
  return pieces;
}

/**
 * Concatenate `pieces` with `separator` between them.
 *
 * The inverse of [`split`], exactly: `join(split(b, s), s)` holds the same
 * bytes as `b` for every `b` and every non-empty `s`.
 *
 * One allocation. The total length is known before anything is copied, which is
 * the whole reason to have this rather than a `reduce` that concatenates two
 * buffers at a time — that version copies the accumulated prefix once per
 * piece, so joining `n` pieces of `k` bytes moves `n²k/2` bytes instead of `nk`.
 */
export function join(pieces: $ReadOnlyArray<Uint8Array>, separator: Uint8Array): Uint8Array {
  if (pieces.length === 0) {
    return new Uint8Array(0);
  }
  let total = separator.length * (pieces.length - 1);
  for (const piece of pieces) {
    total += piece.length;
  }
  const out = new Uint8Array(total);
  let at = 0;
  for (let index = 0; index < pieces.length; index += 1) {
    if (index > 0 && separator.length > 0) {
      out.set(separator, at);
      at += separator.length;
    }
    out.set(pieces[index], at);
    at += pieces[index].length;
  }
  return out;
}

/** Concatenate `pieces` with nothing between them. [`join`] with no separator. */
export function concat(pieces: $ReadOnlyArray<Uint8Array>): Uint8Array {
  return join(pieces, EMPTY);
}

/**
 * `bytes` repeated `count` times.
 *
 * `count` must be a non-negative integer; zero gives an empty buffer. Doubling
 * rather than appending, so a thousand repeats is ten copies of geometrically
 * growing regions rather than a thousand copies of one.
 */
export function repeat(bytes: Uint8Array, count: number): Uint8Array {
  if (!Number.isInteger(count) || count < 0) {
    throw new Error(
      `@uniflowed/std/bytes: repeat needs a non-negative integer, got ${String(count)}`,
    );
  }
  const total = bytes.length * count;
  const out = new Uint8Array(total);
  if (total === 0) {
    return out;
  }
  out.set(bytes, 0);
  let filled = bytes.length;
  while (filled < total) {
    const take = Math.min(filled, total - filled);
    out.set(out.subarray(0, take), filled);
    filled += take;
  }
  return out;
}

/** `text` as UTF-8 bytes. `TextEncoder`, named the way the rest of this is. */
export function fromUtf8(text: string): Uint8Array {
  return ENCODER.encode(text);
}

/**
 * `bytes` decoded as UTF-8.
 *
 * Lossy: an invalid sequence becomes U+FFFD rather than an error, which is what
 * `TextDecoder` does by default and what a log line or an error message wants.
 * Somewhere that must reject malformed input should construct its own
 * `TextDecoder("utf-8", { fatal: true })` — that is a decision about the data,
 * not about the encoding.
 */
export function toUtf8(bytes: Uint8Array): string {
  return DECODER.decode(bytes);
}

/**
 * A growable byte buffer.
 *
 * Go's `bytes.Buffer` on the write side, and the thing to reach for whenever a
 * loop is building up bytes. The alternative people write — keeping an array of
 * chunks and `concat`ing at the end — is fine; the one they write more often
 * is re-allocating a `Uint8Array` per chunk, which is quadratic.
 *
 * ```js
 * const out = new Builder();
 * for (const record of records) {
 *   out.writeUtf8(record.name);
 *   out.writeByte(0x0a);
 * }
 * return out.bytes();
 * ```
 *
 * Capacity doubles, so `n` bytes written in any number of calls costs `O(n)`
 * copying in total, and the buffer never shrinks until [`reset`].
 */
export class Builder {
  #buffer: Uint8Array;
  #length: number = 0;

  /**
   * `capacity` is a hint: the builder grows past it and never below it.
   *
   * Passing the eventual size when it is known — a length prefix has just been
   * read, say — removes every reallocation, which is the only reason the
   * parameter exists.
   */
  constructor(capacity?: number) {
    const initial = capacity == null || capacity < 1 ? 64 : Math.ceil(capacity);
    this.#buffer = new Uint8Array(initial);
  }

  /** How many bytes have been written. */
  length(): number {
    return this.#length;
  }

  /** Append `bytes`. */
  write(bytes: Uint8Array): void {
    this.#reserve(bytes.length);
    this.#buffer.set(bytes, this.#length);
    this.#length += bytes.length;
  }

  /** Append one byte. Values outside 0–255 are truncated, as `Uint8Array` does. */
  writeByte(byte: number): void {
    this.#reserve(1);
    this.#buffer[this.#length] = byte;
    this.#length += 1;
  }

  /** Append `text` as UTF-8. */
  writeUtf8(text: string): void {
    this.write(fromUtf8(text));
  }

  /**
   * A copy of what has been written.
   *
   * A copy rather than a view, and that is the safe default on purpose: the
   * builder reallocates as it grows, so a view handed out before a `write` can
   * end up looking at the buffer the builder abandoned — a bug that appears
   * only when the next write happens to cross a capacity boundary, which is to
   * say in production and not in the test. [`view`] is the same thing without
   * the copy, for a caller that has read why.
   */
  bytes(): Uint8Array {
    return this.#buffer.slice(0, this.#length);
  }

  /**
   * What has been written, without copying it.
   *
   * Valid until the next write. Use it to hand the contents straight to
   * something that reads them immediately — a hash, a socket write, a
   * `TextDecoder` — and never to keep.
   */
  view(): Uint8Array {
    return this.#buffer.subarray(0, this.#length);
  }

  /** Forget everything written, keeping the capacity for the next round. */
  reset(): void {
    this.#length = 0;
  }

  /** Make room for `more` bytes, doubling until there is. */
  #reserve(more: number): void {
    const needed = this.#length + more;
    if (needed <= this.#buffer.length) {
      return;
    }
    let capacity = this.#buffer.length;
    while (capacity < needed) {
      capacity *= 2;
    }
    const grown = new Uint8Array(capacity);
    grown.set(this.#buffer.subarray(0, this.#length), 0);
    this.#buffer = grown;
  }
}

/**
 * Whether `needle` sits at `at` in `haystack`.
 *
 * The bounds are the caller's to check — every caller here has already
 * established them, and re-checking in the inner loop of a search is the one
 * place it would be measurable.
 */
function matchesAt(haystack: Uint8Array, needle: Uint8Array, at: number): boolean {
  for (let index = 0; index < needle.length; index += 1) {
    if (haystack[at + index] !== needle[index]) {
      return false;
    }
  }
  return true;
}

/** The empty separator [`concat`] joins with. One, because it never changes. */
const EMPTY: Uint8Array = new Uint8Array(0);

/** One encoder for the module: constructing one per call is most of the cost. */
const ENCODER: TextEncoder = new TextEncoder();

/** One decoder, for the same reason. */
const DECODER: TextDecoder = new TextDecoder();
