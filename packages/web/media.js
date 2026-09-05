// @flow
//
// `@uniflowed/web/media`: the elements that load bytes.
//
// An image, a `<picture>`, and a font preload. They are grouped because they
// share one problem: each one is a request the browser has to make while the
// page is still being laid out, and each has a well-known way of being got
// wrong that costs either a layout shift or a wasted download.
//
// Every default here is the one that is right more often than not, and every
// one of them can be overridden — a component whose defaults cannot be turned
// off is a component people stop using.
//
// # What belongs in this module
//
// Anything a page renders in order to make the browser fetch bytes while the
// layout is still being decided: another `<source>` form, a video poster, a
// preload for a stylesheet. The test is whether the mistake it prevents is a
// wasted request or a shifted layout.
//
// `Font` is the boundary worth stating, because it is a `<link>` and `<head>`
// has its own module next door. It is here anyway: what is hard about a font
// preload is the fetch — `as`, `crossOrigin`, the silent second download when
// they disagree — and not the tag it happens to be written as. `head.js` is
// for what a component decides to put in the head; this is for what the
// browser is about to download.
//
// Not here: announcing a change (`regions.js`), agreeing with a server about
// an instant (`time.js`) or about a stored value (`cookie.js`).

import * as React from "@uniflowed/react";

/** How an image should be fetched relative to the rest of the page. */
export type Loading = "eager" | "lazy";

/**
 * An `<img>` that does not shift the page while it loads.
 *
 * `width` and `height` are required, and that is the whole point. A browser
 * that knows an image's intrinsic ratio reserves the space before the bytes
 * arrive; one that does not reflows everything below when they do. They are
 * the *intrinsic* dimensions, not a size — CSS still decides how big it looks,
 * and the ratio is what stops the shift.
 *
 * `loading="lazy"` and `decoding="async"` by default, because most images on a
 * page are below the fold and decoding on the main thread blocks it. The image
 * that *is* above the fold should say `loading="eager"`, and saying so is one
 * word.
 */
export component Image(
  src: string,
  alt: string,
  width: number,
  height: number,
  loading?: Loading = "lazy",
  sizes?: string,
  srcSet?: string,
  className?: string,
  ...rest: { readonly [string]: mixed }
) renders React.Node {
  return (
    <img
      src={src}
      alt={alt}
      width={width}
      height={height}
      loading={loading}
      // Always async: decoding a large image synchronously blocks the main
      // thread, and nothing on a page needs that.
      decoding="async"
      sizes={sizes}
      srcSet={srcSet}
      className={className}
      {...rest}
    />
  );
}

/** One alternative rendering of the same image. */
export type Source = {
  readonly srcSet: string,
  readonly type?: string,
  readonly media?: string,
};

/**
 * A `<picture>`: the same image, in formats a browser may prefer.
 *
 * The sources are offered in order and the browser takes the first it
 * understands, so put the format you most want served first — `avif`, then
 * `webp`, then whatever `src` is. The `<img>` is the fallback and is not
 * optional: a `<picture>` with no `<img>` renders nothing at all, which is a
 * mistake that only shows up in the one browser that took none of the sources.
 */
export component Picture(
  src: string,
  alt: string,
  width: number,
  height: number,
  sources?: $ReadOnlyArray<Source> = [],
  loading?: Loading = "lazy",
  className?: string,
) renders React.Node {
  return (
    <picture>
      {sources.map((source) => (
        <source
          key={`${source.type ?? ""}:${source.media ?? ""}:${source.srcSet}`}
          srcSet={source.srcSet}
          type={source.type}
          media={source.media}
        />
      ))}
      <Image
        src={src}
        alt={alt}
        width={width}
        height={height}
        loading={loading}
        className={className}
      />
    </picture>
  );
}

/**
 * Preload a font file, so text does not arrive twice.
 *
 * A font referenced only from CSS is discovered late — the browser has to
 * fetch and parse the stylesheet first — so the first paint uses a fallback
 * and the text reflows when the real face lands. A preload moves the fetch to
 * the start of the page.
 *
 * `crossOrigin` is set unconditionally and deliberately: a font is fetched in
 * CORS mode whatever its origin, so a preload without it is a *second*,
 * separate request rather than the same one — the preload is wasted and the
 * font still arrives late. This is the single most common way to get a font
 * preload wrong, and it fails silently.
 *
 * Write it beside the text that needs the font, not at the root of the page:
 * React hoists a `<link rel="preload">` into `<head>` itself, and dedupes it
 * by href, so the component that depends on a face is the one that asks for
 * it and two components asking for the same face still make one request.
 */
export component Font(
  src: string,
  type?: string = "font/woff2",
  crossOrigin?: "anonymous" | "use-credentials" = "anonymous",
) renders React.Node {
  return <link rel="preload" as="font" href={src} type={type} crossOrigin={crossOrigin} />;
}
