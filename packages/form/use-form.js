// @flow
//
// `@uniflowed/form/use-form`: the form itself.
//
// `useForm` is the only hook that creates anything. Everything else in the
// package — `useWatch`, `useFormState`, `useFieldArray`, `useController` —
// takes the `control` it returns and subscribes to a part of it. That split is
// the whole performance story: the component that owns the form subscribes to
// the form's state, and a component that only cares about one field subscribes
// to one field.
//
//   component SignUp() {
//     const { register, handleSubmit, formState, errorProps } = useForm({
//       defaultValues: { email: "", password: "" },
//       mode: "onTouched",
//     });
//     return (
//       <form onSubmit={handleSubmit(save)}>
//         <input {...register("email", { required: "We need an email address" })} />
//         {formState.errors.email != null && (
//           <p {...errorProps("email")}>{formState.errors.email.message}</p>
//         )}
//         <button disabled={formState.isSubmitting}>Sign up</button>
//       </form>
//     );
//   }
//
// # What a keystroke costs
//
// Nothing, after the first. The input is uncontrolled, so the browser updates
// the text and the store records the value; the form's `formState` snapshot is
// rebuilt, compared, and found identical, and React's `useSyncExternalStore`
// bails out without rendering. The first keystroke is the exception: it turns
// `isDirty` on, which is a real change, and costs one render. See
// `internal/form-store.js` for why that one is not removable without a `Proxy`.
//
// # Why `watch` is two functions in one
//
// `watch("email")` during render subscribes the component to that path and
// returns its value — a change to `password` does not re-render it.
// `watch("email", listener)` subscribes a *callback* and returns an
// unsubscribe, rendering nothing at all: it is what an autosave, an analytics
// call or a dependent fetch actually wants, and none of those should be
// dragging a render along behind them.
//
// The reactive form adds the path to a set held in the store rather than in
// component state, and returns the live value; the subscription is a version
// counter that moves when an observed path changes.
// `internal/form-store.js` explains why that shape and not the obvious one.
//
// A component that is not the one that called `useForm` — a field component
// three levels down, reached through `FormProvider` — should use `useWatch`
// instead. Both are correct; `useWatch` re-renders only that component, which
// is the point of it being down there.
//
// # Why `FormProvider` lives here
//
// It is the same subject: the form, reached from somewhere else. It carries the
// whole return value, which is what makes `useFormContext()` a drop-in for the
// `useForm()` a component would otherwise have had to be given as props.

import * as React from "@uniflowed/react";
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useId,
  useMemo,
  useState,
  useSyncExternalStore,
} from "@uniflowed/react";

import type { ValidationRules } from "./rules.js";
import type { Resolver } from "./resolver.js";
import type { FieldPath, FieldSegment, FieldValues } from "./internal/field-path.js";
import { pathOf } from "./internal/field-path.js";
import type {
  Control,
  FieldErrors,
  FormState,
  Mode,
  ReValidateMode,
  ResetOptions,
  SetValueOptions,
  WatchInfo,
} from "./internal/form-store.js";
import { createFormStore } from "./internal/form-store.js";
import type { ErrorProps, FieldProps } from "./internal/register.js";
import { createRegistrar } from "./internal/register.js";

export type {
  Control,
  FieldErrors,
  FormState,
  Mode,
  ReValidateMode,
  ResetOptions,
  SetValueOptions,
};
export type { ErrorProps, FieldProps };

/** What an imperative `watch` callback is given. */
export type WatchListener<TValues> = (values: TValues, info: WatchInfo) => void;

/** What `getFieldState` answers about one field. */
export type FieldState = {|
  readonly invalid: boolean,
  readonly isDirty: boolean,
  readonly isTouched: boolean,
  readonly error: mixed,
|};

export type UseFormOptions<TValues extends FieldValues, TOutput = TValues> = {|
  /**
   * What the form starts as, and what `reset()` goes back to.
   *
   * Kept as a deep copy, so a caller who later mutates the object they passed
   * does not change what the form resets to.
   *
   * A thunk returning a promise is the other half: an edit form that has to
   * fetch the record it is editing renders immediately — empty, not dirty, with
   * `formState.isLoading` true — and takes the values when they arrive, without
   * an effect, a second component, or a render with the wrong values in it.
   * Called once, from an effect on mount. If the user has typed something by
   * the time it resolves, their text stays and the rest of the record lands
   * around it.
   *
   * Not a `DeepPartial` of `TValues`, which is what React Hook Form takes: Flow
   * can write `Partial<T>` and not the recursive version without an `any` in the
   * middle of it, and the whole values object is the honest requirement anyway —
   * it is what `reset()` goes back to.
   */
  readonly defaultValues?: TValues | (() => Promise<TValues>),
  /**
   * Values something outside the form owns, which the form follows.
   *
   * For a form whose values are not its own: a record refetched by
   * `@uniflowed/query`, a draft in a `@uniflowed/state` atom, a row selected in
   * a list beside the form. When the object changes, the form re-seeds itself —
   * which is the `useEffect` calling `reset` that would otherwise be written by
   * hand, at the same point in the commit, with the same rules, and without the
   * render with the old values in it.
   *
   * What a re-seed keeps is [`resetOptions`], and `keepDirtyValues` is the one
   * to reach for on a form somebody is typing into.
   *
   * Compared by identity, then by content: an object literal written inline is
   * a new object on every render, and re-seeding on each of those would be a
   * loop rather than a feature.
   */
  readonly values?: TValues,
  /**
   * Errors something outside the form owns — a server's answer, usually.
   *
   * `setError` takes one field at a time, so a rejected submit means a loop at
   * the call site and errors that nothing remembers the origin of. This is the
   * map as an input: it is applied when it changes, and what it replaces is its
   * own previous contribution rather than the errors validation produced.
   */
  readonly errors?: FieldErrors,
  /** What a `values` or `errors` re-seed keeps. Defaults to a plain reset. */
  readonly resetOptions?: ResetOptions,
  readonly mode?: Mode,
  readonly reValidateMode?: ReValidateMode,
  /** A schema, in place of the rules on each `register`. See `resolver.js`. */
  readonly resolver?: Resolver<TValues, TOutput>,
  /** Handed to the resolver on every run: a locale, a tenant, a user. */
  readonly context?: mixed,
  /** Move focus to the first field with an error after a failed submit. */
  readonly shouldFocusError?: boolean,
  /**
   * Put the rules on the elements, so the browser enforces them before the
   * JavaScript arrives. `false` by default.
   *
   * `register` then emits `required`, `min`, `max`, `minlength`, `maxlength`
   * and `pattern` from the rules it was given, and a server-rendered form is
   * validated by the browser with no hydration at all. Off by default because
   * `pattern` gives the browser a second opinion about the same regular
   * expression — `rules.js`'s `constraintsOf` says exactly where the two stop
   * agreeing — and a form should not acquire that without asking.
   */
  readonly progressive?: boolean,
  /**
   * Switch the whole form off: every field disabled, none validated, and none
   * of their values submitted.
   *
   * `useForm({ disabled: isSaving })` is what it is for — a user must not be
   * able to keep typing into a form that is being saved, because those
   * keystrokes are in the store and not in the request.
   */
  readonly disabled?: boolean,
|};

// ---------------------------------------------------------------------- //
// Reading a field at its type
//
// Every signature below is an intersection whose first four arms are the same
// four: one per path length, each segment bounded by the keys of what the
// segment before it landed on. `getValues("address", "city")` is `string`
// because `FieldSegment<TValues["address"]>` is `"city" | "zip"` and
// `TValues["address"]["city"]` is what the values say it is.
//
// Three things about that are worth knowing before reading them.
//
// **The last arm is the dotted form, and it has to be last.** Flow resolves an
// intersection by trying its arms in order, so `getValues("address.city")` fails
// the bound on the first four — `"address.city"` is not a key of anything — and
// lands on `(name: FieldPath) => mixed`, which is exactly what it does today.
// Nothing that worked before this stops working, and nothing that was `mixed`
// before is anything but `mixed` now.
//
// **A segment that is not a key is caught, and a segment past the fourth is
// not.** The depth is capped because the recursive form — one arm, matching
// `[K, ...Rest]` — binds `K` as `unknown` in this checker, which is
// ubugeeei-prod/uf#300. Four covers `items.0.tags.0`; a fifth segment falls to
// the dotted arm and is `mixed`.
//
// **`setValue` takes its segments as an array and the readers take them as
// arguments**, and that is forced rather than chosen: the value has to come
// after the path, so a positional `setValue("address", "city", next)` could not
// be told from today's `setValue(name, value, options)` at run time — three
// arguments, all of them possibly strings. An array can be told apart by
// `Array.isArray`, at the call and in the type.
// ---------------------------------------------------------------------- //

/**
 * `getValues`, in its six shapes.
 *
 * The four typed arms, then the whole-form read, then the dotted string. The
 * order is the resolution order and the last two are unchanged by any of this:
 * `getValues()` is `TValues` and `getValues("items.0.price")` is `mixed`,
 * exactly as before.
 */
export type GetValues<TValues> = (<K1 extends FieldSegment<TValues>>(k1: K1) => TValues[K1]) &
  (<K1 extends FieldSegment<TValues>, K2 extends FieldSegment<TValues[K1]>>(
    k1: K1,
    k2: K2,
  ) => TValues[K1][K2]) &
  (<
    K1 extends FieldSegment<TValues>,
    K2 extends FieldSegment<TValues[K1]>,
    K3 extends FieldSegment<TValues[K1][K2]>,
  >(
    k1: K1,
    k2: K2,
    k3: K3,
  ) => TValues[K1][K2][K3]) &
  (<
    K1 extends FieldSegment<TValues>,
    K2 extends FieldSegment<TValues[K1]>,
    K3 extends FieldSegment<TValues[K1][K2]>,
    K4 extends FieldSegment<TValues[K1][K2][K3]>,
  >(
    k1: K1,
    k2: K2,
    k3: K3,
    k4: K4,
  ) => TValues[K1][K2][K3][K4]) &
  (() => TValues) &
  ((name: FieldPath) => mixed);

/**
 * `setValue`, in its five shapes.
 *
 * The path is an array here rather than a list of arguments, for the reason
 * above it: the value follows the path, and three string arguments would be
 * ambiguous at run time. What the typed arms buy is the *value* as well as the
 * path — `setValue(["items", 0, "price"], "cheap")` is refused, which is the
 * half of this that a wrong read cannot tell you about.
 */
export type SetValue<TValues> = (<K1 extends FieldSegment<TValues>>(
  path: [K1],
  value: TValues[K1],
  options?: SetValueOptions,
) => void) &
  (<K1 extends FieldSegment<TValues>, K2 extends FieldSegment<TValues[K1]>>(
    path: [K1, K2],
    value: TValues[K1][K2],
    options?: SetValueOptions,
  ) => void) &
  (<
    K1 extends FieldSegment<TValues>,
    K2 extends FieldSegment<TValues[K1]>,
    K3 extends FieldSegment<TValues[K1][K2]>,
  >(
    path: [K1, K2, K3],
    value: TValues[K1][K2][K3],
    options?: SetValueOptions,
  ) => void) &
  (<
    K1 extends FieldSegment<TValues>,
    K2 extends FieldSegment<TValues[K1]>,
    K3 extends FieldSegment<TValues[K1][K2]>,
    K4 extends FieldSegment<TValues[K1][K2][K3]>,
  >(
    path: [K1, K2, K3, K4],
    value: TValues[K1][K2][K3][K4],
    options?: SetValueOptions,
  ) => void) &
  ((name: FieldPath, value: mixed, options?: SetValueOptions) => void);

/**
 * `getFieldState`, in its five shapes.
 *
 * No value type is involved — a `FieldState` is the same shape whatever the
 * field holds — so what the typed arms check is the path and nothing else. That
 * is still the answer to "how do I read the error for a nested field with the
 * checker's help": the errors stay one flat map keyed by the dotted path, for
 * the reason `resolver.js` gives, and this is the accessor that will not let you
 * misspell one.
 */
export type GetFieldState<TValues> = (<K1 extends FieldSegment<TValues>>(k1: K1) => FieldState) &
  (<K1 extends FieldSegment<TValues>, K2 extends FieldSegment<TValues[K1]>>(
    k1: K1,
    k2: K2,
  ) => FieldState) &
  (<
    K1 extends FieldSegment<TValues>,
    K2 extends FieldSegment<TValues[K1]>,
    K3 extends FieldSegment<TValues[K1][K2]>,
  >(
    k1: K1,
    k2: K2,
    k3: K3,
  ) => FieldState) &
  (<
    K1 extends FieldSegment<TValues>,
    K2 extends FieldSegment<TValues[K1]>,
    K3 extends FieldSegment<TValues[K1][K2]>,
    K4 extends FieldSegment<TValues[K1][K2][K3]>,
  >(
    k1: K1,
    k2: K2,
    k3: K3,
    k4: K4,
  ) => FieldState) &
  ((name: FieldPath) => FieldState);

/**
 * `watch`, in its nine shapes.
 *
 * An intersection rather than one signature, so the reactive reads and the
 * imperative subscription are told apart by the checker instead of by a comment
 * — `const email = watch("email")` and `const stop = watch("email", save)` are
 * different enough that inferring `mixed` for both would be no help at all.
 *
 * The four typed arms come first, and the two-argument one sits above
 * `(name, listener)` without disturbing it: a listener is a function and a
 * segment is not, so the arms are told apart by the checker for the same reason
 * the run time tells them apart with `typeof`.
 *
 * `watch(["a", "b"])` is *not* a path and never becomes one. It means the two
 * fields `a` and `b`, which is the meaning it has here and in React Hook Form,
 * and a library that quietly changed it into `a.b` would change what a working
 * form watched. That is why the readers take segments as arguments: the array
 * slot in this signature was already spoken for.
 */
export type Watch<TValues> = (<K1 extends FieldSegment<TValues>>(k1: K1) => TValues[K1]) &
  (<K1 extends FieldSegment<TValues>, K2 extends FieldSegment<TValues[K1]>>(
    k1: K1,
    k2: K2,
  ) => TValues[K1][K2]) &
  (<
    K1 extends FieldSegment<TValues>,
    K2 extends FieldSegment<TValues[K1]>,
    K3 extends FieldSegment<TValues[K1][K2]>,
  >(
    k1: K1,
    k2: K2,
    k3: K3,
  ) => TValues[K1][K2][K3]) &
  (<
    K1 extends FieldSegment<TValues>,
    K2 extends FieldSegment<TValues[K1]>,
    K3 extends FieldSegment<TValues[K1][K2]>,
    K4 extends FieldSegment<TValues[K1][K2][K3]>,
  >(
    k1: K1,
    k2: K2,
    k3: K3,
    k4: K4,
  ) => TValues[K1][K2][K3][K4]) &
  (() => TValues) &
  ((name: FieldPath) => mixed) &
  ((names: $ReadOnlyArray<FieldPath>) => $ReadOnlyArray<mixed>) &
  ((listener: WatchListener<TValues>) => () => void) &
  ((name: FieldPath, listener: WatchListener<TValues>) => () => void);

export type UseFormReturn<TValues extends FieldValues, TOutput = TValues> = {|
  /** Props for an uncontrolled control. See `internal/register.js`. */
  readonly register: (name: FieldPath, rules?: ValidationRules) => FieldProps,
  /** Props for the element that displays a field's error message. */
  readonly errorProps: (name: FieldPath) => ErrorProps,
  readonly unregister: (names?: FieldPath | $ReadOnlyArray<FieldPath>) => void,
  readonly handleSubmit: (
    onValid: (values: TOutput, event?: mixed) => mixed,
    onInvalid?: (errors: FieldErrors, event?: mixed) => mixed,
  ) => (event?: mixed) => Promise<void>,
  readonly watch: Watch<TValues>,
  readonly getValues: GetValues<TValues>,
  readonly setValue: SetValue<TValues>,
  readonly getFieldState: GetFieldState<TValues>,
  readonly reset: (values?: TValues, options?: ResetOptions) => void,
  readonly setError: (
    name: FieldPath,
    error: {| readonly type?: string, readonly message: string |},
    options?: {| readonly shouldFocus?: boolean |},
  ) => void,
  readonly clearErrors: (names?: FieldPath | $ReadOnlyArray<FieldPath>) => void,
  readonly trigger: (names?: FieldPath | $ReadOnlyArray<FieldPath>) => Promise<boolean>,
  readonly setFocus: (name: FieldPath, options?: {| readonly shouldSelect?: boolean |}) => void,
  readonly formState: FormState<TValues>,
  readonly control: Control<TValues, TOutput>,
|};

const EMPTY_DEFAULTS: FieldValues = Object.freeze({});

/**
 * Create a form.
 *
 * The store is built in a `useState` initialiser, which is React's supported
 * way to make something exactly once: an initialiser runs on the first render
 * of the component and is not re-run, and — unlike a `useRef` filled in during
 * render — it does not have to be guarded against Strict Mode's second pass.
 */
export hook useForm<TValues extends FieldValues, TOutput = TValues>(
  options?: UseFormOptions<TValues, TOutput>,
): UseFormReturn<TValues, TOutput> {
  const idBase = useId();
  const mode: Mode = options?.mode ?? "onSubmit";
  const reValidateMode: ReValidateMode = options?.reValidateMode ?? "onChange";
  const resolver = options?.resolver ?? null;
  const context = options?.context;
  const shouldFocusError = options?.shouldFocusError ?? true;
  const progressive = options?.progressive ?? false;
  const disabled = options?.disabled ?? false;
  const values = options?.values ?? null;
  const inputErrors = options?.errors ?? null;
  const resetOptions = options?.resetOptions ?? null;

  const [instance] = useState(() => {
    const control = createFormStore<TValues, TOutput>({
      defaultValues: (options?.defaultValues ?? EMPTY_DEFAULTS) as $FlowFixMe,
      values,
      errors: inputErrors,
      resetOptions,
      mode,
      reValidateMode,
      resolver,
      context,
      shouldFocusError,
      disabled,
    });
    return { control, registrar: createRegistrar(control, idBase) };
  });
  const control = instance.control;

  // `disabled` is left in the store here, during render, because the effect
  // below is one commit too late for it: a form switched off while it saves has
  // to be switched off in the commit that switched it on. `register` is handed
  // it directly and needs nothing from the store; `useController` holds a
  // `control` and nothing else, so this is where it gets to find out. The store
  // says why this is safe to write during a render, alongside the two writes
  // `register` and `watch` already make.
  control.noteDisabled(disabled);

  // The options the store was built from are the first render's. Anything that
  // can change between renders — a resolver closed over a prop, a context that
  // carries the signed-in user — is pushed in after each render, which is
  // before any event a user can cause.
  useEffect(() => {
    control.configure({
      defaultValues: (options?.defaultValues ?? EMPTY_DEFAULTS) as $FlowFixMe,
      values,
      errors: inputErrors,
      resetOptions,
      mode,
      reValidateMode,
      resolver,
      context,
      shouldFocusError,
      disabled,
    });
  });

  // An asynchronous `defaultValues` starts here rather than in the initialiser
  // above, and the difference is React's rule rather than taste: an initialiser
  // runs during a render, and a render can be thrown away — a fetch started in
  // one is a request nobody asked for. The store makes this idempotent, so
  // Strict Mode's mount, unmount and mount again is still one request.
  useEffect(() => {
    control.loadDefaults();
  }, [control]);

  const formState = useSyncExternalStore(
    control.subscribeFormState,
    control.formState,
    control.formState,
  );

  // The subscription behind `watch(name)`. Its snapshot is a counter rather than
  // the watched values, because the set of watched paths grows *during* the
  // render that reads it — see `internal/form-store.js`.
  //
  // The counter is then a dependency of `watch` itself, and that is not
  // decoration. `watch(name)` reads the store live, and the React Compiler —
  // which uf runs over every `component` and `hook` — is entitled to cache the
  // result of a call whose function and arguments it can see are unchanged. A
  // `watch` whose identity moves whenever an observed value moves is one the
  // compiler cannot hold on to, which makes the live read safe by construction
  // rather than by luck.
  const watchVersion = useSyncExternalStore(
    control.subscribeObserved,
    control.observedVersion,
    control.observedVersion,
  );

  useEffect(() => {
    // Seed `isValid` where the mode already implies eager validation. In
    // `onSubmit` mode nothing runs until a submit, which is what stops a form
    // from firing its server-side checks on page load.
    if (mode !== "onSubmit") {
      control.primeValidity();
    }
  }, [control, mode]);

  const errors = formState.errors;
  const registrar = instance.registrar;

  // `progressive` and `disabled` are read from this render rather than from the
  // store, which learns them from the effect above one commit later. That is
  // soon enough for an event and one commit too late for an attribute: a form
  // disabled while it saves has to render a disabled control on the render that
  // disabled it.
  const register = useCallback(
    (name: FieldPath, rules?: ValidationRules) =>
      registrar.registerWith(errors, { progressive, disabled }, name, rules),
    [registrar, errors, progressive, disabled],
  );

  // One implementation behind each of the intersections above, which is why
  // each of these ends in a cast. An intersection of function types is a
  // *promise about the calls*, and it is kept by the arms being mutually
  // exclusive at run time — a listener is a function, a segment is not; an array
  // of names is an array, a segment is not — rather than by a body Flow could
  // check against nine signatures at once. `watch` was already written this way
  // for its five; the three below join it for the same reason.
  const watch = useCallback(
    (first?: mixed, ...rest: $ReadOnlyArray<mixed>) => {
      const second = rest[0];
      if (typeof first === "function") {
        return control.listen(null, first as $FlowFixMe);
      }
      if (typeof second === "function") {
        return control.listen(String(first), second as $FlowFixMe);
      }
      if (first == null) {
        control.observe("");
        return control.getValues();
      }
      if (Array.isArray(first)) {
        const names: $ReadOnlyArray<FieldPath> = first as $FlowFixMe;
        for (const name of names) {
          control.observe(name);
        }
        return names.map((name) => control.valueAt(name));
      }
      const name = pathOf([String(first), ...rest.map((segment) => String(segment))]);
      control.observe(name);
      return control.valueAt(name);
    },
    [control, watchVersion],
  ) as $FlowFixMe;

  const getFieldState = useCallback(
    (first: mixed, ...rest: $ReadOnlyArray<mixed>): FieldState => {
      const name = pathOf([String(first), ...rest.map((segment) => String(segment))]);
      return {
        invalid: formState.errors[name] != null,
        isDirty: formState.dirtyFields[name] === true,
        isTouched: formState.touchedFields[name] === true,
        error: formState.errors[name],
      };
    },
    [formState],
  ) as $FlowFixMe;

  const setError = useCallback(
    (
      name: FieldPath,
      error: {| readonly type?: string, readonly message: string |},
      setErrorOptions?: {| readonly shouldFocus?: boolean |},
    ) => {
      control.setError(
        name,
        { type: error.type ?? "manual", message: error.message },
        setErrorOptions,
      );
    },
    [control],
  );

  const setFocus = useCallback(
    (name: FieldPath, focusOptions?: {| readonly shouldSelect?: boolean |}) => {
      control.focus(name, focusOptions?.shouldSelect === true);
    },
    [control],
  );

  const getValues = useCallback(
    (first?: mixed, ...rest: $ReadOnlyArray<mixed>) =>
      first == null
        ? control.getValues()
        : control.valueAt(pathOf([String(first), ...rest.map((segment) => String(segment))])),
    [control],
  ) as $FlowFixMe;

  // `setValue` is the store's own function everywhere but here, and here it is
  // wrapped for one line: the array form. `Array.isArray` is the same test the
  // type makes — a tuple in the first position or a string, never both.
  const setValue = useCallback(
    (target: mixed, value: mixed, setValueOptions?: SetValueOptions) => {
      control.setValue(
        Array.isArray(target)
          ? pathOf((target as $ReadOnlyArray<mixed>).map((segment) => String(segment)))
          : String(target),
        value,
        setValueOptions,
      );
    },
    [control],
  ) as $FlowFixMe;

  return useMemo(
    () => ({
      register,
      errorProps: registrar.errorProps,
      unregister: control.unregister,
      handleSubmit: control.submitWith,
      watch,
      getValues,
      setValue,
      getFieldState,
      reset: control.reset,
      setError,
      clearErrors: control.clearErrors,
      trigger: control.trigger,
      setFocus,
      formState,
      control,
    }),
    [
      register,
      registrar,
      control,
      watch,
      getValues,
      setValue,
      getFieldState,
      setError,
      setFocus,
      formState,
    ],
  );
}

const FormContext: React.Context<mixed> = createContext(null);

/**
 * Hand the whole form to everything below it.
 *
 * The alternative is threading `control` through every layer, which is fine for
 * two levels and not for five. What is passed is the entire `useForm` return
 * value, so a field component can call `useFormContext()` where it would
 * otherwise have called `useForm()`.
 */
export component FormProvider<TValues extends FieldValues, TOutput = TValues>(
  form: UseFormReturn<TValues, TOutput>,
  children: React.Node,
) {
  return <FormContext.Provider value={form}>{children}</FormContext.Provider>;
}

/**
 * The form a `FormProvider` above put there.
 *
 * Raises rather than returning null: a field that renders without a form would
 * render a control wired to nothing, and would look entirely correct.
 *
 * Note that `watch(name)`'s *reactive* form belongs to the component that
 * called `useForm`. Read a value from down here with `useWatch({ control })`,
 * which subscribes this component and leaves the form alone.
 */
export hook useFormContext<TValues extends FieldValues, TOutput = TValues>(): UseFormReturn<
  TValues,
  TOutput,
> {
  const form = useContext(FormContext);
  if (form == null) {
    throw new Error("useFormContext must be called inside a FormProvider");
  }
  return form as $FlowFixMe;
}
