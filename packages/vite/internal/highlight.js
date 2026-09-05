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

import rehypeShiki from "@shikijs/rehype";

import { shimFlowGrammar } from "./flow-grammar-shim.js";
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
  flow: "javascript",
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
  [TYPE]: "uf-flow-type",
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
 * @param {{enabled?: boolean, themes?: {light: string, dark: string}, langs?: string[]}} options
 */
export function highlightPlugin(options) {
  const config = options ?? {};
  if (config.enabled === false) {
    return null;
  }
  const themes = config.themes ?? { light: "github-light", dark: "github-dark-dimmed" };
  const langs = [...new Set([...DEFAULT_LANGS, ...(config.langs ?? [])])];

  return [
    rehypeShiki,
    {
      themes,
      langs,
      langAlias: ALIASES,
      defaultColor: false,
      // A fence naming a language Shiki does not have should render as plain
      // code, not fail the build.
      fallbackLanguage: "text",
      transformers: [flowSyntax()],
    },
  ];
}
