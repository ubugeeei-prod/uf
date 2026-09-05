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
import type { FieldPath, FieldValues } from "./internal/field-path.js";
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
   */
  readonly defaultValues?: TValues,
  readonly mode?: Mode,
  readonly reValidateMode?: ReValidateMode,
  /** A schema, in place of the rules on each `register`. See `resolver.js`. */
  readonly resolver?: Resolver<TValues, TOutput>,
  /** Handed to the resolver on every run: a locale, a tenant, a user. */
  readonly context?: mixed,
  /** Move focus to the first field with an error after a failed submit. */
  readonly shouldFocusError?: boolean,
|};

/**
 * `watch`, in its five shapes.
 *
 * An intersection rather than one signature, so the reactive reads and the
 * imperative subscription are told apart by the checker instead of by a comment
 * — `const email = watch("email")` and `const stop = watch("email", save)` are
 * different enough that inferring `mixed` for both would be no help at all.
 */
export type Watch<TValues> = (() => TValues) &
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
  readonly getValues: (name?: FieldPath) => mixed,
  readonly setValue: (name: FieldPath, value: mixed, options?: SetValueOptions) => void,
  readonly getFieldState: (name: FieldPath) => FieldState,
  readonly reset: (values?: TValues, options?: ResetOptions) => void,
  readonly setError: (
    name: FieldPath,
    error: {| readonly type?: string, readonly message: string |},
    options?: {| readonly shouldFocus?: boolean |},
  ) => void,
  readonly clearErrors: (names?: FieldPath | $ReadOnlyArray<FieldPath>) => void,
  readonly trigger: (names?: FieldPath | $ReadOnlyArray<FieldPath>) => Promise<boolean>,
  readonly setFocus: (name: FieldPath, options?: {| readonly shouldSelect?: boolean |}) => void,
  readonly formState: FormState,
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

  const [instance] = useState(() => {
    const control = createFormStore<TValues, TOutput>({
      defaultValues: (options?.defaultValues ?? EMPTY_DEFAULTS) as $FlowFixMe,
      mode,
      reValidateMode,
      resolver,
      context,
      shouldFocusError,
    });
    return { control, registrar: createRegistrar(control, idBase) };
  });
  const control = instance.control;

  // The options the store was built from are the first render's. Anything that
  // can change between renders — a resolver closed over a prop, a context that
  // carries the signed-in user — is pushed in after each render, which is
  // before any event a user can cause.
  useEffect(() => {
    control.configure({
      defaultValues: (options?.defaultValues ?? EMPTY_DEFAULTS) as $FlowFixMe,
      mode,
      reValidateMode,
      resolver,
      context,
      shouldFocusError,
    });
  });

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

  const register = useCallback(
    (name: FieldPath, rules?: ValidationRules) => registrar.registerWith(errors, name, rules),
    [registrar, errors],
  );

  const watch = useCallback(
    (first?: mixed, second?: mixed) => {
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
      const name = String(first);
      control.observe(name);
      return control.valueAt(name);
    },
    [control, watchVersion],
  ) as $FlowFixMe;

  const getFieldState = useCallback(
    (name: FieldPath): FieldState => ({
      invalid: formState.errors[name] != null,
      isDirty: formState.dirtyFields[name] === true,
      isTouched: formState.touchedFields[name] === true,
      error: formState.errors[name],
    }),
    [formState],
  );

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
    (name?: FieldPath) => (name == null ? control.getValues() : control.valueAt(name)),
    [control],
  );

  return useMemo(
    () => ({
      register,
      errorProps: registrar.errorProps,
      unregister: control.unregister,
      handleSubmit: control.submitWith,
      watch,
      getValues,
      setValue: control.setValue,
      getFieldState,
      reset: control.reset,
      setError,
      clearErrors: control.clearErrors,
      trigger: control.trigger,
      setFocus,
      formState,
      control,
    }),
    [register, registrar, control, watch, getValues, getFieldState, setError, setFocus, formState],
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
