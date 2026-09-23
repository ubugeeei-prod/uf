// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.
//
// Every `$error.js` has to be a client module, said while the route table is
// generated rather than the first time a page throws.
//
// Under React Server Components an error boundary catches a throw while the
// *browser* renders, so its component runs in the browser and the module has
// to open with `"use client"`. `@uniflowed/router`'s `rsc.js` has always
// refused a boundary that is not a client reference — but only when it is
// needed, which is when a page throws. A project whose `$error.js` lacked the
// directive built, deployed, and answered the first exception in production
// with a bare `500` and the reason in the server log. The routing guide's own
// example had no directive, so copying the documentation produced exactly
// that.
//
// This is the same rule, asked of the files: the RSC graph loads the route
// table, the table names every boundary module, and a boundary whose source
// does not start with the directive fails the build — or the dev server's
// route table, which is the same moment for `uf dev` — naming every such file
// at once.
//
// What it is not: a parser. The directive prologue is the only part of a
// module this reads (comments, then string-literal statements), which is all
// the directive rule concerns; `crates/uf_rsc`'s scan decides the same thing
// from a real parse for the client/server split, and a module that satisfies
// this check but not that one is refused by the router at request time, as
// before.

import { readFileSync } from "node:fs";
import path from "node:path";

/**
 * Whether `source` opens with a `"use client"` directive.
 *
 * Reads the directive prologue: leading whitespace, `//` and `/* *\/`
 * comments (a `// @flow` header among them), then string-literal statements.
 * `"use client"` anywhere in the prologue counts, as it does for React's own
 * rule; the first thing that is not a comment or a string literal ends it.
 *
 * @param {string} source
 * @returns {boolean}
 */
export function hasUseClientDirective(source) {
  let at = source.charCodeAt(0) === 0xfeff ? 1 : 0;
  const length = source.length;
  for (;;) {
    // Whitespace and comments between statements.
    for (;;) {
      while (at < length && /\s/.test(source[at])) at++;
      if (source.startsWith("//", at)) {
        const end = source.indexOf("\n", at);
        at = end === -1 ? length : end + 1;
      } else if (source.startsWith("/*", at)) {
        const end = source.indexOf("*/", at + 2);
        if (end === -1) return false;
        at = end + 2;
      } else {
        break;
      }
    }
    const quote = source[at];
    if (quote !== '"' && quote !== "'") return false;
    const close = source.indexOf(quote, at + 1);
    if (close === -1) return false;
    const value = source.slice(at + 1, close);
    if (value.includes("\n")) return false;
    at = close + 1;
    // A directive is a whole statement: `;`, a line end, a comment, or the
    // end. Anything else — `"use client".length` — is an expression.
    while (at < length && (source[at] === " " || source[at] === "\t")) at++;
    if (source[at] === ";") {
      at++;
    } else if (
      at < length &&
      source[at] !== "\n" &&
      source[at] !== "\r" &&
      !source.startsWith("//", at) &&
      !source.startsWith("/*", at)
    ) {
      return false;
    }
    if (value === "use client") return true;
  }
}

/**
 * Every error-boundary module a scanned route table names, once each.
 *
 * `table.errors` is one entry per directory holding an `$error.js`; a slot's
 * boundaries are only on the routes that render the slot, as an
 * `errorBoundary`. Both are collected by walking the table rather than by
 * knowing where each kind lives, so a boundary a later change puts somewhere
 * new is still asked about.
 *
 * @param {object} table what `scanRoutes` answered
 * @returns {string[]} absolute paths, sorted
 */
export function errorBoundaryModules(table) {
  const found = new Set();
  for (const entry of table.errors ?? []) {
    if (typeof entry?.module === "string") found.add(entry.module);
  }
  const seen = new Set();
  const walk = (value) => {
    if (value == null || typeof value !== "object" || seen.has(value)) return;
    seen.add(value);
    if (Array.isArray(value)) {
      for (const item of value) walk(item);
      return;
    }
    for (const [key, child] of Object.entries(value)) {
      if (key === "errorBoundary" && typeof child?.module === "string") found.add(child.module);
      walk(child);
    }
  };
  walk(table.routes ?? []);
  return [...found].sort();
}

/**
 * Fail when an error boundary in `table` is not a client module.
 *
 * Throws one error naming every such file, relative to `root`, with the fix;
 * answers nothing otherwise. `read` is the file reader, for a test.
 *
 * @param {object} table what `scanRoutes` answered
 * @param {string} root the project root, for the names in the message
 * @param {(file: string) => string} [read]
 */
export function refuseServerErrorBoundaries(
  table,
  root,
  read = (file) => readFileSync(file, "utf8"),
) {
  const missing = errorBoundaryModules(table).filter((file) => {
    let source;
    try {
      source = read(file);
    } catch {
      // A boundary the scan found and nobody can read is the bundler's to
      // report, with the error it gets opening it.
      return false;
    }
    return !hasUseClientDirective(source);
  });
  if (missing.length === 0) return;
  const names = missing.map((file) => path.relative(root, file).split(path.sep).join("/"));
  throw new Error(
    `uf: ${names.length === 1 ? "an error boundary is" : `${names.length} error boundaries are`} not ` +
      `a client module: ${names.join(", ")}. An \`$error.js\` catches a throw while the browser ` +
      'renders, so its component runs in the browser, and the module has to open with "use client" ' +
      "as its first statement. Without it the page that throws is answered with a bare 500. Add " +
      '"use client"; at the top of each. See docs/app/guide/routing.',
  );
}
