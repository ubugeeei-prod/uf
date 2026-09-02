// @flow
//
// `@uniflowed/validator`.
//
// Validation is ordinary Flow-typed JavaScript. Schemas carry a tiny parse
// function, so they work the same on Node.js, Deno, and Bun without a native
// binding.

type Path = $ReadOnlyArray<string>;

type SchemaKernel<T> = {|
  readonly parse: (mixed, Path) => Result<T>,
|};

type SchemaCarrier<T> = {|
  readonly __kind: "Schema",
  readonly __type: (T) => T,
  readonly __kernel: SchemaKernel<T>,
|};

export opaque type Schema<T> = SchemaCarrier<T>;

export type Issue = {|
  readonly code: string,
  readonly message: string,
  readonly path?: Path,
|};

export type Result<T> =
  | {| readonly ok: true, readonly value: T |}
  | {| readonly ok: false, readonly issues: $ReadOnlyArray<Issue> |};

export type Step<TIn, TOut> = (schema: Schema<TIn>) => Schema<TOut>;

export type Shape = { readonly [string]: Schema<mixed>, ... };
export type Infer<TSchema> = TSchema extends Schema<infer T> ? T : empty;

function makeSchema<T>(kernel: SchemaKernel<T>): Schema<T> {
  return { __kind: "Schema", __type: (value) => value, __kernel: kernel };
}

function readKernel<T>(schema: Schema<T>): SchemaKernel<T> {
  return schema.__kernel;
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

function parseInternal<T>(schema: Schema<T>, value: mixed, path: Path): Result<T> {
  return readKernel(schema).parse(value, path);
}

function refine<T>(
  schema: Schema<T>,
  check: (T) => boolean,
  code: string,
  message: string,
): Schema<T> {
  return makeSchema({
    parse: (value, path) => {
      const result = parseInternal(schema, value, path);
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

function plainRecord(value: mixed): { readonly [string]: mixed, ... } {
  // This is the single object boundary in the validator kernel. Every caller checks
  // `isPlainObject` first, then schemas validate each exported field before exposing T.
  // $FlowFixMe[incompatible-type]
  return value as { readonly [string]: mixed, ... };
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

export function literal<T extends string | number | boolean | null>(expected: T): Schema<T> {
  return makeSchema({
    parse: (value, path) =>
      value === expected ? ok(expected) : failIssue("literal", `expected ${String(expected)}`, path),
  });
}

export function enum_<T extends string>(values: $ReadOnlyArray<T>): Schema<T> {
  return makeSchema({
    parse: (value, path) => {
      for (const option of values) {
        if (value === option) {
          return ok(option);
        }
      }
      return failIssue("enum", `expected one of ${values.join(", ")}`, path);
    },
  });
}

export function array<Item>(item: Schema<Item>): Schema<$ReadOnlyArray<Item>> {
  return makeSchema({
    parse: (value, path) => {
      if (!Array.isArray(value)) {
        return failIssue("type", "expected array", path);
      }
      const out: Array<Item> = [];
      const issues: Array<Issue> = [];
      for (let index = 0; index < value.length; index += 1) {
        const result = parseInternal(item, value[index], path.concat(String(index)));
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

export function tuple<TItems extends $ReadOnlyArray<Schema<mixed>>>(
  items: TItems,
): Schema<$ReadOnlyArray<mixed>> {
  return makeSchema({
    parse: (value, path) => {
      if (!Array.isArray(value)) {
        return failIssue("type", "expected tuple", path);
      }
      if (value.length !== items.length) {
        return failIssue("length", `expected ${String(items.length)} tuple items`, path);
      }
      const out: Array<mixed> = [];
      const issues: Array<Issue> = [];
      for (let index = 0; index < items.length; index += 1) {
        const result = parseInternal(items[index], value[index], path.concat(String(index)));
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

export function union<T>(schemas: $ReadOnlyArray<Schema<T>>): Schema<T> {
  return makeSchema({
    parse: (value, path) => {
      const issues: Array<Issue> = [];
      for (const schema of schemas) {
        const result = parseInternal(schema, value, path);
        if (result.ok) {
          return result;
        }
        mergeIssues(issues, result);
      }
      return { ok: false, issues };
    },
  });
}

function schemaObject<T extends { ... }>(shape: Shape): Schema<T> {
  return makeSchema({
    parse: (value, path) => {
      if (!isPlainObject(value)) {
        return failIssue("type", "expected object", path);
      }
      const record = plainRecord(value);
      const out: { [string]: mixed, ... } = {};
      const issues: Array<Issue> = [];
      for (const key in shape) {
        const result = parseInternal(shape[key], record[key], path.concat(key));
        if (result.ok) {
          out[key] = result.value;
        } else {
          mergeIssues(issues, result);
        }
      }
      // $FlowFixMe[incompatible-type] shape parsers have constructed every output field.
      return issues.length === 0 ? ok(out as T) : { ok: false, issues };
    },
  });
}

export function strictObject<T extends { ... }>(shape: Shape): Schema<T> {
  return makeSchema({
    parse: (value, path) => {
      const result = parseInternal(schemaObject<T>(shape), value, path);
      if (!result.ok || !isPlainObject(value)) {
        return result;
      }
      const issues: Array<Issue> = [];
      for (const key in plainRecord(value)) {
        if (!Object.hasOwn(shape, key)) {
          issues.push(issue("unknown_key", `unexpected key ${key}`, path.concat(key)));
        }
      }
      return issues.length === 0 ? result : { ok: false, issues };
    },
  });
}

export function partial<T extends { ... }>(shape: Shape): Schema<Partial<T>> {
  const partialShape: { [string]: Schema<mixed>, ... } = {};
  for (const key in shape) {
    partialShape[key] = optional(shape[key]);
  }
  return schemaObject<Partial<T>>(partialShape);
}

export { schemaObject as object };

export function optional<T>(schema: Schema<T>): Schema<void | T> {
  return makeSchema({
    parse: (value, path) =>
      value === undefined ? ok(undefined) : parseInternal(schema, value, path),
  });
}

export function nullable<T>(schema: Schema<T>): Schema<null | T> {
  return makeSchema({
    parse: (value, path) => (value === null ? ok(null) : parseInternal(schema, value, path)),
  });
}

export function pipe<A, B>(schema: Schema<A>, step: Step<A, B>): Schema<B> {
  return step(schema);
}

export function transform<A, B>(change: (value: A) => B): Step<A, B> {
  return (schema) =>
    makeSchema({
      parse: (value, path) => {
        const result = parseInternal(schema, value, path);
        if (!result.ok) {
          return result;
        }
        return ok(change(result.value));
      },
    });
}

export function brand<T, Name extends string>(name: Name): Step<T, T> {
  return (schema) => refine(schema, () => true, "brand", `expected brand ${name}`);
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

export function email(): Step<string, string> {
  return (schema) =>
    refine(schema, (input) => /.+@.+\..+/.test(input), "email", "expected email address");
}

export function min(value: number): Step<number, number> {
  return (schema) =>
    refine(schema, (input) => input >= value, "min", `expected at least ${String(value)}`);
}

export function max(value: number): Step<number, number> {
  return (schema) =>
    refine(schema, (input) => input <= value, "max", `expected at most ${String(value)}`);
}

export function date(): Schema<Date> {
  return makeSchema({
    parse: (value, path) =>
      value instanceof Date && Number.isFinite(value.getTime())
        ? ok(value)
        : failIssue("type", "expected Date", path),
  });
}

export function instance<T>(ClassValue: Class<T>): Schema<T> {
  return makeSchema({
    parse: (value, path) =>
      value instanceof ClassValue ? ok(value as T) : failIssue("type", "expected instance", path),
  });
}

export function parse<T>(schema: Schema<T>, value: mixed): T {
  const result = safeParse(schema, value);
  if (result.ok) {
    return result.value;
  }
  throw Error(result.issues.map((entry) => entry.message).join("; "));
}

export function safeParse<T>(schema: Schema<T>, value: mixed): Result<T> {
  return parseInternal(schema, value, []);
}

export hook useValidation<T>(schema: Schema<T>, value: mixed): Result<T> {
  return safeParse(schema, value);
}

export const v: {
  readonly string: typeof string,
  readonly number: typeof number,
  readonly boolean: typeof boolean,
  readonly unknown: typeof unknown,
  readonly literal: typeof literal,
  readonly enum: typeof enum_,
  readonly array: typeof array,
  readonly tuple: typeof tuple,
  readonly union: typeof union,
  readonly object: typeof schemaObject,
  readonly strictObject: typeof strictObject,
  readonly partial: typeof partial,
  readonly optional: typeof optional,
  readonly nullable: typeof nullable,
  readonly pipe: typeof pipe,
  readonly transform: typeof transform,
  readonly brand: typeof brand,
  readonly minLength: typeof minLength,
  readonly maxLength: typeof maxLength,
  readonly startsWith: typeof startsWith,
  readonly email: typeof email,
  readonly min: typeof min,
  readonly max: typeof max,
  readonly date: typeof date,
  readonly instance: typeof instance,
  readonly parse: typeof parse,
  readonly safeParse: typeof safeParse,
  readonly useValidation: typeof useValidation,
} = {
  string,
  number,
  boolean,
  unknown,
  literal,
  enum: enum_,
  array,
  tuple,
  union,
  object: schemaObject,
  strictObject,
  partial,
  optional,
  nullable,
  pipe,
  transform,
  brand,
  minLength,
  maxLength,
  startsWith,
  email,
  min,
  max,
  date,
  instance,
  parse,
  safeParse,
  useValidation,
};
