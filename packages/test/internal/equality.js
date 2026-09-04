// @flow
//
// Structural comparison, and how a mismatch is described.
//
// Two values are compared the way a person means when they write
// `toEqual`: same shape, same contents, recursively, with the built-in types
// that have their own idea of equality (`Date`, `RegExp`, `Map`, `Set`,
// `Error`, typed arrays) compared by what they hold rather than by identity.
//
// Three details are deliberate, because getting them wrong is how an
// assertion library quietly lies:
//
// * `NaN` equals `NaN`, and `+0` does not equal `-0`. Both follow `Object.is`,
//   which is what a test author means by "the same number".
// * Cycles terminate. A pair already being compared is assumed equal while
//   its own comparison is in progress, which is the standard co-inductive
//   reading and the only one that terminates.
// * `toEqual` ignores `undefined` properties and `toStrictEqual` does not,
//   which is the one place the two matchers differ besides prototypes.

/** How strictly two values are compared. */
export type Strictness = "loose" | "strict";

type Pair = {| readonly left: mixed, readonly right: mixed |};

/** Longest rendering of one value inside a failure message. */
export const MAX_RENDER_BYTES: number = 4096;

/** Deepest structure the renderer descends into before it elides. */
const MAX_RENDER_DEPTH = 6;

/** Most entries of a collection the renderer shows before eliding. */
const MAX_RENDER_ENTRIES = 32;

function isObject(value: mixed): boolean {
  return typeof value === "object" && value !== null;
}

function tag(value: mixed): string {
  return Object.prototype.toString.call(value);
}

/**
 * Own enumerable keys, string and symbol, in a stable order.
 *
 * Insertion order is not stable across two objects built differently, and a
 * comparison that depended on it would report a difference where there is
 * none, so string keys are sorted.
 */
function ownKeys(value: interface {}, strictness: Strictness): Array<string | symbol> {
  const strings = Object.keys(value).sort();
  const symbols = Object.getOwnPropertySymbols(value).filter((symbol) =>
    Object.prototype.propertyIsEnumerable.call(value, symbol),
  );
  const keys: Array<string | symbol> = [...strings, ...symbols];
  if (strictness === "strict") {
    return keys;
  }
  // `toEqual` treats an absent property and one set to `undefined` as the
  // same thing, so a key holding `undefined` is not a difference.
  return keys.filter((key) => (value: $FlowFixMe)[key] !== undefined);
}

function sameSet(
  left: Set<mixed>,
  right: Set<mixed>,
  seen: Array<Pair>,
  strictness: Strictness,
): boolean {
  if (left.size !== right.size) {
    return false;
  }
  const remaining = [...right];
  for (const item of left) {
    const at = remaining.findIndex((candidate) => equals(item, candidate, seen, strictness));
    if (at === -1) {
      return false;
    }
    remaining.splice(at, 1);
  }
  return true;
}

function sameMap(
  left: Map<mixed, mixed>,
  right: Map<mixed, mixed>,
  seen: Array<Pair>,
  strictness: Strictness,
): boolean {
  if (left.size !== right.size) {
    return false;
  }
  const remaining = [...right];
  for (const [key, value] of left) {
    const at = remaining.findIndex(
      ([otherKey, otherValue]) =>
        equals(key, otherKey, seen, strictness) && equals(value, otherValue, seen, strictness),
    );
    if (at === -1) {
      return false;
    }
    remaining.splice(at, 1);
  }
  return true;
}

/**
 * Whether `left` and `right` are structurally equal.
 *
 * `seen` carries the pairs currently being compared, which is what makes a
 * cyclic structure terminate.
 */
export function equals(
  left: mixed,
  right: mixed,
  seen: Array<Pair> = [],
  strictness: Strictness = "loose",
): boolean {
  if (Object.is(left, right)) {
    return true;
  }
  if (!isObject(left) || !isObject(right)) {
    return false;
  }
  for (const pair of seen) {
    if (pair.left === left && pair.right === right) {
      return true;
    }
  }

  const leftTag = tag(left);
  if (leftTag !== tag(right)) {
    return false;
  }
  if (strictness === "strict" && Object.getPrototypeOf(left) !== Object.getPrototypeOf(right)) {
    return false;
  }

  const nested = [...seen, { left, right }];

  if (left instanceof Date && right instanceof Date) {
    return Object.is(left.getTime(), right.getTime());
  }
  if (left instanceof RegExp && right instanceof RegExp) {
    return left.source === right.source && left.flags === right.flags;
  }
  if (left instanceof Error && right instanceof Error) {
    return left.name === right.name && left.message === right.message;
  }
  if (left instanceof Set && right instanceof Set) {
    return sameSet(left, right, nested, strictness);
  }
  if (left instanceof Map && right instanceof Map) {
    return sameMap(left, right, nested, strictness);
  }
  if (ArrayBuffer.isView(left) && ArrayBuffer.isView(right)) {
    const leftBytes = new Uint8Array(
      (left: $FlowFixMe).buffer,
      (left: $FlowFixMe).byteOffset,
      (left: $FlowFixMe).byteLength,
    );
    const rightBytes = new Uint8Array(
      (right: $FlowFixMe).buffer,
      (right: $FlowFixMe).byteOffset,
      (right: $FlowFixMe).byteLength,
    );
    if (leftBytes.length !== rightBytes.length) {
      return false;
    }
    for (let index = 0; index < leftBytes.length; index += 1) {
      if (leftBytes[index] !== rightBytes[index]) {
        return false;
      }
    }
    return true;
  }
  if (Array.isArray(left) !== Array.isArray(right)) {
    return false;
  }
  if (Array.isArray(left) && Array.isArray(right) && left.length !== right.length) {
    return false;
  }

  const leftKeys = ownKeys((left: $FlowFixMe), strictness);
  const rightKeys = ownKeys((right: $FlowFixMe), strictness);
  if (leftKeys.length !== rightKeys.length) {
    return false;
  }
  for (const key of leftKeys) {
    if (!Object.prototype.hasOwnProperty.call(right, key)) {
      return false;
    }
    if (!equals((left: $FlowFixMe)[key], (right: $FlowFixMe)[key], nested, strictness)) {
      return false;
    }
  }
  return true;
}

/**
 * Whether every property `expected` names is present and equal in `received`.
 *
 * The recursive half of `toMatchObject`: extra properties on `received` are
 * fine, missing or different ones are not.
 */
export function matchesObject(received: mixed, expected: mixed, seen: Array<Pair> = []): boolean {
  if (!isObject(expected) || !isObject(received)) {
    return equals(received, expected, seen);
  }
  for (const pair of seen) {
    if (pair.left === received && pair.right === expected) {
      return true;
    }
  }
  const nested = [...seen, { left: received, right: expected }];

  if (Array.isArray(expected)) {
    if (!Array.isArray(received) || received.length !== expected.length) {
      return false;
    }
    return expected.every((item, index) => matchesObject(received[index], item, nested));
  }
  for (const key of ownKeys((expected: $FlowFixMe), "loose")) {
    if (!Object.prototype.hasOwnProperty.call(received, key)) {
      return false;
    }
    if (!matchesObject((received: $FlowFixMe)[key], (expected: $FlowFixMe)[key], nested)) {
      return false;
    }
  }
  return true;
}

function quoteString(value: string): string {
  return JSON.stringify(value) ?? `"${value}"`;
}

/**
 * A one-line-ish rendering of `value` for a failure message.
 *
 * This is not `JSON.stringify`: it has to show `undefined`, functions,
 * symbols, `NaN`, cycles and class instances, all of which JSON either drops
 * or refuses. Depth, breadth and total size are bounded, because a failure
 * message that scrolls the terminal is a failure message nobody reads.
 */
/**
 * An element's opening tag, or `null` if this is not an element.
 *
 * Duck-typed rather than `instanceof Element`, because this module must not
 * depend on a DOM existing: a test process without a document simply never has
 * a value that answers to this shape.
 */
function elementTag(value: mixed): string | null {
  const node: $FlowFixMe = value;
  if (
    node == null ||
    typeof node.tagName !== "string" ||
    typeof node.getAttribute !== "function" ||
    node.nodeType !== 1
  ) {
    return null;
  }
  const name = node.tagName.toLowerCase();
  const attributes = Array.from(node.attributes ?? [])
    .slice(0, MAX_RENDER_ENTRIES)
    .map((attribute: $FlowFixMe) => ` ${attribute.name}="${attribute.value}"`)
    .join("");
  const text = (node.textContent ?? "").replace(/\s+/g, " ").trim();
  const shown = text.length > 40 ? `${text.slice(0, 40)}…` : text;
  return shown === "" ? `<${name}${attributes} />` : `<${name}${attributes}>${shown}</${name}>`;
}

export function render(value: mixed, depth: number = 0, seen: Array<mixed> = []): string {
  const text = renderInner(value, depth, seen);
  return text.length > MAX_RENDER_BYTES ? `${text.slice(0, MAX_RENDER_BYTES)}… (elided)` : text;
}

function renderInner(value: mixed, depth: number, seen: Array<mixed>): string {
  if (value === undefined) {
    return "undefined";
  }
  if (value === null) {
    return "null";
  }
  switch (typeof value) {
    case "string":
      return quoteString(value);
    case "number":
      return Object.is(value, -0) ? "-0" : String(value);
    case "bigint":
      return `${String(value)}n`;
    case "boolean":
      return String(value);
    case "symbol":
      return String(value);
    case "function": {
      const name = (value: $FlowFixMe).name;
      return name === "" ? "[Function (anonymous)]" : `[Function ${name}]`;
    }
    default:
      break;
  }
  if (seen.includes(value)) {
    return "[Circular]";
  }
  if (depth >= MAX_RENDER_DEPTH) {
    return Array.isArray(value) ? "[Array]" : "[Object]";
  }

  const nested = [...seen, value];
  const inner = (item: mixed) => renderInner(item, depth + 1, nested);

  // An element, before the object branch reaches it. A DOM node's own
  // properties are event-listener maps and parent pointers, so rendering it as
  // an object buries the failure under a page of internals — and the whole
  // document, through the parent chain. Its opening tag is what a reader needs.
  const tag = elementTag(value);
  if (tag != null) {
    return tag;
  }
  if (value instanceof Date) {
    return `Date(${value.toISOString()})`;
  }
  if (value instanceof RegExp) {
    return String(value);
  }
  if (value instanceof Error) {
    return `${value.name}(${quoteString(value.message)})`;
  }
  if (value instanceof Set) {
    const items = [...value].slice(0, MAX_RENDER_ENTRIES).map(inner);
    const more =
      value.size > MAX_RENDER_ENTRIES ? `, …${value.size - MAX_RENDER_ENTRIES} more` : "";
    return `Set { ${items.join(", ")}${more} }`;
  }
  if (value instanceof Map) {
    const items = [...value]
      .slice(0, MAX_RENDER_ENTRIES)
      .map(([key, item]) => `${inner(key)} => ${inner(item)}`);
    const more =
      value.size > MAX_RENDER_ENTRIES ? `, …${value.size - MAX_RENDER_ENTRIES} more` : "";
    return `Map { ${items.join(", ")}${more} }`;
  }
  if (Array.isArray(value)) {
    const items = value.slice(0, MAX_RENDER_ENTRIES).map(inner);
    const more =
      value.length > MAX_RENDER_ENTRIES ? `, …${value.length - MAX_RENDER_ENTRIES} more` : "";
    return `[${items.join(", ")}${more}]`;
  }

  const record: $FlowFixMe = value;
  const keys = ownKeys(record, "strict").slice(0, MAX_RENDER_ENTRIES);
  const entries = keys.map((key) => {
    const name = typeof key === "symbol" ? `[${String(key)}]` : key;
    return `${name}: ${inner(record[key])}`;
  });
  const total = ownKeys(record, "strict").length;
  const more = total > MAX_RENDER_ENTRIES ? `, …${total - MAX_RENDER_ENTRIES} more` : "";
  const prototype = Object.getPrototypeOf(record);
  const constructorName =
    prototype != null && prototype.constructor != null && prototype.constructor.name !== "Object"
      ? `${prototype.constructor.name} `
      : "";
  return entries.length === 0
    ? `${constructorName}{}`
    : `${constructorName}{ ${entries.join(", ")}${more} }`;
}
