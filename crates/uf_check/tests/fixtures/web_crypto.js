// @flow
//
// The Web Crypto a uf application actually reaches for. Before uf shipped a
// libdef for it, the HMAC half of this file was four type errors, and
// `packages/server/internal/draft.js` carried them; see ubugeeei-prod/uf#619.

// --- What `packages/server/internal/oauth.js` does. -------------------------
//
// A bare string algorithm and a `Uint8Array`, which is the one shape `bom.js`
// already typed. It must keep working: a fix that broke PKCE would be a worse
// trade than the gap it closed.
async function pkceChallenge(verifier: string): Promise<ArrayBuffer> {
  return crypto.subtle.digest("SHA-256", new TextEncoder().encode(verifier));
}

function randomToken(count: number): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(count));
}

// --- What `packages/server/internal/context.js` does. -----------------------

const requestId: string = crypto.randomUUID();

// --- What `packages/server/internal/draft.js` does. -------------------------

function signingKey(secret: Uint8Array): Promise<CryptoKey> {
  return crypto.subtle.importKey("raw", secret, { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
}

async function signExpiry(secret: Uint8Array, message: Uint8Array): Promise<ArrayBuffer> {
  return crypto.subtle.sign("HMAC", await signingKey(secret), message);
}

// --- The rest of the surface, so a change to any of it fails here. ----------

async function verifying(key: CryptoKey, signature: Uint8Array, data: Uint8Array) {
  const ok: boolean = await crypto.subtle.verify("HMAC", key, signature, data);
  const ecdsa: boolean = await crypto.subtle.verify(
    { name: "ECDSA", hash: "SHA-384" },
    key,
    signature,
    data,
  );
  return ok && ecdsa;
}

async function symmetric(): Promise<ArrayBuffer> {
  const key: CryptoKey = await crypto.subtle.generateKey({ name: "AES-GCM", length: 256 }, true, [
    "encrypt",
    "decrypt",
  ]);
  const iv = crypto.getRandomValues(new Uint8Array(12));
  const sealed = await crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, new Uint8Array([1, 2]));
  return crypto.subtle.decrypt({ name: "AES-GCM", iv }, key, sealed);
}

async function asymmetric(): Promise<ArrayBuffer> {
  const pair: CryptoKeyPair = await crypto.subtle.generateKey(
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["sign", "verify"],
  );
  return crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, pair.privateKey, new Uint8Array(1));
}

async function derived(password: Uint8Array, salt: Uint8Array): Promise<CryptoKey> {
  const base = await crypto.subtle.importKey("raw", password, "PBKDF2", false, ["deriveKey"]);
  return crypto.subtle.deriveKey(
    { name: "PBKDF2", hash: "SHA-256", salt, iterations: 100000 },
    base,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt"],
  );
}

async function bits(base: CryptoKey, salt: Uint8Array, info: Uint8Array): Promise<ArrayBuffer> {
  return crypto.subtle.deriveBits({ name: "HKDF", hash: "SHA-256", salt, info }, base, 256);
}

async function jwk(key: CryptoKey): Promise<CryptoKey> {
  const exported: JsonWebKey = await crypto.subtle.exportKey("jwk", key);
  const raw: ArrayBuffer = await crypto.subtle.exportKey("raw", key);
  raw.byteLength;
  return crypto.subtle.importKey("jwk", exported, { name: "HMAC", hash: "SHA-256" }, true, [
    "sign",
  ]);
}

async function wrapping(key: CryptoKey, wrapper: CryptoKey): Promise<CryptoKey> {
  const wrapped = await crypto.subtle.wrapKey("raw", key, wrapper, "AES-KW");
  return crypto.subtle.unwrapKey(
    "raw",
    wrapped,
    wrapper,
    "AES-KW",
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
}

// A key describes itself, and the description is typed rather than `any`.
function describe(key: CryptoKey): string {
  const type: KeyType = key.type;
  const extractable: boolean = key.extractable;
  const usages: ReadonlyArray<KeyUsage> = key.usages;
  return `${key.algorithm.name} ${type} ${String(extractable)} ${String(usages.length)}`;
}

export {
  asymmetric,
  bits,
  derived,
  describe,
  jwk,
  pkceChallenge,
  randomToken,
  requestId,
  signExpiry,
  signingKey,
  symmetric,
  verifying,
  wrapping,
};
