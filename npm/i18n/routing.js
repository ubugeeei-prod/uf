// @flow

import { negotiate, parseAcceptLanguage } from "./negotiate.js";

/** The same locale union is used by middleware, pages and generated parameters. */
export type LocaleRouting<L extends string> = {|
  readonly locales: $ReadOnlyArray<L>,
  readonly locale: (params: { readonly [string]: mixed }) => L,
  readonly staticParams: () => $ReadOnlyArray<{| locale: L |}>,
  readonly middleware: (request: Request) => Response | null,
  readonly metadata: (path?: string) => {|
    alternates: {| languages: { [string]: string } |},
  |},
|};

/**
 * A `[locale]` root segment. The router already knows how to enumerate dynamic
 * routes and write their sitemap entries; this supplies the shared locale list.
 * Negotiated redirects are private because a cookie can change their answer.
 */
export function createLocaleRouting<L extends string>(options: {|
  readonly locales: $ReadOnlyArray<L>,
  readonly defaultLocale: L,
  readonly cookie?: string,
|}): LocaleRouting<L> {
  const locales = [...options.locales];
  if (
    locales.length === 0 ||
    new Set(locales.map((locale) => locale.toLowerCase())).size !== locales.length ||
    !locales.includes(options.defaultLocale) ||
    locales.some((locale) => !/^[A-Za-z]{2,8}(?:-[A-Za-z0-9]{1,8})*$/.test(locale))
  ) {
    throw new TypeError(
      "locale routing needs distinct language tags and a defaultLocale in locales",
    );
  }
  const cookie = options.cookie ?? "uf.locale";
  if (!/^[A-Za-z0-9_.-]+$/.test(cookie)) {
    throw new TypeError("locale routing cookie must be a cookie name");
  }
  return {
    locales: Object.freeze(locales),
    locale(params) {
      const value = params.locale;
      const found = locales.find((locale) => locale === value);
      if (found == null) {
        throw new RangeError(`unsupported route locale: ${String(value)}`);
      }
      return found;
    },
    staticParams() {
      return locales.map((locale) => ({ locale }));
    },
    middleware(request) {
      const url = new URL(request.url);
      if (url.pathname !== "/" || !["GET", "HEAD"].includes(request.method)) return null;
      let preferred = null;
      for (const entry of (request.headers.get("cookie") ?? "").split(";")) {
        const at = entry.indexOf("=");
        if (at < 0 || entry.slice(0, at).trim() !== cookie) continue;
        try {
          preferred = decodeURIComponent(entry.slice(at + 1).trim());
        } catch {
          preferred = null;
        }
        break;
      }
      const accepted = locales.find((locale) => locale === preferred);
      const selected =
        accepted ??
        negotiate(
          parseAcceptLanguage(request.headers.get("accept-language") ?? ""),
          locales,
          options.defaultLocale,
        );
      // Only a configured language tag enters Location; never a cookie's bytes.
      url.pathname = `/${selected}`;
      return new Response(null, {
        status: 307,
        headers: {
          location: url.href,
          vary: "Accept-Language, Cookie",
          "cache-control": "private, no-store",
        },
      });
    },
    metadata(path = "") {
      if (path !== "" && (!path.startsWith("/") || path.startsWith("//") || /[?#\\]/.test(path))) {
        throw new TypeError(
          "locale metadata expects an application path without query or fragment",
        );
      }
      const languages: { [string]: string } = {};
      for (const locale of locales) languages[locale] = `/${locale}${path}`;
      languages["x-default"] = `/${options.defaultLocale}${path}`;
      return { alternates: { languages } };
    },
  };
}
