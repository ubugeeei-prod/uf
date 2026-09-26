// @flow
//
// The `node:` builtins `@uniflowed/test` reaches for, as a browser can have
// them.
//
// # Why a package the browser never runs needs a browser build
//
// A module with an in-source test block imports `@uniflowed/test` at its top
// level, and that block is compiled away by `uf build` — `import.meta.uf.test`
// becomes `void 0`, the bundler drops the branch, and the import goes with it
// because the package declares `sideEffects: false`. But a bundler *resolves*
// a graph before it shakes it, so the six builtins this package's edges import
// were resolved on the way to being discarded: `node:fs` and `node:path` for
// snapshot files, `node:module` and `node:url` for module mocking,
// `node:async_hooks` and `node:util` for output capture. Each one printed
//
//     Module "node:fs" has been externalized for browser compatibility
//
// so a project with one three-line in-source test got six warnings about
// modules `uf build` was in the middle of removing.
//
// `package.json`'s `browser` field maps all six here. Node ignores that field
// and every bundler honours it, so nothing about running a test on a host
// changes and the build is quiet.
//
// One file rather than six of four lines, because the question a reader has is
// "what does a browser not have", and that is answered better in one place
// than in six.
//
// # Why the shims work rather than only resolve
//
// Nothing here is reached by the build that made it necessary: the module that
// imports it is removed. What makes the implementations below worth writing is
// the case where the package *is* evaluated in a browser — a runner that ran a
// test file in a real page, which `docs/roadmap.md` still lists as not done, or
// an application that genuinely imports `@uniflowed/test` into client code.
// A file that exists only to be resolved would be a file that fails the moment
// somebody's build stops shaking it out, and that failure would arrive as a
// missing export rather than as a sentence.
//
// # Two kinds of shim, and the difference is the point
//
// Some of these are **implementations**: `format`, `inspect`, `pathToFileURL`
// and the path helpers do in a browser what they do in Node, closely enough
// for the one caller each has. `AsyncLocalStorage` is an implementation with a
// named limitation.
//
// The rest are **refusals**: the filesystem is not something a page has, and
// `createRequire` cannot invent a synchronous module loader. Each one throws a
// sentence naming what is not available and what to do instead. A shim that
// answered plausibly — an `existsSync` that said `false`, say — would be worse
// than no browser mode at all: `toMatchSnapshot` would read "no snapshot yet",
// record one it could not write, and pass. That is the runner lying, which is
// the failure `uf test` exists not to have.
//
// Each refusal names what is not available and what to do instead, so a
// boundary is a sentence rather than a surprise in a stack trace.

/** How a refusal is worded, so all of them read the same way. */
function unavailable(what: string, instead: string): Error {
  return new Error(
    `${what} is not available in \`uf test --browser\`: a page has no ${what}. ${instead}`,
  );
}

// ---------------------------------------------------------------------- //
// node:fs — refused
// ---------------------------------------------------------------------- //

/** Snapshot files live on a disk the page cannot see. */
export function existsSync(_path: mixed): empty {
  throw unavailable(
    "the filesystem",
    "Snapshots (`toMatchSnapshot`) are written by the host that runs the file; assert with `toMatchInlineSnapshot`, which needs no file, or run the file on Node.",
  );
}

/** @see existsSync */
export function readFileSync(_path: mixed, _encoding?: mixed): empty {
  return existsSync(_path);
}

/** @see existsSync */
export function writeFileSync(_path: mixed, _contents: mixed): empty {
  return existsSync(_path);
}

/** @see existsSync */
export function mkdirSync(_path: mixed, _options?: mixed): empty {
  return existsSync(_path);
}

// ---------------------------------------------------------------------- //
// node:module — refused, with one door left open
// ---------------------------------------------------------------------- //

/**
 * Modules the page has already imported, by the specifier they answer to.
 *
 * The one thing `createRequire` is used for that a browser *can* do. It is
 * `@uniflowed/react-testing`'s `render`, which loads `react-dom/client` through
 * `createRequire` rather than an import because `render` is synchronous and an
 * import is not — a decision that is right on Node and impossible here.
 *
 * So the page's entry module imports `react-dom/client` itself, before any
 * test file runs, and puts it here. `render` then finds it synchronously, and
 * the import that made that possible happened at a moment when being
 * asynchronous cost nothing.
 */
const provided: Map<string, mixed> = new Map();

/**
 * Register a module the page has already imported.
 *
 * Called by whatever brought this package into a page, never by a test.
 */
export function provideModule(specifier: string, module: mixed): void {
  provided.set(specifier, module);
}

/**
 * A `require` that answers for what the page brought with it and refuses the
 * rest.
 *
 * Refusing loudly rather than returning `undefined`: a caller that gets
 * `undefined` back fails later, somewhere else, with a message about a
 * property of `undefined`.
 */
export function createRequire(_from: mixed): (specifier: string) => mixed {
  return (specifier: string) => {
    if (provided.has(specifier)) return provided.get(specifier);
    throw unavailable(
      `a synchronous \`require("${specifier}")\``,
      "Import it from the test file instead; a browser resolves modules asynchronously and cannot be asked to do it in the middle of a call.",
    );
  };
}

// ---------------------------------------------------------------------- //
// node:url — implemented
// ---------------------------------------------------------------------- //

/** A path as the `file:` URL Node would produce. */
export function pathToFileURL(filePath: string): URL {
  const withSlashes = filePath.replace(/\\/g, "/");
  const absolute = withSlashes.startsWith("/") ? withSlashes : `/${withSlashes}`;
  return new URL(`file://${absolute.split("/").map(encodeURIComponent).join("/")}`);
}

/** The path inside a `file:` URL. */
export function fileURLToPath(url: string | URL): string {
  const href = typeof url === "string" ? url : url.href;
  if (!href.startsWith("file://")) {
    throw new TypeError(`not a file URL: ${href}`);
  }
  return decodeURIComponent(href.slice("file://".length));
}

// ---------------------------------------------------------------------- //
// node:util — implemented
// ---------------------------------------------------------------------- //

/**
 * `util.inspect`, to the depth a test's `console.log` needs.
 *
 * Not Node's algorithm — no getters, no circular references marked, no colour.
 * What it has to be is *stable* and *readable*, because its output is what a
 * failing test's printing looks like in the terminal, and `JSON.stringify`
 * with a replacer that names functions and symbols is both.
 */
export function inspect(value: mixed): string {
  if (typeof value === "string") return `'${value}'`;
  return renderValue(value, new Set());
}

function renderValue(value: mixed, seen: Set<mixed>): string {
  if (value === null) return "null";
  if (value === undefined) return "undefined";
  if (typeof value === "string") return `'${value}'`;
  if (typeof value === "bigint") return `${String(value)}n`;
  if (typeof value === "number" || typeof value === "boolean") return String(value);
  if (typeof value === "symbol") return String(value);
  if (typeof value === "function") {
    const name = (value as $FlowFixMe).name;
    return name === "" ? "[Function (anonymous)]" : `[Function: ${String(name)}]`;
  }
  if (value instanceof Error) return `${value.name}: ${value.message}`;
  if (typeof value !== "object") return String(value);
  if (seen.has(value)) return "[Circular]";
  seen.add(value);
  try {
    if (Array.isArray(value)) {
      return `[ ${value.map((item: mixed) => renderValue(item, seen)).join(", ")} ]`;
    }
    const entries = Object.entries(value).map(
      ([key, item]) => `${key}: ${renderValue(item, seen)}`,
    );
    return entries.length === 0 ? "{}" : `{ ${entries.join(", ")} }`;
  } finally {
    seen.delete(value);
  }
}

/**
 * `util.format`, with the `%s`/`%d`/`%i`/`%j`/`%o`/`%O`/`%%` substitutions
 * `console.log` uses.
 *
 * Everything left over is appended, space separated, exactly as Node does —
 * which is the behaviour `console.log("a", 1, { b: 2 })` depends on.
 */
export function format(first: mixed, ...rest: $ReadOnlyArray<mixed>): string {
  const args = [...rest];
  let out = "";
  if (typeof first === "string") {
    let at = 0;
    while (at < first.length) {
      const ch = first[at];
      if (ch !== "%" || at + 1 >= first.length) {
        out += ch;
        at += 1;
        continue;
      }
      const kind = first[at + 1];
      if (kind === "%") {
        out += "%";
        at += 2;
        continue;
      }
      if (!"sdifjoO".includes(kind) || args.length === 0) {
        out += ch;
        at += 1;
        continue;
      }
      const value = args.shift();
      if (kind === "s") out += typeof value === "string" ? value : renderValue(value, new Set());
      else if (kind === "d" || kind === "f") out += String(Number(value));
      else if (kind === "i") out += String(Math.trunc(Number(value)));
      else if (kind === "j") out += safeJson(value);
      else out += renderValue(value, new Set());
      at += 2;
    }
  } else {
    out = renderValue(first, new Set());
  }
  for (const value of args) {
    out += ` ${typeof value === "string" ? value : renderValue(value, new Set())}`;
  }
  return out;
}

function safeJson(value: mixed): string {
  try {
    return JSON.stringify(value) ?? "undefined";
  } catch {
    return "[Circular]";
  }
}

// ---------------------------------------------------------------------- //
// node:async_hooks — implemented, with a limitation that is written down
// ---------------------------------------------------------------------- //

/**
 * A single-value stand-in for Node's asynchronous storage.
 *
 * The real one follows a value through every continuation of the work that
 * entered it; this one holds whichever `run` is on the stack. A browser has no
 * asynchronous context tracking to build the real thing on — the platform
 * proposal for it is not shipped anywhere — and the alternative to an
 * approximation is that `@uniflowed/test`'s output capture cannot be imported
 * at all.
 *
 * What it costs is bounded by what the store is used for here.
 * `internal/output.js` reads it to attribute a `console.log` to the case that
 * printed it, and cases run one at a time, so the answer is right for
 * everything a case prints while it is running. It is wrong for a `setTimeout`
 * a finished case left behind: the print is still reported, attributed to the
 * file rather than to the case. On Node that same straggler is attributed to
 * its case; here it is attributed one level up.
 *
 * That is the whole of the difference, and it is a difference in a label on a
 * line of output — not in what passes.
 */
export class AsyncLocalStorage<T> {
  #store: T | void = undefined;

  /** Run `body` with `value` readable from `getStore`. */
  run<R>(value: T, body: () => R): R {
    const previous = this.#store;
    this.#store = value;
    try {
      return body();
    } finally {
      this.#store = previous;
    }
  }

  /** The value of the nearest enclosing `run`, or `undefined`. */
  getStore(): T | void {
    return this.#store;
  }

  /** Run `body` with no value readable. */
  exit<R>(body: () => R): R {
    const previous = this.#store;
    this.#store = undefined;
    try {
      return body();
    } finally {
      this.#store = previous;
    }
  }
}

// ---------------------------------------------------------------------- //
// node:path — implemented, posix only
// ---------------------------------------------------------------------- //

/**
 * The path operations `internal/snapshot.js` performs on a snapshot path.
 *
 * Posix only, which is not a limitation here: every path a page sees came from
 * a URL, and a URL's separator is `/` on every platform.
 */
export const path: {
  readonly join: (...parts: $ReadOnlyArray<string>) => string,
  readonly dirname: (of: string) => string,
  readonly basename: (of: string, extension?: string) => string,
  readonly extname: (of: string) => string,
  readonly resolve: (...parts: $ReadOnlyArray<string>) => string,
  readonly sep: string,
} = {
  sep: "/",
  join: (...parts) => normalize(parts.filter((part) => part !== "").join("/")),
  dirname: (of) => {
    const at = of.lastIndexOf("/");
    if (at === -1) return ".";
    return at === 0 ? "/" : of.slice(0, at);
  },
  basename: (of, extension) => {
    const name = of.slice(of.lastIndexOf("/") + 1);
    return extension != null && name.endsWith(extension)
      ? name.slice(0, name.length - extension.length)
      : name;
  },
  extname: (of) => {
    const name = of.slice(of.lastIndexOf("/") + 1);
    const at = name.lastIndexOf(".");
    return at <= 0 ? "" : name.slice(at);
  },
  resolve: (...parts) => {
    let out = "";
    for (const part of parts) {
      if (part.startsWith("/")) out = part;
      else if (out === "") out = part;
      else out = `${out}/${part}`;
    }
    return normalize(out.startsWith("/") ? out : `/${out}`);
  },
};

/** `a//b/./c/../d` as `a/b/d`, keeping a leading slash. */
function normalize(input: string): string {
  const absolute = input.startsWith("/");
  const out: Array<string> = [];
  for (const part of input.split("/")) {
    if (part === "" || part === ".") continue;
    if (part === ".." && out.length > 0 && out[out.length - 1] !== "..") out.pop();
    else out.push(part);
  }
  const joined = out.join("/");
  return absolute ? `/${joined}` : joined === "" ? "." : joined;
}

export default path;
