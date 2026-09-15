// @noflow
//
// Plain JavaScript, for the loader's reason: this module imports a project's
// rule plugins through the Flow loader, so it cannot itself be one of the
// modules that loader has to transform first.
//
// The rule runtime behind `uf lint`'s project rules. `../lint-worker.js` owns
// the process and the protocol; this module owns what a rule sees, and it is
// pure, so it is tested without a process.
//
// What a rule sees is ESLint's shape — `meta`, `create(context)` returning
// listeners keyed by ESTree node type, `context.report`, a fixer — over the
// tree uf's own parser produced. ESLint's rather than a shape of uf's own,
// because the point of project rules is that a team does not leave its rules
// behind, and the rules a team has are ESLint rules. The subset is the one
// those rules are written against: node-type listeners and `:exit`, `report`
// with a `node` or a `loc`, `message` or `messageId` with `data`, a `fix`
// returning one edit or several, and `sourceCode.getText`. What is absent is
// absent loudly: a rule that calls `getScope` or asks for tokens throws, and a
// rule that throws is named in the report rather than skipped in silence.

import { createRequire } from "node:module";
import path from "node:path";
import { pathToFileURL } from "node:url";

/**
 * Import every plugin module and pick out the rules `wanted` names.
 *
 * A plugin is a module whose default export has `rules` and a `name`; a rule's
 * id is `name/rule`. A module without `rules` is a build plugin and is passed
 * over without comment, because most entries in `plugins` are. Everything
 * that stops an enabled rule from running — an import that throws, a rule
 * with no `create`, an id no plugin defines — comes back as a sentence in
 * `problems`, and the driver fails the run on it.
 */
export async function loadRules(root, modules, wanted) {
  const defined = new Map();
  const problems = [];
  // Bare specifiers resolve from the project, not from this package: under a
  // strict layout the project's dependencies are not visible from
  // `@uniflowed/host`'s own directory.
  const resolver = createRequire(path.join(root, "package.json"));
  for (const specifier of modules) {
    let namespace;
    try {
      const file = path.isAbsolute(specifier) ? specifier : resolver.resolve(specifier);
      namespace = await import(pathToFileURL(file).href);
    } catch (error) {
      problems.push(`plugin \`${specifier}\` could not be imported: ${describe(error)}`);
      continue;
    }
    const plugin = namespace.default ?? namespace;
    if (plugin === null || typeof plugin !== "object" || plugin.rules == null) {
      continue;
    }
    const name = typeof plugin.meta?.name === "string" ? plugin.meta.name : plugin.name;
    if (typeof name !== "string" || name === "" || name.includes("/")) {
      problems.push(
        `plugin \`${specifier}\` exports \`rules\` without a \`name\` to put before them, or with a \`/\` in it`,
      );
      continue;
    }
    for (const [ruleName, rule] of Object.entries(plugin.rules)) {
      if (rule === null || typeof rule !== "object" || typeof rule.create !== "function") {
        problems.push(`\`${name}/${ruleName}\` in \`${specifier}\` has no \`create\` function`);
        continue;
      }
      defined.set(`${name}/${ruleName}`, rule);
    }
  }
  const rules = [];
  for (const id of wanted) {
    const rule = defined.get(id);
    if (rule === undefined) {
      problems.push(
        `\`${id}\` is enabled in \`lint.rules\`, and no plugin in \`plugins\` defines it`,
      );
    } else {
      rules.push({ id, rule });
    }
  }
  return { root, rules, problems };
}

/**
 * Run every loaded rule over one file.
 *
 * `file` is `{ path, filename, source, ast }`. Offsets in what comes back are
 * JavaScript string indices — the unit ESTree's `range` is in — and the driver
 * turns them into lines and byte columns. `micros` is the time spent inside
 * each rule's own code, `create` included, so the report can name the rule a
 * slow run was slow in.
 */
export function lintFile(loaded, file) {
  const lineStarts = lineStartsOf(file.source);
  const sourceCode = sourceCodeOf(file.source, file.ast, lineStarts);
  const diagnostics = [];
  const problems = [];
  const micros = {};
  const failed = new Set();
  const listeners = new Map();

  const timed = (id, work) => {
    const started = performance.now();
    try {
      return work();
    } catch (error) {
      // One sentence per rule per file: a listener that throws on every
      // identifier would otherwise bury the report in the same line.
      if (!failed.has(id)) {
        failed.add(id);
        problems.push(`\`${id}\` threw on ${file.path}: ${describe(error)}`);
      }
      return undefined;
    } finally {
      micros[id] = (micros[id] ?? 0) + (performance.now() - started) * 1000;
    }
  };

  for (const { id, rule } of loaded.rules) {
    const context = contextFor(id, rule, loaded.root, file, sourceCode, lineStarts, diagnostics);
    const handlers = timed(id, () => rule.create(context));
    if (handlers === null || typeof handlers !== "object") {
      continue;
    }
    for (const [key, handler] of Object.entries(handlers)) {
      if (typeof handler !== "function") {
        continue;
      }
      const entry = { id, handler };
      const list = listeners.get(key);
      if (list === undefined) {
        listeners.set(key, [entry]);
      } else {
        list.push(entry);
      }
    }
  }

  if (listeners.size > 0) {
    walk(file.ast, (key, node) => {
      const list = listeners.get(key);
      if (list === undefined) {
        return;
      }
      for (const { id, handler } of list) {
        if (!failed.has(id)) {
          timed(id, () => handler(node));
        }
      }
    });
  }
  return { diagnostics, micros, problems };
}

/** Keys that hold positions, back-references or trivia rather than children. */
const NOT_CHILDREN = new Set(["comments", "errors", "loc", "parent", "range", "tokens"]);

/**
 * Visit every node depth-first, calling `visit(type, node)` on the way in and
 * `visit(type + ":exit", node)` on the way out, with `node.parent` set.
 *
 * Children are visited in source order, sorted by `range`, rather than in the
 * order their keys come in: the tree arrives through JSON, and a map's key
 * order is not the order the code was written in. An explicit stack, because a
 * recursive walk is a stack overflow waiting for a deeply nested file.
 */
export function walk(root, visit) {
  const stack = [{ node: root, parent: null, exiting: false }];
  while (stack.length > 0) {
    const frame = stack.pop();
    const { node } = frame;
    if (frame.exiting) {
      visit(`${node.type}:exit`, node);
      continue;
    }
    node.parent = frame.parent;
    visit(node.type, node);
    stack.push({ node, parent: frame.parent, exiting: true });
    const children = [];
    for (const key of Object.keys(node)) {
      if (NOT_CHILDREN.has(key)) {
        continue;
      }
      const value = node[key];
      if (Array.isArray(value)) {
        for (const item of value) {
          if (isNode(item)) {
            children.push(item);
          }
        }
      } else if (isNode(value)) {
        children.push(value);
      }
    }
    children.sort((a, b) => (a.range?.[0] ?? 0) - (b.range?.[0] ?? 0));
    for (let index = children.length - 1; index >= 0; index -= 1) {
      stack.push({ node: children[index], parent: node, exiting: false });
    }
  }
}

function isNode(value) {
  return value !== null && typeof value === "object" && typeof value.type === "string";
}

function contextFor(id, rule, root, file, sourceCode, lineStarts, diagnostics) {
  const messages = rule.meta?.messages ?? {};
  // `"code"`, `"whitespace"`, or nothing: which of the two it is decides a
  // fix's tier on the Rust side, so the word is carried rather than a flag.
  const fixable = rule.meta?.fixable ?? null;
  return {
    id,
    // `lint.rules` takes a level and nothing else, so there are no options to
    // hand over; an empty list is what a rule reading `context.options[0]`
    // defensively expects.
    options: [],
    settings: {},
    cwd: root,
    filename: file.filename,
    physicalFilename: file.filename,
    sourceCode,
    getCwd: () => root,
    getFilename: () => file.filename,
    getSourceCode: () => sourceCode,
    report(descriptor) {
      diagnostics.push(diagnosticOf(id, descriptor, messages, fixable, file.source, lineStarts));
    },
  };
}

function diagnosticOf(id, descriptor, messages, fixable, source, lineStarts) {
  if (descriptor === null || typeof descriptor !== "object") {
    throw new TypeError("`context.report` takes an object");
  }
  let message = descriptor.message;
  if (message === undefined && descriptor.messageId !== undefined) {
    message = messages[descriptor.messageId];
    if (typeof message !== "string") {
      throw new TypeError(`\`messageId\` \`${descriptor.messageId}\` is not in \`meta.messages\``);
    }
  }
  if (typeof message !== "string") {
    throw new TypeError("a report needs a `message` or a `messageId`");
  }
  if (descriptor.data != null) {
    message = interpolate(message, descriptor.data);
  }
  const [start, end] = rangeOf(descriptor, lineStarts);
  let fix = null;
  if (typeof descriptor.fix === "function") {
    // ESLint's rule, and the reason for it: a fix nobody declared is a fix
    // nobody reviewed as one.
    if (!fixable) {
      throw new TypeError("a rule that reports a fix must set `meta.fixable`");
    }
    fix = mergeFixes(descriptor.fix(FIXER), source);
    if (fix !== null) {
      // The rule's own word for what its edits do, which is how uf decides
      // the tier: layout-only edits are `--fix`'s, code is `--fix-unsafe`'s.
      fix.kind = fixable === "whitespace" ? "whitespace" : "code";
    }
  }
  return { rule: id, message, start, end, fix };
}

/** `{{ name }}` placeholders, filled from `data`; one it has no value for is left as written. */
export function interpolate(text, data) {
  return text.replace(/\{\{\s*([^{}]+?)\s*\}\}/g, (whole, key) =>
    Object.hasOwn(data, key) ? String(data[key]) : whole,
  );
}

function rangeOf(descriptor, lineStarts) {
  const range = descriptor.node?.range;
  if (Array.isArray(range)) {
    return [range[0], range[1]];
  }
  const loc = descriptor.loc;
  if (loc != null && loc.start != null) {
    return [offsetOf(loc.start, lineStarts), offsetOf(loc.end ?? loc.start, lineStarts)];
  }
  if (loc != null && loc.line != null) {
    const at = offsetOf(loc, lineStarts);
    return [at, at];
  }
  throw new TypeError("a report needs a `node` with a `range`, or a `loc`");
}

/** ESLint's convention for a location: lines count from 1, columns from 0. */
function offsetOf({ line, column }, lineStarts) {
  const index = Math.min(Math.max(line, 1), lineStarts.length) - 1;
  return lineStarts[index] + Math.max(column, 0);
}

/** Where each line starts; `\r\n`, `\n` and a lone `\r` each end one. */
export function lineStartsOf(source) {
  const starts = [0];
  for (let index = 0; index < source.length; index += 1) {
    const code = source.charCodeAt(index);
    if (code === 13 && source.charCodeAt(index + 1) === 10) {
      index += 1;
      starts.push(index + 1);
    } else if (code === 10 || code === 13) {
      starts.push(index + 1);
    }
  }
  return starts;
}

function sourceCodeOf(text, ast, lineStarts) {
  return {
    text,
    ast,
    lines: text.split(/\r\n|[\r\n]/),
    getText(node, before = 0, after = 0) {
      if (node == null) {
        return text;
      }
      return text.slice(Math.max(node.range[0] - before, 0), node.range[1] + after);
    },
    getLocFromIndex(index) {
      let low = 0;
      let high = lineStarts.length - 1;
      while (low < high) {
        const middle = (low + high + 1) >> 1;
        if (lineStarts[middle] <= index) {
          low = middle;
        } else {
          high = middle - 1;
        }
      }
      return { line: low + 1, column: index - lineStarts[low] };
    },
    getIndexFromLoc(loc) {
      return offsetOf(loc, lineStarts);
    },
  };
}

const FIXER = Object.freeze({
  replaceTextRange: (range, text) => ({ range: [range[0], range[1]], text: String(text) }),
  replaceText: (node, text) => FIXER.replaceTextRange(node.range, text),
  insertTextBeforeRange: (range, text) => FIXER.replaceTextRange([range[0], range[0]], text),
  insertTextAfterRange: (range, text) => FIXER.replaceTextRange([range[1], range[1]], text),
  insertTextBefore: (node, text) => FIXER.insertTextBeforeRange(node.range, text),
  insertTextAfter: (node, text) => FIXER.insertTextAfterRange(node.range, text),
  removeRange: (range) => FIXER.replaceTextRange(range, ""),
  remove: (node) => FIXER.replaceTextRange(node.range, ""),
});

/**
 * One edit from whatever a rule's `fix` returned: nothing, one edit, or an
 * iterable of them.
 *
 * Several edits become one covering all of them, with the text between them
 * carried over — ESLint's merge, so that one finding is one edit and two
 * findings' edits can be told apart. Edits that overlap one another have no
 * order that means anything, and are refused.
 */
export function mergeFixes(result, source) {
  if (result == null) {
    return null;
  }
  const fixes = (typeof result[Symbol.iterator] === "function" ? [...result] : [result]).filter(
    (fix) => fix != null,
  );
  if (fixes.length === 0) {
    return null;
  }
  fixes.sort((a, b) => a.range[0] - b.range[0] || a.range[1] - b.range[1]);
  const start = fixes[0].range[0];
  let at = start;
  let text = "";
  for (const fix of fixes) {
    if (fix.range[0] < at) {
      throw new RangeError("a rule's fixes overlap one another");
    }
    text += source.slice(at, fix.range[0]) + fix.text;
    at = fix.range[1];
  }
  return { start, end: at, text };
}

function describe(error) {
  return error instanceof Error ? error.message : String(error);
}
