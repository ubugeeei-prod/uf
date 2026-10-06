// @noflow
//
// Plain JavaScript: executed by the host that runs Vite, before any transform.

/**
 * Where a following statement has to be inserted so the directive prologue
 * stays a prologue.
 *
 * `uf:flow` turns a module's StyleX rules into a CSS import. That import has
 * to come after `"use client"` and `"use server"`. An import ahead of either
 * one makes the string an ordinary expression, and the RSC graph then renders
 * the module on the server — hooks included — because the directive is what
 * keeps the module out of that graph.
 *
 * The scan is the ECMAScript directive prologue: a BOM or a hashbang may
 * lead, comments and whitespace may separate statements, and a statement
 * counts only while it is a plain quoted string terminated by `;`, a line
 * break, or a comment. `"use client".length` is an expression, so the insert
 * goes in front of it.
 *
 * @param {string} source
 * @returns {number} index of the first statement that is not a directive
 */
export function directivePrologueEnd(source) {
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
 * how many source-map lines have to be skipped there. A map that is not
 * shifted points every later frame at the line above the one that ran.
 *
 * @param {string} code
 * @param {string} statement one statement, with or without its trailing newline
 * @returns {{ code: string, line: number, addedLines: number }}
 */
export function insertAfterDirectivePrologue(code, statement) {
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
export function shiftSourceMap(map, line, addedLines) {
  if (map == null || addedLines === 0) return map;
  const next = typeof map === "string" ? JSON.parse(map) : map;
  if (typeof next.mappings !== "string") return next;
  const lines = next.mappings.split(";");
  while (lines.length < line) lines.push("");
  lines.splice(line, 0, ...Array.from({ length: addedLines }, () => ""));
  next.mappings = lines.join(";");
  return next;
}

/** Index after the whitespace and comments that may sit between statements. */
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
