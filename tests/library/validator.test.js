// @flow
//
// `@uniflowed/validator` under the runner that ships with the toolchain.

import { describe, expect, it } from "@uniflowed/test";
import { validatorResolver } from "@uniflowed/form/validator";
import type { InferInput, InferOutput, Schema } from "@uniflowed/validator";
import {
  ValidationError,
  array,
  bigint,
  boolean,
  check,
  checkAsync,
  custom,
  date,
  describe as describeSchema,
  email,
  endsWith,
  enum_,
  fallback,
  flatten,
  includes,
  instance,
  integer,
  intersect,
  is,
  isAsync,
  isoDate,
  lazy,
  lazyAsync,
  length,
  literal,
  looseObject,
  map,
  max,
  maxItems,
  maxLength,
  min,
  minItems,
  minLength,
  multipleOf,
  never,
  nonEmpty,
  null_,
  nullable,
  nullish,
  number,
  object,
  optional,
  parse,
  parseAsync,
  partial,
  pipe,
  record,
  regex,
  safeParse,
  safeParseAsync,
  set,
  startsWith,
  strictObject,
  string,
  toJsonSchema,
  toLowerCase,
  toUpperCase,
  transform,
  transformAsync,
  trim,
  tuple,
  undefined_,
  union,
  unknown,
  url,
  uuid,
  v,
  variant,
  withDefault,
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
    const scalar = union([string(), number()]);
    expect(parse(scalar, "a")).toBe("a");
    expect(parse(scalar, 1)).toBe(1);
    expect(safeParse(scalar, true).ok).toBe(false);
  });

  it("reports only the matching branch of a variant", () => {
    const shape = variant("kind", {
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
    const shape = variant("kind", { circle: object({ kind: literal("circle") }) });
    const failed = safeParse(shape, { kind: "hexagon" });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].code).toBe("variant");
      expect(failed.issues[0].message).toBe("expected one of circle");
      expect(failed.issues[0].path).toEqual(["kind"]);
    }
  });
});

type CommentValue = {| readonly text: string, readonly replies: $ReadOnlyArray<CommentValue> |};

describe("lazy", () => {
  // The annotation is the half Flow cannot infer: `object` reads its type from
  // its shape, and the shape mentions the binding being defined. `infer.js`
  // says so, and `lazy.js` has the same example.
  const comment: Schema<CommentValue> = lazy(() =>
    object({ text: string(), replies: array(comment) }),
  );

  it("describes a recursive shape", () => {
    const tree = { text: "root", replies: [{ text: "child", replies: [] }] };
    expect(parse(comment, tree)).toEqual(tree);
    expect(safeParse(comment, { text: "root", replies: [{ text: 1, replies: [] }] }).ok).toBe(
      false,
    );
  });

  it("points into a reply three levels down", () => {
    const failed = safeParse(comment, {
      text: "root",
      replies: [{ text: "child", replies: [{ text: 7, replies: [] }] }],
    });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].path).toEqual(["replies", "0", "replies", "0", "text"]);
    }
  });

  it("builds the schema once, however deep the value is", () => {
    let built = 0;
    const node: Schema<CommentValue> = lazy(() => {
      built += 1;
      return object({ text: string(), replies: array(node) });
    });
    parse(node, {
      text: "a",
      replies: [{ text: "b", replies: [{ text: "c", replies: [] }] }],
    });
    expect(built).toBe(1);
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
    const length = pipe(
      string(),
      minLength(2),
      transform((text: string) => text.length),
    );
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
    const even = pipe(
      number(),
      check((n: number) => n % 2 === 0, "expected an even number"),
    );
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
    expect(({} as any).__proto__).toBe(Object.prototype);
  });
});

describe("more of the vocabulary", () => {
  it("keeps a bigint away from a number", () => {
    expect(parse(bigint(), 7n)).toBe(7n);
    expect(safeParse(bigint(), 7).ok).toBe(false);
    expect(safeParse(number(), 7n).ok).toBe(false);
  });

  it("refuses everything through never", () => {
    expect(safeParse(never(), undefined).ok).toBe(false);
    expect(safeParse(never(), 1).ok).toBe(false);
  });

  it("keeps null and undefined apart", () => {
    expect(parse(null_(), null)).toBe(null);
    expect(safeParse(null_(), undefined).ok).toBe(false);
    expect(parse(undefined_(), undefined)).toBe(undefined);
    expect(safeParse(undefined_(), null).ok).toBe(false);
  });

  it("takes the caller's word through custom", () => {
    const bytes = custom<Uint8Array>(
      (value) => value instanceof Uint8Array,
      "expected bytes",
      "Uint8Array",
    );
    expect(parse(bytes, new Uint8Array([1, 2])).length).toBe(2);
    const failed = safeParse(bytes, "nope");
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].message).toBe("expected bytes");
    }
  });

  it("keeps the unnamed keys only in a loose object", () => {
    const loose = looseObject({ name: string() });
    expect(parse(loose, { name: "ada", extra: 1 })).toEqual({ name: "ada", extra: 1 });
    expect(parse(object({ name: string() }), { name: "ada", extra: 1 })).toEqual({ name: "ada" });
  });

  it("does not read a field off the prototype", () => {
    // `record[key]` would have found `Object.prototype.constructor` and called
    // the payload well-formed; only own keys count.
    const schema = object({ constructor: string() });
    expect(safeParse(schema, {}).ok).toBe(false);
  });

  it("answers yes or no through is", () => {
    expect(is(string(), "x")).toBe(true);
    expect(is(string(), 1)).toBe(false);
  });
});

describe("maps and sets", () => {
  it("parses a Map's keys and values", () => {
    const scores = map(string(), number());
    const input: Map<mixed, mixed> = new Map();
    input.set("ada", 36);
    const parsed = parse(scores, input);
    expect(parsed.get("ada")).toBe(36);
    expect(safeParse(scores, { ada: 36 }).ok).toBe(false);
  });

  it("says whether the key or the value was wrong", () => {
    const scores = map(string(), number());
    const input: Map<mixed, mixed> = new Map();
    input.set("ada", 36);
    input.set(2, "grace");
    const failed = safeParse(scores, input);
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues.map((entry) => entry.path)).toEqual([
        ["1", "key"],
        ["1", "value"],
      ]);
    }
  });

  it("parses a Set, and folds members a step made equal", () => {
    const names = set(pipe(string(), trim()));
    const input: Set<mixed> = new Set();
    input.add("ada");
    input.add("ada ");
    expect(parse(names, input).size).toBe(1);
  });

  it("points at the position of a bad set member", () => {
    const numbers = set(number());
    const input: Set<mixed> = new Set();
    input.add(1);
    input.add("two");
    const failed = safeParse(numbers, input);
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].path).toEqual(["1"]);
    }
  });
});

describe("intersect", () => {
  const both = intersect(object({ id: string() }), object({ role: string() }));

  it("merges two object shapes", () => {
    expect(parse(both, { id: "1", role: "admin" })).toEqual({ id: "1", role: "admin" });
  });

  it("reports both sides' issues at once", () => {
    const failed = safeParse(both, { id: 1, role: 2 });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues.map((entry) => entry.path)).toEqual([["id"], ["role"]]);
    }
  });

  it("refuses two scalars that disagree", () => {
    const shouting = intersect(string(), pipe(string(), toUpperCase()));
    expect(parse(shouting, "ABC")).toBe("ABC");
    const failed = safeParse(shouting, "abc");
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].code).toBe("intersect");
    }
  });
});

describe("nullish and defaults", () => {
  it("lets either absence through a nullish", () => {
    const maybe = nullish(string());
    expect(parse(maybe, null)).toBe(null);
    expect(parse(maybe, undefined)).toBe(undefined);
    expect(parse(maybe, "x")).toBe("x");
    expect(safeParse(maybe, 1).ok).toBe(false);
  });

  it("substitutes a default for undefined only", () => {
    const page = withDefault(number(), 1);
    expect(parse(page, 7)).toBe(7);
    expect(parse(page, undefined)).toBe(1);
    expect(safeParse(page, null).ok).toBe(false);
    expect(safeParse(page, "7").ok).toBe(false);
  });
});

describe("cross-field checks", () => {
  const passwords = pipe(
    object({ password: string(), confirm: string() }),
    check((form) => form.password === form.confirm, "Passwords must match", ["confirm"]),
  );

  it("reports on the field the user has to change", () => {
    expect(parse(passwords, { password: "a", confirm: "a" })).toEqual({
      password: "a",
      confirm: "a",
    });
    const failed = safeParse(passwords, { password: "a", confirm: "b" });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].path).toEqual(["confirm"]);
      expect(failed.issues[0].message).toBe("Passwords must match");
    }
  });

  it("keeps the forwarded path relative to where the object was", () => {
    const form = object({ account: passwords });
    const failed = safeParse(form, { account: { password: "a", confirm: "b" } });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].path).toEqual(["account", "confirm"]);
    }
  });
});

describe("flatten", () => {
  it("groups issues the way a form renders them", () => {
    const schema = pipe(
      object({ name: string(), age: number() }),
      check(() => false, "This account is not allowed"),
    );
    const failed = safeParse(schema, { name: 1, age: "x" });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(flatten(failed.issues)).toEqual({
        root: [],
        nested: { name: ["expected string"], age: ["expected number"] },
      });
    }
    const rejected = safeParse(schema, { name: "ada", age: 36 });
    expect(rejected.ok).toBe(false);
    if (!rejected.ok) {
      expect(flatten(rejected.issues)).toEqual({
        root: ["This account is not allowed"],
        nested: {},
      });
    }
  });
});

describe("inference", () => {
  const Account = object({
    email: pipe(string(), trim(), email()),
    age: pipe(string(), transform(Number), integer(), min(18)),
    tags: array(pipe(string(), nonEmpty())),
  });

  it("reads the input and the output type off one schema", () => {
    const raw: InferInput<typeof Account> = {
      email: " ada@example.com ",
      age: "36",
      tags: ["admin"],
    };
    const parsed: InferOutput<typeof Account> = parse(Account, raw);
    expect(parsed.email).toBe("ada@example.com");
    expect(parsed.age).toBe(36);
    expect(parsed.tags).toEqual(["admin"]);
  });

  it("rejects a value the schema does not describe", () => {
    // The type-level half of the test above: if the inferred output ever
    // widened, this suppression would be unused and the checker would say so.
    // `flow` sees it; `uf check` does not yet, because it resolves no module
    // for `@uniflowed/validator` and types the import as `any` — the same gap
    // that makes `packages/form/validator.js` report `Issue` as any-typed.
    // $FlowExpectedError[incompatible-type] `age` is a number after the transform.
    const wrong: InferOutput<typeof Account> = { email: "a@b.c", age: "36", tags: [] };
    expect(wrong.age).toBe("36");
  });

  it("infers a tuple position by position", () => {
    const pair = tuple([string(), pipe(string(), transform(Number))]);
    const raw: InferInput<typeof pair> = ["a", "1"];
    const parsed: InferOutput<typeof pair> = parse(pair, raw);
    expect(parsed).toEqual(["a", 1]);
  });

  it("infers the branches of a variant", () => {
    const shape = variant("kind", {
      circle: object({ kind: literal("circle"), radius: number() }),
      square: object({ kind: literal("square"), side: number() }),
    });
    const parsed: InferOutput<typeof shape> = parse(shape, { kind: "circle", radius: 2 });
    expect(parsed.kind).toBe("circle");
  });
});

describe("asynchronous schemas", () => {
  const delay = (ms: number): Promise<void> =>
    new Promise((resolve) => {
      setTimeout(resolve, ms);
    });

  const taken = new Set(["ada"]);
  const free = pipe(
    string(),
    checkAsync(async (name: string) => {
      await delay(1);
      return !taken.has(name);
    }, "That name is taken"),
  );

  it("knows it is asynchronous before it runs", () => {
    expect(isAsync(free)).toBe(true);
    expect(isAsync(string())).toBe(false);
    expect(isAsync(object({ name: free }))).toBe(true);
    expect(isAsync(array(free))).toBe(true);
  });

  it("refuses a synchronous parse, and says which entry point to use", () => {
    expect(() => safeParse(free, "grace")).toThrow("safeParseAsync");
    expect(() => parse(object({ name: free }), { name: "grace" })).toThrow("safeParseAsync");
  });

  it("waits for a check that has to ask", async () => {
    expect(await parseAsync(free, "grace")).toBe("grace");
    const failed = await safeParseAsync(free, "ada");
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].message).toBe("That name is taken");
    }
  });

  it("reports an asynchronous failure at the field's path", async () => {
    const form = object({ profile: object({ handle: free }) });
    const failed = await safeParseAsync(form, { profile: { handle: "ada" } });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].path).toEqual(["profile", "handle"]);
    }
  });

  it("runs an object's asynchronous fields at the same time", async () => {
    const order: Array<string> = [];
    const after = (ms: number, name: string) =>
      pipe(
        string(),
        checkAsync(async () => {
          await delay(ms);
          order.push(name);
          return true;
        }, "never"),
      );
    const schema = object({ slow: after(40, "slow"), fast: after(1, "fast") });
    const result = await safeParseAsync(schema, { slow: "a", fast: "b" });

    expect(result.ok).toBe(true);
    // Sequentially, `slow` is the first field and would have finished first.
    expect(order).toEqual(["fast", "slow"]);
  });

  it("keeps the paths of two asynchronous fields apart", async () => {
    const schema = object({ left: array(free), right: array(free) });
    const failed = await safeParseAsync(schema, { left: ["grace", "ada"], right: ["ada"] });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues.map((entry) => entry.path)).toEqual([
        ["left", "1"],
        ["right", "0"],
      ]);
    }
  });

  it("transforms asynchronously, and changes the output type", async () => {
    const lookup = pipe(
      string(),
      transformAsync(async (id: string) => {
        await delay(1);
        return id.length;
      }),
      min(2),
    );
    expect(await parseAsync(lookup, "abc")).toBe(3);
    expect((await safeParseAsync(lookup, "a")).ok).toBe(false);
  });

  it("takes the first branch of an asynchronous union, in order", async () => {
    const tried: Array<string> = [];
    const branch = (name: string, accept: boolean) =>
      pipe(
        string(),
        checkAsync(async () => {
          await delay(1);
          tried.push(name);
          return accept;
        }, `not ${name}`),
      );
    const either = union([branch("first", false), branch("second", true), branch("third", true)]);

    expect(await parseAsync(either, "x")).toBe("x");
    // The third branch would also have accepted, and was never asked.
    expect(tried).toEqual(["first", "second"]);
  });

  it("recurses through lazyAsync", async () => {
    const node: Schema<CommentValue> = lazyAsync(() =>
      object({ text: free, replies: array(node) }),
    );
    const tree = { text: "grace", replies: [{ text: "hopper", replies: [] }] };
    expect(await parseAsync(node, tree)).toEqual(tree);

    const failed = await safeParseAsync(node, {
      text: "grace",
      replies: [{ text: "ada", replies: [] }],
    });
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].path).toEqual(["replies", "0", "text"]);
    }
  });

  it("accepts a synchronous schema through the asynchronous entry points", async () => {
    expect(await parseAsync(object({ name: string() }), { name: "ada" })).toEqual({ name: "ada" });
    expect((await safeParseAsync(string(), 1)).ok).toBe(false);
  });
});

describe("the rest of the actions", () => {
  it("checks a URL, a UUID and a calendar date", () => {
    expect(parse(pipe(string(), url()), "https://uf.dev/a?b=1")).toBe("https://uf.dev/a?b=1");
    expect(safeParse(pipe(string(), url()), "uf.dev").ok).toBe(false);

    const id = "f81d4fae-7dec-11d0-a765-00a0c91e6bf6";
    expect(parse(pipe(string(), uuid()), id)).toBe(id);
    expect(safeParse(pipe(string(), uuid()), "not-a-uuid").ok).toBe(false);

    expect(parse(pipe(string(), isoDate()), "2026-02-28")).toBe("2026-02-28");
    // The shape matches and the day does not exist.
    expect(safeParse(pipe(string(), isoDate()), "2026-02-30").ok).toBe(false);
    expect(safeParse(pipe(string(), isoDate()), "28-02-2026").ok).toBe(false);
  });

  it("counts characters exactly, and counts items separately", () => {
    expect(parse(pipe(string(), length(3)), "abc")).toBe("abc");
    expect(safeParse(pipe(string(), length(3)), "ab").ok).toBe(false);

    const few = pipe(array(number()), minItems(1), maxItems(2));
    expect(parse(few, [1, 2])).toEqual([1, 2]);
    expect(safeParse(few, []).ok).toBe(false);
    expect(safeParse(few, [1, 2, 3]).ok).toBe(false);
  });

  it("looks for a substring, and changes case", () => {
    expect(parse(pipe(string(), includes("@")), "a@b")).toBe("a@b");
    expect(safeParse(pipe(string(), includes("@")), "ab").ok).toBe(false);
    expect(parse(pipe(string(), toLowerCase()), "ADA")).toBe("ada");
    expect(parse(pipe(string(), toUpperCase()), "ada")).toBe("ADA");
    expect(parse(pipe(string(), nonEmpty()), "a")).toBe("a");
    expect(safeParse(pipe(string(), nonEmpty()), "").ok).toBe(false);
  });

  it("checks a multiple, including one that floating point gets wrong", () => {
    const step = pipe(number(), multipleOf(0.1));
    // `0.3 % 0.1` is 0.09999999999999998, which a naive remainder rejects.
    expect(parse(step, 0.3)).toBe(0.3);
    expect(parse(step, 1.2)).toBe(1.2);
    expect(safeParse(step, 0.35).ok).toBe(false);
  });

  it("carries the eighth step of a pipeline", () => {
    const handle = pipe(
      string(),
      trim(),
      toLowerCase(),
      minLength(3),
      maxLength(16),
      startsWith("@"),
      includes("a"),
      regex(/^@[a-z]+$/, "expected a handle"),
      transform((text: string) => text.slice(1)),
    );
    expect(parse(handle, "  @ADA  ")).toBe("ada");
    const failed = safeParse(handle, " @ad9 ");
    expect(failed.ok).toBe(false);
    if (!failed.ok) {
      expect(failed.issues[0].message).toBe("expected a handle");
    }
  });
});

describe("describe", () => {
  it("says what a leaf is", () => {
    expect(describeSchema(string())).toEqual({ kind: "string" });
    expect(describeSchema(literal("on"))).toEqual({ kind: "literal", value: "on" });
    expect(describeSchema(enum_<string>(["a", "b"]))).toEqual({ kind: "enum", values: ["a", "b"] });
  });

  it("says what a composite is made of", () => {
    expect(describeSchema(array(number()))).toEqual({
      kind: "array",
      item: { kind: "number" },
    });
    expect(describeSchema(strictObject({ name: string() }))).toEqual({
      kind: "object",
      entries: [["name", { kind: "string" }]],
      unknownKeys: "reject",
    });
  });
});

describe("json schema", () => {
  const DIALECT = "https://json-schema.org/draft/2020-12/schema";

  it("exports an object with its properties and its required keys", () => {
    const Account = strictObject({
      name: pipe(string(), minLength(2), maxLength(40)),
      age: pipe(number(), integer(), min(18)),
      nickname: optional(string()),
      website: nullable(pipe(string(), url())),
    });
    expect(toJsonSchema(Account)).toEqual({
      schema: {
        $schema: DIALECT,
        type: "object",
        properties: {
          name: { type: "string", minLength: 2, maxLength: 40 },
          age: { type: "integer", minimum: 18 },
          nickname: { type: "string" },
          website: { anyOf: [{ type: "string", format: "uri" }, { type: "null" }] },
        },
        required: ["name", "age", "website"],
        additionalProperties: false,
      },
      unrepresentable: [],
    });
  });

  it("describes the input side of a transform, and says what that lost", () => {
    const age = pipe(string(), transform(Number), integer(), min(18));
    const exported = toJsonSchema(age);
    // Not `{ type: "string", minimum: 18 }`, which would ask a consumer to
    // compare a string against a number.
    expect(exported.schema).toEqual({ $schema: DIALECT, type: "string" });
    expect(exported.unrepresentable).toEqual([
      { path: [], kind: "integer after a transform" },
      { path: [], kind: "min after a transform" },
    ]);
  });

  it("keeps a constraint that comes before the transform", () => {
    const age = pipe(string(), minLength(2), transform(Number), min(18));
    expect(toJsonSchema(age).schema).toEqual({
      $schema: DIALECT,
      type: "string",
      minLength: 2,
    });
  });

  it("emits a definition and a reference for a recursive schema", () => {
    const node: Schema<CommentValue> = lazy(() => object({ text: string(), replies: array(node) }));
    const exported = toJsonSchema(node);
    expect(exported.schema.$ref).toBe("#/$defs/definition0");
    expect(exported.schema.$defs).toEqual({
      definition0: {
        type: "object",
        properties: {
          text: { type: "string" },
          replies: { type: "array", items: { $ref: "#/$defs/definition0" } },
        },
        required: ["text", "replies"],
      },
    });
    expect(exported.unrepresentable).toEqual([]);
  });

  it("says what it could not represent, and where", () => {
    const schema = object({
      when: date(),
      tags: set(string()),
      id: pipe(
        string(),
        check((text: string) => text.length > 0, "must not be empty"),
      ),
    });
    const exported = toJsonSchema(schema);
    expect(exported.unrepresentable).toEqual([
      { path: ["when"], kind: "date" },
      { path: ["tags"], kind: "set" },
      { path: ["id"], kind: "check must not be empty" },
    ]);
    // Imprecise, never wrong: the document accepts everything the schema does.
    expect(exported.schema.properties).toEqual({
      when: {},
      tags: {},
      id: { type: "string" },
    });
  });

  it("exports the four ways of combining schemas", () => {
    expect(toJsonSchema(union([string(), number()])).schema).toEqual({
      $schema: DIALECT,
      anyOf: [{ type: "string" }, { type: "number" }],
    });
    expect(toJsonSchema(record(number())).schema).toEqual({
      $schema: DIALECT,
      type: "object",
      additionalProperties: { type: "number" },
    });
    expect(toJsonSchema(tuple([string(), number()])).schema).toEqual({
      $schema: DIALECT,
      type: "array",
      prefixItems: [{ type: "string" }, { type: "number" }],
      minItems: 2,
      maxItems: 2,
    });
    expect(
      toJsonSchema(intersect(object({ a: string() }), object({ b: number() }))).schema,
    ).toEqual({
      $schema: DIALECT,
      allOf: [
        { type: "object", properties: { a: { type: "string" } }, required: ["a"] },
        { type: "object", properties: { b: { type: "number" } }, required: ["b"] },
      ],
    });
  });
});

describe("as a form's resolver", () => {
  const account = object({
    email: pipe(string(), email()),
    age: pipe(string(), transform(Number), min(18)),
  });

  it("answers synchronously for a synchronous schema", () => {
    const resolve = validatorResolver(account);
    const answer = resolve({ email: "ada@example.com", age: "36" }, undefined);
    // Not a promise: a form in `onChange` mode runs this on every keystroke.
    expect(answer instanceof Promise).toBe(false);
    expect(answer).toEqual({ values: { email: "ada@example.com", age: 36 } });

    const failed = resolve({ email: "nope", age: "12" }, undefined);
    expect(failed).toEqual({
      errors: {
        email: { type: "email", message: "expected email address" },
        age: { type: "min", message: "expected at least 18" },
      },
    });
  });

  it("waits when the schema has to ask, and keys the errors by field", async () => {
    const taken = new Set(["ada"]);
    const schema = object({
      handle: pipe(
        string(),
        checkAsync(async (name: string) => !taken.has(name), "That handle is taken"),
      ),
    });
    const resolve = validatorResolver(schema);

    const accepted = await resolve({ handle: "grace" }, undefined);
    expect(accepted).toEqual({ values: { handle: "grace" } });

    const rejected = await resolve({ handle: "ada" }, undefined);
    expect(rejected).toEqual({
      errors: { handle: { type: "check", message: "That handle is taken" } },
    });
  });
});
