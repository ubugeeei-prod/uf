// @flow
//
// `@uniflowed/immer`: `produce` — the boundary mutation is allowed to cross.
//
// Application state is immutable; writing it by hand is not. `produce` takes
// the value, hands a recipe a draft that looks like it can be assigned to, and
// returns a new immutable value built from what the recipe did. Mutation
// happens, and it happens to a proxy that exists for the length of one call.
//
// ```js
// const next = produce(state, (draft) => {
//   draft.todos[2].done = true;
// });
// // next !== state, next.todos !== state.todos, next.todos[0] === state.todos[0]
// ```
//
// # Why the walk that publishes the result is here and not with the drafts
//
// `draft.js` records what happened. This module decides what it means. The
// finalize walk is the one place that knows all four of the things that have
// to agree: which subtrees are unchanged and may be shared, which copies are
// finished and may be frozen, where each change sits for a patch, and when a
// draft may be revoked. Splitting those apart is how a value gets frozen
// before its children are finalized, or revoked while the patch generator
// still needs to read it.
//
// # Why a recipe may not both return and mutate
//
// `produce` cannot merge the two. If the recipe assigned to the draft and also
// returned a value, either the assignments are silently discarded or the
// return is, and both are the kind of bug that survives review because the
// code reads as if it works. So it is an error, raised before anything is
// published.
//
// Returning `undefined` means "use the draft", which is why a recipe cannot
// produce `undefined` as its state. An arrow body that happens to evaluate to
// something — `(draft) => (draft.count += 1)` — is a real hazard, and it is
// why the error above says which of the two the recipe should pick.
//
// # Using it with React and with reducers
//
// The curried form is the integration, and no React binding is needed for it:
//
// ```js
// const reducer = produce((draft, action) => {
//   match (action.type) { "add" => { draft.items.push(action.item); } _ => {} }
// });
// const [state, dispatch] = useReducer(reducer, initial);
// ```
//
// `produce` returns the identical value when a recipe changes nothing, so a
// `useState` setter given the result bails out of the re-render on its own,
// and a memoised child sees the same props object it had before. That is the
// whole reason structural sharing is worth its cost, and it is why nothing in
// this package hands a draft to React: a draft is revoked by the time a render
// could read it, and its identity says nothing about whether anything changed.
//
// # Cost
//
// One proxy and one state record per object the recipe *reaches*, one shallow
// copy per object it *changes*, and nothing at all for the rest of the tree. A
// recipe that reads a hundred nodes and writes one leaf allocates a hundred
// proxies and copies the handful of objects between the root and that leaf.
// Freezing is the other half of the bill; see `freeze.js` for the switch.

import type { Draft, DraftState, Scope } from "./draft.js";
import {
  createDraft,
  current,
  eachEntry,
  enterScope,
  hasAssigned,
  isDraft,
  isDraftable,
  leaveScope,
  original,
  revokeScope,
  setEntry,
  stateOf,
} from "./draft.js";
import { freeze, isAutoFreeze, isFrozen, setAutoFreeze } from "./freeze.js";
import type { Patch, PatchOp } from "./patches.js";
import { applyPatches, recordPatches, recordReplacement } from "./patches.js";

export type { Draft, Patch, PatchOp };
export { applyPatches, current, freeze, isDraft, isDraftable, original, setAutoFreeze };

/**
 * What a recipe may do and may return.
 *
 * `void | T`, and nothing wider: returning the draft is returning a `Draft<T>`,
 * which Flow accepts as a `T` because dropping `readonly` widens in the
 * direction assignment already allows. The rest arguments are what makes the
 * curried form a reducer — `produce((draft, action) => ...)` — without a
 * second entry point.
 */
export type Recipe<T> = (draft: Draft<T>, ...rest: $ReadOnlyArray<mixed>) => void | T;

/**
 * `produce`'s two shapes, as an intersection so both infer.
 *
 * Flow picks the first arm whose parameters match, so the two-argument form is
 * written first and the curried form only applies when there is no base.
 */
type Produce = (<T>(base: T, recipe: Recipe<T>) => T) &
  (<T>(recipe: Recipe<T>) => (base: T, ...rest: $ReadOnlyArray<mixed>) => T);

/** The patch buffers one `produce` call fills, or `null` when nobody asked. */
type Recorder = {| readonly patches: Array<Patch>, readonly inverse: Array<Patch> |};

function asSet(value: mixed): Set<mixed> {
  // $FlowFixMe[incompatible-type] only a set-kind draft's copy reaches this.
  return value as Set<mixed>;
}

/**
 * Build the next value from `base` and what `recipe` does to a draft of it.
 *
 * Two shapes:
 *
 * - `produce(base, recipe)` returns the next value.
 * - `produce(recipe)` returns a function that does, for any base. Extra
 *   arguments are passed on to the recipe, which is what makes it a reducer.
 *
 * The recipe writes to the draft, or returns a replacement, or does neither
 * and the base comes back unchanged — by identity, not merely by value. Doing
 * both is an error.
 */
// The implementation is one function; the type is the overload set callers
// see, and Flow has no way to write both at once in a module that also
// implements it.
// $FlowFixMe[incompatible-type]
export const produce = produceImpl as Produce;

/**
 * `produce`, plus the patches that describe what it did and how to undo it.
 *
 * Recording is off unless it is asked for, because it is not free: a draft has
 * to remember every key it touched, and the finalize walk has to build a path
 * for each of them. A `produce` that records nothing keeps neither.
 */
export function produceWithPatches<T>(
  base: T,
  recipe: Recipe<T>,
): [T, $ReadOnlyArray<Patch>, $ReadOnlyArray<Patch>] {
  const patches: Array<Patch> = [];
  const inverse: Array<Patch> = [];
  const next = run(base, recipe, [], { patches, inverse });
  // A produce of a T yields a T; the draft machinery is untyped underneath by
  // necessity, and this is where the type comes back.
  // $FlowFixMe[incompatible-type]
  return [next as T, patches, inverse];
}

function produceImpl(first: mixed, second: mixed): mixed {
  if (typeof first === "function" && typeof second !== "function") {
    const recipe = first;
    const fallback = second;
    return (base: mixed, ...rest: $ReadOnlyArray<mixed>): mixed =>
      run(base === undefined ? fallback : base, recipe, rest, null);
  }
  if (typeof second !== "function") {
    throw new Error("@uniflowed/immer: produce needs a recipe function");
  }
  return run(first, second, [], null);
}

function run(
  base: mixed,
  recipe: mixed,
  rest: $ReadOnlyArray<mixed>,
  recorder: null | Recorder,
): mixed {
  // `produceImpl` has already checked this is callable.
  // $FlowFixMe[incompatible-type]
  const apply = recipe as (...args: $ReadOnlyArray<mixed>) => mixed;
  if (!isDraftable(base)) {
    // Nothing to draft, so the recipe's only move is to return a replacement.
    const produced = apply(base, ...rest);
    const result = produced === undefined ? base : produced;
    if (recorder != null) {
      recordReplacement(base, result, recorder.patches, recorder.inverse);
    }
    if (isAutoFreeze()) {
      freeze(result, true);
    }
    return result;
  }

  const scope = enterScope(recorder != null);
  const state = createDraft(scope, base, null, undefined);
  let produced: mixed;
  try {
    produced = apply(state.draft, ...rest);
  } catch (error) {
    // Nothing is published, so nothing may survive: a draft left alive after a
    // failed recipe is a proxy over a half-written copy.
    revokeScope(scope);
    throw error;
  } finally {
    leaveScope(scope);
  }
  return publish(scope, state, produced, recorder);
}

function publish(
  scope: Scope,
  rootState: DraftState,
  produced: mixed,
  recorder: null | Recorder,
): mixed {
  scope.pending = scope.drafts.length;
  const patches = recorder == null ? null : recorder.patches;
  const inverse = recorder == null ? null : recorder.inverse;

  if (produced === undefined || produced === rootState.draft) {
    const result = finalize(scope, rootState.draft, patches == null ? null : [], patches, inverse);
    revokeScope(scope);
    return result;
  }

  if (rootState.modified) {
    revokeScope(scope);
    throw new Error(
      "@uniflowed/immer: a recipe returned a new value and also modified its draft; " +
        "do one or the other, because there is no correct way to combine them",
    );
  }
  const result = isDraftable(produced)
    ? finalize(scope, produced, null, patches, inverse)
    : produced;
  maybeFreeze(scope, result, false);
  if (patches != null && inverse != null) {
    recordReplacement(rootState.base, result, patches, inverse);
  }
  revokeScope(scope);
  return result;
}

/**
 * Turn a draft, or a value that may contain drafts, into what gets published.
 *
 * The three answers, in the order they are decided:
 *
 * - A draft from an outer `produce` is left alone. It is still live, and the
 *   call that made it will resolve it.
 * - An unmodified draft resolves to its base. This is structural sharing: the
 *   subtree in the result *is* the subtree that went in.
 * - A modified draft resolves to its copy, once its children have been
 *   resolved into it — children first, so a copy is never frozen or described
 *   by a patch while it still holds a proxy.
 */
function finalize(
  scope: Scope,
  value: mixed,
  path: null | $ReadOnlyArray<mixed>,
  patches: null | Array<Patch>,
  inverse: null | Array<Patch>,
): mixed {
  if (isFrozen(value) || !isDraftable(value)) {
    return value;
  }
  const state = stateOf(value);
  if (state == null) {
    // A container the recipe built itself. It is not a draft, but it may hold
    // drafts, and it has to be frozen before it is published.
    eachEntry(value, (key, child) => {
      finalizeProperty(scope, null, value, key, child, path, false, patches, inverse);
    });
    return value;
  }
  if (state.scope !== scope) {
    return value;
  }
  if (!state.modified) {
    maybeFreeze(scope, state.base, true);
    return state.base;
  }
  if (state.finalized) {
    return state.copy;
  }

  state.finalized = true;
  scope.pending -= 1;
  const result = state.copy;
  let walked = result;
  const intoSet = state.kind === "set";
  if (intoSet) {
    // A set is keyed by its members, so a member cannot be replaced in place.
    // The copy is emptied and refilled from a snapshot, which also keeps the
    // members in the order the recipe left them in.
    walked = new Set(asSet(result));
    asSet(result).clear();
  }
  eachEntry(walked, (key, child) => {
    finalizeProperty(scope, state, result, key, child, path, intoSet, patches, inverse);
  });
  maybeFreeze(scope, result, false);
  if (path != null && patches != null && inverse != null) {
    recordPatches(state, path, patches, inverse);
  }
  return state.copy;
}

function finalizeProperty(
  scope: Scope,
  parentState: null | DraftState,
  target: mixed,
  key: mixed,
  value: mixed,
  path: null | $ReadOnlyArray<mixed>,
  intoSet: boolean,
  patches: null | Array<Patch>,
  inverse: null | Array<Patch>,
): void {
  if (value === target) {
    throw new Error("@uniflowed/immer: a draft may not contain itself");
  }
  if (isDraft(value)) {
    // The path is carried down only into keys the recipe did *not* assign. An
    // assigned key already produces one patch describing the whole new value,
    // and a second set of patches for what changed inside it would apply the
    // same edit twice.
    // An array's own bookkeeping is keyed by the string the proxy's `set` trap
    // saw — `"0"` — while the walk above counts in numbers, so the two have to
    // be reconciled before they are compared. They stay apart in the path
    // itself, which uses numbers for array indices the way a patch reader
    // expects.
    const assignedKey = parentState != null && parentState.kind === "array" ? String(key) : key;
    const childPath =
      path != null &&
      parentState != null &&
      parentState.kind !== "set" &&
      !hasAssigned(parentState, assignedKey)
        ? [...path, key]
        : null;
    const finalized = finalize(scope, value, childPath, patches, inverse);
    setEntry(target, key, finalized);
    if (isDraft(finalized)) {
      // A live draft from an outer `produce` ended up in this result. Freezing
      // anything holding it would freeze a value the outer call still writes.
      scope.freezable = false;
    }
    return;
  }
  if (intoSet) {
    setEntry(target, key, value);
  }
  if (!isDraftable(value) || isFrozen(value)) {
    return;
  }
  // With every draft accounted for and freezing off, there is nothing left in
  // this subtree for the walk to find, so it stops here rather than descending
  // through data the recipe never touched.
  if (!isAutoFreeze() && scope.pending < 1) {
    return;
  }
  finalize(scope, value, null, patches, inverse);
  // Everything `eachEntry` hands over is reachable from the published value —
  // an object's enumerable string keys, a Map's entries, a Set's members — so
  // there is no such thing here as a child that is walked but not published,
  // and every one of them is frozen.
  maybeFreeze(scope, value, false);
}

/**
 * Freeze, unless something says not to yet.
 *
 * A nested `produce` freezes nothing: its result is going back into a draft
 * the outer call still owns, and the outer call will freeze the whole thing
 * once. `freezable` is the other brake, set when a live draft from an outer
 * scope has been finalized into this result.
 */
function maybeFreeze(scope: Scope, value: mixed, deep: boolean): void {
  if (scope.suspended == null && scope.freezable && isAutoFreeze()) {
    freeze(value, deep);
  }
}
