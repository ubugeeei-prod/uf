// @flow
//
// `@uniflowed/std/binary`: Go's `encoding/binary`, over `DataView`.
//
// JavaScript has the byte-order primitives already: `DataView#getUint16`,
// `setUint32`, `getFloat64`, and the rest. What it does not have is the piece
// every binary protocol parser writes around them: a cursor that advances only
// after a checked read or write, plus Go-compatible varints.
//
// This module is deliberately a thin layer. A `Cursor` owns no memory and does
// no copies unless the caller asks to write bytes from another buffer. Reads
// return numbers where `DataView` does, varints return `bigint`, and every
// bounds failure points at the offset that could not be read or written.

/** Byte order for multi-byte numbers. */
export type ByteOrder = "big" | "little";

/** Big-endian byte order, matching `DataView`'s default and Go's `BigEndian`. */
export const BIG_ENDIAN: ByteOrder = "big";

/** Little-endian byte order, matching Go's `LittleEndian`. */
export const LITTLE_ENDIAN: ByteOrder = "little";

/** Options for a cursor over a byte buffer. */
export type CursorOptions = {
  readonly byteOrder?: ByteOrder,
};

/** A varint decoded from the start of a byte slice. */
export type VarintResult = {
  readonly value: bigint,
  readonly read: number,
};

/** A malformed binary value. */
export class InvalidBinaryError extends Error {
  offset: number;

  constructor(message: string, offset: number) {
    super(message);
    this.name = "InvalidBinaryError";
    this.offset = offset;
  }
}

/** A checked cursor over a `Uint8Array`. */
export class Cursor {
  _bytes: Uint8Array;
  _view: DataView;
  _offset: number;
  _byteOrder: ByteOrder;

  constructor(bytes: Uint8Array, options?: CursorOptions) {
    this._bytes = bytes;
    this._view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    this._offset = 0;
    this._byteOrder = checkedByteOrder(options?.byteOrder ?? BIG_ENDIAN);
  }

  /** The underlying bytes. Writes through the cursor are visible here. */
  bytes(): Uint8Array {
    return this._bytes;
  }

  /** Current offset in bytes. */
  offset(): number {
    return this._offset;
  }

  /** Bytes left between the current offset and the end of the buffer. */
  remaining(): number {
    return this._bytes.length - this._offset;
  }

  /** Move to an absolute byte offset. */
  seek(offset: number): void {
    assertInteger(offset, "offset", 0, this._bytes.length);
    this._offset = offset;
  }

  /** Move forward by `length` bytes. */
  skip(length: number): void {
    this._offset = this._claim(length);
  }

  /** Return a view of the next `length` bytes and advance past it. */
  getBytes(length: number): Uint8Array {
    const offset = this._claim(length);
    return this._bytes.subarray(offset, offset + length);
  }

  /** Copy `bytes` into the cursor and advance past them. */
  putBytes(bytes: Uint8Array): void {
    const offset = this._claim(bytes.length);
    this._bytes.set(bytes, offset);
  }

  getUint8(): number {
    const offset = this._claim(1);
    return this._view.getUint8(offset);
  }

  putUint8(value: number): void {
    assertInteger(value, "uint8", 0, 0xff);
    const offset = this._claim(1);
    this._view.setUint8(offset, value);
  }

  getInt8(): number {
    const offset = this._claim(1);
    return this._view.getInt8(offset);
  }

  putInt8(value: number): void {
    assertInteger(value, "int8", -0x80, 0x7f);
    const offset = this._claim(1);
    this._view.setInt8(offset, value);
  }

  getUint16(byteOrder?: ByteOrder): number {
    const offset = this._claim(2);
    return this._view.getUint16(offset, this._little(byteOrder));
  }

  putUint16(value: number, byteOrder?: ByteOrder): void {
    assertInteger(value, "uint16", 0, 0xffff);
    const offset = this._claim(2);
    this._view.setUint16(offset, value, this._little(byteOrder));
  }

  getInt16(byteOrder?: ByteOrder): number {
    const offset = this._claim(2);
    return this._view.getInt16(offset, this._little(byteOrder));
  }

  putInt16(value: number, byteOrder?: ByteOrder): void {
    assertInteger(value, "int16", -0x8000, 0x7fff);
    const offset = this._claim(2);
    this._view.setInt16(offset, value, this._little(byteOrder));
  }

  getUint32(byteOrder?: ByteOrder): number {
    const offset = this._claim(4);
    return this._view.getUint32(offset, this._little(byteOrder));
  }

  putUint32(value: number, byteOrder?: ByteOrder): void {
    assertInteger(value, "uint32", 0, 0xffffffff);
    const offset = this._claim(4);
    this._view.setUint32(offset, value, this._little(byteOrder));
  }

  getInt32(byteOrder?: ByteOrder): number {
    const offset = this._claim(4);
    return this._view.getInt32(offset, this._little(byteOrder));
  }

  putInt32(value: number, byteOrder?: ByteOrder): void {
    assertInteger(value, "int32", -0x80000000, 0x7fffffff);
    const offset = this._claim(4);
    this._view.setInt32(offset, value, this._little(byteOrder));
  }

  getFloat32(byteOrder?: ByteOrder): number {
    const offset = this._claim(4);
    return this._view.getFloat32(offset, this._little(byteOrder));
  }

  putFloat32(value: number, byteOrder?: ByteOrder): void {
    const offset = this._claim(4);
    this._view.setFloat32(offset, value, this._little(byteOrder));
  }

  getFloat64(byteOrder?: ByteOrder): number {
    const offset = this._claim(8);
    return this._view.getFloat64(offset, this._little(byteOrder));
  }

  putFloat64(value: number, byteOrder?: ByteOrder): void {
    const offset = this._claim(8);
    this._view.setFloat64(offset, value, this._little(byteOrder));
  }

  /** Decode an unsigned varint at the current offset. */
  getUvarint(): bigint {
    const result = uvarint(this._bytes.subarray(this._offset));
    this._offset += result.read;
    return result.value;
  }

  /** Encode an unsigned varint at the current offset. */
  putUvarint(value: bigint): void {
    this.putBytes(putUvarint(value));
  }

  /** Decode a signed varint at the current offset. */
  getVarint(): bigint {
    const result = varint(this._bytes.subarray(this._offset));
    this._offset += result.read;
    return result.value;
  }

  /** Encode a signed varint at the current offset. */
  putVarint(value: bigint): void {
    this.putBytes(putVarint(value));
  }

  _claim(length: number): number {
    assertInteger(length, "length", 0, Number.MAX_SAFE_INTEGER);
    const offset = this._offset;
    const next = offset + length;
    if (next > this._bytes.length) {
      throw new RangeError(
        `@uniflowed/std/binary: need ${String(length)} byte(s) at offset ${String(
          offset,
        )}, but only ${String(this._bytes.length - offset)} remain`,
      );
    }
    this._offset = next;
    return offset;
  }

  _little(byteOrder?: ByteOrder): boolean {
    return checkedByteOrder(byteOrder ?? this._byteOrder) === LITTLE_ENDIAN;
  }
}

/** Decode an unsigned LEB128 varint from the start of `bytes`. */
export function uvarint(bytes: Uint8Array): VarintResult {
  let value = 0n;
  let shift = 0n;
  for (let index = 0; index < bytes.length; index += 1) {
    const byte = bytes[index];
    if (byte < 0x80) {
      return { value: value | (BigInt(byte) << shift), read: index + 1 };
    }
    value |= BigInt(byte & 0x7f) << shift;
    shift += 7n;
  }
  throw new InvalidBinaryError("@uniflowed/std/binary: truncated uvarint", bytes.length);
}

/** Encode an unsigned LEB128 varint. */
export function putUvarint(value: bigint): Uint8Array {
  assertUnsignedBigInt(value, "uvarint");
  const out = new Uint8Array(uvarintLength(value));
  let current = value;
  let index = 0;
  while (current >= 0x80n) {
    out[index] = Number(current & 0x7fn) | 0x80;
    current >>= 7n;
    index += 1;
  }
  out[index] = Number(current);
  return out;
}

/** Number of bytes needed to encode `value` as an unsigned varint. */
export function uvarintLength(value: bigint): number {
  assertUnsignedBigInt(value, "uvarint");
  let current = value;
  let length = 1;
  while (current >= 0x80n) {
    current >>= 7n;
    length += 1;
  }
  return length;
}

/** Decode a signed Go-style zig-zag varint from the start of `bytes`. */
export function varint(bytes: Uint8Array): VarintResult {
  const result = uvarint(bytes);
  const half = result.value >> 1n;
  return {
    value: (result.value & 1n) === 0n ? half : -(half + 1n),
    read: result.read,
  };
}

/** Encode a signed Go-style zig-zag varint. */
export function putVarint(value: bigint): Uint8Array {
  return putUvarint(zigZag(value));
}

/** Number of bytes needed to encode `value` as a signed varint. */
export function varintLength(value: bigint): number {
  return uvarintLength(zigZag(value));
}

function zigZag(value: bigint): bigint {
  return value < 0n ? (-value << 1n) - 1n : value << 1n;
}

function checkedByteOrder(byteOrder: ByteOrder): ByteOrder {
  if (byteOrder !== BIG_ENDIAN && byteOrder !== LITTLE_ENDIAN) {
    throw new RangeError(
      `@uniflowed/std/binary: byte order must be "big" or "little", got ${String(byteOrder)}`,
    );
  }
  return byteOrder;
}

function assertUnsignedBigInt(value: bigint, name: string): void {
  if (value < 0n) {
    throw new RangeError(`@uniflowed/std/binary: ${name} must be non-negative`);
  }
}

function assertInteger(value: number, name: string, min: number, max: number): void {
  if (!Number.isSafeInteger(value) || value < min || value > max) {
    throw new RangeError(
      `@uniflowed/std/binary: ${name} must be an integer in [${String(min)}, ${String(
        max,
      )}], got ${String(value)}`,
    );
  }
}
