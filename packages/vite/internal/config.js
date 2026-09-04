// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// Loading `uf.config.js`.
//
// The config is a Flow module — `import { defineConfig } from
// "@uniflowed/config"` and the `// @flow` docblock are the documented shape —
// so no host can import it as written. It is transformed through
// `uf transform` like every other module, written under `.uf/config/`, and
// imported from there. Relative imports inside the config are rewritten to
// absolute URLs first, because the compiled copy does not live where the
// author's file does.
//
// The result is the config *object*, functions and plugin instances included.
// `projectConfig` is its JSON projection for the Rust side, which needs the
// data and cannot use the functions.

import { createHash } from "node:crypto";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { pathToFileURL } from "node:url";

import { transformFlow } from "../transform.js";

/** The one config file name uf reads. */
export const CONFIG_FILES = ["uf.config.js"];

/** Where compiled config modules are written, relative to the project root. */
const COMPILED_DIR = path.join(".uf", "config");

/** Longest config file the loader accepts. */
const MAX_CONFIG_BYTES = 1024 * 1024;

/**
 * Find `uf.config.js` at `root`, or `null`.
 */
export function findConfigFile(root) {
  for (const name of CONFIG_FILES) {
    const candidate = path.join(root, name);
    try {
      readFileSync(candidate);
      return candidate;
    } catch {
      // keep looking
    }
  }
  return null;
}

/**
 * Load the project's config object.
 *
 * A project without a config file gets `{}`, which every consumer treats as
 * "the defaults". A config whose default export is not an object is an error
 * at load time, named after the file, rather than a cascade of `undefined`
 * later.
 *
 * @param {string} root absolute project root
 * @returns {Promise<{config: object, file: string | null}>}
 */
export async function loadUfConfig(root) {
  const file = findConfigFile(root);
  if (file == null) return { config: {}, file: null };

  const source = readFileSync(file, "utf8");
  if (source.length > MAX_CONFIG_BYTES) {
    throw new Error(
      `uf: ${file} is ${source.length} bytes, over the ${MAX_CONFIG_BYTES} byte ceiling`,
    );
  }

  const compiled = await compileConfig(source, file, root);
  const module = await import(pathToFileURL(compiled).href);
  const config = module.default;
  if (config == null || typeof config !== "object") {
    throw new Error(
      `uf: ${path.relative(root, file)} must \`export default defineConfig({ ... })\``,
    );
  }
  return { config, file };
}

/**
 * Transform the config to JavaScript and write it where it can be imported.
 *
 * The file name carries a hash of the source, so an edited config is a new
 * module and never a stale cached one — `import()` caches by URL.
 */
async function compileConfig(source, file, root) {
  const hash = createHash("sha256").update(source).digest("hex").slice(0, 16);
  const directory = path.join(root, COMPILED_DIR);
  const target = path.join(directory, `uf.config.${hash}.mjs`);

  const out = await transformFlow(source, file, { root, sourceMap: false });
  const code = rewriteRelativeImports(out?.code ?? source, path.dirname(file));
  mkdirSync(directory, { recursive: true });
  writeFileSync(target, `// Compiled from ${file}. Do not edit; edit the source.\n${code}`);
  return target;
}

/**
 * Make every relative import specifier in `code` absolute.
 *
 * The compiled config lives under `.uf/config/`, so `./plugins/x.js` would
 * otherwise resolve to a file that is not there. Bare specifiers are left
 * alone: they resolve through `node_modules` from the compiled file exactly as
 * they would from the source.
 *
 * Specifiers are string literals in `import`/`export … from` statements and
 * `import()` calls; matching them textually is enough because the transform
 * has already produced plain JavaScript with one statement per line.
 */
export function rewriteRelativeImports(code, baseDirectory) {
  const absolute = (specifier) => pathToFileURL(path.resolve(baseDirectory, specifier)).href;
  return code.replace(
    /((?:\bfrom\s*|\bimport\s*\(?\s*)["'])(\.\.?\/[^"']*)(["'])/g,
    (_, head, specifier, tail) => `${head}${absolute(specifier)}${tail}`,
  );
}

/**
 * The JSON projection of a config object.
 *
 * Functions are dropped, and a Vite plugin object is reduced to its name so
 * `uf inspect` can still say which plugins a project declares. Everything the
 * Rust side reads — lint rules, formatter settings, tasks, hosts — is plain
 * data and survives unchanged.
 */
export function projectConfig(config) {
  return JSON.parse(
    JSON.stringify(config, (key, value) => {
      if (typeof value === "function") return undefined;
      if (key === "plugins" && Array.isArray(value)) {
        return value
          .flat(Infinity)
          .map(pluginName)
          .filter((name) => name != null);
      }
      return value;
    }),
  );
}

function pluginName(plugin) {
  if (plugin == null || plugin === false) return null;
  if (typeof plugin === "string") return plugin;
  if (typeof plugin === "object" && typeof plugin.name === "string") return plugin.name;
  return null;
}
