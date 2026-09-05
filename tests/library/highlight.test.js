// @flow
//
// Which words the documentation build colours as Flow's, and which Flow
// syntax it hands to the JavaScript grammar rewritten.
//
// `component`, `hook`, `renders` and `match` are ordinary identifiers to a
// JavaScript grammar, so `@uniflowed/vite` re-tags them after the grammar has
// run. That is a decision about *words*, and the two ways it goes wrong are
// symmetrical: a keyword left grey, and an identifier that happens to share
// the spelling painted as a keyword.
//
// The second is the one that had bugs, and both of them came from the same
// place — the rule was decided per token, and the grammar splits a line into
// tokens wherever it likes. So every case below asserts both directions: the
// word where it is Flow's, and the same spelling where it is a name, a member,
// a property, a string or a comment.
//
// Some Flow syntax cannot be fixed by re-tagging at all, because the grammar
// stops at it and never emits the tokens: a `component` or `hook` declaration
// head, and an exact object type. Those are rewritten *before* the grammar
// runs and restored afterwards, and the second half of this file is about the
// one property that rewrite must have — the reader sees the source, whatever
// the grammar made of the stand-in.

import { describe, expect, it } from "@uniflowed/test";
// By path, not by subpath export. `internal/` is not part of any package's
// public surface — `package_surface.rs` enforces that, with one allowlisted
// exception for the native bridge — and a test is not a reason to widen it.
// This file is inside the repository, so it can just say where the module is.
import { shimFlowGrammar } from "../../packages/vite/internal/flow-grammar-shim.js";
import {
  FLOW_MARK,
  KEYWORD,
  TYPE,
  markLine,
  markLines,
} from "../../packages/vite/internal/flow-keywords.js";

/**
 * The words `markLine` marked as keywords, given a line already split into
 * tokens.
 *
 * The splits are passed in because they are the whole problem: a grammar
 * emits `text.` and `match` separately, and a rule that cannot see past one
 * token cannot tell that apart from a `match` statement.
 */
function keywordsIn(tokens: $ReadOnlyArray<string>): Array<string> {
  return marksIn(tokens, KEYWORD);
}

/** The words it marked as Flow types, which are painted like `string`. */
function typesIn(tokens: $ReadOnlyArray<string>): Array<string> {
  return marksIn(tokens, TYPE);
}

function marksIn(tokens: $ReadOnlyArray<string>, kind: string): Array<string> {
  return markLine(tokens.map((content) => ({ content, offset: 0 })))
    .filter((token) => token[FLOW_MARK] === kind)
    .map((token) => token.content);
}

/** The keywords marked on each line of a block, which is where a comment or a
 * template literal that outlives its line is decided. */
function keywordsPerLine(lines: $ReadOnlyArray<string>): Array<Array<string>> {
  return markLines(lines.map((line) => [{ content: line, offset: 0 }])).map((line) =>
    line.filter((token) => token[FLOW_MARK] === KEYWORD).map((token) => token.content),
  );
}

/** The text a marked line still says, which marking must never change. */
function textOf(tokens: $ReadOnlyArray<string>): string {
  return markLine(tokens.map((content) => ({ content, offset: 0 })))
    .map((token) => token.content)
    .join("");
}

describe("Flow keywords in fenced code", () => {
  it("marks a declaration", () => {
    expect(keywordsIn(["export ", "component Avatar(size: number) renders React.Node {"])).toEqual([
      "component",
      "renders",
    ]);
  });

  it("marks a match statement", () => {
    expect(keywordsIn(["match (x) { 1 => f(), _ => g() }"])).toEqual(["match"]);
  });

  it("marks a hook declaration", () => {
    expect(keywordsIn(["hook useThing(): number {"])).toEqual(["hook"]);
  });

  it("leaves a member name alone, however the grammar split it", () => {
    // One token, which the old per-token rule got right.
    expect(keywordsIn(["text.match(re)"])).toEqual([]);
    // Three tokens, which is what a grammar actually produces — and what the
    // old look-behind could not see, because it began at the start of its
    // own token and found nothing before the `m`.
    expect(keywordsIn(["text.", "match", "(re)"])).toEqual([]);
    expect(keywordsIn(["a?.", "hook", "()"])).toEqual([]);
    // A member on its own line, which is how a chain is written.
    expect(keywordsIn(["  .", "match", "(x)"])).toEqual([]);
  });

  it("leaves a property name alone", () => {
    expect(keywordsIn(["{ hook: 1 }"])).toEqual([]);
    expect(keywordsIn(["{ +renders: React.Node }"])).toEqual([]);
    expect(keywordsIn(["{ hook: 1, match: 2, renders: 3, component: 4 }"])).toEqual([]);
  });

  it("leaves a longer word that starts with one alone", () => {
    expect(keywordsIn(["const components = matcher.hooks;"])).toEqual([]);
  });
});

describe("component and hook", () => {
  it("marks a declaration, however it is exported", () => {
    expect(keywordsIn(["export default ", "component Home() {"])).toEqual(["component"]);
    expect(keywordsIn(["  export component Card(title: string) {"])).toEqual(["component"]);
    expect(keywordsIn(["export hook useNow(interval: number): Date {"])).toEqual(["hook"]);
  });

  it("marks the function types that share the spelling", () => {
    expect(keywordsIn(["type Fn = component<T: {...}>(value: T) renders mixed;"])).toEqual([
      "component",
      "renders",
    ]);
    expect(keywordsIn(["type H = hook(a: number) => void;"])).toEqual(["hook"]);
  });

  it("leaves a variable of the same name alone", () => {
    // The case the header is about: a name is not a keyword because it is
    // spelled like one.
    expect(keywordsIn(["const component = 1;"])).toEqual([]);
    expect(keywordsIn(["const hook = () => {};"])).toEqual([]);
    expect(keywordsIn(["let component;"])).toEqual([]);
    expect(keywordsIn(['import { component } from "./x.js";'])).toEqual([]);
  });

  it("leaves a function or class of the same name alone", () => {
    expect(keywordsIn(["function component(props) {"])).toEqual([]);
    expect(keywordsIn(["class component {"])).toEqual([]);
    expect(keywordsIn(["new component(1);"])).toEqual([]);
  });

  it("leaves a lowercase JSX element alone", () => {
    expect(keywordsIn(["const el = <", "component", ' prop="x" />;'])).toEqual([]);
  });
});

describe("renders", () => {
  it("marks a render type and keeps its arity with it", () => {
    expect(keywordsIn(["component Tab(label: string) renders React.Node {"])).toEqual([
      "component",
      "renders",
    ]);
    // `renders*` is one word in Flow. Left to the grammar the `*` is a
    // multiplication and the `?` a ternary, so one word came out in two
    // colours.
    expect(keywordsIn(["component Tabs(children: renders* Tab) {"])).toEqual([
      "component",
      "renders*",
    ]);
    expect(keywordsIn(["component Maybe(children: renders? Tab) {"])).toEqual([
      "component",
      "renders?",
    ]);
  });

  it("leaves the spelling alone where no type follows", () => {
    expect(keywordsIn(["let renders = 1;"])).toEqual([]);
    expect(keywordsIn(["x.renders;"])).toEqual([]);
    expect(keywordsIn(["if (renders) {"])).toEqual([]);
    expect(keywordsIn(["const el = <Foo ", "renders", "={1} />;"])).toEqual([]);
  });
});

describe("match", () => {
  it("marks a match that has a subject and a block", () => {
    expect(keywordsIn(["const label = match (status) {"])).toEqual(["match"]);
    // A subject that runs past the end of the line: the block is on a line
    // this rule cannot see, and an unclosed subject is not a method call.
    expect(keywordsIn(["const out = match ("])).toEqual(["match"]);
  });

  it("leaves a call that only looks like one alone", () => {
    expect(keywordsIn(["text.match(re);"])).toEqual([]);
    // Both halves of a real `match` — a subject and a block — and still a
    // function called `match`.
    expect(keywordsIn(["function match(re) { return re; }"])).toEqual([]);
    expect(keywordsIn(["const match = (x) => { return x; };"])).toEqual([]);
  });
});

describe("opaque type", () => {
  it("marks the modifier of a type alias", () => {
    expect(keywordsIn(["opaque type ID = string;"])).toEqual(["opaque"]);
    expect(keywordsIn(["opaque type Sub: string = string;"])).toEqual(["opaque"]);
  });

  it("leaves the word alone anywhere else", () => {
    expect(keywordsIn(["const opaque = 2;"])).toEqual([]);
    expect(keywordsIn(["{ opaque: true }"])).toEqual([]);
    expect(keywordsIn(["surface.opaque;"])).toEqual([]);
  });
});

describe("import typeof", () => {
  it("marks the qualifier a grammar leaves grey beside `import type`", () => {
    expect(keywordsIn(['import typeof Bar from "./bar.js";'])).toEqual(["typeof"]);
  });

  it("leaves the operator alone, which the grammar already colours", () => {
    expect(keywordsIn(['if (typeof x === "string") {'])).toEqual([]);
    expect(keywordsIn(["const t = typeof y;"])).toEqual([]);
    expect(keywordsIn(["type B = $Keys<typeof obj>;"])).toEqual([]);
  });
});

describe("enum", () => {
  // `enum` itself is a keyword to every JavaScript grammar and needs nothing
  // from us. Its representation clause does not.
  it("marks the representation of a string enum", () => {
    expect(keywordsIn(["enum Status of string {"])).toEqual(["of"]);
    expect(keywordsIn(["export enum Mood of symbol {"])).toEqual(["of"]);
  });

  it("leaves every other `of` alone", () => {
    expect(keywordsIn(["for (const x of xs) {"])).toEqual([]);
    expect(keywordsIn(["enum Mood {"])).toEqual([]);
    expect(keywordsIn(["const of = 1;"])).toEqual([]);
  });
});

describe("predicate functions", () => {
  it("marks `%checks`, which a grammar paints as a function call", () => {
    expect(keywordsIn(["function truthy(a: mixed): boolean %checks {"])).toEqual(["%checks"]);
    // Split the way a grammar splits it: the `%` lands on one token and the
    // word on the next, and both pieces have to be marked or the marker comes
    // out half-coloured.
    expect(keywordsIn(["): boolean ", "%", "checks", "(typeof x === 'string');"])).toEqual([
      "%",
      "checks",
    ]);
  });

  it("leaves a variable called `checks` alone", () => {
    expect(keywordsIn(["const checks = 1;"])).toEqual([]);
    expect(keywordsIn(["const rest = total % checks;"])).toEqual([]);
  });
});

describe("mixed and empty", () => {
  it("marks them as types, not as keywords", () => {
    // A grammar gives `string` and `number` the type colour and leaves these
    // two grey. Marking them as keywords made them red instead, which said
    // `mixed` and `export` were the same kind of word.
    expect(typesIn(["function f(a: mixed): empty {"])).toEqual(["mixed", "empty"]);
    expect(keywordsIn(["function f(a: mixed): empty {"])).toEqual([]);
    expect(typesIn(["type T = mixed | string;"])).toEqual(["mixed"]);
    expect(typesIn(["type F = (x: string) => mixed;"])).toEqual(["mixed"]);
    expect(typesIn(["component Card() renders mixed {"])).toEqual(["mixed"]);
  });

  it("leaves a value of the same name alone", () => {
    expect(typesIn(["const mixed = 1;"])).toEqual([]);
    expect(typesIn(["obj.mixed;"])).toEqual([]);
    expect(typesIn(["{ mixed: 5 }"])).toEqual([]);
  });
});

describe("types a JavaScript grammar already handles", () => {
  // Listed so that a later rule cannot start marking inside them without a
  // test noticing. Every one of these is coloured by the grammar; adding a
  // mark would replace that colour, not add to it.
  it("touches nothing in a utility type, an indexed access or a cast", () => {
    expect(keywordsIn(["type A = $ReadOnly<{ a: string }>;"])).toEqual([]);
    expect(keywordsIn(['type V = Obj["key"];'])).toEqual([]);
    expect(keywordsIn(["type X = T[K];"])).toEqual([]);
    expect(keywordsIn(["const a = x as string;"])).toEqual([]);
    expect(keywordsIn(["const b = y as const;"])).toEqual([]);
    expect(keywordsIn(["declare export default class A {}"])).toEqual([]);
    expect(keywordsIn(["type P = {| +name: string, -seen?: boolean |};"])).toEqual([]);
  });
});

describe("strings, comments and regular expressions", () => {
  // The mark sets `color` with `!important`, so marking a word inside a string
  // does not add a colour — it replaces the string's own, in the middle of the
  // string. Both of these were live in this repository's documentation.
  it("leaves a word inside a string alone", () => {
    expect(keywordsIn(['const s = "component renders match hook";'])).toEqual([]);
    expect(keywordsIn(["const b = 'component Foo(';"])).toEqual([]);
    expect(keywordsIn(["const c = `hook useX()`;"])).toEqual([]);
    expect(keywordsIn(['const d = "a \\" component Foo(";'])).toEqual([]);
  });

  it("leaves a word inside a comment alone", () => {
    expect(keywordsIn(["// component hook renders match"])).toEqual([]);
    expect(keywordsIn(["/* and hook useX() too */"])).toEqual([]);
    expect(keywordsIn(["const x = 1; // see `component Foo(a: string)`"])).toEqual([]);
  });

  it("leaves a word inside a regular expression alone", () => {
    expect(keywordsIn(["const re = /component|hook|renders|match/g;"])).toEqual([]);
    // And still divides: a `/` after a name is not a literal, so the rest of
    // the line stays code.
    expect(keywordsIn(["const share = total / count; match (share) {"])).toEqual(["match"]);
  });

  it("still marks the code beside them", () => {
    expect(keywordsIn(['match ("idle") { _ => 1 }'])).toEqual(["match"]);
    expect(keywordsIn(["hook useX(): number { // a hook"])).toEqual(["hook"]);
  });

  it("carries a comment and a template past the end of their line", () => {
    // A line inside either of these is indistinguishable from code when it is
    // read on its own, which is why the block and not the line is the unit.
    expect(
      keywordsPerLine([
        "/*",
        "  component Foo(a: string) renders Node {}",
        "*/",
        "export component Real(a: string) {}",
      ]),
    ).toEqual([[], [], [], ["component"]]);
    expect(
      keywordsPerLine([
        "const src = `",
        "component Fake(a: string) {}",
        "`;",
        "const after = match (x) { _ => 1 };",
      ]),
    ).toEqual([[], [], [], ["match"]]);
  });

  it("does not carry a literal that closed on the line's last character", () => {
    // A literal that ends at the end of the line and one that never ends look
    // the same to anything that reports "no more line". They are not: reading
    // the first as the second redacted every line below it.
    expect(keywordsPerLine(["const a = `x`", "const m = match (y) { _ => 1 };"])).toEqual([
      [],
      ["match"],
    ]);
    expect(keywordsPerLine(['const b = "y"', "hook useX(): number {"])).toEqual([[], ["hook"]]);
    expect(keywordsPerLine(["const c = `still open", "match (y) { _ => 1 }"])).toEqual([[], []]);
  });
});

describe("marking never changes the code", () => {
  it("gives back every character it was given", () => {
    const lines = [
      ["export ", "component Avatar(size: number) renders* React.Node {"],
      ["): boolean ", "%", "checks", " {"],
      ['const s = "component"; // match'],
    ];
    for (const line of lines) {
      expect(textOf(line)).toBe(line.join(""));
    }
  });
});

describe("Flow's syntax shown to the JavaScript grammar", () => {
  // The grammar has no production for `component Name(`, so it abandons the
  // rest of the line — and, for `hook`, the rest of the block. Nothing can be
  // re-tagged afterwards because nothing was tokenised. Same for `{|`, which
  // the grammar reads as a brace and a bitwise or and then mis-scopes every
  // line after it.
  it("rewrites a declaration head as a function declaration", () => {
    expect(shimFlowGrammar("export component Avatar(src: string) {\n").code).toBe(
      "export function Avatar(src: string) {\n",
    );
    expect(shimFlowGrammar("export default hook useNow(): Date {\n").code).toBe(
      "export default function useNow(): Date {\n",
    );
    expect(shimFlowGrammar("  component Card<T>(a: T) {\n").code).toBe(
      "  function Card<T>(a: T) {\n",
    );
  });

  it("leaves a line that is not a declaration head alone", () => {
    for (const line of [
      "const component = 1;\n",
      "{ hook: 1 }\n",
      "  return component;\n",
      "type Fn = component<T>(v: T) renders mixed;\n",
    ]) {
      expect(shimFlowGrammar(line).code).toBe(line);
    }
  });

  it("rewrites the braces of an exact object type", () => {
    expect(shimFlowGrammar("type P = {| +name: string |};\n").code).toBe(
      "type P = {  +name: string  };\n",
    );
    expect(shimFlowGrammar("type Empty = {||};\n").code).toBe("type Empty = {  };\n");
  });

  it("gives back the source, whatever the grammar made of the stand-in", () => {
    // The one property the rewrite has to have. Restoration slices the
    // *original* line at the mapped token boundaries, so a restored block is
    // the source by construction — a mis-shimmed line can come out the wrong
    // colour, it cannot come out saying `function`.
    const source = [
      "export component Post(params: {| +slug: string |}) renders Node {",
      "  return null;",
      "}",
    ].join("\n");
    const shim = shimFlowGrammar(source);

    // Three token splits a grammar might plausibly choose, including the two
    // that the position mapping exists for: the stand-in cut in half, and the
    // stand-in swallowed into a longer token.
    const splits = [
      (line) => [line],
      (line) => line.split(/(?<=\s)/u),
      (line) => [line.slice(0, 11), line.slice(11)],
    ];
    for (const split of splits) {
      const lines = shim.code.split("\n").map((line) =>
        split(line)
          .filter((part) => part !== "")
          .map((content) => ({ content })),
      );
      const restored = shim
        .restore(lines)
        .map((line) => line.map((token) => token.content).join(""))
        .join("\n");
      expect(restored).toBe(source);
    }
  });

  it("numbers the restored tokens against the source, not the rewrite", () => {
    // `offset` is what maps a token back to the file. A stand-in that is not
    // the length of the word it replaces moves every offset after it, so they
    // are recomputed rather than adjusted.
    const source = "export hook useNow(): Date {\nreturn 1;\n";
    const shim = shimFlowGrammar(source);
    const lines = shim.code.split("\n").map((line) => [{ content: line, offset: 0 }]);
    const restored = shim.restore(lines);
    expect(restored[0][0].offset).toBe(0);
    expect(restored[1][0].offset).toBe(source.indexOf("return"));
  });
});
