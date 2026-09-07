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

/** One file a font import put in the build. */
export type FontFace = {
  /** Its URL. */
  readonly url: string,
  /** Its media type. */
  readonly mime: string,
  /** Its size in bytes. */
  readonly bytes: number,
  /** Which script bucket it is, when the family was split. */
  readonly bucket: string | null,
  /** The exact `unicode-range` of what is in it. */
  readonly unicodeRange: string | null,
  /**
   * Whether a page should preload this one.
   *
   * At most one face is ever marked. A preload per bucket downloads the whole
   * family up front, which is the one thing a `unicode-range` split exists to
   * stop.
   */
  readonly preload: boolean,
};

/**
 * What importing a font evaluates to.
 *
 * Produced by `uf:asset` from the font's own `head`, `hhea` and `OS/2` tables.
 */
export type FontAsset = {
  /** The primary self-hosted file's URL. */
  readonly src: string,
  /** The family the `@font-face` declares. */
  readonly family: string,
  /** The metric-matched fallback's family, when there is one. */
  readonly fallbackFamily: string | null,
  /** The whole stack, ready for `font-family`. */
  readonly fontFamily: string,
  /** The file's media type, for the preload. */
  readonly type: string,
  /** The `@font-face` rules: every real face, then the matched fallback. */
  readonly css: string,
  /** Every emitted file, one per bucket when the family was split. */
  readonly faces?: $ReadOnlyArray<FontFace>,
  /** Why the whole font was hosted, when a subset was asked for and refused. */
  readonly subsetDeclined?: string | null,
};

/**
 * What importing an Open Graph template evaluates to.
 *
 * Produced by `uf:asset` from a `*.og.json`, which is a declared template and
 * not a document — see `crates/uf_assets/src/og.rs` for what uf will and will
 * not draw.
 */
export type OgAsset = {
  /** The card's URL, relative to the site. */
  readonly url: string,
  /** Its width in pixels. */
  readonly width: number,
  /** Its height in pixels. */
  readonly height: number,
  /** Its media type. */
  readonly type: string,
  /** The `og:image:alt` text. */
  readonly alt: string,
};

/** What importing `uf:icon/<name>` evaluates to. */
export type IconAsset = {
  /** The symbol's id in the sprite. */
  readonly id: string,
  /** The fragment a `<use>` points at. */
  readonly href: string,
  /** The symbol's `viewBox`. */
  readonly viewBox: string,
  /** Its intrinsic width. */
  readonly width: number,
  /** Its intrinsic height. */
  readonly height: number,
};

/** What importing `uf:icon-sprite` evaluates to. */
export type SpriteAsset = {
  /** The `<svg>` holding one `<symbol>` per icon the build reached. */
  readonly markup: string,
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
 * It is also the thing most easily overdone, so exactly one file is ever
 * preloaded: the build marks the face a page paints first, and a family split
 * into eight `unicode-range` buckets still contributes one link. Preloading
 * every bucket downloads the whole family up front, which is what the split
 * existed to stop, and four preloaded faces have pushed the page's own
 * stylesheet down the same connection. Pass `preload={false}` for a face that
 * does not paint the first screen, and `preload={true}` for one the project's
 * `app.builtins.fonts.preload` turned off but this page needs — the prop wins
 * over the build in both directions, and one link is still the most it emits.
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
  preload?: boolean,
) renders React.Node {
  const asset: FontAsset | null = typeof src === "string" ? null : src;
  const url: string = typeof src === "string" ? src : src.src;
  const finalType = type ?? asset?.type ?? "font/woff2";

  // Which file to preload, and it is never more than one. A font hosted whole
  // has a single face and the build decided whether to preload it; a family
  // split by `unicode-range` has one face marked and the rest deliberately
  // unmarked. A string `src` has no manifest to have decided anything, so it
  // keeps the behaviour it always had.
  const declared = asset?.faces ?? [];
  const all =
    declared.length > 0
      ? declared
      : [{ url, mime: finalType, bytes: 0, bucket: null, unicodeRange: null, preload: true }];
  // The prop overrides the build in both directions: `false` emits no link at
  // all, and `true` emits one for the primary face even when the build left
  // every face unmarked — otherwise `preload` would be a prop that can only
  // ever say no, which is not what a boolean looks like.
  const chosen =
    preload === false
      ? null
      : (all.find((face) => face.preload) ?? (preload === true ? all[0] : null));

  const links =
    chosen == null
      ? []
      : [
          <link
            key={chosen.url}
            rel="preload"
            as="font"
            href={chosen.url}
            type={chosen.mime}
            crossOrigin={crossOrigin}
          />,
        ];
  if (asset == null) return <>{links}</>;

  return (
    <>
      {links}
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

/**
 * The sprite holding every icon this build reached.
 *
 * Render it once, near the top of the document. It is a definitions block and
 * not content: it paints nothing, it is `aria-hidden`, and every `<Icon>` on
 * the page is a forty-byte `<use>` pointing into it.
 *
 * The alternative — a component per icon, each carrying its own path data —
 * repeats the same geometry once per use, and a runtime icon library ships
 * every icon it has because at runtime nothing knows which ones the
 * application imported. A build does, which is the whole reason this exists.
 *
 * The markup comes from `uf assets`, which parses each file, refuses the
 * constructs that must not be inlined into a document — `<script>`,
 * `<foreignObject>`, `on…` handlers, `javascript:`, anything fetching from
 * another origin — and namespaces every internal `id` so two icons defining
 * the same gradient do not collide. `dangerouslySetInnerHTML` is the only way
 * to put an already-serialised subtree into the DOM, and what makes it safe
 * here is that the string was produced by uf from files in the repository, not
 * by anything at runtime.
 */
export component IconSprite(sprite: SpriteAsset) renders React.Node {
  return (
    <div
      // Out of the flow entirely rather than `display: none`: a `<use>` may
      // not resolve into a subtree the browser never laid out, and Safari has
      // historically been the one to enforce it.
      style={{ position: "absolute", width: 0, height: 0, overflow: "hidden" }}
      aria-hidden={true}
      // A sprite is a serialised subtree and `dangerouslySetInnerHTML` is the
      // only way to put one in the DOM: React has no other spelling for
      // "these elements already exist as markup", and rebuilding the symbols
      // as elements would mean parsing SVG in the browser, which is the thing
      // this feature exists to have already done at build time.
      //
      // What the rule is protecting against is a string that reaches this
      // attribute from a request, a form, a database — anything a person can
      // put a `<script>` into. This one cannot be any of those. It is
      // assembled by `uf assets` from files in the repository, and that
      // assembly *refuses* rather than strips: a `<script>`, a
      // `<foreignObject>`, an `on…` attribute, a `javascript:` value or a
      // reference out of the file fails the build with the file named. So the
      // only markup that can arrive here is markup a build already read and
      // accepted, which is the same trust boundary as the rest of the bundle.
      // uf-lint-disable-next-line security/no-dangerously-set-inner-html
      dangerouslySetInnerHTML={{ __html: sprite.markup }}
    />
  );
}

/**
 * One icon out of the sprite.
 *
 * `<Icon icon={star} label="Favourite" />` — with a label when the icon *is*
 * the control, and without one when there is text beside it. That is the whole
 * accessibility decision an icon needs and the one people most often get
 * backwards: an unlabelled icon button is a button a screen reader announces
 * as "button", and a labelled icon next to its own visible text is the same
 * word read twice.
 *
 * `size` sets both dimensions, because an icon whose aspect ratio is not its
 * `viewBox`'s is a squashed icon. Pass `width` and `height` separately only
 * when that is what you mean.
 */
export component Icon(
  icon: IconAsset,
  label?: string,
  size?: number = 24,
  width?: number,
  height?: number,
  className?: string,
  ...rest: { readonly [string]: mixed }
) renders React.Node {
  return (
    <svg
      viewBox={icon.viewBox}
      width={width ?? size}
      height={height ?? size}
      className={className}
      // `role="img"` with a label, nothing without one. An `aria-hidden` icon
      // beside its own text is the correct markup, and `focusable="false"`
      // keeps it out of the tab order in the browsers that put SVGs in it.
      role={label != null ? "img" : undefined}
      aria-label={label}
      aria-hidden={label == null ? true : undefined}
      focusable="false"
      {...rest}
    >
      <use href={icon.href} />
    </svg>
  );
}

/**
 * The `<meta>` tags for one Open Graph card.
 *
 * `<OgImage card={card} />` beside the page that owns it. Three things go
 * wrong with an Open Graph image and this gets all three right: the URL has to
 * be **absolute** — a relative `og:image` is not an Open Graph image at all,
 * and every crawler drops it — the dimensions have to be declared or the card
 * flickers at whatever size the crawler guesses, and `twitter:card` has to say
 * `summary_large_image` or X renders a thumbnail of a 1200x630 picture.
 *
 * `origin` is the site's own, and it is required for exactly the reason above.
 * A project using `@uniflowed/router`'s `Metadata` already has it as
 * `metadataBase` and should pass `card` through `openGraph.images` instead of
 * rendering this — one page's metadata belongs in one place. This component is
 * for a page assembling its own head.
 *
 * The card itself is drawn at build time by `uf assets` from a `*.og.json`
 * template. It is a template and not a renderer: uf will not turn JSX into an
 * image, and it refuses text it cannot lay out rather than drawing it wrong.
 */
export component OgImage(card: OgAsset, origin: string, alt?: string) renders React.Node {
  const absolute = card.url.startsWith("http")
    ? card.url
    : `${origin.replace(/\/+$/, "")}/${card.url.replace(/^\/+/, "")}`;
  return (
    <>
      <meta property="og:image" content={absolute} />
      <meta property="og:image:width" content={String(card.width)} />
      <meta property="og:image:height" content={String(card.height)} />
      <meta property="og:image:type" content={card.type} />
      <meta property="og:image:alt" content={alt ?? card.alt} />
      <meta name="twitter:card" content="summary_large_image" />
      <meta name="twitter:image" content={absolute} />
    </>
  );
}
