// @flow
//
// `@uniflowed/std/glob`: slash-path glob matching, without a host file system.
//
// This is the other half of `path/filepath`: matching a package subpath, route,
// manifest entry or virtual file path against a pattern. It deliberately uses
// `/` on every runtime, treats `**` as recursive only when it owns a whole path
// segment, and never asks a host OS whether anything exists.

import { isAbsolute, normalize } from "./path.js";

type SegmentToken = { readonly parts: Array<SegmentPart>, readonly type: "segment" };
type GlobstarToken = { readonly type: "globstar" };
type Token = SegmentToken | GlobstarToken;
type CharRange = { readonly end: number, readonly start: number };
type SegmentPart =
  | { readonly type: "star" }
  | { readonly type: "question" }
  | { readonly type: "literal", readonly value: string }
  | {
      readonly negated: boolean,
      readonly ranges: Array<CharRange>,
      readonly type: "class",
    };

type CompiledPattern = {
  readonly absolute: boolean,
  readonly tokens: Array<Token>,
};

type ClassRead = {
  readonly end: number,
  readonly part: SegmentPart,
};

type ClassCharacter = {
  readonly end: number,
  readonly value: string,
};

/** A compiled slash-path glob pattern. */
export class GlobPattern {
  pattern: string;
  _absolute: boolean;
  _tokens: Array<Token>;

  constructor(pattern: string) {
    const compiled = compilePattern(pattern);
    this.pattern = pattern;
    this._absolute = compiled.absolute;
    this._tokens = compiled.tokens;
  }

  /** Whether `path` matches this pattern after lexical slash-path cleanup. */
  match(path: string): boolean {
    const cleaned = normalize(path);
    if (isAbsolute(cleaned) !== this._absolute) {
      return false;
    }
    return matchTokens(this._tokens, components(cleaned));
  }
}

/** Compile a slash-path glob pattern. */
export function glob(pattern: string): GlobPattern {
  return new GlobPattern(pattern);
}

/** Match `path` against a string pattern or a compiled [`GlobPattern`]. */
export function matchGlob(pattern: string | GlobPattern, path: string): boolean {
  return (typeof pattern === "string" ? glob(pattern) : pattern).match(path);
}

function compilePattern(pattern: string): CompiledPattern {
  const absolute = pattern.startsWith("/");
  const tokens = [];
  for (const segment of patternSegments(pattern)) {
    tokens.push(
      segment === "**"
        ? { type: "globstar" }
        : { type: "segment", parts: compileSegment(segment, pattern) },
    );
  }
  return { absolute, tokens };
}

function patternSegments(pattern: string): Array<string> {
  return pattern.split("/").filter((segment) => segment !== "" && segment !== ".");
}

function compileSegment(segment: string, pattern: string): Array<SegmentPart> {
  const parts = [];
  const chars = Array.from(segment);
  for (let index = 0; index < chars.length; index += 1) {
    const char = chars[index];
    if (char === "*") {
      if (parts.length === 0 || parts[parts.length - 1].type !== "star") {
        parts.push({ type: "star" });
      }
      continue;
    }
    if (char === "?") {
      parts.push({ type: "question" });
      continue;
    }
    if (char === "[") {
      const charClass = readClass(chars, index, pattern);
      parts.push(charClass.part);
      index = charClass.end;
      continue;
    }
    if (char === "\\") {
      index += 1;
      if (index >= chars.length) {
        throw badPattern(pattern);
      }
      parts.push({ type: "literal", value: chars[index] });
      continue;
    }
    parts.push({ type: "literal", value: char });
  }

  return parts;
}

function readClass(chars: Array<string>, start: number, pattern: string): ClassRead {
  let index = start + 1;
  const negated = chars[index] === "!" || chars[index] === "^";
  if (negated) {
    index += 1;
  }

  const ranges = [];
  let first = true;

  while (index < chars.length) {
    if (chars[index] === "]" && !first) {
      return { end: index, part: { negated, ranges, type: "class" } };
    }

    const startChar = readClassCharacter(chars, index, pattern);
    index = startChar.end + 1;
    if (index < chars.length - 1 && chars[index] === "-") {
      const endChar = readClassCharacter(chars, index + 1, pattern);
      const rangeStart = charCode(startChar.value, pattern);
      const rangeEnd = charCode(endChar.value, pattern);
      if (rangeStart > rangeEnd) {
        throw badPattern(pattern);
      }
      ranges.push({ end: rangeEnd, start: rangeStart });
      index = endChar.end + 1;
    } else {
      const code = charCode(startChar.value, pattern);
      ranges.push({ end: code, start: code });
    }
    first = false;
  }

  throw badPattern(pattern);
}

function readClassCharacter(chars: Array<string>, index: number, pattern: string): ClassCharacter {
  if (chars[index] === "\\") {
    const escaped = index + 1;
    if (escaped >= chars.length) {
      throw badPattern(pattern);
    }
    return { end: escaped, value: chars[escaped] };
  }
  return { end: index, value: chars[index] };
}

function matchTokens(tokens: Array<Token>, parts: Array<string>): boolean {
  const work = [{ partIndex: 0, tokenIndex: 0 }];
  const seen = new Set();

  while (work.length > 0) {
    const state = work.pop();
    if (state == null) {
      continue;
    }
    const key = `${String(state.tokenIndex)}:${String(state.partIndex)}`;
    if (seen.has(key)) {
      continue;
    }
    seen.add(key);

    if (state.tokenIndex === tokens.length) {
      if (state.partIndex === parts.length) {
        return true;
      }
      continue;
    }

    const token = tokens[state.tokenIndex];
    if (token.type === "globstar") {
      work.push({ partIndex: state.partIndex, tokenIndex: state.tokenIndex + 1 });
      if (state.partIndex < parts.length) {
        work.push({ partIndex: state.partIndex + 1, tokenIndex: state.tokenIndex });
      }
      continue;
    }

    if (state.partIndex < parts.length && matchSegment(token.parts, parts[state.partIndex])) {
      work.push({ partIndex: state.partIndex + 1, tokenIndex: state.tokenIndex + 1 });
    }
  }

  return false;
}

function matchSegment(parts: Array<SegmentPart>, segment: string): boolean {
  const chars = Array.from(segment);
  let partIndex = 0;
  let charIndex = 0;
  let starIndex = -1;
  let starCharIndex = 0;

  while (charIndex < chars.length) {
    const part = parts[partIndex];
    if (part != null && part.type !== "star" && matchSegmentPart(part, chars[charIndex])) {
      partIndex += 1;
      charIndex += 1;
      continue;
    }
    if (part != null && part.type === "star") {
      starIndex = partIndex;
      starCharIndex = charIndex;
      partIndex += 1;
      continue;
    }
    if (starIndex >= 0) {
      partIndex = starIndex + 1;
      starCharIndex += 1;
      charIndex = starCharIndex;
      continue;
    }
    return false;
  }

  while (partIndex < parts.length && parts[partIndex].type === "star") {
    partIndex += 1;
  }
  return partIndex === parts.length;
}

function matchSegmentPart(part: SegmentPart, char: string): boolean {
  if (part.type === "question") {
    return true;
  }
  if (part.type === "literal") {
    return part.value === char;
  }
  if (part.type === "class") {
    const code = charCode(char, "");
    let matched = false;
    for (const range of part.ranges) {
      if (code >= range.start && code <= range.end) {
        matched = true;
        break;
      }
    }
    return part.negated ? !matched : matched;
  }
  return false;
}

function charCode(char: string, pattern: string): number {
  const code = char.codePointAt(0);
  if (code == null) {
    throw badPattern(pattern);
  }
  return code;
}

function components(path: string): Array<string> {
  if (path === "." || path === "/") {
    return [];
  }
  const trimmed = path.startsWith("/") ? path.slice(1) : path;
  return trimmed === "" ? [] : trimmed.split("/");
}

function badPattern(pattern: string): SyntaxError {
  return new SyntaxError(`@uniflowed/std/glob: invalid glob pattern ${JSON.stringify(pattern)}`);
}
