// @noflow
//
// `@uniflowed/host/turbopack`: compile Flow inside a Next.js app.
//
// # Why this module is CommonJS rather than Flow
//
// Next.js evaluates `next.config.js` in plain Node. Turbopack then loads a
// webpack loader with `require`, in its own workers. Neither process has uf's
// loader hooks. Until a module is CommonJS, the one place the hooks are
// installed — `uf test` — is the only place the integration loads, which is
// the defect `@uniflowed/react-native/metro.cjs` is CommonJS to avoid.
//
// An ES module config can still `import { withUniflowed } from
// "@uniflowed/host/turbopack"`: Node reads a CommonJS module's named exports.
// `require` of `@uniflowed/host/transform`, which is ESM, needs Node 20.19 or
// 22.12.
//
// # What this is not
//
// This is not `build.builder`. `uf dev` and `uf build` stay on `@uniflowed/vite`.
// Turbopack does not implement uf's builder contract — routes, RSC, the dev
// server, assets, HMR — and pretending a loader is that builder would make
// `uf dev` look like it ran on Turbopack when Vite was still doing the work.
// The integration is the other direction: a Next.js app that stays on Next.js
// can compile its Flow.

"use strict";

const crypto = require("node:crypto");
const fs = require("node:fs");
const os = require("node:os");
const path = require("node:path");

/** The loader Turbopack and webpack both run. A path, because that is what they store. */
const turbopackLoaderPath = path.join(__dirname, "turbopack-loader.cjs");

/**
 * Extension globs, not a `*` glob.
 *
 * Turbopack runs a webpack loader on a file it already knows how to parse
 * (`.js`, `.jsx`, `.ts`, `.tsx`) when the rule's key is an extension glob.
 * A path glob such as `app/**` or a bare `*` does not: those files keep the
 * built-in parser and the loader never runs, so a Flow `component` reaches
 * SWC and the build fails on the syntax the loader was supposed to remove.
 */
const FLOW_EXTENSIONS = Object.freeze(["*.js", "*.jsx", "*.mjs", "*.cjs"]);

/**
 * A Flow pragma at the start of a line.
 *
 * `@noflow` does not match: the letters before `flow` are not `@`. A mention
 * in the middle of a line does not match either, so a comment that talks about
 * the pragma is not a request to compile the file. The loader checks the same
 * expression again, because webpack has no content condition and a `.jsx` file
 * returned unchanged would then be parsed as JavaScript.
 */
const FLOW_PRAGMA = /(?:^|\n)[ \t]*(?:\/\/|\/\*)[ \t]*@flow\b/;

/** `@noflow` the same way. Host and Vite ship it, and they are plain JavaScript. */
const NOFLOW_PRAGMA = /(?:^|\n)[ \t]*(?:\/\/|\/\*)[ \t]*@noflow\b/;

/** `@uniflowed/*` inside a `node_modules` directory. */
const UNIFLOWED_PACKAGE = /(?:^|[\\/])node_modules[\\/]@uniflowed[\\/]/;

/**
 * The loader rules a Turbopack that understands `turbopack.rules` can register.
 *
 * Two rules, on purpose. A project file is not foreign, and a dependency is.
 * `foreign` skips `node_modules` so the content condition is not applied to
 * every package. `@uniflowed/*` is the exception `isFlowModule` already makes:
 * those packages ship Flow, so the second rule opts back in, and only for a
 * file that carries a real `@flow` pragma and not `@noflow`.
 *
 * `as: "*.js"` matters for `.jsx`. The loader lowers JSX. A file the rule
 * matched and the loader then returned untouched would be parsed as
 * JavaScript and fail on the JSX SWC would have accepted.
 *
 * @param {{ identity?: string, root?: string }} [options] plain JSON, which is all Turbopack will pass to a loader
 * @returns {{ [glob: string]: Array<object> }}
 */
function flowRules(options = {}) {
  const rules = {};
  for (const extension of FLOW_EXTENSIONS) {
    // A fresh array, and fresh rule objects, per glob. One shared array would
    // make a later edit of `*.js` silently edit `*.jsx` too.
    const loader = { loader: turbopackLoaderPath, options: plainOptions(options) };
    rules[extension] = [
      {
        condition: {
          all: [
            { not: "foreign" },
            // `transpilePackages` makes an installed `@uniflowed/*` package
            // not foreign. Without this, that file matches both rules, and
            // Turbopack runs every match.
            { not: { path: UNIFLOWED_PACKAGE } },
            { content: FLOW_PRAGMA },
          ],
        },
        loaders: [{ ...loader, options: { ...loader.options } }],
        as      : "*.js",
      },
      {
        condition: {
          all: [
            { path: UNIFLOWED_PACKAGE },
            { not: { content: NOFLOW_PRAGMA } },
            { content: FLOW_PRAGMA },
          ],
        },
        loaders: [{ ...loader, options: { ...loader.options } }],
        as      : "*.js",
      },
    ];
  }
  return rules;
}

function plainOptions(options) {
  return {
    identity: typeof options.identity === "string" ? options.identity : "",
    root    : typeof options.root === "string" ? options.root : "",
    // The resolved `uf`. Loader workers do not inherit a `UF_BINARY` this
    // process set after they were spawned, and the cache key alone does not
    // tell the worker which file to execute.
    binary  : typeof options.binary === "string" ? options.binary : "",
  };
}

/**
 * Compile this Next.js config's Flow with uf.
 *
 * Pass the config the project already has. An object comes back as an object.
 * A function — `(phase, context) => config`, or an async one — comes back as a
 * function that wraps whatever it returns. Composing twice is composing once.
 *
 * The working directory is the project root Next.js was started in. Tests pass
 * a root of their own; a `next.config.js` does not need to.
 *
 * @param {object | ((...args: Array<mixed>) => mixed)} [nextConfig]
 * @param {string} [root]
 * @returns {object | ((...args: Array<mixed>) => mixed)}
 */
function withUniflowed(nextConfig = {}, root) {
  const projectRoot = typeof root === "string" && root !== "" ? root : process.cwd();
  if (typeof nextConfig === "function") {
    return function uniflowedNextConfig(...args) {
      const result = nextConfig(...args);
      if (result != null && typeof result.then === "function") {
        return result.then((config) => applyUniflowed(config ?? {}, projectRoot));
      }
      return applyUniflowed(result ?? {}, projectRoot);
    };
  }
  return applyUniflowed(nextConfig ?? {}, projectRoot);
}

function applyUniflowed(config, projectRoot) {
  if (config == null || typeof config !== "object" || Array.isArray(config)) {
    throw new Error(
      `@uniflowed/host/turbopack: withUniflowed() expected a Next.js config object or function, ` +
        `and received ${config == null ? String(config) : typeof config}.`,
    );
  }
  if (
    rulesContainLoader(config.turbopack?.rules) ||
    rulesContainLoader(config.experimental?.turbo?.rules)
  ) {
    return config;
  }

  const flow = loadHost();
  const compiler = pinCompiler(flow);
  const options = {
    identity: compiler.identity,
    root    : projectRoot,
    binary  : compiler.binary,
  };
  const rules = flowRules(options);

  if (config.transpilePackages != null && !Array.isArray(config.transpilePackages)) {
    throw new Error(
      `@uniflowed/host/turbopack: transpilePackages is ${JSON.stringify(config.transpilePackages)}, ` +
        "and an array of package names was expected.",
    );
  }

  const turbopack =
    config.turbopack != null && typeof config.turbopack === "object" ? config.turbopack : {};
  const next = {
    ...config,
    transpilePackages: mergePackages(
      config.transpilePackages,
      flow.uniflowedPackages(projectRoot),
    ),
    turbopack: {
      ...turbopack,
      rules: mergeRules(turbopack.rules, rules),
    },
  };
  // A `webpack: undefined` own property is still "webpack is set" to a check
  // that uses `in` or `hasOwn`. Next.js 16 rejects a Turbopack build when
  // that happens. Omit the key unless there is a function to install, or a
  // value the project already set.
  const webpack = webpackField(config.webpack, options);
  if (webpack !== undefined) next.webpack = webpack;
  else delete next.webpack;

  // The old key, from Next.js 13 through 15.2. Creating it when the config
  // never had it would resurrect an option Next has since renamed. A config
  // that still sets it keeps the rules there too, or the upgrade codemod is
  // the only thing standing between that project and uncompiled Flow.
  if (config.experimental?.turbo != null && typeof config.experimental.turbo === "object") {
    next.experimental = {
      ...config.experimental,
      turbo: {
        ...config.experimental.turbo,
        rules: mergeRules(config.experimental.turbo.rules, rules),
      },
    };
  }

  if (process.env.UF_PROJECT_ROOT == null || process.env.UF_PROJECT_ROOT === "") {
    process.env.UF_PROJECT_ROOT = projectRoot;
  }
  // Turbopack resolves an import against the files that existed when the
  // build started. A stylesheet the loader creates while compiling the module
  // is invisible to that resolve, so the import of it fails even though the
  // file is on disk afterwards. Creating the empty file here, before Next
  // compiles anything, makes the import resolvable; the loader then replaces
  // the empty file with the generated sheet, or deletes it when the module
  // declares none.
  ensureStyleSheets(projectRoot);
  return next;
}

const SKIP_DIRECTORIES = new Set(["node_modules", ".next", ".git", ".uf", "dist", "coverage", "out"]);
const SOURCE_EXTENSION = /\.(?:js|jsx|mjs|cjs)$/;
const MAX_SOURCE_BYTES = 2 * 1024 * 1024;

/**
 * Where a module's StyleX sheet is written, and the import that reaches it.
 *
 * A project file gets a stylesheet beside itself. The suffix is `.uf.css`,
 * not `.module.css`: a CSS module would hash StyleX's class, and Next only
 * allows a global stylesheet from a layout. Turbopack is told this suffix is
 * plain CSS. Webpack's ban on global CSS outside a layout is opened for it.
 * A file under `node_modules` cannot keep a sibling: the store is not always
 * writable, and the sheet would not survive the next install. Those go under
 * `<root>/.uf/turbopack/`.
 *
 * @param {string} filename
 * @param {string} root
 * @returns {{ file: string, specifier: string }}
 */
function styleModuleFor(filename, root) {
  if (!/(?:^|[\\/])node_modules[\\/]/.test(filename)) {
    const base = `${path.basename(filename)}.uf.css`;
    return { file: `${filename}.uf.css`, specifier: `./${base}` };
  }
  const dir = path.join(root, ".uf", "turbopack");
  const name = crypto.createHash("sha256").update(filename).digest("hex").slice(0, 20);
  const file = path.join(dir, `${name}.css`);
  let specifier = path.relative(path.dirname(filename), file).split(path.sep).join("/");
  if (!specifier.startsWith(".")) specifier = `./${specifier}`;
  return { file, specifier };
}

function ensureStyleSheets(root) {
  walkSources(root, root);
  walkSources(path.join(root, "node_modules", "@uniflowed"), root);
}

function walkSources(directory, root) {
  let entries;
  try {
    entries = fs.readdirSync(directory, { withFileTypes: true });
  } catch {
    return;
  }
  for (const entry of entries) {
    if (SKIP_DIRECTORIES.has(entry.name)) continue;
    const full = path.join(directory, entry.name);
    if (entry.isDirectory()) {
      walkSources(full, root);
      continue;
    }
    if (!entry.isFile() || !SOURCE_EXTENSION.test(entry.name)) continue;
    let source;
    try {
      if (fs.statSync(full).size > MAX_SOURCE_BYTES) continue;
      source = fs.readFileSync(full, "utf8");
    } catch {
      continue;
    }
    if (NOFLOW_PRAGMA.test(source) || !FLOW_PRAGMA.test(source)) continue;
    const sheet = styleModuleFor(full, root);
    if (fs.existsSync(sheet.file)) continue;
    fs.mkdirSync(path.dirname(sheet.file), { recursive: true });
    fs.writeFileSync(sheet.file, "\n");
  }
}

/**
 * webpack only when the build is already a webpack build.
 *
 * Next.js 16 fails `next build` when `webpack` is set and Turbopack is the
 * bundler, which is the default. Installing a webpack function for every
 * project would make the default build fail in order to keep a flag the
 * project did not pass. A project that already has a webpack function has
 * already accepted that choice. A project that passes `--webpack` is making
 * it at the moment the config is evaluated, and `process.argv` still carries
 * the flag because Next loads the config in the CLI process.
 */
function webpackField(existing, options) {
  if (typeof existing === "function" || (existing == null && process.argv.includes("--webpack"))) {
    return function webpack(webpackConfig, context) {
      const config =
        typeof existing === "function"
          ? existing(webpackConfig, context) ?? webpackConfig
          : webpackConfig;
      config.module ??= {};
      config.module.rules ??= [];
      if (!config.module.rules.some(containsLoader)) {
        config.module.rules.unshift({
          test   : /\.(?:js|jsx|mjs|cjs)$/,
          enforce: "pre",
          use    : [{ loader: turbopackLoaderPath, options: plainOptions(options) }],
        });
      }
      allowUfStylesheets(config);
      return config;
    };
  }
  return existing;
}

function mergeRules(existing, additions) {
  const merged = { ...(existing ?? {}) };
  for (const extension of FLOW_EXTENSIONS) {
    const previous = merged[extension];
    const prior = previous == null ? [] : Array.isArray(previous) ? previous : [previous];
    merged[extension] = [...prior, ...additions[extension]];
  }
  // Plain CSS, so StyleX's class is not hashed. A project that already named
  // this glob keeps its own rule.
  if (merged["*.uf.css"] == null) merged["*.uf.css"] = { type: "css" };
  return merged;
}

/**
 * Next rejects a global stylesheet imported from anywhere but a layout.
 * These sheets are global on purpose — a CSS module would rename StyleX's
 * class — and each one is imported by the module that owns it. The error
 * rules are the ones whose message says "Global CSS". Leaving every other
 * CSS rule alone keeps Next's own pipeline for `*.css` and `*.module.css`.
 */
function allowUfStylesheets(config) {
  const rules = config.module?.rules;
  if (!Array.isArray(rules)) return;
  for (const rule of rules) {
    if (!Array.isArray(rule?.oneOf)) continue;
    for (const item of rule.oneOf) {
      const reason = item?.use?.options?.reason;
      if (typeof reason !== "string" || !reason.includes("Global CSS")) continue;
      item.exclude = excludeUfCss(item.exclude);
    }
  }
}

function excludeUfCss(exclude) {
  const pattern = /\.uf\.css$/;
  if (exclude == null) return pattern;
  const list = Array.isArray(exclude) ? exclude : [exclude];
  if (list.some((entry) => String(entry) === String(pattern))) return exclude;
  return [...list, pattern];
}

function mergePackages(existing, required) {
  const merged = [...(existing ?? [])];
  const seen = new Set(merged);
  for (const name of required) {
    if (seen.has(name)) continue;
    seen.add(name);
    merged.push(name);
  }
  return merged;
}

/**
 * Whether a webpack rule, or a turbopack rules map, already names our loader.
 *
 * A rules map is `{ "*.js": rule | Array<rule> }`. Walking every object value
 * would also walk `condition`, which is how a RegExp would be asked whether
 * it is a loader. The map is one level; each entry is a rule or a list of them.
 */
function rulesContainLoader(rules) {
  if (rules == null || typeof rules !== "object") return false;
  const entries = Array.isArray(rules) ? rules : Object.values(rules);
  return entries.some((entry) => {
    if (Array.isArray(entry)) return entry.some(containsLoader);
    return containsLoader(entry);
  });
}

function containsLoader(value) {
  if (value == null) return false;
  if (typeof value === "string") return value === turbopackLoaderPath;
  if (Array.isArray(value)) return value.some(containsLoader);
  if (typeof value !== "object") return false;
  if (value.loader === turbopackLoaderPath) return true;
  if (value.use != null && containsLoader(value.use)) return true;
  if (value.loaders != null && containsLoader(value.loaders)) return true;
  if (value.oneOf != null && containsLoader(value.oneOf)) return true;
  return false;
}

/**
 * `@uniflowed/host/transform`, loaded synchronously.
 *
 * It is an ES module and this file is not. `require` of an ES module is what
 * Node 20.19 and 22.12 added, and on an older Node the error says that rather
 * than failing on a name.
 */
function loadHost() {
  try {
    // A sibling, not the package name. Both files ship in `@uniflowed/host`,
    // and resolving the package name walks `node_modules`, which a checkout
    // that has not installed itself does not have. The module is still ESM;
    // the Node version requirement below is unchanged.
    return require("./transform.js");
  } catch (error) {
    if (error?.code === "ERR_REQUIRE_ESM") {
      throw new Error(
        `@uniflowed/host/turbopack: Node ${process.version} cannot require @uniflowed/host, ` +
          "which is an ES module. Run Next.js on Node 20.19, 22.12 or later.",
      );
    }
    throw error;
  }
}

/**
 * The `uf` that compiles this app, found the way its installer arranged.
 *
 * In order: `UF_BINARY`, which `uf dev` sets to itself; `uf` on `PATH`; and
 * the installer's directory, `UF_BIN_DIR` or `~/.local/bin`. The found file is
 * written back to `UF_BINARY` when that variable was empty, so the binary the
 * loader's cache key names and the binary `@uniflowed/host` spawns cannot be
 * two different files.
 *
 * A name that does not resolve is an error here, while the config is loading,
 * rather than a syntax error from SWC halfway through the first page.
 */
function findUfBinary(flow) {
  const named = process.env.UF_BINARY;
  if (named != null && named !== "") {
    if (flow.ufBinaryIdentity(named) == null) {
      throw new Error(
        `@uniflowed/host/turbopack: UF_BINARY is ${named}, and that file is not an executable uf. ` +
          "Install uf with `curl -fsSL https://setup.uniflowed.dev | sh`, or set UF_BINARY to its path.",
      );
    }
    return named;
  }
  if (flow.ufBinaryIdentity("uf") != null) return "uf";
  const directory = process.env.UF_BIN_DIR || path.join(os.homedir(), ".local", "bin");
  const installed = path.join(directory, "uf");
  if (flow.ufBinaryIdentity(installed) != null) {
    process.env.UF_BINARY = installed;
    return installed;
  }
  throw new Error(
    "@uniflowed/host/turbopack: cannot find the `uf` binary that compiles Flow for this build. " +
      `UF_BINARY is not set, \`uf\` is not on PATH (${process.env.PATH ?? ""}), ` +
      `and there is no executable at ${installed}. ` +
      "Install uf with `curl -fsSL https://setup.uniflowed.dev | sh`, or set UF_BINARY to its path.",
  );
}

function pinCompiler(flow) {
  const binary = findUfBinary(flow);
  const identity = flow.ufBinaryIdentity(binary);
  if (identity == null) {
    throw new Error(`@uniflowed/host/turbopack: ${binary} is not an executable uf.`);
  }
  // The path half is absolute even when lookup started from a bare `uf`.
  const resolved = identity.split("\0")[0];
  if (
    (process.env.UF_BINARY == null || process.env.UF_BINARY === "") &&
    resolved !== ""
  ) {
    process.env.UF_BINARY = resolved;
  }
  // Loader options have to be plain JSON. The identity's own separator is a
  // NUL, which a config serializer has no reason to round-trip. The worker
  // executes `resolved`; the rest of the identity only busts its cache.
  return { identity: identity.replaceAll("\0", "|"), binary: resolved };
}

module.exports = {
  FLOW_EXTENSIONS,
  FLOW_PRAGMA,
  NOFLOW_PRAGMA,
  UNIFLOWED_PACKAGE,
  findUfBinary,
  flowRules,
  loadHost,
  styleModuleFor,
  turbopackLoaderPath,
  withUniflowed,
};
