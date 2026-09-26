// @flow
//
// The bundler's half of React Server Components: what `@uniflowed/vite` does
// to a module in the rsc graph, and what it resolves there.
//
// Each case is a hook or a generator called directly, without a server, because
// each is a decision a line of `internal/flight.js` or `index.js` makes and a
// build is a slow way to find out which line. What a real build and a real
// server do with them is `crates/uf_cli/tests/vite.rs`. See ubugeeei-prod/uf#519.

import fs from "node:fs";

import { describe, expect, it } from "@uniflowed/test";
import { parseAst } from "vite";

import uniflowed from "./index.js";
import {
  FLIGHT_BROWSER_DEPENDENCIES,
  RSC_ENVIRONMENT,
  builtReferencesSource,
  clientExportNames,
  clientModuleUrlPlugin,
  clientReferencePlugin,
  compilerRuntimeSource,
  createFlightState,
  devStylesheets,
  opensWithUseClient,
  rendersFlight,
} from "./internal/flight.js";

/** The directive, built rather than written: see `USE_CLIENT` in `internal/flight.js`. */
const DIRECTIVE = ["use", "client"].join(" ");

/** `uf:flow`, as `uniflowed()` returns it for a project that renders RSC. */
function flowPlugin(): $FlowFixMe {
  return uniflowed({ root: "/project", config: {} }).find((plugin) => plugin.name === "uf:flow");
}

/** A plugin context in one environment, whose `resolve` says what it was asked. */
function contextIn(environment: string): $FlowFixMe {
  return {
    environment: { name: environment },
    resolve: async (id: string) => ({ id: `resolved:${id}` }),
  };
}

describe("resolving `@uniflowed/react`", () => {
  it("is React itself in the rsc graph, whatever Vite says about ssr", async () => {
    // Vite passes `ssr: true` to every server environment. Reading that alone
    // left `@uniflowed/react` — a star re-export of CommonJS React — in the rsc
    // graph, where the module runner forwards none of its names, and every
    // hook a server component imported from it was `undefined` under `uf dev`.
    const resolved = await flowPlugin().resolveId.call(
      contextIn(RSC_ENVIRONMENT),
      "@uniflowed/react",
      "/project/app/$page.js",
      { ssr: true },
    );
    expect(resolved).toEqual({ id: "resolved:react" });
  });

  it("is still the package in the ssr graph, where Node forwards its names", async () => {
    const resolved = await flowPlugin().resolveId.call(
      contextIn("ssr"),
      "@uniflowed/react",
      "/project/app/$page.js",
      { ssr: true },
    );
    expect(resolved).toBe(null);
  });

  it("is a runtime with no client internals for the React Compiler in the rsc graph", async () => {
    const resolved = await flowPlugin().resolveId.call(
      contextIn(RSC_ENVIRONMENT),
      "react/compiler-runtime",
      "/project/app/$page.js",
      { ssr: true },
    );
    expect(resolved).toBe("\0virtual:uf/rsc-compiler-runtime");
  });
});

describe("the React Compiler's cache in the rsc graph", () => {
  it("is every slot the sentinel compiled code tests for, fresh each call", async () => {
    const url = `data:text/javascript,${encodeURIComponent(compilerRuntimeSource())}`;
    // The generated runtime, as a `data:` URL; Flow types only a literal specifier.
    // $FlowFixMe[unsupported-syntax]
    const runtime = await import(url);
    const first = runtime.c(3);
    expect(first).toEqual([
      Symbol.for("react.memo_cache_sentinel"),
      Symbol.for("react.memo_cache_sentinel"),
      Symbol.for("react.memo_cache_sentinel"),
    ]);
    expect(runtime.c(3)).not.toBe(first);
  });
});

describe("a client module in the rsc graph", () => {
  it("is recognised by its directive prologue, and not by a string anywhere else", () => {
    expect(opensWithUseClient(parseAst(`"${DIRECTIVE}";\nexport default 1;`))).toBe(true);
    expect(
      opensWithUseClient(parseAst(`"use strict";\n"${DIRECTIVE}";\nexport const a = 1;`)),
    ).toBe(true);
    expect(opensWithUseClient(parseAst(`const note = "${DIRECTIVE}";\nexport const a = 1;`))).toBe(
      false,
    );
  });

  it("names every export a reference is made for", () => {
    const program = parseAst(
      [
        `"${DIRECTIVE}";`,
        "export default function Counter() {}",
        "export const a = 1, { b, c: [d] } = {};",
        "export function Button() {}",
        "const e = 1;",
        'export { e as "kebab-name" };',
      ].join("\n"),
    );
    expect(clientExportNames(program, "app/counter.js")).toEqual([
      "default",
      "a",
      "b",
      "d",
      "Button",
      "kebab-name",
    ]);
  });

  it("refuses a star re-export, naming the file and the fix", () => {
    const program = parseAst(`"${DIRECTIVE}";\nexport * from "./parts.js";`);
    expect(() => clientExportNames(program, "app/counter.js")).toThrow(
      /app\/counter\.js is a client module and re-exports everything from "\.\/parts\.js"/,
    );
  });

  it("becomes one reference per export, at the URL uf dev serves it from", () => {
    const state = createFlightState({ root: "/project" });
    const plugin = clientReferencePlugin(state);
    const out = plugin.transform(
      `"${DIRECTIVE}";\nexport default function Counter() {}\nexport const label = "x";\n`,
      "/project/app/counter.js",
    );
    expect(out.code).toContain('const url = "/app/counter.js";');
    expect(out.code).toContain('createClientReference(url, "default", [url])');
    expect(out.code).toContain('createClientReference(url, "label", [url])');
    expect(out.code).not.toContain("Counter()");
    expect([...state.clientModules]).toEqual(["/project/app/counter.js"]);
  });

  it("names the client manifest in a build, where the chunk URL does not exist yet", () => {
    const state = createFlightState({ root: "/project" });
    state.production = true;
    const out = clientReferencePlugin(state).transform(
      `"${DIRECTIVE}";\nexport default function Counter() {}\n`,
      "/project/app/counter.js",
    );
    expect(out.code).toContain('import { clientUrl } from "virtual:uf/client-manifest";');
    expect(out.code).toContain('const url = clientUrl("/project/app/counter.js");');
  });

  it("forgets a module whose directive was removed, so its next edit reloads the page", () => {
    const state = createFlightState({ root: "/project" });
    const plugin = clientReferencePlugin(state);
    plugin.transform(
      `"${DIRECTIVE}";\nexport default function Counter() {}\n`,
      "/project/app/counter.js",
    );
    expect([...state.clientModules]).toEqual(["/project/app/counter.js"]);

    plugin.transform("export default function Counter() {}\n", "/project/app/counter.js");

    expect(state.clientModules.size).toBe(0);
  });

  it("leaves a module without the directive exactly as it was", () => {
    const state = createFlightState({ root: "/project" });
    const out = clientReferencePlugin(state).transform(
      "export default function Page() {}\n",
      "/project/app/$page.js",
    );
    expect(out).toBe(null);
    expect(state.clientModules.size).toBe(0);
  });
});

describe("client references in the server bundle", () => {
  it("does not dynamically import router internals the server imports already", () => {
    const installed = "/project/node_modules/@uniflowed/router/internal/error-view.js";
    const workspace = "/repo/packages/router/internal/boundaries.js";
    const app = "/project/app/counter.js";
    const source = builtReferencesSource(
      new Map([
        [installed, "/assets/error-view.js"],
        [workspace, "/assets/boundaries.js"],
        [app, "/assets/counter.js"],
      ]),
    );

    expect(source).toContain(`import * as staticReference0 from ${JSON.stringify(installed)};`);
    expect(source).toContain(`import * as staticReference1 from ${JSON.stringify(workspace)};`);
    expect(source).toContain(
      `[${JSON.stringify("/assets/error-view.js")}, () => Promise.resolve(staticReference0)]`,
    );
    expect(source).toContain(
      `[${JSON.stringify("/assets/boundaries.js")}, () => Promise.resolve(staticReference1)]`,
    );
    expect(source).not.toContain(`import(${JSON.stringify(installed)})`);
    expect(source).not.toContain(`import(${JSON.stringify(workspace)})`);
    expect(source).toContain(
      `[${JSON.stringify("/assets/counter.js")}, () => import(${JSON.stringify(app)})]`,
    );
  });
});

describe("a file of an installed uf package, in the browser under `uf dev`", () => {
  /** A plugin context whose `resolve` answers `id`, as Vite's resolver would, and records each question. */
  function resolvingTo(id: string): $FlowFixMe {
    const asked: Array<string> = [];
    return {
      asked,
      resolve: async (specifier: string) => {
        asked.push(specifier);
        return { id };
      },
    };
  }

  it("is its path, the URL a client reference names, and never Vite's versioned one", async () => {
    // ubugeeei-prod/uf#1118. The reference loaded `dialog.js` and the client
    // component's import loaded `dialog.js?v=…`, so `Dialog.Trigger` found no
    // `Dialog.Root` in a context that was another module's.
    const versioned = "/project/node_modules/@uniflowed/ui/dialog.js?v=1a2b3c4d";
    for (const [specifier, importer] of [
      ["@uniflowed/ui", "/project/app/opener.js"],
      ["./dialog.js", "/project/node_modules/@uniflowed/ui/sheet.js"],
      ["/project/node_modules/@uniflowed/ui/dialog.js", "/project/app/opener.js"],
      ["/node_modules/@uniflowed/ui/dialog.js", undefined],
    ]) {
      const out = await clientModuleUrlPlugin().resolveId.call(
        resolvingTo(versioned),
        specifier,
        importer,
        {},
      );
      expect(out.id).toBe("/project/node_modules/@uniflowed/ui/dialog.js");
    }
  });

  it("keeps every other query, such as a raw import's", async () => {
    const out = await clientModuleUrlPlugin().resolveId.call(
      resolvingTo("/project/node_modules/@uniflowed/ui/dialog.js?raw&v=1a2b3c4d"),
      "/project/node_modules/@uniflowed/ui/dialog.js?raw",
      "/project/app/opener.js",
      {},
    );
    expect(out.id).toBe("/project/node_modules/@uniflowed/ui/dialog.js?raw");
  });

  it("leaves every other import to Vite, without resolving it a second time", async () => {
    const context = resolvingTo("/project/node_modules/some-library/index.js?v=1a2b3c4d");
    const plugin = clientModuleUrlPlugin();

    expect(await plugin.resolveId.call(context, "some-library", "/project/app/page.js", {})).toBe(
      null,
    );
    expect(await plugin.resolveId.call(context, "./button.js", "/project/app/page.js", {})).toBe(
      null,
    );
    expect(context.asked).toEqual([]);
  });

  it("is the browser's graph under `uf dev` alone, and every application rendering RSC has it", () => {
    const plugin = clientModuleUrlPlugin();
    expect(plugin.apply).toBe("serve");
    expect(plugin.applyToEnvironment({ name: "client" })).toBe(true);
    expect(plugin.applyToEnvironment({ name: "ssr" })).toBe(false);
    expect(plugin.applyToEnvironment({ name: RSC_ENVIRONMENT })).toBe(false);

    const names = uniflowed({ root: "/project", config: {} }).map((each) => each.name);
    expect(names).toContain("uf:rsc-client-urls");
  });
});

describe("the stylesheets a development document links from the rsc graph", () => {
  /** A dev server whose rsc graph holds `modules`, as `[id, { file, url }]`. */
  function serverWith(
    modules: Array<[string, { readonly file: ?string, readonly url: string }]>,
  ): $FlowFixMe {
    return {
      config: { root: "/repo/docs", base: "/" },
      environments: { [RSC_ENVIRONMENT]: { moduleGraph: { idToModuleMap: new Map(modules) } } },
    };
  }

  it("links each at a URL the dev server serves, wherever the file is", () => {
    const urls = devStylesheets(
      serverWith([
        [
          "/repo/docs/app/_design/seam.css",
          { file: "/repo/docs/app/_design/seam.css", url: "/app/_design/seam.css" },
        ],
        // A workspace package, outside the project: Vite serves it under
        // `/@fs/`, and the graph's own `url` for it is not that.
        [
          "/repo/packages/brand/tokens.css",
          { file: "/repo/packages/brand/tokens.css", url: "/brand/tokens.css" },
        ],
        // Vite can put a virtual StyleX identifier in `file`; it is not a
        // filesystem path and must still be fetched through `/@id/`.
        [
          "uf-style:/repo/docs/app/$page.js.css",
          {
            file: "uf-style:/repo/docs/app/$page.js.css",
            url: "/@id/uf-style:/repo/docs/app/$page.js.css",
          },
        ],
      ]),
    );
    expect(urls).toEqual([
      "/app/_design/seam.css",
      "/@fs/repo/packages/brand/tokens.css",
      "/@id/uf-style:/repo/docs/app/$page.js.css",
    ]);
  });

  it("leaves out a stylesheet imported for its text or its URL rather than to apply", () => {
    const urls = devStylesheets(
      serverWith([
        [
          "/repo/docs/app/raw.css?inline",
          { file: "/repo/docs/app/raw.css", url: "/app/raw.css?inline" },
        ],
        ["/repo/docs/app/page.js", { file: "/repo/docs/app/page.js", url: "/app/page.js" }],
      ]),
    );
    expect(urls).toEqual([]);
  });
});

describe("which applications render React Server Components", () => {
  it("is every server-rendered web application unless `app.rsc` is false", () => {
    expect(rendersFlight({}, { mount: "hydrate", routeTarget: "web" })).toBe(true);
    expect(rendersFlight({ rsc: false }, { mount: "hydrate", routeTarget: "web" })).toBe(false);
    // A single-page build renders nothing on a server, and a native target has
    // no document to carry a payload.
    expect(rendersFlight({}, { mount: "render", routeTarget: "web" })).toBe(false);
    expect(rendersFlight({}, { mount: "hydrate", routeTarget: "native" })).toBe(false);
  });
});

describe("what the browser's graph pre-bundles for React Server Components", () => {
  /** The configuration `uf:flow` gives `uf dev`, for an application whose `app` is `app`. */
  function servedWith(app: $FlowFixMe): $FlowFixMe {
    const flow = uniflowed({ root: "/project", config: { app } }).find(
      (plugin) => plugin.name === "uf:flow",
    );
    return flow.config({ root: "/project" }, { mode: "development", command: "serve" });
  }

  it("is React's Flight client, under the specifier the router imports it by", () => {
    // ubugeeei-prod/uf#1126. An installed router is in `node_modules`, where
    // Vite pre-bundles nothing it first meets while serving, so the browser was
    // sent the CommonJS file and hydration threw. The optimizer finds a
    // pre-bundled dependency by the specifier that imports it, so the name is
    // read off the router rather than written here a second time.
    const reader = fs.readFileSync(
      new URL("../router/internal/flight-browser.js", import.meta.url),
      "utf8",
    );
    const imported = [...reader.matchAll(/from "(react-server-dom-parcel\/[^"]+)"/g)].map(
      (match) => match[1],
    );
    const { include } = servedWith({}).optimizeDeps;

    expect(imported.length > 0).toBe(true);
    for (const specifier of imported) {
      expect(FLIGHT_BROWSER_DEPENDENCIES).toContain(specifier);
      expect(include).toContain(specifier);
    }
  });

  it("is none of it for an application that renders no Server Component", () => {
    // `react-server-dom-parcel` is an optional peer of the router, which such a
    // project need not install (ubugeeei-prod/uf#992).
    const { include } = servedWith({ rsc: false }).optimizeDeps;

    for (const specifier of FLIGHT_BROWSER_DEPENDENCIES) {
      expect(include).not.toContain(specifier);
    }
  });
});
