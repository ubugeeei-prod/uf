// @flow
//
// `@uniflowed/std/bufio`: buffered byte readers and token scanners.
//
// `@uniflowed/std/io` gives the small byte `Reader` contract. This module is
// the layer that keeps a little unread data around: peeking without consuming,
// reading one byte at a time without forcing every caller to manage leftovers,
// and scanning lines or words with the same machinery a protocol parser would
// give a custom split function.

import type { Reader, ReadResult } from "./io.js";

export type BufferedReaderOptions = {
  readonly bufferSize?: number,
};

export type ScannerOptions = {
  readonly bufferSize?: number,
  readonly maxTokenSize?: number,
  readonly split?: SplitFunc,
};

export type SplitResult = {
  readonly advance: number,
  readonly token: Uint8Array | null,
};

export type SplitFunc = (data: Uint8Array, atEof: boolean) => SplitResult;

const DEFAULT_BUFFER_SIZE = 4096;
const DEFAULT_MAX_TOKEN_SIZE = 64 * 1024;
const EMPTY = new Uint8Array(0);

const utf8 = new TextDecoder();

/** A token grew beyond the scanner's configured maximum. */
export class TokenTooLongError extends Error {
  limit: number;

  constructor(limit: number) {
    super(`token exceeds maximum size of ${String(limit)} bytes`);
    this.name = "TokenTooLongError";
    this.limit = limit;
  }
}

/** A byte reader with an internal buffer for peeking and small reads. */
export class BufferedReader {
  _reader: Reader;
  _buffer: Uint8Array;
  _bufferSize: number;
  _atEof: boolean;

  constructor(reader: Reader, options?: BufferedReaderOptions) {
    this._reader = reader;
    this._buffer = EMPTY;
    this._bufferSize = checkedPositive(options?.bufferSize ?? DEFAULT_BUFFER_SIZE, "bufferSize");
    this._atEof = false;
  }

  /** Bytes currently held in the buffer. */
  buffered(): number {
    return this._buffer.length;
  }

  /** Replace the underlying reader and drop buffered bytes. */
  reset(reader: Reader): void {
    this._reader = reader;
    this._buffer = EMPTY;
    this._atEof = false;
  }

  /** Return up to `maxBytes` bytes, or one buffered chunk when no bound is given. */
  async read(maxBytes?: number): Promise<ReadResult> {
    const requested = maxBytes == null ? this._bufferSize : checkedPositive(maxBytes, "maxBytes");
    await this._fill(1);
    if (this._buffer.length === 0) {
      return { done: true };
    }
    const count = Math.min(requested, this._buffer.length);
    return { done: false, value: this._take(count) };
  }

  /** Return `count` bytes without consuming them. Fewer bytes means EOF. */
  async peek(count: number): Promise<Uint8Array> {
    const requested = checkedNonNegative(count, "count");
    await this._fill(requested);
    return this._buffer.subarray(0, requested).slice();
  }

  /** Consume up to `count` bytes, returning how many were actually skipped. */
  async discard(count: number): Promise<number> {
    let remaining = checkedNonNegative(count, "count");
    let discarded = 0;
    while (remaining > 0) {
      await this._fill(1);
      if (this._buffer.length === 0) {
        return discarded;
      }
      const step = Math.min(remaining, this._buffer.length);
      this._drop(step);
      discarded += step;
      remaining -= step;
    }
    return discarded;
  }

  /** Read one byte, or `null` at EOF. */
  async readByte(): Promise<number | null> {
    await this._fill(1);
    if (this._buffer.length === 0) {
      return null;
    }
    const byte = this._buffer[0];
    this._drop(1);
    return byte;
  }

  async cancel(reason?: mixed): Promise<void> {
    const cancel = this._reader.cancel;
    if (cancel != null) {
      await cancel.call(this._reader, reason);
    }
    this._atEof = true;
    this._buffer = EMPTY;
  }

  async _fill(minBytes: number): Promise<void> {
    while (!this._atEof && this._buffer.length < minBytes) {
      const next = await this._reader.read(this._bufferSize);
      if (next.done) {
        this._atEof = true;
        return;
      }
      if (next.value.length === 0) {
        return;
      }
      this._append(next.value);
    }
  }

  _append(chunk: Uint8Array): void {
    if (this._buffer.length === 0) {
      this._buffer = chunk.slice();
      return;
    }
    const joined = new Uint8Array(this._buffer.length + chunk.length);
    joined.set(this._buffer, 0);
    joined.set(chunk, this._buffer.length);
    this._buffer = joined;
  }

  _take(count: number): Uint8Array {
    const out = this._buffer.subarray(0, count);
    this._drop(count);
    return out;
  }

  _drop(count: number): void {
    this._buffer = count >= this._buffer.length ? EMPTY : this._buffer.subarray(count);
  }
}

/** Create a buffered reader over an `io.Reader`. */
export function newReader(reader: Reader, options?: BufferedReaderOptions): BufferedReader {
  return new BufferedReader(reader, options);
}

/** A token scanner over a byte reader. */
export class Scanner {
  _reader: BufferedReader;
  _split: SplitFunc;
  _maxTokenSize: number;
  _atEof: boolean;
  _token: Uint8Array;
  _error: Error | null;

  constructor(reader: Reader, options?: ScannerOptions) {
    const readerOptions =
      options?.bufferSize == null ? undefined : { bufferSize: options.bufferSize };
    this._reader =
      reader instanceof BufferedReader ? reader : new BufferedReader(reader, readerOptions);
    this._split = options?.split ?? scanLines;
    this._maxTokenSize = checkedPositive(
      options?.maxTokenSize ?? DEFAULT_MAX_TOKEN_SIZE,
      "maxTokenSize",
    );
    this._atEof = false;
    this._token = EMPTY;
    this._error = null;
  }

  /** The most recent token as bytes. */
  bytes(): Uint8Array {
    return this._token.slice();
  }

  /** The most recent token decoded as UTF-8. */
  text(): string {
    return utf8.decode(this._token);
  }

  /** The terminal scanner error, or `null` after clean EOF. */
  error(): Error | null {
    return this._error;
  }

  /** Scan the next token. */
  async scan(): Promise<boolean> {
    this._token = EMPTY;
    if (this._error != null) {
      return false;
    }

    for (;;) {
      await this._reader._fill(1);
      const buffered = this._reader._buffer;
      const result = this._split(buffered, this._atEof);
      validateSplit(result, buffered.length);
      if (result.token != null) {
        if (result.token.length > this._maxTokenSize) {
          this._error = new TokenTooLongError(this._maxTokenSize);
          return false;
        }
        await this._reader.discard(result.advance);
        this._token = result.token;
        return true;
      }
      if (result.advance > 0) {
        await this._reader.discard(result.advance);
        continue;
      }
      if (buffered.length > this._maxTokenSize) {
        this._error = new TokenTooLongError(this._maxTokenSize);
        return false;
      }
      if (this._atEof) {
        return false;
      }

      const previous = this._reader.buffered();
      await this._reader._fill(previous + 1);
      if (this._reader.buffered() === previous) {
        this._atEof = true;
      }
    }
  }
}

/** Split into one-byte tokens. */
export function scanBytes(data: Uint8Array, _atEof: boolean): SplitResult {
  if (data.length === 0) {
    return { advance: 0, token: null };
  }
  return { advance: 1, token: data.subarray(0, 1) };
}

/** Split on `\n`, returning lines without `\r` or `\n`. */
export function scanLines(data: Uint8Array, atEof: boolean): SplitResult {
  for (let index = 0; index < data.length; index += 1) {
    if (data[index] === 0x0a) {
      const end = index > 0 && data[index - 1] === 0x0d ? index - 1 : index;
      return { advance: index + 1, token: data.subarray(0, end) };
    }
  }
  if (atEof && data.length > 0) {
    const end = data[data.length - 1] === 0x0d ? data.length - 1 : data.length;
    return { advance: data.length, token: data.subarray(0, end) };
  }
  return { advance: 0, token: null };
}

/** Split on ASCII whitespace. */
export function scanWords(data: Uint8Array, atEof: boolean): SplitResult {
  let start = 0;
  while (start < data.length && isSpace(data[start])) {
    start += 1;
  }
  for (let end = start; end < data.length; end += 1) {
    if (isSpace(data[end])) {
      return { advance: end + 1, token: data.subarray(start, end) };
    }
  }
  if (atEof && start < data.length) {
    return { advance: data.length, token: data.subarray(start) };
  }
  return { advance: start, token: null };
}

function isSpace(byte: number): boolean {
  return byte === 0x20 || (byte >= 0x09 && byte <= 0x0d);
}

function validateSplit(result: SplitResult, available: number): void {
  if (!Number.isSafeInteger(result.advance) || result.advance < 0) {
    throw new RangeError("split advance must be a non-negative safe integer");
  }
  if (result.advance > available) {
    throw new RangeError("split advance exceeds buffered bytes");
  }
}

function checkedPositive(value: number, name: string): number {
  const checked = checkedNonNegative(value, name);
  if (checked === 0) {
    throw new RangeError(`${name} must be greater than zero`);
  }
  return checked;
}

function checkedNonNegative(value: number, name: string): number {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new RangeError(`${name} must be a non-negative safe integer`);
  }
  return value;
}
