// @flow
//
// `@uniflowed/server/image`: `/__uf/image`, the request-time image endpoint.
//
// `uf assets` resizes an image a module *imported*, at build time, at the
// widths the project declared. This answers for the other kind — a CMS asset,
// a user upload — whose URL is only known when a page renders:
//
//   GET /__uf/image?url=https%3A%2F%2Fimages.example.com%2Fa.jpg&w=640&q=75
//
// It fetches the source, resizes it to `w`, re-encodes it at `q` — as AVIF when
// the request's `Accept` says the browser takes it, otherwise in the source's
// own family — and keeps the answer in a cache keyed on every one of those.
// `Image` from `@uniflowed/web` writes these URLs for a remote `src`.
//
// # It is an SSRF surface, and is built as one
//
// An endpoint that fetches whatever URL a query string names, from inside the
// network the server runs in, is the textbook server-side request forgery. So
// everything a request says is checked before it is used, and the checks are
// the ones that have failed elsewhere:
//
// * **The allow-list is the first thing asked, and nothing else admits a
//   host.** `app.builtins.images.remotePatterns` — see
//   `./internal/remote-patterns.js`. Empty means there is no endpoint at all:
//   the hosts that construct one construct it only for a project that listed
//   something.
// * **Every redirect is a new request, and is asked again.** Redirects are
//   never followed by the platform; each `Location` is resolved against the
//   URL that sent it and has to match the list on its own. Three at most.
// * **The name is resolved and the address is judged**, by the `fetch` the
//   host passes in rather than here, because resolving a name is a thing only
//   a runtime with a resolver can do. `./image-node.js` resolves, refuses a
//   private, loopback or link-local answer (`./internal/addresses.js`), and
//   connects to the address it checked — not to the name again, which is the
//   DNS-rebinding gap between a check and a use.
// * **The body is bounded while it arrives**, not trusted from
//   `Content-Length`: [`MAX_SOURCE_BYTES`], counted as the chunks come in, and
//   the read is cancelled the moment it is passed.
// * **Only PNG, JPEG and WebP**, by the first bytes of the body rather than
//   by a header the origin wrote. An SVG in particular is refused: served from
//   the application's own origin it is a document that can run script.
// * **Only the widths and qualities the project declared.** Every other value
//   is one more encode of every allowed image, which is how an image endpoint
//   becomes a way to pin every core of the machine.
// * **At most [`DEFAULT_CONCURRENCY`] encodes at once**, and a bounded queue
//   behind them, past which a request is a `503` rather than a wait.
// * **Decode dimensions are bounded where the decoding is** — in the
//   transformer, which for `uf start` is `uf_assets::variant` and its
//   `MAX_VARIANT_PIXELS`, read from the header before a pixel is allocated.
//
// # The cache
//
// A `CacheStore` — the same store the route cache is, so a host that named a
// durable `rendering.cache.store` hands this one the same provider and every
// process of a deployment shares its variants. The key is every parameter:
// the source URL after parsing, the width, the quality, and the format that
// was negotiated (not the raw `Accept`, which would give every browser build
// its own copy of the same bytes). The lifetime is the origin's own
// `Cache-Control` `max-age`, held between a minute and a year, and an hour
// when it says nothing; an origin that says `no-store` or `private` is
// answered and not kept.
//
// # What it does not do
//
// Images on the application's own origin. A file in `public/` or an import
// is `uf assets`' work, done once at build time; a relative `url` is refused
// rather than fetched back from this same server.

import { cacheLife, createCacheStore, noStore } from "./cache.js";
import type { CacheOutcome, CacheStore } from "./internal/cache-store.js";
import type { CompiledPattern, RemotePattern } from "./internal/remote-patterns.js";
import { admits, compileRemotePatterns } from "./internal/remote-patterns.js";

export type { RemotePattern } from "./internal/remote-patterns.js";

/** Where the endpoint answers, under the application's base path. */
export const IMAGE_ENDPOINT: string = "/__uf/image";

/**
 * `app.builtins.images.widths` when a project says nothing.
 *
 * `uf_assets::DEFAULT_WIDTHS`, spelled again because this side reads the
 * configuration as JavaScript and never sees the Rust default applied;
 * `tests/library/image-endpoint.test.js` holds the two spellings, and
 * `@uniflowed/web`'s, to each other.
 */
export const DEFAULT_WIDTHS: $ReadOnlyArray<number> = [640, 750, 828, 1080, 1200, 1440, 1920];

/** `app.builtins.images.quality` when a project says nothing. */
export const DEFAULT_QUALITY: number = 75;

/**
 * Largest source the endpoint reads, in bytes.
 *
 * `uf_assets::MAX_VARIANT_SOURCE_BYTES`, the same number on the other side of
 * the transformer, so a body that got this far is never refused there.
 */
export const MAX_SOURCE_BYTES: number = 32 * 1024 * 1024;

/** Most redirects followed to reach a source. */
export const MAX_REDIRECTS: number = 3;

/** How long a source may take to arrive, in milliseconds. */
export const SOURCE_TIMEOUT: number = 15_000;

/** Encodes in flight at once, per endpoint. */
export const DEFAULT_CONCURRENCY: number = 4;

/** Requests waiting for an encode, per endpoint, before one is a `503`. */
export const MAX_QUEUED: number = 64;

/** Variants held in memory, when the host gave no store. */
export const MAX_MEMORY_VARIANTS: number = 256;

/** Lifetime of a variant whose origin stated none, in seconds. */
export const DEFAULT_LIFETIME: number = 60 * 60;

const MIN_LIFETIME = 60;
const MAX_LIFETIME = 365 * 24 * 60 * 60;

/** The source formats the endpoint decodes. */
export type SourceType = "image/png" | "image/jpeg" | "image/webp";

/**
 * One request to a remote host, with redirects *not* followed.
 *
 * What a host passes, because it is the part that differs: `./image-node.js`
 * resolves the name, judges the address and connects to it, and `./image-edge.js`'s
 * is the platform's `fetch` with `redirect: "manual"`. A redirect comes back as
 * the `3xx` it was; the endpoint decides whether to follow it.
 */
export type ImageFetch = (url: URL, signal: AbortSignal) => Promise<Response>;

/** What a transformer is handed. */
export type TransformInput = {|
  readonly bytes: Uint8Array,
  readonly type: SourceType,
  readonly width: number,
  readonly quality: number,
  /** Whether the browser accepts AVIF. */
  readonly avif: boolean,
|};

/** What a transformer answers with. */
export type TransformOutput = {|
  readonly bytes: Uint8Array,
  /** The encoded file's media type. */
  readonly type: string,
|};

/**
 * The encoder: one width of one source.
 *
 * `uf start` and `uf preview` pass `uf` itself (`uf_assets::variant`, over
 * the `uf assets` protocol); `--adapter edge` passes Cloudflare's image
 * binding; a directory built for a runtime with neither is given the module
 * `app.builtins.images.transformer` names. A transformer that answers wider
 * than asked, or in a format the browser did not accept, is a bug in the
 * transformer — the endpoint does not re-check its output.
 */
export type ImageTransform = (input: TransformInput) => Promise<TransformOutput>;

/** Everything a host decides about its endpoint. */
export type ImageEndpointOptions = {|
  /** `app.builtins.images.remotePatterns`. */
  readonly remotePatterns: $ReadOnlyArray<RemotePattern>,
  /** `app.builtins.images.widths`: the only widths a request may name. */
  readonly widths: $ReadOnlyArray<number>,
  /** `app.builtins.images.quality`: the default, and always allowed. */
  readonly quality: number,
  /** `app.builtins.images.qualities`: the other qualities a request may name. */
  readonly qualities?: $ReadOnlyArray<number>,
  readonly fetch: ImageFetch,
  readonly transform: ImageTransform,
  /**
   * Where variants are kept. A memory store of [`MAX_MEMORY_VARIANTS`] when
   * absent; a host with a durable `rendering.cache.store` passes a store over
   * the same provider.
   */
  readonly store?: CacheStore,
  /** Where the endpoint answers. [`IMAGE_ENDPOINT`] when absent. */
  readonly path?: string,
  /** Encodes at once. [`DEFAULT_CONCURRENCY`] when absent. */
  readonly concurrency?: number,
|};

/**
 * A request the endpoint refuses, with the status it is answered with.
 *
 * Thrown by a host's [`ImageFetch`] as well as here: a name that resolves to a
 * private address is a `403` whichever runtime found out.
 */
export class ImageRefusal extends Error {
  status: number;

  constructor(status: number, message: string) {
    super(message);
    this.name = "ImageRefusal";
    this.status = status;
  }
}

/** A source, fetched, bounded and sniffed. */
type Fetched = {|
  readonly bytes: Uint8Array,
  readonly type: SourceType,
  readonly cacheControl: string | null,
|};

/** A variant, as the cache keeps it. */
type Variant = {|
  readonly type: string,
  readonly body: Uint8Array,
  readonly lifetime: number,
|};

/**
 * The endpoint, as a function that answers its own path and declines the rest.
 *
 * `null` for any request that is not for [`IMAGE_ENDPOINT`], so a host can
 * ask it first and carry on; a `Response` for every request that is, refusals
 * included.
 *
 * Throws when it is built from a list it cannot read, rather than answering
 * with an endpoint that admits nothing or more than it should.
 */
export function createImageEndpoint(
  options: ImageEndpointOptions,
): (request: Request) => Promise<Response | null> {
  const patterns = compileRemotePatterns(options.remotePatterns);
  if (patterns.length === 0) {
    throw new TypeError(
      "uf: an image endpoint needs at least one entry in app.builtins.images.remotePatterns; " +
        "with none there is nothing it may fetch, and a host should not construct one",
    );
  }
  const widths = new Set(options.widths);
  const qualities = new Set([options.quality, ...(options.qualities ?? [])]);
  const store = options.store ?? createCacheStore({ maxEntries: MAX_MEMORY_VARIANTS });
  const path = options.path ?? IMAGE_ENDPOINT;
  const limit = gate(options.concurrency ?? DEFAULT_CONCURRENCY);

  return async function image(request: Request): Promise<Response | null> {
    const address = new URL(request.url);
    if (address.pathname !== path) return null;
    const method = request.method.toUpperCase();
    if (method !== "GET" && method !== "HEAD") {
      return refusal(405, "the image endpoint answers GET and HEAD", "GET, HEAD");
    }

    let asked;
    try {
      asked = parse(address.searchParams, widths, qualities, options.quality);
    } catch (error) {
      if (error instanceof ImageRefusal) return refusal(error.status, error.message);
      throw error;
    }
    // Checked here as well as on every hop, so a request for an unlisted host
    // is refused without a cache lookup, a queue slot or a byte of network.
    if (!admits(patterns, asked.source)) {
      return refusal(403, notListed(asked.source));
    }

    const avif = acceptsAvif(request.headers.get("accept"));
    const format = avif ? "avif" : "fallback";
    try {
      const result = await store.resolve(
        {
          key: ["image", asked.source.href, String(asked.width), String(asked.quality), format],
        },
        () =>
          produce({
            patterns,
            fetch: options.fetch,
            transform: options.transform,
            limit,
            source: asked.source,
            width: asked.width,
            quality: asked.quality,
            avif,
          }),
      );
      return variantResponse(result.value, result.outcome, method === "HEAD");
    } catch (error) {
      if (error instanceof ImageRefusal) return refusal(error.status, error.message);
      // Not the message: a transformer's or a network stack's error text is
      // about this server, not about the request. It goes where every other
      // failure on this path goes.
      console.error(error);
      return refusal(502, "the image could not be fetched or processed");
    }
  };
}

/** The parameters, checked, or an [`ImageRefusal`] naming the first bad one. */
function parse(
  params: URLSearchParams,
  widths: Set<number>,
  qualities: Set<number>,
  quality: number,
): {| readonly source: URL, readonly width: number, readonly quality: number |} {
  const one = (name: string): string | null => {
    const values = params.getAll(name);
    // Twice is refused rather than resolved: which of two values a parser
    // takes is exactly the kind of disagreement a cache key and a fetch must
    // not have between them.
    if (values.length > 1) throw new ImageRefusal(400, `\`${name}\` is given more than once`);
    return values.length === 0 ? null : values[0];
  };

  const written = one("url");
  if (written == null || written === "") throw new ImageRefusal(400, "`url` is missing");
  if (written.length > 4096) throw new ImageRefusal(400, "`url` is longer than 4096 characters");
  let source;
  try {
    source = new URL(written);
  } catch {
    throw new ImageRefusal(
      400,
      "`url` is not an absolute URL; an image on this origin is `uf assets`' work, not this endpoint's",
    );
  }
  if (source.protocol !== "https:" && source.protocol !== "http:") {
    throw new ImageRefusal(400, "`url` is not http or https");
  }
  // The fragment never reaches a server, so two URLs that differ only in one
  // are one image and one cache entry.
  source.hash = "";

  const width = integer(one("w"), "w");
  if (width == null) throw new ImageRefusal(400, "`w` is missing");
  if (!widths.has(width)) {
    throw new ImageRefusal(
      400,
      `\`w=${width}\` is not one of app.builtins.images.widths (${[...widths].join(", ")})`,
    );
  }
  const asked = integer(one("q"), "q") ?? quality;
  if (!qualities.has(asked)) {
    throw new ImageRefusal(
      400,
      `\`q=${asked}\` is not app.builtins.images.quality or one of its qualities ` +
        `(${[...qualities].join(", ")})`,
    );
  }
  return { source, width, quality: asked };
}

/** A decimal integer, digits only, or `null` when the parameter is absent. */
function integer(value: string | null, name: string): number | null {
  if (value == null) return null;
  let digits = value !== "" && value.length <= 5;
  for (let index = 0; digits && index < value.length; index += 1) {
    const code = value.charCodeAt(index);
    digits = code >= 0x30 && code <= 0x39;
  }
  if (!digits) throw new ImageRefusal(400, `\`${name}\` is not a whole number`);
  return Number(value);
}

/**
 * Whether an `Accept` header admits `image/avif`.
 *
 * Read as a list of media ranges, so `image/avif;q=0` is the refusal it says it
 * is; a wildcard is *not* taken as a yes, because every browser that sends
 * `*\/*` for an image and means it has shipped AVIF support in the same release
 * that started naming it — and the one that does not is exactly the browser a
 * wrong guess would hand an AVIF to.
 */
export function acceptsAvif(accept: string | null): boolean {
  if (accept == null) return false;
  return accept.split(",").some((range) => {
    const [type, ...parameters] = range.split(";").map((part) => part.trim().toLowerCase());
    if (type !== "image/avif") return false;
    const weight = parameters.find((parameter) => parameter.startsWith("q="));
    return weight == null || Number(weight.slice(2)) > 0;
  });
}

/** Fetch, bound, sniff and encode one variant: the fill behind a miss. */
async function produce(job: {|
  readonly patterns: $ReadOnlyArray<CompiledPattern>,
  readonly fetch: ImageFetch,
  readonly transform: ImageTransform,
  readonly limit: <T>(body: () => Promise<T>) => Promise<T>,
  readonly source: URL,
  readonly width: number,
  readonly quality: number,
  readonly avif: boolean,
|}): Promise<Variant> {
  return job.limit(async () => {
    const fetched = await fetchSource(job.patterns, job.fetch, job.source);
    const lifetime = lifetimeOf(fetched.cacheControl);
    if (lifetime == null) {
      noStore("the origin said no-store or private");
    } else {
      cacheLife({ revalidate: lifetime });
    }
    const encoded = await job.transform({
      bytes: fetched.bytes,
      type: fetched.type,
      width: job.width,
      quality: job.quality,
      avif: job.avif,
    });
    return { type: encoded.type, body: encoded.bytes, lifetime: lifetime ?? 0 };
  });
}

/**
 * The source's bytes, following at most [`MAX_REDIRECTS`] redirects, each of
 * which has to match the allow-list on its own.
 */
async function fetchSource(
  patterns: $ReadOnlyArray<CompiledPattern>,
  fetchOne: ImageFetch,
  start: URL,
): Promise<Fetched> {
  // A controller and a timer rather than `AbortSignal.timeout`, so the timer is
  // cleared the moment the source has arrived instead of holding a process
  // open for the rest of its fifteen seconds.
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), SOURCE_TIMEOUT);
  try {
    return await fetchHops(patterns, fetchOne, start, controller.signal);
  } finally {
    clearTimeout(timer);
  }
}

/** The loop [`fetchSource`] runs under its timer. */
async function fetchHops(
  patterns: $ReadOnlyArray<CompiledPattern>,
  fetchOne: ImageFetch,
  start: URL,
  signal: AbortSignal,
): Promise<Fetched> {
  let current = start;
  for (let hop = 0; ; hop += 1) {
    if (!admits(patterns, current)) {
      throw new ImageRefusal(
        403,
        hop === 0
          ? notListed(current)
          : `${start.href} redirected to ${current.href}, which is not in app.builtins.images.remotePatterns`,
      );
    }
    let response;
    try {
      response = await fetchOne(current, signal);
    } catch (error) {
      if (error instanceof ImageRefusal) throw error;
      if (signal.aborted) {
        throw new ImageRefusal(504, `${current.href} did not answer in ${SOURCE_TIMEOUT}ms`);
      }
      throw new ImageRefusal(502, `${current.href} could not be fetched`);
    }

    if (response.status >= 300 && response.status < 400 && response.status !== 304) {
      await response.body?.cancel();
      const location = response.headers.get("location");
      if (location == null) {
        throw new ImageRefusal(502, `${current.href} redirected without a Location`);
      }
      if (hop >= MAX_REDIRECTS) {
        throw new ImageRefusal(502, `${start.href} redirected more than ${MAX_REDIRECTS} times`);
      }
      let next;
      try {
        next = new URL(location, current);
      } catch {
        throw new ImageRefusal(502, `${current.href} redirected to an unreadable Location`);
      }
      next.hash = "";
      current = next;
      continue;
    }
    if (response.status !== 200) {
      await response.body?.cancel();
      throw new ImageRefusal(502, `${current.href} answered ${response.status}`);
    }

    const declared = Number(response.headers.get("content-length") ?? "");
    if (Number.isFinite(declared) && declared > MAX_SOURCE_BYTES) {
      await response.body?.cancel();
      throw tooLarge(current);
    }
    const bytes = await readBounded(response, current);
    const type = sniff(bytes);
    if (type == null) {
      throw new ImageRefusal(
        415,
        `${current.href} is not a PNG, JPEG or WebP image (an SVG is refused on purpose)`,
      );
    }
    return { bytes, type, cacheControl: response.headers.get("cache-control") };
  }
  // Unreachable: the loop returns or throws. Flow cannot see that.
  throw new ImageRefusal(502, "unreachable");
}

/** Every byte of a body, cancelled at the first chunk past the bound. */
async function readBounded(response: Response, url: URL): Promise<Uint8Array> {
  const body = response.body;
  if (body == null) return new Uint8Array(0);
  const reader = body.getReader();
  const chunks: Array<Uint8Array> = [];
  let total = 0;
  for (;;) {
    const step = await reader.read();
    if (step.done === true) break;
    const chunk = step.value;
    if (chunk == null) continue;
    total += chunk.byteLength;
    if (total > MAX_SOURCE_BYTES) {
      await reader.cancel();
      throw tooLarge(url);
    }
    chunks.push(chunk);
  }
  const bytes = new Uint8Array(total);
  let at = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, at);
    at += chunk.byteLength;
  }
  return bytes;
}

function tooLarge(url: URL): ImageRefusal {
  return new ImageRefusal(413, `${url.href} is larger than ${MAX_SOURCE_BYTES} bytes`);
}

function notListed(url: URL): string {
  return `${url.href} is not in app.builtins.images.remotePatterns`;
}

/** The format the first bytes say, or `null` for anything else. */
export function sniff(bytes: Uint8Array): SourceType | null {
  const starts = (signature: $ReadOnlyArray<number>, at: number = 0): boolean =>
    bytes.length >= at + signature.length &&
    signature.every((byte, index) => bytes[at + index] === byte);
  if (starts([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a])) return "image/png";
  if (starts([0xff, 0xd8, 0xff])) return "image/jpeg";
  // RIFF....WEBP
  if (starts([0x52, 0x49, 0x46, 0x46]) && starts([0x57, 0x45, 0x42, 0x50], 8)) {
    return "image/webp";
  }
  return null;
}

/**
 * How long a variant may be kept, from the origin's `Cache-Control`, or `null`
 * for an origin that said not to keep it.
 */
export function lifetimeOf(cacheControl: string | null): number | null {
  if (cacheControl == null) return DEFAULT_LIFETIME;
  let maxAge = null;
  for (const directive of cacheControl.toLowerCase().split(",")) {
    const [name, value] = directive.trim().split("=");
    if (name === "no-store" || name === "private") return null;
    if (name === "s-maxage" || (name === "max-age" && maxAge == null)) {
      const seconds = Number(value);
      if (Number.isInteger(seconds) && seconds >= 0) maxAge = seconds;
    }
  }
  if (maxAge == null) return DEFAULT_LIFETIME;
  return Math.min(Math.max(maxAge, MIN_LIFETIME), MAX_LIFETIME);
}

/**
 * At most `concurrency` bodies running at once, and at most [`MAX_QUEUED`]
 * waiting; past that, a `503`.
 */
function gate(concurrency: number): <T>(body: () => Promise<T>) => Promise<T> {
  let running = 0;
  const waiting: Array<() => void> = [];
  return async function limit<T>(body: () => Promise<T>): Promise<T> {
    if (running >= concurrency) {
      if (waiting.length >= MAX_QUEUED) {
        throw new ImageRefusal(503, "the image endpoint is busy; try again shortly");
      }
      await new Promise((resolve) => {
        waiting.push(resolve);
      });
    }
    running += 1;
    try {
      return await body();
    } finally {
      running -= 1;
      waiting.shift()?.();
    }
  };
}

function variantResponse(variant: Variant, outcome: CacheOutcome, head: boolean): Response {
  const headers = new Headers({
    "content-type": variant.type,
    "content-length": String(variant.body.byteLength),
    // The response depends on `Accept`, and a shared cache in front of this
    // server that ignored it would hand an AVIF to a browser that cannot draw
    // one.
    vary: "Accept",
    "cache-control":
      variant.lifetime > 0 ? `public, max-age=${variant.lifetime}` : "private, no-store",
    "x-content-type-options": "nosniff",
    // A raster image has nothing to run, and this says so to a browser that
    // is navigated to the URL directly rather than given it in an `<img>`.
    "content-security-policy": "default-src 'none'; sandbox",
    "x-uf-cache": label(outcome),
  });
  return new Response(head ? null : variant.body, { status: 200, headers });
}

function refusal(status: number, message: string, allow?: string): Response {
  const headers = new Headers({
    "content-type": "text/plain; charset=utf-8",
    "cache-control": "no-store",
    "x-content-type-options": "nosniff",
  });
  if (allow != null) headers.set("allow", allow);
  return new Response(`uf: ${message}\n`, { status, headers });
}

/** The header word for an outcome, the route cache's words. */
function label(outcome: CacheOutcome): string {
  return match (outcome) {
    "hit" => "HIT",
    "stale" => "STALE",
    "coalesced" => "COALESCED",
    "miss" => "MISS",
    "uncacheable" => "BYPASS",
  };
}
