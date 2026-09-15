// @flow
//
// The router without the peers it lists as optional, and on the oldest React it
// lists.
//
// `@uniflowed/router` installs beside React 19.2.3, the React that Expo SDK 57
// and React Native 0.87 ship, and lists `react-dom` and
// `react-server-dom-parcel` as optional peers (ubugeeei-prod/uf#992). A native
// app never has `react-dom`, and an application that renders no Server
// Component has no reason to install React's Flight package, which needs React
// 19.3. So every entry those applications import has to load without the two
// packages. The entries that do load Flight have to refuse an older React by
// name, before they do anything, instead of failing inside a render.
//
// Importing the package in place cannot show either. This workspace installs
// both peers and React 19.3 at its root, and every module here resolves from
// there. So each project below gets a copy of the router under
// `node_modules/@uniflowed/router`, where the Flow loader still transforms it,
// and a link for each package it is meant to have. Node resolves a bare
// specifier from the real path of the module that imports it, so a package the
// project was not given cannot be found.

import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { afterAll, describe, expect, it } from "@uniflowed/test";

import { serverComponentsRefusal } from "./internal/react-version.js";

const here = path.dirname(fileURLToPath(import.meta.url));
const workspace = path.resolve(here, "..", "..");
const projects: Array<string> = [];

afterAll(() => {
  for (const project of projects) {
    fs.rmSync(project, { recursive: true, force: true });
  }
});

/** The real directory of a package this workspace installed. */
function installed(name: string): string {
  return fs.realpathSync(path.join(workspace, "node_modules", name));
}

/** This workspace's own copy of a `@uniflowed/*` package. */
function sibling(name: string): string {
  return path.join(workspace, "packages", name);
}

/** Copy the router's shipped modules, and not its tests, from `from` to `to`. */
function copyRouter(from: string, to: string): void {
  fs.mkdirSync(to, { recursive: true });
  for (const entry of fs.readdirSync(from, { withFileTypes: true })) {
    const source = path.join(from, entry.name);
    const target = path.join(to, entry.name);
    if (entry.isDirectory()) {
      if (entry.name !== "node_modules") {
        copyRouter(source, target);
      }
    } else if (!entry.name.endsWith(".test.js")) {
      fs.copyFileSync(source, target);
    }
  }
}

/**
 * A project holding a copy of the router, a link for each package in `links`,
 * and each file in `files`, all under its `node_modules`.
 *
 * A copy and not a link: Node resolves from a module's real path, and a link to
 * `packages/router` would resolve every peer from this workspace again.
 */
function project(
  links: { readonly [string]: string },
  files: { readonly [string]: string } = {},
): string {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), "uf-router-peers-"));
  projects.push(root);
  const modules = path.join(root, "node_modules");
  copyRouter(here, path.join(modules, "@uniflowed", "router"));
  for (const name of Object.keys(links)) {
    const link = path.join(modules, name);
    fs.mkdirSync(path.dirname(link), { recursive: true });
    fs.symlinkSync(links[name], link, "dir");
  }
  for (const relative of Object.keys(files)) {
    const file = path.join(modules, relative);
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, files[relative]);
  }
  return root;
}

/** A module of the router copy in `root`, imported by its path. */
function routerModule(root: string, file: string): Promise<$FlowFixMe> {
  return import(pathToFileURL(path.join(root, "node_modules", "@uniflowed", "router", file)).href);
}

/** The message `run` fails with, whether it throws or rejects, or `null` when it succeeds. */
async function failureOf(run: () => mixed): Promise<string | null> {
  try {
    await run();
  } catch (error) {
    return error instanceof Error ? error.message : String(error);
  }
  return null;
}

/**
 * React 19.3, reporting itself as `version`.
 *
 * Every module of React the router imports, re-exported from this workspace's
 * React with `version` replaced, which is the one thing an entry reads to
 * decide. A real React 19.2.3 would need an install, and what is under test is
 * the refusal, not React.
 */
function reactReporting(version: string): { readonly [string]: string } {
  const react = installed("react");
  const reexport = (file: string): string => {
    const url = JSON.stringify(pathToFileURL(path.join(react, file)).href);
    return `export * from ${url};\nexport { default } from ${url};\n`;
  };
  return {
    "react/package.json": JSON.stringify({
      name: "react",
      version,
      type: "module",
      exports: {
        ".": "./index.js",
        "./compiler-runtime": "./compiler-runtime.js",
        "./jsx-dev-runtime": "./jsx-dev-runtime.js",
        "./jsx-runtime": "./jsx-runtime.js",
        "./package.json": "./package.json",
      },
    }),
    "react/index.js": `${reexport("index.js")}export const version = ${JSON.stringify(version)};\n`,
    "react/compiler-runtime.js": reexport("compiler-runtime.js"),
    "react/jsx-dev-runtime.js": reexport("jsx-dev-runtime.js"),
    "react/jsx-runtime.js": reexport("jsx-runtime.js"),
  };
}

let web: string | null = null;

/** A web application rendered from its modules: React 19.3 and `react-dom`, no Flight. */
function webProject(): string {
  if (web == null) {
    web = project({
      react: installed("react"),
      "react-dom": installed("react-dom"),
      "@uniflowed/hooks": sibling("hooks"),
      "@uniflowed/server": sibling("server"),
    });
  }
  return web;
}

let older: string | null = null;

/** An application on React 19.2.3 that installed Flight anyway. */
function olderReactProject(): string {
  if (older == null) {
    older = project(
      {
        "react-dom": installed("react-dom"),
        "react-server-dom-parcel": installed("react-server-dom-parcel"),
        "@uniflowed/hooks": sibling("hooks"),
        "@uniflowed/server": sibling("server"),
      },
      reactReporting("19.2.3"),
    );
  }
  return older;
}

describe("an application that renders no Server Component", () => {
  it("loads the native entry with neither react-dom nor Flight installed", async () => {
    const native = await routerModule(project({ react: installed("react") }), "native.js");

    expect(typeof native.createNativeRouter).toBe("function");
  });

  it("loads every entry a web app rendered from its modules imports, without Flight", async () => {
    const root = webProject();
    const index = await routerModule(root, "index.js");
    const client = await routerModule(root, "client.js");
    const server = await routerModule(root, "server.js");
    const routing = await routerModule(root, "routing.js");

    expect({
      RouterProvider: typeof index.RouterProvider,
      hydrate: typeof client.hydrate,
      render: typeof client.render,
      createRenderer: typeof server.createRenderer,
      matchRoute: typeof routing.matchRoute,
    }).toEqual({
      RouterProvider: "function",
      hydrate: "function",
      render: "function",
      createRenderer: "function",
      matchRoute: "function",
    });
  });

  it("finds Flight missing only in the entries that render Server Components", async () => {
    // The control: the project really lacks the package, so the cases above
    // loaded without it rather than finding it somewhere.
    const root = webProject();

    expect(await failureOf(() => routerModule(root, "rsc-client.js"))).toContain(
      "react-server-dom-parcel",
    );
    expect(await failureOf(() => routerModule(root, "rsc-ssr.js"))).toContain(
      "react-server-dom-parcel",
    );
  });
});

describe("React Server Components on React 19.2.3", () => {
  it("refuses to hydrate a payload, before it touches the page", async () => {
    const { hydrateFlight } = await routerModule(olderReactProject(), "rsc-client.js");
    const refusal = await failureOf(() => hydrateFlight({ App: () => null }));

    expect(refusal).toContain("@uniflowed/router/rsc/client needs React 19.3.0 or newer");
    expect(refusal).toContain("the React it loaded is 19.2.3");
  });

  it("refuses to render a document from a payload", async () => {
    const { createDocumentRenderer } = await routerModule(olderReactProject(), "rsc-ssr.js");
    const refusal = await failureOf(() =>
      createDocumentRenderer({
        App: () => null,
        renderFlight: () => Promise.reject(new Error("rendered on React 19.2.3")),
        loadClientModule: () => Promise.resolve({}),
      }),
    );

    expect(refusal).toContain("@uniflowed/router/rsc/ssr needs React 19.3.0 or newer");
    expect(refusal).toContain("the React it loaded is 19.2.3");
  });

  it("refuses to fetch the next route's payload", async () => {
    const { fetchFlight } = await routerModule(olderReactProject(), "internal/flight-browser.js");
    const refusal = await failureOf(() => fetchFlight("/next"));

    expect(refusal).toContain("@uniflowed/router/rsc/client needs React 19.3.0 or newer");
  });

  it("refuses a Server Component's route hook", async () => {
    // Through the function the hooks call, because a compiled hook reaches for
    // React's dispatcher before its body runs, and outside a render it has none.
    const { serverRoute } = await routerModule(olderReactProject(), "internal/server-route.js");
    const refusal = await failureOf(() => serverRoute("useRoute"));

    expect(refusal).toContain("useRoute() needs React 19.3.0 or newer");
  });

  it("lets React 19.3 through to the rule behind the check", async () => {
    const { serverRoute } = await routerModule(webProject(), "internal/server-route.js");
    const failure = await failureOf(() => serverRoute("useRoute"));

    expect(failure).toContain("outside a route");
    expect(failure).not.toContain("needs React");
  });
});

describe("the React a refusal names", () => {
  it("is refused below 19.3.0, with the version found, the one needed and the way out", () => {
    for (const version of ["19.2.3", "19.2.99", "18.3.1"]) {
      const refusal = serverComponentsRefusal("@uniflowed/router/rsc", version) ?? "";

      expect(refusal).toContain("@uniflowed/router/rsc needs React 19.3.0 or newer");
      expect(refusal).toContain(`the React it loaded is ${version}`);
      expect(refusal).toContain("`app.rsc: false`");
    }
  });

  it("is accepted from 19.3.0, a canary of 19.3 included", () => {
    const versions = ["19.3.0", "19.3.0-canary-2c8725fb-20250130", "19.4.1", "20.0.0"];

    expect(
      versions.map((version) => serverComponentsRefusal("@uniflowed/router/rsc", version)),
    ).toEqual([null, null, null, null]);
  });
});
