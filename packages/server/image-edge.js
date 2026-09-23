// @flow
//
// `@uniflowed/server/image/edge`: the image endpoint on a Cloudflare Worker.
//
// A Worker has no `uf` to encode with and no filesystem to hand one a file,
// so `--adapter edge` delegates the encode to the platform's own image service
// — the Images binding, which `uf build --adapter edge` declares in
// `wrangler.json` as `IMAGES` — and keeps everything else uf's: the allow-list,
// the redirect loop, the byte bound, the format sniff and the cache are
// `./image.js`, exactly as on every other host.
//
// # Why the binding and not `fetch(url, { cf: { image } })`
//
// The older way to resize on Cloudflare is an option on `fetch` itself, and it
// would have been one line. It is the wrong line: Cloudflare's resizer fetches
// the origin *itself* and follows its redirects itself, so the redirect that
// leaves the allow-list — the one this endpoint exists to refuse — would be
// followed out of uf's sight. The binding takes bytes instead, so the bytes it
// transforms are the ones `./image.js` fetched, bounded and checked.
//
// # What a Worker cannot check
//
// The address a name resolves to. A Worker has no resolver, and its `fetch`
// connects from Cloudflare's network rather than from the one a deployment's
// databases sit in, so the rebinding attack `./image-node.js` closes is not
// one a Worker is open to in the same way. The allow-list and the redirect
// check hold here unchanged; the address check is the one that does not
// exist, and it is stated rather than implied.

import { currentContext } from "./internal/context.js";
import type { ImageFetch, ImageTransform, TransformInput, TransformOutput } from "./image.js";

/** The binding `uf build --adapter edge` writes into `wrangler.json`. */
export const IMAGES_BINDING = "IMAGES";

/**
 * The part of Cloudflare's Images binding this uses.
 *
 * Structural, as `./edge.js` declares the assets binding: the runtime supplies
 * the object, and naming the whole of Cloudflare's type here would be this
 * package holding a copy of another project's types.
 */
export type ImagesBinding = {
  input(stream: ReadableStream<Uint8Array>): ImageTransformer,
  ...
};

/** One chain of transforms on an input. */
export type ImageTransformer = {
  transform(options: {| readonly width: number, readonly fit: "scale-down" |}): ImageTransformer,
  output(options: {| readonly format: string, readonly quality: number |}): Promise<{
    response(): Response,
    ...
  }>,
  ...
};

/** The endpoint's fetch on a Worker: the platform's, redirects not followed. */
export function edgeImageFetch(): ImageFetch {
  return (url, signal) => fetch(url, { redirect: "manual", signal });
}

/**
 * The endpoint's encoder on a Worker: the Images binding.
 *
 * The binding is looked up on the request that is being answered — a Worker
 * is handed its bindings per request, as `env`, and `./edge.js` puts them on
 * the request's context — unless one is passed, which is for a test.
 */
export function cloudflareImageTransform(options?: {|
  readonly binding?: string,
  readonly images?: ImagesBinding,
|}): ImageTransform {
  const name = options?.binding ?? IMAGES_BINDING;
  return async function transform(input: TransformInput): Promise<TransformOutput> {
    const images = options?.images ?? bindingNamed(name);
    // The source's own family when the browser did not ask for AVIF, the rule
    // `uf_assets::variant` follows under `uf start`: a WebP comes back as a
    // PNG, which keeps whatever transparency it had.
    const format = input.avif
      ? "image/avif"
      : input.type === "image/jpeg"
        ? "image/jpeg"
        : "image/png";
    const result = await images
      .input(
        new ReadableStream({
          start(controller) {
            controller.enqueue(input.bytes);
            controller.close();
          },
        }),
      )
      // `scale-down`: never wider than the source, as on every other host.
      .transform({ width: input.width, fit: "scale-down" })
      .output({ format, quality: input.quality });
    const response = result.response();
    return {
      bytes: new Uint8Array(await response.arrayBuffer()),
      type: response.headers.get("content-type") ?? format,
    };
  };
}

/** The Images binding on the current request, or a sentence saying where to put it. */
function bindingNamed(name: string): ImagesBinding {
  const bindings = currentContext()?.bindings ?? null;
  const found = bindings == null ? null : bindings[name];
  if (found == null || typeof found !== "object") {
    throw new Error(
      `@uniflowed/server: the image endpoint encodes through the Cloudflare Images binding ` +
        `${name}, and this Worker has none. \`uf build --adapter edge\` writes ` +
        `\`"images": { "binding": "${name}" }\` into wrangler.json; keep it there.`,
    );
  }
  return found as $FlowFixMe;
}
