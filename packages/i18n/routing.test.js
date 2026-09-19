// @flow

import { describe, expect, it } from "@uniflowed/test";
import { createLocaleRouting } from "./routing.js";

const routing = createLocaleRouting({ locales: ["en", "ja"], defaultLocale: "en" });

describe("locale routes", () => {
  it("negotiates the root, preserves the query, and varies by language and cookie", () => {
    const response = routing.middleware(
      new Request("https://example.com/?from=home", {
        headers: { "accept-language": "ja-JP, en;q=0.5" },
      }),
    );
    expect(response?.status).toBe(307);
    expect(response?.headers.get("location")).toBe("https://example.com/ja?from=home");
    expect(response?.headers.get("vary")).toBe("Accept-Language, Cookie");
    expect(response?.headers.get("cache-control")).toBe("private, no-store");
  });

  it("honours a supported cookie, ignores malformed and untrusted values, and avoids loops", () => {
    for (const value of ["../admin", "%E0%A4%A", "//evil.example", "unknown"]) {
      const response = routing.middleware(
        new Request("https://example.com/", {
          headers: { cookie: `uf.locale=${value}`, "accept-language": "ja" },
        }),
      );
      expect(response?.headers.get("location")).toBe("https://example.com/ja");
    }
    expect(
      routing
        .middleware(
          new Request("https://example.com/", {
            headers: { cookie: "session=1; uf.locale=en", "accept-language": "ja" },
          }),
        )
        ?.headers.get("location"),
    ).toBe("https://example.com/en");
    expect(routing.middleware(new Request("https://example.com/ja"))).toBe(null);
    expect(routing.middleware(new Request("https://example.com/", { method: "POST" }))).toBe(null);
  });

  it("enumerates every locale and gives every translation reciprocal alternates", () => {
    expect(routing.staticParams()).toEqual([{ locale: "en" }, { locale: "ja" }]);
    expect(routing.locale({ locale: "ja" })).toBe("ja");
    expect(() => routing.locale({ locale: "fr" })).toThrow("unsupported route locale");
    expect(routing.metadata("/guide")).toEqual({
      alternates: {
        languages: {
          en: "/en/guide",
          ja: "/ja/guide",
          "x-default": "/en/guide",
        },
      },
    });
    expect(() => routing.metadata("//evil.example")).toThrow("application path");
  });
});
