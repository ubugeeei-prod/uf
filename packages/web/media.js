// @flow
//
// `@uniflowed/web/media`: the elements that load bytes.
//
// An image, a `<picture>`, and a font. They are grouped because they share one
// problem: each one is a request the browser has to make while the page is
// still being laid out, and each has a well-known way of being got wrong that
// costs either a layout shift or a wasted download.
//
// Every default here is the one that is right more often than not, and every
// one of them can be overridden — a component whose defaults cannot be turned
// off is a component people stop using.
//
// # These render a pipeline's output, not just markup
//
// `Image` and `Font` accept the value an `import` of an image or a font
// evaluates to. That value is not a URL: `uf:asset` — the Vite plugin in
// `@uniflowed/vite`, driving `crates/uf_assets` through `uf assets` — decoded
// the file at build time, wrote a variant at every width the project declares,
// re-encoded each one in a modern format where the modern format was actually
// smaller, and measured the font's real metrics. What arrives here is that
// manifest.
//
// Which is what makes these components worth using instead of `<img>` and
// `@font-face`. A `srcSet` can only be written by something that knows which
// other sizes exist; a `size-adjust` can only be computed by something that has
// read the font's `OS/2` table. Neither is knowable from a string, and before
// the pipeline existed these components had a string and wrote the attribute
// the author handed them.
//
// A string still works, and still means exactly what it used to: this is the
// escape hatch for an image uf did not build — one from a CMS, one in
// `public/` — and for it the author supplies `width`, `height` and `srcSet`
// themselves, as they always did.
//
// # What belongs in this module
//
// Anything a page renders in order to make the browser fetch bytes while the
// layout is still being decided: another `<source>` form, a video poster, a
// preload for a stylesheet. The test is whether the mistake it prevents is a
// wasted request or a shifted layout.
//
// `Font` is the boundary worth stating, because it is a `<link>` and a
// `<style>` and `<head>` has its own module next door. It is here anyway: what
// is hard about a font is the fetch — `as`, `crossOrigin`, the silent second
// download when they disagree, the reflow when the real face lands — and not
// the tags it happens to be written as. `head.js` is for what a component
// decides to put in the head; this is for what the browser is about to
// download.
//
// Not here: announcing a change (`regions.js`), agreeing with a server about
// an instant (`time.js`) or about a stored value (`cookie.js`).

import * as React from "@uniflowed/react";

/** How an image should be fetched relative to the rest of the page. */
export type Loading = "eager" | "lazy";

/** One alternative rendering of the same image. */
export type Source = {
  readonly srcSet: string,
  readonly type?: string,
  readonly media?: string,
};

/**
 * What importing an image evaluates to.
 *
 * Produced by `uf:asset`; see `packages/vite/internal/assets.js`. Every field
 * is a fact about files that exist on disk, which is the difference between
 * this and a set of props.
 */
export type ImageAsset = {
  /** The widest variant in the fallback format — what `src` points at. */
  readonly src: string | null,
  /** The source's intrinsic width, or `null` when uf could not decode it. */
  readonly width: number | null,
  /** The source's intrinsic height. */
  readonly height: number | null,
  /** Every fallback-format variant, as a `srcset` string. */
  readonly srcSet: string,
  /** Modern formats that came out smaller, widest-preferred first. */
  readonly sources: $ReadOnlyArray<Source>,
  /**
   * A tiny inline image to show until the real one lands.
   *
   * `null` for an image with transparency: the placeholder is painted under
   * the image, so under one with holes in it it never goes away.
   */
  readonly blurDataURL: string | null,
  /** Why nothing was resized, when nothing was. */
  readonly note: string | null,
};

/**
 * What importing a font evaluates to.
 *
 * Produced by `uf:asset` from the font's own `head`, `hhea` and `OS/2` tables.
 */
export type FontAsset = {
  /** The self-hosted file's URL. */
  readonly src: string,
  /** The family the `@font-face` declares. */
  readonly family: string,
  /** The metric-matched fallback's family, when there is one. */
  readonly fallbackFamily: string | null,
  /** The whole stack, ready for `font-family`. */
  readonly fontFamily: string,
  /** The file's media type, for the preload. */
  readonly type: string,
  /** The `@font-face` rules: the real face, then the matched fallback. */
  readonly css: string,
};

/**
 * The `sizes` to use when the author did not say.
 *
 * A `srcset` with `w` descriptors and no `sizes` means `100vw` to the browser,
 * which makes it fetch a variant wide enough for the whole viewport however
 * small the image is drawn — the ladder is built and then the widest rung is
 * taken every time. Capping at the intrinsic width is the honest default: it
 * never asks for more pixels than exist, and it is exactly right for the
 * common case of an image drawn at its natural size.
 *
 * It is still a guess about a layout this component cannot see, so a page that
 * knows better should say so. That is what the prop is for.
 */
function defaultSizes(width: number): string {
  return `(max-width: ${width}px) 100vw, ${width}px`;
}

/**
 * An `<img>` that does not shift the page while it loads.
 *
 * `width` and `height` are the *intrinsic* dimensions, not a size — CSS still
 * decides how big it looks, and the ratio is what stops the shift. A browser
 * that knows an image's intrinsic ratio reserves the space before the bytes
 * arrive; one that does not reflows everything below when they do.
 *
 * They come from the import when `src` is an imported image, because uf
 * decoded the file and knows them. They are required when `src` is a string,
 * because nothing else does — and a missing dimension throws rather than
 * rendering an `<img>` that will move the page, which is the mistake this
 * component exists to prevent and is not worth failing quietly at.
 *
 * `loading="lazy"` and `decoding="async"` by default, because most images on a
 * page are below the fold and decoding on the main thread blocks it. The one
 * image that *is* above the fold should say `priority`, which makes it eager,
 * gives it a high fetch priority, and preloads it — a `srcSet` behind a lazy
 * `<img>` is discovered late, and for the largest image on the page that is
 * usually the whole of the Largest Contentful Paint.
 */
export component Image(
  src: string | ImageAsset,
  alt: string,
  width?: number,
  height?: number,
  loading?: Loading,
  priority?: boolean = false,
  sizes?: string,
  srcSet?: string,
  placeholder?: boolean = true,
  className?: string,
  style?: { readonly [string]: string | number },
  ...rest: { readonly [string]: mixed }
) renders React.Node {
  // Refined rather than cast: `src` is a union and `typeof` narrows it, so
  // neither branch needs `any`. A cast here would be the one place in this
  // module where the manifest's shape stopped being checked.
  const asset: ImageAsset | null = typeof src === "string" ? null : src;
  const url: string | null = typeof src === "string" ? src : src.src;
  const finalWidth = width ?? asset?.width ?? null;
  const finalHeight = height ?? asset?.height ?? null;

  if (finalWidth == null || finalHeight == null) {
    // Not a warning. An `<img>` with no intrinsic ratio moves every line below
    // it when it lands, and the whole reason to write `<Image>` rather than
    // `<img>` is that this cannot happen by accident. When uf could not decode
    // the file it says so in `note`, and the fix is to pass the dimensions.
    const because =
      asset?.note != null
        ? ` uf did not measure it: ${asset.note}`
        : " uf has no manifest for a string `src`.";
    throw new Error(
      `<Image src=${JSON.stringify(url ?? "")}> needs \`width\` and \`height\`.${because}`,
    );
  }
  if (url == null) {
    throw new Error("<Image> was given an imported image with no emitted file.");
  }

  const finalSrcSet = srcSet ?? asset?.srcSet;
  const finalSizes =
    sizes ?? (finalSrcSet != null && finalSrcSet !== "" ? defaultSizes(finalWidth) : undefined);
  const finalLoading: Loading = loading ?? (priority ? "eager" : "lazy");
  const blur = placeholder ? asset?.blurDataURL : null;

  const image = (
    <img
      src={url}
      alt={alt}
      width={finalWidth}
      height={finalHeight}
      loading={finalLoading}
      // Always async: decoding a large image synchronously blocks the main
      // thread, and nothing on a page needs that.
      decoding="async"
      // Only when it was asked for. `fetchPriority="auto"` is the default and
      // writing it out says nothing; `"high"` on every image says nothing too,
      // because a priority everything has is a priority nothing has.
      fetchPriority={priority ? "high" : undefined}
      sizes={finalSizes}
      srcSet={finalSrcSet}
      className={className}
      style={
        blur == null
          ? style
          : {
              // The placeholder is a background rather than a second `<img>`:
              // it needs no element, it cannot be announced twice to a screen
              // reader, and the browser paints over it as the real image
              // decodes instead of swapping one node for another.
              backgroundImage: `url("${blur}")`,
              backgroundSize: "cover",
              backgroundPosition: "center",
              ...style,
            }
      }
      {...rest}
    />
  );

  // A preload, for the one image the page is judged on. React hoists it into
  // `<head>` and dedupes by href, so writing it here — beside the image it is
  // about — costs nothing and cannot drift from what the `<img>` asks for:
  // `imageSrcSet` and `imageSizes` have to match, or the preload fetches a
  // different variant than the one the `<img>` then requests and the page pays
  // for both.
  const preload =
    priority === true ? (
      <link
        rel="preload"
        as="image"
        href={url}
        imageSrcSet={finalSrcSet}
        imageSizes={finalSizes}
        fetchPriority="high"
      />
    ) : null;

  const sources = asset?.sources ?? [];
  if (sources.length === 0) {
    return (
      <>
        {preload}
        {image}
      </>
    );
  }

  return (
    <>
      {preload}
      <picture>
        {sources.map((source) => (
          <source
            key={`${source.type ?? ""}:${source.media ?? ""}`}
            srcSet={source.srcSet}
            type={source.type}
            media={source.media}
            sizes={finalSizes}
          />
        ))}
        {image}
      </picture>
    </>
  );
}

/**
 * A `<picture>`: the same image, in formats a browser may prefer.
 *
 * For sources a project assembled itself. An imported image needs none of
 * this — `Image` renders the `<picture>` when the pipeline produced an
 * alternative format, and skips it when it did not, which is the difference
 * between offering a format and offering a format that is smaller.
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
 * Self-host a font, declare it, and preload it.
 *
 * # The preload
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
 * # The `@font-face`, and the one that stops the reflow
 *
 * Given an imported font, this also emits the rules that declare it. There are
 * two: the real face, and a second face over a font the reader already has,
 * carrying the `size-adjust`, `ascent-override`, `descent-override` and
 * `line-gap-override` uf computed from the real font's own metrics. Scale the
 * fallback until it occupies the same space and the swap moves nothing — which
 * is the half of `font-display: swap` that everybody wants and nobody gets by
 * default.
 *
 * The rules only apply to text that asks for them, so use `fontFamily` from the
 * import: it names the real face, then the matched fallback, then the local
 * face it was scaled from, in that order. Naming only the real family gets the
 * download and none of the metric matching.
 *
 * # One weight per import
 *
 * The generated face is `font-weight: 400; font-style: normal`, and the family
 * is the file's stem: `Inter-Bold.woff2` is the family `Inter-Bold`, not
 * `Inter` at 700, so `font-weight: bold` will not reach it. Render it under its
 * own `fontFamily` instead. A variable font is one file covering the range and
 * is unaffected. `uf assets` accepts `weight` and `style` already; what does
 * not exist yet is a way for an import to say them.
 *
 * Write it beside the text that needs the font, not at the root of the page:
 * React hoists both `<link rel="preload">` and `<style>` into `<head>` itself,
 * and dedupes them, so the component that depends on a face is the one that
 * asks for it and two components asking for the same face still make one
 * request.
 */
export component Font(
  src: string | FontAsset,
  type?: string,
  crossOrigin?: "anonymous" | "use-credentials" = "anonymous",
) renders React.Node {
  const asset: FontAsset | null = typeof src === "string" ? null : src;
  const url: string = typeof src === "string" ? src : src.src;
  const finalType = type ?? asset?.type ?? "font/woff2";

  const preload = (
    <link rel="preload" as="font" href={url} type={finalType} crossOrigin={crossOrigin} />
  );
  if (asset == null) return preload;

  return (
    <>
      {preload}
      {/*
        Hoisted and deduped by React, keyed on `href`. The href is the font's
        own content-hashed URL, so two components asking for the same face
        contribute one `<style>` and a project with two faces gets two.
        `precedence` is what makes React hoist it at all; "high" puts the
        `@font-face` rules above the application's own stylesheets, which is
        where a declaration the rest of the sheet refers to belongs.
      */}
      <style href={url} precedence="high">
        {asset.css}
      </style>
    </>
  );
}
