// @flow
//
// The seal on an inline server closure's captured variables.
//
// Two independent checks, because either one alone can be wrong in the same
// way as the code. The first is RFC 8439 §2.8.2, which does not go through
// Node. The second is Node's own `chacha20-poly1305`, which is what a worker
// cannot call and what this file therefore reimplements. A secret sealed for
// one action must not be readable as text, and a flipped byte or a different
// action id is the same refusal.

import { createCipheriv, createHash } from "node:crypto";
import { describe, expect, it } from "@uniflowed/test";

import { aeadSeal, openCaptures, sealCaptures, sealWithNonce } from "../../npm/router/internal/closure-seal.js";

const RFC_KEY = hex(
  "808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f",
);
const RFC_NONCE = hex("070000004041424344454647");
const RFC_AAD = hex("50515253c0c1c2c3c4c5c6c7");
const RFC_PLAIN = new TextEncoder().encode(
  "Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.",
);
const RFC_CIPHERTEXT = hex(
  "d31a8d34648e60db7b86afbc53ef7ec2" +
    "a4aded51296e08fea9e2b5a736ee62d6" +
    "3dbea45e8ca9671282fafb69da92728b" +
    "1a71de0a9e060b2905d6a5b67ecd3b36" +
    "92ddbd7f2d778b8c9803aee328091b58" +
    "fab324e4fad675945585808b4831d7bc" +
    "3ff4def08e4b7a9de576d26586cec64b" +
    "6116",
);
const RFC_TAG = hex("1ae10b594f09e26a7e902ecbd0600691");

const BUILD_ID = "build-id-for-tests";
const ACTION_ID = "ab".repeat(32);
const SECRET = "super-secret-capture-value";

function hex(text: string): Uint8Array {
  const bytes = new Uint8Array(text.length / 2);
  for (let index = 0; index < bytes.length; index += 1) {
    bytes[index] = Number.parseInt(text.slice(index * 2, index * 2 + 2), 16);
  }
  return bytes;
}

function toHex(bytes: Uint8Array): string {
  return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
}

describe("an inline closure's captures", () => {
  it("matches the RFC 8439 AEAD vector", () => {
    const sealed = aeadSeal(RFC_KEY, RFC_NONCE, RFC_AAD, RFC_PLAIN);
    expect(toHex(sealed.ciphertext)).toBe(toHex(RFC_CIPHERTEXT));
    expect(toHex(sealed.tag)).toBe(toHex(RFC_TAG));
  });

  it("matches node:crypto for the same key, nonce and action id", () => {
    const plaintext = JSON.stringify({ args: [SECRET] });
    const nonce = Uint8Array.from({ length: 12 }, (_, index) => index + 1);
    const sealed = sealWithNonce(BUILD_ID, ACTION_ID, plaintext, nonce);
    const key = createHash("sha256").update(BUILD_ID).digest();
    const cipher = createCipheriv("chacha20-poly1305", key, Buffer.from(nonce), {
      authTagLength: 16,
    });
    cipher.setAAD(Buffer.from(ACTION_ID));
    const ciphertext = Buffer.concat([cipher.update(plaintext, "utf8"), cipher.final()]);
    const tag = cipher.getAuthTag();
    const expected = Buffer.concat([Buffer.from(nonce), ciphertext, tag]).toString("base64");
    expect(sealed).toBe(expected);
    expect(openCaptures(BUILD_ID, ACTION_ID, sealed)).toBe(plaintext);
  });

  it("does not carry the captured value in the clear", () => {
    const sealed = sealCaptures(BUILD_ID, ACTION_ID, SECRET);
    expect(sealed.includes(SECRET)).toBe(false);
    expect(sealed.includes(BUILD_ID)).toBe(false);
    expect(openCaptures(BUILD_ID, ACTION_ID, sealed)).toBe(SECRET);
  });

  it("refuses a flipped byte and a blob sealed for another action", () => {
    const sealed = sealCaptures(BUILD_ID, ACTION_ID, SECRET);
    const flipped = sealed.split("");
    const at = Math.min(20, flipped.length - 1);
    flipped[at] = flipped[at] === "A" ? "B" : "A";
    expect(() => openCaptures(BUILD_ID, ACTION_ID, flipped.join(""))).toThrow(
      "server action closure was refused",
    );
    expect(() => openCaptures(BUILD_ID, "cd".repeat(32), sealed)).toThrow(
      "server action closure was refused",
    );
    try {
      openCaptures(BUILD_ID, ACTION_ID, flipped.join(""));
    } catch (error) {
      const message = error instanceof Error ? error.message : "";
      expect(message.includes(SECRET)).toBe(false);
      expect(message.includes(BUILD_ID)).toBe(false);
    }
  });

  it("names a missing or over-long build id without repeating it", () => {
    expect(() => sealCaptures("short", ACTION_ID, SECRET)).toThrow(
      "UF_BUILD_ID is missing or shorter than 8 bytes",
    );
    expect(() => sealCaptures("", ACTION_ID, SECRET)).toThrow(
      "UF_BUILD_ID is missing or shorter than 8 bytes",
    );
    const long = "x".repeat(257);
    expect(() => sealCaptures(long, ACTION_ID, SECRET)).toThrow(
      "UF_BUILD_ID is longer than 256 bytes",
    );
  });
});
