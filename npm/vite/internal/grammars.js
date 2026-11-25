// @noflow
//
// The tree-sitter grammars uf highlights with, and nothing else.
//
// They are vendored, WASM and highlight query only, rather than depended on
// as packages: a grammar package ships its C sources and native binding too,
// and bash's alone is twenty megabytes for a file this module never reads.
// `internal/grammars/SOURCES.txt` says where the bytes came from.

import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const ROOT = path.join(path.dirname(fileURLToPath(import.meta.url)), "grammars");

/**
 * One grammar and the fence names that mean it.
 *
 * `id` is the grammar. `aliases` are the other words a fence says for it:
 * `shellscript` is what uf's own samples already use, and `flow` is
 * JavaScript's grammar because no maintained tree-sitter grammar covers
 * modern Flow. The words Flow adds are marked after the grammar runs.
 */
const LANGUAGES = [
  {
    id     : "javascript",
    aliases: ["js", "jsx", "flow"],
    dir    : "javascript",
    wasm   : "tree-sitter-javascript.wasm",
    // JSX lives in a second query. Concatenated, one Query covers both.
    queries: ["highlights.scm", "highlights-jsx.scm"],
  },
  {
    id     : "json",
    aliases: [],
    dir    : "json",
    wasm   : "tree-sitter-json.wasm",
    queries: ["highlights.scm"],
  },
  {
    id     : "css",
    aliases: [],
    dir    : "css",
    wasm   : "tree-sitter-css.wasm",
    queries: ["highlights.scm"],
  },
  {
    id     : "html",
    aliases: [],
    dir    : "html",
    wasm   : "tree-sitter-html.wasm",
    queries: ["highlights.scm"],
  },
  {
    id     : "bash",
    aliases: ["sh", "shell", "shellscript", "console"],
    dir    : "bash",
    wasm   : "tree-sitter-bash.wasm",
    queries: ["highlights.scm"],
  },
  {
    id     : "rust",
    aliases: [],
    dir    : "rust",
    wasm   : "tree-sitter-rust.wasm",
    queries: ["highlights.scm"],
  },
  {
    id     : "yaml",
    aliases: ["yml"],
    dir    : "yaml",
    wasm   : "tree-sitter-yaml.wasm",
    queries: ["highlights.scm"],
  },
  {
    id     : "toml",
    aliases: [],
    dir    : "toml",
    wasm   : "tree-sitter-toml.wasm",
    queries: ["highlights.scm"],
  },
  {
    id     : "markdown",
    aliases: ["md", "mdx"],
    dir    : "markdown",
    wasm   : "tree-sitter-markdown.wasm",
    queries: ["highlights.scm"],
  },
  {
    id     : "diff",
    aliases: [],
    dir    : "diff",
    wasm   : "tree-sitter-diff.wasm",
    queries: ["highlights.scm"],
  },
];

/** The fence names uf highlights without a project naming them. */
export const DEFAULT_LANGS = [
  "javascript",
  "jsx",
  "json",
  "css",
  "html",
  "markdown",
  "mdx",
  "shellscript",
  "rust",
  "toml",
  "yaml",
  "diff",
];

/** Every fence name a project must not claim with its own grammar. */
export function shippedNames() {
  return new Set(LANGUAGES.flatMap((language) => [language.id, ...language.aliases]));
}

/** The grammar a fence name means, or `null` when uf does not ship it. */
export function languageFor(name) {
  return (
    LANGUAGES.find((language) => language.id === name || language.aliases.includes(name)) ?? null
  );
}

/** The ids a config error can list. Aliases are the fence's own spelling. */
export function shippedIds() {
  return LANGUAGES.map((language) => language.id);
}

/** Every vendored grammar, read, in the order fences fall back to. */
export function readShipped() {
  return LANGUAGES.map(readLanguage);
}

/**
 * The bytes and the highlight query for one vendored grammar.
 *
 * Read on each compile rather than imported, so a test can point a project
 * grammar at the same files and so the query sources stay ordinary text.
 */
export function readLanguage(language) {
  const dir = path.join(ROOT, language.dir);
  return {
    id   : language.id,
    wasm : fs.readFileSync(path.join(dir, language.wasm)),
    query: language.queries.map((name) => fs.readFileSync(path.join(dir, name), "utf8")).join("\n"),
  };
}
