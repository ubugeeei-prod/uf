/**
 * @fileoverview WHATWG Streams, of which the vendored `streams.js` predates
 * the generics.
 *
 * Flow's `evals/flow-typed/environment/streams.js` declares `ReadableStream`,
 * `WritableStream` and `TransformStream` with no type parameters at all, and a
 * reader whose `read()` resolves `{ value: ?any, done: boolean }`. Every
 * runtime uf targets has had the parameterised form for years, and so has
 * every piece of code that uses one: a server that streams a response writes
 * `ReadableStream<Uint8Array>`, which the vendored declaration answers with
 *
 *     Cannot instantiate `ReadableStream` because class `ReadableStream` is
 *     not a polymorphic type.
 *
 * — twenty-nine times against this repository's own packages, out of the two
 * hundred diagnostics that point into `streams.js`. The code is right and the
 * declaration is old.
 *
 * # How this replaces the vendored one
 *
 * Library definitions are merged in reverse declaration order, so a later
 * file's `ReadableStream` shadows an earlier one's — the mechanism
 * `web-crypto.js` uses for `Crypto` and `vite-client.js` for `Import$Meta`,
 * and the reason `ENVIRONMENTS` lists uf's own libdefs last. Each class is
 * therefore redeclared **whole**, because a shadow is a replacement and not an
 * extension.
 *
 * # Why every parameter has a default
 *
 * `bom.js` writes `body: ?ReadableStream` on a response and `dom.js` writes
 * `stream(): ReadableStream` on a blob, without type arguments and out of
 * reach of this file. A parameter with no default would turn each of those
 * into "cannot use `ReadableStream` without 1 type argument" — trading a
 * generic that nobody can instantiate for a generic nobody can name. `= any`
 * keeps the unparameterised spelling meaning what it means today, while
 * `ReadableStream<Uint8Array>` starts meaning what it says.
 *
 * # What is deliberately not here
 *
 * The queueing strategy classes (`ByteLengthQueuingStrategy`,
 * `CountQueuingStrategy`) and the byte-stream controller's request object are
 * the vendored file's and are left alone: nothing about them is wrong, and a
 * shadow that redeclared them would have to be kept in step with a file uf
 * does not own. Only what the generics reach is replaced.
 */

/** How much a stream will buffer before it stops asking for more. */
type StreamQueuingStrategy<T> = {
  highWaterMark?: number,
  size?: (chunk: T) => number,
  ...
};

/** What `pipeTo` and `pipeThrough` may be told not to do. */
type StreamPipeOptions = {
  preventClose?: boolean,
  preventAbort?: boolean,
  preventCancel?: boolean,
  signal?: AbortSignal,
  ...
};

/** The object a `ReadableStream` is constructed from. */
type StreamSource<T> = {
  type?: void,
  autoAllocateChunkSize?: number,
  start?: (controller: ReadableStreamDefaultController<T>) => void | Promise<void>,
  pull?: (controller: ReadableStreamDefaultController<T>) => void | Promise<void>,
  cancel?: (reason: mixed) => void | Promise<void>,
  ...
};

/** The object a `WritableStream` is constructed from. */
type StreamSink<T> = {
  start?: (controller: WritableStreamDefaultController) => void | Promise<void>,
  write?: (chunk: T, controller: WritableStreamDefaultController) => void | Promise<void>,
  close?: () => void | Promise<void>,
  abort?: (reason: mixed) => void | Promise<void>,
  ...
};

/** The object a `TransformStream` is constructed from. */
type StreamTransformer<I, O> = {
  readableType?: void,
  writableType?: void,
  start?: (controller: TransformStreamDefaultController<O>) => void | Promise<void>,
  transform?: (chunk: I, controller: TransformStreamDefaultController<O>) => void | Promise<void>,
  flush?: (controller: TransformStreamDefaultController<O>) => void | Promise<void>,
  ...
};

/**
 * The readable-and-writable pair `pipeThrough` takes.
 *
 * An interface rather than an object type, because the argument everybody
 * passes is a class instance — `new TransformStream()`, `new
 * TextDecoderStream()` — and Flow says so in as many words: *class instances
 * are not subtypes of object types; consider rewriting object type as an
 * interface*. Read-only members for the other half of the same call: a
 * `TransformStream`'s own `readable` is read-only, and an invariant member
 * here would reject it for being no more writable than it promises.
 */
declare interface StreamTransform<I, O> {
  readonly readable: ReadableStream<O>;
  readonly writable: WritableStream<I>;
}

/**
 * One read from a reader.
 *
 * A union rather than `{ done: boolean, value?: T }`, so that a `done` of
 * `false` is what narrows `value` to a chunk — which is how the loop everybody
 * writes around `read()` is checked at all.
 */
type StreamReadResult<T> = { done: false, value: T, ... } | { done: true, value: void, ... };

declare class ReadableStreamDefaultReader<T = any> {
  constructor(stream: ReadableStream<T>): void;

  readonly closed: Promise<void>;

  cancel(reason?: mixed): Promise<void>;
  read(): Promise<StreamReadResult<T>>;
  releaseLock(): void;
}

declare class ReadableStreamBYOBReader {
  constructor(stream: ReadableStream<Uint8Array>): void;

  readonly closed: Promise<void>;

  cancel(reason?: mixed): Promise<void>;
  read<V extends $TypedArray>(view: V): Promise<StreamReadResult<V>>;
  releaseLock(): void;
}

declare class ReadableStreamDefaultController<T = any> {
  readonly desiredSize: number | null;

  close(): void;
  enqueue(chunk: T): void;
  error(reason?: mixed): void;
}

declare class WritableStreamDefaultController {
  readonly signal: AbortSignal;

  error(reason?: mixed): void;
}

declare class TransformStreamDefaultController<O = any> {
  readonly desiredSize: number | null;

  enqueue(chunk: O): void;
  error(reason?: mixed): void;
  terminate(): void;
}

declare class WritableStreamDefaultWriter<T = any> {
  constructor(stream: WritableStream<T>): void;

  readonly closed: Promise<void>;
  readonly desiredSize: number | null;
  readonly ready: Promise<void>;

  abort(reason?: mixed): Promise<void>;
  close(): Promise<void>;
  releaseLock(): void;
  write(chunk: T): Promise<void>;
}

declare class ReadableStream<T = any> {
  static from<U>(source: Iterable<U> | AsyncIterable<U>): ReadableStream<U>;

  constructor(source?: StreamSource<T>, strategy?: StreamQueuingStrategy<T>): void;

  readonly locked: boolean;

  cancel(reason?: mixed): Promise<void>;
  getReader(): ReadableStreamDefaultReader<T>;
  getReader(options: { mode: "byob", ... }): ReadableStreamBYOBReader;
  // Answers the readable half, which is what makes a chain of transforms one
  // expression. The vendored declaration returned `void`, so the spelling
  // everybody writes — `response.body.pipeThrough(new TextDecoderStream())` —
  // had nothing to read from.
  pipeThrough<U>(
    transform: StreamTransform<T, U>,
    options?: StreamPipeOptions,
  ): ReadableStream<U>;
  pipeTo(destination: WritableStream<T>, options?: StreamPipeOptions): Promise<void>;
  tee(): [ReadableStream<T>, ReadableStream<T>];
  values(options?: { preventCancel?: boolean, ... }): AsyncIterator<T>;
  @@asyncIterator(): AsyncIterator<T>;
}

declare class WritableStream<T = any> {
  constructor(sink?: StreamSink<T>, strategy?: StreamQueuingStrategy<T>): void;

  readonly locked: boolean;

  abort(reason?: mixed): Promise<void>;
  close(): Promise<void>;
  getWriter(): WritableStreamDefaultWriter<T>;
}

declare class TransformStream<I = any, O = any> {
  constructor(
    transformer?: StreamTransformer<I, O>,
    writableStrategy?: StreamQueuingStrategy<I>,
    readableStrategy?: StreamQueuingStrategy<O>,
  ): void;

  readonly readable: ReadableStream<O>;
  readonly writable: WritableStream<I>;
}
