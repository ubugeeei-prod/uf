// @flow
//
// Every line here must be a type error. A libdef that typed the algorithm as
// `string` or the key as `any` would pass `web_crypto.js` and fail to catch a
// single one of these, which is why both fixtures exist.

async function misuses(key: CryptoKey, data: Uint8Array): Promise<void> {
  // An algorithm name that is not one. A typo in it is the whole reason the
  // parameter is a union of literals rather than `string`.
  await crypto.subtle.sign("HMAC-SHA256", key, data);

  // The dictionary form of the same mistake.
  await crypto.subtle.sign({ name: "HMACC" }, key, data);

  // `ECDSA` signs under a named hash, and the specification requires it.
  await crypto.subtle.sign({ name: "ECDSA" }, key, data);

  // A key is a `CryptoKey` and not the bytes it was imported from.
  await crypto.subtle.sign("HMAC", data, data);

  // `HMAC` needs the hash it is keyed for.
  await crypto.subtle.importKey("raw", data, { name: "HMAC" }, false, ["sign"]);

  // A key usage that is not one. `signing` is the word people reach for.
  await crypto.subtle.importKey("raw", data, { name: "HMAC", hash: "SHA-256" }, false, ["signing"]);

  // A key format that is not one.
  await crypto.subtle.importKey("pem", data, { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);

  // A digest that does not exist.
  await crypto.subtle.digest("SHA-999", data);

  // `extractable` is a boolean, and passing the usages in its place is the
  // argument-order mistake this signature exists to catch.
  await crypto.subtle.importKey("raw", data, { name: "HMAC", hash: "SHA-256" }, ["sign"], false);
}

export { misuses };
