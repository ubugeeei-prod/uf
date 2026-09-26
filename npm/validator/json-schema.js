// @flow
//
// `@uniflowed/validator/json-schema`: a schema as a document somebody else can
// read.
//
//   const { schema, unrepresentable } = toJsonSchema(Account);
//   // { $schema: "…/2020-12/schema", type: "object", properties: { … } }
//
// The roadmap calls this "schema exports", and it is the reason `schema.js`
// makes every schema carry a [`Description`] beside its parse function. A
// closure cannot be read: a schema that was *only* the four lines that check a
// value can be run and can never be published, documented, handed to an
// OpenAPI generator, or turned into types for a consumer written in another
// language. The description is the readable half, and this module is the first
// consumer of it.
//
// # What "unrepresentable" means, and why it is a return value
//
// JSON Schema describes JSON. `map`, `set`, `bigint`, `date`, `instance` and
// `custom` describe values that JSON does not have, and `check` describes a
// predicate no declarative format can express. A converter has three options
// for those: throw, lie, or say so. Throwing makes one `Date` field fatal for a
// document that is otherwise fine. Lying — emitting `{}` and moving on — is
// what makes an exported schema quietly weaker than the one the application
// runs.
//
// So this says so. The node becomes `{}`, which accepts anything and is
// therefore *never wrong*, only imprecise, and the place it happened is
// reported in `unrepresentable` with the path to it. A caller that wants the
// strict behaviour asserts the list is empty; a caller documenting an API that
// happens to have one `Date` in it gets its document.
//
// # The exported document describes the input, not the output
//
// `pipe(string(), transform(Number))` exports as `{ "type": "string" }`. That
// is the whole point of a JSON Schema: it validates what arrives on the wire,
// and what arrives is the input. `InferOutput` is for the code on this side of
// the boundary; the document is for the code on the other side, which has
// never heard of the transform.
//
// The consequence is easy to get wrong, and this module got it wrong first: a
// step *after* a transform constrains the output, not the input.
// `pipe(string(), transform(Number), min(18))` must not export as `{ "type":
// "string", "minimum": 18 }`, which would ask a consumer to compare a string
// against a number. It exports as `{ "type": "string" }`, and `min after a
// transform` is reported as unrepresentable — because the schema does reject
// `"5"` and the document does not, and that is exactly what the list is for.
//
// `fallback` follows the same rule to its conclusion: a fallback accepts every
// value, so it exports as `{}`. Nothing is lost that JSON Schema could have
// held.
//
// # Recursion
//
// A `lazy` schema is a cycle, and the description of one is a thunk rather
// than a value for exactly that reason. Each `lazy` carries an identity, and
// the walk keeps a table from that identity to a name under `$defs`: the first
// visit converts the body, and every visit after it emits a `$ref`. A comment
// tree comes out as one definition and a reference to it, which is what JSON
// Schema is for.
//
// # What this is not, yet
//
// `uf prepare` lists a `GenerateValidatorTypes` step, and there is no code
// behind it — no crate reads a schema and writes a `.js.flow`. This module is
// the half that can exist without one: a description a generator would read,
// and a converter proving the description is complete enough to build
// something from. When that step is implemented it should consume
// [`describe`], not re-derive a shape from the source.

import type { Path } from "./issue.js";
import { put } from "./plain-object.js";
import type { Constraint, Description, Schema } from "./schema.js";
import { describe } from "./schema.js";

/** One node of a JSON Schema document. */
export type JsonSchemaNode = { readonly [string]: mixed, ... };

/** Somewhere the document is less precise than the schema it came from. */
export type Unrepresentable = {|
  readonly path: Path,
  readonly kind: string,
|};

/** A JSON Schema document, and everything JSON Schema could not say. */
export type JsonSchemaExport = {|
  readonly schema: JsonSchemaNode,
  readonly unrepresentable: $ReadOnlyArray<Unrepresentable>,
|};

const DIALECT = "https://json-schema.org/draft/2020-12/schema";

/** `left` with `right`'s keywords on top, written prototype-safely. */
function merge(left: JsonSchemaNode, right: JsonSchemaNode): JsonSchemaNode {
  const out: { [string]: mixed, ... } = {};
  for (const key of Object.keys(left)) {
    put(out, key, left[key]);
  }
  for (const key of Object.keys(right)) {
    put(out, key, right[key]);
  }
  return out;
}

/**
 * Whether a field may be absent.
 *
 * `optional`, `nullish` and a default all mean the key can be missing, and any
 * of them may be wrapped in the steps of a pipeline — `pipe(optional(string()),
 * …)` is an `optional` under a `constrained` — so the wrappers are unwrapped
 * before the question is asked. `nullable` is not here: `null` is a value, and
 * a key holding it is present.
 */
function mayBeAbsent(description: Description): boolean {
  return match (description) {
    {kind: "optional", inner: _} => true,
    {kind: "nullish", inner: _} => true,
    {kind: "default", inner: _} => true,
    {kind: "fallback", inner: _} => true,
    {kind: "constrained", inner: const inner, constraint: _} => mayBeAbsent(inner),
    {kind: "transformed", inner: const inner} => mayBeAbsent(inner),
    _ => false,
  };
}

/**
 * Whether a description is of a value some step already changed.
 *
 * Only the spine of pipeline wrappers is followed. A transform *inside* an
 * object's field does not make the object transformed: the field's own node is
 * where that is handled.
 */
function describesTransformedValue(description: Description): boolean {
  return match (description) {
    {kind: "transformed", inner: _} => true,
    {kind: "constrained", inner: const inner, constraint: _} => describesTransformedValue(inner),
    _ => false,
  };
}

/** What to call a constraint in the `unrepresentable` list. */
function labelFor(constraint: Constraint): string {
  return constraint.kind === "opaque" ? `check ${constraint.label}` : constraint.kind;
}

/** The JSON Schema keywords one `pipe` step contributes, if any. */
function keywordsFor(constraint: Constraint): JsonSchemaNode {
  return match (constraint) {
    {kind: "minLength", value: const value} => { minLength: value },
    {kind: "maxLength", value: const value} => { maxLength: value },
    {kind: "length", value: const value} => { minLength: value, maxLength: value },
    {kind: "minItems", value: const value} => { minItems: value },
    {kind: "maxItems", value: const value} => { maxItems: value },
    {kind: "min", value: const value} => { minimum: value },
    {kind: "max", value: const value} => { maximum: value },
    {kind: "integer"} => { type: "integer" },
    {kind: "multipleOf", value: const value} => { multipleOf: value },
    {kind: "pattern", source: const source} => { pattern: source },
    {kind: "format", name: const name} => { format: name },
    {kind: "brand", name: const name} => { title: name },
    {kind: "opaque", label: _} => {},
  };
}

/**
 * Convert `schema` to a JSON Schema document.
 *
 * The walk keeps three pieces of state — the definitions a recursive schema
 * needs, the names already given out, and the places JSON Schema could not
 * say what the schema meant — which is why it is a closure over a converter
 * rather than a free function.
 */
export function toJsonSchema(schema: Schema<mixed, mixed>): JsonSchemaExport {
  const definitions: { [string]: JsonSchemaNode, ... } = {};
  const names = new Map<symbol, string>();
  const unrepresentable: Array<Unrepresentable> = [];

  function unsupported(kind: string, path: Path): JsonSchemaNode {
    unrepresentable.push({ path: path.slice(), kind });
    return {};
  }

  function object(
    entries: $ReadOnlyArray<[string, Description]>,
    unknownKeys: "strip" | "reject" | "keep",
    path: Path,
  ): JsonSchemaNode {
    const properties: { [string]: JsonSchemaNode, ... } = {};
    const required: Array<string> = [];
    for (const [key, inner] of entries) {
      put(properties, key, convert(inner, path.concat(key)));
      if (!mayBeAbsent(inner)) {
        required.push(key);
      }
    }
    const base = { type: "object", properties, required };
    return unknownKeys === "reject" ? merge(base, { additionalProperties: false }) : base;
  }

  function recursive(id: symbol, inner: () => Description, path: Path): JsonSchemaNode {
    const already = names.get(id);
    if (already != null) {
      return { $ref: `#/$defs/${already}` };
    }
    const name = `definition${String(names.size)}`;
    names.set(id, name);
    put(definitions, name, convert(inner(), path));
    return { $ref: `#/$defs/${name}` };
  }

  function convert(description: Description, path: Path): JsonSchemaNode {
    return match (description) {
      {kind: "unknown"} => {},
      {kind: "never"} => { not: {} },
      {kind: "string"} => { type: "string" },
      {kind: "number"} => { type: "number" },
      {kind: "boolean"} => { type: "boolean" },
      {kind: "null"} => { type: "null" },
      {kind: "bigint"} => unsupported("bigint", path),
      {kind: "undefined"} => unsupported("undefined", path),
      {kind: "date"} => unsupported("date", path),
      {kind: "instance", name: const name} => unsupported(`instance ${name}`, path),
      {kind: "custom", name: const name} => unsupported(`custom ${name}`, path),
      {kind: "map", key: _, value: _} => unsupported("map", path),
      {kind: "set", item: _} => unsupported("set", path),
      {kind: "literal", value: const value} => { const: value },
      {kind: "enum", values: const values} => { enum: values.slice() },
      {kind: "array", item: const item} =>
        {
          type: "array",
          items: convert(item, path.concat("*")),
        },
      {kind: "tuple", items: const items} =>
        {
          type: "array",
          prefixItems: items.map((item, index) => convert(item, path.concat(String(index)))),
          minItems: items.length,
          maxItems: items.length,
        },
      {kind: "record", value: const value} =>
        {
          type: "object",
          additionalProperties: convert(value, path.concat("*")),
        },
      {kind: "object", entries: const entries, unknownKeys: const unknownKeys} =>
        object(entries, unknownKeys, path),
      {kind: "union", options: const options} =>
        {
          anyOf: options.map((option) => convert(option, path)),
        },
      {kind: "variant", key: _, branches: const branches} =>
        {
          oneOf: branches.map(([, branch]) => convert(branch, path)),
        },
      {kind: "intersect", parts: const parts} =>
        {
          allOf: parts.map((part) => convert(part, path)),
        },
      {kind: "optional", inner: const inner} => convert(inner, path),
      {kind: "default", inner: const inner} => convert(inner, path),
      {kind: "nullable", inner: const inner} =>
        {
          anyOf: [convert(inner, path), { type: "null" }],
        },
      {kind: "nullish", inner: const inner} =>
        {
          anyOf: [convert(inner, path), { type: "null" }],
        },
      {kind: "fallback", inner: _} => {},
      {kind: "transformed", inner: const inner} => convert(inner, path),
      {kind: "lazy", id: const id, inner: const inner} => recursive(id, inner, path),
      {kind: "constrained", inner: const inner, constraint: const constraint} =>
        constrained(inner, constraint, path),
    };
  }

  /**
   * A pipeline step's keywords on top of what it refined.
   *
   * Two of them contribute nothing. A `check`'s predicate is a closure, and a
   * step that sits above a `transform` is about the output rather than the
   * input this document describes. Both leave the node alone and add a line
   * saying the document is looser than the schema at that path.
   */
  function constrained(inner: Description, constraint: Constraint, path: Path): JsonSchemaNode {
    const node = convert(inner, path);
    if (describesTransformedValue(inner)) {
      unrepresentable.push({
        path: path.slice(),
        kind: `${labelFor(constraint)} after a transform`,
      });
      return node;
    }
    if (constraint.kind === "opaque") {
      unrepresentable.push({ path: path.slice(), kind: labelFor(constraint) });
      return node;
    }
    return merge(node, keywordsFor(constraint));
  }

  const root = convert(describe(schema), []);
  const document =
    Object.keys(definitions).length === 0
      ? merge({ $schema: DIALECT }, root)
      : merge(merge({ $schema: DIALECT }, root), { $defs: definitions });
  return { schema: document, unrepresentable };
}
