// @flow
//
// Every line marked `// misuse:` must be reported. A line without one must
// not be, so a libdef that widened something on the way to fixing this file's
// neighbour fails here rather than in a user's project a month later.
//
// This is the half of ubugeeei-prod/uf#619 that says the declarations are
// *typed*: `subtle: any` would make `fixtures/web_crypto.js` pass and every
// line below pass with it.

/* eslint-disable no-unused-vars */

async function misuses(
  key: CryptoKey,
  pair: CryptoKeyPair,
  secret: Uint8Array,
  signature: Uint8Array,
  salt: Uint8Array,
  info: Uint8Array,
) {
  // misuse: a hash that is not one of the four the specification defines
  await crypto.subtle.digest("SHA-224", secret);

  // misuse: signing with the raw secret rather than with an imported key
  await crypto.subtle.sign("HMAC", secret, secret);

  // misuse: an HMAC import with no hash, which the specification requires
  await crypto.subtle.importKey("raw", secret, { name: "HMAC" }, false, ["sign"]);

  // misuse: an algorithm object whose `name` is not an algorithm
  await crypto.subtle.importKey("raw", secret, { name: "HMAC-SHA256" }, false, ["sign"]);

  // misuse: a key usage that is a typo for one
  await crypto.subtle.importKey("raw", secret, "HKDF", false, ["singn"]);

  // misuse: `saltLength` handed to ECDSA, which has no such parameter
  await crypto.subtle.sign({ name: "ECDSA", saltLength: 32 }, key, secret);

  // misuse: `digest` handed a key instead of bytes
  await crypto.subtle.digest("SHA-256", key);

  // misuse: `exportKey("jwk")` read as bytes
  const asBytes: ArrayBuffer = await crypto.subtle.exportKey("jwk", key);

  // misuse: `exportKey("raw")` read as JSON
  const asJson: JsonWebKey = await crypto.subtle.exportKey("raw", key);

  // misuse: a symmetric `generateKey` read as a pair
  const asPair: CryptoKeyPair = await crypto.subtle.generateKey(
    { name: "AES-GCM", length: 256 },
    true,
    ["encrypt"],
  );

  // misuse: `deriveBits` handed the derivation for a different algorithm
  await crypto.subtle.deriveBits({ name: "PBKDF2", hash: "SHA-256", salt, info }, key, 256);

  // misuse: an AES key length the specification does not define
  await crypto.subtle.generateKey({ name: "AES-GCM", length: 200 }, true, ["encrypt"]);

  // misuse: an EC curve that does not exist
  await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-224" }, true, ["sign"]);

  // misuse: `getRandomValues` handed something that is not a typed array
  crypto.getRandomValues("thirty-two bytes, honestly");

  // misuse: a key's usages copied into a list somebody could push onto
  const mutableUsages: Array<KeyUsage> = key.usages;

  // And the same calls written correctly, so that a libdef which simply
  // refused everything would fail this file too.
  await crypto.subtle.digest("SHA-256", secret);
  await crypto.subtle.sign("HMAC", key, secret);
  await crypto.subtle.importKey("raw", secret, { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  await crypto.subtle.verify("HMAC", key, signature, secret);
  await crypto.subtle.sign({ name: "ECDSA", hash: "SHA-256" }, pair.privateKey, secret);
  crypto.getRandomValues(new Uint8Array(4));

  return [asBytes, asJson, asPair, mutableUsages];
}

export { misuses };
