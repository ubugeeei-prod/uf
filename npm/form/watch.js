// @flow
//
// `@uniflowed/form/watch`: subscribing to a part of a form.
//
// Two hooks, and between them they are the reason a large form stays fast.
// `useForm` is called once, at the top; these are called wherever a value or an
// error is actually rendered, and each subscribes only that component to only
// what it named.
//
//   component Total(control: Control<Order>) {
//     const items = useWatch({ control, name: "items" });
//     return <output>{total(items)}</output>;
//   }
//
//   component Message(control: Control<Order>, name: string) {
//     const { errors } = useFormState({ control, name });
//     return errors[name] == null ? null : <p role="alert">{errors[name].message}</p>;
//   }
//
// Typing in `items.3.price` re-renders `Total` and the `Message` for that
// field. It does not re-render the form, the other rows, or the other messages.
//
// # Why these are hooks and `watch` is a method
//
// Because a subscription is a hook's job. `useSyncExternalStore` has to be
// called unconditionally at the top of a component, so a component that wants
// its own narrow subscription has to ask for it in its own body — which is what
// these are. `useForm().watch` exists for the component that already owns the
// form, where there is nowhere narrower to put the subscription.
//
// # Why the subscription is keyed by the path string
//
// `name` arrives as a prop, and `["a", "b"]` written inline is a different
// array on every render — so the cache cannot be keyed by its identity, and a
// `useMemo` cannot be relied on to hold one either: React may drop a memo, and
// a dropped memo here would mean a new snapshot, which reads as a change, which
// renders, which drops the memo again. Joining the names into a string gives a
// key that is equal whenever the request is equal, and the store keeps the
// cached snapshot under it.

import { useCallback, useMemo, useSyncExternalStore } from "@uniflowed/react";

import type { FieldPath, FieldSegments, FieldValues, ValueAtPath } from "./internal/field-path.js";
import { pathOf } from "./internal/field-path.js";
import type { Control, FormState } from "./internal/form-store.js";

/** Joins several names into one cache key. A field path never contains a NUL. */
const KEY_SEPARATOR = "\u0000";

function keyOf(name: FieldPath | $ReadOnlyArray<FieldPath> | void): string {
  if (name == null) {
    return "";
  }
  return typeof name === "string" ? name : name.join(KEY_SEPARATOR);
}

export type UseWatchOptions<TValues extends FieldValues, TOutput, TPath> = {|
  readonly control: Control<TValues, TOutput>,
  /**
   * One path, several, or none for the whole form.
   *
   * Watching `"items"` also wakes on `"items.0.name"`: a change under a path is
   * a change to it, which is what makes watching a field array's root work.
   */
  readonly name?: FieldPath | $ReadOnlyArray<FieldPath>,
  /**
   * The same field, addressed by its segments — and typed.
   *
   * `path: ["address", "city"]` is `string` where `name: "address.city"` is
   * `mixed`, and a segment that is not a key is refused. It is a separate
   * option rather than a shape `name` also accepts because `name: ["a", "b"]`
   * already means the two fields `a` and `b`, and that meaning is not
   * available to be reinterpreted.
   *
   * Pass one or the other. `path` wins if both are given, which is a
   * tiebreaker rather than a feature: passing both is a mistake, and the typed
   * one is the one that was meant.
   */
  readonly path?: TPath,
  /** Used while the value at the field is `undefined`. */
  readonly defaultValue?: mixed,
|};

/**
 * The value at a path, re-rendering this component when it changes.
 *
 * One name answers with the value; several answer with a frozen tuple in the
 * order they were asked for; none answers with the whole values object, which
 * changes identity on every write and so re-renders on every one. A `path`
 * answers with the value *at its type* — see [`ValueAtPath`].
 *
 * # Why `path` is typed differently from `getValues`'s segments
 *
 * `getValues` is a field on an object, so its type can be an intersection of
 * one signature per path length, and each of those bounds its segments by the
 * keys of what the segment before it landed on. A hook is a declaration, and a
 * declaration has one signature: there is nowhere to put the other three.
 *
 * So `path` is one generic tuple and the value is computed from it by
 * [`ValueAtPath`], which is a conditional type rather than a bound. The type it
 * produces is the same. What differs is *when* a misspelt segment is reported:
 * a bound is checked at the call whatever the result is used for, and a
 * conditional is evaluated against the type the result is wanted at — so
 * `const city: string = useWatch({ control, path: ["address", "cty"] })` is an
 * error and `const city: mixed = ...` is not. Both are better than `mixed`
 * everywhere, and the difference is written down here rather than found.
 */
export hook useWatch<TValues extends FieldValues, TOutput, TPath extends FieldSegments = []>(
  options: UseWatchOptions<TValues, TOutput, TPath>,
): ValueAtPath<TValues, TPath> {
  const control = options.control;
  const path = options.path;
  const key = path == null ? keyOf(options.name) : pathOf(path);
  const defaultValue = options.defaultValue;

  // Split back out of the key rather than kept from the caller's array, so an
  // inline `["a", "b"]` does not re-subscribe on every render.
  const paths = useMemo(() => key.split(KEY_SEPARATOR), [key]);

  const subscribe = useCallback(
    (listener: () => void) => control.subscribeWatch(key, paths, listener),
    [control, key, paths],
  );
  const snapshot = useCallback(() => control.watchSnapshot(key, paths), [control, key, paths]);

  const value = useSyncExternalStore(subscribe, snapshot, snapshot);
  // The one cast in this file, and it is the seam between a store keyed by
  // strings and a signature that answers at a type. `watchSnapshot` reads a
  // value tree it has no type for — the store is one object with `mixed` at
  // every leaf — and `ValueAtPath` is the caller's own values object indexed by
  // the segments they passed. The two agree because `pathOf` above turned those
  // segments into the very key the snapshot was read at.
  return (value === undefined ? defaultValue : value) as $FlowFixMe;
}

export type UseFormStateOptions<TValues extends FieldValues, TOutput> = {|
  readonly control: Control<TValues, TOutput>,
  /**
   * Narrow the subscription to these fields.
   *
   * Without it the component re-renders on any form-state change; with it, only
   * when something about these fields — their errors, their dirty or touched
   * flags — or the submission state moves. The submission flags are always
   * included, because `isSubmitting` is not about any one field and a message
   * component that could not see it would be useless during a submit.
   */
  readonly name?: FieldPath | $ReadOnlyArray<FieldPath>,
|};

/**
 * A form's state, subscribed from wherever it is rendered.
 *
 * The same object `useForm().formState` gives, but read in *this* component, so
 * a change re-renders this component instead of the whole form. That is the
 * whole reason to reach for it: a form with forty message components pays only
 * for the one whose error changed.
 */
export hook useFormState<TValues extends FieldValues, TOutput>(
  options: UseFormStateOptions<TValues, TOutput>,
): FormState<TValues> {
  const control = options.control;
  const key = keyOf(options.name);
  const scoped = options.name != null;

  const names = useMemo(() => (scoped ? key.split(KEY_SEPARATOR) : null), [scoped, key]);
  const snapshot = useCallback(() => control.fieldStateSnapshot(key, names), [control, key, names]);

  return useSyncExternalStore(control.subscribeFormState, snapshot, snapshot);
}
