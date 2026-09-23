// @flow
//
// Internal to `@uniflowed/web`: where `Image` sends a remote `src`.
//
// This file is what `Image` reads *outside* a uf build — under `uf test`, in a
// library that renders one without uf's Vite plugins — and there it says there
// is no endpoint, so a remote `src` is written into `<img>` as it was given.
//
// Inside one, `uf:asset` (`@uniflowed/vite`'s `internal/assets.js`) answers
// for this module with the project's own values: the endpoint's path under the
// base path when `app.builtins.images.remotePatterns` lists anything, and the
// widths and quality the endpoint will accept. Generated rather than read at
// runtime, because the browser bundle and the server one both render `Image`
// and have to write the same `srcset`, or hydration disagrees about it.
//
// Which is also why nothing but the value lives here: the whole module is
// replaced, so a function beside it would vanish from the build. What `Image`
// does with the value is `./remote-image.js`.

/** What `Image` needs to know about `/__uf/image`. */
export type ImageEndpoint = {|
  /** The endpoint's path, or `null` when the project has none. */
  readonly path: string | null,
  /** The widths it accepts, which are the only ones worth a `srcset` rung. */
  readonly widths: $ReadOnlyArray<number>,
  /** The quality it encodes at when a request names none. */
  readonly quality: number,
|};

/**
 * No endpoint, and uf's defaults for the rest.
 *
 * The widths and quality are `uf_assets`' defaults, and
 * `tests/library/image-endpoint.test.js` holds them to the Rust ones and to
 * `@uniflowed/server/image`'s.
 */
export const IMAGE_ENDPOINT: ImageEndpoint = {
  path: null,
  widths: [640, 750, 828, 1080, 1200, 1440, 1920],
  quality: 75,
};
