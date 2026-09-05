// @flow
//
// The field path, and the value tree it addresses.
//
// A form's values are one plain object, and every other module in this package
// addresses part of it by a string: `"email"`, `"address.city"`,
// `"items.2.quantity"`. That string is the form's only naming scheme — it is
// what `register` hands an input as its `name`, what an error is keyed by, what
// `watch` subscribes to, and what `useFieldArray` rewrites when a row moves. So
// the grammar lives here and everything else takes it as given.
//
// # Why writes are immutable
//
// [`writeAt`] returns a new tree that shares every untouched subtree with the
// old one. That is not a style preference, it is what makes a snapshot legal.
// `useSyncExternalStore` requires `getSnapshot` to return a value whose
// identity changes when — and only when — the data changed, and a watcher on
// `"address"` compares exactly that identity to decide whether to re-render.
//
// A store that mutated the tree in place would hand every watcher the same
// object twice and never re-render anything; a version counter beside a mutable
// tree would fix that by re-rendering *everyone*. Structural sharing answers
// both questions at once, and it is the reason typing into `items.9.name` does
// not wake a watcher on `items.0`.
//
// # Why the segments are cached
//
// Splitting `"items.2.quantity"` is a per-keystroke cost: the change handler
// reads a path, the store writes one, and every watcher compares one. The paths
// in a form are a small closed set — as many as it has fields — so they are
// split once and remembered.
//
// The cache is capped because that is only true until someone renders a field
// array over a user-supplied list. Ten thousand rows are ten thousand distinct
// paths, and an unbounded `Map` keyed by strings that grow with data is a leak
// with a friendly name. Past the cap the split still happens, it is just not
// remembered.

/** A form's values: a plain object addressed by [`FieldPath`]. */
export type FieldValues = { readonly [string]: mixed, ... };

/**
 * A dotted path from the root of the values, e.g. `"items.2.quantity"`.
 *
 * A `string`, and deliberately not a type derived from the shape of the values.
 * Flow has no template-literal types, so there is no honest way to spell
 * "a path that exists in `TValues`" — and a type that only *looks* like it
 * checks paths would be worse than one that admits it does not. See the
 * package's `index.js` for what this costs and what is typed instead.
 */
export type FieldPath = string;

const SEGMENTS: Map<string, $ReadOnlyArray<string>> = new Map();

/** How many split paths are remembered. See the module docs. */
const SEGMENT_CACHE_LIMIT = 4096;

/**
 * Split a path into its segments, accepting both spellings of an index.
 *
 * `items[2].name` and `items.2.name` are the same path. Both spellings exist in
 * the wild — the first is what people write, the second is what a field array
 * produces when it joins a name to an index — and a library that treated them
 * as two paths would put the error somewhere the input could not find it.
 */
export function segmentsOf(path: FieldPath): $ReadOnlyArray<string> {
  if (path === "") {
    return EMPTY_SEGMENTS;
  }
  const cached = SEGMENTS.get(path);
  if (cached != null) {
    return cached;
  }
  const segments = path
    .replace(/\[(\w+)\]/g, ".$1")
    .split(".")
    .filter((segment) => segment !== "");
  if (SEGMENTS.size < SEGMENT_CACHE_LIMIT) {
    SEGMENTS.set(path, segments);
  }
  return segments;
}

const EMPTY_SEGMENTS: $ReadOnlyArray<string> = [];

/**
 * Whether a segment addresses an array index rather than an object key.
 *
 * Canonical decimals only: `"0"` and `"12"` are indices, `"01"` and `"1e3"` are
 * keys. The distinction decides what [`writeAt`] *creates* when a path runs
 * through a value that is not there yet, and creating an object where the rest
 * of the form expects an array is the kind of bug that only shows up once
 * somebody calls `.map` on it.
 */
function isIndex(segment: string): boolean {
  if (segment.length === 0 || segment.length > 10) {
    return false;
  }
  for (let at = 0; at < segment.length; at += 1) {
    const code = segment.charCodeAt(at);
    if (code < 48 || code > 57) {
      return false;
    }
  }
  return segment.length === 1 || segment.charCodeAt(0) !== 48;
}

/** Read the value at `path`; the whole tree for `""`. */
export function readAt(root: mixed, path: FieldPath): mixed {
  const segments = segmentsOf(path);
  let node = root;
  for (let at = 0; at < segments.length; at += 1) {
    if (node == null || typeof node !== "object") {
      return undefined;
    }
    node = (node as $FlowFixMe)[segments[at]];
  }
  return node;
}

/**
 * Set a property without running a setter the input chose.
 *
 * `node[key] = value` invokes the prototype's setter when `key` is
 * `__proto__`, so a field literally named that would change the object's
 * prototype instead of gaining a property — and a form's field names can come
 * from a schema, a server, or a row of user data. `defineProperty` writes an
 * own property whatever it is called. `@uniflowed/validator` guards its object
 * parser the same way, for the same reason.
 */
function put(node: { [string]: mixed, ... }, key: string, value: mixed): void {
  Object.defineProperty(node, key, {
    value,
    writable: true,
    enumerable: true,
    configurable: true,
  });
}

/** A shallow copy of `node` as the container `key` implies, creating one if needed. */
function container(node: mixed, key: string): { [string]: mixed, ... } | Array<mixed> {
  if (isIndex(key)) {
    return Array.isArray(node) ? node.slice() : [];
  }
  if (node != null && typeof node === "object" && !Array.isArray(node)) {
    return { ...(node as $FlowFixMe) };
  }
  return {};
}

function setIn(node: mixed, segments: $ReadOnlyArray<string>, at: number, value: mixed): mixed {
  const key = segments[at];
  const next = container(node, key);
  if (at === segments.length - 1) {
    put(next as $FlowFixMe, key, value);
    return next;
  }
  const child = node == null || typeof node !== "object" ? undefined : (node as $FlowFixMe)[key];
  put(next as $FlowFixMe, key, setIn(child, segments, at + 1, value));
  return next;
}

/**
 * A tree with `value` at `path`, sharing every untouched subtree with `root`.
 *
 * Only the containers along the path are copied: writing `items.2.quantity` in
 * a hundred-row form allocates three objects, not a hundred. Writing the same
 * value twice still allocates — the caller decides whether a write is a change,
 * because only the caller knows whether "same" means `Object.is` or
 * [`sameValue`].
 */
export function writeAt<TRoot>(root: TRoot, path: FieldPath, value: mixed): TRoot {
  const segments = segmentsOf(path);
  if (segments.length === 0) {
    return value as $FlowFixMe;
  }
  return setIn(root, segments, 0, value) as $FlowFixMe;
}

/**
 * A tree with `path` gone.
 *
 * An object loses the key. An array keeps its length and holds `undefined` at
 * that index, because the alternative — splicing — silently renames every path
 * after it, and the caller that wanted renaming is `useFieldArray`, which does
 * it deliberately and fixes up the errors and the keys to match.
 */
export function removeAt<TRoot>(root: TRoot, path: FieldPath): TRoot {
  const segments = segmentsOf(path);
  if (segments.length === 0) {
    return root;
  }
  const parentPath = segments.slice(0, -1);
  const key = segments[segments.length - 1];
  const parent = parentPath.length === 0 ? root : readAt(root, parentPath.join("."));
  if (parent == null || typeof parent !== "object") {
    return root;
  }
  if (Array.isArray(parent)) {
    return writeAt(root, path, undefined);
  }
  const copy: { [string]: mixed, ... } = { ...(parent as $FlowFixMe) };
  delete copy[key];
  return parentPath.length === 0 ? (copy as $FlowFixMe) : writeAt(root, parentPath.join("."), copy);
}

/**
 * Whether a change at `changed` is visible to a watcher of `watched`.
 *
 * True in both directions, and that is the point. A watcher on `"address"`
 * must wake when `"address.city"` is written, and a watcher on
 * `"address.city"` must wake when the whole of `"address"` is replaced. The
 * empty path is the root, which everything is under.
 */
export function pathsOverlap(watched: FieldPath, changed: FieldPath): boolean {
  if (watched === "" || changed === "" || watched === changed) {
    return true;
  }
  return watched.startsWith(`${changed}.`) || changed.startsWith(`${watched}.`);
}

/**
 * Whether `path` is `prefix` or lives under it.
 *
 * One-directional, unlike [`pathsOverlap`]: this is the question a field array
 * asks about the paths it owns, and `"items"` owns `"items.0.name"` but not
 * the other way round.
 */
export function isUnder(path: FieldPath, prefix: FieldPath): boolean {
  return prefix === "" || path === prefix || path.startsWith(`${prefix}.`);
}

/**
 * The index a path occupies within `prefix`, or `null` if it is not in it.
 *
 * `indexUnder("items.2.name", "items")` is `2`. This is how a field array
 * rewrites the errors, the dirty flags and the touched flags of the rows that
 * moved — they are keyed by path, so moving a row is a string rewrite rather
 * than a walk of the value tree.
 */
export function indexUnder(path: FieldPath, prefix: FieldPath): number | null {
  if (!path.startsWith(prefix === "" ? "" : `${prefix}.`)) {
    return null;
  }
  const rest = prefix === "" ? path : path.slice(prefix.length + 1);
  const end = rest.indexOf(".");
  const head = end < 0 ? rest : rest.slice(0, end);
  return isIndex(head) ? Number(head) : null;
}

/** `"items.2.name"` with its index replaced, given `prefix` of `"items"`. */
export function withIndexUnder(path: FieldPath, prefix: FieldPath, index: number): FieldPath {
  const rest = prefix === "" ? path : path.slice(prefix.length + 1);
  const end = rest.indexOf(".");
  const tail = end < 0 ? "" : rest.slice(end);
  return prefix === "" ? `${index}${tail}` : `${prefix}.${index}${tail}`;
}

/**
 * Whether two field values are the same as far as a form is concerned.
 *
 * Structural for arrays and plain objects, `Object.is` for everything else, so
 * an empty select and a `""` default are not reported as a change the user
 * made. Dates compare by their time, because two `Date` objects for the same
 * instant are two objects and one edit.
 *
 * This is what decides `isDirty`, and getting it wrong is not subtle: a form
 * that reports itself dirty on mount blocks navigation, enables Save, and
 * prompts about unsaved changes that nobody made.
 */
export function sameValue(left: mixed, right: mixed): boolean {
  if (Object.is(left, right)) {
    return true;
  }
  if (left instanceof Date && right instanceof Date) {
    return left.getTime() === right.getTime();
  }
  // A blank control and a missing default are the same emptiness. An input
  // that was never touched reads `""`, and a default of `undefined` or `null`
  // means the same thing to every form that ever rendered one.
  if (isBlank(left) && isBlank(right)) {
    return true;
  }
  if (Array.isArray(left) && Array.isArray(right)) {
    if (left.length !== right.length) {
      return false;
    }
    return left.every((item, at) => sameValue(item, right[at]));
  }
  if (isPlainObject(left) && isPlainObject(right)) {
    const leftKeys = Object.keys(left as $FlowFixMe);
    const rightKeys = Object.keys(right as $FlowFixMe);
    if (leftKeys.length !== rightKeys.length) {
      return false;
    }
    return leftKeys.every((key) =>
      sameValue((left as $FlowFixMe)[key], (right as $FlowFixMe)[key]),
    );
  }
  return false;
}

function isBlank(value: mixed): boolean {
  return value == null || value === "";
}

function isPlainObject(value: mixed): boolean {
  return value != null && typeof value === "object" && !Array.isArray(value);
}

/**
 * A deep copy of the default values.
 *
 * `defaultValues` is the caller's object, and `reset()` has to be able to hand
 * back what was originally passed however many times a nested field was
 * written since. Keeping a copy means a caller that later mutates the object it
 * passed does not change what the form resets to — which is a bug that
 * reproduces once a week and looks like the form losing data.
 */
export function cloneValues<TValue>(value: TValue): TValue {
  if (Array.isArray(value)) {
    return value.map((item) => cloneValues(item)) as $FlowFixMe;
  }
  if (value instanceof Date) {
    return new Date(value.getTime()) as $FlowFixMe;
  }
  if (isPlainObject(value)) {
    const copy: { [string]: mixed, ... } = {};
    for (const key of Object.keys(value as $FlowFixMe)) {
      put(copy, key, cloneValues((value as $FlowFixMe)[key]));
    }
    return copy as $FlowFixMe;
  }
  return value;
}
