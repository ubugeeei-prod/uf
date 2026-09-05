// @flow
//
// Which words the documentation build colours as Flow keywords.
//
// `component`, `hook`, `renders` and `match` are ordinary identifiers to a
// JavaScript grammar, so `@uniflowed/vite` re-tags them after the grammar
// has run. That is a decision about *words*, and the two ways it goes wrong
// are symmetrical: a keyword left grey, and an identifier that happens to
// share the spelling painted as a keyword.
//
// The second is the one that had bugs, and both of them came from the same
// place — the rule was decided per token, and the grammar splits a line into
// tokens wherever it likes.

import { describe, expect, it } from "@uniflowed/test";
// By path, not by subpath export. `internal/` is not part of any package's
// public surface — `package_surface.rs` enforces that, with one allowlisted
// exception for the native bridge — and a test is not a reason to widen it.
// This file is inside the repository, so it can just say where the module is.
import { markLine } from "../../packages/vite/internal/highlight.js";

const FLAG = Symbol.for("uf.flowKeyword");

/**
 * The words `markLine` marked, given a line already split into tokens.
 *
 * The splits are passed in because they are the whole problem: a grammar
 * emits `text.` and `match` separately, and a rule that cannot see past one
 * token cannot tell that apart from a `match` statement.
 */
function keywordsIn(tokens: $ReadOnlyArray<string>): Array<string> {
  return markLine(tokens.map((content) => ({ content, offset: 0 })))
    .filter((token) => token[FLAG] === true)
    .map((token) => token.content);
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
  });

  it("leaves a property name alone", () => {
    expect(keywordsIn(["{ hook: 1 }"])).toEqual([]);
    expect(keywordsIn(["{ +renders: React.Node }"])).toEqual([]);
  });

  it("leaves a longer word that starts with one alone", () => {
    expect(keywordsIn(["const components = matcher.hooks;"])).toEqual([]);
  });
});
