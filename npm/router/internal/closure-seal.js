// @noflow
//
// Synchronous ChaCha20-Poly1305 (RFC 8439) for an inline server closure.
//
// The seal has to be synchronous: WebCrypto's `subtle` is async, and awaiting
// it would make `bindServerClosure` a promise the component cannot return as a
// prop. `node:crypto` is synchronous and is not there on a worker, so this
// file is the cipher. It imports nothing. The build id is a key and never
// appears in an error, a log, or the blob's associated data — the action id
// does, so a blob sealed for one action is refused by another.

const REFUSED = "server action closure was refused";
const BUILD_ID_SHORT = "UF_BUILD_ID is missing or shorter than 8 bytes";
const BUILD_ID_LONG = "UF_BUILD_ID is longer than 256 bytes";
const P = (1n << 130n) - 5n;
const MOD_128 = (1n << 128n) - 1n;
const BASE64 = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

const SHA256_K = new Uint32Array([
  0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
  0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
  0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
  0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
  0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
  0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
  0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
  0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
]);

/** @param {Uint8Array} key @param {Uint8Array} nonce @param {Uint8Array} aad @param {Uint8Array} plaintext */
export function aeadSeal(key, nonce, aad, plaintext) {
  requireLength("key", key, 32);
  requireLength("nonce", nonce, 12);
  const ciphertext = chachaXor(key, 1, nonce, plaintext);
  const tag = poly1305(oneTimeKey(key, nonce), macData(aad, ciphertext));
  return { ciphertext, tag };
}

/**
 * Seal `plaintext` for one action. The result is standard base64 of
 * `nonce || ciphertext || tag` and does not contain the build id.
 *
 * @param {string} buildId
 * @param {string} actionId
 * @param {string} plaintext
 */
export function sealCaptures(buildId, actionId, plaintext) {
  const nonce = new Uint8Array(12);
  crypto.getRandomValues(nonce);
  return sealWithNonce(buildId, actionId, plaintext, nonce);
}

/**
 * `sealCaptures` with the nonce supplied, so a test can match another
 * implementation byte for byte.
 *
 * @param {string} buildId
 * @param {string} actionId
 * @param {string} plaintext
 * @param {Uint8Array} nonce
 */
export function sealWithNonce(buildId, actionId, plaintext, nonce) {
  const key = buildKey(buildId);
  requireLength("nonce", nonce, 12);
  const sealed = aeadSeal(key, nonce, utf8(String(actionId)), utf8(String(plaintext)));
  return encodeBase64(concat([nonce, sealed.ciphertext, sealed.tag]));
}

/**
 * The plaintext, or a refusal that names nothing about the blob or the key.
 *
 * A missing or over-long build id is a different error: the process is
 * misconfigured, and saying so does not publish the id.
 *
 * @param {string} buildId
 * @param {string} actionId
 * @param {string} sealed
 */
export function openCaptures(buildId, actionId, sealed) {
  const key = buildKey(buildId);
  const bytes = decodeBase64(sealed);
  if (bytes == null || bytes.length < 12 + 16) throw refused();
  const nonce = bytes.subarray(0, 12);
  const tag = bytes.subarray(bytes.length - 16);
  const ciphertext = bytes.subarray(12, bytes.length - 16);
  const aad = utf8(String(actionId));
  const expect = poly1305(oneTimeKey(key, nonce), macData(aad, ciphertext));
  if (!sameBytes(expect, tag)) throw refused();
  const plain = chachaXor(key, 1, nonce, ciphertext);
  try {
    return new TextDecoder("utf-8", { fatal: true }).decode(plain);
  } catch {
    throw refused();
  }
}

/** @param {string | undefined} buildId */
function buildKey(buildId) {
  if (typeof buildId !== "string" || buildId.length === 0) throw new Error(BUILD_ID_SHORT);
  const bytes = utf8(buildId);
  if (bytes.length < 8) throw new Error(BUILD_ID_SHORT);
  if (bytes.length > 256) throw new Error(BUILD_ID_LONG);
  return sha256(bytes);
}

function refused() {
  return new Error(REFUSED);
}

/** @param {Uint8Array} key @param {Uint8Array} nonce */
function oneTimeKey(key, nonce) {
  return chachaBlock(key, 0, nonce).subarray(0, 32);
}

/** @param {Uint8Array} aad @param {Uint8Array} ciphertext */
function macData(aad, ciphertext) {
  return concat([pad16(aad), pad16(ciphertext), le64(aad.length), le64(ciphertext.length)]);
}

/** @param {Uint8Array} data */
function pad16(data) {
  const extra = (16 - (data.length % 16)) % 16;
  if (extra === 0) return data;
  const out = new Uint8Array(data.length + extra);
  out.set(data);
  return out;
}

/** @param {number} length */
function le64(length) {
  const out = new Uint8Array(8);
  let value = BigInt(length);
  for (let index = 0; index < 8; index += 1) {
    out[index] = Number(value & 0xffn);
    value >>= 8n;
  }
  return out;
}

/**
 * @param {Uint8Array} key
 * @param {number} counter
 * @param {Uint8Array} nonce
 */
function chachaBlock(key, counter, nonce) {
  const state = new Uint32Array(16);
  state[0] = 0x61707865;
  state[1] = 0x3320646e;
  state[2] = 0x79622d32;
  state[3] = 0x6b206574;
  for (let index = 0; index < 8; index += 1) state[4 + index] = readU32(key, index * 4);
  state[12] = counter >>> 0;
  state[13] = readU32(nonce, 0);
  state[14] = readU32(nonce, 4);
  state[15] = readU32(nonce, 8);
  const working = state.slice();
  for (let round = 0; round < 10; round += 1) {
    quarter(working, 0, 4, 8, 12);
    quarter(working, 1, 5, 9, 13);
    quarter(working, 2, 6, 10, 14);
    quarter(working, 3, 7, 11, 15);
    quarter(working, 0, 5, 10, 15);
    quarter(working, 1, 6, 11, 12);
    quarter(working, 2, 7, 8, 13);
    quarter(working, 3, 4, 9, 14);
  }
  const out = new Uint8Array(64);
  for (let index = 0; index < 16; index += 1) {
    writeU32(out, index * 4, (working[index] + state[index]) >>> 0);
  }
  return out;
}

/** @param {Uint32Array} state @param {number} a @param {number} b @param {number} c @param {number} d */
function quarter(state, a, b, c, d) {
  state[a] = (state[a] + state[b]) >>> 0;
  state[d] = rotl(state[d] ^ state[a], 16);
  state[c] = (state[c] + state[d]) >>> 0;
  state[b] = rotl(state[b] ^ state[c], 12);
  state[a] = (state[a] + state[b]) >>> 0;
  state[d] = rotl(state[d] ^ state[a], 8);
  state[c] = (state[c] + state[d]) >>> 0;
  state[b] = rotl(state[b] ^ state[c], 7);
}

/** @param {number} value @param {number} bits */
function rotl(value, bits) {
  return ((value << bits) | (value >>> (32 - bits))) >>> 0;
}

/**
 * @param {Uint8Array} key
 * @param {number} counter
 * @param {Uint8Array} nonce
 * @param {Uint8Array} data
 */
function chachaXor(key, counter, nonce, data) {
  const out = new Uint8Array(data.length);
  let offset = 0;
  let block = counter >>> 0;
  while (offset < data.length) {
    const stream = chachaBlock(key, block, nonce);
    const take = Math.min(64, data.length - offset);
    for (let index = 0; index < take; index += 1)
      out[offset + index] = data[offset + index] ^ stream[index];
    offset += take;
    block = (block + 1) >>> 0;
  }
  return out;
}

/** @param {Uint8Array} key @param {Uint8Array} data */
function poly1305(key, data) {
  const r = clamped(key.subarray(0, 16));
  const s = little(key.subarray(16, 32));
  let acc = 0n;
  for (let offset = 0; offset < data.length; offset += 16) {
    const take = Math.min(16, data.length - offset);
    const block = new Uint8Array(take + 1);
    block.set(data.subarray(offset, offset + take));
    block[take] = 1;
    acc = ((acc + little(block)) * r) % P;
  }
  return bytesOf((acc + s) & MOD_128, 16);
}

/** @param {Uint8Array} bytes */
function clamped(bytes) {
  const copy = bytes.slice();
  copy[3] &= 15;
  copy[7] &= 15;
  copy[11] &= 15;
  copy[15] &= 15;
  copy[4] &= 252;
  copy[8] &= 252;
  copy[12] &= 252;
  return little(copy);
}

/** @param {Uint8Array} bytes */
function little(bytes) {
  let value = 0n;
  for (let index = bytes.length - 1; index >= 0; index -= 1) {
    value = (value << 8n) | BigInt(bytes[index]);
  }
  return value;
}

/** @param {bigint} value @param {number} length */
function bytesOf(value, length) {
  const out = new Uint8Array(length);
  let rest = value;
  for (let index = 0; index < length; index += 1) {
    out[index] = Number(rest & 0xffn);
    rest >>= 8n;
  }
  return out;
}

/** @param {Uint8Array} bytes */
function sha256(bytes) {
  const bitLength = BigInt(bytes.length) * 8n;
  const withBit = bytes.length + 1;
  const zeroPad = (64 + 56 - (withBit % 64)) % 64;
  const padded = new Uint8Array(withBit + zeroPad + 8);
  padded.set(bytes);
  padded[bytes.length] = 0x80;
  const view = new DataView(padded.buffer);
  view.setUint32(padded.length - 8, Number((bitLength >> 32n) & 0xffffffffn));
  view.setUint32(padded.length - 4, Number(bitLength & 0xffffffffn));

  const hash = new Uint32Array([
    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab, 0x5be0cd19,
  ]);
  const words = new Uint32Array(64);
  for (let offset = 0; offset < padded.length; offset += 64) {
    for (let index = 0; index < 16; index += 1) words[index] = view.getUint32(offset + index * 4);
    for (let index = 16; index < 64; index += 1) {
      const s0 =
        rotr(words[index - 15], 7) ^ rotr(words[index - 15], 18) ^ (words[index - 15] >>> 3);
      const s1 =
        rotr(words[index - 2], 17) ^ rotr(words[index - 2], 19) ^ (words[index - 2] >>> 10);
      words[index] = (words[index - 16] + s0 + words[index - 7] + s1) >>> 0;
    }
    let a = hash[0];
    let b = hash[1];
    let c = hash[2];
    let d = hash[3];
    let e = hash[4];
    let f = hash[5];
    let g = hash[6];
    let h = hash[7];
    for (let index = 0; index < 64; index += 1) {
      const s1 = rotr(e, 6) ^ rotr(e, 11) ^ rotr(e, 25);
      const ch = (e & f) ^ (~e & g);
      const temp1 = (h + s1 + ch + SHA256_K[index] + words[index]) >>> 0;
      const s0 = rotr(a, 2) ^ rotr(a, 13) ^ rotr(a, 22);
      const maj = (a & b) ^ (a & c) ^ (b & c);
      const temp2 = (s0 + maj) >>> 0;
      h = g;
      g = f;
      f = e;
      e = (d + temp1) >>> 0;
      d = c;
      c = b;
      b = a;
      a = (temp1 + temp2) >>> 0;
    }
    hash[0] = (hash[0] + a) >>> 0;
    hash[1] = (hash[1] + b) >>> 0;
    hash[2] = (hash[2] + c) >>> 0;
    hash[3] = (hash[3] + d) >>> 0;
    hash[4] = (hash[4] + e) >>> 0;
    hash[5] = (hash[5] + f) >>> 0;
    hash[6] = (hash[6] + g) >>> 0;
    hash[7] = (hash[7] + h) >>> 0;
  }
  const out = new Uint8Array(32);
  const outView = new DataView(out.buffer);
  for (let index = 0; index < 8; index += 1) outView.setUint32(index * 4, hash[index]);
  return out;
}

/** @param {number} value @param {number} bits */
function rotr(value, bits) {
  return ((value >>> bits) | (value << (32 - bits))) >>> 0;
}

/** @param {Uint8Array} bytes @param {number} offset */
function readU32(bytes, offset) {
  return (
    (bytes[offset] |
      (bytes[offset + 1] << 8) |
      (bytes[offset + 2] << 16) |
      (bytes[offset + 3] << 24)) >>>
    0
  );
}

/** @param {Uint8Array} bytes @param {number} offset @param {number} value */
function writeU32(bytes, offset, value) {
  bytes[offset] = value & 0xff;
  bytes[offset + 1] = (value >>> 8) & 0xff;
  bytes[offset + 2] = (value >>> 16) & 0xff;
  bytes[offset + 3] = (value >>> 24) & 0xff;
}

/** @param {string} text */
function utf8(text) {
  return new TextEncoder().encode(text);
}

/** @param {Array<Uint8Array>} parts */
function concat(parts) {
  let size = 0;
  for (const part of parts) size += part.length;
  const out = new Uint8Array(size);
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

/** @param {Uint8Array} bytes */
function encodeBase64(bytes) {
  let out = "";
  for (let index = 0; index < bytes.length; index += 3) {
    const b0 = bytes[index];
    const b1 = index + 1 < bytes.length ? bytes[index + 1] : 0;
    const b2 = index + 2 < bytes.length ? bytes[index + 2] : 0;
    const triple = (b0 << 16) | (b1 << 8) | b2;
    out += BASE64[(triple >> 18) & 63];
    out += BASE64[(triple >> 12) & 63];
    out += index + 1 < bytes.length ? BASE64[(triple >> 6) & 63] : "=";
    out += index + 2 < bytes.length ? BASE64[triple & 63] : "=";
  }
  return out;
}

/** @param {string} text */
function decodeBase64(text) {
  if (typeof text !== "string" || text.length === 0 || text.length % 4 !== 0) return null;
  const out = [];
  for (let index = 0; index < text.length; index += 4) {
    const c0 = BASE64.indexOf(text[index]);
    const c1 = BASE64.indexOf(text[index + 1]);
    const pad2 = text[index + 2] === "=";
    const pad3 = text[index + 3] === "=";
    const c2 = pad2 ? 0 : BASE64.indexOf(text[index + 2]);
    const c3 = pad3 ? 0 : BASE64.indexOf(text[index + 3]);
    if (c0 < 0 || c1 < 0 || c2 < 0 || c3 < 0) return null;
    if ((pad2 || pad3) && index + 4 !== text.length) return null;
    if (pad2 && !pad3) return null;
    const triple = (c0 << 18) | (c1 << 12) | (c2 << 6) | c3;
    out.push((triple >> 16) & 255);
    if (!pad2) out.push((triple >> 8) & 255);
    if (!pad3) out.push(triple & 255);
  }
  return Uint8Array.from(out);
}

/** @param {Uint8Array} left @param {Uint8Array} right */
function sameBytes(left, right) {
  if (left.length !== right.length) return false;
  let diff = 0;
  for (let index = 0; index < left.length; index += 1) diff |= left[index] ^ right[index];
  return diff === 0;
}

/** @param {string} name @param {Uint8Array} bytes @param {number} length */
function requireLength(name, bytes, length) {
  if (!(bytes instanceof Uint8Array) || bytes.length !== length) {
    throw new Error(`${name} must be ${String(length)} bytes`);
  }
}
