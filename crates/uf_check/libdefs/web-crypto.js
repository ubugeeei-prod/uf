/**
 * @fileoverview Web Crypto, typed for Flow.
 *
 * Flow's vendored `bom.js` declares `Crypto` with `getRandomValues`,
 * `randomUUID` and a `subtle` holding **one** method:
 *
 *     subtle: {
 *       digest(algorithm: string, data: Uint8Array): Promise<ArrayBuffer>
 *     },
 *
 * That is enough to hash and nothing else. `packages/server/internal/oauth.js`
 * only hashes, so it checked clean; `packages/server/internal/draft.js` signs
 * the draft-mode cookie with HMAC-SHA256 and reported four errors from
 * `uf check` for calls that are correct and that every runtime uf targets
 * implements — `subtle.sign`, `subtle.importKey`, and `CryptoKey` twice.
 * See ubugeeei-prod/uf#619, and #615, which took the errors rather than
 * hand-rolling HMAC out of `digest` to satisfy a type checker.
 *
 * # Why this file and not a patch
 *
 * `bom.js` lives in the `upstream/flow` submodule, which
 * `tools/upstream/sync.sh` checks out fresh with no patch step: a diff in it
 * is deleted by the next sync. Library definitions are merged in reverse
 * declaration order, so a later file's declaration of a name shadows an
 * earlier one's, and this file is declared last in
 * `crates/uf_check/src/upstream/environments.rs`. That is the extension point
 * `libdefs/vite-client.js` uses for `Import$Meta`, and it is the one used
 * here.
 *
 * Shadowing is per **name**, not per member, so `Crypto` below has to restate
 * `getRandomValues` and `randomUUID` exactly as `bom.js` had them. It does.
 * `crypto.randomUUID()` in `packages/server/internal/context.js` and
 * `crypto.getRandomValues` in `oauth.js` and `draft.js` are what would break
 * if it did not, and the fixtures exercise all three.
 *
 * # Not `any`
 *
 * A `subtle` typed `any` would have closed the issue and been worse than the
 * gap: every call would check, including the wrong ones, and the report would
 * say a program was fine when nobody had looked. So the algorithm parameters
 * are the dictionaries the specification gives them, keyed by a **literal**
 * `name` so the union is disjoint — `{ name: "HMAC", hash: "SHA-256" }` lands
 * on `HmacImportParams` and `{ name: "HMAC" }` at an `importKey` does not land
 * anywhere, because the specification requires the hash there. A key is a
 * `CryptoKey` and never bytes, so signing with the raw secret is an error, and
 * `generateKey`'s pair is a separate type from a single key.
 *
 * # The whole of `SubtleCrypto`, not the four names that were missing
 *
 * The next occurrence of this issue is somebody writing correct code and being
 * told it is wrong, so the survey is the fix. All twelve methods are here:
 * `encrypt`, `decrypt`, `sign`, `verify`, `digest`, `generateKey`,
 * `deriveKey`, `deriveBits`, `importKey`, `exportKey`, `wrapKey`, `unwrapKey`.
 * Only `digest`, `sign` and `importKey` are reached by anything in this
 * repository today; the other nine are here so that the tenth thing somebody
 * writes is not another issue.
 *
 * # The dialect
 *
 * `readonly` rather than the `+` variance sigil, `interface` for a dictionary,
 * and overloads written as repeated signatures — the same choices
 * `libdefs/vite-client.js` made, and for the same reason: a libdef that
 * disagrees with its neighbours is a libdef nobody can edit.
 */

/**
 * Bytes an algorithm reads.
 *
 * The specification's `BufferSource`, which is already a global name here —
 * `webassembly.js` defines it as `$TypedArray | ArrayBuffer`. That spelling
 * leaves out `DataView`, which Web Crypto does accept, and widening a name
 * `BodyInit` and `WebAssembly.instantiate` also read is a change to two APIs
 * that did not ask for one. So this is Web Crypto's own alias.
 */
type CryptoBufferSource = $ArrayBufferView | ArrayBuffer;

/** The hashes every runtime uf targets implements. */
type CryptoHashName = "SHA-1" | "SHA-256" | "SHA-384" | "SHA-512";

/** A hash named either way the specification allows. */
type CryptoHashIdentifier = CryptoHashName | { readonly name: CryptoHashName };

/**
 * What a key may be used for.
 *
 * A closed set rather than `string`: `["singn"]` is the typo that produces a
 * key which exists, imports cleanly and throws at the first `sign`.
 */
type KeyUsage =
  | "encrypt"
  | "decrypt"
  | "sign"
  | "verify"
  | "deriveKey"
  | "deriveBits"
  | "wrapKey"
  | "unwrapKey";

/** Which half of a pair, or neither. */
type KeyType = "public" | "private" | "secret";

/** How key material is spelled on the way in and out. */
type KeyFormat = "raw" | "pkcs8" | "spki" | "jwk";

/** The named curves the specification defines for ECDSA and ECDH. */
type NamedCurve = "P-256" | "P-384" | "P-521";

/**
 * The algorithm a key reports about itself.
 *
 * Inexact and almost entirely optional on purpose: the specification gives
 * each algorithm its own dictionary here — an AES key reports a `length`, an
 * RSA key a `modulusLength` — and a reader holding a `CryptoKey` of unknown
 * provenance can only ask. `name` is the one member every one of them has.
 */
type KeyAlgorithm = {
  readonly name: string,
  readonly length?: number,
  readonly hash?: { readonly name: string },
  readonly namedCurve?: string,
  readonly modulusLength?: number,
  readonly publicExponent?: Uint8Array,
  ...
};

/** One of the extra primes of a multi-prime RSA key, as JWK spells it. */
type RsaOtherPrimesInfo = {
  readonly r?: string,
  readonly d?: string,
  readonly t?: string,
  ...
};

/**
 * A key as JSON, per RFC 7517 and the Web Crypto registry.
 *
 * Inexact, because the members that are present depend on `kty` and a JWK from
 * a provider routinely carries registry members uf has never heard of. `kty`
 * is the one that is always there.
 */
type JsonWebKey = {
  readonly kty: string,
  readonly use?: string,
  readonly key_ops?: ReadonlyArray<string>,
  readonly alg?: string,
  readonly ext?: boolean,
  readonly crv?: string,
  readonly x?: string,
  readonly y?: string,
  readonly d?: string,
  readonly n?: string,
  readonly e?: string,
  readonly p?: string,
  readonly q?: string,
  readonly dp?: string,
  readonly dq?: string,
  readonly qi?: string,
  readonly oth?: ReadonlyArray<RsaOtherPrimesInfo>,
  readonly k?: string,
  ...
};

/**
 * A key the platform holds.
 *
 * Opaque by construction, which is the point of it: the material is inside the
 * implementation and `exportKey` is the only way out, so a key that was
 * imported with `extractable: false` cannot become bytes. `draft.js` imports
 * its HMAC key that way.
 */
declare class CryptoKey {
  readonly type: KeyType;
  readonly extractable: boolean;
  readonly algorithm: KeyAlgorithm;
  readonly usages: ReadonlyArray<KeyUsage>;
}

/** What `generateKey` hands back for an asymmetric algorithm. */
type CryptoKeyPair = {
  readonly privateKey: CryptoKey,
  readonly publicKey: CryptoKey,
  ...
};

// --- Algorithm parameters -------------------------------------------------
//
// One dictionary per specification dictionary, each tagged with a literal
// `name`. The tags are what make the unions below disjoint, and a disjoint
// union is what turns "the wrong algorithm object" into an error a reader can
// act on rather than a list of every alternative that did not match.

/** `RSA-OAEP`, at `encrypt`, `decrypt`, `wrapKey` and `unwrapKey`. */
interface RsaOaepParams {
  readonly name: "RSA-OAEP";
  readonly label?: CryptoBufferSource;
}

/** `AES-CTR`, at `encrypt` and `decrypt`. */
interface AesCtrParams {
  readonly name: "AES-CTR";
  readonly counter: CryptoBufferSource;
  readonly length: number;
}

/** `AES-CBC`, at `encrypt` and `decrypt`. */
interface AesCbcParams {
  readonly name: "AES-CBC";
  readonly iv: CryptoBufferSource;
}

/** `AES-GCM`, at `encrypt` and `decrypt`. */
interface AesGcmParams {
  readonly name: "AES-GCM";
  readonly iv: CryptoBufferSource;
  readonly additionalData?: CryptoBufferSource;
  readonly tagLength?: number;
}

/** `RSA-PSS`, at `sign` and `verify`. */
interface RsaPssParams {
  readonly name: "RSA-PSS";
  readonly saltLength: number;
}

/** `ECDSA`, at `sign` and `verify`. The hash is required and is not implied. */
interface EcdsaParams {
  readonly name: "ECDSA";
  readonly hash: CryptoHashIdentifier;
}

/** `ECDH` and `X25519`, at `deriveKey` and `deriveBits`. */
interface EcdhKeyDeriveParams {
  readonly name: "ECDH" | "X25519";
  readonly public: CryptoKey;
}

/** `HKDF`, at `deriveKey` and `deriveBits`. */
interface HkdfParams {
  readonly name: "HKDF";
  readonly hash: CryptoHashIdentifier;
  readonly salt: CryptoBufferSource;
  readonly info: CryptoBufferSource;
}

/** `PBKDF2`, at `deriveKey` and `deriveBits`. */
interface Pbkdf2Params {
  readonly name: "PBKDF2";
  readonly hash: CryptoHashIdentifier;
  readonly salt: CryptoBufferSource;
  readonly iterations: number;
}

/** `HMAC`, at `generateKey` and `importKey`. The hash is what keys it. */
interface HmacKeyParams {
  readonly name: "HMAC";
  readonly hash: CryptoHashIdentifier;
  readonly length?: number;
}

/** The RSA family, at `importKey`. */
interface RsaHashedImportParams {
  readonly name: "RSASSA-PKCS1-v1_5" | "RSA-PSS" | "RSA-OAEP";
  readonly hash: CryptoHashIdentifier;
}

/** The RSA family, at `generateKey`. */
interface RsaHashedKeyGenParams {
  readonly name: "RSASSA-PKCS1-v1_5" | "RSA-PSS" | "RSA-OAEP";
  readonly modulusLength: number;
  readonly publicExponent: Uint8Array;
  readonly hash: CryptoHashIdentifier;
}

/** The elliptic-curve family, at `generateKey` and `importKey`. */
interface EcKeyParams {
  readonly name: "ECDSA" | "ECDH";
  readonly namedCurve: NamedCurve;
}

/** The AES family, at `generateKey` and at `deriveKey`'s derived type. */
interface AesKeyParams {
  readonly name: "AES-CTR" | "AES-CBC" | "AES-GCM" | "AES-KW";
  readonly length: 128 | 192 | 256;
}

/** `HMAC` at `deriveKey`'s derived type, where the length is in bits. */
interface HmacDerivedKeyParams {
  readonly name: "HMAC";
  readonly hash: CryptoHashIdentifier;
  readonly length?: number;
}

/** What `encrypt` and `decrypt` accept. */
type EncryptAlgorithm =
  | "RSA-OAEP"
  | RsaOaepParams
  | AesCtrParams
  | AesCbcParams
  | AesGcmParams;

/** What `sign` and `verify` accept. */
type SignAlgorithm =
  | "HMAC"
  | "RSASSA-PKCS1-v1_5"
  | "Ed25519"
  | { readonly name: "HMAC" | "RSASSA-PKCS1-v1_5" | "Ed25519" }
  | RsaPssParams
  | EcdsaParams;

/** What `deriveKey` and `deriveBits` accept as the derivation. */
type DeriveAlgorithm = EcdhKeyDeriveParams | HkdfParams | Pbkdf2Params;

/** What `deriveKey` accepts as the type of the key it produces. */
type DerivedKeyAlgorithm = AesKeyParams | HmacDerivedKeyParams;

/** What `generateKey` accepts. */
type KeyGenAlgorithm =
  | HmacKeyParams
  | RsaHashedKeyGenParams
  | EcKeyParams
  | AesKeyParams
  | { readonly name: "Ed25519" | "X25519" }
  | "Ed25519"
  | "X25519";

/**
 * An algorithm whose *import* needs nothing beyond its name.
 *
 * `HMAC` is deliberately not here, and neither is the RSA or the EC family:
 * the specification normalises `importKey`'s algorithm per operation, and for
 * those three it requires the hash or the curve. `{ name: "HMAC" }` at an
 * `importKey` is a key whose digest nobody chose, so it is an error here
 * exactly as it is a `TypeError` at run time. At a `sign` the same object is
 * fine — the key already carries the hash — which is why the two unions are
 * different unions rather than one shared one.
 */
type ImportKeyBareName =
  | "AES-CTR"
  | "AES-CBC"
  | "AES-GCM"
  | "AES-KW"
  | "PBKDF2"
  | "HKDF"
  | "Ed25519"
  | "X25519";

/** What `importKey` and `unwrapKey` accept for the key being read. */
type ImportKeyAlgorithm =
  | ImportKeyBareName
  | { readonly name: ImportKeyBareName }
  | HmacKeyParams
  | RsaHashedImportParams
  | EcKeyParams;

/**
 * `crypto.subtle`.
 *
 * Every method returns a promise and every one of them rejects rather than
 * throwing, which is why `draft.js` awaits its key rather than caching a
 * value. The overloads are the two places the specification's return type
 * depends on an argument's *value*: `exportKey("jwk", …)` is a `JsonWebKey`
 * and every other format is bytes, and `generateKey` for an asymmetric
 * algorithm is a pair where a symmetric one is a single key.
 */
interface SubtleCrypto {
  /** Hash bytes. The one method `bom.js` already had. */
  digest(
    algorithm: CryptoHashIdentifier,
    data: CryptoBufferSource,
  ): Promise<ArrayBuffer>;

  /** Sign bytes with a key whose usages include `"sign"`. */
  sign(
    algorithm: SignAlgorithm,
    key: CryptoKey,
    data: CryptoBufferSource,
  ): Promise<ArrayBuffer>;

  /** Check a signature with a key whose usages include `"verify"`. */
  verify(
    algorithm: SignAlgorithm,
    key: CryptoKey,
    signature: CryptoBufferSource,
    data: CryptoBufferSource,
  ): Promise<boolean>;

  /** Encrypt bytes. */
  encrypt(
    algorithm: EncryptAlgorithm,
    key: CryptoKey,
    data: CryptoBufferSource,
  ): Promise<ArrayBuffer>;

  /** Decrypt bytes. */
  decrypt(
    algorithm: EncryptAlgorithm,
    key: CryptoKey,
    data: CryptoBufferSource,
  ): Promise<ArrayBuffer>;

  /** Generate a symmetric key. */
  generateKey(
    algorithm: HmacKeyParams | AesKeyParams,
    extractable: boolean,
    keyUsages: ReadonlyArray<KeyUsage>,
  ): Promise<CryptoKey>;
  /** Generate an asymmetric key pair. */
  generateKey(
    algorithm: RsaHashedKeyGenParams | EcKeyParams | { readonly name: "Ed25519" | "X25519" },
    extractable: boolean,
    keyUsages: ReadonlyArray<KeyUsage>,
  ): Promise<CryptoKeyPair>;
  /** Generate a key for an algorithm named by a bare string. */
  generateKey(
    algorithm: KeyGenAlgorithm,
    extractable: boolean,
    keyUsages: ReadonlyArray<KeyUsage>,
  ): Promise<CryptoKey | CryptoKeyPair>;

  /** Derive a key from another key. */
  deriveKey(
    algorithm: DeriveAlgorithm,
    baseKey: CryptoKey,
    derivedKeyAlgorithm: DerivedKeyAlgorithm,
    extractable: boolean,
    keyUsages: ReadonlyArray<KeyUsage>,
  ): Promise<CryptoKey>;

  /** Derive raw bytes from a key. */
  deriveBits(
    algorithm: DeriveAlgorithm,
    baseKey: CryptoKey,
    length?: number | null,
  ): Promise<ArrayBuffer>;

  /** Read a key out of JSON. */
  importKey(
    format: "jwk",
    keyData: JsonWebKey,
    algorithm: ImportKeyAlgorithm,
    extractable: boolean,
    keyUsages: ReadonlyArray<KeyUsage>,
  ): Promise<CryptoKey>;
  /** Read a key out of bytes. */
  importKey(
    format: "raw" | "pkcs8" | "spki",
    keyData: CryptoBufferSource,
    algorithm: ImportKeyAlgorithm,
    extractable: boolean,
    keyUsages: ReadonlyArray<KeyUsage>,
  ): Promise<CryptoKey>;

  /** Write an extractable key out as JSON. */
  exportKey(format: "jwk", key: CryptoKey): Promise<JsonWebKey>;
  /** Write an extractable key out as bytes. */
  exportKey(format: "raw" | "pkcs8" | "spki", key: CryptoKey): Promise<ArrayBuffer>;

  /** Export a key already encrypted under another one. */
  wrapKey(
    format: KeyFormat,
    key: CryptoKey,
    wrappingKey: CryptoKey,
    wrapAlgorithm: EncryptAlgorithm | "AES-KW" | { readonly name: "AES-KW" },
  ): Promise<ArrayBuffer>;

  /** Import a key that arrived encrypted under another one. */
  unwrapKey(
    format: KeyFormat,
    wrappedKey: CryptoBufferSource,
    unwrappingKey: CryptoKey,
    unwrapAlgorithm: EncryptAlgorithm | "AES-KW" | { readonly name: "AES-KW" },
    unwrappedKeyAlgorithm: ImportKeyAlgorithm,
    extractable: boolean,
    keyUsages: ReadonlyArray<KeyUsage>,
  ): Promise<CryptoKey>;
}

/**
 * `crypto`.
 *
 * `getRandomValues` and `randomUUID` are restated from `bom.js` rather than
 * added to it, because a later libdef shadows a name whole. The typed-array
 * bound on `getRandomValues` is Flow's own, comment included: the
 * specification refuses a float array, so `$TypedArray` would be wrong.
 */
declare interface Crypto {
  // Not using $TypedArray as that would include Float32Array and Float64Array which are not accepted
  getRandomValues: <
    T:
      | Int8Array
      | Uint8Array
      | Uint8ClampedArray
      | Int16Array
      | Uint16Array
      | Int32Array
      | Uint32Array
      | BigInt64Array
      | BigUint64Array,
  >(
    typedArray: T,
  ) => T;
  randomUUID: () => string;
  subtle: SubtleCrypto;
}
