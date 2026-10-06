// @flow
// Hot updates that keep the page's state.
//
// Three edits used to reload the document under `uf dev` and now arrive as a
// render: a route module whose loader or metadata changed, a route file added
// or removed, and a server component (`rsc-hmr.test.js`). These pin the pieces
// the dev server generates for that — the Fast Refresh wrapper, the route
// table's loaders, `virtual:uf/client`, and what the watcher sends — and the
// boundary check the vendored runtime makes.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";

import { afterAll, describe, expect, it } from "@uniflowed/test";

import uniflowed from "./index.js";
import { flightClientSource } from "./internal/flight.js";
import { addRefreshWrapper } from "./internal/refresh.js";
import { clientModuleSource, routesModuleSource } from "./internal/routes.js";
// The DOM is installed before the refresh runtime is imported, and never
// after: the runtime writes to `window` while it is evaluated.
import { installDom } from "../../npm/react-testing/internal/dom.js";

const roots: Array<string> = [];
const closers: Array<() => void> = [];

afterAll(() => {
  for (const close of closers) close();
  for (const root of roots) fs.rmSync(root, { recursive: true, force: true });
});

/** The vendored Fast Refresh runtime, once there is a window to import it. */
async function refreshRuntime(): Promise<$FlowFixMe> {
  installDom();
  return import("./internal/refresh-runtime.js");
}

describe("the Fast Refresh wrapper", () => {
  const code = 'export default function Page() {}\n$RefreshReg$(Page, "Page");\n';

  it("hands a route module's next exports to the router", () => {
    const { code: wrapped } = addRefreshWrapper(code, null, "app/$page.js", { route: true });
    expect(wrapped).toContain('"app/$page.js", currentExports, nextExports, true)');
    // Before the boundary is checked, so a table rebuilt after an invalidation
    // still gets this instance.
    const handed = wrapped.indexOf('window.__UF_HOT_ROUTE__("app/$page.js", nextExports)');
    expect(handed).toBeGreaterThan(-1);
    expect(handed).toBeLessThan(wrapped.indexOf("validateRefreshBoundaryAndEnqueueUpdate"));
  });

  it("publishes every evaluation of a component module under its path", () => {
    const { code: wrapped } = addRefreshWrapper(code, null, "app/counter.js");
    expect(wrapped).toContain(
      "(window.__UF_LATEST_MODULES__ ??= new Map()).set(new URL(import.meta.url).pathname, currentExports);",
    );
  });

  it("leaves every other module to plain Fast Refresh", () => {
    const { code: wrapped } = addRefreshWrapper(code, null, "app/button.js");
    expect(wrapped).toContain('"app/button.js", currentExports, nextExports)');
    expect(wrapped).not.toContain("__UF_HOT_ROUTE__");
  });
});

describe("the refresh runtime's URL", () => {
  it("is the URL the preamble imports, so both get one runtime", () => {
    // A component module's import of the runtime is rewritten by Vite to the
    // URL of the id it resolved to. When that was `\0uf:react-refresh`, the
    // browser loaded the runtime twice and Fast Refresh never reached React.
    const plugin: $FlowFixMe = uniflowed({ root: "/project", config: {} })[0];
    expect(plugin.resolveId.call({}, "/@react-refresh")).toBe("/@react-refresh");
  });
});

describe("the refresh boundary", () => {
  component Page() {
    return null;
  }
  const loaderBefore = async () => 1;
  const loaderAfter = async () => 2;

  it("refuses a changed data export in an ordinary module", async () => {
    const runtime = await refreshRuntime();
    const message = runtime.validateRefreshBoundaryAndEnqueueUpdate(
      "app/util.js",
      { default: Page, loader: loaderBefore },
      { default: Page, loader: loaderAfter },
    );
    expect(message).toContain('"loader" export is incompatible');
  });

  it("accepts a route module's changed loader and metadata", async () => {
    const runtime = await refreshRuntime();
    const message = runtime.validateRefreshBoundaryAndEnqueueUpdate(
      "app/$page.js",
      { default: Page, loader: loaderBefore, metadata: { title: "a" } },
      { default: Page, loader: loaderAfter, metadata: { title: "b" } },
      true,
    );
    expect(message).toBeUndefined();
  });

  it("still refuses a route module that lost an export", async () => {
    const runtime = await refreshRuntime();
    const message = runtime.validateRefreshBoundaryAndEnqueueUpdate(
      "app/$page.js",
      { default: Page, loader: loaderBefore },
      { default: Page },
      true,
    );
    expect(message).toBe("Could not Fast Refresh (export removed)");
  });

  it("links to a page that exists", async () => {
    const runtime = await refreshRuntime();
    const message = runtime.validateRefreshBoundaryAndEnqueueUpdate(
      "app/util.js",
      { value: 1 },
      { value: 2 },
    );
    expect(message).not.toContain("__README_URL__");
  });
});

describe("the development route table", () => {
  const table = {
    routes: [
      {
        path     : "/",
        params   : [],
        mdx      : false,
        page     : "/project/app/$page.js",
        layouts  : ["/project/app/$layout.js"],
        loading  : [],
        templates: [],
        slots    : [],
      },
    ],
  };

  it("tags every loader with the module's path from the root", () => {
    const source = routesModuleSource(table, { relativeTo: "/project", hot: true });
    expect(source).toContain('hotLoader("app/$page.js", () => import("/project/app/$page.js"))');
    expect(source).toContain(
      'hotLoader("app/$layout.js", () => import("/project/app/$layout.js"))',
    );
    expect(source).toContain("load.ufHotFile = ufHotFile;");
    expect(source).toContain("export const hotFiles = new Set();");
    expect(source).toContain("hotFiles.add(ufHotFile);");
  });

  it("is the plain table in a build", () => {
    const source = routesModuleSource(table, { relativeTo: "/project" });
    expect(source).toContain('page: () => import("/project/app/$page.js")');
    expect(source).not.toContain("hotLoader");
  });
});

describe("virtual:uf/client", () => {
  it("accepts a new route table and listens for uf:refresh in development", () => {
    const source = clientModuleSource("/project/app.js", { mount: "hydrate", hot: true });
    expect(source).toContain(
      'import { hydrate, acceptHotRouteModules, refreshForHotUpdate, replaceRoutesForHotUpdate } from "@uniflowed/router/client";',
    );
    expect(source).toContain('import.meta.hot.accept("virtual:uf/routes"');
    expect(source).toContain('import.meta.hot.on("uf:refresh"');
    expect(source).toContain("acceptHotRouteModules();");
  });

  it("listens for uf:refresh under React Server Components", () => {
    const source = flightClientSource("/project/app.js", { hot: true });
    expect(source).toContain(
      'import { hydrateFlight, acceptHotRouteModules, refreshForHotUpdate } from "@uniflowed/router/rsc/client";',
    );
    expect(source).toContain("acceptHotRouteModules();");
    expect(source).toContain('import.meta.hot.on("uf:refresh"');
  });

  it("carries none of it in a build", () => {
    for (const source of [
      clientModuleSource("/project/app.js", { mount: "hydrate" }),
      flightClientSource("/project/app.js"),
    ]) {
      expect(source).not.toContain("import.meta.hot");
      expect(source).not.toContain("ForHotUpdate");
    }
  });
});

describe("a route file added to a running dev server", () => {
  /** A plugin serving a fresh project, and what its watcher and socket were handed. */
  function serve(app: { ... }) {
    const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-hmr-"));
    roots.push(root);
    fs.mkdirSync(path.join(root, "app"), { recursive: true });
    fs.writeFileSync(path.join(root, "app/$page.js"), "export default function Page() {}\n");
    const plugin: $FlowFixMe = uniflowed({ root, config: { app } })[0];
    plugin.configResolved({ root, base: "/", command: "serve", isProduction: false });

    const sent = [];
    const reloaded = [];
    const listeners: { [string]: Array<(file: string) => void> } = {};
    const table = { id: "\0virtual:uf/routes" };
    const graph = {
      getModuleById: (id: string) => (id === table.id ? table : null),
      invalidateModule() {},
    };
    plugin.configureServer({
      middlewares: { use() {} },
      httpServer: {
        once(event, close) {
          if (event === "close") closers.push(close);
        },
      },
      watcher: {
        on(event, listener) {
          (listeners[event] ??= []).push(listener);
        },
        add() {},
      },
      moduleGraph : graph,
      environments: { client: { hot: { send: (message) => sent.push(message) } } },
      ws          : { send: (message) => sent.push(message) },
      reloadModule: async (module) => {
        reloaded.push(module);
      },
    });
    const add = (file: string) => {
      for (const listener of listeners.add ?? []) listener(path.join(root, file));
    };
    return { add, sent, reloaded, table };
  }

  it("is a render of the URL on screen under React Server Components", () => {
    const { add, sent, reloaded } = serve({});
    add("app/about/$page.js");
    expect(sent).toEqual([{ type: "custom", event: "uf:refresh", data: {} }]);
    expect(reloaded).toEqual([]);
  });

  it("is a hot update of the browser's table otherwise", () => {
    const { add, sent, reloaded, table } = serve({ rsc: false });
    add("app/about/$page.js");
    expect(reloaded).toEqual([table]);
    expect(sent).toEqual([]);
  });

  it("ignores a file that is not a route file", () => {
    const { add, sent, reloaded } = serve({ rsc: false });
    add("app/about/helpers.js");
    expect(reloaded).toEqual([]);
    expect(sent).toEqual([]);
  });
});
