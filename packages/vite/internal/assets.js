// @noflow
//
// Plain JavaScript: Vite imports this module directly, before any transform.
//
// `uf:asset` — what an imported image or font becomes.
//
// The name and the hook set are not new. `crates/uf_plugin/src/builtin.rs` has
// declared `uf:asset` — "resolves, fingerprints, and emits non-JavaScript
// imports", `resolveId` + `load` + `generateBundle` + `writeBundle` +
// `transformIndexHtml` — since before there was anything behind it, and
// `uf inspect` has been listing it in the resolved pipeline. This is the
// implementation of a plugin uf was already claiming to run — and `resolveId`
// and `generateBundle`, declared there from the start and unimplemented until
// icons arrived, are now real: the first resolves `uf:icon/…`, the second
// assembles the sprite once the graph is complete.
//
// # What an import becomes
//
// ```js
// import hero from "./hero.jpg";
// <Image src={hero} alt="…" sizes="(max-width: 640px) 100vw, 640px" />
// ```
//
// `hero` is not a URL string. It is the manifest `crates/uf_assets` produced —
// the intrinsic width and height, every emitted variant with its own width, the
// blur placeholder — because a `srcSet` can only be written by something that
// knows which other sizes exist, and a URL string does not.
//
// A font import is the same shape: the self-hosted file, the `@font-face` rules
// that declare it, and the metric-matched fallback.
//
// # Where the work happens, and when
//
// In `uf`, over the `uf assets` protocol — one native process for the whole
// build rather than an image codec in the dependency tree. Both schedules go
// through the same process with the same parameters, and both write to the
// same cache directory:
//
// * **`uf build`** reads each emitted variant out of the cache and hands it to
//   Rollup with `emitFile`, so the bundler owns what lands in `dist/` and the
//   size report counts it.
// * **`uf dev`** serves the same files out of the same cache directory over a
//   middleware, transformed on the first import and reused after.
//
// The files are named by a content hash of the source and the parameters, so
// the second build of an unchanged image does no work in either mode and a dev
// session warms the cache a build then reuses. That is also the whole of "the
// two must agree": there is one pipeline and one set of bytes, and the only
// thing that differs between them is the URL prefix they are served under.
//
// # Icons, and why they are not `.svg`
//
// `import Star from "uf:icon/star"` resolves against the directory
// `app.builtins.icons.dir` names, and `import sprite from "uf:icon-sprite"` is
// the sprite built from every icon the build reached. Neither claims an
// extension, which is the point: `.svg` stays Vite's, so `vite-plugin-svgr`
// and everything like it keep working, and uf adds a capability in its own
// namespace instead of taking one away.
//
// The sprite is emitted in `generateBundle`, which is the first hook that runs
// after every module has been loaded — and therefore the first moment the set
// of icons a build reached is the whole set.
//
// # What this plugin deliberately does not claim
//
// An import with a query — `./hero.png?url`, `?raw`, `?inline` — is left to
// Vite. Those are Vite's own asset conventions and a project reaching for one
// is reaching past uf on purpose; claiming them here would make a documented
// Vite feature unreachable from a uf project, which is red line 8 in
// `docs/red-lines.md`. `import hero from "./hero.png"` is uf's; everything
// with a `?` after it is Vite's.

import { existsSync, readFileSync } from "node:fs";
import path from "node:path";

import { AssetService, ICON_PREFIX, ICON_SPRITE, assetKind } from "@uniflowed/host/assets";

/** Where transformed assets are kept, relative to the project root. */
export const CACHE_DIR = ".uf/cache/assets";

/**
 * The URL prefix a dev server answers transformed assets on.
 *
 * `@` first, following the convention Vite uses for everything that is not a
 * file in the project: it cannot collide with a real path, and Vite's own
 * middlewares leave it alone.
 */
export const DEV_PREFIX = "@uf-asset/";

/**
 * What stands in for the sprite until `generateBundle` knows what is in it.
 *
 * A string no source file would contain, replaced in the generated chunk once
 * every module has been loaded. The module graph needs *something* at `load`
 * time and the sprite is not knowable then; see the `sprite` branch of `load`.
 */
const SPRITE_PLACEHOLDER = "__UF_ICON_SPRITE__";

/**
 * The module source for one transformed asset.
 *
 * A frozen object literal rather than a JSON blob assigned to a variable: this
 * is what the component destructures, it is small, and a build that inlines it
 * into the one component that used it is the right outcome.
 */
export function assetModuleSource(manifest) {
  return `export default Object.freeze(${JSON.stringify(manifest)});\n`;
}

/**
 * The `srcSet` for one format, and the URLs that go in it.
 *
 * Written here rather than in the component so that a build and a dev server
 * cannot produce different strings from the same manifest: the only input that
 * differs between them is `baseUrl`, and it is an argument.
 */
export function withUrls(image, baseUrl) {
  const variants = image.variants.map((variant) => ({
    ...variant,
    url: `${baseUrl}${variant.file}`,
  }));
  // Widest last within a format, which is the order a `srcset` reads best in
  // and the order `sizes` is evaluated against.
  variants.sort((left, right) => left.width - right.width);

  const formats = [];
  for (const variant of variants) {
    if (variant.format === image.format) continue;
    if (!formats.includes(variant.format)) formats.push(variant.format);
  }

  const srcSetFor = (format) =>
    variants
      .filter((variant) => variant.format === format)
      // `640w` and not `2x`: a density descriptor describes one layout width,
      // and the whole point of the ladder is that the layout width is not
      // known here. With `w`, the browser combines it with `sizes` and picks.
      .map((variant) => `${variant.url} ${variant.width}w`)
      .join(", ");

  const fallbacks = variants.filter((variant) => variant.format === image.format);
  const widest = fallbacks[fallbacks.length - 1] ?? variants[variants.length - 1];

  return {
    src: widest?.url ?? null,
    width: image.width,
    height: image.height,
    srcSet: srcSetFor(image.format),
    // Alternatives first: a browser takes the first `<source>` it understands,
    // so the format every browser understands must not be offered before the
    // ones that are smaller.
    sources: formats.map((format) => ({
      type: variants.find((variant) => variant.format === format).mime,
      srcSet: srcSetFor(format),
    })),
    blurDataURL: image.blur,
    // Carried through so a project can see what the pipeline decided and why,
    // rather than having to infer it from what is missing: `hero.declined` is
    // the widths where the alternative format was encoded and came out larger,
    // with both byte counts. Nothing prints them — a line on every build about
    // a format that was correctly not emitted is noise — and `uf explain build`
    // is where the limit itself is stated.
    declined: image.declined,
    note: image.note,
  };
}

/**
 * uf's asset pipeline, as a Vite plugin.
 *
 * @param {object} options
 * @param {object} [options.images] `app.builtins.images`
 * @param {object} [options.fonts] `app.builtins.fonts`
 * @param {object} [options.icons] `app.builtins.icons`
 * @param {object} [options.og] `app.builtins.og`
 * @param {string} [options.command] the `uf` binary to transform through
 */
export function assetPlugin({ images = {}, fonts = {}, icons = {}, og = {}, command } = {}) {
  // Both halves can be turned off independently, and a plugin that is off is
  // still in the array: `uf inspect` lists the resolved pipeline, and a
  // pipeline that changes shape when a feature is disabled is a pipeline whose
  // listing cannot be compared between two projects.
  const imagesOn = images.enabled !== false;
  const fontsOn = fonts.enabled !== false;
  const iconsOn = icons.enabled !== false;
  const ogOn = og.enabled !== false;
  const iconDir = icons.dir ?? "icons";

  let root = process.cwd();
  let base = "/";
  let assetsDir = "assets";
  let isBuild = false;
  /** @type {import("vite").ViteDevServer | null} */
  let server = null;
  /** @type {AssetService | null} */
  let service = null;
  /**
   * The manifest for each source path, so one image is transformed once.
   *
   * `uf build` runs Vite twice over the same modules — once for the browser
   * bundle and once for the server one — from a single plugin array, so both
   * passes share this map and the second decodes nothing. It survives
   * `buildEnd` deliberately: clearing it there is what made the server pass
   * redo every image, which is the whole cost this map exists to avoid.
   */
  const transformed = new Map();
  /** Whether anything imported the sprite at all. */
  let spriteRequested = false;

  const cacheDir = () => path.resolve(root, CACHE_DIR);
  /**
   * The `uf assets` process, started on the first asset and not before.
   *
   * Lazily rather than in `buildStart`, which is where `uf:flow` starts its
   * transform service: that one is going to be asked about every module in the
   * project, and this one is asked about nothing at all in a project that
   * imports no images or fonts. A build that has no use for an image codec
   * should not spawn one.
   */
  const ensureService = () => {
    service ??= new AssetService({ command, root });
    return service;
  };

  /**
   * Where a transformed file is served from.
   *
   * A build's URL is the bundler's output directory; a dev server's is this
   * plugin's own middleware. This is the *only* thing that differs between the
   * two schedules, and it is one string.
   */
  const baseUrl = () => (isBuild ? `${base}${assetsDir}/` : `${base}${DEV_PREFIX}`);

  const claims = (id) => {
    // A query is Vite's, not uf's. See the header.
    if (id.includes("?")) return null;
    if (id.startsWith("\0")) return null;
    const kind = assetKind(id);
    if (kind === "image" && !imagesOn) return null;
    if (kind === "font" && !fontsOn) return null;
    if (kind === "og" && !ogOn) return null;
    if ((kind === "icon" || kind === "sprite") && !iconsOn) return null;
    return kind;
  };

  /**
   * The file one `uf:icon/<name>` names.
   *
   * The name is a path segment and nothing else. `..` in it would reach out of
   * the icon directory and turn an import into a way to read the repository,
   * so it is rejected rather than resolved — the same rule the dev middleware
   * applies to an emitted file name.
   */
  const iconFile = (id) => {
    const name = id.slice(ICON_PREFIX.length);
    if (name === "" || name.includes("..") || path.isAbsolute(name)) return null;
    return { name, file: path.resolve(root, iconDir, `${name}.svg`) };
  };

  /** Tell a dev server the sprite it already sent is missing an icon. */
  const invalidateSprite = () => {
    if (server == null || !spriteRequested) return;
    const module = server.moduleGraph.getModuleById(ICON_SPRITE);
    if (module != null) server.moduleGraph.invalidateModule(module);
  };

  return {
    name: "uf:asset",
    // Before Vite's own asset handling, which would otherwise claim the same
    // extensions and return a URL string.
    enforce: "pre",

    configResolved(config) {
      root = config.root;
      base = config.base;
      assetsDir = config.build?.assetsDir ?? "assets";
      isBuild = config.command === "build";
    },

    // `uf:icon/…` and `uf:icon-sprite` are uf's own ids and resolve to
    // themselves. Returning the id unchanged rather than a `\0`-prefixed one
    // keeps it readable in a stack trace and in `vite --debug`, and nothing
    // else in the pipeline claims the `uf:` scheme.
    resolveId(id) {
      if (!iconsOn) return null;
      if (id === ICON_SPRITE || id.startsWith(ICON_PREFIX)) return id;
      return null;
    },

    async load(id) {
      const kind = claims(id);
      if (kind == null) return null;

      if (kind === "sprite") {
        spriteRequested = true;
        // A build cannot know the sprite here: `load` runs while the graph is
        // still being walked, so the set of icons reached so far is not the
        // set. It emits a placeholder that `generateBundle` — the first hook
        // after every module has been loaded — replaces with the real markup.
        //
        // A dev server has no `generateBundle`, so it assembles from what has
        // been reached and invalidates this module whenever a new icon turns
        // up. That costs one extra reload the first time a page introduces an
        // icon and is exactly right afterwards, which is the trade a dev
        // server makes everywhere else too.
        if (isBuild) return assetModuleSource({ markup: SPRITE_PLACEHOLDER });
        const sprite = await ensureService().sprite({ outDir: cacheDir() });
        return assetModuleSource({ markup: sprite.markup });
      }

      if (kind === "icon") {
        const resolved = iconFile(id);
        if (resolved == null) return null;
        if (!existsSync(resolved.file)) {
          this.error(
            `${id} does not exist: uf looked for ${path.relative(root, resolved.file)}. ` +
              "`app.builtins.icons.dir` is where `uf:icon/…` resolves against.",
          );
        }
        const asset = await ensureService().icon(resolved.file, {
          outDir: cacheDir(),
          name: resolved.name,
        });
        this.addWatchFile(resolved.file);
        invalidateSprite();
        return assetModuleSource({
          id: asset.id,
          href: `#${asset.id}`,
          viewBox: asset.viewBox,
          width: asset.width,
          height: asset.height,
        });
      }

      const file = path.resolve(id);
      // Not this plugin's to fail on: an id with one of these extensions that
      // is not a file on disk is a virtual module somebody else owns.
      if (!existsSync(file)) return null;

      return loadAsset.call(this, {
        kind,
        file,
        transformed,
        service: ensureService(),
        cacheDir: cacheDir(),
        baseUrl: baseUrl(),
        assetsDir,
        isBuild,
        images,
        fonts,
      });
    },

    // After every module has been loaded, which is the first moment the set of
    // icons this build reached is the whole set. A sprite assembled in `load`
    // would hold the icons reached *so far*, which is a different sprite on
    // every run depending on module order.
    async generateBundle(_options, bundle) {
      if (!iconsOn || !spriteRequested) return;
      const sprite = await ensureService().sprite({ outDir: cacheDir() });
      this.emitFile({ type: "asset", fileName: `${assetsDir}/${sprite.file}`, source: sprite.markup });
      // The placeholder is replaced rather than the module re-run: by
      // `generateBundle` the chunk is already generated, and the sprite is one
      // string in it.
      for (const chunk of Object.values(bundle)) {
        if (chunk.type === "chunk" && chunk.code.includes(SPRITE_PLACEHOLDER)) {
          chunk.code = chunk.code.replace(
            new RegExp(SPRITE_PLACEHOLDER, "g"),
            JSON.stringify(sprite.markup).slice(1, -1),
          );
        }
      }
    },

    configureServer(devServer) {
      server = devServer;
      devServer.httpServer?.once("close", () => {
        service?.close();
        service = null;
      });

      // Before Vite's own middlewares: nothing else knows this prefix, and the
      // files are outside the module graph, so there is nothing to wait for.
      const directory = cacheDir();
      devServer.middlewares.use((request, response, next) => {
        const url = request.url ?? "";
        const at = url.indexOf(DEV_PREFIX);
        if (at === -1) return next();
        const name = decodeURIComponent(url.slice(at + DEV_PREFIX.length).split("?")[0]);
        // The name is a file name and nothing else. Every emitted name is one
        // path segment by construction, so a request carrying a separator is
        // not a name this plugin ever minted — refusing it rather than
        // resolving it is what keeps the cache directory from being a way to
        // read the rest of the disk.
        if (name === "" || name.includes("/") || name.includes("\\") || name.includes("..")) {
          response.statusCode = 400;
          response.end("bad asset name");
          return;
        }
        const target = path.join(directory, name);
        if (!existsSync(target)) return next();
        response.setHeader("Content-Type", contentTypeOf(name));
        // The name is a content hash, so the bytes under it never change.
        response.setHeader("Cache-Control", "public, max-age=31536000, immutable");
        response.end(readFileSync(target));
      });
    },

    watchChange(id) {
      // The memo below is what stops `uf build` decoding every image twice,
      // once per bundle. In a dev server it would also stop uf ever noticing
      // that an image was edited: Vite invalidates the module and calls `load`
      // again, and `load` would hand back the manifest it made before the
      // change. Dropping both keys is cheap and the next `load` redoes the
      // work — which, because the emitted names are content hashes, writes new
      // files and leaves the old ones for anything still holding a URL.
      transformed.delete(`image:${path.resolve(id)}`);
      transformed.delete(`font:${path.resolve(id)}`);
      transformed.delete(`og:${path.resolve(id)}`);
    },

    buildEnd() {
      // A dev server keeps its service for the whole session; a build is done
      // with it here. The same rule `uf:flow` follows next door.
      //
      // `transformed` is *not* cleared. A build's second pass over the same
      // modules then needs no process at all — every answer is already in the
      // map, and `ensureService` is never reached.
      if (server == null) {
        service?.close();
        service = null;
      }
    },
  };
}

/**
 * Transform one asset and return the module that stands for it.
 *
 * Split out of the hook so the hook stays readable and so the memoisation is
 * visible: `uf build` runs Vite twice over the same modules, once for the
 * browser bundle and once for the server one, and an image transformed on both
 * passes would be decoded twice for one build.
 */
async function loadAsset(context) {
  const { kind, file, transformed, service, cacheDir, baseUrl, assetsDir, isBuild, images, fonts } =
    context;
  const key = `${kind}:${file}`;
  let manifest = transformed.get(key);
  if (manifest == null) {
    if (kind === "image") {
      manifest = await service.image(file, {
        outDir: cacheDir,
        widths: images.widths,
        quality: images.quality,
        blur: images.placeholder,
      });
    } else if (kind === "og") {
      manifest = await service.og(file, { outDir: cacheDir });
    } else {
      manifest = await service.font(file, {
        outDir: cacheDir,
        family: fonts.family,
        display: fonts.display,
        subset: fonts.subset,
        preload: fonts.preload,
        baseUrl,
      });
    }
    transformed.set(key, manifest);
  }

  const files =
    kind === "image"
      ? manifest.variants.map((v) => v.file)
      : kind === "og"
        ? [manifest.file]
        : // Every bucket, not just the primary: a `unicode-range` split emits
          // one file per script and the browser fetches whichever the page
          // needs. Emitting only the one the manifest calls primary would
          // leave the others named in the stylesheet and absent from `dist/`.
          manifest.faces.map((face) => face.file);
  if (isBuild) {
    // Handed to Rollup rather than copied by hand, so the bundler owns what
    // lands in the output directory and `uf_bundle`'s size report — which
    // walks that directory — counts every one of them.
    for (const name of files) {
      this.emitFile({
        type: "asset",
        // `fileName` rather than `name`: the name is already a content hash of
        // the source and the parameters, and letting Rollup hash it again
        // would move it on every encoder change while saying nothing new.
        fileName: `${assetsDir}/${name}`,
        source: readFileSync(path.join(cacheDir, name)),
      });
    }
  }

  if (kind === "image") {
    return assetModuleSource(withUrls(manifest, baseUrl));
  }
  if (kind === "og") {
    return assetModuleSource({
      // The shape `Metadata.openGraph.images` and `OgImage` both read. A URL,
      // a size and the alt text, which is all a card ever is to a page.
      url: `${baseUrl}${manifest.file}`,
      width: manifest.width,
      height: manifest.height,
      type: manifest.mime,
      alt: manifest.alt,
      bytes: manifest.bytes,
    });
  }
  return assetModuleSource({
    src: `${baseUrl}${manifest.file}`,
    family: manifest.family,
    fallbackFamily: manifest.fallbackFamily,
    // The stack a page should set `font-family` to: the real face, then the
    // metric-matched fallback, then the local face it was scaled from. Written
    // here so no page has to remember that the fallback only ever applies when
    // it is named after the real face.
    fontFamily: [manifest.family, manifest.fallbackFamily, manifest.fallback?.local]
      .filter((name) => name != null)
      .map((name) => JSON.stringify(name))
      .join(", "),
    type: manifest.mime,
    css: manifest.css,
    metrics: manifest.metrics,
    fallback: manifest.fallback,
    fallbackDeclined: manifest.fallbackDeclined,
    // Every emitted file with its URL, so `Font` can preload exactly the one
    // marked rather than all of them — which is the one thing that would undo
    // a `unicode-range` split.
    faces: (manifest.faces ?? []).map((face) => ({
      ...face,
      url: `${baseUrl}${face.file}`,
    })),
    subset: manifest.subset ?? null,
    // Carried through for the same reason `declined` is on an image: a project
    // that asked for a subset and got the whole font is entitled to the
    // sentence saying why, without reading this plugin.
    subsetDeclined: manifest.subsetDeclined ?? null,
    sourceBytes: manifest.sourceBytes ?? null,
  });
}

/** The media type for one emitted file name. */
function contentTypeOf(name) {
  const extension = name.slice(name.lastIndexOf(".") + 1).toLowerCase();
  const types = {
    avif: "image/avif",
    gif: "image/gif",
    jpg: "image/jpeg",
    jpeg: "image/jpeg",
    otf: "font/otf",
    png: "image/png",
    svg: "image/svg+xml",
    ttf: "font/ttf",
    webp: "image/webp",
    woff: "font/woff",
    woff2: "font/woff2",
  };
  return types[extension] ?? "application/octet-stream";
}
