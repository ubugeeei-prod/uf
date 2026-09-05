// @noflow
//
// Which words in a highlighted line are Flow's, and what kind of word each is.
//
// No highlighter ships a Flow grammar, so a JavaScript one runs over uf's code
// samples and Flow's own vocabulary — `component`, `hook`, `renders`, `match`,
// `opaque`, `mixed` — arrives as ordinary identifiers. This module decides,
// after the grammar has run, which of those occurrences are really Flow's, and
// `internal/highlight.js` turns the decision into a class.
//
// # Why this is not a regular expression over a token
//
// It used to be. A `(?<![.\w$])` look-behind was supposed to leave
// `text.match(…)` alone; it cannot, because the grammar splits a line into
// tokens wherever it likes and a look-behind only ever sees inside the token
// it is matching. Given `text.` and `match` as two tokens — which is what a
// grammar actually produces — the look-behind starts at the beginning of a
// string, finds nothing, and `match` came out red.
//
// So the line is reassembled and every rule is asked about the whole line.
//
// # Why the line is redacted first
//
// The class the mark produces sets `color` with `!important`, so marking a
// word inside a string literal does not merely add a colour: it *replaces* the
// string's own, in the middle of the string. `const s = "component renders"`
// came out with two red words inside a blue string, and `// component hook`
// with two red words inside a grey comment. Both were live in this repository.
//
// A rule cannot be trusted to notice that on its own, so nothing is asked:
// {@link redact} blanks every string, template, comment and regular expression
// on the line before any rule sees it, keeping the line's length so every
// position still means what it meant. A word that was inside one of those is
// then not there to match, and the punctuation around it — the `.` of
// `"x".match(y)` — still is.
//
// # Why the rules are per word rather than one shared rule
//
// The words are Flow's for different reasons and the counter-examples differ.
// `hook` is a keyword in `hook useThing()` and a property in `{ hook: 1 }`;
// `match` is a keyword in `match (x) {` and a method in `text.match(re)`;
// `mixed` is a *type*, not a keyword, and wants the colour `string` and
// `number` get rather than the one `export` gets. One predicate over all of
// them is how `const component = 1` ended up red.

/** The mark a decided word carries, read by `internal/highlight.js`. */
export const FLOW_MARK = Symbol.for("uf.flowMark");

/** A word Flow reserves. Painted like `export` and `return`. */
export const KEYWORD = "keyword";

/**
 * A type Flow has and JavaScript does not. Painted like `string` and `number`,
 * because that is what it is — `mixed` beside a red `component` said they were
 * the same kind of word, and they are not.
 */
export const TYPE = "type";

/**
 * Every occurrence this module will consider.
 *
 * Whole words only, so `components`, `matcher` and `hooks` never reach a rule.
 * `%checks` is here as one candidate rather than as `checks`, because the `%`
 * is the only thing that distinguishes Flow's predicate marker from a variable
 * called `checks` — and a JavaScript grammar paints that variable as a
 * function call, which is the wrong answer twice over.
 *
 * `enum` is absent: every JavaScript grammar already treats it as a keyword.
 */
const CANDIDATES =
  /%checks(?![\w$])|(?<![\w$])(?:component|hook|renders|match|opaque|typeof|of|mixed|empty)(?![\w$])/g;

/**
 * Words that may not be immediately followed by a Flow declaration keyword,
 * because what follows them is a name being bound.
 *
 * `function match(re) {` satisfies every shape rule for a `match` expression —
 * a subject in parentheses and a block after it — and is a function called
 * `match`.
 */
const BINDERS = /(?:^|[^\w$])(?:const|let|var|function|class|new)\s*$/;

/** Characters after which a `/` opens a regular expression, not a division. */
const BEFORE_REGEX = "(,=:[!&|?{};+-*%~^<>";

/**
 * Keywords after which a `/` opens a regular expression.
 *
 * `return /x/` and `case /x/` are the ones that turn up; without them the
 * literal is read as a division and its contents are left un-redacted.
 */
const BEFORE_REGEX_WORD =
  /(?:^|[^\w$])(?:return|case|typeof|in|of|new|do|else|yield|await|void|delete)$/;

/** Scanner carry: the line begins in ordinary code. */
const CODE = 0;

/** Scanner carry: the line begins inside a `/* … *\/`. */
const BLOCK_COMMENT = 1;

/** Scanner carry: the line begins inside a template literal. */
const TEMPLATE = 2;

/**
 * The line with everything that is not code blanked out, same length.
 *
 * Blanking rather than removing, and blanking with spaces rather than dropping
 * the characters, because every rule works in line coordinates and the marks
 * are sliced out of the *original* text afterwards. A redacted line has to
 * agree with the real one about where every remaining character is.
 *
 * `carry` is the state the previous line ended in, so a block comment or a
 * template literal that spans lines stays redacted on the lines below its
 * opener. The returned `next` is that state for the line after this one.
 *
 * Two deliberate approximations, both erring towards redacting more:
 *
 * * A template literal is blanked whole, `${…}` included. A Flow keyword
 *   inside an interpolation is therefore missed. Tracking brace depth through
 *   nested templates to recover it would be a JavaScript lexer, and the cost
 *   of the approximation is a missing colour rather than a wrong one.
 * * A regular expression is recognised by what precedes it, which is the only
 *   way to tell `/re/` from a division without parsing. `a / b` is never
 *   mistaken for one; a literal in a position this does not list is left
 *   un-redacted.
 */
export function redact(text, carry = CODE) {
  let out = "";
  let at = 0;
  let state = carry;

  while (at < text.length) {
    if (state === BLOCK_COMMENT) {
      const close = text.indexOf("*/", at);
      const to = close === -1 ? text.length : close + 2;
      out += " ".repeat(to - at);
      at = to;
      state = close === -1 ? BLOCK_COMMENT : CODE;
      continue;
    }
    if (state === TEMPLATE) {
      const close = closingAt(text, at, "`");
      const to = close === -1 ? text.length : close;
      out += " ".repeat(to - at);
      at = to;
      state = close === -1 ? TEMPLATE : CODE;
      continue;
    }

    const here = text[at];
    const next = text[at + 1];

    if (here === "/" && next === "/") {
      out += " ".repeat(text.length - at);
      return { code: out, next: CODE };
    }
    if (here === "/" && next === "*") {
      out += "  ";
      at += 2;
      state = BLOCK_COMMENT;
      continue;
    }
    if (here === '"' || here === "'") {
      const close = closingAt(text, at + 1, here);
      const to = close === -1 ? text.length : close;
      out += " ".repeat(to - at);
      at = to;
      continue;
    }
    if (here === "`") {
      const close = closingAt(text, at + 1, "`");
      const to = close === -1 ? text.length : close;
      out += " ".repeat(to - at);
      at = to;
      state = close === -1 ? TEMPLATE : CODE;
      continue;
    }
    if (here === "/" && opensRegex(out)) {
      const close = closingAt(text, at + 1, "/");
      const to = close === -1 ? text.length : close;
      out += " ".repeat(to - at);
      at = to;
      continue;
    }

    out += here;
    at += 1;
  }
  return { code: out, next: state === BLOCK_COMMENT || state === TEMPLATE ? state : CODE };
}

/**
 * One past the `close` that ends the literal begun before `from`, or `-1` when
 * the literal does not close on this line.
 *
 * `-1` rather than the line's length, because a literal that ends on the last
 * character of the line and one that does not end at all are different
 * answers, and returning the length for both made ``const a = `x` `` carry a
 * template into the next line and redact it whole.
 */
function closingAt(text, from, close) {
  let at = from;
  while (at < text.length) {
    const here = text[at];
    if (here === "\\") {
      at += 2;
      continue;
    }
    if (here === close) {
      return at + 1;
    }
    at += 1;
  }
  return -1;
}

/** Whether a `/` written after `before` opens a regular expression. */
function opensRegex(before) {
  const code = before.trimEnd();
  if (code === "") {
    return true;
  }
  return BEFORE_REGEX.includes(code[code.length - 1]) || BEFORE_REGEX_WORD.test(code);
}

/**
 * The rule for each word, and the kind of mark a decided occurrence gets.
 *
 * A rule is given the redacted line either side of the candidate. Both sides,
 * because both sides carry the answer: `hook` is decided by what follows it
 * and `typeof` by what precedes it.
 *
 * `modifier` is the one character a decided word may swallow. `renders*` and
 * `renders?` are single tokens in Flow, and leaving the arity to be coloured
 * as a multiplication or a ternary splits one word into two colours.
 */
const WORDS = new Map([
  ["component", { kind: KEYWORD, decide: declaresOrTypesAFunction }],
  ["hook", { kind: KEYWORD, decide: declaresOrTypesAFunction }],
  ["renders", { kind: KEYWORD, decide: introducesARenderType, modifier: /^[*?]/ }],
  ["match", { kind: KEYWORD, decide: takesAMatchSubject }],
  ["opaque", { kind: KEYWORD, decide: precedesTypeAlias }],
  ["typeof", { kind: KEYWORD, decide: qualifiesAnImport }],
  ["of", { kind: KEYWORD, decide: givesEnumRepresentation }],
  ["%checks", { kind: KEYWORD, decide: () => true }],
  ["mixed", { kind: TYPE, decide: standsInATypePosition }],
  ["empty", { kind: TYPE, decide: standsInATypePosition }],
]);

/**
 * Whether the candidate is a member name — `text.match`, `a?.hook`.
 *
 * Checked before every other rule for every word: a name after a dot is never
 * Flow's, whatever it is spelled.
 */
function isMemberName(before) {
  return before.trimEnd().endsWith(".");
}

/** The last character of `before` that is not whitespace. */
function lastSignificant(before) {
  const code = before.trimEnd();
  return code === "" ? "" : code[code.length - 1];
}

/**
 * `component Avatar(…)`, `hook useNow(…)`, and the function types that share
 * their spelling — `type Fn = component<T>(…) renders mixed`.
 *
 * The declaration form needs no context beyond its own shape, because nothing
 * else in JavaScript is `word Name(`. The type form does: `component(props)`
 * and `hook(x)` are ordinary calls, so the bare-parenthesis spelling is only
 * Flow's where a type can appear at all. `component<` needs no such test —
 * a comparison against a generic call is not a thing anyone writes.
 *
 * This is what leaves `const component = 1;` and `{ hook: 1 }` alone: neither
 * is followed by a name and a parenthesis, and neither sits in a type
 * position.
 */
function declaresOrTypesAFunction(before, after) {
  if (isMemberName(before) || BINDERS.test(before)) {
    return false;
  }
  if (/^\s+[A-Za-z_$][\w$]*\s*[(<]/.test(after)) {
    return true;
  }
  if (/^\s*</.test(after)) {
    return true;
  }
  return /^\s*\(/.test(after) && ":|&,(<[=>".includes(lastSignificant(before));
}

/**
 * `renders T`, `renders? T`, `renders* T`.
 *
 * A render type is always followed by a type, so requiring one is what
 * separates it from every other use of the spelling: `{ +renders: Node }` is
 * followed by a colon, `const renders = 1` by an equals sign, `x.renders` by
 * nothing at all.
 *
 * `renders (A | B)` is not marked. Parenthesised render types are legal and
 * rare, and accepting them would accept every call to a function named
 * `renders`, which is neither.
 */
function introducesARenderType(before, after) {
  if (isMemberName(before) || BINDERS.test(before)) {
    return false;
  }
  return /^\s*[*?]?\s*[A-Za-z_$]/.test(after);
}

/**
 * `match (subject) { … }`, statement or expression.
 *
 * The subject and the block are both required, and that is the whole rule:
 * `text.match(re)` has a subject and no block, and a function called `match`
 * declared as `function match(re) {` has both — which is why {@link BINDERS}
 * is consulted first.
 *
 * A subject that runs past the end of the line is accepted on the strength of
 * the unclosed parenthesis; the block is on a line this rule cannot see, and
 * an unclosed subject is not something a method call does.
 */
function takesAMatchSubject(before, after) {
  if (isMemberName(before) || BINDERS.test(before)) {
    return false;
  }
  const open = /^\s*\(/.exec(after);
  if (open == null) {
    return false;
  }
  let depth = 0;
  for (let at = open[0].length - 1; at < after.length; at += 1) {
    const here = after[at];
    if (here === "(") {
      depth += 1;
    } else if (here === ")") {
      depth -= 1;
      if (depth === 0) {
        return /^\s*\{/.test(after.slice(at + 1));
      }
    }
  }
  return true;
}

/** `opaque type ID = string`. Nothing else in Flow spells `opaque`. */
function precedesTypeAlias(before, after) {
  return !isMemberName(before) && /^\s+type(?![\w$])/.test(after);
}

/**
 * The `typeof` of `import typeof Bar from …`.
 *
 * A grammar gives `import type` its keyword colour and leaves `import typeof`
 * grey, because `type` is a modifier it knows and `typeof` in that position is
 * not. The `typeof` of an expression is already coloured and is not this one,
 * which is why the rule asks what precedes rather than matching the word.
 */
function qualifiesAnImport(before) {
  return /^\s*(?:import|export)\s+$/.test(before);
}

/** The `of` of `enum Status of string { … }`, and no other `of`. */
function givesEnumRepresentation(before) {
  return /^\s*(?:export\s+)?(?:declare\s+)?enum\s+[A-Za-z_$][\w$]*\s+$/.test(before);
}

/**
 * Whether `mixed` or `empty` is being used as a type rather than as a name.
 *
 * A type follows the punctuation that introduces one — `a: mixed`,
 * `A | mixed`, `Array<mixed>`, `(x) => mixed`, `renders mixed`. An equals sign
 * only introduces one inside a type alias, which is the difference between
 * `type T = mixed` and `const mixed = 1`; both have an identifier after an
 * `=`, and only one of them is Flow's.
 */
function standsInATypePosition(before, after) {
  if (isMemberName(before) || /^\s*:/.test(after)) {
    return false;
  }
  const code = before.trimEnd();
  if (code.endsWith("=>")) {
    return true;
  }
  const previous = lastSignificant(before);
  if (":|&,(<[".includes(previous)) {
    return true;
  }
  if (previous === "=") {
    return /(?:^|[^\w$])type\s+[A-Za-z_$][\w$]*[^=]*=$/.test(code);
  }
  return /(?:^|[^\w$])renders\s*[*?]?$/.test(code);
}

/**
 * The marks a line carries, as `[start, end, kind]` in line coordinates.
 *
 * `code` is the redacted line; the marks are sliced out of the real one.
 */
function marksIn(code) {
  const marks = [];
  for (const found of code.matchAll(CANDIDATES)) {
    const start = found.index ?? 0;
    const end = start + found[0].length;
    const word = WORDS.get(found[0]);
    if (word == null || !word.decide(code.slice(0, start), code.slice(end))) {
      continue;
    }
    const swallows = word.modifier != null && word.modifier.test(code.slice(end));
    marks.push([start, swallows ? end + 1 : end, word.kind]);
  }
  return marks;
}

/**
 * A whole block of tokenised lines, with Flow's words marked.
 *
 * The block rather than the line is the unit because a block comment and a
 * template literal both run past a line ending, and a line inside one is
 * indistinguishable from code when it is read on its own.
 */
export function markLines(lines) {
  let carry = CODE;
  return lines.map((line) => {
    const text = line.map((token) => token.content).join("");
    const { code, next } = redact(text, carry);
    carry = next;
    return split(line, text, marksIn(code));
  });
}

/**
 * One line, read as if the block began there.
 *
 * Exported because it is where the decision is visible without starting Shiki:
 * give it the token split a grammar would produce and it says which words it
 * marked. `tests/library/highlight.test.js` uses exactly that, and the splits
 * in it are the ones that had bugs.
 */
export function markLine(line) {
  return markLines([line])[0];
}

/**
 * The line's tokens, cut around each mark.
 *
 * A grammar puts `component Avatar(src: string) {` in one token, so the marked
 * word usually has to be cut out of a token rather than found as one. The cuts
 * are taken from the original `text`, so a mark that spans a token boundary —
 * `%checks`, which arrives as ` %` and `checks` — becomes one marked piece per
 * token it crosses rather than being lost.
 */
function split(line, text, marks) {
  if (marks.length === 0) {
    return line;
  }

  const out = [];
  let at = 0;
  for (const token of line) {
    const start = at;
    const end = at + token.content.length;
    at = end;

    const cuts = [];
    for (const [from, to, kind] of marks) {
      if (from < end && to > start) {
        cuts.push([Math.max(from, start), Math.min(to, end), kind]);
      }
    }
    if (cuts.length === 0) {
      out.push(token);
      continue;
    }

    let index = start;
    for (const [from, to, kind] of cuts) {
      if (from > index) {
        out.push(piece(token, text.slice(index, from), index - start));
      }
      out.push({ ...piece(token, text.slice(from, to), from - start), [FLOW_MARK]: kind });
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
