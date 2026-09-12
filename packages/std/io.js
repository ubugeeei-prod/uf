// @flow
//
// `@uniflowed/std/io`: byte readers, byte writers and stream adapters.
//
// Web Streams are the platform boundary. They are also deliberately a boundary:
// a program parsing a binary protocol, collecting a response body, or copying
// one generated payload into another wants a tiny byte-oriented contract rather
// than the full stream surface at every call site. This module is that contract:
// `Reader` and `Writer` over `Uint8Array`, plus the handful of operations Go's
// `io` makes boring.
//
// The Web Stream adapters are structural and constructor-injected for the same
// reason `@uniflowed/effect` does it that way. Node, Bun, Deno and edge workers
// agree on the runtime shape, but they do not all expose the same Flow libdef
// for `ReadableStream` and `WritableStream`.

/** One read from a byte reader. */
export type ReadResult =
  | { readonly done: true }
  | { readonly done: false, readonly value: Uint8Array };

/** A source of byte chunks. */
export interface Reader {
  read(maxBytes?: number): Promise<ReadResult>;
  cancel?(reason?: mixed): Promise<void> | void;
}

/** A sink for byte chunks. */
export interface Writer {
  write(chunk: Uint8Array): Promise<number> | number;
  close?(): Promise<void> | void;
  abort?(reason?: mixed): Promise<void> | void;
}

/** Options for a reader over an in-memory byte buffer. */
export type BytesReaderOptions = {
  readonly chunkSize?: number,
};

/** A writer accepted fewer bytes than it was handed. */
export class ShortWriteError extends Error {
  expected: number;
  written: number;

  constructor(expected: number, written: number) {
    super(`short write: wrote ${String(written)} of ${String(expected)} bytes`);
    this.name = "ShortWriteError";
    this.expected = expected;
    this.written = written;
  }
}

/** A reader over a `Uint8Array`, yielding views into the original buffer. */
export class BytesReader {
  _bytes: Uint8Array;
  _offset: number;
  _chunkSize: number;

  constructor(bytes: Uint8Array, options?: BytesReaderOptions) {
    this._bytes = bytes;
    this._offset = 0;
    const fallback = bytes.length === 0 ? 1 : bytes.length;
    this._chunkSize = checkedSize(options?.chunkSize ?? fallback, "chunkSize");
  }

  /** Current offset in bytes. */
  offset(): number {
    return this._offset;
  }

  /** Bytes that have not been read yet. */
  remaining(): number {
    return this._bytes.length - this._offset;
  }

  /** Start reading from the beginning again. */
  reset(): void {
    this._offset = 0;
  }

  async read(maxBytes?: number): Promise<ReadResult> {
    if (this._offset >= this._bytes.length) {
      return { done: true };
    }
    const requested =
      maxBytes == null
        ? this._chunkSize
        : Math.min(this._chunkSize, checkedSize(maxBytes, "maxBytes"));
    const start = this._offset;
    const end = Math.min(this._bytes.length, start + requested);
    this._offset = end;
    return { done: false, value: this._bytes.subarray(start, end) };
  }
}

/** A writer that collects bytes into memory. */
export class BufferWriter {
  _chunks: Array<Uint8Array>;
  _length: number;

  constructor() {
    this._chunks = [];
    this._length = 0;
  }

  /** Bytes written so far. */
  length(): number {
    return this._length;
  }

  /** Drop the collected output. */
  reset(): void {
    this._chunks = [];
    this._length = 0;
  }

  write(chunk: Uint8Array): number {
    const owned = chunk.slice();
    this._chunks.push(owned);
    this._length += owned.length;
    return chunk.length;
  }

  /** A copy of everything written so far. */
  bytes(): Uint8Array {
    if (this._chunks.length === 0) {
      return new Uint8Array(0);
    }
    if (this._chunks.length === 1) {
      return this._chunks[0].slice();
    }
    const out = new Uint8Array(this._length);
    let offset = 0;
    for (const chunk of this._chunks) {
      out.set(chunk, offset);
      offset += chunk.length;
    }
    return out;
  }
}

/** A reader that stops after `limit` bytes. */
export class LimitedReader {
  _reader: Reader;
  _remaining: number;

  constructor(reader: Reader, limit: number) {
    this._reader = reader;
    this._remaining = checkedSize(limit, "limit");
  }

  remaining(): number {
    return this._remaining;
  }

  async read(): Promise<ReadResult> {
    if (this._remaining <= 0) {
      return { done: true };
    }
    const next = await this._reader.read(this._remaining);
    if (next.done) {
      return next;
    }
    const chunk = next.value;
    if (chunk.length <= this._remaining) {
      this._remaining -= chunk.length;
      return next;
    }
    const value = chunk.subarray(0, this._remaining);
    this._remaining = 0;
    return { done: false, value };
  }

  async cancel(reason?: mixed): Promise<void> {
    await cancelReader(this._reader, reason);
  }
}

/** Create a byte reader over a `Uint8Array`. */
export function readerFromBytes(bytes: Uint8Array, options?: BytesReaderOptions): Reader {
  return new BytesReader(bytes, options);
}

/** Create a reader that exposes at most `limit` bytes from another reader. */
export function limitReader(reader: Reader, limit: number): Reader {
  return new LimitedReader(reader, limit);
}

/** Read a byte reader to the end. */
export async function readAll(reader: Reader): Promise<Uint8Array> {
  const writer = new BufferWriter();
  await copy(writer, reader);
  return writer.bytes();
}

/** Copy all chunks from `reader` into `writer`, returning the byte count. */
export async function copy(writer: Writer, reader: Reader): Promise<number> {
  let total = 0;
  for (;;) {
    const next = await reader.read();
    if (next.done) {
      return total;
    }
    const expected = next.value.length;
    const written = await writer.write(next.value);
    if (!Number.isSafeInteger(written) || written < 0 || written > expected) {
      throw new ShortWriteError(expected, Number(written));
    }
    total += written;
    if (written !== expected) {
      throw new ShortWriteError(expected, written);
    }
  }
}

type StreamReadStep = {
  readonly done?: boolean,
  readonly value?: mixed,
  ...
};

type StreamReader = {
  readonly read: () => Promise<StreamReadStep>,
  readonly cancel: (reason?: mixed) => Promise<void> | void,
  ...
};

type ReadableStreamLike = {
  readonly getReader: () => StreamReader,
  ...
};

type StreamWriter = {
  readonly write: (chunk: Uint8Array) => Promise<void> | void,
  readonly close: () => Promise<void> | void,
  readonly abort: (reason?: mixed) => Promise<void> | void,
  ...
};

type WritableStreamLike = {
  readonly getWriter: () => StreamWriter,
  ...
};

type ReadableController = {
  readonly enqueue: (chunk: Uint8Array) => mixed,
  readonly close: () => mixed,
  readonly error: (reason: mixed) => mixed,
  ...
};

export type ReadableSource = {
  readonly pull: (controller: ReadableController) => Promise<void>,
  readonly cancel: (reason?: mixed) => Promise<void>,
};

export type WritableSink = {
  readonly write: (chunk: Uint8Array) => Promise<void>,
  readonly close: () => Promise<void>,
  readonly abort: (reason?: mixed) => Promise<void>,
};

/** Adapt a web `ReadableStream`-shaped object into an `io.Reader`. */
export function readerFromReadableStream(stream: ReadableStreamLike): Reader {
  const reader = stream.getReader();
  let pending: Uint8Array | null = null;
  return {
    async read(maxBytes?: number): Promise<ReadResult> {
      const requested = maxBytes == null ? null : checkedSize(maxBytes, "maxBytes");
      const buffered = pending;
      if (buffered != null) {
        if (requested != null && buffered.length > requested) {
          pending = buffered.subarray(requested);
          return { done: false, value: buffered.subarray(0, requested) };
        }
        pending = null;
        return { done: false, value: buffered };
      }

      const next = await reader.read();
      if (next.done === true) {
        return { done: true };
      }
      if (!(next.value instanceof Uint8Array)) {
        throw new TypeError("io reader expected Uint8Array stream chunks");
      }
      if (requested != null && next.value.length > requested) {
        pending = next.value.subarray(requested);
        return { done: false, value: next.value.subarray(0, requested) };
      }
      return { done: false, value: next.value };
    },
    async cancel(reason?: mixed): Promise<void> {
      await reader.cancel(reason);
    },
  };
}

/** Adapt a web `WritableStream`-shaped object into an `io.Writer`. */
export function writerFromWritableStream(stream: WritableStreamLike): Writer {
  const writer = stream.getWriter();
  return {
    async write(chunk: Uint8Array): Promise<number> {
      await writer.write(chunk);
      return chunk.length;
    },
    async close(): Promise<void> {
      await writer.close();
    },
    async abort(reason?: mixed): Promise<void> {
      await writer.abort(reason);
    },
  };
}

/**
 * Adapt an `io.Reader` into a web `ReadableStream`.
 *
 * The caller supplies the constructor so the module never has to choose a host
 * global or name a non-portable libdef.
 */
export function readableStreamFromReader<Made>(
  reader: Reader,
  make: (source: ReadableSource) => Made,
): Made {
  let closed = false;
  return make({
    async pull(controller: ReadableController): Promise<void> {
      if (closed) {
        return;
      }
      try {
        const next = await reader.read();
        if (next.done) {
          closed = true;
          controller.close();
        } else {
          controller.enqueue(next.value);
        }
      } catch (error) {
        closed = true;
        controller.error(error);
      }
    },
    async cancel(reason?: mixed): Promise<void> {
      closed = true;
      await cancelReader(reader, reason);
    },
  });
}

/**
 * Adapt an `io.Writer` into a web `WritableStream`.
 *
 * Like `readableStreamFromReader`, the stream constructor is supplied by the
 * host-facing caller.
 */
export function writableStreamFromWriter<Made>(
  writer: Writer,
  make: (sink: WritableSink) => Made,
): Made {
  return make({
    async write(chunk: Uint8Array): Promise<void> {
      const written = await writer.write(chunk);
      if (written !== chunk.length) {
        throw new ShortWriteError(chunk.length, written);
      }
    },
    async close(): Promise<void> {
      await closeWriter(writer);
    },
    async abort(reason?: mixed): Promise<void> {
      await abortWriter(writer, reason);
    },
  });
}

async function cancelReader(reader: Reader, reason?: mixed): Promise<void> {
  const cancel = reader.cancel;
  if (cancel != null) {
    await cancel.call(reader, reason);
  }
}

async function closeWriter(writer: Writer): Promise<void> {
  const close = writer.close;
  if (close != null) {
    await close.call(writer);
  }
}

async function abortWriter(writer: Writer, reason?: mixed): Promise<void> {
  const abort = writer.abort;
  if (abort != null) {
    await abort.call(writer, reason);
  }
}

function checkedSize(value: number, name: string): number {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new RangeError(`${name} must be a non-negative safe integer`);
  }
  if (value === 0 && (name === "chunkSize" || name === "maxBytes")) {
    throw new RangeError(`${name} must be greater than zero`);
  }
  return value;
}
