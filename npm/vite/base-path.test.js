// @flow
//
// `app.router.basePath` and `app.router.trailingSlash`, as `@uniflowed/vite`
// reads them.
//
// Three things are this package's own. The settings are normalised once, where
// the config is read, so every host compares one spelling. A prerendered
// document is written outside Vite's HTML transform, so the asset URLs it links
// get the base from here or from nowhere. And Vite serves under its `base` with
// a trailing slash, so the bare base path, which is the application's root, has
// to be handed to Vite's own middleware in the spelling Vite knows.
//
// What every front door does with the settings is `@uniflowed/server`'s
// `internal/routing.js`, and `tests/library/deploy.test.js` compares the doors.

import { describe, expect, it } from "@uniflowed/test";

import { routingRulesOf } from "./internal/routes.js";
import { answersInFrontOfFiles, assetsFromManifest, forViteBase } from "./internal/serve.js";

describe("the settings as the config is read", () => {
  it("are the root and the default policy when the project says nothing", () => {
    const routing = routingRulesOf({});

    expect(routing.basePath).toBe("");
    expect(routing.trailingSlash).toBe("ignore");
  });

  it("forgive a trailing slash on the base and read an unknown policy as the default", () => {
    // `uf_config` refuses both when it reads the file; this is the spelling a
    // driver started by hand on an unchecked config still compares with.
    const routing = routingRulesOf({ basePath: "/docs//", trailingSlash: "sometimes" });

    expect(routing.basePath).toBe("/docs");
    expect(routing.trailingSlash).toBe("ignore");
  });

  it("put the front door's answers in front of the files for either setting alone", () => {
    expect(answersInFrontOfFiles(routingRulesOf({}))).toBe(false);
    expect(answersInFrontOfFiles(routingRulesOf({ basePath: "/docs" }))).toBe(true);
    expect(answersInFrontOfFiles(routingRulesOf({ trailingSlash: "never" }))).toBe(true);
  });
});

describe("the asset URLs a build writes into a document", () => {
  const manifest = {
    "virtual:uf/client": {
      file: "assets/client-abc.js",
      name: "client",
      isEntry: true,
      css: ["assets/client-abc.css"],
      imports: ["_shared-def.js"],
    },
    "_shared-def.js": { file: "assets/shared-def.js", css: ["assets/shared-def.css"] },
  };

  it("are at the root of the host without a base path", () => {
    expect(assetsFromManifest(manifest)).toEqual({
      scripts: ["/assets/client-abc.js"],
      styles: ["/assets/client-abc.css", "/assets/shared-def.css"],
      preloads: ["/assets/shared-def.js"],
    });
  });

  it("carry the base path in front of every one", () => {
    expect(assetsFromManifest(manifest, "/docs")).toEqual({
      scripts: ["/docs/assets/client-abc.js"],
      styles: ["/docs/assets/client-abc.css", "/docs/assets/shared-def.css"],
      preloads: ["/docs/assets/shared-def.js"],
    });
  });
});

describe("a request Vite's own middleware is handed under a base path", () => {
  /** The half of an `IncomingMessage` the rewrite reads and writes. */
  const incoming = (url: string): { url: string } => ({ url });

  it("is spelled with the slash when it is the bare base, the query kept", () => {
    const routing = routingRulesOf({ basePath: "/docs", trailingSlash: "never" });
    const bare = incoming("/docs");
    const queried = incoming("/docs?tab=api");

    forViteBase(routing, bare);
    forViteBase(routing, queried);

    // Vite takes `/docs/` off and hands on the root; `/docs` it would have
    // answered with a 404 of its own.
    expect(bare.url).toBe("/docs/");
    expect(queried.url).toBe("/docs/?tab=api");
  });

  it("is left alone anywhere else", () => {
    const routing = routingRulesOf({ basePath: "/docs" });
    for (const url of ["/docs/", "/docs/guide", "/docsx", "/guide", "/"]) {
      const request = incoming(url);
      forViteBase(routing, request);
      expect(request.url).toBe(url);
    }
  });

  it("is left alone for a project at the root", () => {
    const request = incoming("/docs");

    forViteBase(routingRulesOf({}), request);

    expect(request.url).toBe("/docs");
  });
});
