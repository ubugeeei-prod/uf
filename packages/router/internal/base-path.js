// @flow
//
// Internal to `@uniflowed/router`: `app.router.basePath` and
// `app.router.trailingSlash`, as the router reads them.
//
// The route table has no base in it — `/guide` is `/guide` whether the site is
// served at `/` or at `/docs` — and the browser's address bar has one. This
// module is the one place those two spellings meet: a `Link` and a navigation
// turn an application path into the address, and hydration, `popstate` and a
// refresh turn the address back into an application path before the table is
// asked. Every other part of the router speaks application paths.
//
// The policy decides the spelling of a path a link writes, so that a page is
// linked by the address its server answers without a redirect. The server's
// half — the `308` for the other spelling, and the `404` outside the base — is
// `@uniflowed/server`'s `internal/routing.js`.
//
// # Installed, like navigation
//
// Module state beside `installNavigation`, set once by the entry that started
// the application: `virtual:uf/client` in the browser and `virtual:uf/server`
// in the graph that renders documents. `@uniflowed/vite` writes both calls from
// `uf.config.js`, so a dev server and a build link the same way. A test that
// installs nothing gets the root and `"ignore"`, which is what every
// application had before the setting existed.

/** Which spelling of a path is the page; see `app.router.trailingSlash`. */
export type TrailingSlash = "never" | "always" | "ignore";

/** What an entry installs. */
export type RoutingSettings = {|
  readonly basePath?: string,
  readonly trailingSlash?: TrailingSlash,
|};

let installedBase: string = "";
let installedSlash: TrailingSlash = "ignore";

/**
 * Say where this application is served and how its paths are spelled. Called
 * once, by the entry that starts it.
 */
export function installRouting(settings: RoutingSettings): void {
  installedBase = normalizeBase(settings.basePath ?? "");
  installedSlash = settings.trailingSlash ?? "ignore";
}

/**
 * The path this application is served under: `""` at the root, `"/docs"`
 * otherwise.
 *
 * For code that builds an absolute address the router did not build for it —
 * a middleware's `Response.redirect(new URL(`${basePath()}/sign-in`, request.url))`
 * — because a route handler and a middleware are handed the application path,
 * and an address built from `/sign-in` alone leaves the base behind.
 */
export function basePath(): string {
  return installedBase;
}

/** How this application spells a path. */
export function trailingSlash(): TrailingSlash {
  return installedSlash;
}

/**
 * The address for an application path: the base in front, the policy's
 * spelling, the query and the fragment kept.
 *
 * Only a path that starts with a single `/` is an application path. Anything
 * else — `https://…`, `//cdn…`, `mailto:`, `?page=2`, `#top`, `../up` — is
 * returned as it was written, because the browser resolves it against the
 * address that is already there.
 */
export function addressOf(to: string): string {
  if (!to.startsWith("/") || to.startsWith("//")) {
    return to;
  }
  const { path, rest } = splitPath(to);
  return `${installedBase}${spellPath(path, installedSlash, installedBase !== "")}${rest}`;
}

/**
 * The application path for an address's pathname, or `null` when the address
 * is outside the base.
 *
 * `/docs/guide` is `/guide` under `/docs`, and `/docs` and `/docs/` are both
 * `/`. `/docsx` is not under `/docs`: a base is whole segments.
 */
export function applicationPathOf(pathname: string): string | null {
  if (installedBase === "") {
    return pathname;
  }
  if (pathname === installedBase) {
    return "/";
  }
  if (pathname.startsWith(`${installedBase}/`)) {
    return pathname.slice(installedBase.length);
  }
  return null;
}

/**
 * `path` in `policy`'s spelling.
 *
 * The root of an application at the root is `/` whatever the policy says. The
 * root of an application under a base is the base itself — `/docs` — unless
 * the policy is `"always"`, which spells it `/docs/`. A path whose last segment
 * looks like a file keeps what it was written with, because `/robots.txt/` is
 * not a page.
 */
export function spellPath(path: string, policy: TrailingSlash, underBase: boolean): string {
  let end = path.length;
  while (end > 0 && path.charCodeAt(end - 1) === 47) {
    end -= 1;
  }
  const trimmed = path.slice(0, end);
  if (trimmed === "") {
    return policy === "always" || !underBase ? "/" : "";
  }
  if (policy === "ignore" || looksLikeAFile(trimmed)) {
    return path;
  }
  return policy === "always" ? `${trimmed}/` : trimmed;
}

/** Whether a path's last segment has an extension. */
function looksLikeAFile(path: string): boolean {
  const last = path.slice(path.lastIndexOf("/") + 1);
  return last.includes(".");
}

function splitPath(to: string): {| readonly path: string, readonly rest: string |} {
  let end = to.length;
  for (let index = 0; index < to.length; index += 1) {
    const code = to.charCodeAt(index);
    // `?` and `#`.
    if (code === 63 || code === 35) {
      end = index;
      break;
    }
  }
  return { path: to.slice(0, end), rest: to.slice(end) };
}

/**
 * A base as `uf_config` accepts one, with a trailing slash forgiven: `""`,
 * `"/"` and absent are the root.
 */
function normalizeBase(base: string): string {
  let end = base.length;
  while (end > 0 && base.charCodeAt(end - 1) === 47) {
    end -= 1;
  }
  return base.slice(0, end);
}
