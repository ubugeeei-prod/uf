// @flow
//
// The store a form is, and the snapshots React reads from it.
//
// One mutable object per `useForm`, holding the values, the errors, the dirty
// and touched sets, the submission flags, the field records and the
// subscriptions. Nothing else in this package is mutable, and nothing outside
// this module reads a mutable field: everything React renders comes out of a
// snapshot function defined here.
//
// # Why an external store at all
//
// Because the values have to survive a keystroke without a render. That is the
// premise of an uncontrolled form: the text lives in the DOM, the value lives
// here, and React is told only when something a component actually asked about
// has changed. A `useState` per field would make every keystroke a render of
// the whole form, which is the cost this library exists to remove.
//
// React's supported way to render from state it does not own is
// `useSyncExternalStore`, and its contract is what every decision below answers
// to: `subscribe` must be stable, `getSnapshot` must be cheap and idempotent,
// and its result's *identity* must change exactly when the data does. Two
// consequences run through this file.
//
// First, the value tree is written immutably (`writeAt`, in `field-path.js`),
// so a watcher on `"address"` can compare one reference and know. Second, every
// snapshot is cached and compared before it is replaced: [`formState`] rebuilds
// its object, finds every field equal to the last one, and hands back the
// previous object. React's `useSyncExternalStore` bails out on an `Object.is`
// equal snapshot without rendering, so a keystroke that changes no form state
// costs ten comparisons rather than a render.
//
// # What that buys exactly, and what it does not
//
// A component reading `formState` re-renders once per *observable transition* —
// the first keystroke, which turns `isDirty` on, and a submit, which turns
// `isSubmitting` on and off again. It does not re-render per keystroke.
//
// It is not zero. React Hook Form reaches zero with a `Proxy` that records
// which `formState` keys a render read and narrows the subscription to those.
// That trick is unavailable here, and not because it is hard: recording a read
// during render is a side effect during render, it depends on the render
// happening at all, and the React Compiler is free to skip a render whose
// inputs did not change. So the arrangement is the one above, plus
// `useFormState`, which lets a component that only wants one field's error
// subscribe to that instead of to all of it.
//
// # The two writes that happen during render, and why they are safe
//
// `register(name, rules)` runs in the render body and records the field's rules
// here. `watch(name)` runs in the render body and adds `name` to the set of
// paths the owning component observes. Both write to an object that existed
// before the render — normally exactly the thing not to do.
//
// They are safe for three specific reasons, and would not be if any one of them
// stopped holding:
//
//  - Neither is part of a snapshot a component has already rendered. Rules are
//    read by validation, which runs in event handlers and effects. The observed
//    set decides which paths a *future* notification wakes; it never changes a
//    value a render already produced.
//  - Both are idempotent. Strict Mode renders twice and a concurrent render can
//    be thrown away; running either again records the same rules and adds a
//    string that is already in the set.
//  - A render the React Compiler skips is a render whose inputs did not change,
//    so the rules it would have recorded are already recorded and the paths it
//    would have observed are already observed. Skipping it is correct.
//
// # Why the owning form's watch subscription is a counter
//
// `watch("a")` has to add `"a"` to what the component observes, and it runs
// during the render that already read the snapshot. If the snapshot were the
// tuple of observed values, growing the set would change it and cost a second
// render on mount. So the form-level snapshot is a version number that moves
// only when an observed path changes, and `watch` returns the live value. That
// is still safe under `useSyncExternalStore`: React re-reads the snapshot after
// rendering and after committing, so a change between the read and the paint
// produces a new number and another render.
//
// # Why validation stays synchronous when it can
//
// `runRules` answers rather than promises unless the caller's own `validate` is
// async. A form in `onChange` mode that awaited every check would put a
// microtask between a keystroke and the error it cleared, and would have to
// flip `isValidating` on and off around it — two renders per keystroke, for a
// comparison that already had its answer. So `isValidating` becomes true only
// when something really is pending.
//
// # Why a slow answer cannot overwrite a fast one
//
// Every pass takes a sequence number and stamps the fields it is about to
// answer for. A result whose field has since been stamped by a newer pass is
// dropped. So a username check that takes 800ms cannot land on top of the error
// the next keystroke already produced — which is the bug every hand-rolled
// async validation has.

import type { FieldError, ValidationRules } from "../rules.js";
import { dependenciesOf, runRules, transformOf, whenSettled } from "../rules.js";
import type { Resolver, ResolverErrors, ResolverResult } from "../resolver.js";
import { errorsOf, runResolver } from "../resolver.js";
import type { FieldPath, FieldValues } from "./field-path.js";
import {
  cloneValues,
  indexUnder,
  isUnder,
  pathsOverlap,
  readAt,
  removeAt,
  sameValue,
  withIndexUnder,
  writeAt,
} from "./field-path.js";
import { focusElement, readElements, writeElements } from "./field-element.js";

/**
 * When a field is validated, before the form has ever been submitted.
 *
 * `onSubmit` is the default because it is the only one that never tells someone
 * their email address is invalid while they are halfway through typing it.
 * `onTouched` waits for the first blur and validates on every change after — a
 * field has to have been visited before it is allowed to complain.
 */
export type Mode = "onSubmit" | "onBlur" | "onChange" | "onTouched" | "all";

/** When a field is re-validated once the form has been submitted at least once. */
export type ReValidateMode = "onChange" | "onBlur" | "onSubmit";

/** Errors, dirty flags and touched flags, all keyed by field path. */
export type FieldErrors = { readonly [string]: FieldError, ... };
export type FieldFlags = { readonly [string]: boolean, ... };

/** Everything a form knows about itself that is not a value. */
export type FormState = {|
  readonly errors: FieldErrors,
  readonly isDirty: boolean,
  readonly dirtyFields: FieldFlags,
  readonly touchedFields: FieldFlags,
  readonly isSubmitting: boolean,
  readonly isSubmitted: boolean,
  readonly isSubmitSuccessful: boolean,
  readonly isValidating: boolean,
  readonly isValid: boolean,
  readonly submitCount: number,
|};

/**
 * What `setValue` does beyond writing the value.
 *
 * All three default to `false`, which is React Hook Form's choice and the right
 * one: a value the application put there is not a value the user changed, so it
 * does not make the form dirty, does not mark the field visited, and does not
 * make it start complaining. `useController` passes `shouldDirty`, because
 * there the write *is* the user typing.
 */
export type SetValueOptions = {|
  readonly shouldValidate?: boolean,
  readonly shouldDirty?: boolean,
  readonly shouldTouch?: boolean,
|};

/** What `reset` keeps rather than throwing away. */
export type ResetOptions = {|
  readonly keepValues?: boolean,
  readonly keepDefaultValues?: boolean,
  readonly keepErrors?: boolean,
  readonly keepDirty?: boolean,
  readonly keepTouched?: boolean,
  readonly keepSubmitCount?: boolean,
  readonly keepIsSubmitted?: boolean,
|};

/** What an imperative `watch` listener is told. */
export type WatchInfo = {|
  /** The path that changed, or `""` when the whole form was replaced. */
  readonly name: FieldPath,
  readonly type: "change" | "set" | "reset" | "array",
|};

type FieldRecord = {|
  elements: Array<mixed>,
  rules: ValidationRules,
|};

/** One row of a field array: its values, plus the key React identifies it by. */
export type FieldArrayRow = { readonly id: string, readonly [string]: mixed, ... };

type RowsCell = {|
  items: mixed,
  keys: $ReadOnlyArray<string>,
  snapshot: $ReadOnlyArray<FieldArrayRow>,
|};

type WatchCell = {|
  paths: $ReadOnlyArray<FieldPath>,
  values: Array<mixed>,
  snapshot: mixed,
  listeners: Set<() => void>,
|};

const NO_RULES: ValidationRules = Object.freeze({});
const NO_FLAGS: FieldFlags = Object.freeze({});
const NO_ERRORS: FieldErrors = Object.freeze({});

export type CreateStoreOptions<TValues extends FieldValues, TOutput> = {|
  readonly defaultValues: TValues,
  readonly mode: Mode,
  readonly reValidateMode: ReValidateMode,
  readonly resolver: Resolver<TValues, TOutput> | null,
  readonly context: mixed,
  readonly shouldFocusError: boolean,
|};

/**
 * The handle every hook in this package takes.
 *
 * Its members are this package's business, not an application's: pass it to
 * `useFieldArray`, `useWatch`, `useFormState` and `useController`, and read
 * nothing off it. It is a plain type rather than an opaque one because Flow's
 * opaque types are opaque to the rest of *this package* as well, and the way
 * around that — routing every operation through free functions here so the
 * other modules can see through it — would drag `useFieldArray`'s
 * implementation into this file in order to hide a type. The comment is the
 * weaker guarantee and it is the honest one.
 */
export type Control<TValues extends FieldValues, TOutput = TValues> = {|
  /** Phantom, never called: carries `TValues` so two forms are not one type. */
  readonly __values: () => TValues,
  readonly __output: () => TOutput,

  readonly getValues: () => TValues,
  readonly valueAt: (name: FieldPath) => mixed,
  readonly setValue: (name: FieldPath, value: mixed, options?: SetValueOptions) => void,
  readonly reset: (values?: TValues, options?: ResetOptions) => void,

  readonly rulesFor: (name: FieldPath, rules: ValidationRules) => void,
  readonly attach: (name: FieldPath, element: mixed) => void,
  readonly detach: (name: FieldPath, element: mixed) => void,
  readonly unregister: (names?: FieldPath | $ReadOnlyArray<FieldPath>) => void,
  readonly handleChange: (name: FieldPath) => void,
  readonly handleControlledChange: (name: FieldPath) => void,
  readonly handleBlur: (name: FieldPath) => void,
  readonly focus: (name: FieldPath, select?: boolean) => void,

  readonly errorAt: (name: FieldPath) => FieldError | void,
  readonly setError: (
    name: FieldPath,
    error: FieldError,
    options?: {| readonly shouldFocus?: boolean |},
  ) => void,
  readonly clearErrors: (names?: FieldPath | $ReadOnlyArray<FieldPath>) => void,
  readonly trigger: (names?: FieldPath | $ReadOnlyArray<FieldPath>) => Promise<boolean>,
  readonly primeValidity: () => void,

  readonly submitWith: (
    onValid: (values: TOutput, event?: mixed) => mixed,
    onInvalid?: (errors: FieldErrors, event?: mixed) => mixed,
  ) => (event?: mixed) => Promise<void>,

  readonly configure: (options: CreateStoreOptions<TValues, TOutput>) => void,

  readonly subscribeFormState: (listener: () => void) => () => void,
  readonly formState: () => FormState,
  readonly fieldStateSnapshot: (key: string, names: $ReadOnlyArray<FieldPath> | null) => FormState,

  readonly subscribeWatch: (
    key: string,
    paths: $ReadOnlyArray<FieldPath>,
    listener: () => void,
  ) => () => void,
  readonly watchSnapshot: (key: string, paths: $ReadOnlyArray<FieldPath>) => mixed,

  readonly observe: (name: FieldPath) => void,
  readonly subscribeObserved: (listener: () => void) => () => void,
  readonly observedVersion: () => number,

  readonly listen: (
    name: FieldPath | null,
    listener: (values: TValues, info: WatchInfo) => void,
  ) => () => void,

  readonly arrayRows: (name: FieldPath) => $ReadOnlyArray<FieldArrayRow>,
  readonly spliceArray: (
    name: FieldPath,
    start: number,
    remove: number,
    inserted: $ReadOnlyArray<mixed>,
  ) => void,
  readonly moveArray: (name: FieldPath, from: number, to: number) => void,
  readonly swapArray: (name: FieldPath, left: number, right: number) => void,
  readonly updateArray: (name: FieldPath, index: number, value: mixed) => void,
  readonly replaceArray: (name: FieldPath, items: $ReadOnlyArray<mixed>) => void,
|};

function isPromise(value: mixed): boolean {
  return value != null && typeof (value as $FlowFixMe).then === "function";
}

function noop(): void {}

/**
 * Re-raise a rejection from a validation nobody is awaiting.
 *
 * A `validate` that rejects is a bug in the caller's code. Swallowing it would
 * leave that field silently unvalidated for the rest of the session; re-raising
 * it outside the promise chain puts it where an uncaught error goes, which is
 * where somebody will see it.
 */
function reportAsyncFailure(error: mixed): void {
  queueMicrotask(() => {
    throw error;
  });
}

function toList(names: FieldPath | $ReadOnlyArray<FieldPath>): $ReadOnlyArray<FieldPath> {
  return typeof names === "string" ? [names] : names;
}

/**
 * Whether two flat maps hold the same keys pointing at the same values.
 *
 * Identity on the values is enough: everything compared here comes out of a
 * cached snapshot, so the `FieldError` for an unchanged field is the same
 * object it was last time.
 */
function sameShallow(
  left: { readonly [string]: mixed, ... },
  right: { readonly [string]: mixed, ... },
): boolean {
  if (left === right) {
    return true;
  }
  const leftKeys = Object.keys(left);
  const rightKeys = Object.keys(right);
  if (leftKeys.length !== rightKeys.length) {
    return false;
  }
  return leftKeys.every((key) => Object.is(left[key], right[key]));
}

/**
 * Whether a change to a field should be validated now.
 *
 * A pure function of the mode rather than a branch buried in the store, because
 * it is the piece of behaviour a reader is most likely to come looking for and
 * the piece most worth testing on its own.
 */
export function validatesOnChange(
  mode: Mode,
  reValidateMode: ReValidateMode,
  isSubmitted: boolean,
  isTouched: boolean,
): boolean {
  if (isSubmitted) {
    return reValidateMode === "onChange";
  }
  if (mode === "onChange" || mode === "all") {
    return true;
  }
  // `onTouched` means "not before the user has been in this field", so a change
  // counts only once a blur has happened.
  return mode === "onTouched" && isTouched;
}

/** Whether a blur should be validated now. */
export function validatesOnBlur(
  mode: Mode,
  reValidateMode: ReValidateMode,
  isSubmitted: boolean,
): boolean {
  if (isSubmitted) {
    return reValidateMode === "onBlur";
  }
  return mode === "onBlur" || mode === "onTouched" || mode === "all";
}

/**
 * Build the store that one `useForm` owns.
 *
 * Everything is closed over rather than held on `this` — the same shape
 * `@uniflowed/validator` uses, for the same reason: the returned object is the
 * whole interface, there is no second way in, and a member that is not on it
 * cannot be reached by anybody.
 */
export function createFormStore<TValues extends FieldValues, TOutput>(
  initial: CreateStoreOptions<TValues, TOutput>,
): Control<TValues, TOutput> {
  /**
   * The options as of the latest render.
   *
   * A store made once from the first render's options would keep the first
   * render's `resolver` and `context` forever, so a form whose schema depends
   * on a prop would validate against a stale one. `useForm` calls [`configure`]
   * from an effect after every render, which is before any event a user can
   * cause and after every render that could have changed them.
   */
  let settings: CreateStoreOptions<TValues, TOutput> = initial;

  function configure(next: CreateStoreOptions<TValues, TOutput>): void {
    settings = next;
  }

  let defaultValues: TValues = cloneValues(initial.defaultValues);
  let values: TValues = cloneValues(initial.defaultValues);

  const fields: Map<FieldPath, FieldRecord> = new Map();
  const errors: Map<FieldPath, FieldError> = new Map();
  const dirty: Set<FieldPath> = new Set();
  const touched: Set<FieldPath> = new Set();

  /**
   * The fields allowed to show an error.
   *
   * A form in `onChange` mode validates whatever it is asked to, but somebody
   * who has typed into the first field has not yet failed at the second one,
   * and showing both is how a form greets a user with four red messages. A
   * field joins this set when it is validated on its own account — changed,
   * blurred, triggered — or when a submit makes every field eligible at once.
   */
  const eligible: Set<FieldPath> = new Set();

  /** Each field's last verdict, for `isValid`. See [`refreshValidity`]. */
  const validity: Map<FieldPath, boolean> = new Map();

  let isSubmitting = false;
  let isSubmitted = false;
  let isSubmitSuccessful = false;
  let isValidating = false;
  let isValid = false;
  let submitCount = 0;

  let validationSeq = 0;
  const fieldSeq: Map<FieldPath, number> = new Map();
  let pendingValidations = 0;
  let resolvedOutput: TOutput | null = null;

  const formStateListeners: Set<() => void> = new Set();
  const watchCells: Map<string, WatchCell> = new Map();
  const observed: Set<FieldPath> = new Set();
  const observedListeners: Set<() => void> = new Set();
  let observedVersionCount = 0;
  const imperative: Set<{|
    readonly name: FieldPath | null,
    readonly listener: (values: TValues, info: WatchInfo) => void,
  |}> = new Set();

  const arrayKeys: Map<FieldPath, Array<string>> = new Map();
  const arrayFrozenKeys: Map<FieldPath, $ReadOnlyArray<string>> = new Map();
  const rowCells: Map<FieldPath, RowsCell> = new Map();
  let nextKey = 0;

  let formStateStale = true;
  let lastFormState: FormState | null = null;
  let errorsStale = true;
  let lastErrors: FieldErrors = NO_ERRORS;
  let dirtyStale = true;
  let lastDirty: FieldFlags = NO_FLAGS;
  let touchedStale = true;
  let lastTouched: FieldFlags = NO_FLAGS;
  const sliceCells: Map<string, FormState> = new Map();

  // ------------------------------------------------------------------ //
  // Snapshots
  // ------------------------------------------------------------------ //

  function frozenMap<TValue>(source: Map<FieldPath, TValue>): { readonly [string]: TValue, ... } {
    const out: { [string]: TValue, ... } = {};
    for (const [name, value] of source) {
      // `defineProperty` rather than assignment: a field can be called
      // `__proto__`, and assigning to that runs a setter instead of adding a
      // key. `@uniflowed/validator` guards its object parser the same way.
      Object.defineProperty(out, name, {
        value,
        writable: true,
        enumerable: true,
        configurable: true,
      });
    }
    return Object.freeze(out);
  }

  function errorsSnapshot(): FieldErrors {
    if (errorsStale) {
      errorsStale = false;
      lastErrors = errors.size === 0 ? NO_ERRORS : frozenMap(errors);
    }
    return lastErrors;
  }

  function flagsOf(source: Set<FieldPath>): FieldFlags {
    if (source.size === 0) {
      return NO_FLAGS;
    }
    const asMap: Map<FieldPath, boolean> = new Map();
    for (const name of source) {
      asMap.set(name, true);
    }
    return frozenMap(asMap);
  }

  function dirtySnapshot(): FieldFlags {
    if (dirtyStale) {
      dirtyStale = false;
      lastDirty = flagsOf(dirty);
    }
    return lastDirty;
  }

  function touchedSnapshot(): FieldFlags {
    if (touchedStale) {
      touchedStale = false;
      lastTouched = flagsOf(touched);
    }
    return lastTouched;
  }

  /**
   * The form state, as one immutable object whose identity survives a no-op.
   *
   * The comparison at the end is what makes a keystroke free. React re-checks
   * this on every notification and re-renders only when the reference moved, so
   * a change that leaves all ten of these alone costs one allocation and ten
   * comparisons instead of a render of the form.
   */
  function formState(): FormState {
    const previous = lastFormState;
    if (!formStateStale && previous != null) {
      return previous;
    }
    formStateStale = false;
    const next: FormState = {
      errors: errorsSnapshot(),
      isDirty: dirty.size > 0,
      dirtyFields: dirtySnapshot(),
      touchedFields: touchedSnapshot(),
      isSubmitting,
      isSubmitted,
      isSubmitSuccessful,
      isValidating,
      isValid,
      submitCount,
    };
    if (previous != null && sameFormState(previous, next)) {
      return previous;
    }
    lastFormState = next;
    return next;
  }

  function sameFormState(left: FormState, right: FormState): boolean {
    return (
      left.errors === right.errors &&
      left.dirtyFields === right.dirtyFields &&
      left.touchedFields === right.touchedFields &&
      left.isDirty === right.isDirty &&
      left.isSubmitting === right.isSubmitting &&
      left.isSubmitted === right.isSubmitted &&
      left.isSubmitSuccessful === right.isSubmitSuccessful &&
      left.isValidating === right.isValidating &&
      left.isValid === right.isValid &&
      left.submitCount === right.submitCount
    );
  }

  function invalidateFormState(): void {
    formStateStale = true;
    for (const listener of formStateListeners) {
      listener();
    }
  }

  function invalidateErrors(): void {
    errorsStale = true;
    invalidateFormState();
  }

  function subscribeFormState(listener: () => void): () => void {
    formStateListeners.add(listener);
    return () => {
      formStateListeners.delete(listener);
    };
  }

  /**
   * The form state as seen by a component that only cares about some fields.
   *
   * Every form-state listener is woken by every change — the notification is
   * cheap and broad. What makes `useFormState({ control, name })` narrow is
   * this: the slice is rebuilt, compared with the last one field by field, and
   * the previous object handed back when nothing in *that* slice moved. React's
   * `useSyncExternalStore` then bails out without rendering. So a form with
   * forty fields can put a message component next to each one and a keystroke
   * still renders only the one whose error changed.
   *
   * `names` of `null` means the whole form, and returns the shared snapshot
   * rather than a copy of it.
   */
  function fieldStateSnapshot(key: string, names: $ReadOnlyArray<FieldPath> | null): FormState {
    const whole = formState();
    if (names == null) {
      return whole;
    }
    const cell = sliceCells.get(key);
    const errorSlice = sliceOf(whole.errors, names);
    const dirtySlice = sliceOf(whole.dirtyFields, names);
    const next: FormState = {
      errors: errorSlice,
      // Scoped, like the maps beside them: in a slice, "dirty" means one of
      // *these* fields was changed and "valid" means none of *these* fields has
      // an error. Leaving them as the whole form's would undo the narrowing —
      // any field anywhere going wrong would move `isValid`, and every scoped
      // subscriber would render for it.
      isDirty: Object.keys(dirtySlice).length > 0,
      dirtyFields: dirtySlice,
      touchedFields: sliceOf(whole.touchedFields, names),
      isSubmitting: whole.isSubmitting,
      isSubmitted: whole.isSubmitted,
      isSubmitSuccessful: whole.isSubmitSuccessful,
      // Submission is about the form, not about a field, so these stay whole.
      isValidating: whole.isValidating,
      isValid: Object.keys(errorSlice).length === 0,
      submitCount: whole.submitCount,
    };
    if (cell != null && sameSlice(cell, next)) {
      return cell;
    }
    sliceCells.set(key, next);
    return next;
  }

  /** The entries of `source` at, or under, one of `names`. */
  function sliceOf<TValue>(
    source: { readonly [string]: TValue, ... },
    names: $ReadOnlyArray<FieldPath>,
  ): { readonly [string]: TValue, ... } {
    const kept: Map<FieldPath, TValue> = new Map();
    for (const path of Object.keys(source)) {
      if (names.some((name) => isUnder(path, name))) {
        kept.set(path, source[path]);
      }
    }
    return kept.size === 0 ? (NO_FLAGS as $FlowFixMe) : frozenMap(kept);
  }

  /**
   * Whether two slices say the same thing.
   *
   * The three maps are compared by content rather than by identity, because a
   * slice is rebuilt on every notification: comparing references would report a
   * change every time any field anywhere moved, which is the thing this is for.
   */
  function sameSlice(left: FormState, right: FormState): boolean {
    return (
      left.isDirty === right.isDirty &&
      left.isSubmitting === right.isSubmitting &&
      left.isSubmitted === right.isSubmitted &&
      left.isSubmitSuccessful === right.isSubmitSuccessful &&
      left.isValidating === right.isValidating &&
      left.isValid === right.isValid &&
      left.submitCount === right.submitCount &&
      sameShallow(left.errors, right.errors) &&
      sameShallow(left.dirtyFields, right.dirtyFields) &&
      sameShallow(left.touchedFields, right.touchedFields)
    );
  }

  // ------------------------------------------------------------------ //
  // Watching values
  // ------------------------------------------------------------------ //

  function shapeOf(collected: Array<mixed>, paths: $ReadOnlyArray<FieldPath>): mixed {
    // One path answers with the value itself, so `useWatch({ name: "a" })`
    // hands back what is at `a` rather than a one-element array. Several answer
    // with a frozen tuple, because a caller who mutates what they destructured
    // must not be able to change what the next comparison sees.
    return paths.length === 1 ? collected[0] : Object.freeze(collected.slice());
  }

  function cellFor(key: string, paths: $ReadOnlyArray<FieldPath>): WatchCell {
    let cell = watchCells.get(key);
    if (cell == null) {
      const collected = paths.map((path) => readAt(values, path));
      cell = {
        paths,
        values: collected,
        snapshot: shapeOf(collected, paths),
        listeners: new Set(),
      };
      watchCells.set(key, cell);
    }
    return cell;
  }

  /**
   * The values at `paths`, as something whose identity only moves when they do.
   *
   * The cache lives in the store rather than in a `useMemo`, and that is not an
   * optimisation. `useSyncExternalStore` calls `getSnapshot` during render and
   * compares the result with the previous one; a cache React is free to discard
   * would hand back an equal-but-new array, which reads as a change, which
   * renders, which discards the cache again.
   */
  function watchSnapshot(key: string, paths: $ReadOnlyArray<FieldPath>): mixed {
    const cell = cellFor(key, paths);
    const collected = paths.map((path) => readAt(values, path));
    let changed = collected.length !== cell.values.length;
    for (let at = 0; !changed && at < collected.length; at += 1) {
      changed = !Object.is(collected[at], cell.values[at]);
    }
    cell.paths = paths;
    if (changed) {
      cell.values = collected;
      cell.snapshot = shapeOf(collected, paths);
    }
    return cell.snapshot;
  }

  function subscribeWatch(
    key: string,
    paths: $ReadOnlyArray<FieldPath>,
    listener: () => void,
  ): () => void {
    const cell = cellFor(key, paths);
    cell.paths = paths;
    cell.listeners.add(listener);
    return () => {
      cell.listeners.delete(listener);
      if (cell.listeners.size === 0) {
        // A long-lived form with a field array would otherwise accumulate one
        // cell per path anybody ever watched.
        watchCells.delete(key);
      }
    };
  }

  /**
   * Add a path to the set the owning component watches.
   *
   * Called from `watch(name)` during render, and monotone on purpose: a
   * component that watches a path on only some renders keeps the subscription
   * on the others, which costs a comparison and never a wrong answer. Dropping
   * a path would mean knowing that a render had finished, and a render
   * finishing is not something a library is told about.
   */
  function observe(name: FieldPath): void {
    observed.add(name);
  }

  function observedVersion(): number {
    return observedVersionCount;
  }

  function subscribeObserved(listener: () => void): () => void {
    observedListeners.add(listener);
    return () => {
      observedListeners.delete(listener);
    };
  }

  function listen(
    name: FieldPath | null,
    listener: (values: TValues, info: WatchInfo) => void,
  ): () => void {
    const handle = { name, listener };
    imperative.add(handle);
    return () => {
      imperative.delete(handle);
    };
  }

  /** Wake every subscription a change at `path` is visible to. */
  function announce(path: FieldPath, type: WatchInfo["type"]): void {
    for (const cell of watchCells.values()) {
      if (cell.listeners.size === 0) {
        continue;
      }
      if (cell.paths.some((watched) => pathsOverlap(watched, path))) {
        for (const listener of cell.listeners) {
          listener();
        }
      }
    }

    if (observedListeners.size > 0) {
      for (const watched of observed) {
        if (pathsOverlap(watched, path)) {
          observedVersionCount += 1;
          for (const listener of observedListeners) {
            listener();
          }
          break;
        }
      }
    }

    if (imperative.size > 0) {
      const info: WatchInfo = { name: path, type };
      for (const handle of imperative) {
        if (handle.name == null || pathsOverlap(handle.name, path)) {
          handle.listener(values, info);
        }
      }
    }
  }

  // ------------------------------------------------------------------ //
  // Values
  // ------------------------------------------------------------------ //

  function getValues(): TValues {
    return values;
  }

  function valueAt(name: FieldPath): mixed {
    return name === "" ? values : readAt(values, name);
  }

  function updateDirty(name: FieldPath, next: mixed): void {
    const wasDirty = dirty.has(name);
    const nowDirty = !sameValue(next, readAt(defaultValues, name));
    if (wasDirty === nowDirty) {
      return;
    }
    if (nowDirty) {
      dirty.add(name);
    } else {
      dirty.delete(name);
    }
    dirtyStale = true;
    invalidateFormState();
  }

  function markTouched(name: FieldPath): void {
    if (touched.has(name)) {
      return;
    }
    touched.add(name);
    touchedStale = true;
    invalidateFormState();
  }

  function elementsOf(name: FieldPath): $ReadOnlyArray<mixed> {
    return fields.get(name)?.elements ?? [];
  }

  function setValue(name: FieldPath, next: mixed, setOptions?: SetValueOptions): void {
    values = writeAt(values, name, next);
    const elements = elementsOf(name);
    if (elements.length > 0) {
      // The DOM owns what the user sees. A store that changed its own copy and
      // stopped there would leave an uncontrolled input showing the old text.
      writeElements(elements, next);
    }
    if (setOptions?.shouldDirty === true) {
      updateDirty(name, next);
    }
    if (setOptions?.shouldTouch === true) {
      markTouched(name);
    }
    announce(name, "set");
    if (setOptions?.shouldValidate === true) {
      fireValidation([name]);
    }
  }

  // ------------------------------------------------------------------ //
  // Fields
  // ------------------------------------------------------------------ //

  function recordFor(name: FieldPath): FieldRecord {
    let record = fields.get(name);
    if (record == null) {
      record = { elements: [], rules: NO_RULES };
      fields.set(name, record);
    }
    return record;
  }

  /** Whether a rule set has anything to check. See [`refreshValidity`]. */
  function isTrivial(rules: ValidationRules): boolean {
    return (
      rules.required == null &&
      rules.min == null &&
      rules.max == null &&
      rules.minLength == null &&
      rules.maxLength == null &&
      rules.pattern == null &&
      rules.validate == null
    );
  }

  /** The first of the two writes that happen during render; see the module docs. */
  function rulesFor(name: FieldPath, rules: ValidationRules): void {
    const record = recordFor(name);
    record.rules = rules;
    if (isTrivial(rules)) {
      // A field with nothing to check is valid the moment it exists, which is
      // what keeps `isValid` honest for a form whose fields are mostly optional.
      validity.set(name, true);
    }
  }

  /**
   * A control mounted, so reconcile it with what the form holds.
   *
   * The order is the opposite of what it looks like. A field the form already
   * has a value for wins, and that value is written into the control. A field
   * the form knows nothing about adopts whatever the control already shows,
   * which is how a form written as markup — `defaultValue` on the input rather
   * than a `defaultValues` object — still has values before anybody types.
   */
  function attach(name: FieldPath, element: mixed): void {
    const record = recordFor(name);
    if (record.elements.includes(element)) {
      return;
    }
    record.elements.push(element);

    const known = readAt(values, name);
    if (known !== undefined) {
      writeElements([element], known);
      return;
    }
    const shown = readElements(record.elements, transformOf(record.rules));
    if (shown !== undefined && shown !== "" && shown !== null) {
      values = writeAt(values, name, shown);
      defaultValues = writeAt(defaultValues, name, shown);
    }
  }

  function detach(name: FieldPath, element: mixed): void {
    const record = fields.get(name);
    if (record == null) {
      return;
    }
    const at = record.elements.indexOf(element);
    if (at >= 0) {
      record.elements.splice(at, 1);
    }
  }

  /** Every field with a control in the document — the ones validation is about. */
  function liveNames(): Array<FieldPath> {
    const names = [];
    for (const [name, record] of fields) {
      if (record.elements.length > 0) {
        names.push(name);
      }
    }
    return names;
  }

  function unregister(names?: FieldPath | $ReadOnlyArray<FieldPath>): void {
    const targets = names == null ? Array.from(fields.keys()) : toList(names);
    for (const name of targets) {
      fields.delete(name);
      errors.delete(name);
      dirty.delete(name);
      touched.delete(name);
      eligible.delete(name);
      validity.delete(name);
      values = removeAt(values, name);
    }
    errorsStale = true;
    dirtyStale = true;
    touchedStale = true;
    invalidateFormState();
    announce("", "set");
  }

  function focus(name: FieldPath, select?: boolean): void {
    const element = elementsOf(name)[0];
    if (element != null) {
      focusElement(element, select === true);
    }
  }

  // ------------------------------------------------------------------ //
  // Events from the controls
  // ------------------------------------------------------------------ //

  function handleChange(name: FieldPath): void {
    const record = fields.get(name);
    if (record == null) {
      return;
    }
    const next = readElements(record.elements, transformOf(record.rules));
    values = writeAt(values, name, next);
    updateDirty(name, next);
    announce(name, "change");

    if (validatesOnChange(settings.mode, settings.reValidateMode, isSubmitted, touched.has(name))) {
      fireValidation(withDependencies(name, record.rules));
    }
  }

  /**
   * The same decision as [`handleChange`], for a field with no element.
   *
   * `useController` has already written the value — it was handed one rather
   * than having to read it off a control — so all that is left is the part that
   * depends on the mode. Splitting it out keeps `handleChange` about the DOM
   * and this about the rules, instead of one function with a flag.
   */
  function handleControlledChange(name: FieldPath): void {
    const record = fields.get(name);
    if (record == null) {
      return;
    }
    if (validatesOnChange(settings.mode, settings.reValidateMode, isSubmitted, touched.has(name))) {
      fireValidation(withDependencies(name, record.rules));
    }
  }

  function handleBlur(name: FieldPath): void {
    const record = fields.get(name);
    markTouched(name);
    if (record == null) {
      return;
    }
    if (validatesOnBlur(settings.mode, settings.reValidateMode, isSubmitted)) {
      fireValidation(withDependencies(name, record.rules));
    }
  }

  /**
   * The field, plus the fields whose errors depend on it.
   *
   * `deps` is what makes a "must match the password" field stop complaining the
   * moment the password it must match is corrected, rather than at its own next
   * blur.
   */
  function withDependencies(name: FieldPath, rules: ValidationRules): $ReadOnlyArray<FieldPath> {
    const deps = dependenciesOf(rules);
    return deps.length === 0 ? [name] : [name, ...deps];
  }

  function fireValidation(names: $ReadOnlyArray<FieldPath>): void {
    for (const name of names) {
      eligible.add(name);
    }
    const outcome = validateNames(names);
    if (isPromise(outcome)) {
      (outcome as $FlowFixMe).then(noop, reportAsyncFailure);
    }
  }

  // ------------------------------------------------------------------ //
  // Errors
  // ------------------------------------------------------------------ //

  function errorAt(name: FieldPath): FieldError | void {
    return errors.get(name);
  }

  function setError(
    name: FieldPath,
    error: FieldError,
    errorOptions?: {| readonly shouldFocus?: boolean |},
  ): void {
    errors.set(name, error);
    invalidateErrors();
    if (errorOptions?.shouldFocus === true) {
      focus(name);
    }
  }

  function clearErrors(names?: FieldPath | $ReadOnlyArray<FieldPath>): void {
    if (names == null) {
      if (errors.size === 0) {
        return;
      }
      errors.clear();
    } else {
      let removed = false;
      for (const name of toList(names)) {
        removed = errors.delete(name) || removed;
      }
      if (!removed) {
        return;
      }
    }
    invalidateErrors();
  }

  // ------------------------------------------------------------------ //
  // Validation
  // ------------------------------------------------------------------ //

  function beginValidating(): void {
    pendingValidations += 1;
    if (pendingValidations === 1) {
      isValidating = true;
      invalidateFormState();
    }
  }

  function endValidating(): void {
    pendingValidations = Math.max(0, pendingValidations - 1);
    if (pendingValidations === 0 && isValidating) {
      isValidating = false;
      invalidateFormState();
    }
  }

  /**
   * Run the checks for `names`, publish what the form may show, and answer.
   *
   * Publishing is scoped to [`eligible`] rather than to `names`, so a pass
   * triggered by one field also refreshes the errors already on screen.
   * Synchronous when nothing awaited, which is what the `whenSettled` shape
   * running through this file is for.
   */
  function validateNames(names: $ReadOnlyArray<FieldPath> | null): boolean | Promise<boolean> {
    const targets = names == null ? liveNames() : names;
    validationSeq += 1;
    const seq = validationSeq;
    for (const name of targets) {
      fieldSeq.set(name, seq);
    }

    const collected = settings.resolver == null ? collectFromRules(targets) : collectFromResolver();
    const waiting = isPromise(collected);
    if (waiting) {
      beginValidating();
    }
    // Written out for the same reason as in `collectFromResolver`: what
    // `collected` holds is already `TValue | Promise<TValue>`, so inference
    // takes the whole union for `TValue`.
    return whenSettled<Map<FieldPath, FieldError>, boolean>(collected, (found) => {
      if (waiting) {
        endValidating();
      }
      publish(found, targets, seq);
      return names == null ? found.size === 0 : targets.every((name) => !found.has(name));
    }) as $FlowFixMe;
  }

  /**
   * The resolver's verdict on the whole form.
   *
   * A resolver is always given every value, because a schema is about the whole
   * value: there is no way to ask `object({ ... })` about one field, and a rule
   * like "these two dates must be in order" is not a property of either of them.
   */
  function collectFromResolver(): Map<FieldPath, FieldError> | Promise<Map<FieldPath, FieldError>> {
    const resolver = settings.resolver;
    if (resolver == null) {
      return new Map();
    }
    // The type arguments are written out because Flow cannot infer them here:
    // `whenSettled` takes `TValue | Promise<TValue>`, and `runResolver`
    // returns exactly that shape, so inference picks the whole union as
    // `TValue` and hands the callback a value that might still be a promise.
    // Saying which is which costs one line and removes four errors that were
    // never about this code.
    return whenSettled<ResolverResult<TOutput>, Map<FieldPath, FieldError>>(
      runResolver(resolver, values, settings.context),
      (result) => {
        const found: Map<FieldPath, FieldError> = new Map();
        const reported: ResolverErrors = errorsOf(result);
        for (const name of Object.keys(reported)) {
          found.set(name, reported[name]);
        }
        resolvedOutput = found.size === 0 ? (result.values as $FlowFixMe) : null;
        // A resolver answers for the whole form at once, so its verdict is the
        // whole answer to `isValid` — no per-field bookkeeping needed.
        isValid = found.size === 0;
        return found;
      },
    ) as $FlowFixMe;
  }

  /**
   * Each field's own rules, in order, without awaiting the ones that answer now.
   *
   * The loop runs while the answers are synchronous and becomes a promise chain
   * only at the first check that is not. Recursing per field instead would put
   * a stack frame per field on a form that has thousands of them.
   */
  function collectFromRules(
    targets: $ReadOnlyArray<FieldPath>,
  ): Map<FieldPath, FieldError> | Promise<Map<FieldPath, FieldError>> {
    return checkFrom(targets, 0, new Map());
  }

  function checkFrom(
    targets: $ReadOnlyArray<FieldPath>,
    from: number,
    found: Map<FieldPath, FieldError>,
  ): Map<FieldPath, FieldError> | Promise<Map<FieldPath, FieldError>> {
    let at = from;
    while (at < targets.length) {
      const name = targets[at];
      const record = fields.get(name);
      if (record == null) {
        at += 1;
        continue;
      }
      const outcome = runRules(record.rules, readAt(values, name), values);
      if (isPromise(outcome)) {
        const here = at;
        return (outcome as $FlowFixMe).then((error: FieldError | null) => {
          recordVerdict(targets[here], error, found);
          return checkFrom(targets, here + 1, found);
        });
      }
      recordVerdict(name, outcome as $FlowFixMe, found);
      at += 1;
    }
    refreshValidity();
    return found;
  }

  function recordVerdict(
    name: FieldPath,
    error: FieldError | null,
    found: Map<FieldPath, FieldError>,
  ): void {
    if (error != null) {
      found.set(name, error);
    }
    validity.set(name, error == null);
  }

  /**
   * `isValid` for a form checked by rules rather than by a schema.
   *
   * A schema answers for everything at once; rules do not, and running every
   * field's rules on every keystroke would fire an async `validate` belonging to
   * a field nobody touched — a uniqueness request per character typed somewhere
   * else. So each field's last verdict is remembered and `isValid` is their
   * conjunction. A field that has never been checked leaves the form invalid
   * until it is, and `useForm` primes them all once on mount in the eager modes.
   */
  function refreshValidity(): void {
    let ok = true;
    for (const name of liveNames()) {
      if (validity.get(name) !== true) {
        ok = false;
        break;
      }
    }
    isValid = ok;
  }

  function publish(
    found: Map<FieldPath, FieldError>,
    targets: $ReadOnlyArray<FieldPath>,
    seq: number,
  ): void {
    const names: Set<FieldPath> = new Set(targets);
    for (const name of found.keys()) {
      names.add(name);
    }

    let changed = false;
    for (const name of names) {
      if ((fieldSeq.get(name) ?? 0) > seq) {
        // A newer pass has already answered for this field. Dropping the older
        // answer is the whole of the stale-result rule.
        continue;
      }
      // A path that is not a registered field can only have come from a
      // resolver or from `setError`, and no interaction would ever make it
      // "eligible" — so it always is.
      if (fields.has(name) && !eligible.has(name)) {
        continue;
      }
      const error = found.get(name);
      const current = errors.get(name);
      if (error == null) {
        changed = errors.delete(name) || changed;
      } else if (current?.type !== error.type || current?.message !== error.message) {
        errors.set(name, error);
        changed = true;
      }
    }

    if (changed) {
      invalidateErrors();
    } else {
      // `isValid` and `isValidating` may still have moved.
      invalidateFormState();
    }
  }

  function trigger(names?: FieldPath | $ReadOnlyArray<FieldPath>): Promise<boolean> {
    const targets = names == null ? null : toList(names);
    for (const name of targets ?? liveNames()) {
      eligible.add(name);
    }
    return Promise.resolve(validateNames(targets));
  }

  /**
   * Check everything once without showing anything, to seed `isValid`.
   *
   * Called from an effect on mount in the eager modes, and deliberately not in
   * `onSubmit` mode: there, running every `validate` before the user has done
   * anything would mean a form that fires its server-side checks on page load.
   */
  function primeValidity(): void {
    const collected =
      settings.resolver == null ? collectFromRules(liveNames()) : collectFromResolver();
    if (isPromise(collected)) {
      (collected as $FlowFixMe).then(() => {
        invalidateFormState();
      }, reportAsyncFailure);
      return;
    }
    invalidateFormState();
  }

  // ------------------------------------------------------------------ //
  // Submitting
  // ------------------------------------------------------------------ //

  /**
   * The handler a `<form onSubmit>` gets.
   *
   * `isSubmitting` is set before the first await and cleared in a `finally`, so
   * a submit that throws leaves the button enabled rather than the form stuck
   * forever. The error is not swallowed: a library that ate it would turn a
   * failed request into a form that quietly did nothing, and the caller's own
   * `try`/`catch` is where that decision belongs.
   */
  function submitWith(
    onValid: (values: TOutput, event?: mixed) => mixed,
    onInvalid?: (errors: FieldErrors, event?: mixed) => mixed,
  ): (event?: mixed) => Promise<void> {
    return async (event?: mixed) => {
      const nativeEvent: $FlowFixMe = event;
      if (nativeEvent != null && typeof nativeEvent.preventDefault === "function") {
        nativeEvent.preventDefault();
        nativeEvent.stopPropagation?.();
      }

      isSubmitting = true;
      isSubmitSuccessful = false;
      invalidateFormState();

      try {
        // Everything becomes eligible: a submit is the moment a form is allowed
        // to say all of what is wrong with it at once.
        for (const name of liveNames()) {
          eligible.add(name);
        }
        // What a server told the previous attempt is not evidence about this
        // one, and nothing will re-check a path no field is registered at.
        // Clearing those here is what stops `setError("root", ...)` from
        // blocking every later submit.
        for (const name of Array.from(errors.keys())) {
          if (!fields.has(name)) {
            errors.delete(name);
            errorsStale = true;
          }
        }

        resolvedOutput = null;
        const ok = await Promise.resolve(validateNames(null));

        if (!ok || errors.size > 0) {
          if (settings.shouldFocusError) {
            focusFirstError();
          }
          if (onInvalid != null) {
            await onInvalid(errorsSnapshot(), event);
          }
          return;
        }

        // With a resolver, `onValid` receives the resolver's *output* —
        // `{ age: 42 }` where the form held `{ age: "42" }` — so a schema that
        // coerces is not re-run by hand at the submit boundary.
        const output: TOutput =
          resolvedOutput == null ? (values as $FlowFixMe) : (resolvedOutput as $FlowFixMe);
        await onValid(output, event);
        isSubmitSuccessful = true;
      } finally {
        isSubmitting = false;
        isSubmitted = true;
        submitCount += 1;
        invalidateFormState();
      }
    };
  }

  /** Focus the first field, in registration order, that has an error. */
  function focusFirstError(): void {
    for (const [name, record] of fields) {
      if (errors.has(name) && record.elements.length > 0) {
        focusElement(record.elements[0], true);
        return;
      }
    }
  }

  // ------------------------------------------------------------------ //
  // Resetting
  // ------------------------------------------------------------------ //

  function reset(nextValues?: TValues, resetOptions?: ResetOptions): void {
    if (nextValues != null && resetOptions?.keepDefaultValues !== true) {
      defaultValues = cloneValues(nextValues);
    }
    if (resetOptions?.keepValues !== true) {
      values = cloneValues(nextValues ?? defaultValues);
      for (const [name, record] of fields) {
        if (record.elements.length > 0) {
          writeElements(record.elements, readAt(values, name));
        }
      }
      arrayKeys.clear();
      arrayFrozenKeys.clear();
      rowCells.clear();
    }
    if (resetOptions?.keepErrors !== true) {
      errors.clear();
      eligible.clear();
      errorsStale = true;
    }
    if (resetOptions?.keepDirty !== true) {
      dirty.clear();
      dirtyStale = true;
    }
    if (resetOptions?.keepTouched !== true) {
      touched.clear();
      touchedStale = true;
    }
    if (resetOptions?.keepSubmitCount !== true) {
      submitCount = 0;
    }
    if (resetOptions?.keepIsSubmitted !== true) {
      isSubmitted = false;
      isSubmitSuccessful = false;
    }
    isSubmitting = false;
    validity.clear();
    for (const [name, record] of fields) {
      if (isTrivial(record.rules)) {
        validity.set(name, true);
      }
    }
    refreshValidity();
    invalidateFormState();
    announce("", "reset");
  }

  // ------------------------------------------------------------------ //
  // Field arrays
  // ------------------------------------------------------------------ //

  function currentArray(name: FieldPath): Array<mixed> {
    const held = readAt(values, name);
    return Array.isArray(held) ? held.slice() : [];
  }

  function makeKey(): string {
    nextKey += 1;
    return `uf-${nextKey}`;
  }

  /** Replace an array's keys, and the frozen copy [`arrayRows`] compares. */
  function setKeys(name: FieldPath, keys: Array<string>): void {
    arrayKeys.set(name, keys);
    arrayFrozenKeys.set(name, Object.freeze(keys.slice()));
  }

  /**
   * The keys `useFieldArray` renders rows with.
   *
   * Made once per row and carried through every operation, never derived from
   * the index. That is the whole contract of the hook: React identifies a row by
   * its key, so removing the middle of three rows must leave the first and last
   * with the keys they had. Renumber them and React reuses the wrong DOM node —
   * and because the inputs are uncontrolled, the row that stayed keeps the text
   * of the row that went.
   */
  function keysOf(name: FieldPath): $ReadOnlyArray<string> {
    let keys = arrayKeys.get(name);
    const length = currentArray(name).length;
    if (keys == null) {
      keys = [];
      arrayKeys.set(name, keys);
    }
    let moved = false;
    while (keys.length < length) {
      keys.push(makeKey());
      moved = true;
    }
    if (keys.length > length) {
      keys.length = length;
      moved = true;
    }
    let frozen = arrayFrozenKeys.get(name);
    if (moved || frozen == null) {
      frozen = Object.freeze(keys.slice());
      arrayFrozenKeys.set(name, frozen);
    }
    return frozen;
  }

  /**
   * The rows of the array at `name`, as `useFieldArray` renders them.
   *
   * Cached here, and emphatically not in a `useMemo` in the hook. The React
   * Compiler — which uf runs over every `component` and `hook` — memoises a
   * value whose dependencies it can see do not change, and a read of this store
   * during render is not one of them: it is a call into an object the compiler
   * has no reason to think is reactive. A `useMemo` over `keysOf(name)`
   * therefore held the keys from the first render for ever, and a field array
   * that had a row removed from the middle rendered the surviving rows under
   * the removed row's keys.
   *
   * That is not a compiler bug and it is not worked around: the rule it
   * enforces is the right one, and the answer is that anything a render reads
   * out of this store must come through `useSyncExternalStore`, whose whole
   * job is to tell React when a store it does not own has changed. So the cache
   * is here, keyed on the two things it depends on, and the hook subscribes.
   */
  function arrayRows(name: FieldPath): $ReadOnlyArray<FieldArrayRow> {
    const items = readAt(values, name);
    const keys = keysOf(name);
    const cell = rowCells.get(name);
    if (cell != null && cell.items === items && cell.keys === keys) {
      return cell.snapshot;
    }
    const list = Array.isArray(items) ? items : [];
    const snapshot = Object.freeze(
      list.map((item, at) => {
        const row: { [string]: mixed, ... } =
          item != null && typeof item === "object" && !Array.isArray(item)
            ? { ...(item as $FlowFixMe) }
            : { value: item };
        // `id` last, so a row that happens to carry its own `id` does not
        // quietly replace the key React is about to identify it by.
        row.id = keys[at];
        return Object.freeze(row) as $FlowFixMe;
      }),
    );
    rowCells.set(name, { items, keys, snapshot });
    return snapshot;
  }

  /**
   * Rewrite every path-keyed fact about the rows of `name`.
   *
   * Errors, dirty flags and touched flags are keyed by path, so a row moving
   * from index 2 to index 1 is a rename of `items.2.*` to `items.1.*` rather
   * than a walk of anything. `mapping` gives the row's new index, or `null` for
   * a row that is gone.
   */
  function remapUnder(name: FieldPath, mapping: (index: number) => number | null): void {
    remapKeyed(errors, name, mapping);
    remapKeyed(validity, name, mapping);
    remapKeyed(fieldSeq, name, mapping);
    remapSet(dirty, name, mapping);
    remapSet(touched, name, mapping);
    remapSet(eligible, name, mapping);
    for (const path of Array.from(fields.keys())) {
      const index = indexUnder(path, name);
      if (index != null && mapping(index) == null) {
        fields.delete(path);
      }
    }
    errorsStale = true;
    dirtyStale = true;
    touchedStale = true;
  }

  function remapKeyed<TValue>(
    source: Map<FieldPath, TValue>,
    name: FieldPath,
    mapping: (index: number) => number | null,
  ): void {
    const moved: Array<[FieldPath, TValue]> = [];
    for (const [path, value] of Array.from(source)) {
      const index = indexUnder(path, name);
      if (index == null) {
        continue;
      }
      source.delete(path);
      const next = mapping(index);
      if (next != null) {
        moved.push([withIndexUnder(path, name, next), value]);
      }
    }
    for (const [path, value] of moved) {
      source.set(path, value);
    }
  }

  function remapSet(
    source: Set<FieldPath>,
    name: FieldPath,
    mapping: (index: number) => number | null,
  ): void {
    const moved: Array<FieldPath> = [];
    for (const path of Array.from(source)) {
      const index = indexUnder(path, name);
      if (index == null) {
        continue;
      }
      source.delete(path);
      const next = mapping(index);
      if (next != null) {
        moved.push(withIndexUnder(path, name, next));
      }
    }
    for (const path of moved) {
      source.add(path);
    }
  }

  /**
   * Finish an array operation: store the values, sync the controls, announce.
   *
   * The controls have to be written to here, and it is not obvious why. A row
   * that *moved* gets a new `name` on the next render, so its ref is detached
   * and re-attached and [`attach`] writes the value in. A row that stayed where
   * it is — the one `update` replaced, the rows before a splice point — keeps
   * its name and its ref, so React does not touch it, and an uncontrolled input
   * would go on showing the old text with the new value behind it. Writing
   * every mounted control under the array closes that gap; the rows that do
   * move are written twice, once here and once when they re-attach, and the
   * second write is the one that is right for them.
   */
  /**
   * Forget what was known about one row without forgetting the row.
   *
   * The narrow half of [`remapUnder`], for `update`: the values are new, so the
   * errors and the dirty and touched flags are about something that is no
   * longer there — but the row is in the same place, with the same key and the
   * same mounted controls.
   */
  function forgetRow(name: FieldPath, index: number): void {
    const prefix = `${name}.${index}`;
    for (const path of Array.from(errors.keys())) {
      if (isUnder(path, prefix)) {
        errors.delete(path);
      }
    }
    for (const source of [validity, fieldSeq]) {
      for (const path of Array.from(source.keys())) {
        if (isUnder(path, prefix)) {
          source.delete(path);
        }
      }
    }
    for (const source of [dirty, touched, eligible]) {
      for (const path of Array.from(source)) {
        if (isUnder(path, prefix)) {
          source.delete(path);
        }
      }
    }
    errorsStale = true;
    dirtyStale = true;
    touchedStale = true;
  }

  function afterArrayChange(name: FieldPath, next: Array<mixed>): void {
    values = writeAt(values, name, next);
    for (const [path, record] of fields) {
      if (record.elements.length > 0 && isUnder(path, name)) {
        writeElements(record.elements, readAt(values, path));
      }
    }
    if (sameValue(next, readAt(defaultValues, name))) {
      dirty.delete(name);
    } else {
      dirty.add(name);
    }
    dirtyStale = true;
    invalidateFormState();
    announce(name, "array");
  }

  function spliceArray(
    name: FieldPath,
    start: number,
    remove: number,
    inserted: $ReadOnlyArray<mixed>,
  ): void {
    const next = currentArray(name);
    const keys = Array.from(keysOf(name));
    const at = Math.max(0, Math.min(start, next.length));
    next.splice(at, remove, ...inserted);
    keys.splice(at, remove, ...inserted.map(() => makeKey()));
    setKeys(name, keys);
    remapUnder(name, (index) => {
      if (index < at) {
        return index;
      }
      if (index < at + remove) {
        return null;
      }
      return index - remove + inserted.length;
    });
    afterArrayChange(name, next);
  }

  function moveArray(name: FieldPath, from: number, to: number): void {
    const next = currentArray(name);
    const keys = Array.from(keysOf(name));
    if (from < 0 || to < 0 || from >= next.length || to >= next.length) {
      return;
    }
    next.splice(to, 0, ...next.splice(from, 1));
    keys.splice(to, 0, ...keys.splice(from, 1));
    setKeys(name, keys);
    remapUnder(name, (index) => {
      if (index === from) {
        return to;
      }
      if (from < to) {
        return index > from && index <= to ? index - 1 : index;
      }
      return index >= to && index < from ? index + 1 : index;
    });
    afterArrayChange(name, next);
  }

  function swapArray(name: FieldPath, left: number, right: number): void {
    const next = currentArray(name);
    const keys = Array.from(keysOf(name));
    if (left < 0 || right < 0 || left >= next.length || right >= next.length) {
      return;
    }
    const heldValue = next[left];
    next[left] = next[right];
    next[right] = heldValue;
    const heldKey = keys[left];
    keys[left] = keys[right];
    keys[right] = heldKey;
    setKeys(name, keys);
    remapUnder(name, (index) => (index === left ? right : index === right ? left : index));
    afterArrayChange(name, next);
  }

  function updateArray(name: FieldPath, index: number, value: mixed): void {
    const next = currentArray(name);
    if (index < 0 || index >= next.length) {
      return;
    }
    next[index] = value;
    // The row's contents are replaced, so what was wrong with the old contents
    // is not news about the new ones — but the row keeps its key, because it is
    // still the same row, and it keeps its field records, because its controls
    // are still mounted under the same names. Deleting those would unregister
    // inputs that nothing is going to re-attach.
    forgetRow(name, index);
    afterArrayChange(name, next);
  }

  function replaceArray(name: FieldPath, items: $ReadOnlyArray<mixed>): void {
    setKeys(
      name,
      items.map(() => makeKey()),
    );
    remapUnder(name, () => null);
    afterArrayChange(name, items.slice());
  }

  // ------------------------------------------------------------------ //

  return {
    __values: () => values,
    __output: () => values as $FlowFixMe,
    getValues,
    valueAt,
    setValue,
    reset,
    rulesFor,
    attach,
    detach,
    unregister,
    handleChange,
    handleControlledChange,
    handleBlur,
    focus,
    errorAt,
    setError,
    clearErrors,
    trigger,
    primeValidity,
    submitWith,
    configure,
    subscribeFormState,
    formState,
    fieldStateSnapshot,
    subscribeWatch,
    watchSnapshot,
    observe,
    subscribeObserved,
    observedVersion,
    listen,
    arrayRows,
    spliceArray,
    moveArray,
    swapArray,
    updateArray,
    replaceArray,
  };
}
