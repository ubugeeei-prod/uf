// @flow
//
// `@uniflowed/std/zip`: ZIP container reading and writing.
//
// The web platform already owns the DEFLATE codec through
// `CompressionStream` and `DecompressionStream`. What it does not own is the
// ZIP container around the compressed byte ranges: the central directory, local
// headers, CRC checks and file names. This module keeps that split explicit.

import { crc32 } from "./hash.js";

export type ZipCompression = "store" | "deflate";

export type ZipEntry = {
  readonly path: string,
  readonly compression: ZipCompression,
  readonly size: number,
  readonly compressedSize: number,
  readonly crc32: number,
};

type ZipRecord = {
  readonly entry: ZipEntry,
  readonly localHeaderOffset: number,
};

type WriteRecord = {
  readonly entry: ZipEntry,
  readonly data: Uint8Array,
};

type StreamReadStep = {
  readonly done?: boolean,
  readonly value?: mixed,
  ...
};

type StreamReader = {
  readonly read: () => Promise<StreamReadStep>,
  readonly cancel?: (reason?: mixed) => Promise<void> | void,
  ...
};

type ReadableStreamLike = {
  readonly getReader: () => StreamReader,
  ...
};

type StreamWriter = {
  readonly write: (chunk: Uint8Array) => Promise<void> | void,
  readonly close: () => Promise<void> | void,
  readonly abort?: (reason?: mixed) => Promise<void> | void,
  ...
};

type WritableStreamLike = {
  readonly getWriter: () => StreamWriter,
  ...
};

type ByteTransform = {
  readonly readable: ReadableStreamLike,
  readonly writable: WritableStreamLike,
  ...
};

export type ByteTransformFactory = (format: "deflate-raw") => ByteTransform;

export type ZipCodecOptions = {
  readonly makeCompressionStream?: ByteTransformFactory,
  readonly makeDecompressionStream?: ByteTransformFactory,
};

export type ZipReaderOptions = {
  readonly makeDecompressionStream?: ByteTransformFactory,
};

export type ZipWriterOptions = {
  readonly compression?: ZipCompression,
  readonly makeCompressionStream?: ByteTransformFactory,
};

/** Deflate raw bytes with the host's web compression stream. */
export async function deflate(bytes: Uint8Array, options?: ZipCodecOptions): Promise<Uint8Array> {
  return transformBytes(bytes, options?.makeCompressionStream ?? defaultCompressionStream);
}

/** Inflate raw DEFLATE bytes with the host's web decompression stream. */
export async function inflate(bytes: Uint8Array, options?: ZipCodecOptions): Promise<Uint8Array> {
  return transformBytes(bytes, options?.makeDecompressionStream ?? defaultDecompressionStream);
}

/** Read entries from an in-memory ZIP archive. */
export class ZipReader {
  _bytes: Uint8Array;
  _view: DataView;
  _records: Array<ZipRecord>;
  _byPath: Map<string, ZipRecord>;
  _makeDecompressionStream: ByteTransformFactory;

  constructor(bytes: Uint8Array, options?: ZipReaderOptions) {
    this._bytes = bytes;
    this._view = view(bytes);
    this._makeDecompressionStream = options?.makeDecompressionStream ?? defaultDecompressionStream;
    this._records = readCentralDirectory(bytes, this._view);
    this._byPath = new Map();
    for (const record of this._records) {
      if (this._byPath.has(record.entry.path)) {
        throw new Error(`@uniflowed/std/zip: duplicate entry ${JSON.stringify(record.entry.path)}`);
      }
      this._byPath.set(record.entry.path, record);
    }
  }

  entries(): $ReadOnlyArray<ZipEntry> {
    return this._records.map((record) => record.entry);
  }

  async read(pathOrEntry: string | ZipEntry): Promise<Uint8Array> {
    const path = typeof pathOrEntry === "string" ? pathOrEntry : pathOrEntry.path;
    const record = this._byPath.get(path);
    if (record == null) {
      throw new Error(`@uniflowed/std/zip: entry not found: ${JSON.stringify(path)}`);
    }
    const compressed = readCompressedData(this._bytes, this._view, record);
    const entry = record.entry;
    const data =
      entry.compression === "store"
        ? compressed.slice()
        : await inflate(compressed, { makeDecompressionStream: this._makeDecompressionStream });
    if (data.length !== entry.size) {
      throw new Error(`@uniflowed/std/zip: ${entry.path} inflated to the wrong size`);
    }
    if (crc32(data) !== entry.crc32) {
      throw new Error(`@uniflowed/std/zip: ${entry.path} failed CRC32 verification`);
    }
    return data;
  }
}

/** Write a small in-memory ZIP archive. */
export class ZipWriter {
  _records: Array<WriteRecord>;
  _makeCompressionStream: ByteTransformFactory;

  constructor(options?: ZipWriterOptions) {
    this._records = [];
    this._makeCompressionStream = options?.makeCompressionStream ?? defaultCompressionStream;
  }

  async add(path: string, bytes: Uint8Array, options?: ZipWriterOptions): Promise<ZipEntry> {
    const checked = checkedPath(path);
    if (this._records.some((record) => record.entry.path === checked)) {
      throw new Error(`@uniflowed/std/zip: duplicate entry ${JSON.stringify(checked)}`);
    }
    const compression = checkedCompression(options?.compression ?? "deflate");
    const owned = bytes.slice();
    const data =
      compression === "store"
        ? owned
        : await deflate(owned, {
            makeCompressionStream: options?.makeCompressionStream ?? this._makeCompressionStream,
          });
    const entry = {
      path: checked,
      compression,
      size: owned.length,
      compressedSize: data.length,
      crc32: crc32(owned),
    };
    this._records.push({ entry, data });
    return entry;
  }

  bytes(): Uint8Array {
    const local = [];
    const central = [];
    let offset = 0;
    for (const record of this._records) {
      const name = ENCODER.encode(record.entry.path);
      const header = localHeader(record.entry, name);
      local.push(header, name, record.data);
      central.push(centralHeader(record.entry, name, offset));
      offset += header.length + name.length + record.data.length;
    }
    const centralOffset = offset;
    const centralBytes = concat(central);
    const end = endOfCentralDirectory(this._records.length, centralBytes.length, centralOffset);
    return concat(local.concat([centralBytes, end]));
  }
}

async function transformBytes(
  bytes: Uint8Array,
  makeTransform: ByteTransformFactory,
): Promise<Uint8Array> {
  const transform = makeTransform("deflate-raw");
  const read = readStream(transform.readable);
  const writer = transform.writable.getWriter();
  await writer.write(bytes);
  await writer.close();
  return await read;
}

async function readStream(stream: ReadableStreamLike): Promise<Uint8Array> {
  const reader = stream.getReader();
  const chunks = [];
  for (;;) {
    const next = await reader.read();
    if (next.done === true) {
      return concat(chunks);
    }
    if (!(next.value instanceof Uint8Array)) {
      await reader.cancel?.("@uniflowed/std/zip: stream produced a non-byte chunk");
      throw new TypeError("@uniflowed/std/zip: stream produced a non-byte chunk");
    }
    chunks.push(next.value);
  }
}

function readCentralDirectory(bytes: Uint8Array, data: DataView): Array<ZipRecord> {
  const end = findEnd(bytes, data);
  const records = [];
  let offset = end.centralOffset;
  for (let index = 0; index < end.entries; index += 1) {
    need(bytes, offset, 46, "central directory header");
    if (u32(data, offset) !== CENTRAL_FILE_HEADER) {
      throw new Error("@uniflowed/std/zip: invalid central directory signature");
    }
    const flags = u16(data, offset + 8);
    if ((flags & ENCRYPTED) !== 0) {
      throw new Error("@uniflowed/std/zip: encrypted entries are not supported");
    }
    const compression = compressionFromMethod(u16(data, offset + 10));
    const crc = u32(data, offset + 16);
    const compressedSize = u32(data, offset + 20);
    const size = u32(data, offset + 24);
    const nameLength = u16(data, offset + 28);
    const extraLength = u16(data, offset + 30);
    const commentLength = u16(data, offset + 32);
    const disk = u16(data, offset + 34);
    const localHeaderOffset = u32(data, offset + 42);
    if (disk !== 0 || isZip64(size) || isZip64(compressedSize) || isZip64(localHeaderOffset)) {
      throw new Error("@uniflowed/std/zip: Zip64 and multi-disk archives are not supported");
    }
    const nameOffset = offset + 46;
    need(bytes, nameOffset, nameLength + extraLength + commentLength, "central directory name");
    const path = checkedPath(
      decodeName(bytes.subarray(nameOffset, nameOffset + nameLength), flags),
    );
    records.push({
      entry: { path, compression, size, compressedSize, crc32: crc },
      localHeaderOffset,
    });
    offset = nameOffset + nameLength + extraLength + commentLength;
  }
  if (offset !== end.centralOffset + end.centralSize) {
    throw new Error("@uniflowed/std/zip: central directory size does not match its entries");
  }
  return records;
}

function readCompressedData(bytes: Uint8Array, data: DataView, record: ZipRecord): Uint8Array {
  const offset = record.localHeaderOffset;
  need(bytes, offset, 30, "local file header");
  if (u32(data, offset) !== LOCAL_FILE_HEADER) {
    throw new Error("@uniflowed/std/zip: invalid local file header signature");
  }
  const nameLength = u16(data, offset + 26);
  const extraLength = u16(data, offset + 28);
  const start = offset + 30 + nameLength + extraLength;
  need(bytes, start, record.entry.compressedSize, "compressed entry data");
  return bytes.subarray(start, start + record.entry.compressedSize);
}

function findEnd(
  bytes: Uint8Array,
  data: DataView,
): {
  readonly entries: number,
  readonly centralSize: number,
  readonly centralOffset: number,
} {
  for (
    let offset = bytes.length - 22;
    offset >= Math.max(0, bytes.length - 0xffff - 22);
    offset -= 1
  ) {
    if (u32(data, offset) !== END_OF_CENTRAL_DIRECTORY) continue;
    const entries = u16(data, offset + 10);
    const centralSize = u32(data, offset + 12);
    const centralOffset = u32(data, offset + 16);
    const commentLength = u16(data, offset + 20);
    if (offset + 22 + commentLength !== bytes.length) continue;
    if (
      u16(data, offset + 4) !== 0 ||
      u16(data, offset + 6) !== 0 ||
      entries !== u16(data, offset + 8) ||
      entries === 0xffff ||
      isZip64(centralSize) ||
      isZip64(centralOffset)
    ) {
      throw new Error("@uniflowed/std/zip: Zip64 and multi-disk archives are not supported");
    }
    need(bytes, centralOffset, centralSize, "central directory");
    return { entries, centralSize, centralOffset };
  }
  throw new Error("@uniflowed/std/zip: end of central directory not found");
}

function localHeader(entry: ZipEntry, name: Uint8Array): Uint8Array {
  const out = new Uint8Array(30);
  const data = view(out);
  put32(data, 0, LOCAL_FILE_HEADER);
  put16(data, 4, VERSION_NEEDED);
  put16(data, 6, UTF8_NAMES);
  put16(data, 8, methodForCompression(entry.compression));
  put32(data, 14, entry.crc32);
  put32(data, 18, entry.compressedSize);
  put32(data, 22, entry.size);
  put16(data, 26, name.length);
  return out;
}

function centralHeader(entry: ZipEntry, name: Uint8Array, localHeaderOffset: number): Uint8Array {
  const out = new Uint8Array(46 + name.length);
  const data = view(out);
  put32(data, 0, CENTRAL_FILE_HEADER);
  put16(data, 4, VERSION_NEEDED);
  put16(data, 6, VERSION_NEEDED);
  put16(data, 8, UTF8_NAMES);
  put16(data, 10, methodForCompression(entry.compression));
  put32(data, 16, entry.crc32);
  put32(data, 20, entry.compressedSize);
  put32(data, 24, entry.size);
  put16(data, 28, name.length);
  put32(data, 42, localHeaderOffset);
  out.set(name, 46);
  return out;
}

function endOfCentralDirectory(entries: number, size: number, offset: number): Uint8Array {
  const out = new Uint8Array(22);
  const data = view(out);
  put32(data, 0, END_OF_CENTRAL_DIRECTORY);
  put16(data, 8, entries);
  put16(data, 10, entries);
  put32(data, 12, size);
  put32(data, 16, offset);
  return out;
}

function concat(chunks: $ReadOnlyArray<Uint8Array>): Uint8Array {
  const length = chunks.reduce((sum, chunk) => sum + chunk.length, 0);
  const out = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    out.set(chunk, offset);
    offset += chunk.length;
  }
  return out;
}

function decodeName(bytes: Uint8Array, flags: number): string {
  if ((flags & UTF8_NAMES) === 0) {
    for (const byte of bytes) {
      if (byte > 0x7f) {
        throw new Error("@uniflowed/std/zip: non-UTF-8 entry names are not supported");
      }
    }
  }
  return DECODER.decode(bytes);
}

function checkedPath(path: string): string {
  if (
    path === "" ||
    path.includes("\0") ||
    path.includes("\\") ||
    path.startsWith("/") ||
    /^[A-Za-z]:/.test(path)
  ) {
    throw new Error(`@uniflowed/std/zip: unsafe entry path ${JSON.stringify(path)}`);
  }
  const parts = path.split("/");
  for (let index = 0; index < parts.length; index += 1) {
    const part = parts[index];
    const trailingDirectorySlash = index === parts.length - 1 && part === "";
    if (trailingDirectorySlash) continue;
    if (part === "" || part === "." || part === "..") {
      throw new Error(`@uniflowed/std/zip: unsafe entry path ${JSON.stringify(path)}`);
    }
  }
  return path;
}

function checkedCompression(compression: ZipCompression): ZipCompression {
  if (compression !== "store" && compression !== "deflate") {
    throw new Error(`@uniflowed/std/zip: unknown compression ${String(compression)}`);
  }
  return compression;
}

function compressionFromMethod(method: number): ZipCompression {
  if (method === 0) return "store";
  if (method === 8) return "deflate";
  throw new Error(`@uniflowed/std/zip: compression method ${String(method)} is not supported`);
}

function methodForCompression(compression: ZipCompression): number {
  return compression === "store" ? 0 : 8;
}

function defaultCompressionStream(format: "deflate-raw"): ByteTransform {
  return defaultTransform("CompressionStream", format);
}

function defaultDecompressionStream(format: "deflate-raw"): ByteTransform {
  return defaultTransform("DecompressionStream", format);
}

function defaultTransform(name: string, format: "deflate-raw"): ByteTransform {
  const globals: { readonly [string]: mixed } = globalThis;
  const constructor = globals[name];
  if (typeof constructor !== "function") {
    throw new Error(`@uniflowed/std/zip: ${name} is not available`);
  }
  const Transform = constructor as $FlowFixMe;
  return new Transform(format);
}

function view(bytes: Uint8Array): DataView {
  return new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
}

function need(bytes: Uint8Array, offset: number, length: number, label: string): void {
  if (offset < 0 || length < 0 || offset + length > bytes.length) {
    throw new Error(`@uniflowed/std/zip: truncated ${label}`);
  }
}

function u16(data: DataView, offset: number): number {
  return data.getUint16(offset, true);
}

function u32(data: DataView, offset: number): number {
  return data.getUint32(offset, true);
}

function put16(data: DataView, offset: number, value: number): void {
  if (value < 0 || value > 0xffff) {
    throw new RangeError("@uniflowed/std/zip: value does not fit in uint16");
  }
  data.setUint16(offset, value, true);
}

function put32(data: DataView, offset: number, value: number): void {
  if (value < 0 || value > 0xffffffff) {
    throw new RangeError("@uniflowed/std/zip: value does not fit in uint32");
  }
  data.setUint32(offset, value, true);
}

function isZip64(value: number): boolean {
  return value === 0xffffffff;
}

const LOCAL_FILE_HEADER = 0x04034b50;
const CENTRAL_FILE_HEADER = 0x02014b50;
const END_OF_CENTRAL_DIRECTORY = 0x06054b50;
const ENCRYPTED = 1 << 0;
const UTF8_NAMES = 1 << 11;
const VERSION_NEEDED = 20;
const ENCODER: TextEncoder = new TextEncoder();
const DECODER: TextDecoder = new TextDecoder();
