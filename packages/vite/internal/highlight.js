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
// No highlighter has a Flow grammar, and Flow's declaration keywords —
// `component`, `hook`, `renders`, `match`, `enum` — are ordinary identifiers to
// a JavaScript one. They would be the only uncoloured words in a uf code
// sample, which is the wrong way round: they are the reason the sample is
// there.
//
// Rather than write and maintain a TextMate grammar, `flowKeywords` re-tags
// those tokens after the JavaScript grammar has run. It only recolours a token
// whose whole text is one of the words, so `components` and `matcher` are left
// alone, and it does not touch tokens inside a string or a comment because the
// grammar has already given those their own colour.

import rehypeShiki from "@shikijs/rehype";

/**
 * Flow's declaration keywords, which a JavaScript grammar reads as names.
 *
 * Whole words only, so `components`, `matcher` and `hooks` stay identifiers,
 * and never after a dot, so `text.match(…)` and `list.hook` are left alone.
 * The grammar splits a line into tokens wherever it likes — `match` arrives at
 * the end of its own token, with the `(` that follows it in the next one — so
 * the rule has to be decidable from the characters around the word alone,
 * which is why it looks behind rather than ahead.
 *
 * `enum` is not here: every JavaScript grammar already treats it as a keyword.
 */
const FLOW_KEYWORD_PATTERN =
  /(?<![.\w$])(?:component|hook|renders|match|opaque|mixed|empty)(?![\w$])/g;



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
 * Give Flow's keywords a class the stylesheet can colour.
 *
 * A JavaScript grammar reads `component` as a name, and does not even put it in
 * a token of its own: `export component Avatar(…) {` arrives as the keyword
 * `export` followed by one long token holding all the rest. So `tokens` splits
 * that token around each Flow keyword, on whole-word boundaries, and `span`
 * marks the resulting pieces.
 *
 * Marking rather than recolouring, because the colour is not knowable here.
 * Copying the colour from another keyword in the same block was the first
 * attempt and it fails exactly where it matters: a snippet that is nothing but
 * `component Tab(label: string) renders React.Node` contains no keyword the
 * grammar recognises, so there was nothing to copy from. A class moves the
 * decision to CSS, which is where the theme's colours already live.
 */
function flowKeywords() {
  return {
    name: "uf:flow-keywords",
    tokens(lines) {
      return lines.map((line) => line.flatMap(split));
    },
    span(node, _line, _col, _lineElement, token) {
      if (!isFlowKeyword(token.content)) {
        return;
      }
      const existing = node.properties.class;
      node.properties.class =
        existing == null ? KEYWORD_CLASS : `${existing} ${KEYWORD_CLASS}`;
    },
  };
}

/** The class a Flow keyword's span carries. */
const KEYWORD_CLASS = "uf-flow-keyword";

/** Whether a token is exactly one Flow keyword, after splitting. */
function isFlowKeyword(content) {
  return [...content.matchAll(FLOW_KEYWORD_PATTERN)].some(
    (match) => match[0].length === content.length,
  );
}

/**
 * One token, split around any Flow keyword inside it.
 *
 * `offset` is carried through for every piece, because Shiki and any other
 * transformer use it to map a token back to the source; a piece with the wrong
 * offset breaks anything that reads positions, such as line highlighting.
 */
function split(token) {
  const text = token.content;
  // `matchAll` works on its own copy of the pattern. `test` would not: on a
  // global regex it advances `lastIndex`, so every second call would start
  // partway through an unrelated string and miss.
  const matches = text.length === 0 ? [] : [...text.matchAll(FLOW_KEYWORD_PATTERN)];
  if (matches.length === 0) {
    return [token];
  }

  const pieces = [];
  let index = 0;
  for (const match of matches) {
    const at = match.index ?? 0;
    if (at > index) {
      pieces.push(piece(token, text.slice(index, at), index));
    }
    pieces.push(piece(token, match[0], at));
    index = at + match[0].length;
  }
  if (index < text.length) {
    pieces.push(piece(token, text.slice(index), index));
  }
  return pieces;
}

function piece(token, content, at) {
  return {
    ...token,
    content,
    offset: (token.offset ?? 0) + at,
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
      transformers: [flowKeywords()],
    },
  ];
}
