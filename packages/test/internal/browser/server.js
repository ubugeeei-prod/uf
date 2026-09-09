// @flow
//
// Internal to `@uniflowed/test`: the modules a page imports, over HTTP.
//
// A Node worker gets its modules from a loader hook: Node asks uf what a file
// is, uf answers with transformed JavaScript, and resolution stays Node's. A
// page has no such hook. What it has is a URL for every module and an import
// map for every bare specifier, and this module is those two things — the
// smallest server that can hand a browser a uf project.
//
// # Why a server and not a bundle
//
// Bundling the file under test would make `uf test --browser` a second build
// pipeline: a graph walker, a tree shaker, a chunking strategy and a source-map
// merge, every one of which uf already owns somewhere else and none of which
// belongs to a test runner. `docs/red-lines.md` is explicit that uf owns
// orchestration rather than implementation, and a bundler inside the test
// runner is the opposite of that.
//
// Serving is the cheaper half of the same job. Each module is served at the
// URL its path already implies, transformed by the same `uf transform` a build
// uses; relative specifiers resolve as URLs, with no rewriting; bare
// specifiers resolve through one import map the page is given before it loads
// anything. Nothing walks the graph, because the browser walks it.
//
// # The three things a page needs that a Node worker does not
//
// **An import map.** `import { render } from "@uniflowed/react-testing"` is a
// specifier no browser resolves. The map is built from *manifests* rather than
// from sources — every package the project depends on, and everything those
// depend on, with the entry points their `exports` declare — so no JavaScript
// is parsed to build it and a specifier that could legally be written is in it.
// See `importMap`.
//
// **A `browser` field that is honoured.** `@uniflowed/test` maps six `node:`
// builtins to a shim, `@uniflowed/host` maps a seventh, and both mappings mean
// something different inside each package. That is what an import map's
// `scopes` are: a mapping that applies to one importer's directory and not to
// the whole page.
//
// **CommonJS, wrapped.** React ships CommonJS, a browser imports ESM, and a
// component test that cannot import React is not a component test. See
// `wrapCommonJs` for what the wrapper does, what it refuses, and why the list
// of named exports is read from Node rather than lexed out of the source.
//
// # What this server will not serve
//
// Only files under a root the run named, and only after the path has been
// resolved and checked against those roots. The server is bound to 127.0.0.1
// and lives for one run, but "a local port that reads any file" is a shape
// worth not having at all.

import { createServer } from "node:http";
import { createRequire } from "node:module";
import { readFileSync, readdirSync, realpathSync } from "node:fs";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

import { isFlowModule } from "@uniflowed/host/transform";

/** Where a file on disk is served from. */
const FILE_PREFIX = "/@fs";

/** Where the harness page lives, so a person can open it by hand. */
const PAGE_PATH = "/uf-test/";

/** How many packages the manifest crawl will visit. */
const MAX_PACKAGES = 4_000;

/**
 * Conditions honoured in an `exports` map, in order.
 *
 * `browser` first, because that is what the page is, then the ESM spellings,
 * because a browser imports ES modules and the CommonJS wrapper below is a
 * fallback rather than a preference. `require` is last for the same reason.
 * `development` sits between them: `uf test` runs the development build of a
 * library where there is a choice, because a test that fails should say why
 * and a production build says nothing.
 */
const CONDITIONS = ["browser", "import", "module", "development", "default", "require"];

/** A `package.json`, as this module reads one. */
type Manifest = {
  readonly name?: string,
  readonly type?: string,
  readonly main?: string,
  readonly module?: string,
  readonly exports?: mixed,
  readonly browser?: mixed,
  readonly dependencies?: { readonly [string]: string },
  readonly peerDependencies?: { readonly [string]: string },
  ...
};

/** One package the crawl found. */
type Package = {|
  /** The directory holding its `package.json`, real path. */
  readonly directory: string,
  readonly manifest: Manifest,
|};

/** What `create` hands back. */
export type ModuleServer = {|
  /** Where the page lives. */
  readonly url: string,
  /** Hand the page its next file, and take the events it writes. */
  readonly offer: (request: PageRequest | null) => void,
  /** Called with every event the page posts. */
  readonly onEvent: (listener: (event: { readonly [string]: mixed }) => void) => void,
  /** Whether the page has asked for work at least once. */
  readonly connected: () => boolean,
  readonly close: () => Promise<void>,
|};

/** One file, as the page is told about it. */
export type PageRequest = {|
  readonly file: string,
  readonly filter: string | null,
  readonly timeoutMs: number,
  readonly generation: number,
|};

/** How `create` is configured. */
export type ServerOptions = {|
  /** The project root; every served file must be under this or a package. */
  readonly root: string,
  /** Transforms a Flow module, or answers `null` when it is not uf's. */
  readonly transform: (id: string, code: string) => Promise<string | null>,
  /** Where a message about a refused request goes. */
  readonly warn: (message: string) => void,
|};

/**
 * Start the module server on a free port of the loopback interface.
 *
 * Port zero, resolved after listening: a fixed port is a collision with
 * whatever else the machine is running, and a run that fails because somebody's
 * dev server is on 5173 would be uf's fault for having an opinion about it.
 */
export async function create(options: ServerOptions): Promise<ModuleServer> {
  const root = realpathSync(options.root);
  const packages = crawlPackages(root);
  const map = importMap(packages);
  const roots = [root, ...packages.map((entry) => entry.directory)];
  const listeners: Array<(event: { readonly [string]: mixed }) => void> = [];
  const transformed: Map<string, Promise<string>> = new Map();

  /** The request the page is waiting for, and how to hand it over. */
  let waiting: ((body: string) => void) | null = null;
  let pending: PageRequest | null = null;
  let finished = false;
  let asked = false;

  function offer(request: PageRequest | null): void {
    if (request == null) {
      finished = true;
    } else {
      pending = request;
    }
    const answer = waiting;
    if (answer == null) return;
    const body = takeAnswer();
    if (body == null) return;
    waiting = null;
    answer(body);
  }

  /** The body of a `/next` reply, or `null` when there is nothing to say yet. */
  function takeAnswer(): string | null {
    if (pending != null) {
      const request = pending;
      pending = null;
      return JSON.stringify(request);
    }
    return finished ? JSON.stringify({ done: true }) : null;
  }

  const server = createServer((incoming, outgoing) => {
    const url = incoming.url ?? "/";
    const at = url.indexOf("?");
    const route = at === -1 ? url : url.slice(0, at);
    if (route === "/" || route === PAGE_PATH) {
      reply(outgoing, 200, "text/html; charset=utf-8", harnessHtml(map));
      return;
    }
    if (route === "/uf-test/next") {
      asked = true;
      const body = takeAnswer();
      if (body != null) {
        reply(outgoing, 200, "application/json", body);
        return;
      }
      // Held open. The page has nothing to do until `uf` says so, and a poll
      // loop would be a busy wait against a runner that is already streaming.
      waiting = (answer: string) => reply(outgoing, 200, "application/json", answer);
      outgoing.on("close", () => {
        waiting = null;
      });
      return;
    }
    if (route === "/uf-test/events") {
      readBody(incoming, (body) => {
        for (const event of parseEvents(body, options.warn)) {
          for (const listener of listeners) listener(event);
        }
        reply(outgoing, 200, "application/json", "{}");
      });
      return;
    }
    if (route.startsWith(`${FILE_PREFIX}/`)) {
      serveFile(decodeURIComponent(route.slice(FILE_PREFIX.length)), roots, transformed, options)
        .then((served) => reply(outgoing, 200, served.type, served.body))
        .catch((error: Error) => {
          // A module the page could not have is answered *as a module*, so the
          // failure arrives as the sentence below in the test's own report
          // rather than as a browser console line nobody is reading. A 404
          // would surface as "Failed to fetch dynamically imported module",
          // which names the importer and never the missing file.
          reply(
            outgoing,
            200,
            "text/javascript; charset=utf-8",
            `throw new Error(${JSON.stringify(`uf test --browser: ${error.message}`)});\n`,
          );
        });
      return;
    }
    reply(outgoing, 404, "text/plain; charset=utf-8", "not found\n");
  });

  await new Promise((resolve) => {
    server.listen(0, "127.0.0.1", resolve);
  });
  const address = server.address();
  const port = address != null && typeof address === "object" ? address.port : 0;

  return {
    url: `http://127.0.0.1:${String(port)}${PAGE_PATH}`,
    offer,
    onEvent: (listener) => {
      listeners.push(listener);
    },
    connected: () => asked,
    close: () =>
      new Promise((resolve) => {
        // Whatever is still held open is answered first: a page blocked on
        // `/next` would otherwise keep the socket, and `close` waits for every
        // connection to end.
        offer(null);
        server.closeAllConnections();
        server.close(() => resolve());
      }),
  };
}

/** Everything a request handler needs to say to say one thing. */
function reply(outgoing: $FlowFixMe, status: number, type: string, body: string): void {
  outgoing.writeHead(status, {
    "content-type": type,
    "cache-control": "no-store",
  });
  outgoing.end(body);
}

/** Read a request body, bounded, and hand it over. */
function readBody(incoming: $FlowFixMe, done: (body: string) => void): void {
  let body = "";
  incoming.on("data", (chunk: mixed) => {
    if (body.length < 8 * 1024 * 1024) body += String(chunk);
  });
  incoming.on("end", () => done(body));
}

/**
 * The events in one POST.
 *
 * Everything a page posts is untrusted — it is whatever the test file caused to
 * be written — so a malformed body is a warning and not an exception: the run
 * loses those events and keeps its deadline, which is the trade `uf test` makes
 * everywhere else it reads from a host.
 */
function parseEvents(
  body: string,
  warn: (message: string) => void,
): Array<{ readonly [string]: mixed }> {
  try {
    const parsed = JSON.parse(body);
    if (!Array.isArray(parsed)) return [];
    return parsed.filter((event) => event != null && typeof event === "object");
  } catch (error) {
    warn(`unreadable page event: ${String(error)}`);
    return [];
  }
}

/** A served module: its body and what to call it. */
type Served = {| readonly body: string, readonly type: string |};

/**
 * One file, transformed if it is uf's and wrapped if it is CommonJS.
 *
 * Cached by path for the life of the run rather than per page load. The page
 * reloads between files — that is how a browser run isolates one file from the
 * next — and re-transforming a hundred unchanged modules on every reload would
 * make the isolation cost more than the run.
 */
async function serveFile(
  requested: string,
  roots: $ReadOnlyArray<string>,
  cache: Map<string, Promise<string>>,
  options: ServerOptions,
): Promise<Served> {
  const absolute = path.resolve("/", requested);
  let real: string;
  try {
    real = realpathSync(absolute);
  } catch {
    throw new Error(`there is no file at ${absolute}`);
  }
  // Resolved first, then checked. A prefix test against the requested path
  // would be a prefix test against `..` and a symlink.
  if (!roots.some((allowed) => real === allowed || real.startsWith(`${allowed}${path.sep}`))) {
    throw new Error(
      `${absolute} is outside this project and the packages it depends on, so the browser is not being handed it`,
    );
  }
  if (!/\.(?:js|jsx|mjs|cjs)$/.test(real)) {
    // A JSON import, a stylesheet, an asset. Not this change: a page can have
    // them and `uf test --browser` has no opinion about what they should mean
    // yet, so saying so is better than serving bytes the page will misread.
    throw new Error(
      `${path.basename(real)} is not a JavaScript module, and \`uf test --browser\` serves no others yet`,
    );
  }
  let body = cache.get(real);
  if (body == null) {
    body = buildModule(real, options);
    cache.set(real, body);
  }
  return { body: await body, type: "text/javascript; charset=utf-8" };
}

/** A file's text, as the page should see it. */
async function buildModule(real: string, options: ServerOptions): Promise<string> {
  const source = readFileSync(real, "utf8");
  if (isCommonJs(real)) {
    return wrapCommonJs(real, source);
  }
  if (!isFlowModule(real)) {
    return source;
  }
  const transformed = await options.transform(real, source);
  return transformed ?? source;
}

/**
 * Whether Node would treat this file as CommonJS.
 *
 * Node's own rule rather than a look at the source: `.cjs` is CommonJS, `.mjs`
 * is not, and a `.js` is whichever the nearest `package.json` says. Guessing
 * from the text — "it says `require(`, so…" — gets a module that mentions the
 * word in a comment wrong, and gets an ES module that assigns to a local called
 * `exports` wrong in the other direction.
 */
function isCommonJs(file: string): boolean {
  if (file.endsWith(".cjs")) return true;
  if (file.endsWith(".mjs")) return false;
  const manifest = nearestManifest(path.dirname(file));
  return manifest?.type !== "module";
}

/**
 * A CommonJS module, as an ES module.
 *
 * # What this is and is not
 *
 * It is an adapter, not an implementation of CommonJS. React and the scheduler
 * are the reason it exists: they ship CommonJS, `@uniflowed/react` re-exports
 * React by name, and a browser cannot import any of it. Everything below is
 * the smallest thing that makes those work, and each limit is a throw with a
 * sentence rather than a silence:
 *
 * * `require` answers for the specifiers found in the module's own source and
 *   refuses every other. A `require` built at runtime — `require(name)` —
 *   cannot be served, because the import that would satisfy it has to be
 *   written before the page fetches the module.
 * * There is no `require.cache`, no `require.resolve`, and no circular
 *   dependency support beyond what ESM already gives.
 * * `process` is `{ env: { NODE_ENV: "development" } }` and nothing else, which
 *   is the whole of what a browser build of a CommonJS package reads.
 *
 * # Why the named exports are read from Node
 *
 * `export * from "react"` needs React's export *names*, statically, and a
 * CommonJS module has none: they exist only after the body has run. Node
 * solves this by lexing the source for assignments to `exports`; uf solves it
 * by asking the module. The driver is a Node process, it can `require` the
 * file, and the keys of what comes back are exactly the names — no lexer, no
 * heuristics, and no drift between what the page sees and what the same
 * `import` would see on a Node worker.
 *
 * A module that throws when Node loads it falls back to a default export
 * alone. That is honest: the failure then arrives in the page, from the same
 * code, with the browser's own message.
 */
function wrapCommonJs(file: string, source: string): string {
  const specifiers = requiredSpecifiers(source);
  const load = createRequire(pathToFileURL(file).href);
  const imports: Array<string> = [];
  const entries: Array<string> = [];
  specifiers.forEach((specifier, index) => {
    // Resolved here so the page is given a URL rather than a specifier its
    // import map may not carry: a `require` of a relative path is not a bare
    // specifier, and an import map has nothing to say about one.
    let target: string;
    try {
      target = moduleUrl(load.resolve(specifier));
    } catch {
      // Left to the page. `require("node:fs")` inside a dependency is a real
      // thing to write and a real thing to fail on, and the failure should
      // name the specifier at the moment it is reached rather than stop the
      // whole module from loading.
      entries.push(
        `${JSON.stringify(specifier)}: () => { throw new Error(${JSON.stringify(
          `uf test --browser: a require of ${JSON.stringify(specifier)} in ${path.basename(file)} names something a page cannot be given`,
        )}); }`,
      );
      return;
    }
    imports.push(`import * as __uf_${String(index)} from ${JSON.stringify(target)};`);
    entries.push(
      `${JSON.stringify(specifier)}: () => __uf_${String(index)}.default ?? __uf_${String(index)}`,
    );
  });

  const names = exportNames(load, file);
  const reexports = names.map(
    (name) => `export const ${name} = __uf_pick(${JSON.stringify(name)});`,
  );
  const unwritten = JSON.stringify(
    `uf test --browser: ${path.basename(file)} does not contain a literal require of `,
  );
  const built = JSON.stringify(
    ", so no import could be made for it. A specifier built at run time cannot be served to a page.",
  );

  return [
    ...imports,
    "const __uf_deps = {",
    ...entries.map((entry) => `  ${entry},`),
    "};",
    "const __uf_module = { exports: {} };",
    "const __uf_require = (specifier) => {",
    "  const get = __uf_deps[specifier];",
    "  if (get == null) {",
    `    throw new Error(${unwritten} + JSON.stringify(specifier) + ${built});`,
    "  }",
    "  return get();",
    "};",
    'const __uf_process = globalThis.process ?? { env: { NODE_ENV: "development" }, platform: "browser", argv: [] };',
    "(function (module, exports, require, process, __filename, __dirname) {",
    source,
    `})(__uf_module, __uf_module.exports, __uf_require, __uf_process, ${JSON.stringify(file)}, ${JSON.stringify(path.dirname(file))});`,
    "const __uf_exports = __uf_module.exports;",
    "const __uf_pick = (name) => (__uf_exports == null ? undefined : __uf_exports[name]);",
    "export default __uf_exports;",
    ...reexports,
    "",
  ].join("\n");
}

/** Every `require("…")` written in a source, in order, without repeats. */
function requiredSpecifiers(source: string): Array<string> {
  const found: Array<string> = [];
  // Deliberately generous. An extra entry costs one import of a module the
  // page was going to be able to reach anyway; a missing one is a throw naming
  // the specifier. Over-matching is the safe direction, which is why this can
  // be a pattern rather than a parser.
  const pattern = /\brequire\(\s*["']([^"'\n]+)["']\s*\)/g;
  let match = pattern.exec(source);
  while (match != null) {
    const specifier = match[1];
    if (!found.includes(specifier)) found.push(specifier);
    match = pattern.exec(source);
  }
  return found;
}

/** The names a CommonJS module exports, as Node sees them. */
function exportNames(load: (specifier: string) => mixed, file: string): Array<string> {
  let loaded: mixed;
  try {
    loaded = load(file);
  } catch {
    return [];
  }
  // A CommonJS module is whatever `module.exports` ended up as, which is
  // usually an object and is a function often enough to matter — `express` and
  // half of npm export one. Both carry own enumerable keys and both are read
  // here through the same indexed shape, which is the amount this function
  // knows about either.
  if (loaded == null || (typeof loaded !== "object" && typeof loaded !== "function")) {
    return [];
  }
  const exported: { readonly [string]: mixed } = loaded as $FlowFixMe;
  const names = new Set<string>();
  for (const name of Object.keys(exported)) {
    // `default` is written separately, and a key that is not an identifier
    // cannot be an export name. Both are dropped rather than mangled: a name
    // nobody can import is not worth inventing a spelling for.
    if (name !== "default" && /^[A-Za-z_$][A-Za-z0-9_$]*$/.test(name) && !isReserved(name)) {
      names.add(name);
    }
  }
  return [...names];
}

/** Words that cannot be a `const`. */
const RESERVED: Set<string> = new Set([
  "await",
  "break",
  "case",
  "catch",
  "class",
  "const",
  "continue",
  "debugger",
  "default",
  "delete",
  "do",
  "else",
  "enum",
  "export",
  "extends",
  "false",
  "finally",
  "for",
  "function",
  "if",
  "import",
  "in",
  "instanceof",
  "new",
  "null",
  "return",
  "super",
  "switch",
  "this",
  "throw",
  "true",
  "try",
  "typeof",
  "var",
  "void",
  "while",
  "with",
  "yield",
  "let",
  "static",
]);

function isReserved(name: string): boolean {
  return RESERVED.has(name);
}

/** The URL a file on disk is served at. */
export function moduleUrl(file: string): string {
  return `${FILE_PREFIX}${realpathSync(file).split(path.sep).map(encodeURIComponent).join("/")}`;
}

/**
 * The page, which is one import map and one module.
 *
 * Deliberately the smallest document that can hold a test: no stylesheet, no
 * viewport meta, nothing that would make a measurement here disagree with a
 * measurement in the application. A browser's defaults are the answer a
 * component test wants, and every line added to this document is a line
 * standing between the test and them.
 *
 * `<!doctype html>` is not decoration. Without it a browser is in quirks mode,
 * where `box-sizing`, table cell heights and percentage heights all behave
 * differently — a layout answer from a quirks-mode page would be a wrong
 * answer wearing a real browser's authority.
 */
function harnessHtml(map: string): string {
  const page = moduleUrl(fileURLToPath(new URL("./page.js", import.meta.url).href));
  return [
    "<!doctype html>",
    '<html lang="en">',
    '<meta charset="utf-8">',
    "<title>uf test</title>",
    `<script type="importmap">${map}</script>`,
    `<script type="module" src="${page}"></script>`,
    "",
  ].join("\n");
}

// ---------------------------------------------------------------------- //
// The import map
// ---------------------------------------------------------------------- //

/**
 * Every package the project can reach, from `node_modules` rather than source.
 *
 * A bare specifier a file may legally write is a package installed somewhere
 * above it, and *that* is a directory listing — no parser, no graph walk, no
 * chance of disagreeing with what a file actually imports in a direction that
 * matters. The map is allowed to be wider than the graph; a package nothing
 * imports costs two lines of JSON and is never fetched.
 *
 * The order is Node's: the nearest `node_modules` first, climbing to the root.
 * A workspace is covered without knowing what a workspace is, because a
 * workspace is a symlink in `node_modules` and this reads links.
 *
 * # The one place this is narrower than Node
 *
 * Two copies of one package at two depths are one entry here and two modules
 * on Node: an import map's `imports` is global, so the page gets whichever copy
 * is nearest the project. Making it exact would mean a `scopes` entry per
 * importing directory — the resolution algorithm written out as data — and the
 * shape it would fix is a project whose test and whose component disagree about
 * which React they mean, which is a broken install rather than a browser-mode
 * problem.
 */
function crawlPackages(root: string): Array<Package> {
  const found: Map<string, Package> = new Map();
  const seen: Set<string> = new Set();
  const queue: Array<string> = [];

  // Every `node_modules` from the project up to the filesystem root, nearest
  // first, then each installed package's own nested one.
  let directory = root;
  for (;;) {
    queue.push(path.join(directory, "node_modules"));
    const parent = path.dirname(directory);
    if (parent === directory) break;
    directory = parent;
  }

  while (queue.length > 0 && found.size < MAX_PACKAGES) {
    const modules = queue.shift();
    if (modules == null || seen.has(modules)) continue;
    seen.add(modules);
    for (const name of packageNames(modules)) {
      const linked = path.join(modules, name);
      let real: string;
      try {
        real = realpathSync(linked);
      } catch {
        continue;
      }
      // The first copy found wins, which is the nearest one; see above.
      if (found.has(real)) continue;
      const manifest = readManifest(path.join(real, "package.json"));
      if (manifest == null) continue;
      found.set(real, { directory: real, manifest });
      queue.push(path.join(real, "node_modules"));
    }
  }
  return [...found.values()];
}

/**
 * The package names directly inside one `node_modules`.
 *
 * `@scope/name` is two directory levels and one name, which is the only thing
 * this has to know about how npm lays a directory out.
 */
function packageNames(modules: string): Array<string> {
  const names: Array<string> = [];
  for (const entry of readDirectory(modules)) {
    if (entry.startsWith(".")) continue;
    if (entry.startsWith("@")) {
      for (const scoped of readDirectory(path.join(modules, entry))) {
        if (!scoped.startsWith(".")) names.push(`${entry}/${scoped}`);
      }
      continue;
    }
    names.push(entry);
  }
  return names;
}

/** One directory's entries, or none when there is no such directory. */
function readDirectory(directory: string): Array<string> {
  try {
    return readdirSync(directory);
  } catch {
    return [];
  }
}

/** A manifest, or `null` when there is not one or it is not readable. */
function readManifest(file: string): Manifest | null {
  try {
    const parsed = JSON.parse(readFileSync(file, "utf8"));
    return parsed != null && typeof parsed === "object" ? parsed : null;
  } catch {
    return null;
  }
}

/**
 * The nearest `package.json` at or above a directory.
 *
 * A `while` with a named stopping condition rather than a `for (;;)` with two
 * `return`s in it: the checker cannot see that an unbounded loop always leaves
 * through one of them, and a reader has to take the same thing on trust.
 */
function nearestManifest(from: string): Manifest | null {
  let directory = from;
  let parent = path.dirname(directory);
  while (parent !== directory) {
    const manifest = readManifest(path.join(directory, "package.json"));
    if (manifest != null) return manifest;
    directory = parent;
    parent = path.dirname(directory);
  }
  return readManifest(path.join(directory, "package.json"));
}

/** An import map, as the page's `<script type="importmap">`. */
export function importMap(packages: $ReadOnlyArray<Package>): string {
  const imports: { [string]: string } = {};
  const scopes: { [string]: { [string]: string } } = {};

  for (const entry of packages) {
    const name = typeof entry.manifest.name === "string" ? entry.manifest.name : null;
    if (name == null) continue;
    for (const [subpath, target] of entryPoints(entry)) {
      imports[subpath === "." ? name : `${name}/${subpath.slice(2)}`] = target;
    }
    // The `browser` field, as a scope. It is the importing *package's*
    // substitution — `@uniflowed/test` and `@uniflowed/host` map `node:module`
    // to two different files, and both are right — so it cannot be a global
    // entry, and a scope keyed on the package's own directory is exactly the
    // shape of "when this package asks".
    const substitutions = browserField(entry);
    if (Object.keys(substitutions).length > 0) {
      scopes[`${moduleUrl(entry.directory)}/`] = substitutions;
    }
  }
  // A `node:` specifier written in a *test file* is deliberately not in here.
  // Only a package that declared a `browser` substitution gets one, in its own
  // scope; a test that imports `node:fs` and runs in a page should say so, and
  // it does — the browser refuses the specifier by name, at the import.
  return JSON.stringify({ imports, scopes }, null, 2);
}

/** The `subpath → URL` pairs one package publishes. */
function entryPoints(entry: Package): Array<[string, string]> {
  const found: Array<[string, string]> = [];
  const exported = entry.manifest.exports;
  if (typeof exported === "string") {
    found.push([".", join(entry, exported)]);
  } else if (exported != null && typeof exported === "object" && !Array.isArray(exported)) {
    const keys = Object.keys(exported);
    // `{"import": …, "default": …}` with no `.` key is a conditions object for
    // the package root rather than a subpath map, which is how React's own
    // manifest is written.
    if (!keys.some((key) => key === "." || key.startsWith("./"))) {
      const target = condition(exported);
      if (target != null) found.push([".", join(entry, target)]);
    } else {
      for (const key of keys) {
        if (key !== "." && !key.startsWith("./")) continue;
        // `./internal/*.js` cannot be an import-map key, but the prefix it
        // stands for can: a trailing-slash key maps a whole directory, which
        // covers every file the pattern would have matched.
        const wildcard = key.indexOf("*");
        const target = condition(exported[key]);
        if (target == null) continue;
        if (wildcard === -1) {
          found.push([key, join(entry, target)]);
          continue;
        }
        const targetWildcard = target.indexOf("*");
        if (targetWildcard === -1) continue;
        found.push([key.slice(0, wildcard), join(entry, target.slice(0, targetWildcard))]);
      }
    }
  }
  if (found.length === 0) {
    const main = entry.manifest.module ?? entry.manifest.main ?? "./index.js";
    found.push([".", join(entry, main)]);
  }
  // `package.json` is importable from several of uf's packages and is never in
  // an `exports` map's wildcards.
  found.push(["./package.json", join(entry, "./package.json")]);
  return found;
}

/** The first condition this page honours, following nested objects. */
function condition(target: mixed): string | null {
  if (typeof target === "string") return target;
  if (target == null || typeof target !== "object" || Array.isArray(target)) return null;
  for (const name of CONDITIONS) {
    if (name in target) {
      const chosen = condition(target[name]);
      if (chosen != null) return chosen;
    }
  }
  return null;
}

/**
 * A package-relative target, as a URL.
 *
 * The trailing slash is carried through by hand, because `path.join` removes
 * one and an import map is strict about it: a key ending in `/` may only map to
 * a value ending in `/`, and a pair that breaks that rule is dropped by the
 * browser with a console warning nobody is reading. That pair is how a
 * wildcard subpath — `"./internal/*.js"` — reaches the page at all.
 */
function join(entry: Package, target: string): string {
  const cleaned = target.startsWith("./") ? target.slice(2) : target;
  const absolute = path.join(entry.directory, cleaned);
  const slash = target.endsWith("/") && !absolute.endsWith(path.sep) ? "/" : "";
  // Not `moduleUrl`, which stats: an `exports` map may name a file a published
  // package has and this checkout does not, and a missing entry point should
  // fail when something imports it rather than stop the map being built.
  return `${FILE_PREFIX}${absolute.split(path.sep).map(encodeURIComponent).join("/")}${slash}`;
}

/** A package's `browser` substitutions, as an import map's scope body. */
function browserField(entry: Package): { [string]: string } {
  const field = entry.manifest.browser;
  const substitutions: { [string]: string } = {};
  if (field == null || typeof field !== "object" || Array.isArray(field)) {
    return substitutions;
  }
  for (const key of Object.keys(field)) {
    const target = field[key];
    // `false` means "replace with an empty module", which this does not do:
    // an empty module is a silence, and silence is what the whole `browser`
    // shim in this package exists not to be. Nothing uf ships uses it.
    if (typeof target !== "string") continue;
    substitutions[key] = target.startsWith(".") ? join(entry, target) : target;
  }
  return substitutions;
}
