// @flow
//
// Internal to `@uniflowed/web`: the `src` and `srcset` `Image` writes for a
// remote image, from what `./image-endpoint.js` says about the endpoint.

import type { ImageEndpoint } from "./image-endpoint.js";

/**
 * The `src` and `srcset` for a remote image, through the endpoint.
 *
 * One rung per accepted width narrower than the image, and one more — the
 * narrowest accepted width that covers it, or the widest there is — described
 * at the width it will actually come back at, because the endpoint never
 * upscales and a descriptor wider than the file is a browser choosing a rung
 * for pixels that do not exist.
 *
 * `null` when the project has no endpoint, or the URL is not one it fetches.
 */
export function remoteSources(
  endpoint: ImageEndpoint,
  src: string,
  intrinsic: number,
  quality: number,
): {| readonly src: string, readonly srcSet: string |} | null {
  const path = endpoint.path;
  if (path == null) return null;
  const scheme = src.slice(0, 8).toLowerCase();
  if (!scheme.startsWith("https://") && !scheme.startsWith("http://")) return null;
  const widths = [...endpoint.widths].sort((a, b) => a - b);
  if (widths.length === 0) return null;
  const at = (width: number): string =>
    `${path}?url=${encodeURIComponent(src)}&w=${width}&q=${quality}`;
  const cover = widths.find((width) => width >= intrinsic) ?? widths[widths.length - 1];
  // Everything narrower than the rung that covers the image is narrower than
  // the image too, so each of those comes back at its own width.
  const rungs = widths.filter((width) => width < cover).map((width) => [width, width]);
  rungs.push([cover, Math.min(cover, intrinsic)]);
  return {
    src: at(cover),
    srcSet: rungs.map(([asked, described]) => `${at(asked)} ${described}w`).join(", "),
  };
}
