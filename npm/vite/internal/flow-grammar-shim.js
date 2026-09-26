// @noflow
//
// The Flow syntax a JavaScript grammar cannot parse, shown to it as
// JavaScript it can, and taken back afterwards.
//
// # Why re-tagging the word is not enough
//
// `internal/flow-keywords.js` colours Flow's words after the grammar has run.
// That works for a word the grammar tokenised and mis-labelled. It cannot work
// for syntax that stops the grammar, because there are then no tokens to
// re-label — the whole construct arrives as one unstyled run. Two pieces of
// Flow do that, and both are common enough that this repository's own
// documentation hit them on its first page about Flow.
//
// **A `component` or `hook` declaration.** A JavaScript grammar has no
// production for an identifier where a declaration keyword belongs, so it
// gives up on the rest of the line:
//
//     export component Avatar(src: string, size: number = 32) {
//     └ keyword ┘ └ name ┘ └───────── one grey token ─────────┘
//
// The parameter names, their types and the default value were all the same
// undifferentiated grey. `export hook useNow(…) {` was worse: the grammar's
// state did not recover, and all eight lines of that sample came out as one
// grey token each — a code block with no highlighting at all, on the page
// whose subject is Flow's syntax.
//
// **An exact object type, `{| … |}`.** This one is quieter and travels
// further. The grammar reads the `{|` as a brace and a bitwise or, and every
// line *after* it in the same block is then mis-scoped: `export` came out in
// the colour of a function call, `return` likewise, and a JSX tag lost its
// element colour. One type annotation discoloured the rest of the sample.
//
// # What this does instead
//
// It hands the grammar `function` where the source says `component` or `hook`,
// and a plain brace where the source says `{|`, takes the tokens that
// produces, and rebuilds them over the original text. The grammar then walks
// the parameter list, the return type and the body the way it does for any
// function, and `flow-keywords.js` recolours the restored words.
//
// # Why this cannot corrupt the sample
//
// Because {@link restoreLine} never copies from the text the grammar saw. It
// maps each token's boundaries back into the original line and slices *that*,
// so the concatenation of a restored line is the original line by
// construction, whatever the grammar decided to do with a stand-in. A
// mis-shimmed line can come out with the wrong colours. It cannot come out
// saying `function`.
//
// # What it does not cover
//
// The declaration rewrite is anchored to the start of a line, after an
// optional `export` or `export default`, which is where a declaration begins
// and where the grammar breaks. A declaration written anywhere else is left to
// the ordinary path.
//
// Neither rewrite asks whether the line is inside a string or a comment, so a
// line of quoted sample code that opens with `component Name(` is rewritten
// too. The text still survives exactly; only its colours are a function's
// rather than a string's, and `flow-keywords.js` still refuses to call the
// word a keyword. Buying the remaining fidelity would mean lexing the block
// twice, once here and once there, to fix a case that is a code sample inside
// a code sample.

/**
 * A `component` or `hook` declaration head at the start of a line.
 *
 * The name and the opening bracket are matched but not captured: requiring
 * them is what distinguishes a declaration from `const component = 1`, and
 * consuming them would mean putting them back.
 */
const DECLARATION_HEAD =
  /^([ \t]*(?:export[ \t]+(?:default[ \t]+)?)?)(component|hook)(?=[ \t]+[A-Za-z_$][\w$]*[ \t]*[(<])/;

/**
 * The word a declaration keyword is shown as.
 *
 * `function` and not `function*`: a generator tokenises identically here, and
 * the length no longer has to match now that restoration maps positions rather
 * than assuming they line up.
 */
const DECLARATION_STAND_IN = "function";

/**
 * The braces of an exact object type, and the ordinary braces they are shown
 * as.
 *
 * Same length in both directions, so the rest of the line does not move; the
 * mapping would cope either way, but a rewrite that cannot shift anything is
 * one less thing to reason about. `{||}` — the empty exact object — is two
 * adjacent rewrites rather than an overlapping one, which is why these are
 * matched as a pair of two-character sequences rather than as one bracket.
 */
const EXACT_OBJECT = /\{\||\|\}/g;

const EXACT_OBJECT_STAND_INS = { "{|": "{ ", "|}": " }" };

/**
 * `source` rewritten for the grammar, with the undo that belongs to it.
 *
 * The undo is returned rather than exported separately because it closes over
 * the original lines, and pairing the wrong undo with a rewrite is the one
 * mistake that would matter. `restore` is the identity when nothing was
 * rewritten, so the caller has no case to distinguish.
 *
 * @param {string} source
 * @returns {{code: string, restore: (lines: Array<Array<object>>) => Array<Array<object>>}}
 */
export function shimFlowGrammar(source) {
  const original = source.split("\n");
  const edits = new Map();

  const shimmed = original.map((line, index) => {
    const lineEdits = editsFor(line);
    if (lineEdits.length === 0) {
      return line;
    }
    edits.set(index, lineEdits);
    return rewrite(line, lineEdits);
  });

  if (edits.size === 0) {
    return { code: source, restore: (lines) => lines };
  }
  return {
    code: shimmed.join("\n"),
    restore: (lines) => restore(lines, edits, original),
  };
}

/**
 * Every rewrite one line needs, in the order they occur.
 *
 * Ordered and non-overlapping, because {@link mapping} walks them once and
 * accumulates the shift each one makes. The declaration head is found first
 * and starts at the line's indentation, so it can never overlap an exact
 * object brace, which needs a `{` or a `|`.
 */
function editsFor(line) {
  const found = [];
  const head = DECLARATION_HEAD.exec(line);
  if (head != null) {
    found.push({ column: head[1].length, text: head[2], standIn: DECLARATION_STAND_IN });
  }
  for (const brace of line.matchAll(EXACT_OBJECT)) {
    found.push({
      column: brace.index ?? 0,
      text: brace[0],
      standIn: EXACT_OBJECT_STAND_INS[brace[0]],
    });
  }
  return found;
}

/** `line` with every rewrite applied, left to right. */
function rewrite(line, edits) {
  let out = "";
  let at = 0;
  for (const edit of edits) {
    out += line.slice(at, edit.column) + edit.standIn;
    at = edit.column + edit.text.length;
  }
  return out + line.slice(at);
}

/**
 * The tokenised block, rebuilt over the original source.
 *
 * Offsets are recomputed rather than adjusted: a stand-in that is not the
 * length of the text it stands for moves everything after it on the line, and
 * every line after that one, so there is no correct delta to add. Walking the
 * restored lines gives the right answer directly.
 */
function restore(lines, edits, original) {
  let offset = 0;
  return lines.map((tokens, index) => {
    const text = original[index] ?? tokens.map((token) => token.content).join("");
    const lineEdits = edits.get(index);
    const restored =
      lineEdits == null ? reoffset(tokens, offset) : restoreLine(tokens, text, lineEdits, offset);
    offset += text.length + 1;
    return restored;
  });
}

/** A line the grammar saw unchanged, with its offsets moved to the original. */
function reoffset(tokens, offset) {
  let at = 0;
  return tokens.map((token) => {
    const placed = { ...token, offset: offset + at };
    at += token.content.length;
    return placed;
  });
}

/**
 * One rewritten line's tokens, re-cut over the original text.
 *
 * Every token boundary is a position in the line the grammar saw; {@link
 * mapping} turns it into a position in the line the reader gets. A boundary
 * that falls *inside* a stand-in maps to the end of the text it stands for, so
 * that text always lands in exactly one piece and no character of the line is
 * dropped — the pieces still tile the line end to end even when the grammar
 * split a stand-in or swallowed it into a longer token.
 */
function restoreLine(tokens, text, edits, offset) {
  const map = mapping(edits);
  const out = [];
  let seen = 0;
  let at = 0;
  for (const token of tokens) {
    const from = map(seen);
    seen += token.content.length;
    const to = map(seen);
    if (to <= from) {
      continue;
    }
    out.push({ ...token, content: text.slice(from, to), offset: offset + at });
    at += to - from;
  }
  return out;
}

/** A position in the rewritten line, as a position in the original one. */
function mapping(edits) {
  return (at) => {
    let shift = 0;
    for (const edit of edits) {
      const from = edit.column + shift;
      if (at <= from) {
        return at - shift;
      }
      if (at < from + edit.standIn.length) {
        return edit.column + edit.text.length;
      }
      shift += edit.standIn.length - edit.text.length;
    }
    return at - shift;
  };
}
