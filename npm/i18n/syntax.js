// @flow
//
// `@uniflowed/i18n/syntax`: MessageFormat 2 source into a tree.
//
// A message is a small language, and this is its parser. Nothing here formats
// anything, knows what a locale is, or has heard of `Intl` — it turns text
// into a tree and refuses text it cannot turn into one. `format.js` is the
// half that runs.
//
// # Why a hand-written scanner and not a regular expression
//
// Three reasons, in the order they became true.
//
// MF2 is not a regular language. `{$count :number style=percent}` nests an
// operand, an annotation and a list of options; a quoted pattern nests a
// pattern that nests placeholders. A regular expression that appeared to
// handle it would be handling the examples in the specification and nothing
// else, and the failures would be silent — a malformed message that parsed
// into something plausible, formatted, and shipped.
//
// A parse error has to say *where*. `unexpected "}" at offset 34` is a
// message somebody can act on with the source in front of them; `did not
// match` is not, and a translator handed the second will delete characters
// until it stops complaining.
//
// And `crates/uf_lib/tests/package_surface.rs` reads every shipped module with
// a scanner that does not model regular-expression literals — it says so, in
// the comment on `code_only` — so a `/…/` in this file would blank the wrong
// half of the module and fail a structural test for a reason nobody could see
// from the error. That is a small reason next to the first two, and it is the
// one that would have cost an afternoon.
//
// # The subset, and where the line is
//
// Implemented: simple messages, quoted patterns, the four escapes, variable
// and literal placeholders, the six functions of the MF2 default registry with
// their options, `.input` and `.local` declarations, and `.match` with any
// number of selectors.
//
// Refused, each with an error that names itself rather than a generic syntax
// complaint:
//
// - **Markup** — `{#bold}…{/bold}`. `format` returns a string, and markup only
//   means something to a caller that can turn a list of parts into elements.
//   Dropping the tags would silently lose emphasis a translator put in;
//   inlining HTML would put unescaped translator input into a page. Refusing
//   it at the point the catalogue is defined is the only one of the three that
//   cannot go wrong quietly.
// - **Attributes** — `{$x @unit}`. The specification says attributes do not
//   affect formatting, so accepting and ignoring them would be correct. They
//   are refused anyway, because the only thing they are for is a tool that
//   reads them, uf has no such tool, and a message that carries an attribute
//   uf will never read is a message whose author believes something that is
//   not true.
// - **Reserved and private-use annotations** — `{$x !foo}`, `{$x ^bar}`. The
//   specification reserves these sigils for later versions and for private
//   agreements. Refusing them is what keeps a message that parses here from
//   meaning something different under a conforming implementation.
//
// # What a name is
//
// MF2's `name` production draws on a wide slice of Unicode, and this
// implements a documented approximation rather than the table: ASCII letters,
// `_`, and any code point at or above U+00A1 may start a name; digits, `-`,
// `.` and U+00B7 may continue one. The gap is the specification's exclusion of
// surrogates and a handful of ranges above U+00A1, which this admits.
//
// Erring wide is the right direction here. A name uf accepts and the
// specification does not is a message that works everywhere uf runs and is
// rejected by a stricter tool — visible the moment anyone tries. A name uf
// rejected would be a message a translator wrote correctly and uf refused,
// which looks like a bug in their translation.

/** A quoted or unquoted literal: `|two words|`, `42`, `percent`. */
export type MessageLiteral = { readonly kind: "literal", readonly value: string };

/** A reference to an argument or to something `.input`/`.local` declared. */
export type MessageVariable = { readonly kind: "variable", readonly name: string };

/** What a placeholder or an option may be given. */
export type MessageOperand = MessageLiteral | MessageVariable;

/** One `name=value` inside a function annotation. */
export type MessageOption = {
  readonly name: string,
  readonly value: MessageOperand,
};

/** `:number`, and the options it was given. */
export type MessageAnnotation = {
  readonly name: string,
  readonly options: $ReadOnlyArray<MessageOption>,
};

/**
 * One `{…}`.
 *
 * `operand` is absent for an annotation-only expression such as
 * `{:datetime}` in a `.local`, and `annotation` is absent for a bare `{$name}`
 * — but never both, which the parser enforces rather than the type, because
 * the type that says so is a union whose two arms are identical everywhere
 * else and would be read at every use.
 */
export type MessageExpression = {
  readonly kind: "expression",
  readonly operand: MessageOperand | null,
  readonly annotation: MessageAnnotation | null,
  /** Offset in the source, so a formatting error can point at it too. */
  readonly at: number,
};

/** Literal text between placeholders, with escapes already resolved. */
export type MessageText = { readonly kind: "text", readonly value: string };

export type MessagePart = MessageText | MessageExpression;

/** A run of text and placeholders: what actually gets formatted. */
export type MessagePattern = $ReadOnlyArray<MessagePart>;

/**
 * `.input {$count :number}` or `.local $n = {$count :number}`.
 *
 * One shape for both, because the difference is only where the value comes
 * from — an `.input` re-annotates an argument under its own name, a `.local`
 * introduces a new one — and every consumer treats them the same way.
 */
export type MessageDeclaration = {
  readonly kind: "input" | "local",
  readonly name: string,
  readonly expression: MessageExpression,
};

/** One key of one variant: a literal, or `*`. */
export type MessageVariantKey =
  | { readonly kind: "literal", readonly value: string }
  | { readonly kind: "catch-all" };

/** One line of a `.match`: its keys, and what to format if they win. */
export type MessageVariant = {
  readonly keys: $ReadOnlyArray<MessageVariantKey>,
  readonly pattern: MessagePattern,
};

export type MessageBody =
  | { readonly kind: "pattern", readonly pattern: MessagePattern }
  | {
      readonly kind: "select",
      readonly selectors: $ReadOnlyArray<MessageVariable>,
      readonly variants: $ReadOnlyArray<MessageVariant>,
    };

/** A parsed message: its declarations, and the body they feed. */
export type MessageNode = {
  readonly declarations: $ReadOnlyArray<MessageDeclaration>,
  readonly body: MessageBody,
};

/**
 * A message that is not MF2, or is MF2 uf does not implement.
 *
 * Carries the offset as well as putting it in the text, so a caller that has
 * the source — `catalogue.js` does, and names the key beside it — can point at
 * the character rather than reprinting the sentence.
 */
export class MessageSyntaxError extends Error {
  offset: number;
  source: string;

  constructor(message: string, source: string, offset: number) {
    super(`@uniflowed/i18n: ${message} at offset ${String(offset)}`);
    this.name = "MessageSyntaxError";
    this.offset = offset;
    this.source = source;
  }
}

/** MF2's `s`: the five code points that count as whitespace. */
function isSpace(code: number): boolean {
  return code === 0x20 || code === 0x09 || code === 0x0d || code === 0x0a || code === 0x3000;
}

function isDigit(code: number): boolean {
  return code >= 0x30 && code <= 0x39;
}

/** See the module header on why this is wider than the specification's. */
function isNameStart(code: number): boolean {
  return (
    (code >= 0x41 && code <= 0x5a) ||
    (code >= 0x61 && code <= 0x7a) ||
    code === 0x5f ||
    code >= 0xa1
  );
}

function isNameChar(code: number): boolean {
  return isNameStart(code) || isDigit(code) || code === 0x2d || code === 0x2e || code === 0xb7;
}

/**
 * A character that may appear unescaped in a literal that has no `|` around
 * it.
 *
 * Wider than `name-char` by `+` alone, which is what lets `1e+6` and `+1` be
 * written as option values and variant keys without quoting. The specification
 * reaches the same place through a separate `number-literal` production; one
 * predicate is the same answer with one thing to read.
 */
function isUnquotedChar(code: number): boolean {
  return isNameChar(code) || code === 0x2b;
}

/**
 * The cursor.
 *
 * A mutable object rather than an index threaded through twenty functions:
 * every function here advances it and the alternative is returning a position
 * beside every value, which is the same state with a chance to forget to
 * thread it.
 */
type Cursor = { at: number };

function fail(source: string, at: number, message: string): empty {
  throw new MessageSyntaxError(message, source, at);
}

function peek(source: string, cursor: Cursor): number {
  return cursor.at < source.length ? source.charCodeAt(cursor.at) : -1;
}

function skipSpace(source: string, cursor: Cursor): void {
  while (cursor.at < source.length && isSpace(source.charCodeAt(cursor.at))) {
    cursor.at += 1;
  }
}

/** True if `word` is next, and consumes it if so. */
function eatWord(source: string, cursor: Cursor, word: string): boolean {
  if (source.startsWith(word, cursor.at)) {
    cursor.at += word.length;
    return true;
  }
  return false;
}

function expect(source: string, cursor: Cursor, character: string, what: string): void {
  if (source[cursor.at] !== character) {
    fail(source, cursor.at, `expected ${character} ${what}`);
  }
  cursor.at += 1;
}

function readName(source: string, cursor: Cursor, what: string): string {
  const start = cursor.at;
  if (cursor.at >= source.length || !isNameStart(source.charCodeAt(cursor.at))) {
    fail(source, cursor.at, `expected ${what}`);
  }
  cursor.at += 1;
  while (cursor.at < source.length && isNameChar(source.charCodeAt(cursor.at))) {
    cursor.at += 1;
  }
  return source.slice(start, cursor.at);
}

/**
 * The four escapes MF2 allows, and nothing else.
 *
 * `\n` is deliberately not one of them. A translator who writes `\n` expecting
 * a line break gets a syntax error naming the character, which is a better
 * afternoon than a message that renders a literal backslash-n to a user.
 */
function readEscape(source: string, cursor: Cursor): string {
  cursor.at += 1;
  const escaped = source[cursor.at];
  if (escaped !== "\\" && escaped !== "{" && escaped !== "}" && escaped !== "|") {
    fail(
      source,
      cursor.at - 1,
      "a backslash may only escape one of \\ { } |, so an ordinary backslash is written \\\\",
    );
  }
  cursor.at += 1;
  return escaped;
}

function readQuotedLiteral(source: string, cursor: Cursor): string {
  const opened = cursor.at;
  cursor.at += 1;
  let value = "";
  while (cursor.at < source.length) {
    const character = source[cursor.at];
    if (character === "|") {
      cursor.at += 1;
      return value;
    }
    if (character === "\\") {
      value += readEscape(source, cursor);
      continue;
    }
    value += character;
    cursor.at += 1;
  }
  return fail(source, opened, "a quoted literal was opened with | and never closed");
}

function readLiteral(source: string, cursor: Cursor, what: string): MessageLiteral {
  if (source[cursor.at] === "|") {
    return { kind: "literal", value: readQuotedLiteral(source, cursor) };
  }
  const start = cursor.at;
  while (cursor.at < source.length && isUnquotedChar(source.charCodeAt(cursor.at))) {
    cursor.at += 1;
  }
  if (cursor.at === start) {
    fail(source, cursor.at, `expected ${what}`);
  }
  return { kind: "literal", value: source.slice(start, cursor.at) };
}

function readVariable(source: string, cursor: Cursor): MessageVariable {
  expect(source, cursor, "$", "to start a variable");
  return { kind: "variable", name: readName(source, cursor, "a variable name after $") };
}

/**
 * The sigils MF2 reserves for later versions and for private agreements.
 *
 * Named here rather than caught by "unexpected character", because the two
 * mean different things to whoever reads the error: an unexpected character is
 * a typo, and one of these is a message written for a different implementation.
 */
const RESERVED_SIGILS: $ReadOnlyArray<string> = ["!", "%", "*", "+", "<", ">", "?", "~", "^", "&"];

function readAnnotation(source: string, cursor: Cursor): MessageAnnotation {
  expect(source, cursor, ":", "to start a function annotation");
  const name = readName(source, cursor, "a function name after :");
  if (source[cursor.at] === ":") {
    fail(
      source,
      cursor.at,
      `a namespaced function (:${name}:…) is outside the subset uf implements`,
    );
  }
  const options: Array<MessageOption> = [];
  while (true) {
    const before = cursor.at;
    skipSpace(source, cursor);
    if (cursor.at === before) break;
    const next = peek(source, cursor);
    if (next < 0 || !isNameStart(next)) {
      // The whitespace belonged to whatever closes the expression.
      cursor.at = before;
      break;
    }
    const optionName = readName(source, cursor, "an option name");
    skipSpace(source, cursor);
    expect(source, cursor, "=", `after the option name ${optionName}`);
    skipSpace(source, cursor);
    const value =
      source[cursor.at] === "$"
        ? readVariable(source, cursor)
        : readLiteral(source, cursor, `a value for the option ${optionName}`);
    options.push({ name: optionName, value });
  }
  return { name, options };
}

function readExpression(source: string, cursor: Cursor): MessageExpression {
  const at = cursor.at;
  expect(source, cursor, "{", "to start a placeholder");
  skipSpace(source, cursor);

  const opener = source[cursor.at];
  if (opener === "#" || opener === "/") {
    fail(
      source,
      cursor.at,
      "markup ({#tag} … {/tag}) is outside the subset uf implements; format returns a string, " +
        "and a string cannot carry markup without either losing it or inlining it unescaped",
    );
  }
  if (opener != null && RESERVED_SIGILS.includes(opener)) {
    fail(
      source,
      cursor.at,
      `${opener} starts an annotation MessageFormat 2 reserves, so a message using it means ` +
        "something uf cannot know",
    );
  }

  let operand: MessageOperand | null = null;
  let annotation: MessageAnnotation | null = null;
  if (opener === "$") {
    operand = readVariable(source, cursor);
  } else if (opener !== ":") {
    operand = readLiteral(source, cursor, "a variable, a literal or a :function inside {}");
  }

  const beforeSpace = cursor.at;
  skipSpace(source, cursor);
  const annotating = source[cursor.at];
  if (annotating === ":") {
    annotation = readAnnotation(source, cursor);
  } else if (annotating != null && RESERVED_SIGILS.includes(annotating)) {
    // The same refusal as at the opener, and it has to be here as well:
    // `{!reserved}` is caught above and `{$x !reserved}` is not, because by
    // then the operand has been read and the sigil is in annotation position.
    // Checking only one of the two spellings would accept half of exactly the
    // messages this refuses.
    fail(
      source,
      cursor.at,
      `${annotating} starts an annotation MessageFormat 2 reserves, so a message using it means ` +
        "something uf cannot know",
    );
  } else if (operand != null) {
    cursor.at = beforeSpace;
  }

  skipSpace(source, cursor);
  if (source[cursor.at] === "@") {
    fail(
      source,
      cursor.at,
      "an @attribute is outside the subset uf implements; it would not change what this " +
        "message formats to, and nothing in uf reads one",
    );
  }
  expect(source, cursor, "}", "to close the placeholder");
  return { kind: "expression", operand, annotation, at };
}

/**
 * Text and placeholders up to `stop`.
 *
 * `stop` is `"}}"` inside a quoted pattern and the empty string for a simple
 * message, which runs to the end of the source. A bare `}` is an error in
 * both, because MF2 makes it one — and the error names the escape, since the
 * character is common in text somebody is translating.
 */
function readPattern(source: string, cursor: Cursor, stop: string): MessagePattern {
  const parts: Array<MessagePart> = [];
  let text = "";

  const flush = () => {
    if (text !== "") {
      parts.push({ kind: "text", value: text });
      text = "";
    }
  };

  while (cursor.at < source.length) {
    if (stop !== "" && source.startsWith(stop, cursor.at)) break;
    const character = source[cursor.at];
    if (character === "\\") {
      text += readEscape(source, cursor);
      continue;
    }
    if (character === "{") {
      flush();
      parts.push(readExpression(source, cursor));
      continue;
    }
    if (character === "}") {
      fail(source, cursor.at, "an unescaped } in a pattern; write \\} for a literal brace");
    }
    text += character;
    cursor.at += 1;
  }

  if (stop !== "" && !source.startsWith(stop, cursor.at)) {
    fail(source, cursor.at, `a quoted pattern was opened with {{ and never closed with ${stop}`);
  }
  flush();
  return parts;
}

function readQuotedPattern(source: string, cursor: Cursor): MessagePattern {
  if (!eatWord(source, cursor, "{{")) {
    fail(source, cursor.at, "expected a quoted pattern, which is written {{ like this }}");
  }
  const pattern = readPattern(source, cursor, "}}");
  cursor.at += 2;
  return pattern;
}

function readDeclaration(source: string, cursor: Cursor): MessageDeclaration {
  if (eatWord(source, cursor, ".input")) {
    skipSpace(source, cursor);
    const at = cursor.at;
    const expression = readExpression(source, cursor);
    const operand = expression.operand;
    if (operand == null || operand.kind !== "variable") {
      return fail(source, at, ".input must be given a variable, as in .input {$count :number}");
    }
    return { kind: "input", name: operand.name, expression };
  }
  if (!eatWord(source, cursor, ".local")) {
    return fail(source, cursor.at, "expected .input, .local or .match");
  }
  skipSpace(source, cursor);
  const variable = readVariable(source, cursor);
  skipSpace(source, cursor);
  expect(source, cursor, "=", `after .local $${variable.name}`);
  skipSpace(source, cursor);
  return { kind: "local", name: variable.name, expression: readExpression(source, cursor) };
}

function readVariantKey(source: string, cursor: Cursor): MessageVariantKey {
  if (source[cursor.at] === "*") {
    cursor.at += 1;
    return { kind: "catch-all" };
  }
  return { kind: "literal", value: readLiteral(source, cursor, "a variant key or *").value };
}

function readMatcher(source: string, cursor: Cursor): MessageBody {
  const selectors: Array<MessageVariable> = [];
  while (true) {
    skipSpace(source, cursor);
    if (source[cursor.at] !== "$") break;
    selectors.push(readVariable(source, cursor));
  }
  if (selectors.length === 0) {
    fail(source, cursor.at, ".match needs at least one selector, as in .match $count");
  }

  const variants: Array<MessageVariant> = [];
  while (true) {
    skipSpace(source, cursor);
    if (cursor.at >= source.length) break;
    const keys: Array<MessageVariantKey> = [];
    while (keys.length < selectors.length) {
      if (keys.length > 0) skipSpace(source, cursor);
      keys.push(readVariantKey(source, cursor));
    }
    skipSpace(source, cursor);
    variants.push({ keys, pattern: readQuotedPattern(source, cursor) });
  }

  if (variants.length === 0) {
    fail(source, cursor.at, ".match needs at least one variant");
  }
  // Checked here rather than at format time on purpose: a `.match` with no
  // catch-all formats correctly for every value somebody tried and throws on
  // the first one they did not, which is the failure mode a translation layer
  // must not have. MF2 requires it for the same reason.
  const catchAll = variants.some((variant) =>
    variant.keys.every((key) => key.kind === "catch-all"),
  );
  if (!catchAll) {
    fail(
      source,
      cursor.at,
      "a .match needs a variant whose keys are all *, so that every value formats to something",
    );
  }
  return { kind: "select", selectors, variants };
}

/**
 * A duplicate declaration, caught where it is written.
 *
 * MF2 makes this an error, and the reason is worth keeping in view: two
 * `.local $n` lines look like a redefinition and are not — the second cannot
 * see the first, because a declaration's expression is resolved against what
 * was in scope before it. A message with two of them means something nobody
 * intended whichever way it is read.
 */
function assertDeclarationsAreDistinct(
  source: string,
  declarations: $ReadOnlyArray<MessageDeclaration>,
): void {
  const seen: Set<string> = new Set();
  for (const declaration of declarations) {
    if (seen.has(declaration.name)) {
      fail(source, declaration.expression.at, `$${declaration.name} is declared twice`);
    }
    seen.add(declaration.name);
  }
}

/**
 * Parse an MF2 message.
 *
 * Throws [`MessageSyntaxError`] rather than returning a result, and that is a
 * decision rather than an oversight: every caller in this package is
 * `catalogue.js` building a catalogue at start-up, where there is nothing
 * useful to do with a bad message except stop. A `safeParse` twin would exist
 * for a tool that wants to report several at once, and nothing in uf is that
 * tool yet.
 */
export function parseMessage(source: string): MessageNode {
  const cursor: Cursor = { at: 0 };

  // MF2 decides simple against complex on the first character alone, which is
  // why a multi-line message has to begin with `.input` or `.match` hard
  // against the backtick. Trimming here would be a kindness that changed what
  // a message means: " .match" is a simple message whose text starts with a
  // space, and uf must not turn it into a matcher nobody wrote.
  if (source[0] !== ".") {
    return {
      declarations: [],
      body: { kind: "pattern", pattern: readPattern(source, cursor, "") },
    };
  }

  const declarations: Array<MessageDeclaration> = [];
  let body: MessageBody | null = null;
  while (cursor.at < source.length) {
    skipSpace(source, cursor);
    if (eatWord(source, cursor, ".match")) {
      body = readMatcher(source, cursor);
      break;
    }
    if (source[cursor.at] === "{") {
      body = { kind: "pattern", pattern: readQuotedPattern(source, cursor) };
      break;
    }
    declarations.push(readDeclaration(source, cursor));
  }

  if (body == null) {
    // `return fail(…)` rather than a bare call: `fail` returns `empty`, and
    // returning it is what tells the checker the lines below cannot run with
    // `body` still null. A bare call leaves the narrowing to an inference the
    // checker does not make.
    return fail(
      source,
      cursor.at,
      "a message with declarations needs a body: either a .match or a {{quoted pattern}}",
    );
  }
  skipSpace(source, cursor);
  if (cursor.at < source.length) {
    fail(source, cursor.at, "unexpected text after the end of the message");
  }
  assertDeclarationsAreDistinct(source, declarations);
  return { declarations, body };
}

/** What a message asks of the outside world, read off the tree. */
export type MessageUsage = {
  /** Every variable it reads and did not declare itself: its parameters. */
  readonly variables: $ReadOnlyArray<string>,
  /** Every `:function` it names, so a caller can refuse ones it cannot run. */
  readonly functions: $ReadOnlyArray<string>,
  /** `["count", "number"]` for each annotation applied directly to a variable. */
  readonly annotated: $ReadOnlyArray<[string, string]>,
};

/**
 * What a message needs, in one walk.
 *
 * This is where the two halves of the promise this package makes are compared.
 * Flow checks the *call* against the declared parameters; `catalogue.js`
 * checks the declared parameters against this, which is the half a type system
 * with no template-literal types cannot reach on its own — a message is a
 * string literal, and the placeholders inside a string literal are not part of
 * its type in any checker.
 *
 * One walk and one return value rather than three functions, because all three
 * facts are wanted at the same moment by the same caller, and a second walk is
 * a second chance for the two to disagree about what a declaration shadows.
 */
export function messageUsage(node: MessageNode): MessageUsage {
  const variables: Set<string> = new Set();
  const functions: Set<string> = new Set();
  const annotated: Array<[string, string]> = [];
  const declared: Set<string> = new Set();

  const visitOperand = (operand: MessageOperand | null) => {
    if (operand != null && operand.kind === "variable" && !declared.has(operand.name)) {
      variables.add(operand.name);
    }
  };
  const visitExpression = (expression: MessageExpression) => {
    visitOperand(expression.operand);
    const annotation = expression.annotation;
    if (annotation == null) return;
    functions.add(annotation.name);
    const operand = expression.operand;
    if (operand != null && operand.kind === "variable" && !declared.has(operand.name)) {
      annotated.push([operand.name, annotation.name]);
    }
    for (const option of annotation.options) visitOperand(option.value);
  };
  const visitPattern = (pattern: MessagePattern) => {
    for (const part of pattern) {
      if (part.kind === "expression") visitExpression(part);
    }
  };

  for (const declaration of node.declarations) {
    // Order matters, and in both directions. A `.local` is resolved against
    // what was in scope before it, so its own expression may read an argument
    // — `.local $n = {$count :number}` has the parameter `count`. An `.input`
    // names the argument it re-annotates, so `.input {$count :number}` also
    // has the parameter `count`, and marking it declared first would hide it.
    visitExpression(declaration.expression);
    declared.add(declaration.name);
  }

  const body = node.body;
  if (body.kind === "pattern") {
    visitPattern(body.pattern);
  } else {
    for (const selector of body.selectors) visitOperand(selector);
    for (const variant of body.variants) visitPattern(variant.pattern);
  }

  return {
    variables: Array.from(variables).sort(),
    functions: Array.from(functions).sort(),
    annotated,
  };
}
