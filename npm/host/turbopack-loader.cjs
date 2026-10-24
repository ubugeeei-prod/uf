// @noflow
//
// `@uniflowed/host/turbopack-loader.cjs`: the webpack loader Turbopack runs for
// a Flow module, and the one webpack runs when `next build --webpack` does.
//
// Turbopack loads `turbopack.rules` with `require`, in its own workers, so
// this is CommonJS for the same reason `turbopack.cjs` is. The export is the
// loader function webpack calls. `compile` is the same transformation without
// a loader context, which is what the tests call.

"use strict";

const fs = require("node:fs");
const path = require("node:path");

const {
  FLOW_PRAGMA,
  NOFLOW_PRAGMA,
  findUfBinary,
  loadHost,
  styleModuleFor,
} = require("./turbopack.cjs");

/** React Compiler findings in dependencies, printed only when this is `all`. */
const ALL_DIAGNOSTICS = "UF_REACT_COMPILER_DIAGNOSTICS";

const JS_EXTENSION = /\.(?:js|jsx|mjs|cjs)$/;

/**
 * webpack loader entry. Turbopack runs this through loader-runner.
 *
 * @this {{
 *   async: () => (error: ?Error, code?: string, map?: mixed) => void,
 *   cacheable?: (cacheable: boolean) => void,
 *   mode?: string,
 *   resourcePath: string,
 *   rootContext?: string,
 *   query?: string | { [key: string]: mixed },
 *   getOptions?: () => { [key: string]: mixed },
 *   emitWarning?: (error: Error) => void,
 *   addDependency?: (file: string) => void,
 * }}
 * @param {string | Buffer} source
 * @param {mixed} inputSourceMap
 */
function loader(source, inputSourceMap) {
  const callback = this.async();
  if (typeof callback !== "function") {
    throw new Error(
      "@uniflowed/host/turbopack: the loader was called without async support.",
    );
  }
  if (typeof this.cacheable === "function") this.cacheable(true);
  const options = readOptions(this);
  const root =
    typeof options.root === "string" && options.root !== ""
      ? options.root
      : this.rootContext || process.cwd();
  compile(String(source), this.resourcePath, {
    root,
    binary      : typeof options.binary === "string" ? options.binary : "",
    development : this.mode === "development",
    emitWarning : (error) => {
      if (typeof this.emitWarning === "function") this.emitWarning(error);
    },
    addDependency: (file) => {
      if (typeof this.addDependency === "function") this.addDependency(file);
    },
  }).then(
    (result) => {
      if (result == null) callback(null, String(source), inputSourceMap);
      else callback(null, result.code, result.map ?? inputSourceMap);
    },
    (error) => callback(error instanceof Error ? error : new Error(String(error))),
  );
}

/**
 * Compile one module, or return null when it is not Flow.
 *
 * `refresh` is off. Next.js registers components for Fast Refresh itself, and
 * a component registered twice is two families to the runtime. `nativeStyles`
 * is off because this loader serves a browser, and the native StyleX subset
 * refuses the CSS a web component is allowed to write.
 *
 * @param {string} source
 * @param {string} filename absolute path, possibly with a query
 * @param {{
 *   root?: string,
 *   binary?: string,
 *   development?: boolean,
 *   emitWarning?: (error: Error) => void,
 *   addDependency?: (file: string) => void,
 * }} [options]
 * @returns {Promise<{ code: string, map: mixed, cssPath: string | null } | null>}
 */
async function compile(source, filename, options = {}) {
  const file = cleanPath(filename);
  if (!shouldTransform(source, file)) return null;

  const flow = loadHost();
  // The config resolved this path. Prefer it over whatever `PATH` the worker
  // happened to inherit, then let the usual lookup fill in a test that called
  // `compile` directly.
  if (typeof options.binary === "string" && options.binary !== "") {
    process.env.UF_BINARY = options.binary;
  }
  findUfBinary(flow);
  const root = options.root || process.env.UF_PROJECT_ROOT || process.cwd();
  const previousRoot = process.env.UF_PROJECT_ROOT;
  process.env.UF_PROJECT_ROOT = root;
  let out;
  try {
    out = await flow.transformFlow(source, file, {
      root,
      development : options.development === true,
      refresh     : false,
      sourceMap   : true,
      nativeStyles: false,
      inSourceTests: false,
    });
  } catch (error) {
    throw locatedError(error, file);
  } finally {
    if (previousRoot == null) delete process.env.UF_PROJECT_ROOT;
    else process.env.UF_PROJECT_ROOT = previousRoot;
  }

  if (out == null) {
    throw new Error(
      `@uniflowed/host/turbopack: ${file} matched a Flow rule but uf did not transform it.`,
    );
  }

  reportDiagnostics(out.diagnostics, file, root, options.emitWarning);
  trace(file);

  const stylesheet = styleModuleFor(file, root);
  const css = typeof out.css === "string" ? out.css : "";
  if (css.trim() === "") {
    removeIfExists(stylesheet.file);
    return { code: out.code, map: normalizeMap(out.map), cssPath: null };
  }

  // The sheet is plain CSS. `*.uf.css` is registered as global CSS, so the
  // class StyleX already wrote is the class the browser matches.
  writeIfChanged(stylesheet.file, css);
  if (typeof options.addDependency === "function") options.addDependency(stylesheet.file);
  const inserted = insertAfterDirectivePrologue(
    out.code,
    `import ${JSON.stringify(stylesheet.specifier)};`,
  );
  return {
    code   : inserted.code,
    map    : shiftSourceMap(normalizeMap(out.map), inserted.line, inserted.addedLines),
    cssPath: stylesheet.file,
  };
}

/**
 * Whether this loader should compile `source`.
 *
 * The Turbopack rule already requires a pragma. webpack's `test` does not, so
 * the loader repeats the check and leaves a plain module for SWC. `@noflow`
 * wins over a later `@flow`: host and Vite are plain JavaScript that mention
 * the word in comments.
 */
function shouldTransform(source, filename) {
  if (!JS_EXTENSION.test(filename)) return false;
  if (NOFLOW_PRAGMA.test(source)) return false;
  return FLOW_PRAGMA.test(source);
}

function writeIfChanged(file, contents) {
  let current = null;
  try {
    current = fs.readFileSync(file, "utf8");
  } catch {
    // The first build of this module. Writing below creates it.
  }
  if (current === contents) return;
  try {
    fs.mkdirSync(path.dirname(file), { recursive: true });
    fs.writeFileSync(file, contents);
  } catch (error) {
    throw new Error(
      `@uniflowed/host/turbopack: could not write ${file}: ${error instanceof Error ? error.message : String(error)}`,
    );
  }
}

function removeIfExists(file) {
  try {
    fs.unlinkSync(file);
  } catch (error) {
    if (error?.code !== "ENOENT") {
      throw new Error(
        `@uniflowed/host/turbopack: could not remove ${file}: ${error instanceof Error ? error.message : String(error)}`,
      );
    }
  }
}

function reportDiagnostics(diagnostics, filename, root, emitWarning) {
  if (!Array.isArray(diagnostics) || diagnostics.length === 0) return;
  const warn = typeof emitWarning === "function" ? emitWarning : () => {};
  const project = isProjectFile(root, filename);
  const all = process.env[ALL_DIAGNOSTICS] === "all";
  if (!project && !all) {
    noteSuppressed(warn);
    return;
  }
  for (const diagnostic of diagnostics) {
    if (diagnostic == null || typeof diagnostic !== "object") continue;
    const at =
      diagnostic.line == null ? "" : `:${diagnostic.line}:${(diagnostic.column ?? 0) + 1}`;
    const who = diagnostic.function == null ? "" : ` (in ${diagnostic.function})`;
    const relative = path.relative(root, filename) || filename;
    warn(new Error(`${relative}${at}: ${diagnostic.message ?? ""}${who}`));
  }
}

let suppressedNoted = false;

function noteSuppressed(warn) {
  if (suppressedNoted) return;
  suppressedNoted = true;
  warn(
    new Error(
      "React Compiler findings in dependencies were omitted. " +
        `Set ${ALL_DIAGNOSTICS}=all to see them.`,
    ),
  );
}

function isProjectFile(root, filename) {
  const relative = path.relative(root, filename);
  if (relative === "" || relative.startsWith("..") || path.isAbsolute(relative)) return false;
  return !relative.split(path.sep).includes("node_modules");
}

function locatedError(error, filename) {
  if (error == null || error.name !== "TransformError" || error.loc == null) return error;
  const line = error.loc.line;
  const column = (error.loc.column ?? 0) + 1;
  const failure = new Error(`${filename}:${line}:${column}: ${error.message}`);
  failure.name = "TransformError";
  failure.loc = { file: filename, line, column: error.loc.column ?? 0 };
  return failure;
}

function trace(filename) {
  const file = process.env.UF_TURBOPACK_TRACE;
  if (file == null || file === "") return;
  fs.appendFileSync(file, `${filename}\n`);
}

function readOptions(context) {
  if (context != null && typeof context.getOptions === "function") {
    try {
      const options = context.getOptions();
      if (options != null && typeof options === "object") return options;
    } catch {
      // loader-runner without loader-utils. `query` still carries the object.
    }
  }
  const query = context?.query;
  if (query != null && typeof query === "object") return query;
  if (typeof query === "string" && query.startsWith("?")) {
    try {
      const parsed = JSON.parse(query.slice(1));
      if (parsed != null && typeof parsed === "object") return parsed;
    } catch {
      return {};
    }
  }
  return {};
}

function normalizeMap(map) {
  if (map == null || map === "") return null;
  if (typeof map === "string") {
    try {
      const parsed = JSON.parse(map);
      return parsed != null && typeof parsed === "object" ? parsed : null;
    } catch {
      return null;
    }
  }
  return typeof map === "object" ? map : null;
}

function cleanPath(filename) {
  const query = filename.search(/[?#]/);
  return query === -1 ? filename : filename.slice(0, query);
}

// The directive prologue, copied from `npm/vite/internal/directive-prologue.js`.
//
// `@uniflowed/host` cannot import `@uniflowed/vite`: Vite already depends on
// host, and a cycle would make the package unusable. The two copies have to
// agree, and `turbopack.test.js` is what says so for `"use client"`.

/**
 * Where a following statement has to be inserted so the directive prologue
 * stays a prologue.
 *
 * @param {string} source
 * @returns {number}
 */
function directivePrologueEnd(source) {
  let at = source.charCodeAt(0) === 0xfeff ? 1 : 0;
  const length = source.length;
  if (source.startsWith("#!", at)) {
    const end = source.indexOf("\n", at);
    at = end === -1 ? length : end + 1;
  }
  for (;;) {
    at = skipGap(source, at);
    const start = at;
    const quote = source[at];
    if (quote !== '"' && quote !== "'") return start;
    const close = source.indexOf(quote, at + 1);
    if (close === -1) return start;
    const value = source.slice(at + 1, close);
    if (value.includes("\n") || value.includes("\\")) return start;
    at = close + 1;
    while (at < length && (source[at] === " " || source[at] === "\t")) at++;
    if (source[at] === ";") {
      at++;
      continue;
    }
    if (
      at >= length ||
      source[at] === "\n" ||
      source[at] === "\r" ||
      source.startsWith("//", at) ||
      source.startsWith("/*", at)
    ) {
      continue;
    }
    return start;
  }
}

/**
 * Insert `statement` immediately after the directive prologue.
 *
 * `line` is the zero-based line the statement occupies, and `addedLines` is
 * how many source-map lines have to be skipped there.
 *
 * @param {string} code
 * @param {string} statement
 * @returns {{ code: string, line: number, addedLines: number }}
 */
function insertAfterDirectivePrologue(code, statement) {
  const at = directivePrologueEnd(code);
  const piece = statement.endsWith("\n") ? statement : `${statement}\n`;
  return {
    code      : code.slice(0, at) + piece + code.slice(at),
    line      : countNewlines(code, at),
    addedLines: countNewlines(piece, piece.length),
  };
}

/**
 * Record `addedLines` empty lines in `map`, starting at zero-based `line`.
 *
 * @param {string | { mappings?: string } | null | undefined} map
 * @param {number} line
 * @param {number} addedLines
 */
function shiftSourceMap(map, line, addedLines) {
  if (map == null || addedLines === 0) return map;
  const next = typeof map === "string" ? JSON.parse(map) : { ...map };
  if (typeof next.mappings !== "string") return next;
  const lines = next.mappings.split(";");
  while (lines.length < line) lines.push("");
  lines.splice(line, 0, ...Array.from({ length: addedLines }, () => ""));
  next.mappings = lines.join(";");
  return next;
}

function skipGap(source, at) {
  const length = source.length;
  for (;;) {
    while (at < length && isSpace(source[at])) at++;
    if (source.startsWith("//", at)) {
      const end = source.indexOf("\n", at);
      at = end === -1 ? length : end + 1;
      continue;
    }
    if (source.startsWith("/*", at)) {
      const end = source.indexOf("*/", at + 2);
      if (end === -1) return length;
      at = end + 2;
      continue;
    }
    return at;
  }
}

function isSpace(character) {
  return (
    character === " " ||
    character === "\t" ||
    character === "\n" ||
    character === "\r" ||
    character === "\f" ||
    character === "\v"
  );
}

function countNewlines(source, until) {
  let count = 0;
  for (let index = 0; index < until; index++) {
    if (source.charCodeAt(index) === 10) count++;
  }
  return count;
}

module.exports = loader;
module.exports.compile = compile;
module.exports.insertAfterDirectivePrologue = insertAfterDirectivePrologue;
module.exports.shiftSourceMap = shiftSourceMap;
module.exports.shouldTransform = shouldTransform;
module.exports.styleModuleFor = styleModuleFor;
