// @noflow
//
// TextMate grammars a project brings itself, for fenced code in a language
// Shiki does not ship.
//
// `app.builtins.markdown.mdx.highlight.langs` names grammars to load beside the
// default ones. A name is one of Shiki's; a project whose documentation is
// about its own language had no way to colour it, and every fence in that
// language came out as the `text` fallback. So an entry may also be a file: a
// TextMate grammar in JSON, the format every editor extension already ships,
// read relative to the project root.
//
// # What an entry is
//
// A path, when it contains a `/` or ends in `.json`; a language name
// otherwise. No language Shiki ships has either in its name, so the two
// cannot be confused, and `./` is the spelling the reference shows.
//
// # Which fence it colours
//
// A grammar file says what it is in `scopeName` (`source.foo`), and the fence
// is the last segment of that (`foo`). `name` is a display name in most
// editor grammars (`Foo Language`), so it is an alias only when it is already
// a fence-shaped word. `aliases` and `fileTypes` (`["foo", ".fooz"]`) are
// aliases too. An alias that is one of Shiki's own languages is dropped rather
// than allowed to take that language over: a grammar listing `js` among its
// file types must not recolour every JavaScript sample on the site.

import fs from "node:fs";
import path from "node:path";

import { bundledLanguages, bundledLanguagesInfo } from "shiki";

/** A fence-shaped word: what a reader types after the backticks. */
const FENCE_WORD = /^[a-z0-9][a-z0-9_+#.-]*$/;

/** Every id and alias Shiki ships, which a grammar file may not claim. */
const SHIPPED = new Set(bundledLanguagesInfo.flatMap((info) => [info.id, ...(info.aliases ?? [])]));

/**
 * Whether a `highlight.langs` entry names a grammar file rather than one of
 * Shiki's languages.
 *
 * @param {string} entry
 */
export function isGrammarFile(entry) {
  return entry.includes("/") || entry.endsWith(".json");
}

/**
 * The absolute paths of the grammar files a config names, in order.
 *
 * @param {{langs?: string[]} | undefined} options
 * @param {string} root
 */
export function grammarFiles(options, root) {
  return (options?.langs ?? []).filter(isGrammarFile).map((entry) => path.resolve(root, entry));
}

/**
 * Read one grammar file into the registration Shiki takes in `langs`.
 *
 * Throws an error naming the entry as written and the file it resolved to,
 * because a missing grammar would otherwise be a page of grey code and no
 * message at all.
 *
 * @param {string} entry the `langs` entry, as written in `uf.config.js`
 * @param {string} root the project root it is relative to
 */
export function loadGrammar(entry, root) {
  const file = path.resolve(root, entry);
  const refuse = (reason) => refusal(entry, root, reason);

  let source;
  try {
    source = fs.readFileSync(file, "utf8");
  } catch (error) {
    throw refuse(
      error?.code === "ENOENT"
        ? "does not exist; the path is read from the project root"
        : `could not be read: ${error?.message ?? error}`,
    );
  }
  let grammar;
  try {
    grammar = JSON.parse(source);
  } catch (error) {
    throw refuse(`is not JSON: ${error?.message ?? error}`);
  }
  if (grammar == null || typeof grammar !== "object" || Array.isArray(grammar)) {
    throw refuse("is not a TextMate grammar: it is not an object");
  }
  if (typeof grammar.scopeName !== "string" || grammar.scopeName === "") {
    throw refuse("is not a TextMate grammar: it has no `scopeName`");
  }
  if (!Array.isArray(grammar.patterns)) {
    throw refuse("is not a TextMate grammar: it has no `patterns`");
  }

  const id = grammar.scopeName.split(".").pop().toLowerCase();
  if (!FENCE_WORD.test(id)) {
    throw refuse(
      `has the scope \`${grammar.scopeName}\`, whose last segment \`${id}\` cannot be a fence language`,
    );
  }
  if (SHIPPED.has(id)) {
    throw refuse(
      `names the language \`${id}\`, which Shiki already ships; list "${id}" in \`langs\` instead of a file`,
    );
  }

  const candidates = [
    ...(Array.isArray(grammar.aliases) ? grammar.aliases : []),
    ...(Array.isArray(grammar.fileTypes) ? grammar.fileTypes : []),
    typeof grammar.name === "string" ? grammar.name : "",
  ];
  const aliases = [
    ...new Set(
      candidates
        .filter((alias) => typeof alias === "string")
        .map((alias) => alias.replace(/^\./, "").toLowerCase())
        .filter((alias) => alias !== id && FENCE_WORD.test(alias) && !SHIPPED.has(alias)),
    ),
  ];

  return { ...grammar, name: id, aliases };
}

/** The error a grammar entry is refused with, naming it as written and as resolved. */
function refusal(entry, root, reason) {
  return new Error(
    `uf: app.builtins.markdown.mdx.highlight.langs: "${entry}" (${path.resolve(root, entry)}) ${reason}`,
  );
}

/**
 * Every scope name Shiki ships, with the language it belongs to.
 *
 * Read by loading every bundled language, which is a fraction of a second
 * and done at most once per process, and only for a project that brings a
 * grammar file.
 */
let shippedScopes /*: ?Promise<Map<string, string>> */ = null;

function shippedScopeNames() {
  shippedScopes ??= (async () => {
    const scopes = new Map();
    for (const load of new Set(Object.values(bundledLanguages))) {
      for (const registration of (await load()).default) {
        if (!scopes.has(registration.scopeName)) {
          scopes.set(registration.scopeName, registration.name);
        }
      }
    }
    return scopes;
  })();
  return shippedScopes;
}

/**
 * Refuse a grammar file whose `scopeName` is one Shiki already ships.
 *
 * The id check in `loadGrammar` reads only the last segment, and a scope such
 * as `text.html.basic` ends in a word no language is called. Registered beside
 * Shiki's own HTML grammar it would share that grammar's scope, and every
 * grammar that embeds HTML would pick up whichever of the two came last.
 *
 * @param {$ReadOnlyArray<{scopeName: string}>} grammars the loaded grammars, in `entries` order
 * @param {$ReadOnlyArray<string>} entries the `langs` entries they came from
 * @param {string} root the project root the entries are relative to
 */
export async function refuseShippedScopes(grammars, entries, root) {
  if (grammars.length === 0) {
    return;
  }
  const scopes = await shippedScopeNames();
  grammars.forEach((grammar, index) => {
    const language = scopes.get(grammar.scopeName);
    if (language != null) {
      throw refusal(
        entries[index],
        root,
        `has the scope \`${grammar.scopeName}\`, which Shiki already ships as \`${language}\`; list "${language}" in \`langs\` instead of a file`,
      );
    }
  });
}
