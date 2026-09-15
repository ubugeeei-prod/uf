// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// React Server Components, as the bundler applies them (ubugeeei-prod/uf#519,
// ubugeeei-prod/uf#252).
//
// # A second module graph
//
// React's Flight renderer only runs where `react` is the build with no
// `useState` in it — the one a package exports under the `react-server`
// condition — and the HTML renderer and the browser only run where it is not.
// No single module graph can hold both, so a uf application is three:
//
//   * **`rsc`**, a Vite environment of its own, resolved under `react-server`.
//     It holds the route table, every page, layout and loader, and the Flight
//     renderer (`@uniflowed/router/rsc`). A module that opens with the use
//     client directive is not evaluated here: it is replaced by a client
//     reference per export, naming the chunk the browser loads it from.
//   * **`ssr`**, Vite's own server environment. It holds the HTML renderer, the
//     route handlers, the middleware, the action table — and the *server copy*
//     of every client module, which is what renders a client component into
//     HTML. It reaches the rsc graph through one module, the bridge.
//   * **`client`**, the browser's. Its entry hydrates from the payload the
//     document carries, and it holds no page, layout or loader — only the
//     client modules, each an entry of its own, loaded when a payload names it.
//
// Vite is still the whole bundler: every graph is an environment, every
// resolution is Vite's, and uf adds a transform, four virtual modules and the
// order the builds run in. `docs/architecture.md` has the picture.
//
// # A build is three passes, in this order
//
// 1. **rsc** records every client module it replaced, and leaves the manifest
//    those references read as an import, because no chunk URL exists yet.
// 2. **client** builds the entry and one entry per recorded client module, with
//    their export names kept, so each chunk still has the export a reference
//    asks for.
// 3. **ssr** bundles the rsc output in through the bridge, and resolves the
//    manifest the rsc output left open to the client build's chunk URLs.
//
// # Stylesheets come from the rsc graph
//
// A layout's stylesheet is imported by the layout, and the layout is in the rsc
// graph now, so the client build never sees it. The rsc build emits its assets,
// the driver copies them beside the client's, and the document links every
// stylesheet the rsc build emitted — the rule `assetsFromManifest` already
// applies to the client build, which links a route's stylesheet on every page.
// In development the rsc environment's module graph is read instead, once the
// route's modules have been imported.

import path from "node:path";

import {
  createRunnableDevEnvironment,
  defaultServerConditions,
  isCSSRequest,
  parseAst,
} from "vite";

/** The environment the Flight renderer runs in. */
export const RSC_ENVIRONMENT = "rsc";

/** The virtual modules this file generates; `./routes.js`'s `VIRTUAL` has the rest. */
export const FLIGHT_VIRTUAL = Object.freeze({
  /** The rsc graph's entry: the Flight renderer over the whole route table. */
  entry: "virtual:uf/rsc",
  /** The ssr graph's one door into the rsc graph. */
  bridge: "virtual:uf/rsc-bridge",
  /** The chunk URL of every client module, for the references a build writes. */
  manifest: "virtual:uf/client-manifest",
  /** The server copy of every client module, keyed by that URL. */
  references: "virtual:uf/client-references",
  /** `react/compiler-runtime`, as the rsc graph gets it; see `compilerRuntimeSource`. */
  compilerRuntime: "virtual:uf/rsc-compiler-runtime",
});

/**
 * `react/compiler-runtime` in the rsc graph.
 *
 * Every Flow module goes through the React Compiler, and what it emits calls
 * `c(size)` from `react/compiler-runtime` for a component's memo cache. React
 * 19.3's runtime reads that cache through `react`'s *client* internals, and the
 * `react` a graph resolved under `react-server` has only server internals — so
 * every compiled server component threw `Cannot read properties of undefined
 * (reading 'H')` before it rendered a byte.
 *
 * A server component renders once per request and never again, so there is
 * nothing for a cache to remember: this is the cache React's Flight renderer
 * itself hands `useMemoCache`, every slot the sentinel the compiled code tests
 * for, fresh on each call.
 */
export function compilerRuntimeSource() {
  return `const sentinel = Symbol.for("react.memo_cache_sentinel");
export function c(size) {
  const cache = new Array(size);
  for (let index = 0; index < size; index += 1) cache[index] = sentinel;
  return cache;
}
`;
}

/**
 * The global a development server leaves for the ssr graph to reach the rsc
 * graph by.
 *
 * A function that imports the rsc entry through the rsc environment's module
 * runner each time it is called, so an edit to a server component is in the
 * next render the way an edit to anything else is.
 */
export const DEV_RSC_HOOK = "uf.dev.rsc";

/**
 * The last segment of a route's payload URL.
 *
 * A third spelling, beside `packages/router/internal/flight.js` and
 * `packages/server/internal/flight.js`, because this file is plain JavaScript
 * that Vite imports before any Flow transform exists. `packages/server/flight.test.js`
 * holds all three to one answer.
 */
export const FLIGHT_SEGMENT = "__uf.flight";

/** The document a payload path is for, or `null` for any other path. */
export function flightDocumentPath(pathname) {
  const suffix = `/${FLIGHT_SEGMENT}`;
  if (!pathname.endsWith(suffix)) return null;
  const document = pathname.slice(0, -suffix.length);
  return document === "" ? "/" : document;
}

/**
 * The directive, spelled without quotes.
 *
 * `uf lint`'s `server/no-server-only-import-in-client` decides a file is a
 * client module by finding the directive in quotes on any line of code, and
 * this file, which imports `node:path`, is not one.
 */
const USE_CLIENT = `use client`;

/**
 * Whether an application renders through React Server Components.
 *
 * On unless `app.rsc` is `false`, read as `!== false` for the reason every
 * default-on flag in `../index.js` is. A single-page build (`modes: ["csr"]`)
 * renders nothing on a server, so it has no payload to render, and a native
 * target has no document to write one into.
 */
export function rendersFlight(app, { mount, routeTarget }) {
  return app?.rsc !== false && mount === "hydrate" && routeTarget === "web";
}

/**
 * The rsc environment, as `config()` declares it.
 *
 * `noExternal: true` because every module has to be resolved under
 * `react-server`: a dependency left to Node would be resolved by Node, under
 * the default conditions, and would import the `react` with `useState` in it.
 * `optimizeDeps` names React and the Flight server because both are CommonJS
 * and the module runner runs ES modules; uf's own packages are excluded for the
 * reason the client excludes them — they ship Flow.
 *
 * `process.env.NODE_ENV` is fixed in a build. A server bundle reads it at run
 * time otherwise, and a server started without it runs React's development
 * build, whose payload carries every server component's source location and
 * every error's stack — to the browser.
 *
 * @param {{ production: boolean, exclude: Array<string> }} options
 */
export function rscEnvironment({ production, exclude }) {
  return {
    consumer: "server",
    resolve: {
      conditions: ["react-server", ...defaultServerConditions],
      externalConditions: ["react-server", ...defaultServerConditions],
      noExternal: true,
    },
    optimizeDeps: {
      include: [
        "react",
        "react/jsx-runtime",
        "react/jsx-dev-runtime",
        "react-server-dom-parcel/server",
      ],
      exclude,
    },
    define: production ? { "process.env.NODE_ENV": JSON.stringify("production") } : {},
    dev: {
      createEnvironment(name, config) {
        return createRunnableDevEnvironment(name, config);
      },
    },
    build: {
      // Its stylesheets and images, which only this graph imports; see the
      // header. The manifest is how the driver finds the stylesheets.
      emitAssets: true,
      manifest: true,
    },
  };
}

/**
 * What the browser's graph pre-bundles for an application React Server
 * Components render: React's Flight client, spelled the way the router's
 * `internal/flight-browser.js` imports it, because the optimizer finds a
 * pre-bundled dependency by the specifier that imports it.
 *
 * Named rather than left to be discovered, because in a project that installs
 * the router nothing discovers it (ubugeeei-prod/uf#1126). The package is
 * CommonJS. `uf:flow` excludes every `@uniflowed/*` package from the optimizer,
 * since they ship Flow, and Vite pre-bundles a dependency it first meets while
 * serving only when the module importing it is outside `node_modules`. An
 * installed router is inside it, so the browser was sent the CommonJS file and
 * hydration stopped at "does not provide an export named 'createFromFetch'". A
 * linked router, the layout of this repository, is outside it, which is why
 * nothing here failed.
 *
 * It resolves from the project, which installs `react-server-dom-parcel` as the
 * router's peer.
 */
export const FLIGHT_BROWSER_DEPENDENCIES = Object.freeze([
  "react-server-dom-parcel/client.browser",
]);

/**
 * The state the plugins and the driver share for one application.
 *
 * `clientModules` is filled by the rsc graph's transform and read by the client
 * build, which makes each of them an entry. `chunkUrls` is filled by the driver
 * from the client build's manifest, and is what the ssr build resolves every
 * reference to. `rscOutput` is the rsc build's entry file, which the ssr build
 * bundles in; `null` under `uf dev`, where the rsc graph runs in-process.
 *
 * @param {{ root: string }} options
 */
export function createFlightState({ root }) {
  return {
    root,
    base: "/",
    production: false,
    clientModules: new Set(),
    chunkUrls: new Map(),
    rscOutput: null,
  };
}

/**
 * The plugin that replaces a client module with references, in the rsc graph.
 *
 * After `uf:flow`, which is a `pre` plugin, so a Flow module is JavaScript by
 * the time its exports are read — and after Vite's own transforms, so a `.jsx`
 * module is too. The module is parsed with Vite's parser rather than scanned:
 * the directive only counts as the first statement of the module, and an
 * export list is not something a regular expression reads correctly.
 *
 * In development a reference names the URL Vite serves the module at, which is
 * the URL the browser already imports it by from any other client module, so
 * both reach one instance. In a build it names the client manifest, which the
 * ssr build resolves once the client build has written the chunks.
 *
 * @param {ReturnType<typeof createFlightState>} state
 */
export function clientReferencePlugin(state) {
  return {
    name: "uf:rsc-references",
    applyToEnvironment(environment) {
      return environment.name === RSC_ENVIRONMENT;
    },
    transform(code, id) {
      const file = cleanId(id);
      let program = null;
      if (
        code.includes(USE_CLIENT) &&
        !id.startsWith("\0") &&
        !isCSSRequest(file) &&
        SCRIPT.test(file)
      ) {
        try {
          program = parseAst(code);
        } catch {
          program = null;
        }
      }
      if (program == null || !opensWithUseClient(program)) {
        // Forgotten as well as not recorded: a module whose directive was
        // removed under `uf dev` is a server module from that edit on, so
        // `hotUpdate` reloads the page for its next edit instead of leaving it
        // to Fast Refresh, which has nothing of it in the browser to replace.
        state.clientModules.delete(file);
        return null;
      }
      state.clientModules.add(file);
      const names = clientExportNames(program, projectPath(state.root, file));
      const lines = [`import { createClientReference } from "react-server-dom-parcel/server";`];
      if (state.production) {
        lines.push(`import { clientUrl } from ${JSON.stringify(FLIGHT_VIRTUAL.manifest)};`);
        lines.push(`const url = clientUrl(${JSON.stringify(file)});`);
      } else {
        lines.push(`const url = ${JSON.stringify(devUrlOf(state.root, state.base, file))};`);
      }
      names.forEach((name, index) => {
        lines.push(
          `const reference${index} = createClientReference(url, ${JSON.stringify(name)}, [url]);`,
        );
        lines.push(`export { reference${index} as ${JSON.stringify(name)} };`);
      });
      return { code: `${lines.join("\n")}\n`, map: null };
    },
  };
}

/**
 * The plugin that gives a file of a `@uniflowed/*` package one URL in the
 * browser under `uf dev`: its path, with no `?v=`.
 *
 * A client reference names the URL `devUrlOf` gives its file, and the browser
 * imports that URL when a payload names it. Vite gave the same file a second
 * URL when a client module imported it: a file in `node_modules` of a package
 * the dependency optimizer excludes — every `@uniflowed/*` package, because
 * they ship Flow — carries `?v=` and the optimizer's hash. A browser keys a
 * module by its URL, so those were two modules. `Dialog.Root` rendered by a
 * server component and `Dialog.Trigger` rendered by a client component held
 * two `DialogContext`s, and hydration threw "Dialog.Trigger must be rendered
 * inside a Dialog.Root". Only in a project that installed its packages, since a
 * linked package is not in `node_modules` and gets no query: every project but
 * this repository.
 *
 * The query only lets the browser cache the file without asking, so dropping it
 * costs a revalidation. `pre`, so it can ask Vite's resolver first and drop
 * what that added; only in the browser's graph, because the rsc graph records a
 * client module by its path already and the ssr graph has no optimizer.
 */
export function clientModuleUrlPlugin() {
  return {
    name: "uf:rsc-client-urls",
    apply: "serve",
    enforce: "pre",
    applyToEnvironment(environment) {
      return environment.name === "client";
    },
    async resolveId(id, importer, options) {
      if (!reachesUniflowedPackage(id, importer)) return null;
      const resolved = await this.resolve(id, importer, { ...options, skipSelf: true });
      if (resolved == null) return null;
      const unversioned = withoutVersion(resolved.id);
      return unversioned === resolved.id ? resolved : { ...resolved, id: unversioned };
    },
  };
}

/** Where every `@uniflowed/*` package is, installed, whatever manages `node_modules`. */
const UNIFLOWED_FILES = "/node_modules/@uniflowed/";

/**
 * Whether an import can resolve to a file of an installed `@uniflowed/*`
 * package: a bare import of one, a path or URL into one, or a relative import
 * from inside one. Everything else is left to Vite without a second resolution.
 */
function reachesUniflowedPackage(id, importer) {
  if (id.startsWith("\0")) return false;
  if (id.startsWith("@uniflowed/") || id.includes(UNIFLOWED_FILES)) return true;
  return /^\.\.?\//.test(id) && typeof importer === "string" && importer.includes(UNIFLOWED_FILES);
}

/** `id` without the optimizer's `v=`, for a file of an installed `@uniflowed/*` package. */
function withoutVersion(id) {
  const at = id.indexOf("?");
  if (at === -1 || !id.slice(0, at).includes(UNIFLOWED_FILES)) return id;
  const kept = id
    .slice(at + 1)
    .split("&")
    .filter((parameter) => !/^v=[\w.-]*$/.test(parameter));
  return kept.length === 0 ? id.slice(0, at) : `${id.slice(0, at)}?${kept.join("&")}`;
}

/** The module kinds a reference can stand in for. */
const SCRIPT = /\.(?:[cm]?js|jsx|mdx)$/;

/**
 * Whether a module's directive prologue holds the use client directive.
 *
 * The prologue rather than the first statement, because `"use strict"` may
 * come before it and is still a directive.
 */
export function opensWithUseClient(program) {
  for (const statement of program.body) {
    if (statement.type !== "ExpressionStatement" || typeof statement.directive !== "string") {
      return false;
    }
    if (statement.directive === USE_CLIENT) return true;
  }
  return false;
}

/**
 * Every name a client module exports, which is every reference it becomes.
 *
 * `export *` is refused rather than followed: a reference is one per export,
 * and the names behind a star are another module's, which this transform would
 * have to resolve and parse before it could write this one.
 *
 * @param {object} program
 * @param {string} file the module, as the error names it
 */
export function clientExportNames(program, file) {
  const names = [];
  for (const node of program.body) {
    if (node.type === "ExportDefaultDeclaration") {
      names.push("default");
    } else if (node.type === "ExportNamedDeclaration") {
      const declaration = node.declaration;
      if (declaration != null) {
        if (declaration.type === "VariableDeclaration") {
          for (const declarator of declaration.declarations) bindingNames(declarator.id, names);
        } else if (declaration.id != null) {
          names.push(declaration.id.name);
        }
      }
      for (const specifier of node.specifiers ?? []) names.push(exportedName(specifier.exported));
    } else if (node.type === "ExportAllDeclaration") {
      if (node.exported != null) {
        names.push(exportedName(node.exported));
        continue;
      }
      throw new Error(
        `uf: ${file} is a client module and re-exports everything from ` +
          `${JSON.stringify(node.source.value)}. A client module becomes one reference per ` +
          "export, so each export has to be named: write `export { A, B } from " +
          `${JSON.stringify(node.source.value)}\` instead.`,
      );
    }
  }
  return [...new Set(names)];
}

function exportedName(node) {
  return node.type === "Identifier" ? node.name : String(node.value);
}

function bindingNames(pattern, names) {
  if (pattern == null) return;
  switch (pattern.type) {
    case "Identifier":
      names.push(pattern.name);
      break;
    case "ObjectPattern":
      for (const property of pattern.properties) {
        bindingNames(property.type === "RestElement" ? property.argument : property.value, names);
      }
      break;
    case "ArrayPattern":
      for (const element of pattern.elements) bindingNames(element, names);
      break;
    case "RestElement":
      bindingNames(pattern.argument, names);
      break;
    case "AssignmentPattern":
      bindingNames(pattern.left, names);
      break;
    default:
      break;
  }
}

/**
 * The URL Vite serves `file` at in development.
 *
 * Root-relative for a file under the project, and `/@fs/` for one outside it —
 * a workspace package, which Vite resolves to its real path — which is the URL
 * any client module importing it is rewritten to as well.
 */
export function devUrlOf(root, base, file) {
  const relative = path.relative(root, file);
  const inside = relative !== "" && !relative.startsWith("..") && !path.isAbsolute(relative);
  const forward = file.split(path.sep).join("/");
  // `/@fs/` and then the path. On Windows the path starts at its drive letter,
  // `C:/work/button.js`, with no slash of its own to follow the prefix.
  const pathname = inside
    ? `/${relative.split(path.sep).join("/")}`
    : `/@fs${forward.startsWith("/") ? "" : "/"}${forward}`;
  return `${base.replace(/\/$/, "")}${pathname}`;
}

/**
 * The file a `/@fs/` URL's path names: the inverse of [`devUrlOf`] for a file
 * outside the project, read the way Vite reads that prefix. A POSIX path gets
 * its leading slash back; a Windows path starts at its drive letter.
 *
 * Also the body of the loader [`devReferencesSource`] generates, which is why
 * it closes over nothing.
 */
export function fsFileOf(pathname) {
  const rest = pathname.slice("/@fs/".length);
  return /^[A-Za-z]:\//.test(rest) ? rest : `/${rest}`;
}

/** `virtual:uf/rsc`: the Flight renderer over the rsc graph's route table. */
export function rscEntrySource(routesId) {
  return `import { createFlightRenderer } from "@uniflowed/router/rsc";
import { routes, notFound, errors } from ${JSON.stringify(routesId)};
export { routes, notFound, errors };
export const renderFlight = createFlightRenderer({ routes, notFound, errors });
`;
}

/**
 * `virtual:uf/rsc-bridge` under `uf dev`: the rsc graph, through its runner.
 *
 * `renderFlight` imports the rsc entry on every call rather than once, which
 * costs a map lookup when nothing changed and is what puts an edited server
 * component in the next render. The route table is read once, because nothing
 * under `uf dev` reads it — the driver's build is its reader.
 */
export function devBridgeSource() {
  return `const load = globalThis[Symbol.for(${JSON.stringify(DEV_RSC_HOOK)})];
if (typeof load !== "function") {
  throw new Error(
    "uf: the rsc environment is not running, so there is nothing to render a route with. " +
      "This module is served by uf dev, which starts that environment first.",
  );
}
export async function renderFlight(url, options) {
  return (await load()).renderFlight(url, options);
}
export const { routes, notFound, errors } = await load();
`;
}

/** `virtual:uf/rsc-bridge` in a build: the rsc build's output, bundled in. */
export function builtBridgeSource(rscOutput) {
  return `export { renderFlight, routes, notFound, errors } from ${JSON.stringify(rscOutput)};\n`;
}

/**
 * `virtual:uf/client-manifest` in the ssr build: every client module's chunk.
 *
 * Keyed by absolute path, which is what the rsc build's references were written
 * with. The paths stay in the server bundle; what reaches a payload, and so a
 * browser, is the URL.
 *
 * @param {Map<string, string>} chunkUrls
 */
export function clientManifestSource(chunkUrls) {
  return `const urls = new Map(${JSON.stringify([...chunkUrls])});
export function clientUrl(file) {
  const url = urls.get(file);
  if (url == null) {
    throw new Error(
      "uf: the client build wrote no chunk for " + file + ", which a server component " +
        "renders as a client component. Its passes disagree; run uf build again.",
    );
  }
  return url;
}
`;
}

/** `virtual:uf/client-references` under `uf dev`: the module at a dev URL. */
export function devReferencesSource(root, base) {
  return `const root = ${JSON.stringify(root)};
const base = ${JSON.stringify(base)};
const fsFileOf = ${fsFileOf.toString()};
export function loadClientModule(url) {
  const pathname = url.startsWith(base) ? url.slice(base.length - 1) : url;
  const file = pathname.startsWith("/@fs/") ? fsFileOf(pathname) : root + decodeURI(pathname);
  return import(/* @vite-ignore */ file);
}
`;
}

/**
 * `virtual:uf/client-references` in a build: the server copy at a chunk URL.
 *
 * One `import()` per client module, so a server bundle loads a client
 * component's server copy the first time a payload names it and not before.
 *
 * @param {Map<string, string>} chunkUrls
 */
export function builtReferencesSource(chunkUrls) {
  const entries = [...chunkUrls].map(
    ([file, url]) => `  [${JSON.stringify(url)}, () => import(${JSON.stringify(file)})],`,
  );
  return `const table = new Map([
${entries.join("\n")}
]);
export function loadClientModule(url) {
  const load = table.get(url);
  if (load == null) {
    return Promise.reject(
      new Error("uf: a payload named the client chunk " + url + ", and this server has no copy of it"),
    );
  }
  return load();
}
`;
}

/**
 * `virtual:uf/client` for an application React Server Components render.
 *
 * No route table: the browser resolves no route and imports no page. What it
 * has is the application root and the payload the document carries, which
 * `hydrateFlight` reads. That function comes from `@uniflowed/router/rsc/client`,
 * not from `@uniflowed/router/client`, the entry an application rendered from
 * its modules starts from. So only this kind of application has React's Flight
 * client in its bundle: `react-server-dom-parcel` is an optional peer of the
 * router, and a project on React 19.2 does not install it (ubugeeei-prod/uf#992).
 * Strict Mode and navigation are generated constants for the reasons
 * `clientModuleSource` in `./routes.js` gives.
 */
export function flightClientSource(appEntry, options = {}) {
  const strictMode = options.strictMode === true ? ", strictMode: true" : "";
  const navigation = options.navigation === "document" ? ', navigation: "document"' : "";
  return `import { hydrateFlight } from "@uniflowed/router/rsc/client";
import App from ${JSON.stringify(appEntry)};
hydrateFlight({ App${strictMode}${navigation} });
`;
}

/**
 * `virtual:uf/server` for an application React Server Components render.
 *
 * The exports and their order are `serverModuleSource`'s in `./routes.js`, and
 * that comment is the argument for them. Two things differ. The renderer is
 * `createDocumentRenderer` from `@uniflowed/router/rsc/ssr`, an entry of its own
 * for the reason `flightClientSource` gives. It renders the payload the rsc graph
 * writes rather than the route's modules, and adds `flight` for a browser that
 * is navigating. And `routes`, `notFound` and `errors` come through the bridge,
 * because the page modules they import are the rsc graph's: the driver reads a
 * page's `generateStaticParams` from the graph that renders it.
 */
export function flightServerSource(
  appEntry,
  routesId,
  actionsId,
  routing = { redirects: [], rewrites: [], headers: [] },
) {
  return `import {
  createActionDispatcher,
  createDispatcher,
  createMiddlewareRunner,
} from "@uniflowed/router/server";
import { createDocumentRenderer } from "@uniflowed/router/rsc/ssr";
import { handlers, middleware } from ${JSON.stringify(routesId)};
import { actions } from ${JSON.stringify(actionsId)};
import { renderFlight, routes, notFound, errors } from ${JSON.stringify(FLIGHT_VIRTUAL.bridge)};
import { loadClientModule } from ${JSON.stringify(FLIGHT_VIRTUAL.references)};
import App from ${JSON.stringify(appEntry)};
export { routes, handlers, middleware, notFound, errors };
export { beginRequest } from "@uniflowed/router/server";
const renderer = createDocumentRenderer({ App, renderFlight, loadClientModule });
export const render = renderer.render;
export const prerender = renderer.prerender;
export const flight = renderer.flight;
export { shellDocument } from "@uniflowed/router/server";
export const dispatch = createDispatcher({ handlers });
export const callAction = createActionDispatcher({ actions });
export const runMiddleware = createMiddlewareRunner({ middleware });
export const routing = ${JSON.stringify(routing)};
`;
}

/**
 * The stylesheets the rsc graph has imported so far, as development URLs.
 *
 * Read after the route's modules have been imported — `createDocumentRenderer`
 * reads a document's assets once the payload's route has resolved — so a
 * layout's stylesheet is in the graph by the time its document's head is
 * written. Every stylesheet the graph holds, in the order it met them, which is
 * the development version of the build's rule and cascades the same way.
 */
export function devStylesheets(server) {
  const environment = server.environments?.[RSC_ENVIRONMENT];
  if (environment == null) return [];
  const { root, base } = server.config;
  const urls = [];
  for (const [id, module] of environment.moduleGraph.idToModuleMap) {
    if (id.includes("?") || !isCSSRequest(cleanId(id))) continue;
    // From the file rather than the graph's own `url`, which is not the URL the
    // browser can fetch for a stylesheet outside the project — a workspace
    // package's, which Vite serves under `/@fs/` — and the same rule a client
    // reference's URL follows. A stylesheet with no file is a virtual one.
    urls.push(
      typeof module.file === "string" && module.file !== ""
        ? devUrlOf(root, base, module.file)
        : `${base.replace(/\/$/, "")}/@id/${id.replace(/\0/g, "__x00__")}`,
    );
  }
  return urls;
}

/**
 * `head` with a stylesheet link for each of `hrefs`, before `</head>` when the
 * head is closed and at its end when it is not.
 *
 * The document opening a development server transforms may stop inside the
 * head, which is why "at its end" is an answer rather than an error.
 */
export function linkStylesheets(head, hrefs) {
  if (hrefs.length === 0) return head;
  const links = hrefs
    .map(
      (href) =>
        `<link rel="stylesheet" href="${href.replace(/&/g, "&amp;").replace(/"/g, "&quot;")}">`,
    )
    .join("");
  const close = head.search(/<\/head>/i);
  return close === -1 ? `${head}${links}` : `${head.slice(0, close)}${links}${head.slice(close)}`;
}

/** A module's path as an error names it: project-relative when it can be. */
function projectPath(root, file) {
  const relative = path.relative(root, file);
  return relative.startsWith("..") || path.isAbsolute(relative)
    ? file
    : relative.split(path.sep).join("/");
}

function cleanId(id) {
  const at = id.indexOf("?");
  return at === -1 ? id : id.slice(0, at);
}
