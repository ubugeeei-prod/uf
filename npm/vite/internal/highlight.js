// @noflow
//
// Syntax highlighting for fenced code, at build time.
//
// uf's claim is that MDX works without a plugin list, and a documentation page
// whose code samples are undifferentiated grey does not meet it. So this is on
// by default, it runs during the build rather than shipping a highlighter to
// the browser, and it knows about Flow.
//
// # Flow
//
// No highlighter has a Flow grammar. A JavaScript one runs instead, and it
// fails at Flow's syntax in two different ways that need two different
// answers:
//
// * It **mis-labels** the words it does tokenise. `component`, `hook`,
//   `renders`, `match`, `opaque` and `mixed` are ordinary identifiers to it,
//   so they would be the only uncoloured words in a uf sample — which is the
//   wrong way round, since they are the reason the sample is there.
//   `internal/flow-keywords.js` decides which occurrences are really Flow's,
//   after the grammar has run.
//
// * It **stops** at a `component` or `hook` declaration and mis-scopes
//   everything after an exact object type, so the rest of the construct — or
//   the rest of the block — arrives as one unstyled run, or in the colours of
//   whatever the grammar fell into. There is nothing to re-label; the tokens
//   do not exist. `internal/flow-grammar-shim.js` shows the grammar JavaScript
//   it can parse instead and rebuilds the tokens over the real text, before
//   this module ever sees them.
//
// The order is fixed and is the reason they are separate modules: the shim
// runs before tokenising, the marking runs after, and each is testable without
// the other. Writing and maintaining a Flow TextMate grammar would replace
// both; until one exists, this is the honest approximation, and its limits are
// recorded in each module's header.
//
// Marking rather than recolouring, because the colour is not knowable here.
// Copying the colour from another keyword in the same block was the first
// attempt and it fails exactly where it matters: a snippet that is nothing but
// `component Tab(label: string) renders React.Node` contains no keyword the
// grammar recognises, so there was nothing to copy from. A class moves the
// decision to CSS, which is where the theme's colours already live.

import rehypeShikiFromHighlighter from "@shikijs/rehype/core";
import { createHighlighter } from "shiki";

import { shimFlowGrammar } from "./flow-grammar-shim.js";
import { isGrammarFile, loadGrammar, refuseShippedScopes } from "./grammar-files.js";
import { FLOW_MARK, KEYWORD, TYPE, markLines } from "./flow-keywords.js";

/**
 * The languages a documentation page actually uses.
 *
 * Loading every grammar Shiki ships would cost seconds and megabytes for
 * languages nobody writes here; `highlight.langs` extends this.
 */
const DEFAULT_LANGS = [
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

/**
 * `flow` and the shorthands people type in a fence, mapped to a real grammar.
 *
 * Only names that are *not* grammars in their own right: Shiki rejects an
 * alias pointing at itself, and `jsx`, `md` and `yml` are already loaded
 * languages or aliases it knows.
 */
const ALIASES = {
  flow   : "javascript",
  console: "shellscript",
};

/**
 * The fences both Flow passes apply to.
 *
 * Flow is JavaScript, and both passes read the source as JavaScript: the shim
 * rewrites JavaScript declarations, and the marking pass lexes strings and
 * comments to keep out of them. Neither means anything in another language. A
 * `match` in a Rust sample is Rust's and Rust's grammar has already coloured
 * it; the backticks that fence a code block inside an `mdx` sample are not an
 * unterminated template literal, though a JavaScript lexer pointed at them
 * says they are.
 *
 * `js` is here because Shiki's own alias table resolves it and this runs
 * before that.
 */
const FLOW_FENCES = new Set(["javascript", "js", "jsx"]);

/** The class each kind of mark carries, for the stylesheet to colour. */
const MARK_CLASSES = {
  [KEYWORD]: "uf-flow-keyword",
  [TYPE]   : "uf-flow-type",
};

/**
 * Where the shim's undo is parked between the two hooks that need it.
 *
 * Shiki builds a fresh context object for each block it highlights and calls
 * every hook with it, so `this.meta` is the one place a `preprocess` can leave
 * something for the `tokens` of the same block — and only that block. A field
 * on the transformer would be shared by every block in the document.
 *
 * Its absence is also how `tokens` knows the fence was not a Flow one: the
 * language is `preprocess`'s to see, and asking twice is how the two halves
 * would come to disagree about a block.
 */
const RESTORE = Symbol.for("uf.flowRestore");

/**
 * Give Flow's syntax the colours the grammar could not.
 *
 * Three hooks, one per phase: rewrite the declarations the grammar cannot
 * parse, undo that and mark Flow's words on the token stream, and put the
 * mark's class on the span.
 */
function flowSyntax() {
  return {
    name: "uf:flow-syntax",
    preprocess(code, options) {
      if (!FLOW_FENCES.has(ALIASES[options.lang] ?? options.lang)) {
        return code;
      }
      const shim = shimFlowGrammar(code);
      this.meta[RESTORE] = shim.restore;
      return shim.code;
    },
    tokens(lines) {
      const restore = this.meta[RESTORE];
      return restore == null ? lines : markLines(restore(lines));
    },
    span(node, _line, _col, _lineElement, token) {
      const added = MARK_CLASSES[token[FLOW_MARK]];
      if (added == null) {
        return;
      }
      const existing = node.properties.class;
      node.properties.class = existing == null ? added : `${existing} ${added}`;
    },
  };
}

/**
 * The rehype plugin entry, or `null` when a project turns highlighting off.
 *
 * Two themes rather than one, emitted as CSS variables (`defaultColor: false`),
 * because a page follows the reader's light or dark preference and a build-time
 * highlighter cannot know it. The stylesheet picks between them.
 *
 * A `langs` entry that is a path is a TextMate grammar file, read relative to
 * `root`; see `./grammar-files.js`. It is read here, once per build, so a file
 * that is missing or is not a grammar fails the build with its path rather
 * than leaving its fences grey.
 *
 * The highlighter is the plugin's own rather than Shiki's process-wide
 * singleton, which registers a language once per name and keeps it: a grammar
 * file edited during `uf dev` would otherwise go on colouring with the
 * patterns it had when the server started. `reloadGrammars` re-reads the
 * files and replaces it.
 *
 * @param {{enabled?: boolean, themes?: {light?: string, dark?: string}, langs?: string[]}} options
 * @param {{root?: string}} [context]
 */
export function highlightPlugin(options, context = {}) {
  const config = options ?? {};
  if (config.enabled === false) {
    return null;
  }
  // Each theme on its own, so a project that names one keeps uf's other
  // rather than handing Shiki half a pair.
  const themes = {
    light: config.themes?.light ?? "github-light",
    dark : config.themes?.dark ?? "github-dark-dimmed",
  };
  const root = context.root ?? process.cwd();
  const named = (config.langs ?? []).filter((entry) => !isGrammarFile(entry));
  const entries = (config.langs ?? []).filter(isGrammarFile);
  const files = entries.map((entry) => loadGrammar(entry, root));
  const langs = [...new Set([...DEFAULT_LANGS, ...named]), ...files];

  const shikiOptions = {
    themes,
    langs,
    langAlias   : ALIASES,
    defaultColor: false,
    // A fence naming a language Shiki does not have should render as plain
    // code, not fail the build.
    fallbackLanguage: "text",
    transformers    : [flowSyntax()],
  };
  OWNED.set(shikiOptions, { entries, root, handler: null, highlighter: null });

  return [rehypeOwnedShiki, shikiOptions];
}

/**
 * What each plugin instance owns, by the options object it was given: the
 * grammar files it reads, and the highlighter built from them.
 */
const OWNED = new WeakMap();

/**
 * Re-read the grammar files a plugin instance was configured with, and build
 * its highlighter afresh on the next page it compiles.
 *
 * Every check a grammar is held to — that it reads as one, and that its scope
 * is not one Shiki ships — runs before anything is replaced. A file that fails
 * rejects the promise, naming it, and leaves the languages and the working
 * highlighter as they were, so the pages still compile.
 *
 * @param {Object} options the options `highlightPlugin` returned
 * @returns {Promise<void>}
 */
export async function reloadGrammars(options) {
  const owned = OWNED.get(options);
  if (owned == null) {
    return;
  }
  const grammars = owned.entries.map((entry) => loadGrammar(entry, owned.root));
  await refuseShippedScopes(grammars, owned.entries, owned.root);
  options.langs = [...options.langs.filter((lang) => typeof lang !== "object"), ...grammars];
  owned.handler = null;
}

/**
 * The rehype plugin: Shiki's own transform, over a highlighter this instance
 * owns and builds on the first page it is asked to colour.
 */
function rehypeOwnedShiki(options) {
  const processor = this;
  return async (tree) => {
    const owned = OWNED.get(options);
    if (owned.handler == null) {
      const previous = owned.highlighter;
      owned.handler = ownHighlighter(options, owned).then((highlighter) => {
        owned.highlighter = highlighter;
        previous?.dispose();
        return rehypeShikiFromHighlighter.call(processor, highlighter, options);
      });
      // A handler that failed is not kept: the next page asks again, so a
      // grammar fixed in place is picked up without a restart.
      owned.handler.catch(() => {
        owned.handler = null;
      });
    }
    const handler = await owned.handler;
    return handler(tree);
  };
}

/**
 * Build the highlighter from one snapshot of the languages: the list checked
 * is the list registered, even when `reloadGrammars` replaces `options.langs`
 * while the check is awaited.
 */
async function ownHighlighter(options, owned) {
  const langs = options.langs;
  const grammars = langs.filter((lang) => typeof lang === "object");
  await refuseShippedScopes(grammars, owned.entries, owned.root);
  return createHighlighter({
    themes: Object.values(options.themes),
    langs,
    langAlias: options.langAlias,
  });
}
