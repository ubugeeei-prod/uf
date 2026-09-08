// @flow
//
// The Web Crypto a uf project actually calls. Every line here must check
// clean; `web_crypto_misuse.js` is the other half.

async function signExpiry(secret: Uint8Array, message: Uint8Array): Promise<ArrayBuffer> {
  const key = await crypto.subtle.importKey(
    "raw",
    secret,
    { name: "HMAC", hash: "SHA-256" },
    false,
    ["sign"],
  );
  return crypto.subtle.sign("HMAC", key, message);
}

async function verifyExpiry(key: CryptoKey, mac: Uint8Array, message: Uint8Array): Promise<boolean> {
  return crypto.subtle.verify({ name: "HMAC" }, key, mac, message);
}

// The one `bom.js` already had, which must not have been taken away.
async function hash(data: Uint8Array): Promise<ArrayBuffer> {
  return crypto.subtle.digest("SHA-256", data);
}

// Nor these two, which are on `Crypto` rather than on `subtle`.
function randomBytes(): Uint8Array {
  return crypto.getRandomValues(new Uint8Array(32));
}

function id(): string {
  return crypto.randomUUID();
}

// A key's own properties are readable and typed.
function usages(key: CryptoKey): Array<string> {
  return key.usages.map((usage) => `${usage} on a ${key.type} key`);
}

// The neighbours of the four the issue named, so the next module to reach for
// one does not find the same gap.
async function encrypt(key: CryptoKey, iv: Uint8Array, data: Uint8Array): Promise<ArrayBuffer> {
  return crypto.subtle.encrypt({ name: "AES-GCM", iv }, key, data);
}

async function derive(base: CryptoKey, salt: Uint8Array): Promise<ArrayBuffer> {
  return crypto.subtle.deriveBits(
    { name: "PBKDF2", hash: "SHA-256", salt, iterations: 100000 },
    base,
    256,
  );
}

async function generate(): Promise<CryptoKey | CryptoKeyPair> {
  return crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, ["sign", "verify"]);
}

async function exported(key: CryptoKey): Promise<ArrayBuffer | JsonWebKey> {
  return crypto.subtle.exportKey("jwk", key);
}

export { signExpiry, verifyExpiry, hash, randomBytes, id, usages, encrypt, derive, generate, exported };
