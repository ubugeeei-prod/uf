/**
 * @fileoverview Web Crypto, of which the vendored `bom.js` declares one method.
 *
 * Flow's `evals/flow-typed/environment/bom.js` types `crypto.subtle` as an
 * inline object with `digest` in it and nothing else. So a module that only
 * hashes checks clean, and a module that signs — `packages/server/internal/
 * draft.js`, which HMACs the draft-mode cookie so a holder cannot extend its
 * own expiry — reported four errors for calls that are correct and that every
 * runtime uf targets implements. There is no `CryptoKey` in that file at all,
 * so even the annotation on the key was unresolvable. See
 * ubugeeei-prod/uf#619, found by #615, which took the errors rather than
 * hand-rolling HMAC out of `digest` to satisfy a checker.
 *
 * # How this replaces the vendored one
 *
 * Library definitions are merged in reverse declaration order, so a later
 * file's `Crypto` shadows an earlier one's — the same mechanism
 * `vite-client.js` uses for `Import$Meta`, and the reason `ENVIRONMENTS` lists
 * uf's own libdefs last. `Crypto` is therefore redeclared **whole** rather than
 * extended: `getRandomValues` and `randomUUID` are repeated here exactly as
 * `bom.js` has them, because a shadow that dropped them would trade four
 * errors for two.
 *
 * # Why the algorithm parameters are spelled out
 *
 * A declaration that typed the algorithm as `string` or the key as `any` would
 * close the issue and be worse than leaving it open: it would accept
 * `sign({ name: "HMAC" }, key, data)` with the hash missing, and `importKey`
 * with a `usages` array containing a word that is not a key usage. The shapes
 * below are the specification's, so a wrong algorithm object is still an
 * error, while every algorithm a uf project would reach for is expressible.
 *
 * The object forms are inexact. The specification's dictionaries inherit from
 * `Algorithm`, and an implementation is free to accept members this file does
 * not name; the closed thing here is the discriminating `name`, which is what
 * makes a typo in it an error.
 */

/** The digests Web Crypto names, as `SubtleCrypto` spells them. */
type HashAlgorithmName = "SHA-1" | "SHA-256" | "SHA-384" | "SHA-512";

/** A hash, named directly or wrapped in the dictionary form. */
type HashAlgorithmIdentifier = HashAlgorithmName | { name: HashAlgorithmName, ... };

/** The curves `ECDSA` and `ECDH` accept. */
type NamedCurve = "P-256" | "P-384" | "P-521";

/** What a key may be used for. A key imported for `sign` cannot `wrapKey`. */
type CryptoKeyUsage =
  | "encrypt"
  | "decrypt"
  | "sign"
  | "verify"
  | "deriveKey"
  | "deriveBits"
  | "wrapKey"
  | "unwrapKey";

/** The serialisations `importKey` and `exportKey` speak. */
type CryptoKeyFormat = "raw" | "pkcs8" | "spki" | "jwk";

/** Which half of a pair a key is, or that it is symmetric. */
type CryptoKeyType = "public" | "private" | "secret";

/** A JSON Web Key, as `importKey("jwk", …)` takes it. */
type JsonWebKey = {
  alg?: string,
  crv?: string,
  d?: string,
  e?: string,
  ext?: boolean,
  k?: string,
  key_ops?: Array<string>,
  kty?: string,
  n?: string,
  use?: string,
  x?: string,
  y?: string,
  ...
};

/** What `CryptoKey.algorithm` reports: always a name, sometimes more. */
type KeyAlgorithm = { name: string, ... };

/**
 * An opaque handle to key material.
 *
 * Not constructible: a `CryptoKey` comes from `importKey`, `generateKey`,
 * `deriveKey` or `unwrapKey`, which is the property that keeps the bytes of an
 * unextractable key out of the program that uses it.
 */
declare class CryptoKey {
  +algorithm: KeyAlgorithm;
  +extractable: boolean;
  +type: CryptoKeyType;
  +usages: Array<CryptoKeyUsage>;
}

/** What `generateKey` returns for an asymmetric algorithm. */
type CryptoKeyPair = { privateKey: CryptoKey, publicKey: CryptoKey, ... };

/** The algorithms `sign` and `verify` accept. */
type SignatureAlgorithmIdentifier =
  | "HMAC"
  | "Ed25519"
  | { name: "HMAC", ... }
  | { name: "Ed25519", ... }
  | { name: "RSASSA-PKCS1-v1_5", ... }
  | { name: "RSA-PSS", saltLength: number, ... }
  | { name: "ECDSA", hash: HashAlgorithmIdentifier, ... };

/** The algorithms `encrypt` and `decrypt` accept. */
type EncryptionAlgorithmIdentifier =
  | { name: "AES-GCM", iv: BufferSource, additionalData?: BufferSource, tagLength?: number, ... }
  | { name: "AES-CBC", iv: BufferSource, ... }
  | { name: "AES-CTR", counter: BufferSource, length: number, ... }
  | { name: "RSA-OAEP", label?: BufferSource, ... };

/** The algorithms `importKey`, `generateKey` and `unwrapKey` accept. */
type KeyAlgorithmIdentifier =
  | "PBKDF2"
  | "HKDF"
  | "Ed25519"
  | "X25519"
  | { name: "HMAC", hash: HashAlgorithmIdentifier, length?: number, ... }
  | { name: "AES-GCM" | "AES-CBC" | "AES-CTR" | "AES-KW", length?: number, ... }
  | { name: "PBKDF2" | "HKDF", ... }
  | { name: "Ed25519" | "X25519", ... }
  | { name: "ECDSA" | "ECDH", namedCurve: NamedCurve, ... }
  | {
      name: "RSASSA-PKCS1-v1_5" | "RSA-PSS" | "RSA-OAEP",
      hash: HashAlgorithmIdentifier,
      modulusLength?: number,
      publicExponent?: Uint8Array,
      ...
    };

/** The algorithms `deriveKey` and `deriveBits` accept. */
type DerivationAlgorithmIdentifier =
  | { name: "PBKDF2", hash: HashAlgorithmIdentifier, salt: BufferSource, iterations: number, ... }
  | { name: "HKDF", hash: HashAlgorithmIdentifier, salt: BufferSource, info: BufferSource, ... }
  | { name: "ECDH" | "X25519", public: CryptoKey, ... };

/** The algorithms `digest` accepts. */
type DigestAlgorithmIdentifier = HashAlgorithmIdentifier;

declare interface SubtleCrypto {
  decrypt(
    algorithm: EncryptionAlgorithmIdentifier,
    key: CryptoKey,
    data: BufferSource,
  ): Promise<ArrayBuffer>;
  deriveBits(
    algorithm: DerivationAlgorithmIdentifier,
    baseKey: CryptoKey,
    length: number,
  ): Promise<ArrayBuffer>;
  deriveKey(
    algorithm: DerivationAlgorithmIdentifier,
    baseKey: CryptoKey,
    derivedKeyAlgorithm: KeyAlgorithmIdentifier,
    extractable: boolean,
    keyUsages: Array<CryptoKeyUsage>,
  ): Promise<CryptoKey>;
  digest(algorithm: DigestAlgorithmIdentifier, data: BufferSource): Promise<ArrayBuffer>;
  encrypt(
    algorithm: EncryptionAlgorithmIdentifier,
    key: CryptoKey,
    data: BufferSource,
  ): Promise<ArrayBuffer>;
  exportKey(format: CryptoKeyFormat, key: CryptoKey): Promise<ArrayBuffer | JsonWebKey>;
  generateKey(
    algorithm: KeyAlgorithmIdentifier,
    extractable: boolean,
    keyUsages: Array<CryptoKeyUsage>,
  ): Promise<CryptoKey | CryptoKeyPair>;
  importKey(
    format: CryptoKeyFormat,
    keyData: BufferSource | JsonWebKey,
    algorithm: KeyAlgorithmIdentifier,
    extractable: boolean,
    keyUsages: Array<CryptoKeyUsage>,
  ): Promise<CryptoKey>;
  sign(
    algorithm: SignatureAlgorithmIdentifier,
    key: CryptoKey,
    data: BufferSource,
  ): Promise<ArrayBuffer>;
  unwrapKey(
    format: CryptoKeyFormat,
    wrappedKey: BufferSource,
    unwrappingKey: CryptoKey,
    unwrapAlgorithm: EncryptionAlgorithmIdentifier | { name: "AES-KW", ... },
    unwrappedKeyAlgorithm: KeyAlgorithmIdentifier,
    extractable: boolean,
    keyUsages: Array<CryptoKeyUsage>,
  ): Promise<CryptoKey>;
  verify(
    algorithm: SignatureAlgorithmIdentifier,
    key: CryptoKey,
    signature: BufferSource,
    data: BufferSource,
  ): Promise<boolean>;
  wrapKey(
    format: CryptoKeyFormat,
    key: CryptoKey,
    wrappingKey: CryptoKey,
    wrapAlgorithm: EncryptionAlgorithmIdentifier | { name: "AES-KW", ... },
  ): Promise<ArrayBuffer>;
}

declare interface Crypto {
  // Repeated from `bom.js` verbatim, including its comment: this declaration
  // shadows that one whole, so anything left out here is taken away.
  //
  // Not using $TypedArray as that would include Float32Array and Float64Array which are not accepted
  getRandomValues: <
    T: Int8Array | Uint8Array | Uint8ClampedArray | Int16Array | Uint16Array | Int32Array | Uint32Array | BigInt64Array | BigUint64Array
  >(typedArray: T) => T;
  randomUUID: () => string;
  subtle: SubtleCrypto;
}

declare var crypto: Crypto;
