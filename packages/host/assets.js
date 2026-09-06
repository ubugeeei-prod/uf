// @noflow
//
// Plain JavaScript: this runs in the host that runs Vite, beside
// `transform.js`, and reaching the Flow transform is what that file is for.
//
// The JavaScript side of the `uf assets` service.
//
// Decoding, resizing and re-encoding images lives in `crates/uf_assets` and is
// reached exactly the way the Flow transform is reached: one long-lived `uf`
// process per host process, newline-delimited JSON in, replies in request
// order out. The protocol is documented on the other side, in
// `crates/uf_cli/src/commands/assets.rs`.
//
// Why a second service rather than a second message on the transform one: the
// transform service runs on a thread with half a gigabyte of stack because
// every stage of the Flow chain recurses, and it is started for every host
// that touches a module — the Node loader hook and the Bun preload included,
// neither of which has any use for an image pipeline. A build that imports no
// images should not start a process that can resize them, and a `uf test` run
// should not start one at all.

import { spawn } from "node:child_process";
import { createInterface } from "node:readline";

import { ufBinary, ufBinaryIdentity } from "./transform.js";

/**
 * File extensions the pipeline claims.
 *
 * `.svg` is deliberately not among them. uf could only ever copy one through —
 * it is already resolution independent, so there is nothing to resize — and
 * claiming it would take `.svg` away from `vite-plugin-svgr` and everything
 * like it, which turn one into a component. A plugin that claims an extension
 * to do nothing with it is the shape of red line 8 in `docs/red-lines.md`: if
 * Vite can do it, a uf project can do it. So an SVG import stays Vite's, and
 * `<Image src={url} width={…} height={…} />` is how you render one.
 *
 * `.gif` and `.avif` *are* claimed even though no decoder for them is compiled
 * in, and that is the opposite decision for a reason: uf has something to say
 * about them. The import still evaluates to a manifest, and the manifest
 * carries a `note` saying the file was served unchanged and why — which is
 * what tells an author their AVIF is not being resized, rather than leaving
 * them to notice.
 */
export const IMAGE_EXTENSIONS = [".png", ".jpg", ".jpeg", ".webp", ".gif", ".avif"];

/** Font file extensions the pipeline claims. */
export const FONT_EXTENSIONS = [".woff2", ".woff", ".ttf", ".otf"];

/**
 * Whether this import is one uf's asset pipeline handles, and as what.
 *
 * Returns `"image"`, `"font"`, or `null`. The query string is stripped first:
 * `./hero.png?width=400` is an image, and the query is how an import says what
 * it wants.
 */
export function assetKind(id) {
  const clean = stripQuery(id);
  const lower = clean.toLowerCase();
  if (IMAGE_EXTENSIONS.some((extension) => lower.endsWith(extension))) return "image";
  if (FONT_EXTENSIONS.some((extension) => lower.endsWith(extension))) return "font";
  return null;
}

function stripQuery(id) {
  const at = id.indexOf("?");
  return at === -1 ? id : id.slice(0, at);
}

/** An asset the pipeline could not process. */
export class AssetError extends Error {
  constructor(id, message) {
    super(message);
    this.name = "AssetError";
    this.id = id;
  }
}

/**
 * One `uf assets` process, with requests answered in the order they were sent.
 *
 * The same arrangement as `TransformService` next door, including the reason
 * there are no correlation ids: the service replies once per request and in
 * order, so a plain queue of resolvers pairs a reply with its caller. Any exit
 * is final and every outstanding request is rejected at once.
 */
export class AssetService {
  #child;
  #pending = [];
  #identity;
  #failure = null;

  /**
   * @param {object} [options]
   * @param {string} [options.command] the `uf` binary; `ufBinary()` by default
   * @param {string} [options.root] project root, so `uf.config.js` is found
   */
  constructor(options = {}) {
    const command = options.command ?? ufBinary();
    const root = options.root ?? process.cwd();
    // Read before the spawn and kept, for the reason `TransformService` gives:
    // the child goes on executing the binary it started from however many
    // times that file is rewritten underneath it, and anything cached from an
    // answer of this service belongs under *this* identity.
    this.#identity = ufBinaryIdentity(command);
    this.#child = spawn(command, ["--cwd", root, "assets"], {
      stdio: ["pipe", "pipe", "inherit"],
    });

    createInterface({ input: this.#child.stdout }).on("line", (line) => {
      const waiting = this.#pending.shift();
      if (!waiting) return;
      let reply;
      try {
        reply = JSON.parse(line);
      } catch {
        waiting.reject(new Error(`uf assets sent a malformed reply: ${line}`));
        return;
      }
      if (reply.error != null) {
        waiting.reject(new AssetError(waiting.id, reply.error));
        return;
      }
      waiting.resolve(reply);
    });

    this.#child.on("error", (error) => {
      this.#settleAll(new Error(`could not run \`${command} assets\`: ${error.message}`));
    });
    this.#child.on("close", (code) => {
      this.#settleAll(new Error(`uf assets exited (${code})`));
    });
  }

  #send(request) {
    if (this.#failure) return Promise.reject(this.#failure);
    return new Promise((resolve, reject) => {
      this.#pending.push({ id: request.id, resolve, reject });
      this.#child.stdin.write(`${JSON.stringify(request)}\n`);
    });
  }

  #settleAll(error) {
    this.#failure = error;
    while (this.#pending.length > 0) this.#pending.shift().reject(error);
  }

  /**
   * Resize and re-encode one image.
   *
   * Resolves to the manifest the component reads: `{ width, height, format,
   * variants, blur, declined, note }`. Every field is named here rather than
   * passed through, which is deliberate and is the bug `transform.js` records
   * next door: a shim that copies three of four fields drops the fourth
   * silently, and the caller sees `undefined` rather than an error.
   *
   * @param {string} id absolute path to the source image
   * @param {object} options
   * @param {string} options.outDir where variants are written
   * @param {number[]} [options.widths] overriding the project's
   * @param {number} [options.quality]
   * @param {boolean} [options.blur]
   */
  async image(id, options) {
    const reply = await this.#send({
      kind: "image",
      id,
      outDir: options.outDir,
      widths: options.widths,
      quality: options.quality,
      blur: options.blur,
    });
    const image = reply.image;
    if (image == null) throw new AssetError(id, "uf assets returned no image");
    return {
      width: image.width ?? null,
      height: image.height ?? null,
      format: image.format,
      variants: image.variants ?? [],
      blur: image.blur ?? null,
      declined: image.declined ?? [],
      note: image.note ?? null,
    };
  }

  /**
   * Self-host one font and describe it.
   *
   * Resolves to `{ file, mime, bytes, family, fallbackFamily, container,
   * metrics, fallback, fallbackDeclined, css }`.
   *
   * @param {string} id absolute path to the source font
   * @param {object} options
   * @param {string} options.outDir where the copy is written
   * @param {string} [options.family]
   * @param {string} [options.weight]
   * @param {string} [options.style]
   * @param {string} [options.display]
   * @param {string} [options.baseUrl] prefixed to the file name in `src: url()`
   * @param {string | null} [options.fallback] `null` for no fallback face
   */
  async font(id, options) {
    const request = {
      kind: "font",
      id,
      outDir: options.outDir,
      family: options.family,
      weight: options.weight,
      style: options.style,
      display: options.display,
      baseUrl: options.baseUrl,
    };
    // Only sent when the caller had an opinion. The service distinguishes "not
    // mentioned, use the project's" from "explicitly none", and a key that is
    // always present collapses the two.
    if ("fallback" in options) request.fallback = options.fallback;
    const reply = await this.#send(request);
    const font = reply.font;
    if (font == null) throw new AssetError(id, "uf assets returned no font");
    return {
      file: font.file,
      mime: font.mime,
      bytes: font.bytes,
      family: font.family,
      fallbackFamily: font.fallbackFamily ?? null,
      container: font.container,
      metrics: font.metrics,
      fallback: font.fallback ?? null,
      fallbackDeclined: font.fallbackDeclined ?? null,
      css: font.css,
    };
  }

  /**
   * The build of `uf` this service's child is executing, or `null`.
   *
   * @returns {string | null}
   */
  get identity() {
    return this.#identity;
  }

  /** Stop the process. Outstanding requests are rejected. */
  close() {
    this.#child.stdin.end();
    this.#child.kill();
  }
}
