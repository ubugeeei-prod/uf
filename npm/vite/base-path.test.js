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

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { describe, expect, it } from "@uniflowed/test";

import { routingRulesOf } from "./internal/routes.js";
import {
  answersInFrontOfFiles,
  assetsFromManifest,
  createStaticBuildHandler,
  forViteBase,
} from "./internal/serve.js";

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
      file   : "assets/client-abc.js",
      name   : "client",
      isEntry: true,
      css    : ["assets/client-abc.css"],
      imports: ["_shared-def.js"],
    },
    "_shared-def.js": { file: "assets/shared-def.js", css: ["assets/shared-def.css"] },
  };

  it("are at the root of the host without a base path", () => {
    expect(assetsFromManifest(manifest)).toEqual({
      scripts : ["/assets/client-abc.js"],
      styles  : ["/assets/client-abc.css", "/assets/shared-def.css"],
      preloads: ["/assets/shared-def.js"],
    });
  });

  it("carry the base path in front of every one", () => {
    expect(assetsFromManifest(manifest, "/docs")).toEqual({
      scripts : ["/docs/assets/client-abc.js"],
      styles  : ["/docs/assets/client-abc.css", "/docs/assets/shared-def.css"],
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

describe("a static build under `uf preview`", () => {
  // ubugeeei-prod/uf#1678. A `build.staticBuild` has no server bundle, so
  // every page was left to Vite's file server, which answered each one the
  // build prerendered with a 404 and knew nothing of the base path.
  const files = {
    "index.html"      : "<p>home</p>",
    "guide/index.html": "<p>guide</p>",
    "404.html"        : "<p>missing</p>",
    "assets/client.js": "export {};",
  };

  function outDir(): string {
    const root = fs.realpathSync(fs.mkdtempSync(path.join(os.tmpdir(), "uf-static-preview-")));
    for (const [name, text] of Object.entries(files)) {
      fs.mkdirSync(path.dirname(path.join(root, name)), { recursive: true });
      fs.writeFileSync(path.join(root, name), String(text));
    }
    return root;
  }

  /** A `ServerResponse` that keeps what it was sent. */
  function recorder(): $FlowFixMe {
    const response = {
      statusCode   : 200,
      statusMessage: "",
      headersSent  : false,
      headers      : {} as { [string]: mixed },
      body         : "",
      setHeader(name: string, value: mixed) {
        response.headers[name.toLowerCase()] = value;
      },
      write(chunk: Uint8Array | string): boolean {
        response.body += typeof chunk === "string" ? chunk : new TextDecoder().decode(chunk);
        return true;
      },
      end(chunk?: Uint8Array | string) {
        if (chunk != null) response.write(chunk);
      },
      destroy() {},
      on() {},
      once() {},
      off() {},
    };
    return response;
  }

  async function ask(
    basePath: string,
    url     : string,
    accept  : string = "text/html",
  ): Promise<{ answered: boolean, status: number, body: string, location: mixed }> {
    const root = outDir();
    try {
      const answer = createStaticBuildHandler({ root, routing: routingRulesOf({ basePath }) });
      const response = recorder();
      const answered = await answer(
        new Request(`http://localhost${url}`, { headers: { accept } }),
        response,
      );
      return {
        answered,
        status  : response.statusCode,
        body    : response.body,
        location: response.headers.location,
      };
    } finally {
      fs.rmSync(root, { recursive: true, force: true });
    }
  }

  it("answers every page it prerendered, by the path a person types", async () => {
    expect(await ask("", "/")).toEqual({
      answered: true,
      status  : 200,
      body    : "<p>home</p>",
      location: undefined,
    });
    expect((await ask("", "/guide/")).body).toBe("<p>guide</p>");
    expect((await ask("", "/guide")).body).toBe("<p>guide</p>");
  });

  it("serves the pages under the base path, with the base taken off", async () => {
    expect((await ask("/docs", "/docs/")).body).toBe("<p>home</p>");
    expect((await ask("/docs", "/docs/guide/")).body).toBe("<p>guide</p>");
    expect((await ask("/docs", "/docs/assets/client.js", "*/*")).body).toBe("export {};");
    // Outside the base is not the application's, as at every other door.
    expect((await ask("/docs", "/guide/")).status).toBe(404);
  });

  it("answers a document nothing matched with 404.html and a 404, as a static host does", async () => {
    const missing = await ask("", "/nowhere/");
    expect(missing.status).toBe(404);
    expect(missing.body).toBe("<p>missing</p>");
    // Anything but a document is left to go on, and Vite answers it.
    expect((await ask("", "/assets/gone.js", "*/*")).answered).toBe(false);
  });
});
