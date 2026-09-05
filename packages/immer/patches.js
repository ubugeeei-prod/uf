// @flow
//
// `@uniflowed/immer`: patches — what a recipe did, as data.
//
// A patch is the difference between two states, small enough to send. That is
// the whole reason this module exists: undo/redo keeps the inverses instead of
// whole snapshots, a collaborative editor sends the patches rather than the
// document, and an optimistic update is rolled back by applying the inverse.
//
// # Why recording and applying live together
//
// They are one contract read in two directions, and the only test that means
// anything is the round trip: `applyPatches(base, patches)` must equal what
// `produce` returned, and `applyPatches(next, inverse)` must equal the base.
// Splitting the two apart is how a generator and an applier drift until an
// `add` at the end of an array means "splice here" to one and "assign here" to
// the other.
//
// # Why applying does not go through `produce`
//
// The obvious implementation drafts the base and replays the patches into the
// draft. It is correct and it is wasteful: applying patches is a mechanical
// walk down known paths, and drafting builds a proxy, a state record and a
// revocation closure for every node on the way. So the walk here copies each
// node on a patch's path exactly once — the `owned` set is what makes "once"
// hold across a whole batch of patches — and shares everything else with the
// base, which is the same structural sharing `produce` gives, without the
// machinery.

import {
  type DraftState,
  deleteEntry,
  getEntry,
  hasAssigned,
  hasEntry,
  isDraft,
  isDraftable,
  setEntry,
  shallowCopy,
} from "./draft.js";
import { freeze, isAutoFreeze } from "./freeze.js";

/** What a patch does at its path. */
export type PatchOp = "add" | "remove" | "replace";

/**
 * One change, addressed by the path from the root of the state.
 *
 * The path is `mixed` rather than `Array<string | number>`, which is what most
 * implementations of this shape declare, because a `Map` key is any value at
 * all: the moment state contains a `Map` keyed by an object, a narrower type
 * is a lie the checker would help enforce. Code that only ever patches objects
 * and arrays can narrow each step where it reads it.
 *
 * A `path` of length zero addresses the whole state, and only `replace` uses
 * it: it is what `produceWithPatches` records when a recipe returns a new
 * value instead of writing to the draft.
 */
export type Patch = {|
  readonly op: PatchOp,
  readonly path: $ReadOnlyArray<mixed>,
  readonly value?: mixed,
|};

function asArray(value: mixed): Array<mixed> {
  // $FlowFixMe[incompatible-type] the caller has matched an array-kind draft.
  return value as Array<mixed>;
}

function asSet(value: mixed): Set<mixed> {
  // $FlowFixMe[incompatible-type] the caller has matched a set-kind draft.
  return value as Set<mixed>;
}

/**
 * Append the patches for one changed draft, and their inverses.
 *
 * Called from the finalize walk, after the draft's children have been
 * finalized, so every value read here is a published value and never a live
 * draft. Doing it earlier would put proxies into the patch stream, and a patch
 * holding a revoked proxy is worse than no patch at all.
 */
export function recordPatches(
  state: DraftState,
  path: $ReadOnlyArray<mixed>,
  patches: Array<Patch>,
  inverse: Array<Patch>,
): void {
  match (state.kind) {
    "array" => {
      recordArrayPatches(state, path, patches, inverse);
    }
    "set" => {
      recordSetPatches(state, path, patches, inverse);
    }
    _ => {
      recordAssignedPatches(state, path, patches, inverse);
    }
  }
}

/** The patches for a recipe that returned a value instead of writing to one. */
export function recordReplacement(
  base: mixed,
  replacement: mixed,
  patches: Array<Patch>,
  inverse: Array<Patch>,
): void {
  patches.push({ op: "replace", path: [], value: replacement });
  inverse.push({ op: "replace", path: [], value: base });
}

/**
 * Objects and maps: one patch per key the recipe touched.
 *
 * Keyed containers can say exactly what changed, so they do. `assigned` is the
 * record the draft kept while the recipe ran; without it a diff would have to
 * compare every key of the copy against the base, which costs the size of the
 * object rather than the size of the change.
 */
function recordAssignedPatches(
  state: DraftState,
  path: $ReadOnlyArray<mixed>,
  patches: Array<Patch>,
  inverse: Array<Patch>,
): void {
  const assigned = state.assigned;
  if (assigned == null) {
    return;
  }
  assigned.forEach((written, key) => {
    const before = getEntry(state.base, key);
    const after = getEntry(state.copy, key);
    const op: PatchOp = !written ? "remove" : hasEntry(state.base, key) ? "replace" : "add";
    // A key written back to the value it already held is not a change. It
    // happens whenever a recipe assigns unconditionally — `draft.status =
    // next` in a loop — and emitting it would fill an undo stack with
    // no-op steps.
    if (op === "replace" && before === after) {
      return;
    }
    const at = [...path, key];
    patches.push(op === "remove" ? { op, path: at } : { op, path: at, value: after });
    inverse.push(
      match (op) {
        "add" => { op: "remove", path: at },
        "remove" => { op: "add", path: at, value: before },
        _ => { op: "replace", path: at, value: before },
      },
    );
  });
}

/**
 * Arrays: replacements in the overlap, then the tail that was added or cut.
 *
 * When the result is shorter than the base, the two are swapped and the patch
 * lists with them, because shortening read backwards is lengthening: the same
 * loops then describe the removal, and its inverse restores the cut items in
 * order. Doing it any other way needs a second pair of loops that has to agree
 * with the first.
 */
function recordArrayPatches(
  state: DraftState,
  path: $ReadOnlyArray<mixed>,
  patches: Array<Patch>,
  inverse: Array<Patch>,
): void {
  let base = asArray(state.base);
  let copy = asArray(state.copy);
  let forward = patches;
  let backward = inverse;
  if (copy.length < base.length) {
    const shorter = base;
    base = copy;
    copy = shorter;
    forward = inverse;
    backward = patches;
  }
  for (let index = 0; index < base.length; index += 1) {
    if (hasAssigned(state, String(index)) && copy[index] !== base[index]) {
      const at = [...path, index];
      forward.push({ op: "replace", path: at, value: copy[index] });
      backward.push({ op: "replace", path: at, value: base[index] });
    }
  }
  for (let index = base.length; index < copy.length; index += 1) {
    forward.push({ op: "add", path: [...path, index], value: copy[index] });
  }
  // Backwards, so applying the inverse removes the last item first and every
  // earlier index is still where the patch says it is.
  for (let index = copy.length - 1; index >= base.length; index -= 1) {
    backward.push({ op: "remove", path: [...path, index] });
  }
}

/**
 * Sets: what left and what arrived, by position.
 *
 * A set has no keys, so a patch has to carry the member itself; the index in
 * the path is there to give the patch a stable address and is not read back
 * when it is applied. A member that was drafted and changed leaves as a
 * `remove` and returns as an `add`, because after the change it is a different
 * value and a set has no way to say otherwise.
 */
function recordSetPatches(
  state: DraftState,
  path: $ReadOnlyArray<mixed>,
  patches: Array<Patch>,
  inverse: Array<Patch>,
): void {
  const base = asSet(state.base);
  const copy = asSet(state.copy);
  let index = 0;
  base.forEach((member) => {
    if (!copy.has(member)) {
      patches.push({ op: "remove", path: [...path, index], value: member });
      inverse.unshift({ op: "add", path: [...path, index], value: member });
    }
    index += 1;
  });
  index = 0;
  copy.forEach((member) => {
    if (!base.has(member)) {
      patches.push({ op: "add", path: [...path, index], value: member });
      inverse.unshift({ op: "remove", path: [...path, index], value: member });
    }
    index += 1;
  });
}

/**
 * Keys that would rewrite the prototype chain rather than the state.
 *
 * A patch is data, and data arrives from somewhere — a websocket, a stored
 * undo stack, another process. `base["__proto__"] = value` runs a setter that
 * changes what every object in the program inherits, so a patch stream is a
 * prototype-pollution vector unless the walk refuses these two names.
 */
function guardStep(node: mixed, step: mixed): mixed {
  if ((step === "__proto__" || step === "constructor") && !(node instanceof Map)) {
    throw new Error(`@uniflowed/immer: a patch may not address ${String(step)}`);
  }
  return step;
}

function describePath(path: $ReadOnlyArray<mixed>): string {
  return path.length === 0 ? "the root" : path.map(String).join("/");
}

function applyStep(node: mixed, key: mixed, patch: Patch): void {
  if (patch.op === "replace") {
    if (node instanceof Set) {
      throw new Error(
        "@uniflowed/immer: a Set member cannot be replaced in place; remove it and add",
      );
    }
    setEntry(node, key, patch.value);
    return;
  }
  if (patch.op === "add") {
    // An `add` into an array inserts rather than assigns, which is what makes
    // the inverse of a `remove` restore the item at the position it left from.
    if (Array.isArray(node)) {
      asArray(node).splice(Number(key), 0, patch.value);
      return;
    }
    setEntry(node, key, patch.value);
    return;
  }
  if (patch.op === "remove") {
    if (Array.isArray(node)) {
      asArray(node).splice(Number(key), 1);
      return;
    }
    // A set has no keys, so the patch's own value says which member to drop.
    deleteEntry(node, node instanceof Set ? patch.value : key);
    return;
  }
  // Patches arrive as data — from a socket, a stored undo stack, another
  // process — so an op outside the three is a real runtime possibility rather
  // than a case the type checker has already ruled out.
  throw new Error(`@uniflowed/immer: unknown patch op ${String(patch.op)}`);
}

/** `value`, or a copy of it that this batch of patches is free to write to. */
function own(value: mixed, owned: Set<mixed>): mixed {
  if (owned.has(value)) {
    return value;
  }
  const copy = shallowCopy(value);
  owned.add(copy);
  return copy;
}

function applyOne(root: mixed, patch: Patch, owned: Set<mixed>): mixed {
  const path = patch.path;
  if (path.length === 0) {
    if (patch.op !== "replace") {
      throw new Error(`@uniflowed/immer: a ${patch.op} patch cannot address the whole state`);
    }
    return patch.value;
  }
  const next = own(root, owned);
  let node = next;
  for (let index = 0; index < path.length - 1; index += 1) {
    const step = guardStep(node, path[index]);
    const child = getEntry(node, step);
    if (!isDraftable(child)) {
      throw new Error(`@uniflowed/immer: no value to patch at ${describePath(path)}`);
    }
    const owning = own(child, owned);
    if (owning !== child) {
      setEntry(node, step, owning);
    }
    node = owning;
  }
  applyStep(node, guardStep(node, path[path.length - 1]), patch);
  return next;
}

function applyToDraft(draft: mixed, patch: Patch): void {
  const path = patch.path;
  if (path.length === 0) {
    throw new Error("@uniflowed/immer: a patch cannot replace the draft it is applied to");
  }
  let node = draft;
  for (let index = 0; index < path.length - 1; index += 1) {
    node = getEntry(node, guardStep(node, path[index]));
    if (node == null || typeof node !== "object") {
      throw new Error(`@uniflowed/immer: no value to patch at ${describePath(path)}`);
    }
  }
  applyStep(node, guardStep(node, path[path.length - 1]), patch);
}

/**
 * `base` with `patches` applied, sharing everything they did not touch.
 *
 * A draft is written to in place and returned, which is what makes
 * `produce(base, (draft) => applyPatches(draft, patches))` work and is how a
 * caller replays a patch stream alongside its own edits. Anything else is
 * treated as published state and is never written to: the result is a new
 * value, frozen when auto-freezing is on.
 *
 * Patch values are used by reference rather than deep-copied. Freezing the
 * result makes the sharing safe, and copying every incoming value would double
 * the cost of applying a stream of patches for a guarantee the freeze already
 * gives. With auto-freezing off, a caller that keeps mutating a value it put
 * in a patch will see the applied state change with it.
 */
export function applyPatches<T>(base: T, patches: $ReadOnlyArray<Patch>): T {
  if (isDraft(base)) {
    for (const patch of patches) {
      applyToDraft(base, patch);
    }
    return base;
  }
  if (!isDraftable(base) && patches.length > 0) {
    throw new Error("@uniflowed/immer: applyPatches expects an object, array, Map or Set");
  }
  const owned: Set<mixed> = new Set();
  let result: mixed = base;
  for (const patch of patches) {
    result = applyOne(result, patch, owned);
  }
  if (isAutoFreeze()) {
    freeze(result, true);
  }
  // Patches describe a change to a T, and the walk above is untyped by
  // necessity: a patch path is data.
  // $FlowFixMe[incompatible-type]
  return result as T;
}
