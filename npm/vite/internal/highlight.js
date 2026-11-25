// @noflow
//
// Syntax highlighting for fenced code, at build time.
//
// uf's claim is that MDX works without a plugin list, and a documentation page
// whose code samples are undifferentiated grey does not meet it. So this is on
// by default, it runs during the build rather than shipping a highlighter to
// the browser, and it knows about Flow.
//
// The grammars are tree-sitter's, compiled to WebAssembly. The colours are not
// a theme engine's: uf ships two palettes, github-light and github-dark-dimmed,
// and writes both into every token as CSS variables. The stylesheet picks,
// because a build cannot know which the reader prefers.
//
// # Flow
//
// No maintained tree-sitter grammar covers Flow. The JavaScript one runs
// instead, and it fails at Flow's syntax in two different ways that need two
// different answers:
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
// the other. Writing and maintaining a Flow grammar would replace both; until
// one exists, this is the honest approximation, and its limits are recorded in
// each module's header.
//
// Marking rather than recolouring, because the colour is not knowable here.
// A class moves the decision to CSS, which is where the theme's colours
// already live.

import { Language, Parser, Query } from "web-tree-sitter";

import { shimFlowGrammar } from "./flow-grammar-shim.js";
import { grammarError, isGrammarFile, loadGrammar } from "./grammar-files.js";
import { FLOW_MARK, KEYWORD, TYPE, markLines } from "./flow-keywords.js";
import { DEFAULT_LANGS, languageFor, readShipped, shippedIds } from "./grammars.js";

/**
 * `flow` and the shorthands people type in a fence, mapped to a real grammar
 * by `languageFor`. Listed here because both Flow passes apply to them and to
 * nothing else: the shim rewrites JavaScript declarations, and the marking
 * pass lexes strings and comments to keep out of them. Neither means anything
 * in another language. A `match` in a Rust sample is Rust's.
 */
const FLOW_FENCES = new Set(["javascript", "js", "jsx", "flow"]);

/** The class each kind of mark carries, for the stylesheet to colour. */
const MARK_CLASSES = {
  [KEYWORD]: "uf-flow-keyword",
  [TYPE]   : "uf-flow-type",
};

/**
 * The capture's head, before the first `.`, as one of the palette's groups.
 *
 * A capture the palette has no row for is plain. `variable` is plain on
 * purpose: an identifier is the text's own colour, and painting it is how a
 * sample turns into confetti.
 */
const GROUP_OF = {
  keyword    : "keyword",
  string     : "string",
  comment    : "comment",
  number     : "number",
  function   : "function",
  type       : "type",
  tag        : "tag",
  attribute  : "attribute",
  constant   : "constant",
  operator   : "operator",
  punctuation: "punctuation",
  property   : "property",
  variable   : "plain",
  label      : "plain",
  embedded   : "string",
  markup     : "keyword",
  module     : "type",
  namespace  : "type",
  boolean    : "constant",
  constructor: "function",
};

/**
 * The two palettes uf ships, as the colours the stylesheet already used for
 * these themes. A project that names another theme is refused when a fence is
 * coloured, not when the plugin is built: the config stores the name, and a
 * build with no fence never asks.
 */
const PALETTES = {
  "github-light": {
    plain      : "#24292e",
    keyword    : "#d73a49",
    string     : "#032f62",
    comment    : "#6a737d",
    number     : "#005cc5",
    function   : "#6f42c1",
    type       : "#005cc5",
    tag        : "#22863a",
    attribute  : "#6f42c1",
    constant   : "#005cc5",
    operator   : "#d73a49",
    punctuation: "#24292e",
    property   : "#005cc5",
  },
  "github-dark-dimmed": {
    plain      : "#adbac7",
    keyword    : "#f47067",
    string     : "#96d0ff",
    comment    : "#768390",
    number     : "#6cb6ff",
    function   : "#dcbdfb",
    type       : "#6cb6ff",
    tag        : "#8ddb8c",
    attribute  : "#dcbdfb",
    constant   : "#6cb6ff",
    operator   : "#f47067",
    punctuation: "#adbac7",
    property   : "#6cb6ff",
  },
};

/**
 * The rehype plugin entry, or `null` when a project turns highlighting off.
 *
 * A `langs` entry that is a path is a tree-sitter grammar, read relative to
 * `root`; see `./grammar-files.js`. It is read here, once per build, so a file
 * that is missing or is not a grammar fails the build with its path rather
 * than leaving its fences grey. A name uf does not ship fails here too: there
 * is no catalogue to fetch it from.
 *
 * The grammars are this plugin's own rather than a process-wide registry. A
 * grammar file edited during `uf dev` would otherwise go on colouring with
 * the query it had when the server started. `reloadGrammars` re-reads the
 * files and replaces them.
 *
 * @param {{enabled?: boolean, themes?: {light?: string, dark?: string}, langs?: string[]}} options
 * @param {{root?: string}} [context]
 */
export function highlightPlugin(options, context = {}) {
  const config = options ?? {};
  if (config.enabled === false) {
    return null;
  }
  // Each theme on its own, so a project that names one keeps uf's other.
  const themes = {
    light: config.themes?.light ?? "github-light",
    dark : config.themes?.dark ?? "github-dark-dimmed",
  };
  const root = context.root ?? process.cwd();
  const listed = config.langs ?? [];
  const named = listed.filter((entry) => !isGrammarFile(entry));
  const entries = listed.filter(isGrammarFile);
  const unknown = named.find((name) => languageFor(name) == null);
  if (unknown != null) {
    throw new Error(
      `uf: app.builtins.markdown.mdx.highlight.langs names "${unknown}", and uf ships ${shippedIds().join(", ")}`,
    );
  }
  const grammars = entries.map((entry) => loadGrammar(entry, root));
  const langs = [
    ...new Set([...DEFAULT_LANGS, ...named, ...grammars.map((grammar) => grammar.name)]),
  ];

  const highlightOptions = { themes, langs };
  OWNED.set(highlightOptions, {
    entries,
    root,
    grammars,
    registry: null,
    pending : null,
    token   : null,
  });
  return [rehypeHighlight, highlightOptions];
}

/**
 * What each plugin instance owns, by the options object it was given: the
 * grammar files it reads, and the registry built from them.
 */
const OWNED = new WeakMap();

/**
 * Re-read the grammar files a plugin instance was configured with, and build
 * its registry afresh.
 *
 * Every check a grammar is held to runs before anything is replaced. A file
 * that fails rejects the promise, naming it, and leaves the languages and the
 * working registry as they were, so the pages still compile.
 *
 * @param {Object} options the options `highlightPlugin` returned
 * @returns {Promise<void>}
 */
export async function reloadGrammars(options) {
  const owned = OWNED.get(options);
  if (owned == null) {
    return;
  }
  // Synchronous, and before `pending` moves: a file that does not read as a
  // grammar never disturbs the registry the pages are using.
  const grammars = owned.entries.map((entry) => loadGrammar(entry, owned.root));
  await beginCompile(owned, grammars);
}

/**
 * The rehype plugin: walk every fenced block and colour it.
 *
 * The registry is built on the first block, not when the plugin is created,
 * because loading the wasm modules is the slow part and a build that never
 * reaches a fence should not pay it. A registry that failed to build is not
 * kept: the next page asks again, so a grammar fixed in place is picked up
 * without a restart.
 */
function rehypeHighlight(options) {
  return async (tree) => {
    const owned = OWNED.get(options);
    await colorTree(owned, tree, options.themes);
  };
}

/** One parser for the process. `Parser.init` loads the binding's own wasm. */
let wasmReady = null;

function initWasm() {
  wasmReady ??= Parser.init().catch((error) => {
    wasmReady = null;
    throw error;
  });
  return wasmReady;
}

/**
 * Compile every grammar this instance highlights with.
 *
 * One registry owns one parser. `setLanguage` on a shared parser is a race
 * when two pages colour at once, so a registry also owns the queue that
 * serialises its parses. A custom query tree-sitter refuses names the file;
 * a vendored one that fails is a bug in what uf shipped.
 *
 * @param {Array<{name: string, wasm: Uint8Array, query: string, entry?: string, root?: string, queryFile?: string}>} grammars
 */
async function compileRegistry(grammars) {
  await initWasm();
  const languages = new Map();
  const compiled = [];
  try {
    for (const read of readShipped()) {
      const entry = await compileOne(read.wasm, read.query);
      languages.set(read.id, entry);
      compiled.push(entry);
    }
    for (const grammar of grammars) {
      const entry = await compileOne(grammar.wasm, grammar.query, grammar);
      languages.set(grammar.name, entry);
      compiled.push(entry);
    }
  } catch (error) {
    for (const entry of compiled) entry.query.delete();
    throw error;
  }
  return {
    languages,
    parser  : new Parser(),
    tail    : Promise.resolve(),
    disposed: false,
  };
}

/**
 * Load one grammar's wasm and compile its query.
 *
 * The bytes are copied into a fresh `Uint8Array`. `Language.load` of a path
 * would cache by that path, and a grammar edited during `uf dev` would come
 * back as the module it was the first time.
 */
async function compileOne(wasm, source, grammar) {
  const bytes = new Uint8Array(wasm);
  let language;
  try {
    language = await Language.load(bytes);
  } catch (error) {
    if (grammar == null) throw error;
    throw grammarError(
      grammar.entry,
      grammar.root,
      `is not a tree-sitter grammar: ${error?.message ?? error}`,
    );
  }
  try {
    return { language, query: new Query(language, source) };
  } catch (error) {
    // `Language` in web-tree-sitter 0.27 has no `delete`. A query that fails
    // to compile leaves its module to the process, which is one refused reload.
    if (grammar == null) throw error;
    const file = grammar.queryFile?.split(/[\\/]/).pop() ?? "highlights.scm";
    throw grammarError(
      grammar.entry,
      grammar.root,
      `has a highlight query tree-sitter refused (${file}): ${error?.message ?? error}`,
    );
  }
}

/**
 * Start a compile, and make it the one a later page waits for.
 *
 * A compile that loses a race with a newer one is dropped. A compile that
 * fails puts the previous registry back, so a reload that was refused does
 * not take the pages' highlighter with it.
 */
function beginCompile(owned, grammars) {
  const token = Symbol();
  owned.token = token;
  const pending = compileRegistry(grammars).then(
    (registry) => {
      if (owned.token !== token) {
        disposeLater(registry);
        return owned.registry;
      }
      const previous = owned.registry;
      owned.registry = registry;
      owned.grammars = grammars;
      if (previous != null) disposeLater(previous);
      return registry;
    },
    (error) => {
      if (owned.token === token) {
        owned.pending = owned.registry != null ? Promise.resolve(owned.registry) : null;
      }
      throw error;
    },
  );
  owned.pending = pending;
  return pending;
}

/** Free a registry once whatever is already queued on it has finished. */
function disposeLater(registry) {
  if (registry == null || registry.disposed) return;
  registry.disposed = true;
  registry.tail = registry.tail.then(() => {
    registry.parser.delete();
    for (const entry of registry.languages.values()) entry.query.delete();
  });
}

/**
 * Run `fn` on the registry the instance currently owns.
 *
 * The check and the enqueue have no await between them. A reload can replace
 * the registry only at an await, so the function is queued on a registry
 * before that registry can be disposed, and the disposal waits behind it.
 */
async function withRegistry(owned, fn) {
  for (;;) {
    if (owned.pending == null) beginCompile(owned, owned.grammars);
    const token = owned.token;
    const pending = owned.pending;
    const registry = await pending;
    if (registry == null || registry.disposed || owned.token !== token) continue;
    const run = registry.tail.then(() => fn(registry));
    registry.tail = run.then(
      () => undefined,
      () => undefined,
    );
    return run;
  }
}

/** Every `pre > code` in the tree, coloured in place. */
async function colorTree(owned, node, themes) {
  if (node == null || typeof node !== "object") return;
  if (node.type === "element" && node.tagName === "pre") {
    const code = node.children?.find(
      (child) => child?.type === "element" && child.tagName === "code",
    );
    if (code != null) await colorCode(owned, node, code, themes);
  }
  for (const child of node.children ?? []) {
    await colorTree(owned, child, themes);
  }
}

/** One fenced block. An unknown theme is refused here, where a fence is coloured. */
async function colorCode(owned, pre, code, themes) {
  const colors = {
    light: palette(themes.light, "light"),
    dark : palette(themes.dark, "dark"),
  };
  const source = textContent(code);
  const fence = languageOf(code.properties);
  const lines = await withRegistry(owned, (registry) => highlightedLines(registry, fence, source));
  paint(pre, code, lines, colors);
}

function palette(name, which) {
  const found = PALETTES[name];
  if (found == null) {
    throw new Error(
      `uf: app.builtins.markdown.mdx.highlight.themes.${which} is "${name}", and uf ships github-light and github-dark-dimmed`,
    );
  }
  return found;
}

/**
 * The lines of `source` as the grammar tokenised them.
 *
 * A fence uf does not know is plain text, not an error: a documentation page
 * may show a language it does not highlight. A Flow fence is the JavaScript
 * grammar, with the shim in front and the marking pass behind.
 */
function highlightedLines(registry, fence, source) {
  const id = grammarId(registry, fence);
  const flow = fence != null && FLOW_FENCES.has(fence) && id === "javascript";
  if (id == null) return plainLines(source);
  const shim = flow ? shimFlowGrammar(source) : null;
  const text = shim?.code ?? source;
  const compiled = registry.languages.get(id);
  registry.parser.setLanguage(compiled.language);
  const tree = registry.parser.parse(text);
  if (tree == null) return plainLines(source);
  try {
    let lines = segments(text, compiled.query.captures(tree.rootNode));
    if (shim != null) lines = shim.restore(lines);
    if (flow) lines = markLines(lines);
    return lines;
  } finally {
    tree.delete();
  }
}

/** The grammar a fence names, or `null` when the block is plain text. */
function grammarId(registry, fence) {
  if (fence == null) return null;
  const shipped = languageFor(fence);
  if (shipped != null) return shipped.id;
  return registry.languages.has(fence) ? fence : null;
}

/** `source` as one uncoloured run per line. An empty block is one empty line. */
function plainLines(source) {
  if (source.length === 0) return [[]];
  return linesFrom([{ content: source, offset: 0, name: null }]);
}

/**
 * Cut `source` wherever a capture starts or ends, and keep the smallest
 * capture that covers each piece. Two captures of the same length: the later
 * one in the query wins, which is the one the query wrote as the refinement.
 *
 * The indexes tree-sitter reports are indexes into the JavaScript string, so
 * the pieces are sliced from `source` directly. A piece is then split on
 * newlines. The newline itself is not a token; the line break is how the
 * block is painted, and putting it in a token would copy it twice.
 */
function segments(source, captures) {
  if (source.length === 0) return [[]];
  const points = new Set([0, source.length]);
  const spans = [];
  captures.forEach((capture, index) => {
    const start = capture.node.startIndex;
    const end = capture.node.endIndex;
    if (end <= start || start >= source.length || end <= 0) return;
    const from = Math.max(0, start);
    const to = Math.min(source.length, end);
    points.add(from);
    points.add(to);
    spans.push({ name: capture.name, start: from, end: to, index });
  });
  const cuts = [...points].sort((left, right) => left - right);
  const parts = [];
  for (let index = 0; index < cuts.length - 1; index += 1) {
    const start = cuts[index];
    const end = cuts[index + 1];
    if (end <= start) continue;
    let best = null;
    for (const span of spans) {
      if (span.start <= start && span.end >= end) {
        const length = span.end - span.start;
        if (
          best == null ||
          length < best.length ||
          (length === best.length && span.index > best.index)
        ) {
          best = { length, index: span.index, name: span.name };
        }
      }
    }
    parts.push({ content: source.slice(start, end), offset: start, name: best?.name ?? null });
  }
  return linesFrom(parts);
}

/** Parts tiled across `source`, broken into lines. A trailing newline yields an empty line. */
function linesFrom(parts) {
  const lines = [[]];
  for (const part of parts) {
    let offset = part.offset;
    const pieces = part.content.split("\n");
    for (let index = 0; index < pieces.length; index += 1) {
      if (index > 0) {
        lines.push([]);
        offset += 1;
      }
      const content = pieces[index];
      if (content === "") continue;
      lines[lines.length - 1].push({ content, offset, name: part.name });
      offset += content.length;
    }
  }
  return lines;
}

/** Write the coloured lines back onto the block the reader copies from. */
function paint(pre, code, lines, colors) {
  pre.properties ??= {};
  addClass(pre.properties, "uf-highlight");
  pre.properties.style = cssVars(colors.light.plain, colors.dark.plain);
  const children = [];
  lines.forEach((line, index) => {
    if (index > 0) children.push({ type: "text", value: "\n" });
    children.push({
      type      : "element",
      tagName   : "span",
      properties: { className: ["line"] },
      children  : line.map((token) => tokenElement(token, colors)),
    });
  });
  code.children = children;
}

function tokenElement(token, colors) {
  const marked = token[FLOW_MARK];
  const group = marked === KEYWORD ? "keyword" : marked === TYPE ? "type" : groupOf(token.name);
  const properties = { style: cssVars(colors.light[group], colors.dark[group]) };
  const added = MARK_CLASSES[marked];
  if (added != null) properties.className = [added];
  return {
    type   : "element",
    tagName: "span",
    properties,
    children: [{ type: "text", value: token.content }],
  };
}

function groupOf(name) {
  if (name == null) return "plain";
  return GROUP_OF[name.split(".")[0]] ?? "plain";
}

function cssVars(light, dark) {
  return `--uf-highlight-light:${light};--uf-highlight-dark:${dark}`;
}

function addClass(properties, name) {
  const current = properties.className;
  const list =
    current == null
      ? []
      : Array.isArray(current)
        ? current.map(String)
        : String(current).split(/\s+/).filter(Boolean);
  if (!list.includes(name)) list.push(name);
  properties.className = list;
}

/** The text of a hast node, which is what the grammar tokenises. */
function textContent(node) {
  if (node.type === "text") return node.value ?? "";
  return (node.children ?? []).map(textContent).join("");
}

/** The language a fence declared, from `language-*` or `lang-*`. */
function languageOf(properties) {
  const raw = properties?.className ?? properties?.class ?? [];
  const names = Array.isArray(raw) ? raw : String(raw).split(/\s+/);
  for (const name of names) {
    const match = /^(?:language|lang)-(\S+)$/.exec(String(name));
    if (match != null) return match[1];
  }
  return null;
}
