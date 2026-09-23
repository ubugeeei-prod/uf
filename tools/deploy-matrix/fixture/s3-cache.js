// @flow
//
// A route-cache provider over an S3 bucket, for the fixture's `serverless`
// variant.
//
// `--adapter serverless` has no store of its own for a regenerated page — an
// instance's memory and its `/tmp` go with the instance — so a project that
// regenerates pages on a Lambda names a provider in `rendering.cache.store`,
// and the build refuses it by name otherwise. This is that provider, written
// the way a project would write one: `@uniflowed/server/cache/kv`'s
// `createKvCache` does the indexing, and this module is only a KV namespace
// over S3's REST API. In the matrix the bucket lives in Kumo, so the page a
// Lambda regenerated is read back by the next invocation from outside the
// process, exactly as it would be from real S3.
//
// What it is not: a production S3 client. Requests are unsigned, because Kumo
// does not check SigV4 and the point here is the cache seam rather than AWS
// authentication; a real deployment would put the AWS SDK behind the same four
// methods.
//
// Keys are stored hex-encoded. A cache key can hold `/`, `%` and anything
// else, and S3 path-style addressing (and Kumo's path cleaning) would read
// `//` or `%2F` as structure. Hex keeps every key one opaque path segment and
// keeps prefixes prefixes, which is what `list({ prefix })` needs.

import type { CacheProvider } from "@uniflowed/server/cache/kv";
import { createKvCache, type KvNamespace } from "@uniflowed/server/cache/kv";

/** `text` as lowercase hex of its UTF-8 bytes. */
function hex(text: string): string {
  return Array.from(new TextEncoder().encode(text), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

/** The inverse of [`hex`]. */
function unhex(encoded: string): string {
  const bytes = new Uint8Array(encoded.length / 2);
  for (let at = 0; at < bytes.length; at++) {
    bytes[at] = Number.parseInt(encoded.slice(at * 2, at * 2 + 2), 16);
  }
  return new TextDecoder().decode(bytes);
}

/** The five XML entities S3 escapes in a listing. */
function unescapeXml(text: string): string {
  return text
    .replaceAll("&lt;", "<")
    .replaceAll("&gt;", ">")
    .replaceAll("&quot;", '"')
    .replaceAll("&apos;", "'")
    .replaceAll("&amp;", "&");
}

/** Fail with the status and body, which is what a person debugging this needs. */
async function refuse(what: string, response: Response): Promise<empty> {
  throw new Error(`s3-cache: ${what} answered ${response.status}: ${await response.text()}`);
}

/**
 * A Workers-KV-shaped namespace over the S3 bucket at `endpoint`/`bucket`.
 *
 * Creates the bucket on first use (an existing one is not an error). Expiry is
 * not implemented: `createKvCache` asks for it only as a hint, and an entry's
 * own `expiresAt` is still checked when it is read.
 */
export function s3Namespace(endpoint: string, bucket: string): KvNamespace {
  const base = `${endpoint.replace(/\/$/, "")}/${bucket}`;
  let created: Promise<void> | null = null;
  const ready = (): Promise<void> => {
    created ??= fetch(base, { method: "PUT" }).then(async (response) => {
      if (!response.ok && response.status !== 409) {
        await refuse(`creating ${bucket}`, response);
      }
    });
    return created;
  };
  return {
    async get(key: string, _type: "text"): Promise<string | null> {
      await ready();
      const response = await fetch(`${base}/${hex(key)}`);
      if (response.status === 404) return null;
      if (!response.ok) await refuse(`GET ${key}`, response);
      return await response.text();
    },
    async put(key: string, value: string): Promise<void> {
      await ready();
      const response = await fetch(`${base}/${hex(key)}`, { method: "PUT", body: value });
      if (!response.ok) await refuse(`PUT ${key}`, response);
    },
    async delete(key: string): Promise<void> {
      await ready();
      const response = await fetch(`${base}/${hex(key)}`, { method: "DELETE" });
      if (!response.ok && response.status !== 404) await refuse(`DELETE ${key}`, response);
    },
    async list(options: {| prefix: string, cursor?: string |}) {
      await ready();
      const query = new URLSearchParams({ "list-type": "2", prefix: hex(options.prefix) });
      if (options.cursor != null) query.set("continuation-token", options.cursor);
      const response = await fetch(`${base}?${query.toString()}`);
      if (!response.ok) await refuse(`LIST ${options.prefix}`, response);
      const xml = await response.text();
      const keys = Array.from(xml.matchAll(/<Key>([^<]*)<\/Key>/g), (match) => ({
        name: unhex(unescapeXml(match[1])),
      }));
      const next = /<NextContinuationToken>([^<]*)<\/NextContinuationToken>/.exec(xml)?.[1];
      const truncated = /<IsTruncated>true<\/IsTruncated>/.test(xml);
      return truncated && next != null
        ? { keys, list_complete: false, cursor: unescapeXml(next) }
        : { keys, list_complete: true };
    },
  };
}

/**
 * The provider `rendering.cache.store` names in the `serverless` variant.
 *
 * Reads `UF_MATRIX_S3_ENDPOINT` and `UF_MATRIX_S3_BUCKET`, which the matrix
 * sets on the Lambda; without the endpoint it fails at once and says so,
 * rather than regenerating into nowhere.
 */
export function createCacheProvider(): CacheProvider {
  const endpoint = process.env.UF_MATRIX_S3_ENDPOINT;
  if (endpoint == null || endpoint === "") {
    throw new Error(
      "s3-cache: UF_MATRIX_S3_ENDPOINT is not set, so there is no bucket to keep " +
        "regenerated pages in. tools/deploy-matrix sets it to Kumo's address.",
    );
  }
  return createKvCache({
    namespace: s3Namespace(endpoint, process.env.UF_MATRIX_S3_BUCKET ?? "uf-deploy-matrix"),
  });
}
