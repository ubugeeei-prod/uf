// @flow
//
// `@uniflowed/std/tar`: ustar archive reading and writing.
//
// Tar is the other half of the archive gap in #710: the platform can compress
// byte streams, but it does not know how to frame a directory tree into 512
// byte records. This module intentionally stays at the container layer. It
// reads and writes regular files and directories from POSIX ustar archives and
// leaves gzip, zstd and other compression wrappers to the caller.

export type TarEntryKind = "file" | "directory";

export type TarEntry = {
  readonly path: string,
  readonly kind: TarEntryKind,
  readonly size: number,
  readonly mode: number,
};

type TarRecord = {
  readonly entry: TarEntry,
  readonly dataOffset: number,
};

type WriteRecord = {
  readonly entry: TarEntry,
  readonly data: Uint8Array,
};

export type TarEntryOptions = {
  readonly mode?: number,
  readonly kind?: TarEntryKind,
};

/** Read entries from an in-memory ustar archive. */
export class TarReader {
  _bytes: Uint8Array;
  _records: Array<TarRecord>;
  _byPath: Map<string, TarRecord>;

  constructor(bytes: Uint8Array) {
    this._bytes = bytes;
    this._records = readRecords(bytes);
    this._byPath = new Map();
    for (const record of this._records) {
      if (this._byPath.has(record.entry.path)) {
        throw new Error(`@uniflowed/std/tar: duplicate entry ${JSON.stringify(record.entry.path)}`);
      }
      this._byPath.set(record.entry.path, record);
    }
  }

  entries(): $ReadOnlyArray<TarEntry> {
    return this._records.map((record) => record.entry);
  }

  read(pathOrEntry: string | TarEntry): Uint8Array {
    const path = typeof pathOrEntry === "string" ? pathOrEntry : pathOrEntry.path;
    const record = this._byPath.get(path);
    if (record == null) {
      throw new Error(`@uniflowed/std/tar: entry not found: ${JSON.stringify(path)}`);
    }
    if (record.entry.kind !== "file") {
      throw new Error(`@uniflowed/std/tar: ${record.entry.path} is not a file`);
    }
    return this._bytes.subarray(record.dataOffset, record.dataOffset + record.entry.size).slice();
  }
}

/** Write a small in-memory ustar archive. */
export class TarWriter {
  _records: Array<WriteRecord>;

  constructor() {
    this._records = [];
  }

  add(path: string, bytes: Uint8Array, options?: TarEntryOptions): TarEntry {
    const kind = options?.kind ?? (path.endsWith("/") ? "directory" : "file");
    const checked = checkedPath(kind === "directory" && !path.endsWith("/") ? `${path}/` : path);
    if (this._records.some((record) => record.entry.path === checked)) {
      throw new Error(`@uniflowed/std/tar: duplicate entry ${JSON.stringify(checked)}`);
    }
    if (kind === "directory" && bytes.length !== 0) {
      throw new Error("@uniflowed/std/tar: directory entries cannot contain data");
    }
    const entry: TarEntry = {
      path: checked,
      kind,
      size: bytes.length,
      mode: checkedMode(options?.mode ?? (kind === "directory" ? 0o755 : 0o644)),
    };
    this._records.push({ entry, data: bytes.slice() });
    return entry;
  }

  bytes(): Uint8Array {
    const chunks: Array<Uint8Array> = [];
    for (const record of this._records) {
      chunks.push(header(record.entry), record.data, padding(record.data.length));
    }
    chunks.push(new Uint8Array(BLOCK_SIZE * 2));
    return concat(chunks);
  }
}

function readRecords(bytes: Uint8Array): Array<TarRecord> {
  const records: Array<TarRecord> = [];
  let offset = 0;
  while (offset < bytes.length) {
    need(bytes, offset, BLOCK_SIZE, "header");
    const block = bytes.subarray(offset, offset + BLOCK_SIZE);
    if (isZeroBlock(block)) {
      need(bytes, offset + BLOCK_SIZE, BLOCK_SIZE, "end blocks");
      if (!isZeroBlock(bytes.subarray(offset + BLOCK_SIZE, offset + BLOCK_SIZE * 2))) {
        throw new Error("@uniflowed/std/tar: archive is missing end blocks");
      }
      return records;
    }
    verifyChecksum(block, offset);
    const path = checkedPath(readPath(block));
    const kind = readKind(block[156]);
    const size = readOctal(block, 124, 12, "size");
    const mode = readOctal(block, 100, 8, "mode");
    const dataOffset = offset + BLOCK_SIZE;
    const dataLength = paddedSize(size);
    need(bytes, dataOffset, dataLength, "entry data");
    if (kind === "directory" && size !== 0) {
      throw new Error("@uniflowed/std/tar: directory entries cannot contain data");
    }
    records.push({ entry: { path, kind, size, mode }, dataOffset });
    offset = dataOffset + dataLength;
  }
  throw new Error("@uniflowed/std/tar: archive is missing end blocks");
}

function header(entry: TarEntry): Uint8Array {
  const out = new Uint8Array(BLOCK_SIZE);
  const [name, prefix] = splitPath(entry.path);
  writeText(out, 0, 100, name);
  writeOctal(out, 100, 8, entry.mode);
  writeOctal(out, 108, 8, 0);
  writeOctal(out, 116, 8, 0);
  writeOctal(out, 124, 12, entry.size);
  writeOctal(out, 136, 12, 0);
  for (let index = 148; index < 156; index += 1) out[index] = 0x20;
  out[156] = entry.kind === "directory" ? 0x35 : 0x30;
  writeText(out, 257, 6, "ustar");
  writeText(out, 263, 2, "00");
  writeText(out, 345, 155, prefix);
  writeChecksum(out);
  return out;
}

function readPath(block: Uint8Array): string {
  const name = readString(block, 0, 100);
  const prefix = readString(block, 345, 155);
  if (name === "") {
    throw new Error("@uniflowed/std/tar: empty entry path");
  }
  return prefix === "" ? name : `${prefix}/${name}`;
}

function splitPath(path: string): [string, string] {
  const bytes = ENCODER.encode(path);
  if (bytes.length <= 100) return [path, ""];
  if (bytes.length > 255) {
    throw new Error(`@uniflowed/std/tar: path is too long: ${JSON.stringify(path)}`);
  }
  const parts = path.split("/");
  for (let index = parts.length - 1; index > 0; index -= 1) {
    const prefix = parts.slice(0, index).join("/");
    const name = parts.slice(index).join("/");
    if (ENCODER.encode(prefix).length <= 155 && ENCODER.encode(name).length <= 100) {
      return [name, prefix];
    }
  }
  throw new Error(`@uniflowed/std/tar: path cannot fit in ustar header: ${JSON.stringify(path)}`);
}

function readKind(flag: number): TarEntryKind {
  if (flag === 0 || flag === 0x30) return "file";
  if (flag === 0x35) return "directory";
  throw new Error(`@uniflowed/std/tar: entry type ${String.fromCharCode(flag)} is not supported`);
}

function checkedPath(path: string): string {
  if (
    path === "" ||
    path.includes("\0") ||
    path.includes("\\") ||
    path.startsWith("/") ||
    /^[A-Za-z]:/.test(path)
  ) {
    throw new Error(`@uniflowed/std/tar: unsafe entry path ${JSON.stringify(path)}`);
  }
  const parts = path.split("/");
  for (let index = 0; index < parts.length; index += 1) {
    const part = parts[index];
    const trailingDirectorySlash = index === parts.length - 1 && part === "";
    if (trailingDirectorySlash) continue;
    if (part === "" || part === "." || part === "..") {
      throw new Error(`@uniflowed/std/tar: unsafe entry path ${JSON.stringify(path)}`);
    }
  }
  return path;
}

function checkedMode(mode: number): number {
  if (!Number.isSafeInteger(mode) || mode < 0 || mode > 0o7777) {
    throw new RangeError("@uniflowed/std/tar: mode must fit in four octal digits");
  }
  return mode;
}

function readOctal(block: Uint8Array, offset: number, length: number, label: string): number {
  const text = readString(block, offset, length).trim();
  if (text === "") return 0;
  if (!/^[0-7]+$/.test(text)) {
    throw new Error(`@uniflowed/std/tar: invalid octal ${label}`);
  }
  const value = Number.parseInt(text, 8);
  if (!Number.isSafeInteger(value)) {
    throw new Error(`@uniflowed/std/tar: ${label} is too large`);
  }
  return value;
}

function writeOctal(out: Uint8Array, offset: number, length: number, value: number): void {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new RangeError("@uniflowed/std/tar: octal field needs a non-negative integer");
  }
  const text = value.toString(8);
  if (text.length > length - 2) {
    throw new RangeError("@uniflowed/std/tar: octal field does not fit");
  }
  writeAscii(out, offset, length, `${text.padStart(length - 1, "0")}\0`);
}

function writeChecksum(block: Uint8Array): void {
  let sum = 0;
  for (const byte of block) sum += byte;
  writeAscii(block, 148, 8, `${sum.toString(8).padStart(6, "0")}\0 `);
}

function verifyChecksum(block: Uint8Array, offset: number): void {
  const expected = readOctal(block, 148, 8, "checksum");
  let actual = 0;
  for (let index = 0; index < block.length; index += 1) {
    actual += index >= 148 && index < 156 ? 0x20 : block[index];
  }
  if (actual !== expected) {
    throw new Error(`@uniflowed/std/tar: checksum mismatch at offset ${String(offset)}`);
  }
}

function readString(block: Uint8Array, offset: number, length: number): string {
  let end = offset;
  const limit = offset + length;
  while (end < limit && block[end] !== 0) end += 1;
  return DECODER.decode(block.subarray(offset, end));
}

function writeText(out: Uint8Array, offset: number, length: number, text: string): void {
  const bytes = ENCODER.encode(text);
  if (bytes.length > length) {
    throw new Error(`@uniflowed/std/tar: field is too long: ${JSON.stringify(text)}`);
  }
  out.set(bytes, offset);
}

function writeAscii(out: Uint8Array, offset: number, length: number, text: string): void {
  for (let index = 0; index < text.length; index += 1) {
    out[offset + index] = text.charCodeAt(index);
  }
  for (let index = text.length; index < length; index += 1) {
    out[offset + index] = 0;
  }
}

function padding(size: number): Uint8Array {
  return new Uint8Array((BLOCK_SIZE - (size % BLOCK_SIZE)) % BLOCK_SIZE);
}

function paddedSize(size: number): number {
  return size + ((BLOCK_SIZE - (size % BLOCK_SIZE)) % BLOCK_SIZE);
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

function need(bytes: Uint8Array, offset: number, length: number, label: string): void {
  if (offset < 0 || length < 0 || offset + length > bytes.length) {
    throw new Error(`@uniflowed/std/tar: truncated ${label}`);
  }
}

function isZeroBlock(block: Uint8Array): boolean {
  for (const byte of block) {
    if (byte !== 0) return false;
  }
  return true;
}

const BLOCK_SIZE = 512;
const ENCODER: TextEncoder = new TextEncoder();
const DECODER: TextDecoder = new TextDecoder();
