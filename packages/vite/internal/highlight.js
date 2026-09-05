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
 * Whole words only, so `components`, `matcher` and `hooks` stay identifiers.
 *
 * # Why the context is not in this pattern
 *
 * It used to be: a `(?<![.\w$])` look-behind was supposed to leave
 * `text.match(…)` alone. It does not, and cannot, because the grammar splits
 * a line into tokens wherever it likes and the look-behind only ever sees
 * the token it is matching inside. Given `text.` and `match` as two tokens —
 * which is exactly what happens — the look-behind is at the start of a
 * string and matches happily, and `text.match(re)` came out with `match`
 * coloured as a keyword. `{ hook: 1 }` had the same problem from the other
 * side.
 *
 * So the pattern finds candidates and {@link isKeywordAt} decides, against
 * the whole line. `tokens` is given the line, so the context is there; it
 * was only ever the per-token matching that threw it away.
 *
 * `enum` is not here: every JavaScript grammar already treats it as a keyword.
 */
const FLOW_KEYWORD_PATTERN =
  /(?<![\w$])(?:component|hook|renders|match|opaque|mixed|empty)(?![\w$])/g;

/**
 * Whether the candidate at `[start, end)` in `line` is really a keyword.
 *
 * Two rejections, both from real code in this repository's own
 * documentation:
 *
 * * **After a dot.** `text.match(re)`, `list.hook`. A member name is never a
 *   declaration keyword.
 * * **Before a colon.** `{ hook: 1 }`, `{ +renders: Node }`. A property name
 *   is not either, in a value or in a type.
 */
function isKeywordAt(line, start, end) {
  const before = line.slice(0, start).trimEnd();
  if (before.endsWith(".") || before.endsWith("?.")) {
    return false;
  }
  const after = line.slice(end).trimStart();
  return !after.startsWith(":");
}

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
      return lines.map(markLine);
    },
    span(node, _line, _col, _lineElement, token) {
      if (token[KEYWORD_FLAG] !== true) {
        return;
      }
      const existing = node.properties.class;
      node.properties.class = existing == null ? KEYWORD_CLASS : `${existing} ${KEYWORD_CLASS}`;
    },
  };
}

/** The class a Flow keyword's span carries. */
const KEYWORD_CLASS = "uf-flow-keyword";

/**
 * The mark `tokens` leaves for `span` to read.
 *
 * A flag rather than re-deciding in `span`, because `span` is given one token
 * and the decision needs the line. Deciding once, where the context is, is
 * also the only way the two can never disagree.
 */
const KEYWORD_FLAG = Symbol.for("uf.flowKeyword");

/**
 * One line of tokens, split around the Flow keywords in it.
 *
 * Exported because it is where the whole decision lives and it is testable
 * without starting Shiki: give it the token split a grammar would produce
 * and it says which words it marked. `tests/library/highlight.test.js` uses
 * exactly that, and the splits in it are the ones that had bugs.
 *
 * The line is reassembled from its tokens so that {@link isKeywordAt} can see
 * across the boundaries the grammar happened to draw, and the resulting
 * offsets are line-relative for exactly as long as it takes to slice the
 * tokens up again.
 */
export function markLine(line) {
  const text = line.map((token) => token.content).join("");
  const keywords = [];
  for (const match of text.matchAll(FLOW_KEYWORD_PATTERN)) {
    const start = match.index ?? 0;
    const end = start + match[0].length;
    if (isKeywordAt(text, start, end)) {
      keywords.push([start, end]);
    }
  }
  if (keywords.length === 0) {
    return line;
  }

  const out = [];
  let at = 0;
  for (const token of line) {
    const start = at;
    const end = at + token.content.length;
    at = end;

    // The cut points inside this token, in order.
    const cuts = [];
    for (const [from, to] of keywords) {
      if (from < end && to > start) {
        cuts.push([Math.max(from, start), Math.min(to, end)]);
      }
    }
    if (cuts.length === 0) {
      out.push(token);
      continue;
    }

    let index = start;
    for (const [from, to] of cuts) {
      if (from > index) {
        out.push(piece(token, text.slice(index, from), index - start));
      }
      out.push({ ...piece(token, text.slice(from, to), from - start), [KEYWORD_FLAG]: true });
      index = to;
    }
    if (index < end) {
      out.push(piece(token, text.slice(index, end), index - start));
    }
  }
  return out;
}

/**
 * A slice of one token.
 *
 * `offset` is carried through for every piece, because Shiki and any other
 * transformer use it to map a token back to the source; a piece with the wrong
 * offset breaks anything that reads positions, such as line highlighting.
 */
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
