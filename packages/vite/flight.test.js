// @flow
//
// The bundler's half of React Server Components: what `@uniflowed/vite` does
// to a module in the rsc graph, and what it resolves there.
//
// Each case is a hook or a generator called directly, without a server, because
// each is a decision a line of `internal/flight.js` or `index.js` makes and a
// build is a slow way to find out which line. What a real build and a real
// server do with them is `crates/uf_cli/tests/vite.rs`. See ubugeeei-prod/uf#519.

import { describe, expect, it } from "@uniflowed/test";
import { parseAst } from "vite";

import uniflowed from "./index.js";
import {
  RSC_ENVIRONMENT,
  clientExportNames,
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

describe("the stylesheets a development document links from the rsc graph", () => {
  /** A dev server whose rsc graph holds `modules`, as `[id, { file, url }]`. */
  function serverWith(modules: Array<[string, { +file: ?string, +url: string }]>): $FlowFixMe {
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
        // A module's StyleX sheet, which has no file.
        [
          "uf-style:/repo/docs/app/$page.js.css",
          { file: null, url: "/@id/uf-style:/repo/docs/app/$page.js.css" },
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
