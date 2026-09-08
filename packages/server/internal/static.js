// @flow
//
// Which file answers a request for a static asset, and with what headers.
//
// The *policy* half of serving a directory, with no answer about the body in
// it. `../node.js` had all of both until a second host wanted the first:
// `Bun.serve` is 17% faster on a 215 KB asset than the same server handed a
// `Readable.toWeb(createReadStream(…))`, and level with `node:http` when it is
// handed one — so the body has to be the host's and the rest must not be.
// See ubugeeei-prod/uf#391 for those numbers.
//
// # Why this is a module and not a copied function
//
// Almost none of what follows is about reading a file. It is a containment
// check, a symlink re-check (#550), the rule that a draft request is never
// answered with a prerendered document (#620), the order in which `/guide`,
// `/guide/` and `guide.html` are tried, and a closed content-type table. Two
// issues have already been spent getting that right. A second host that
// reimplemented it against its own file API is how the third traversal bug
// gets in, and the fastest `Bun.file` in the world does not pay for that.
//
// So a host supplies bytes for a path this module chose, and chooses nothing.

import { realpath, stat } from "node:fs/promises";
import path from "node:path";

import { prerenderedMayAnswer } from "./draft.js";

/**
 * Content types for what a uf build emits.
 *
 * A closed table rather than a dependency, and deliberately short: every entry
 * is an extension `uf build` actually writes or a project actually puts in
 * `public/`. Anything else is `application/octet-stream`, which a browser
 * downloads rather than executes — the safe answer for a file whose type we do
 * not know, and the reason this is not a guess based on the bytes.
 */
const CONTENT_TYPES: { readonly [string]: string } = Object.freeze({
  ".avif": "image/avif",
  ".css": "text/css; charset=utf-8",
  ".gif": "image/gif",
  ".html": "text/html; charset=utf-8",
  ".ico": "image/x-icon",
  ".jpeg": "image/jpeg",
  ".jpg": "image/jpeg",
  ".js": "text/javascript; charset=utf-8",
  ".json": "application/json; charset=utf-8",
  ".map": "application/json; charset=utf-8",
  ".mjs": "text/javascript; charset=utf-8",
  ".png": "image/png",
  ".svg": "image/svg+xml",
  ".txt": "text/plain; charset=utf-8",
  ".webmanifest": "application/manifest+json",
  ".webp": "image/webp",
  ".woff": "font/woff",
  ".woff2": "font/woff2",
  ".xml": "application/xml; charset=utf-8",
});

/**
 * One directory, and the real path it resolves to once anyone has asked.
 *
 * Resolved lazily and kept, because the root is fixed for the life of a
 * handler and a checkout reached through a symlink — which is every macOS
 * `/tmp` — would otherwise fail its own containment test on every request.
 */
export type StaticRoot = {
  readonly root: string,
  realRoot: string | null,
};

/** The file to answer with, and the headers that describe it. */
export type StaticFile = {|
  /** Absolute path. The host opens this and nothing else. */
  readonly path: string,
  /**
   * Ready to hand to a `Response`.
   *
   * Writable rather than `readonly` because `Response`'s `init.headers` will
   * not take a read-only index signature, and a fresh object is built per
   * request anyway — so the immutability would buy nothing and cost a cast at
   * both hosts.
   */
  readonly headers: { [string]: string },
  /**
   * The request was `HEAD`, so the headers are the whole answer.
   *
   * Carried rather than left to the host to re-derive: a host that forgot
   * would send a body to a `HEAD`, and the check belongs beside the one that
   * admitted the method in the first place.
   */
  readonly headOnly: boolean,
|};

/** Begin serving `root`, whose real path is resolved on the first request. */
export function staticRoot(root: string): StaticRoot {
  return { root: path.resolve(root), realRoot: null };
}

/**
 * The file this request should be answered with, or `null` for "not mine".
 *
 * `null` is not 404: it means this handler has nothing, and the caller goes on
 * to the application. A traversal, a method that is not `GET`/`HEAD`, a draft
 * request for a prerendered document and a file that is simply absent all
 * arrive here as the same answer, which is what they should look like from
 * outside.
 *
 * `GET` and `HEAD` only. A `POST` to a path that happens to have a file under
 * it belongs to a route handler, and answering it with the file's bytes would
 * be the same mistake as rendering a page for it.
 *
 * # The path is checked once, after it is resolved
 *
 * `docs/security.md` rule 2: never authorize against a raw request string or a
 * partially decoded path. The pathname is decoded first, then resolved against
 * the root, and *then* checked to be inside it — so `%2e%2e%2f`, a backslash
 * on Windows, and a symlinked directory all reduce to the same question, asked
 * once, of the value that is actually opened.
 */
export async function locateStatic(
  state: StaticRoot,
  request: Request,
): Promise<StaticFile | null> {
  const method = request.method.toUpperCase();
  if (method !== "GET" && method !== "HEAD") return null;

  const pathname = decodePathname(new URL(request.url).pathname);
  if (pathname == null) return null;

  const resolved = path.resolve(state.root, `.${pathname}`);
  if (resolved !== state.root && !resolved.startsWith(state.root + path.sep)) return null;

  state.realRoot ??= (await realpathOrNull(state.root)) ?? state.root;
  const root = state.realRoot;

  // `/guide/` and `/guide` are the same prerendered document, and neither
  // spelling is the one a person types. `<path>.html` is last because a
  // build writes `guide/index.html`, and only a hand-placed file in
  // `public/` is ever `guide.html`.
  const candidates =
    pathname.endsWith("/") === true
      ? [path.join(resolved, "index.html")]
      : [resolved, path.join(resolved, "index.html"), `${resolved}.html`];

  // A draft request is never answered with a prerendered document, and
  // `./draft.js`'s `prerenderedMayAnswer` is where that is argued — once, for
  // all four front doors. What is this door's own is the last sentence of it:
  // which of these bytes are a *document*. Here that is the file's extension,
  // because here the bytes are files.
  const prerendered = prerenderedMayAnswer(request.headers.get("cookie"));

  for (const candidate of candidates) {
    if (!prerendered && candidate.toLowerCase().endsWith(".html")) continue;
    const info = await statFile(candidate);
    if (info == null || !info.isFile()) continue;
    // The containment check above is textual, and a symlink is how a path
    // that reads as inside the root opens a file outside it. `dist/` is
    // written by the build, but `public/` is copied verbatim from whatever
    // the author — or a dependency's install script — put there, so the
    // check has to be made again on the value that is actually opened.
    // ubugeeei-prod/uf#550; the same shape as the tarball traversals in
    // `docs/security.md`, at the serving end.
    //
    // A link that stays inside the root still works, because that is a thing
    // people do on purpose. One that leaves is a 404, indistinguishable from
    // a file that is not there — which is what it should look like.
    const opened = await realpathOrNull(candidate);
    if (opened == null || (opened !== root && !opened.startsWith(root + path.sep))) {
      continue;
    }
    return {
      path: candidate,
      headers: {
        "content-type":
          CONTENT_TYPES[path.extname(candidate).toLowerCase()] ?? "application/octet-stream",
        "content-length": String(info.size),
      },
      headOnly: method === "HEAD",
    };
  }
  return null;
}

function decodePathname(pathname: string): string | null {
  try {
    const decoded = decodeURIComponent(pathname);
    // A NUL truncates the name every C-level `open` sees, so a path holding
    // one is refused rather than normalised into something shorter.
    return decoded.includes("\0") ? null : decoded;
  } catch {
    // A percent escape that is not one. There is no file behind it.
    return null;
  }
}

/**
 * `fs.realpath`, or `null` when the path cannot be resolved.
 *
 * A broken symlink, a component that is not a directory, or a permission the
 * process does not have all end here — and all of them mean the same thing to
 * the caller: this is not a file to serve.
 */
async function realpathOrNull(file: string): Promise<string | null> {
  try {
    return await realpath(file);
  } catch {
    return null;
  }
}

async function statFile(file: string) {
  try {
    return await stat(file);
  } catch {
    return null;
  }
}
