// @flow
//
// `@uniflowed/validator` under the runner that ships with the toolchain.

import { describe, expect, it } from "@uniflowed/test";
import {
  ValidationError,
  array,
  boolean,
  check,
  date,
  email,
  endsWith,
  enum_,
  fallback,
  instance,
  integer,
  lazy,
  literal,
  max,
  maxLength,
  min,
  minLength,
  nullable,
  number,
  object,
  optional,
  parse,
  partial,
  pipe,
  record,
  regex,
  safeParse,
  startsWith,
  strictObject,
  string,
  transform,
  trim,
  tuple,
  union,
  unknown,
  v,
  variant,
} from "@uniflowed/validator";

describe("primitives", () => {
  it("accepts and rejects by type", () => {
    expect(parse(string(), "x")).toBe("x");
    expect(parse(number(), 1)).toBe(1);
    expect(parse(boolean(), true)).toBe(true);
    expect(safeParse(string(), 1).ok).toBe(false);
    expect(safeParse(boolean(), "true").ok).toBe(false);
  });

  it("rejects NaN and the infinities as numbers", () => {
    expect(safeParse(number(), Number.NaN).ok).toBe(false);
    expect(safeParse(number(), Number.POSITIVE_INFINITY).ok).toBe(false);
  });

  it("lets anything through unknown", () => {
    expect(parse(unknown(), Symbol.iterator)).toBe(Symbol.iterator);
  });

  it("matches a literal by identity", () => {
    expect(parse(literal("on"), "on")).toBe("on");
    expect(safeParse(literal("on"), "off").ok).toBe(false);
  });

  it("matches one of an enum, and names them all when it does not", () => {
    const level = enum_<string>(["debug", "info", "warn"]);
    expect(parse(level, "info")).toBe("info");
    const failed = safeParse(level, "trace");
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].message).toBe("expected one of debug, info, warn");
    }
  });

  it("accepts only a real Date", () => {
    const when = new Date("2026-01-01T00:00:00Z");
    expect(parse(date(), when)).toBe(when);
    expect(safeParse(date(), new Date("nope")).ok).toBe(false);
    expect(safeParse(date(), "2026-01-01").ok).toBe(false);
  });

  it("accepts an instance of a class", () => {
    expect(parse(instance(Map), new Map())).toBeInstanceOf(Map);
    expect(safeParse(instance(Map), new Set()).ok).toBe(false);
  });
});

describe("collections", () => {
  it("parses every item of an array", () => {
    expect(parse(array(number()), [1, 2, 3])).toEqual([1, 2, 3]);
    expect(safeParse(array(number()), [1, "2"]).ok).toBe(false);
    expect(safeParse(array(number()), "nope").ok).toBe(false);
  });

  it("points at the item that failed", () => {
    const failed = safeParse(array(number()), [1, 2, "three"]);
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].path).toEqual(["2"]);
    }
  });

  it("checks a tuple's arity and each position", () => {
    const pair = tuple([string(), number()]);
    expect(parse(pair, ["a", 1])).toEqual(["a", 1]);
    expect(safeParse(pair, ["a"]).ok).toBe(false);
    expect(safeParse(pair, ["a", "b"]).ok).toBe(false);
  });

  it("parses every value of a record", () => {
    const scores = record(number());
    expect(parse(scores, { a: 1, b: 2 })).toEqual({ a: 1, b: 2 });
    const failed = safeParse(scores, { a: 1, b: "two" });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].path).toEqual(["b"]);
    }
  });
});

describe("objects", () => {
  const user = object({ name: string(), age: number() });

  it("keeps the shape's keys and drops the rest", () => {
    expect(parse(user, { name: "ada", age: 36, extra: true })).toEqual({ name: "ada", age: 36 });
  });

  it("reports every bad field at once, each with its path", () => {
    const failed = safeParse(user, { name: 1, age: "x" });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues).toHaveLength(2);
      expect(failed.issues[0].path).toEqual(["name"]);
      expect(failed.issues[1].path).toEqual(["age"]);
    }
  });

  it("reports a nested path from the root", () => {
    const team = object({ lead: object({ contact: object({ email: string() }) }) });
    const failed = safeParse(team, { lead: { contact: { email: 42 } } });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].path).toEqual(["lead", "contact", "email"]);
    }
  });

  it("does not confuse paths between sibling branches", () => {
    const pair = object({ left: array(number()), right: array(number()) });
    const failed = safeParse(pair, { left: [1, "x"], right: ["y"] });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues.map((entry) => entry.path)).toEqual([
        ["left", "1"],
        ["right", "0"],
      ]);
    }
  });

  it("rejects an unknown key only in a strict object", () => {
    const strict = strictObject({ name: string() });
    expect(parse(object({ name: string() }), { name: "ada", extra: 1 })).toEqual({ name: "ada" });
    const failed = safeParse(strict, { name: "ada", extra: 1 });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].code).toBe("unknown_key");
      expect(failed.issues[0].path).toEqual(["extra"]);
    }
  });

  it("makes every field optional in a partial", () => {
    const draft = partial({ name: string(), age: number() });
    expect(parse(draft, {})).toEqual({ name: undefined, age: undefined });
    expect(parse(draft, { name: "ada" })).toEqual({ name: "ada", age: undefined });
    expect(safeParse(draft, { name: 1 }).ok).toBe(false);
  });

  it("rejects an array where an object is expected", () => {
    expect(safeParse(user, [] as $FlowFixMe).ok).toBe(false);
  });
});

describe("optional, nullable and fallback", () => {
  it("lets undefined through an optional", () => {
    expect(parse(optional(string()), undefined)).toBe(undefined);
    expect(parse(optional(string()), "x")).toBe("x");
    expect(safeParse(optional(string()), null).ok).toBe(false);
  });

  it("lets null through a nullable", () => {
    expect(parse(nullable(string()), null)).toBe(null);
    expect(safeParse(nullable(string()), undefined).ok).toBe(false);
  });

  it("substitutes a fallback instead of failing", () => {
    const port = fallback(number(), 8080);
    expect(parse(port, 3000)).toBe(3000);
    expect(parse(port, "not a port")).toBe(8080);
  });
});

describe("unions", () => {
  it("takes the first branch that accepts", () => {
    const scalar = union<mixed>([string(), number()]);
    expect(parse(scalar, "a")).toBe("a");
    expect(parse(scalar, 1)).toBe(1);
    expect(safeParse(scalar, true).ok).toBe(false);
  });

  it("reports only the matching branch of a variant", () => {
    const shape = variant<mixed>("kind", {
      circle: object({ kind: literal("circle"), radius: number() }),
      square: object({ kind: literal("square"), side: number() }),
    });
    expect(parse(shape, { kind: "square", side: 2 })).toEqual({ kind: "square", side: 2 });

    const failed = safeParse(shape, { kind: "circle", radius: "big" });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues).toHaveLength(1);
      expect(failed.issues[0].path).toEqual(["radius"]);
    }
  });

  it("names the known discriminants when none match", () => {
    const shape = variant<mixed>("kind", { circle: object({ kind: literal("circle") }) });
    const failed = safeParse(shape, { kind: "hexagon" });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].code).toBe("variant");
      expect(failed.issues[0].message).toBe("expected one of circle");
      expect(failed.issues[0].path).toEqual(["kind"]);
    }
  });
});

describe("lazy", () => {
  it("describes a recursive shape", () => {
    const comment = lazy<mixed>(() =>
      object({ text: string(), replies: array(comment) }),
    );
    const tree = { text: "root", replies: [{ text: "child", replies: [] }] };
    expect(parse(comment, tree)).toEqual(tree);
    expect(safeParse(comment, { text: "root", replies: [{ text: 1, replies: [] }] }).ok).toBe(
      false,
    );
  });
});

describe("pipe", () => {
  it("applies several steps left to right", () => {
    const handle = pipe(string(), minLength(3), maxLength(8), startsWith("@"));
    expect(parse(handle, "@ada")).toBe("@ada");
    expect(safeParse(handle, "@a").ok).toBe(false);
    expect(safeParse(handle, "ada").ok).toBe(false);
    expect(safeParse(handle, "@abcdefghij").ok).toBe(false);
  });

  it("is the identity with no steps", () => {
    expect(parse(pipe(string()), "x")).toBe("x");
  });

  it("transforms after validating", () => {
    const length = pipe(string(), minLength(2), transform((text: string) => text.length));
    expect(parse(length, "abc")).toBe(3);
    expect(safeParse(length, "a").ok).toBe(false);
  });

  it("trims before checking length, in the order written", () => {
    const name = pipe(string(), trim(), minLength(1));
    expect(parse(name, "  ada  ")).toBe("ada");
    expect(safeParse(name, "   ").ok).toBe(false);
  });

  it("checks numeric bounds and integrality", () => {
    const age = pipe(number(), integer(), min(0), max(150));
    expect(parse(age, 36)).toBe(36);
    expect(safeParse(age, 36.5).ok).toBe(false);
    expect(safeParse(age, -1).ok).toBe(false);
    expect(safeParse(age, 200).ok).toBe(false);
  });

  it("matches a regular expression", () => {
    const slug = pipe(string(), regex(/^[a-z-]+$/, "expected a slug"));
    expect(parse(slug, "hello-world")).toBe("hello-world");
    const failed = safeParse(slug, "Hello World");
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].message).toBe("expected a slug");
    }
  });

  it("survives a global regular expression used twice", () => {
    const digits = pipe(string(), regex(/\d+/g));
    expect(parse(digits, "123")).toBe("123");
    expect(parse(digits, "456")).toBe("456");
  });

  it("checks a suffix and an address", () => {
    expect(parse(pipe(string(), endsWith(".js")), "index.js")).toBe("index.js");
    expect(safeParse(pipe(string(), endsWith(".js")), "index.ts").ok).toBe(false);
    expect(parse(pipe(string(), email()), "ada@example.com")).toBe("ada@example.com");
    expect(safeParse(pipe(string(), email()), "ada").ok).toBe(false);
  });

  it("takes an arbitrary predicate through check", () => {
    const even = pipe(number(), check((n: number) => n % 2 === 0, "expected an even number"));
    expect(parse(even, 4)).toBe(4);
    const failed = safeParse(even, 5);
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].message).toBe("expected an even number");
      expect(failed.issues[0].code).toBe("check");
    }
  });

  it("reports a step's failure at the field's path", () => {
    const form = object({ email: pipe(string(), email()) });
    const failed = safeParse(form, { email: "nope" });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].path).toEqual(["email"]);
    }
  });
});

describe("parse", () => {
  it("raises a ValidationError carrying the issues", () => {
    let caught = null;
    try {
      parse(object({ name: string(), age: number() }), { name: 1, age: "x" });
    } catch (error) {
      caught = error;
    }
    expect(caught).toBeInstanceOf(ValidationError);
    expect(caught).toBeInstanceOf(Error);
    if (caught instanceof ValidationError) {
      expect(caught.name).toBe("ValidationError");
      expect(caught.issues).toHaveLength(2);
      expect(caught.issues[0].path).toEqual(["name"]);
      expect(caught.message).toBe("expected string at name; expected number at age");
    }
  });

  it("says what it wanted when the root itself is wrong", () => {
    expect(() => parse(string(), 1)).toThrow("expected string");
  });
});

describe("v", () => {
  it("is the same builders under one name", () => {
    const user = v.object({ name: v.pipe(v.string(), v.minLength(1)) });
    expect(v.parse(user, { name: "ada" })).toEqual({ name: "ada" });
    expect(v.safeParse(user, { name: "" }).ok).toBe(false);
    expect(v.string).toBe(string);
    expect(v.enum).toBe(enum_);
  });
});

describe("what the review found", () => {
  it("reports an unknown key and a field failure together", () => {
    const schema = v.strictObject({ name: v.string() });
    const result = v.safeParse(schema, { name: 1, extra: true });

    expect(result.ok).toBe(false);
    if (result.ok) {
      throw new Error("expected a failure");
    }
    const kinds = result.issues.map((entry) => entry.code).sort();
    // Reporting only the first meant fixing `name` revealed `extra`, which is
    // the behaviour this validator collects issues to avoid.
    expect(kinds).toEqual(["type", "unknown_key"]);
  });

  it("does not let a __proto__ key reach the prototype", () => {
    const schema = v.record(v.number());
    const hostile = JSON.parse('{"__proto__": 1, "safe": 2}');
    const result = v.safeParse(schema, hostile);

    expect(result.ok).toBe(true);
    if (!result.ok) {
      throw new Error("expected a success");
    }
    // An own property, and the prototype untouched — `out[key] = …` would have
    // run the legacy setter and changed the prototype instead.
    expect(Object.prototype.hasOwnProperty.call(result.value, "__proto__")).toBe(true);
    expect(Object.getPrototypeOf(result.value)).toBe(Object.prototype);
    expect(({}: any).__proto__).toBe(Object.prototype);
  });
});
