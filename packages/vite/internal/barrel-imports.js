// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// `import { Switch } from "@uniflowed/ui"`, as the bundler has to see it: an
// import from `switch.js` (ubugeeei-prod/uf#1118).
//
// # Why a barrel import is rewritten at all
//
// `@uniflowed/ui` is imported through its barrel, which re-exports every one of
// its modules, and `sideEffects: false` lets a bundler drop the modules a page
// does not use. But that happens when a build tree-shakes, and two things
// happen before it:
//
//   * **The rsc pass records client modules as it transforms them.**
//     `clientReferencePlugin` in `./flight.js` makes every `"use client"` module
//     the rsc graph *loads* an entry of the client build. A page that imported
//     `{ Switch }` loaded the barrel, the barrel loaded all forty-one modules,
//     and the client build of one switch was 57 files and 146 KB gzipped where
//     an import of `switch.js` alone is 6 files and 88 KB.
//   * **A development server evaluates what is imported.** Nothing is
//     tree-shaken under `uf dev`, so a page using one part ran all of them.
//
// So a named import from the barrel becomes an import from the file that
// defines the name, before either happens: the idea behind Next.js's
// `optimizePackageImports`. `uf_rsc` draws the client boundary of such an
// import at that module too (`uf_lib::client_modules_exporting`), so the
// analysis and the bundle agree about what a name reaches.
//
// # Read off the barrel the project resolves
//
// Which file defines a name is read from the barrel itself: its static
// `import { A } from "./a.js"`, `export { A } from "./a.js"` and
// `export * as A from "./a.js"` statements. Not a table kept here, because the
// barrel a project installed is the one whose names count.
//
// A name that reading cannot place stays an import from the barrel, and so does
// every form that binds no name — `import * as ui`, `export * from` and
// `import()` — which is the rule `uf_rsc` applies to the same forms. A barrel
// import left alone costs size, never correctness, for the next reason.
//
// # One module, however it is reached
//
// A target is the file's absolute path, not a package subpath, so the package
// can export `.` alone. It is the path the barrel's own `./switch.js` resolves
// to, and Vite gives an absolute, a relative and a bare import of one file the
// same id in every environment — the `?v=` a development server adds to a file
// in `node_modules` included. So a module still loading the barrel and a module
// rewritten past it share one `switch.js`, and one React context between their
// parts.
//
// # Namespaces
//
// `Dialog` is `dialog.js` itself: the barrel says `export * as Dialog from
// "./dialog.js"` (ubugeeei-prod/uf#1453), so `import { Dialog } from
// "@uniflowed/ui"` becomes `import * as Dialog from ".../dialog.js"` and
// `export { Dialog as Modal } from "@uniflowed/ui"` becomes
// `export * as Modal from ".../dialog.js"`. A module namespace is the same object
// however many importers ask for it, so nothing is generated to stand in for
// it. `ContextMenu` spans two modules and is still one: `context-menu.js`
// re-exports `Menu`'s parts, and importing it reaches `menu.js` through that.
//
// Until #1453 the barrel built each namespace as an object literal, and a
// generated `index.js?uf-namespace=Dialog` module rebuilt the object for an
// importer. That form is no longer read: `@uniflowed/vite` and `@uniflowed/ui`
// are released together, and an object literal the reading meets now is
// `OPAQUE` — left on the barrel, which costs size and never correctness.

import { readFileSync, statSync } from "node:fs";
import path from "node:path";

import { normalizePath, parseAst } from "vite";

/** The packages whose barrel imports are rewritten. */
export const BARREL_PACKAGES = Object.freeze(["@uniflowed/ui"]);

/** The module kinds whose imports are read, once earlier plugins made them JavaScript. */
const SCRIPT = /\.(?:[cm]?[jt]sx?|mdx)$/;

/** A name the barrel exports that no rewrite can place. */
const OPAQUE = Object.freeze({ kind: "opaque" });

/**
 * The plugin that rewrites named imports from a barrel to the files defining
 * them, in every environment.
 *
 * A normal plugin, so it runs after `uf:flow` and `uf:mdx` and after Vite's own
 * transforms: whatever the module was written in, it is JavaScript by the time
 * its imports are read.
 */
export function barrelImportsPlugin() {
  /** Each barrel's reading, keyed by its file and kept while the file is unchanged. */
  const readings = new Map();
  /** The barrels already reported as unreadable, so each is reported once. */
  const reported = new Set();

  return {
    name: "uf:barrel-imports",

    async transform(code, id) {
      if (id.startsWith("\0")) return null;
      if (!BARREL_PACKAGES.some((name) => code.includes(name))) return null;
      if (!SCRIPT.test(cleanId(id))) return null;
      let program;
      try {
        program = parseAst(code);
      } catch {
        return null;
      }
      const statements = program.body.filter(importsFromBarrel);
      if (statements.length === 0) return null;

      const edits = [];
      for (const name of BARREL_PACKAGES) {
        const own = statements.filter((node) => node.source.value === name);
        if (own.length === 0) continue;
        const resolved = await this.resolve(name, id, { skipSelf: true });
        // Left to whatever loads an external module, which reads the barrel
        // as a whole; nothing here can change what that loads.
        if (resolved == null || resolved.external) continue;
        const file = cleanId(resolved.id);
        const barrel = readBarrel(readings, file);
        if (barrel.problem != null) {
          if (!reported.has(file)) {
            reported.add(file);
            this.warn(
              `uf: could not read which file defines each export of ${name} (${barrel.problem}), ` +
                "so a module importing from it loads every module it re-exports",
            );
          }
          continue;
        }
        this.addWatchFile?.(file);
        for (const node of own) {
          const replacement = rewriteStatement(node, barrel.exports, file);
          if (replacement != null) edits.push({ node, replacement });
        }
      }
      if (edits.length === 0) return null;
      return { code: applyEdits(code, edits), map: null };
    },
  };
}

/**
 * Where each name a barrel exports is defined, read from its source.
 *
 * The barrel is Flow, read with the TypeScript grammar: the only type syntax a
 * barrel has is `export type { A } from` and `import { type A }`, which the two
 * grammars spell alike. A barrel this cannot parse throws, and its importers
 * are left as they were.
 *
 * Each name maps to one of three things:
 *
 *   * `{ kind: "binding", file, name }` — an export of another module, passed
 *     through under this name;
 *   * `{ kind: "module", file }` — another module whole, re-exported as a
 *     namespace with `export * as Name from "./file.js"`;
 *   * `OPAQUE` — anything else, which stays an import from the barrel.
 *
 * @param {string} source
 * @param {string} file the barrel's absolute path
 * @returns {Map<string, object>}
 */
export function barrelExports(source, file) {
  const program = parseAst(source, { lang: "ts" });
  const directory = path.dirname(file);
  const fileOf = (node) =>
    typeof node?.value === "string" && /^\.\.?\//.test(node.value)
      ? normalizePath(path.join(directory, node.value))
      : null;

  // Every binding an import made, before any export is read: `export { A }`
  // may come before the `import` it names.
  const bindings = new Map();
  for (const node of program.body) {
    if (node.type === "ImportDeclaration" && node.importKind !== "type") {
      const from = fileOf(node.source);
      for (const specifier of node.specifiers) {
        if (specifier.importKind === "type") continue;
        const imported =
          specifier.type === "ImportSpecifier"
            ? nameOf(specifier.imported)
            : specifier.type === "ImportDefaultSpecifier"
              ? "default"
              : null;
        bindings.set(
          specifier.local.name,
          from == null || imported == null ? null : { file: from, name: imported },
        );
      }
    }
  }

  const local = (name) => {
    const binding = bindings.get(name);
    return binding == null ? OPAQUE : { kind: "binding", ...binding };
  };

  const exports = new Map();
  for (const node of program.body) {
    if (node.type === "ExportDefaultDeclaration") {
      exports.set("default", OPAQUE);
    }
    // `export * as Dialog from "./dialog.js"`: the namespace is the module. A
    // bare `export * from` names nothing, so it places nothing.
    if (node.type === "ExportAllDeclaration" && node.exported != null) {
      if (node.exportKind === "type") continue;
      const from = fileOf(node.source);
      exports.set(nameOf(node.exported), from == null ? OPAQUE : { kind: "module", file: from });
      continue;
    }
    if (node.type !== "ExportNamedDeclaration" || node.exportKind === "type") continue;
    const { declaration } = node;
    if (declaration != null) {
      if (declaration.type === "VariableDeclaration") {
        for (const declarator of declaration.declarations) {
          if (declarator.id.type === "Identifier") {
            exports.set(declarator.id.name, local(declarator.id.name));
          }
        }
      } else if (declaration.id?.type === "Identifier" && !declaration.type.startsWith("TS")) {
        exports.set(declaration.id.name, OPAQUE);
      }
      continue;
    }
    const from = node.source == null ? null : fileOf(node.source);
    for (const specifier of node.specifiers) {
      if (specifier.exportKind === "type") continue;
      const name = nameOf(specifier.local);
      exports.set(
        nameOf(specifier.exported),
        node.source == null
          ? local(name)
          : from == null
            ? OPAQUE
            : { kind: "binding", file: from, name },
      );
    }
  }
  return exports;
}

/** An import or re-export whose source is one of `BARREL_PACKAGES`. */
function importsFromBarrel(node) {
  if (node.type === "ImportDeclaration") {
    return node.importKind !== "type" && BARREL_PACKAGES.includes(node.source.value);
  }
  return (
    node.type === "ExportNamedDeclaration" &&
    node.source != null &&
    node.exportKind !== "type" &&
    BARREL_PACKAGES.includes(node.source.value)
  );
}

/**
 * The statements one import or re-export from a barrel becomes, as source, or
 * `null` to leave it as it was.
 *
 * @param {object} node the statement
 * @param {Map<string, object>} exports the barrel's reading
 * @param {string} barrel the barrel's absolute path
 */
function rewriteStatement(node, exports, barrel) {
  const isImport = node.type === "ImportDeclaration";
  const specifiers = node.specifiers;
  // A bare import binds nothing, and the barrel declares it has no side
  // effects: what it loads is every module, for nothing.
  if (isImport && specifiers.length === 0) return "";
  // `import * as ui` names nothing that can be placed; see the header.
  if (specifiers.some((specifier) => specifier.type === "ImportNamespaceSpecifier")) return null;

  const kept = [];
  let keptDefault = null;
  const moved = new Map();
  // One statement each: `import * as Dialog` binds a single name.
  const namespaces = [];
  for (const specifier of specifiers) {
    if (specifier.type === "ImportDefaultSpecifier") {
      keptDefault = specifier.local.name;
      continue;
    }
    const exported = nameOf(isImport ? specifier.imported : specifier.local);
    const binding = isImport ? specifier.local.name : nameOf(specifier.exported);
    const printed = (name) =>
      name === binding ? printName(name) : `${printName(name)} as ${printName(binding)}`;
    const target = specifier.importKind === "type" ? OPAQUE : (exports.get(exported) ?? OPAQUE);
    if (target.kind === "opaque") {
      kept.push(printed(exported));
      continue;
    }
    if (target.kind === "module") {
      // A namespace binding has to be an identifier, which an import's local
      // name always is and an export's exported name may not be.
      if (!IDENTIFIER.test(binding)) {
        kept.push(printed(exported));
        continue;
      }
      namespaces.push({ file: target.file, binding });
      continue;
    }
    const list = moved.get(target.file) ?? [];
    list.push(printed(target.name));
    moved.set(target.file, list);
  }
  if (moved.size === 0 && namespaces.length === 0) return null;

  const keyword = isImport ? "import" : "export";
  const statements = [];
  if (kept.length > 0 || keptDefault != null) {
    const clause = [keptDefault, kept.length > 0 ? `{ ${kept.join(", ")} }` : null]
      .filter(Boolean)
      .join(", ");
    statements.push(`${keyword} ${clause} from ${JSON.stringify(node.source.value)};`);
  }
  for (const [source, list] of moved) {
    statements.push(`${keyword} { ${list.join(", ")} } from ${JSON.stringify(source)};`);
  }
  for (const { file, binding } of namespaces) {
    statements.push(`${keyword} * as ${binding} from ${JSON.stringify(file)};`);
  }
  return statements.join("\n");
}

/**
 * `code` with each statement replaced, and every line after it where it was.
 *
 * No source map: the only lines that change are the imports themselves, and
 * each replacement is padded to the line count it replaced, so a position
 * anywhere after them maps as it did — the reason `clientReferencePlugin`
 * returns `map: null` too.
 */
function applyEdits(code, edits) {
  let out = "";
  let at = 0;
  for (const { node, replacement } of [...edits].sort((a, b) => a.node.start - b.node.start)) {
    const original = code.slice(node.start, node.end);
    const lines = original.split("\n").length - 1;
    const written = replacement.split("\n").length - 1;
    const text =
      written <= lines
        ? `${replacement}${"\n".repeat(lines - written)}`
        : `${replacement.split("\n").join(" ")}${"\n".repeat(lines)}`;
    out += code.slice(at, node.start) + text;
    at = node.end;
  }
  return out + code.slice(at);
}

/** A barrel's reading, re-read only when its size or modification time changes. */
function readBarrel(readings, file) {
  let stats;
  try {
    stats = statSync(file);
  } catch (error) {
    return { exports: new Map(), problem: error.message };
  }
  const known = readings.get(file);
  if (known != null && known.mtimeMs === stats.mtimeMs && known.size === stats.size) return known;
  const reading = { mtimeMs: stats.mtimeMs, size: stats.size, exports: new Map(), problem: null };
  try {
    reading.exports = barrelExports(readFileSync(file, "utf8"), file);
  } catch (error) {
    reading.problem = error.message;
  }
  readings.set(file, reading);
  return reading;
}

const IDENTIFIER = /^[A-Za-z_$][\w$]*$/;

function nameOf(node) {
  return node.type === "Identifier" ? node.name : String(node.value);
}

function printName(name) {
  return IDENTIFIER.test(name) ? name : JSON.stringify(name);
}

function cleanId(id) {
  const at = id.indexOf("?");
  return at === -1 ? id : id.slice(0, at);
}
