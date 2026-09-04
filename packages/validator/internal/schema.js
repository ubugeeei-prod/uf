// @flow
//
// `@uniflowed/validator`: schemas as ordinary Flow-typed JavaScript, so they
// work identically on Node.js, Deno and Bun with no native binding.
//
// # Why every schema is one closure
//
// A schema is a `parse` function and nothing else. No class, no registry, no
// interpreter walking a description of a schema at runtime. The engine that
// validates a `string()` *is* the four-line closure `string()` returned, which
// a JIT can inline into the object parser that calls it. It is also why the
// module tree-shakes: a project that never calls `date()` never ships the Date
// check, because there is no table holding a reference to it.
//
// # Why the path is a mutable buffer
//
// Issues report where they happened — `["users", "2", "email"]` — and the
// obvious way to carry that is a fresh array per field, `path.concat(key)`.
// That allocates once per field per parse, on the *successful* path, to
// produce a value almost every parse throws away. Instead one array is pushed
// and popped as the walk descends, and only an actual issue copies it. A
// thousand-row payload that validates cleanly allocates no paths at all.
//
// # Why parse throws a typed error
//
// `parse` raises [`ValidationError`], which carries the structured issues, not
// just a joined message. A caller writing an HTTP handler needs the field
// paths to build a response body, and re-parsing them out of a string is not
// a thing an API should make anyone do.

/** Where an issue happened, as object keys and array indices from the root. */
type Path = $ReadOnlyArray<string>;

/** The mutable buffer the walk descends with. See the module docs. */
type PathBuffer = Array<string>;

type SchemaKernel<T> = {|
  readonly parse: (mixed, PathBuffer) => Result<T>,
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

/**
 * What [`parse`] raises.
 *
 * A real `Error` subclass so it survives `instanceof`, logging and a `catch`
 * that only knows about errors, and it carries `issues` so a caller can build
 * a field-by-field response without parsing the message back apart.
 */
export class ValidationError extends Error {
  readonly issues: $ReadOnlyArray<Issue>;

  constructor(issues: $ReadOnlyArray<Issue>) {
    super(issues.map(describeIssue).join("; "));
    this.name = "ValidationError";
    this.issues = issues;
  }
}

function describeIssue(entry: Issue): string {
  const at = entry.path == null || entry.path.length === 0 ? "" : ` at ${entry.path.join(".")}`;
  return `${entry.message}${at}`;
}

function makeSchema<T>(kernel: SchemaKernel<T>): Schema<T> {
  return { __kind: "Schema", __type: (value) => value, __kernel: kernel };
}

function readKernel<T>(schema: Schema<T>): SchemaKernel<T> {
  return schema.__kernel;
}

function issue(code: string, message: string, path: PathBuffer): Issue {
  return path.length === 0 ? { code, message } : { code, message, path: path.slice() };
}

function ok<T>(value: T): Result<T> {
  return { ok: true, value };
}

function failIssue(code: string, message: string, path: PathBuffer): Result<empty> {
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

function parseInternal<T>(schema: Schema<T>, value: mixed, path: PathBuffer): Result<T> {
  return readKernel(schema).parse(value, path);
}

/**
 * Parse `value` at one step deeper in the path.
 *
 * The push/pop pair is why the buffer stays balanced even when a nested schema
 * returns early: nothing between them can throw except user code inside a
 * `transform` or a `check`, and a schema that raised has already failed the
 * whole parse.
 */
function parseAt<T>(schema: Schema<T>, value: mixed, path: PathBuffer, key: string): Result<T> {
  path.push(key);
  const result = parseInternal(schema, value, path);
  path.pop();
  return result;
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

/**
 * Write a parsed field into the object being built.
 *
 * `out[key] = value` runs a setter when `key` is `__proto__`, so an input
 * carrying that key would change the object's prototype instead of adding a
 * field — and everything downstream would then read attacker-chosen values
 * from a prototype it never inspected. `defineProperty` writes an own
 * property whatever the key is called.
 */
function put<Value>(out: { [string]: Value, ... }, key: string, value: Value): void {
  Object.defineProperty(out, key, {
    value,
    writable: true,
    enumerable: true,
    configurable: true,
  });
}

export function string(): Schema<string> {
  return makeSchema({
    parse: (value, path) =>
      typeof value === "string" ? ok(value) : failIssue("type", "expected string", path),
  });
}

/**
 * A finite number.
 *
 * `NaN` and the infinities are rejected. They are numbers to `typeof` and
 * disasters to arithmetic, and a validator that lets `NaN` through has not
 * validated anything — every comparison downstream silently answers `false`.
 */
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
      value === expected
        ? ok(expected)
        : failIssue("literal", `expected ${String(expected)}`, path),
  });
}

export function enum_<T extends string>(values: $ReadOnlyArray<T>): Schema<T> {
  const message = `expected one of ${values.join(", ")}`;
  return makeSchema({
    parse: (value, path) => {
      for (const option of values) {
        if (value === option) {
          return ok(option);
        }
      }
      return failIssue("enum", message, path);
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
      let issues: null | Array<Issue> = null;
      for (let index = 0; index < value.length; index += 1) {
        const result = parseAt(item, value[index], path, String(index));
        if (result.ok) {
          out.push(result.value);
        } else {
          issues = issues ?? [];
          mergeIssues(issues, result);
        }
      }
      return issues == null ? ok(out) : { ok: false, issues };
    },
  });
}

export function tuple<TItems extends $ReadOnlyArray<Schema<mixed>>>(
  items: TItems,
): Schema<$ReadOnlyArray<mixed>> {
  const arity = items.length;
  return makeSchema({
    parse: (value, path) => {
      if (!Array.isArray(value)) {
        return failIssue("type", "expected tuple", path);
      }
      if (value.length !== arity) {
        return failIssue("length", `expected ${String(arity)} tuple items`, path);
      }
      const out: Array<mixed> = [];
      let issues: null | Array<Issue> = null;
      for (let index = 0; index < arity; index += 1) {
        const result = parseAt(items[index], value[index], path, String(index));
        if (result.ok) {
          out.push(result.value);
        } else {
          issues = issues ?? [];
          mergeIssues(issues, result);
        }
      }
      return issues == null ? ok(out) : { ok: false, issues };
    },
  });
}

/**
 * An object whose keys are not known ahead of time.
 *
 * Only own enumerable keys are read, so a payload carrying `__proto__` or
 * `constructor` cannot smuggle an inherited value into the parsed result.
 */
export function record<Value>(value: Schema<Value>): Schema<{ readonly [string]: Value, ... }> {
  return makeSchema({
    parse: (input, path) => {
      if (!isPlainObject(input)) {
        return failIssue("type", "expected object", path);
      }
      const source = plainRecord(input);
      const out: { [string]: Value, ... } = {};
      let issues: null | Array<Issue> = null;
      for (const key of Object.keys(source)) {
        const result = parseAt(value, source[key], path, key);
        if (result.ok) {
          put(out, key, result.value);
        } else {
          issues = issues ?? [];
          mergeIssues(issues, result);
        }
      }
      // $FlowFixMe[incompatible-type] every retained key went through `value`.
      return issues == null ? ok(out) : { ok: false, issues };
    },
  });
}

/**
 * The first schema that accepts the value.
 *
 * When none do, every branch's issues are reported, because there is no way to
 * know which branch the author meant. That is also why [`variant`] exists: a
 * discriminated union can know, and its errors say one useful thing instead of
 * every possible thing.
 */
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

/**
 * A union chosen by the value of one key.
 *
 * The discriminant is read first and the matching branch is the only one run,
 * so an invalid `{ kind: "circle", radius: "big" }` reports `expected number
 * at radius` rather than every reason it is not a square, a triangle and a
 * line as well. Unmatched discriminants name the ones that exist.
 */
export function variant<T>(
  key: string,
  branches: { readonly [string]: Schema<T>, ... },
): Schema<T> {
  const known = Object.keys(branches);
  return makeSchema({
    parse: (value, path) => {
      if (!isPlainObject(value)) {
        return failIssue("type", "expected object", path);
      }
      const discriminant = plainRecord(value)[key];
      if (typeof discriminant !== "string" || !Object.hasOwn(branches, discriminant)) {
        path.push(key);
        const failed = failIssue("variant", `expected one of ${known.join(", ")}`, path);
        path.pop();
        return failed;
      }
      return parseInternal(branches[discriminant], value, path);
    },
  });
}

/**
 * Pairs of `[key, schema]`, resolved once when the object schema is built.
 *
 * `for (const key in shape)` on every parse walks the prototype chain and
 * re-reads the same descriptors for the life of the process. The shape cannot
 * change after construction, so the walk belongs at construction.
 */
function shapeEntries(shape: Shape): $ReadOnlyArray<[string, Schema<mixed>]> {
  return Object.keys(shape).map((key) => [key, shape[key]]);
}

function schemaObject<T extends { ... }>(shape: Shape): Schema<T> {
  const entries = shapeEntries(shape);
  return makeSchema({
    parse: (value, path) => {
      if (!isPlainObject(value)) {
        return failIssue("type", "expected object", path);
      }
      const record = plainRecord(value);
      const out: { [string]: mixed, ... } = {};
      let issues: null | Array<Issue> = null;
      for (const [key, schema] of entries) {
        const result = parseAt(schema, record[key], path, key);
        if (result.ok) {
          put(out, key, result.value);
        } else {
          issues = issues ?? [];
          mergeIssues(issues, result);
        }
      }
      // $FlowFixMe[incompatible-type] shape parsers have constructed every output field.
      return issues == null ? ok(out as T) : { ok: false, issues };
    },
  });
}

/**
 * An object that rejects keys the shape does not name.
 *
 * `object()` drops unknown keys silently, which is right for a tolerant reader
 * of someone else's payload. `strictObject()` is for the other case — a
 * configuration file, an internal API — where an unrecognised key is almost
 * always a typo the user would rather hear about than have ignored.
 */
export function strictObject<T extends { ... }>(shape: Shape): Schema<T> {
  const inner = schemaObject<T>(shape);
  const allowed = new Set(Object.keys(shape));
  return makeSchema({
    parse: (value, path) => {
      const result = parseInternal(inner, value, path);
      if (!isPlainObject(value)) {
        return result;
      }

      // The unknown-key scan runs whether or not the fields parsed. Returning
      // early on a field failure meant `{ name: 1, extra: true }` reported the
      // wrong type of `name` and said nothing about `extra`, so fixing the
      // first error revealed the second — which is the whole reason this
      // validator collects issues instead of stopping at one.
      const issues: Array<Issue> = [];
      if (!result.ok) {
        mergeIssues(issues, result);
      }
      for (const key of Object.keys(plainRecord(value))) {
        if (!allowed.has(key)) {
          path.push(key);
          issues.push(issue("unknown_key", `unexpected key ${key}`, path));
          path.pop();
        }
      }
      return issues.length === 0 ? result : { ok: false, issues };
    },
  });
}

export function partial<T extends { ... }>(shape: Shape): Schema<Partial<T>> {
  const partialShape: { [string]: Schema<mixed>, ... } = {};
  for (const key of Object.keys(shape)) {
    partialShape[key] = optional(shape[key]);
  }
  return schemaObject<Partial<T>>(partialShape);
}

export { schemaObject as object };

/**
 * A schema built on first use.
 *
 * The one way to write a recursive type: a comment tree cannot name its own
 * schema in its own initialiser, but it can name a function that returns it.
 * The result is memoised, so recursion costs one closure, not one per node.
 */
export function lazy<T>(build: () => Schema<T>): Schema<T> {
  let built: null | Schema<T> = null;
  return makeSchema({
    parse: (value, path) => {
      if (built == null) {
        built = build();
      }
      return parseInternal(built, value, path);
    },
  });
}

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

/**
 * A schema that never fails, substituting `value` when the inner one does.
 *
 * For the boundary where a bad field should not sink the whole payload — a
 * cached response, a user preference — and where the alternative is a
 * `safeParse` and a hand-written `if` at every call site.
 */
export function fallback<T>(schema: Schema<T>, value: T): Schema<T> {
  return makeSchema({
    parse: (input, path) => {
      const result = parseInternal(schema, input, path);
      return result.ok ? result : ok(value);
    },
  });
}

/**
 * Apply steps to a schema, left to right.
 *
 * Variadic, because refinement is cumulative in practice — an email is a
 * string that is long enough *and* looks like an address — and forcing
 * `pipe(pipe(pipe(...)))` for that is a tax on the common case.
 */
export function pipe<A>(schema: Schema<A>, ...steps: $ReadOnlyArray<Step<any, any>>): Schema<any> {
  let piped: Schema<any> = schema;
  for (const step of steps) {
    piped = step(piped);
  }
  return piped;
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

/**
 * An arbitrary predicate, with the message it should report.
 *
 * Every other step in this module is a special case of this one. It exists so
 * that a rule the library did not anticipate — a checksum, a business rule,
 * one field agreeing with another — is a one-liner rather than a reason to
 * abandon the schema and hand-roll validation.
 */
export function check<T>(predicate: (value: T) => boolean, message: string): Step<T, T> {
  return (schema) => refine(schema, predicate, "check", message);
}

/**
 * A nominal marker on an otherwise ordinary value.
 *
 * It does not check anything at runtime, and it is not pretending to: the
 * point is the name in the schema, so that `UserId` and `PostId` read
 * differently at the call site even though both are strings underneath.
 */
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

export function endsWith(value: string): Step<string, string> {
  return (schema) =>
    refine(schema, (input) => input.endsWith(value), "ends_with", `expected suffix ${value}`);
}

/**
 * A string matching `pattern`.
 *
 * The pattern is tested with `RegExp.prototype.test` against a fresh `lastIndex`
 * every time, because a caller who reaches for `/g` would otherwise get a
 * schema that alternates between accepting and rejecting the same input.
 */
export function regex(pattern: RegExp, message?: string): Step<string, string> {
  return (schema) =>
    refine(
      schema,
      (input) => {
        pattern.lastIndex = 0;
        return pattern.test(input);
      },
      "regex",
      message ?? `expected a match for ${String(pattern)}`,
    );
}

export function trim(): Step<string, string> {
  return transform((input: string) => input.trim());
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

export function integer(): Step<number, number> {
  return (schema) => refine(schema, Number.isInteger, "integer", "expected an integer");
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

/** Parse, or raise a [`ValidationError`] carrying every issue found. */
export function parse<T>(schema: Schema<T>, value: mixed): T {
  const result = safeParse(schema, value);
  if (result.ok) {
    return result.value;
  }
  throw new ValidationError(result.issues);
}

/** Parse into a result, so failure is a value rather than control flow. */
/**
 * A schema as a standalone function.
 *
 * `safeParse(schema, value)` needs both halves at the call site, which is fine
 * where the schema is in scope and useless where it is not — a boundary that
 * wants to validate what arrives takes a *function*, not a schema and an
 * import of this module. `parser(User)` is that function, and it is why
 * `@uniflowed/fetch` can check a response body without depending on the
 * validator at all.
 */
export function parser<T>(schema: Schema<T>): (value: mixed) => Result<T> {
  return (value: mixed) => safeParse(schema, value);
}

export function safeParse<T>(schema: Schema<T>, value: mixed): Result<T> {
  return parseInternal(schema, value, []);
}

/**
 * Validate a value during render.
 *
 * A hook rather than a plain call so the React Compiler memoises it with the
 * rest of the component: re-rendering for an unrelated reason does not re-walk
 * the payload.
 */
export hook useValidation<T>(schema: Schema<T>, value: mixed): Result<T> {
  return safeParse(schema, value);
}

/**
 * Every builder under one name, for callers who prefer `v.string()`.
 *
 * The named exports are the primary surface — they are what tree-shakes — and
 * this is the convenience alias, typed by `typeof` so the two can never drift.
 */
export const v: {
  readonly string: typeof string,
  readonly number: typeof number,
  readonly boolean: typeof boolean,
  readonly unknown: typeof unknown,
  readonly literal: typeof literal,
  readonly enum: typeof enum_,
  readonly array: typeof array,
  readonly tuple: typeof tuple,
  readonly record: typeof record,
  readonly union: typeof union,
  readonly variant: typeof variant,
  readonly object: typeof schemaObject,
  readonly strictObject: typeof strictObject,
  readonly partial: typeof partial,
  readonly lazy: typeof lazy,
  readonly optional: typeof optional,
  readonly nullable: typeof nullable,
  readonly fallback: typeof fallback,
  readonly pipe: typeof pipe,
  readonly transform: typeof transform,
  readonly check: typeof check,
  readonly brand: typeof brand,
  readonly minLength: typeof minLength,
  readonly maxLength: typeof maxLength,
  readonly startsWith: typeof startsWith,
  readonly endsWith: typeof endsWith,
  readonly regex: typeof regex,
  readonly trim: typeof trim,
  readonly email: typeof email,
  readonly min: typeof min,
  readonly max: typeof max,
  readonly integer: typeof integer,
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
  record,
  union,
  variant,
  object: schemaObject,
  strictObject,
  partial,
  lazy,
  optional,
  nullable,
  fallback,
  pipe,
  transform,
  check,
  brand,
  minLength,
  maxLength,
  startsWith,
  endsWith,
  regex,
  trim,
  email,
  min,
  max,
  integer,
  date,
  instance,
  parse,
  safeParse,
  useValidation,
};
