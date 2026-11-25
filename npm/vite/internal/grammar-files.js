// @noflow
//
// Tree-sitter grammars a project brings itself, for fenced code in a language
// uf does not ship.
//
// `app.builtins.markdown.mdx.highlight.langs` names grammars to load beside the
// ones uf highlights. A name is one of those. A project whose documentation is
// about its own language had no way to colour it, and every fence in that
// language came out as plain text. So an entry may also be a file: a
// tree-sitter grammar compiled to WebAssembly, read relative to the project
// root, with its highlight query in the file beside it.
//
// # What an entry is
//
// A path, when it contains a `/` or ends in `.wasm`; a language name
// otherwise. No language uf ships has either in its name, so the two cannot
// be confused, and `./` is the spelling the reference shows.
//
// # Which fence it colours
//
// The file's own name. `acme.wasm` colours `acme`, and `tree-sitter-acme.wasm`
// colours `acme` too: the `tree-sitter-` packages use is not part of the
// language. The query is `<stem>.highlights.scm` in the same directory, and
// the stem keeps that prefix, so the query for `tree-sitter-acme.wasm` is
// `tree-sitter-acme.highlights.scm`. A name uf already highlights is refused:
// a file must not take over JavaScript.

import fs from "node:fs";
import path from "node:path";

import { shippedNames } from "./grammars.js";

/** A fence-shaped word: what a reader types after the backticks. */
const FENCE_WORD = /^[a-z0-9][a-z0-9_+#.-]*$/;

/** The four bytes every WebAssembly module starts with. */
const WASM_MAGIC = [0x00, 0x61, 0x73, 0x6d];

/**
 * Whether a `highlight.langs` entry names a grammar file rather than one of
 * the languages uf ships.
 *
 * @param {string} entry
 */
export function isGrammarFile(entry) {
  return entry.includes("/") || entry.endsWith(".wasm");
}

/**
 * The absolute paths a dev server should watch for one grammar entry: the
 * module, and the highlight query beside it. A change to either is a change
 * to how every page is coloured.
 *
 * @param {{langs?: string[]} | undefined} options
 * @param {string} root
 */
export function grammarFiles(options, root) {
  const files = [];
  for (const entry of options?.langs ?? []) {
    if (!isGrammarFile(entry)) continue;
    const wasm = path.resolve(root, entry);
    files.push(wasm, queryFile(wasm));
  }
  return files;
}

/**
 * Read one grammar file into the registration the highlighter loads.
 *
 * Throws an error naming the entry as written and the file it resolved to,
 * because a missing grammar would otherwise be a page of grey code and no
 * message at all. The bytes are checked for the WebAssembly magic and the
 * query for being present; that the bytes are a grammar tree-sitter can load
 * is the build's question, asked when the module is compiled.
 *
 * @param {string} entry the `langs` entry, as written in `uf.config.js`
 * @param {string} root the project root it is relative to
 */
export function loadGrammar(entry, root) {
  const file = path.resolve(root, entry);
  const refuse = (reason) => grammarError(entry, root, reason);

  let wasm;
  try {
    wasm = fs.readFileSync(file);
  } catch (error) {
    throw refuse(
      error?.code === "ENOENT"
        ? "does not exist; the path is read from the project root"
        : `could not be read: ${error?.message ?? error}`,
    );
  }
  if (!isWasm(wasm)) {
    throw refuse("is not a tree-sitter grammar: it does not start with the WebAssembly magic");
  }

  const name = path
    .basename(file, ".wasm")
    .replace(/^tree-sitter-/, "")
    .toLowerCase();
  if (!FENCE_WORD.test(name)) {
    throw refuse(`names \`${name}\`, which cannot be a fence language`);
  }
  if (shippedNames().has(name)) {
    throw refuse(
      `names the language \`${name}\`, which uf already highlights; list "${name}" in \`langs\` instead of a file`,
    );
  }

  const queryPath = queryFile(file);
  let query;
  try {
    query = fs.readFileSync(queryPath, "utf8");
  } catch (error) {
    throw refuse(
      error?.code === "ENOENT"
        ? `has no highlight query at ${path.basename(queryPath)}`
        : `could not be read: ${error?.message ?? error}`,
    );
  }
  if (query.trim() === "") {
    throw refuse(`has an empty highlight query (${path.basename(queryPath)})`);
  }

  return { name, wasm, query, queryFile: queryPath, entry, root };
}

/** The error a grammar entry is refused with, naming it as written and as resolved. */
export function grammarError(entry, root, reason) {
  return new Error(
    `uf: app.builtins.markdown.mdx.highlight.langs: "${entry}" (${path.resolve(root, entry)}) ${reason}`,
  );
}

/** `<stem>.highlights.scm` beside a wasm module. The stem keeps `tree-sitter-`. */
function queryFile(wasmFile) {
  const stem = path.basename(wasmFile, ".wasm");
  return path.join(path.dirname(wasmFile), `${stem}.highlights.scm`);
}

/** Whether `bytes` open with the WebAssembly magic. */
function isWasm(bytes) {
  return WASM_MAGIC.every((byte, index) => bytes[index] === byte);
}
