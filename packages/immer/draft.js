// @flow
//
// `@uniflowed/immer`: the draft, and the copy-on-write bookkeeping under it.
//
// A draft is the only thing a recipe ever touches. Everything here exists to
// keep one promise: the value handed to `produce` is never written to, and no
// reference the recipe holds can be used to write to it after `produce`
// returns.
//
// # Why the Proxy target is the state record, not the base
//
// The obvious design proxies the base object. It cannot be made safe. A trap
// receives the target, so every trap would hold a live reference to the
// caller's state, and one forgotten `Reflect.set(target, ...)` writes straight
// through to it. Proxying a small record that *describes* the update instead
// means the base is reachable only as `state.base`, and reading it is always a
// deliberate act. It is also faster: the trap already has the state, so no
// `WeakMap` lookup stands between a property read and the answer.
//
// Arrays are the exception, and only just: the target is `[state]`, because
// `Array.isArray` reads an internal slot no trap can forge, and application
// code branches on it. The array traps unwrap `target[0]` and hand the same
// state to the same implementations.
//
// # Why the drafts are revocable
//
// `Proxy.revocable` is what turns "do not keep the draft" from documentation
// into a rule. `produce` revokes every draft it made before it returns, so a
// draft that escapes — assigned to a module variable, captured by a callback,
// stored as React state — throws on the next access instead of silently
// reading a half-finished copy. The type checker cannot see a value escape a
// closure; the runtime can.
//
// # Why children are drafted lazily and installed on write
//
// Reading `draft.user.name` has to produce a draft for `user`, or writing
// through it would not be recorded. The usual implementation stores that child
// draft back into the parent's copy immediately, which means *reading* a
// nested object shallow-copies its parent — a recipe that only looks at state
// allocates a copy of every object on the path it looked at, and then throws
// them all away.
//
// So a child draft is remembered in `state.children` instead, and installs
// itself into the parent's copy in `markChanged`, at the moment it is first
// written. A recipe that changes nothing allocates no copies at all, and the
// copies a recipe that changes one leaf allocates are exactly the ones on the
// path from the root to that leaf. That is what structural sharing is.
//
// # Why Map and Set are proxies too
//
// The alternative is a `class DraftMap extends Map`, which is what most
// implementations do, and it cannot be revoked — a subclass instance stays
// usable forever and has to fake revocation with a flag every method checks.
// Proxying them keeps one lifecycle for all four kinds. The cost is one real
// incompatibility, written down rather than hidden: `Map.prototype.get.call`
// on a draft throws, because the draft has no map internal slot. Calling
// `draft.get(key)` — which is what code actually does — works, and so does
// `instanceof Map`, `Object.prototype.toString`, spreading and `new Map(draft)`.
//
// The collection methods are shared functions that find their state through
// `this[DRAFT_STATE]`, not closures built per draft. A drafted `Map` therefore
// costs one state record and one proxy, not one closure per method, and
// `draft.get === draft.get` holds the way it does on a real `Map`.
//
// # What of this is API
//
// `@uniflowed/immer/draft` exists for the code that receives a value which may
// or may not be a draft — a reducer helper, a logger, a devtool — and that
// code wants `isDraft`, `isDraftable`, `original` and `current`, which is why
// this module is reachable at all rather than hidden under `internal/`.
//
// The rest — `stateOf`, the scope functions, `createDraft`, and the container
// helpers `shallowCopy`, `eachEntry`, `setEntry`, `getEntry`, `deleteEntry`,
// `hasEntry`, `hasAssigned` — is the seam `produce`, `freeze` and `patches`
// are built on. It is exported because ES modules have no other way for a
// sibling to reach it, not because an application should call it. Say so here
// rather than let a reader guess from the shape.

/**
 * The key every draft answers to, and the only way in to a draft's state.
 *
 * `Symbol.for` rather than `Symbol()` so two copies of this package in one
 * dependency graph recognise each other's drafts. The alternative fails
 * silently and horribly: `isDraft` returns `false` for a real draft, and
 * `produce` publishes a live proxy as if it were finished state.
 */
const DRAFT_STATE: symbol = Symbol.for("@uniflowed/immer.draft");

/** Which of the four kinds of value a draft stands in for. */
export type DraftKind = "object" | "array" | "map" | "set";

/**
 * A writable view of `T`, for the recipe's parameter.
 *
 * The object arm is `{...{[K in keyof T]: ...}}` rather than the mapped type
 * alone because a bare mapped type keeps the source property's variance, so
 * `Draft<{readonly a: number}>` would still reject `draft.a = 1` — which is
 * the entire point of a draft. Spreading the mapped result drops the variance
 * and leaves the recursion intact.
 */
export type Draft<T> = T extends $ReadOnlyArray<infer Item>
  ? Array<Draft<Item>>
  : T extends $ReadOnlyMap<infer K, infer V>
    ? Map<K, Draft<V>>
    : T extends $ReadOnlySet<infer V>
      ? Set<Draft<V>>
      : T extends { ... }
        ? { ...{ [K in keyof T]: Draft<T[K]> } }
        : T;

/**
 * One `produce` call's drafts.
 *
 * Scopes nest because a recipe may call `produce` again, and the inner call
 * must not freeze or revoke anything the outer one still owns — hence
 * `suspended`, which is restored when the inner call leaves.
 */
export type Scope = {
  /** Every draft made in this scope, in creation order, for revocation. */
  drafts: Array<DraftState>,
  /** The scope this one interrupted, or `null` at the top. */
  suspended: null | Scope,
  /** Whether writes must record which keys they touched, for patches. */
  records: boolean,
  /** Whether the produced value may be frozen; see `finalizeProperty`. */
  freezable: boolean,
  /** Drafts made and not yet resolved by the finalize walk. */
  pending: number,
};

/**
 * What a draft is, underneath.
 *
 * `copy` is null until the first write, and that is the whole copy-on-write
 * story: a draft with no copy contributes its `base` to the result by
 * reference, which is why an untouched subtree comes out of `produce` as the
 * same object that went in.
 */
export type DraftState = {
  kind: DraftKind,
  scope: Scope,
  /** The draft this one hangs off, or `null` for the root. */
  parent: null | DraftState,
  /** Where this draft sits in `parent`. Meaningless when `parent` is null. */
  key: mixed,
  base: mixed,
  copy: mixed,
  /** The proxy handed to the recipe. */
  draft: mixed,
  modified: boolean,
  finalized: boolean,
  /** Drafted children, keyed the way the parent keys them. */
  children: null | Map<mixed, DraftState>,
  /** key to `true` (written) or `false` (removed), while recording patches. */
  assigned: null | Map<mixed, boolean>,
  revoke: () => void,
};

/**
 * The scope a draft with no parent joins.
 *
 * Module state, and the one piece of it here. `produce` is synchronous and
 * single-threaded, so "the scope being built right now" is a well-defined
 * thing; an `await` inside a recipe is not supported for exactly this reason,
 * and the drafts being revoked on return is what makes that failure loud.
 */
let openScope: null | Scope = null;

/**
 * Read an unknown value's property.
 *
 * The three casts in this file are all here, in `readProp`, `writeProp` and
 * `dropProp`: a draft's `base` and `copy` are `mixed` because a state record
 * describes all four kinds, and JavaScript indexes an object with a string or
 * a symbol while Flow's index types admit only the former.
 */
function readProp(target: mixed, prop: mixed): mixed {
  // $FlowFixMe[incompatible-use] every caller has established `target` is an object.
  return target[prop];
}

function writeProp(target: mixed, prop: mixed, value: mixed): void {
  // $FlowFixMe[incompatible-use] every caller has established `target` is an object.
  target[prop] = value;
}

function dropProp(target: mixed, prop: mixed): void {
  // $FlowFixMe[incompatible-use] every caller has established `target` is an object.
  delete target[prop];
}

function asObject(value: mixed): { ... } {
  // $FlowFixMe[incompatible-type] callers check `isDraftable` or `typeof` first.
  return value as { ... };
}

function asMap(value: mixed): Map<mixed, mixed> {
  // $FlowFixMe[incompatible-type] only a map-kind draft reaches this.
  return value as Map<mixed, mixed>;
}

function asSet(value: mixed): Set<mixed> {
  // $FlowFixMe[incompatible-type] only a set-kind draft reaches this.
  return value as Set<mixed>;
}

function asArray(value: mixed): Array<mixed> {
  // $FlowFixMe[incompatible-type] callers check `Array.isArray` first.
  return value as Array<mixed>;
}

function asKey(prop: mixed): string {
  // A property key is a string or a symbol, and every reflective API here
  // takes either; Flow's index types admit only the former.
  // $FlowFixMe[incompatible-type]
  return prop as string;
}

/**
 * Whether `prop` is reachable on `target`, inherited properties included.
 *
 * `in` rather than `Object.hasOwn`, because a draft has to answer about the
 * value it stands in for, prototype and all.
 */
function hasProp(target: mixed, prop: mixed): boolean {
  return asKey(prop) in asObject(target);
}

/**
 * `Object.prototype`, read once at load.
 *
 * Flow's library definitions do not declare `Object.prototype`, and "a plain
 * object" is exactly "one whose prototype is this, or nothing at all".
 */
const objectPrototype: mixed = Object.getPrototypeOf({});

/** The state behind `value`, or `null` when `value` is not a draft. */
export function stateOf(value: mixed): null | DraftState {
  if (value == null || typeof value !== "object") {
    return null;
  }
  const state = readProp(value, DRAFT_STATE);
  // $FlowFixMe[incompatible-type] nothing but a draft answers to the symbol.
  return state == null ? null : (state as DraftState);
}

/**
 * Whether `value` is a draft.
 *
 * Throws on a revoked draft rather than answering `false`, because a revoked
 * draft is not "not a draft" — it is a draft someone kept, and reporting that
 * as a plain value is how a stale proxy ends up published as state.
 */
export function isDraft(value: mixed): boolean {
  return stateOf(value) != null;
}

/**
 * Whether `produce` can draft `value`, which is also what `freeze` recurses
 * into and what `applyPatches` may walk through.
 *
 * Class instances are deliberately not draftable. Copying one means copying
 * whatever invariants its constructor established, and this library has no way
 * to know them; treating it as an opaque leaf is the only honest answer.
 */
export function isDraftable(value: mixed): boolean {
  if (value == null || typeof value !== "object") {
    return false;
  }
  if (Array.isArray(value)) {
    return true;
  }
  const proto = Object.getPrototypeOf(value);
  if (proto == null || proto === objectPrototype) {
    return true;
  }
  return value instanceof Map || value instanceof Set;
}

function kindOf(value: mixed): DraftKind {
  if (Array.isArray(value)) {
    return "array";
  }
  if (value instanceof Map) {
    return "map";
  }
  if (value instanceof Set) {
    return "set";
  }
  return "object";
}

/**
 * A copy of `value` one level deep, which the draft then writes into.
 *
 * Plain objects take the spread, which is the fast path a JIT recognises.
 * Anything else goes through descriptors so a null prototype survives; losing
 * it would silently give the copy an `Object.prototype`, and code that chose
 * `Object.create(null)` chose it to avoid exactly that.
 */
export function shallowCopy(value: mixed): mixed {
  if (Array.isArray(value)) {
    return value.slice();
  }
  if (value instanceof Map) {
    return new Map(value);
  }
  if (value instanceof Set) {
    return new Set(value);
  }
  const source = asObject(value);
  const proto = Object.getPrototypeOf(source);
  if (proto === objectPrototype) {
    return { ...source };
  }
  return Object.create(proto, Object.getOwnPropertyDescriptors(source));
}

/** Every entry of a value this package owns, as the pair its kind is keyed by. */
export function eachEntry(target: mixed, visit: (key: mixed, value: mixed) => void): void {
  match (kindOf(target)) {
    "array" => {
      const items = asArray(target);
      for (let index = 0; index < items.length; index += 1) {
        visit(index, items[index]);
      }
    }
    "map" => {
      asMap(target).forEach((value, key) => visit(key, value));
    }
    // A set is keyed by its members, so the key and the value are the same
    // thing. Callers that rebuild a set have to add rather than assign, which
    // is why `setEntry` takes the kind into account too.
    "set" => {
      asSet(target).forEach((value) => visit(value, value));
    }
    _ => {
      const source = asObject(target);
      for (const key of Object.keys(source)) {
        visit(key, readProp(source, key));
      }
    }
  }
}

/** Write `value` at `key`, in whichever way this kind of container is written. */
export function setEntry(target: mixed, key: mixed, value: mixed): void {
  match (kindOf(target)) {
    "map" => {
      asMap(target).set(key, value);
    }
    "set" => {
      asSet(target).add(value);
    }
    _ => {
      writeProp(target, key, value);
    }
  }
}

/** Read the value at `key`, in whichever way this kind of container is read. */
export function getEntry(target: mixed, key: mixed): mixed {
  return target instanceof Map ? target.get(key) : readProp(target, key);
}

/** Remove `key`, in whichever way this kind of container is emptied. */
export function deleteEntry(target: mixed, key: mixed): void {
  if (target instanceof Map || target instanceof Set) {
    target.delete(key);
    return;
  }
  dropProp(target, key);
}

/** Whether `key` is present, in whichever way this kind of container answers. */
export function hasEntry(target: mixed, key: mixed): boolean {
  if (target instanceof Map || target instanceof Set) {
    return target.has(key);
  }
  return hasProp(target, key);
}

/** Whether the recipe assigned or removed `key` on this draft. */
export function hasAssigned(state: DraftState, key: mixed): boolean {
  return state.assigned != null && state.assigned.has(key);
}

function latest(state: DraftState): mixed {
  return state.copy == null ? state.base : state.copy;
}

/**
 * Give `state` a copy to write into, without declaring it changed.
 *
 * Separate from `markChanged` because a `Set` has to build its copy merely to
 * hand out drafts of its members: a set is keyed by the member itself, so the
 * only place a draft can stand in for one is inside a rebuilt set.
 */
function prepareCopy(state: DraftState): void {
  if (state.copy != null) {
    return;
  }
  if (state.kind === "set") {
    const copy: Set<mixed> = new Set();
    asSet(state.base).forEach((member) => {
      copy.add(isDraftable(member) ? childDraft(state, member, member) : member);
    });
    state.copy = copy;
    return;
  }
  state.copy = shallowCopy(state.base);
}

/**
 * Record that this draft differs from its base, and that its ancestors do too.
 *
 * The walk up the parents is where the copies come from: an ancestor that was
 * only read still has no copy, so it is made here, and the child installs
 * itself into it. Everything not on this path keeps pointing at the base, and
 * comes out of `produce` as the identical object.
 */
function markChanged(state: DraftState): void {
  if (state.modified) {
    return;
  }
  state.modified = true;
  prepareCopy(state);
  const parent = state.parent;
  if (parent != null) {
    markChanged(parent);
    // A set already holds its children: `prepareCopy` put them there, because
    // there is no key to install them at afterwards.
    if (parent.kind !== "set") {
      setEntry(parent.copy, state.key, state.draft);
    }
  }
}

function recordAssigned(state: DraftState, key: mixed, written: boolean): void {
  if (!state.scope.records) {
    return;
  }
  let assigned = state.assigned;
  if (assigned == null) {
    assigned = new Map();
    state.assigned = assigned;
  }
  assigned.set(key, written);
}

/** A live proxy and the switch that kills it. */
type Revocable = {| readonly proxy: mixed, readonly revoke: () => void |};

/**
 * `Proxy.revocable`, behind the one suppression this package needs for it.
 *
 * Two things are being worked around, and both are in the type checker rather
 * than in the runtime. The target and the handler deliberately disagree: every
 * trap here reads the state record instead of the object the runtime hands it,
 * and the traps this library has no use for — `apply`, `construct` — are left
 * out. And Flow's own library definition says `Proxy.revocable` returns
 * `T & {revoke()}` rather than `{proxy, revoke}`, which is not what the
 * language does.
 */
function makeRevocable(target: mixed, traps: mixed): Revocable {
  // $FlowFixMe[incompatible-call]
  // $FlowFixMe[incompatible-type]
  // $FlowFixMe[prop-missing]
  // $FlowFixMe[incompatible-return]
  return Proxy.revocable(target, traps);
}

/** The draft standing in for `value` at `key`, made once and remembered. */
function childDraft(parent: DraftState, key: mixed, value: mixed): mixed {
  let children = parent.children;
  if (children == null) {
    children = new Map();
    parent.children = children;
  }
  const existing = children.get(key);
  if (existing != null) {
    return existing.draft;
  }
  const child = createDraft(parent.scope, value, parent, key);
  children.set(key, child);
  return child.draft;
}

const objectTraps = {
  get(state: DraftState, prop: mixed): mixed {
    if (prop === DRAFT_STATE) {
      return state;
    }
    const source = latest(state);
    const value = readProp(source, prop);
    if (state.finalized || !isDraftable(value)) {
      return value;
    }
    // Only what came out of the base is drafted. A value the recipe put here
    // is the recipe's own object, and drafting it would copy something the
    // caller can already mutate freely.
    if (value !== readProp(state.base, prop)) {
      return value;
    }
    return childDraft(state, prop, value);
  },

  has(state: DraftState, prop: mixed): boolean {
    return hasProp(latest(state), prop);
  },

  ownKeys(state: DraftState): Array<string | symbol> {
    return Reflect.ownKeys(asObject(latest(state)));
  },

  set(state: DraftState, prop: mixed, value: mixed): boolean {
    if (!state.modified) {
      const currentValue = readProp(latest(state), prop);
      // Writing back what is already there is not a change. The second half
      // separates "the property holds undefined" from "the property is absent",
      // which `===` cannot: assigning `undefined` to a key the base does not
      // have does add a key.
      if (Object.is(value, currentValue) && (value !== undefined || hasProp(state.base, prop))) {
        return true;
      }
      markChanged(state);
    }
    const copy = state.copy;
    if (Object.is(readProp(copy, prop), value) && (value !== undefined || hasProp(copy, prop))) {
      return true;
    }
    writeProp(copy, prop, value);
    recordAssigned(state, prop, true);
    return true;
  },

  deleteProperty(state: DraftState, prop: mixed): boolean {
    if (readProp(state.base, prop) !== undefined || hasProp(state.base, prop)) {
      markChanged(state);
      recordAssigned(state, prop, false);
    } else {
      // Never in the base, so removing it only has to undo an assignment.
      if (state.assigned != null) {
        state.assigned.delete(prop);
      }
      if (!state.modified) {
        return true;
      }
    }
    if (state.copy != null) {
      dropProp(state.copy, prop);
    }
    if (state.children != null) {
      state.children.delete(prop);
    }
    return true;
  },

  /**
   * Report the property as configurable, except an array's `length`.
   *
   * A proxy may not claim a property is configurable when the target's is not,
   * and `length` on the `[state]` target is the one such property in play. Get
   * this wrong and the runtime throws `TypeError: proxy can't report a
   * non-configurable own property as configurable` on `Object.keys(draft)`.
   */
  getOwnPropertyDescriptor(state: DraftState, prop: mixed): mixed {
    const owner = latest(state);
    const descriptor = Reflect.getOwnPropertyDescriptor(asObject(owner), asKey(prop));
    if (descriptor == null) {
      return descriptor;
    }
    return {
      writable: true,
      configurable: state.kind !== "array" || prop !== "length",
      enumerable: descriptor.enumerable,
      value: readProp(owner, prop),
    };
  },

  getPrototypeOf(state: DraftState): mixed {
    return Object.getPrototypeOf(asObject(state.base));
  },

  defineProperty(): empty {
    throw new Error("@uniflowed/immer: Object.defineProperty cannot be used on a draft");
  },

  setPrototypeOf(): empty {
    throw new Error("@uniflowed/immer: Object.setPrototypeOf cannot be used on a draft");
  },
};

/**
 * The same traps, over `[state]`.
 *
 * Written out rather than derived in a loop because a shipped module may not
 * run anything when it is imported, and because a reader looking for the array
 * behaviour should find it rather than a `for` loop over `Object.keys`.
 */
const arrayTraps = {
  get(target: Array<DraftState>, prop: mixed): mixed {
    return objectTraps.get(target[0], prop);
  },
  has(target: Array<DraftState>, prop: mixed): boolean {
    return objectTraps.has(target[0], prop);
  },
  ownKeys(target: Array<DraftState>): Array<string | symbol> {
    return objectTraps.ownKeys(target[0]);
  },
  set(target: Array<DraftState>, prop: mixed, value: mixed): boolean {
    return objectTraps.set(target[0], prop, value);
  },
  deleteProperty(target: Array<DraftState>, prop: mixed): boolean {
    return objectTraps.deleteProperty(target[0], prop);
  },
  getOwnPropertyDescriptor(target: Array<DraftState>, prop: mixed): mixed {
    return objectTraps.getOwnPropertyDescriptor(target[0], prop);
  },
  getPrototypeOf(target: Array<DraftState>): mixed {
    return objectTraps.getPrototypeOf(target[0]);
  },
  defineProperty(): empty {
    return objectTraps.defineProperty();
  },
  setPrototypeOf(): empty {
    return objectTraps.setPrototypeOf();
  },
};

function selfState(receiver: mixed): DraftState {
  const state = stateOf(receiver);
  if (state == null) {
    throw new Error(
      "@uniflowed/immer: a collection method was called with a `this` that is not a draft",
    );
  }
  return state;
}

function mapDraftGet(state: DraftState, key: mixed): mixed {
  const value = asMap(latest(state)).get(key);
  if (state.finalized || !isDraftable(value)) {
    return value;
  }
  if (value !== asMap(state.base).get(key)) {
    return value;
  }
  return childDraft(state, key, value);
}

function mapDraftSet(state: DraftState, key: mixed, value: mixed): void {
  const source = asMap(latest(state));
  if (source.has(key) && Object.is(source.get(key), value)) {
    return;
  }
  markChanged(state);
  asMap(state.copy).set(key, value);
  recordAssigned(state, key, true);
}

function mapDraftDelete(state: DraftState, key: mixed): boolean {
  if (!asMap(latest(state)).has(key)) {
    return false;
  }
  markChanged(state);
  if (asMap(state.base).has(key)) {
    recordAssigned(state, key, false);
  } else if (state.assigned != null) {
    state.assigned.delete(key);
  }
  asMap(state.copy).delete(key);
  if (state.children != null) {
    state.children.delete(key);
  }
  return true;
}

function mapDraftClear(state: DraftState): void {
  if (asMap(latest(state)).size === 0) {
    return;
  }
  markChanged(state);
  asMap(state.base).forEach((_value, key) => recordAssigned(state, key, false));
  asMap(state.copy).clear();
  state.children = null;
}

/**
 * A map draft's values, drafted as they are handed out.
 *
 * Written as a generator over the live key iterator rather than over a
 * snapshot, so it behaves the way a real `Map`'s iterator does when the recipe
 * writes to the map while walking it.
 */
function* mapDraftValues(state: DraftState): Iterator<mixed> {
  for (const key of asMap(latest(state)).keys()) {
    yield mapDraftGet(state, key);
  }
}

function* mapDraftEntries(state: DraftState): Iterator<[mixed, mixed]> {
  for (const key of asMap(latest(state)).keys()) {
    yield [key, mapDraftGet(state, key)];
  }
}

/**
 * `Map`'s surface, as functions that find their draft through `this`.
 *
 * Shared rather than bound per draft: `draft.get` has to be the same function
 * every time it is read, the way `map.get` is, and building ten closures for
 * every drafted `Map` in a large state tree is a cost with nothing to show for
 * it. Each entry is a thin wrapper so the work stays in named functions the
 * rest of this module can call without going through a proxy.
 */
const mapMethods = {
  get: function (this: mixed, key: mixed): mixed {
    return mapDraftGet(selfState(this), key);
  },
  set: function (this: mixed, key: mixed, value: mixed): mixed {
    mapDraftSet(selfState(this), key, value);
    return this;
  },
  has: function (this: mixed, key: mixed): boolean {
    return asMap(latest(selfState(this))).has(key);
  },
  delete: function (this: mixed, key: mixed): boolean {
    return mapDraftDelete(selfState(this), key);
  },
  clear: function (this: mixed): void {
    mapDraftClear(selfState(this));
  },
  forEach: function (
    this: mixed,
    visit: (value: mixed, key: mixed, map: mixed) => void,
    thisArg?: mixed,
  ): void {
    const draft = this;
    const state = selfState(this);
    // Through `mapDraftGet`, so the callback is handed drafts of the values
    // and can write through them.
    asMap(latest(state)).forEach((_value, key) => {
      visit.call(thisArg, mapDraftGet(state, key), key, draft);
    });
  },
  keys: function (this: mixed): Iterator<mixed> {
    return asMap(latest(selfState(this))).keys();
  },
  values: function (this: mixed): Iterator<mixed> {
    return mapDraftValues(selfState(this));
  },
  entries: function (this: mixed): Iterator<[mixed, mixed]> {
    return mapDraftEntries(selfState(this));
  },
  [Symbol.iterator]: function (this: mixed): Iterator<[mixed, mixed]> {
    return mapDraftEntries(selfState(this));
  },
};

/**
 * Whether the set draft holds `value`, as itself or as its draft.
 *
 * The second half is the part a plain `Set` never needs: once a member has
 * been drafted, the copy holds the draft in its place, and a recipe asking
 * about the original member is asking about the same entry.
 */
function setDraftHas(state: DraftState, value: mixed): boolean {
  const copy = state.copy;
  if (copy == null) {
    return asSet(state.base).has(value);
  }
  if (asSet(copy).has(value)) {
    return true;
  }
  const child = state.children == null ? null : state.children.get(value);
  return child != null && asSet(copy).has(child.draft);
}

function setDraftAdd(state: DraftState, value: mixed): void {
  if (setDraftHas(state, value)) {
    return;
  }
  prepareCopy(state);
  markChanged(state);
  asSet(state.copy).add(value);
}

function setDraftDelete(state: DraftState, value: mixed): boolean {
  if (!setDraftHas(state, value)) {
    return false;
  }
  prepareCopy(state);
  markChanged(state);
  const copy = asSet(state.copy);
  if (copy.delete(value)) {
    return true;
  }
  const child = state.children == null ? null : state.children.get(value);
  return child != null && copy.delete(child.draft);
}

function setDraftClear(state: DraftState): void {
  if (asSet(latest(state)).size === 0) {
    return;
  }
  prepareCopy(state);
  markChanged(state);
  asSet(state.copy).clear();
}

/**
 * `Set`'s surface.
 *
 * Every read that can expose a member goes through the copy, because a set
 * stores its members as its own keys: the only place a draft can stand in for
 * a member is inside the set itself, and `prepareCopy` is what puts it there
 * in the original order.
 */
const setMethods = {
  add: function (this: mixed, value: mixed): mixed {
    setDraftAdd(selfState(this), value);
    return this;
  },
  has: function (this: mixed, value: mixed): boolean {
    return setDraftHas(selfState(this), value);
  },
  delete: function (this: mixed, value: mixed): boolean {
    return setDraftDelete(selfState(this), value);
  },
  clear: function (this: mixed): void {
    setDraftClear(selfState(this));
  },
  forEach: function (
    this: mixed,
    visit: (value: mixed, key: mixed, set: mixed) => void,
    thisArg?: mixed,
  ): void {
    const draft = this;
    const state = selfState(this);
    prepareCopy(state);
    asSet(state.copy).forEach((member) => visit.call(thisArg, member, member, draft));
  },
  values: function (this: mixed): Iterator<mixed> {
    const state = selfState(this);
    prepareCopy(state);
    return asSet(state.copy).values();
  },
  keys: function (this: mixed): Iterator<mixed> {
    const state = selfState(this);
    prepareCopy(state);
    return asSet(state.copy).values();
  },
  entries: function (this: mixed): Iterator<[mixed, mixed]> {
    const state = selfState(this);
    prepareCopy(state);
    return asSet(state.copy).entries();
  },
  [Symbol.iterator]: function (this: mixed): Iterator<mixed> {
    const state = selfState(this);
    prepareCopy(state);
    return asSet(state.copy).values();
  },
};

const collectionTraps = {
  get(state: DraftState, prop: mixed): mixed {
    if (prop === DRAFT_STATE) {
      return state;
    }
    if (prop === "size") {
      return asMap(latest(state)).size;
    }
    if (prop === "constructor") {
      return state.kind === "map" ? Map : Set;
    }
    if (prop === Symbol.toStringTag) {
      return state.kind === "map" ? "Map" : "Set";
    }
    // Anything not in the table is deliberately `undefined` rather than the
    // real prototype's method: a `Map.prototype` method invoked on a proxy
    // throws on its internal slot, and a missing method is a better error than
    // one that looks present and fails from inside the runtime.
    return readProp(state.kind === "map" ? mapMethods : setMethods, prop);
  },

  has(state: DraftState, prop: mixed): boolean {
    return (
      prop === "size" || readProp(state.kind === "map" ? mapMethods : setMethods, prop) != null
    );
  },

  ownKeys(): Array<string | symbol> {
    return [];
  },

  getOwnPropertyDescriptor(): mixed {
    return undefined;
  },

  getPrototypeOf(state: DraftState): mixed {
    return Object.getPrototypeOf(asObject(state.base));
  },

  set(): empty {
    throw new Error("@uniflowed/immer: a Map or Set draft has no writable properties");
  },

  defineProperty(): empty {
    throw new Error("@uniflowed/immer: Object.defineProperty cannot be used on a draft");
  },

  setPrototypeOf(): empty {
    throw new Error("@uniflowed/immer: Object.setPrototypeOf cannot be used on a draft");
  },
};

/** Open a scope for one `produce` call, suspending whatever was open. */
export function enterScope(records: boolean): Scope {
  const scope: Scope = {
    drafts: [],
    suspended: openScope,
    records,
    freezable: true,
    pending: 0,
  };
  openScope = scope;
  return scope;
}

/** Restore the scope `enterScope` suspended. */
export function leaveScope(scope: Scope): void {
  if (openScope === scope) {
    openScope = scope.suspended;
  }
}

/**
 * Revoke every draft the scope made.
 *
 * Reverse order so a child is dead before its parent, which is the order the
 * finalize walk released them in and the order a debugger reads best.
 */
export function revokeScope(scope: Scope): void {
  for (let index = scope.drafts.length - 1; index >= 0; index -= 1) {
    scope.drafts[index].revoke();
  }
  scope.drafts.length = 0;
}

/** Build the draft standing in for `base`, and register it with its scope. */
export function createDraft(
  scope: Scope,
  base: mixed,
  parent: null | DraftState,
  key: mixed,
): DraftState {
  const kind = kindOf(base);
  const state: DraftState = {
    kind,
    scope,
    parent,
    key,
    base,
    copy: null,
    draft: null,
    modified: false,
    finalized: false,
    children: null,
    assigned: null,
    revoke: () => {},
  };
  const traps = match (kind) {
    "array" => arrayTraps,
    "map" | "set" => collectionTraps,
    _ => objectTraps,
  };
  const revocable = makeRevocable(kind === "array" ? [state] : state, traps);
  state.draft = revocable.proxy;
  state.revoke = revocable.revoke;
  scope.drafts.push(state);
  return state;
}

/**
 * The value `draft` was made from.
 *
 * The base, not a copy: it is the caller's own object, and handing it back is
 * the point — `original(draft) === base` is how a recipe compares what it has
 * to what it started with without paying for a snapshot.
 */
export function original<T>(draft: T): T {
  const state = stateOf(draft);
  if (state == null) {
    throw new Error("@uniflowed/immer: original() expects a draft");
  }
  // A draft of T stands in for a T; the runtime check above is what the type
  // checker cannot do.
  // $FlowFixMe[incompatible-type]
  return state.base as T;
}

/**
 * A plain snapshot of what `draft` holds right now.
 *
 * Deliberately a copy, and deliberately not frozen. `current` exists to be
 * logged, compared and kept while the recipe carries on writing, so it must
 * not share the mutable copy the draft is still writing into. An *unmodified*
 * draft is the exception and returns its base directly: nothing is going to
 * change it, and copying it would throw away the structural sharing that makes
 * this library worth using.
 *
 * That copy is the cost. `current` is for inspection; the cheap snapshot is
 * the value `produce` returns.
 */
export function current<T>(draft: T): T {
  const state = stateOf(draft);
  if (state == null) {
    throw new Error("@uniflowed/immer: current() expects a draft");
  }
  // $FlowFixMe[incompatible-type]
  return resolveDraft(state) as T;
}

function resolveDraft(state: DraftState): mixed {
  if (!state.modified) {
    return state.base;
  }
  return rebuild(state.copy, true);
}

function resolveValue(value: mixed): mixed {
  const state = stateOf(value);
  if (state != null) {
    return resolveDraft(state);
  }
  if (!isDraftable(value)) {
    return value;
  }
  // A container the recipe built itself may still hold drafts of the base, so
  // it has to be walked — but it is copied only if it turns out to hold one.
  return rebuild(value, false);
}

/**
 * `source` with every draft inside it replaced by that draft's own snapshot.
 *
 * `always` is set for the copy a modified draft is writing into, which must be
 * copied whether or not it holds drafts: the recipe has not finished with it,
 * and a snapshot that shares it would keep changing afterwards.
 */
function rebuild(source: mixed, always: boolean): mixed {
  if (source instanceof Set) {
    const members: Array<mixed> = [];
    let changed = false;
    source.forEach((member) => {
      const next = resolveValue(member);
      changed = changed || next !== member;
      members.push(next);
    });
    return changed || always ? new Set(members) : source;
  }
  let copy: mixed = always ? shallowCopy(source) : null;
  eachEntry(source, (key, child) => {
    const next = resolveValue(child);
    if (next !== child) {
      if (copy == null) {
        copy = shallowCopy(source);
      }
      setEntry(copy, key, next);
    }
  });
  return copy == null ? source : copy;
}
