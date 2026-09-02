// @flow
//
// `@uniflowed/validator`.
//
// Validation is ordinary Flow-typed JavaScript. Schemas carry a tiny parse
// function, so they work the same on Node.js, Deno, and Bun without a native
// binding.

type Path = $ReadOnlyArray<string>;

type SchemaKernel = {
  +parse: (mixed, Path) => Result<mixed>,
};

type SchemaCarrier<T> = {
  +__kind: "Schema",
  +__type: (T) => T,
  +__kernel: SchemaKernel,
};

export opaque type Schema<T> = SchemaCarrier<T>;

export type Issue = {
  +code: string,
  +message: string,
  +path?: Path,
};

export type Result<T> =
  | { +ok: true, +value: T }
  | { +ok: false, +issues: $ReadOnlyArray<Issue> };

export type Step<TIn, TOut> = (schema: Schema<TIn>) => Schema<TOut>;

export type Shape = { +[string]: Schema<mixed> };

function makeSchema<T>(kernel: SchemaKernel): Schema<T> {
  return ({ __kind: "Schema", __type: (value) => value, __kernel: kernel }: any);
}

function readKernel<T>(schema: Schema<T>): SchemaKernel {
  return (schema: any).__kernel;
}

function issue(code: string, message: string, path: Path): Issue {
  return path.length === 0 ? { code, message } : { code, message, path };
}

function ok<T>(value: T): Result<T> {
  return { ok: true, value };
}

function failIssue(code: string, message: string, path: Path): Result<empty> {
  return { ok: false, issues: [issue(code, message, path)] };
}

function mergeIssues(issues: Array<Issue>, result: Result<mixed>): void {
  match (result) {
    {ok: false, issues: const nextIssues} => {
      for (const entry of nextIssues) {
        issues.push(entry);
      }
    }
    _ => {}
  }
}

function parse<T>(schema: Schema<T>, value: mixed, path: Path): Result<T> {
  return (readKernel(schema).parse(value, path): any);
}

function refine<T>(
  schema: Schema<T>,
  check: (T) => boolean,
  code: string,
  message: string,
): Schema<T> {
  return makeSchema({
    parse: (value, path) => {
      const result = parse(schema, value, path);
      if (!result.ok) {
        return result;
      }
      return check(result.value) ? result : failIssue(code, message, path);
    },
  });
}

function isPlainObject(value: mixed): boolean {
  return value != null && typeof value === "object" && !Array.isArray(value);
}

export function string(): Schema<string> {
  return makeSchema({
    parse: (value, path) =>
      typeof value === "string" ? ok(value) : failIssue("type", "expected string", path),
  });
}

export function number(): Schema<number> {
  return makeSchema({
    parse: (value, path) =>
      typeof value === "number" && Number.isFinite(value)
        ? ok(value)
        : failIssue("type", "expected number", path),
  });
}

export function boolean(): Schema<boolean> {
  return makeSchema({
    parse: (value, path) =>
      typeof value === "boolean" ? ok(value) : failIssue("type", "expected boolean", path),
  });
}

export function unknown(): Schema<mixed> {
  return makeSchema({ parse: (value) => ok(value) });
}

export function literal<T: string | number | boolean | null>(expected: T): Schema<T> {
  return makeSchema({
    parse: (value, path) =>
      value === expected ? ok(expected) : failIssue("literal", `expected ${String(expected)}`, path),
  });
}

export function array<Item>(item: Schema<Item>): Schema<$ReadOnlyArray<Item>> {
  return makeSchema({
    parse: (value, path) => {
      if (!Array.isArray(value)) {
        return failIssue("type", "expected array", path);
      }
      const out = [];
      const issues = [];
      for (let index = 0; index < value.length; index += 1) {
        const result = parse(item, value[index], path.concat(String(index)));
        if (result.ok) {
          out.push(result.value);
        } else {
          mergeIssues(issues, result);
        }
      }
      return issues.length === 0 ? ok(out) : { ok: false, issues };
    },
  });
}

export function object<T: { ... }>(shape: Shape): Schema<T> {
  return makeSchema({
    parse: (value, path) => {
      if (!isPlainObject(value)) {
        return failIssue("type", "expected object", path);
      }
      const input = (value: any);
      const out = {};
      const issues = [];
      for (const key in shape) {
        const result = parse(shape[key], input[key], path.concat(key));
        if (result.ok) {
          out[key] = result.value;
        } else {
          mergeIssues(issues, result);
        }
      }
      return issues.length === 0 ? ok((out: any)) : { ok: false, issues };
    },
  });
}

export function optional<T>(schema: Schema<T>): Schema<void | T> {
  return makeSchema({
    parse: (value, path) => (value === undefined ? ok(undefined) : parse(schema, value, path)),
  });
}

export function pipe<A, B>(schema: Schema<A>, step: Step<A, B>): Schema<B> {
  return step(schema);
}

export function minLength(value: number): Step<string, string> {
  return (schema) =>
    refine(
      schema,
      (input) => input.length >= value,
      "min_length",
      `expected at least ${String(value)} characters`,
    );
}

export function maxLength(value: number): Step<string, string> {
  return (schema) =>
    refine(
      schema,
      (input) => input.length <= value,
      "max_length",
      `expected at most ${String(value)} characters`,
    );
}

export function startsWith(value: string): Step<string, string> {
  return (schema) =>
    refine(schema, (input) => input.startsWith(value), "starts_with", `expected prefix ${value}`);
}

export function min(value: number): Step<number, number> {
  return (schema) =>
    refine(schema, (input) => input >= value, "min", `expected at least ${String(value)}`);
}

export function max(value: number): Step<number, number> {
  return (schema) =>
    refine(schema, (input) => input <= value, "max", `expected at most ${String(value)}`);
}

export function safeParse<T>(schema: Schema<T>, value: mixed): Result<T> {
  return parse(schema, value, []);
}

export hook useValidation<T>(schema: Schema<T>, value: mixed): Result<T> {
  return safeParse(schema, value);
}

export const v: {
  +string: typeof string,
  +number: typeof number,
  +boolean: typeof boolean,
  +unknown: typeof unknown,
  +literal: typeof literal,
  +array: typeof array,
  +object: typeof object,
  +optional: typeof optional,
  +pipe: typeof pipe,
  +minLength: typeof minLength,
  +maxLength: typeof maxLength,
  +startsWith: typeof startsWith,
  +min: typeof min,
  +max: typeof max,
  +safeParse: typeof safeParse,
  +useValidation: typeof useValidation,
} = {
  string,
  number,
  boolean,
  unknown,
  literal,
  array,
  object,
  optional,
  pipe,
  minLength,
  maxLength,
  startsWith,
  min,
  max,
  safeParse,
  useValidation,
};
