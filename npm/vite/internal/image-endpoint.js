// @noflow
//
// Plain JavaScript: executed by the host that serves a build, and by `uf dev`.
//
// `/__uf/image` on the three doors `uf` itself runs — `uf dev`, `uf preview`
// and `uf start` — with `uf` as the encoder.
//
// The endpoint is `@uniflowed/server/image`'s, the fetch is
// `@uniflowed/server/image/node`'s, and this is the one piece those two
// cannot have: a transformer that reaches the `uf` binary. It writes the
// bounded source to a private temporary directory, asks `uf assets` for one
// variant of it (`crates/uf_cli/src/commands/assets.rs`'s `variant` request,
// which is `uf_assets::variant`), and reads the one file that comes back.
// Both files are removed before the answer is returned: the cache is the
// endpoint's `CacheStore`, not this directory.
//
// A deployed directory has no `uf` in it, which is why this is not in
// `@uniflowed/server`. What `--adapter` links instead is argued in
// `driver.js`'s `imageEndpointSource`.

import { randomUUID } from "node:crypto";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";

import { AssetService } from "@uniflowed/host/assets";

/**
 * Whether the project asked for an endpoint at all.
 *
 * `app.builtins.images.remotePatterns` with something in it, and images not
 * turned off. Nothing else constructs one: a project that listed no host has
 * no `/__uf/image`, and the path is an ordinary 404.
 *
 * @param {{enabled?: boolean, remotePatterns?: unknown[]} | undefined} images
 */
export function servesRemoteImages(images) {
  return (
    images?.enabled !== false &&
    Array.isArray(images?.remotePatterns) &&
    images.remotePatterns.length > 0
  );
}

/**
 * The endpoint's options from `app.builtins.images`, with uf's defaults where
 * the project said nothing — the same defaults `uf_assets::ImagesConfig`
 * applies, from `@uniflowed/server/image` where both sides can read them.
 *
 * @param {object} images `app.builtins.images`
 * @param {{DEFAULT_WIDTHS: number[], DEFAULT_QUALITY: number}} defaults
 */
export function endpointSettings(images, defaults) {
  return {
    remotePatterns: images.remotePatterns,
    widths: images.widths ?? defaults.DEFAULT_WIDTHS,
    quality: images.quality ?? defaults.DEFAULT_QUALITY,
    qualities: images.qualities ?? [],
  };
}

/**
 * An encoder that is `uf` itself.
 *
 * One `uf assets` process for the life of the server, started on the first
 * miss and not before, and one temporary directory beside it.
 *
 * @param {{root: string, command?: string}} options
 */
export function ufImageTransform({ root, command }) {
  let service = null;
  let directory = null;
  return async function transform({ bytes, width, quality, avif }) {
    service ??= new AssetService({ command, root });
    directory ??= mkdtemp(path.join(os.tmpdir(), "uf-image-"));
    const base = path.join(await directory, randomUUID());
    const source = `${base}.source`;
    const out = `${base}.out`;
    await writeFile(source, bytes);
    try {
      const variant = await service.variant(source, { outDir: out, width, quality, avif });
      const encoded = await readFile(path.join(out, variant.file));
      return {
        bytes: new Uint8Array(encoded.buffer, encoded.byteOffset, encoded.byteLength),
        type: variant.mime,
      };
    } finally {
      await rm(source, { force: true });
      await rm(out, { recursive: true, force: true });
    }
  };
}

/**
 * `/__uf/image` for a door `uf` runs, or `undefined` for a project with no
 * remote images.
 *
 * `store` is the `CacheStore` a durable `rendering.cache.store` gave the route
 * cache's provider to, when there is one; without it the endpoint keeps its
 * variants in memory.
 *
 * @param {{images?: object, root: string, command?: string, store?: object}} options
 */
export async function localImageEndpoint({ images, root, command, store }) {
  if (!servesRemoteImages(images)) return undefined;
  const [endpoint, node] = await Promise.all([
    import("@uniflowed/server/image"),
    import("@uniflowed/server/image/node"),
  ]);
  return endpoint.createImageEndpoint({
    ...endpointSettings(images, endpoint),
    fetch: node.nodeImageFetch({
      allowPrivateAddresses: images.dangerouslyAllowPrivateAddresses === true,
    }),
    transform: ufImageTransform({ root, command }),
    ...(store == null ? {} : { store }),
  });
}
