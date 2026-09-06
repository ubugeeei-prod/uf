// @flow
//
// `@uniflowed/form`: forms that do not re-render.
//
// The premise React Hook Form established, and it is the right one: a form's
// values do not belong in React state. A controlled form re-renders on every
// character, which is fine for three fields and is what makes a forty-field
// page feel broken. So the text stays in the DOM, the values live in a store
// beside it, and React is told only when something a component actually asked
// about has changed.
//
//   component SignUp() {
//     const { register, handleSubmit, formState, errorProps } = useForm({
//       defaultValues: { email: "", password: "" },
//       mode: "onTouched",
//     });
//
//     return (
//       <form onSubmit={handleSubmit(createAccount)}>
//         <input {...register("email", { required: "We need an email address" })} />
//         {formState.errors.email != null && (
//           <p {...errorProps("email")}>{formState.errors.email.message}</p>
//         )}
//         <button disabled={formState.isSubmitting}>Sign up</button>
//       </form>
//     );
//   }
//
// `useForm({ disabled: formState.isSubmitting })` is the other half of that
// last line, and it is worth knowing about before it is needed: disabling the
// button stops a second submit, and disabling the *form* stops the keystrokes
// that would otherwise land in the store after the request had already read
// it.
//
// # This is built on React's rules, not around them
//
// The store is reached through `useSyncExternalStore` — the API that exists for
// exactly this — with cached, immutable snapshots and a stated server snapshot.
// There are no proxies, no getters that change what a render already read, no
// stable mutable object standing in for state, and nothing that needs a render
// to have happened exactly once. Strict Mode's double render, a concurrent
// render that is thrown away, and a render the React Compiler skips are all
// correct here, and `internal/form-store.js` sets out why for each of the two
// writes this package does make during a render.
//
// What that costs is one thing, and it is written down rather than glossed:
// reading `formState` in the component that called `useForm` re-renders it once
// when `isDirty` first turns on, and twice per submit. It does not re-render
// per keystroke. React Hook Form gets that first render to zero with a `Proxy`
// that records which `formState` keys a render read; that technique depends on
// a render having happened, which is the thing the React Compiler is allowed to
// skip. `useFormState` is the answer here: subscribe where the value is
// rendered, not at the top.
//
// # How the package is laid out
//
// Seven modules beside this one, split by subject rather than by layer:
//
// - `use-form.js` — the form: `useForm`, and `FormProvider` for reaching it
//   from below.
// - `watch.js` — subscribing to a part of one: `useWatch`, `useFormState`.
// - `field-array.js` — a list of rows, and the keys that make one work.
// - `controller.js` — the field that owns its own value, and so must be
//   controlled.
// - `rules.js` — what `required`, `min`, `pattern` and the rest mean.
// - `resolver.js` — the contract a schema library plugs into.
// - `validator.js` — `@uniflowed/validator` plugged into it. A separate entry
//   point, so a form that uses another schema library never resolves it.
//
// `internal/` holds the four modules a consumer has no business calling:
// `field-path.js` (the path grammar and the immutable value tree),
// `field-element.js` (reading a value out of a control and putting one back),
// `form-store.js` (the store and its snapshots) and `register.js` (the props an
// uncontrolled input is given, and the accessibility wiring around them).
//
// The line is between values and types. Not one function from those four is
// exported here or reachable through a subpath — `createFormStore` is not an
// API. Their *types* are, because `Control` and `FormState` are in the
// signature of every hook above, and a package whose public types cannot be
// named is a package nobody can write a wrapper for.
//
// # What is typed, and what is not
//
// `defaultValues`, `getValues()`, `reset()` and the values `handleSubmit` hands
// `onValid` are all `TValues` — or the resolver's output type, where a schema
// coerces. Inference flows through `useForm({ defaultValues })` without an
// annotation.
//
// Field *paths* are `string`, and the value at a path is `mixed`. Flow has no
// template-literal types, so `"address.city"` cannot be checked against the
// shape of the values, and there is no way to say "the type at this path".
// Pretending otherwise — a `FieldPath<T>` alias that is really `string` — would
// be a type that looks like it checks something and does not. Where a value's
// type matters, name it at the use site: `const city = watch("address.city") as
// string`.
//
// The errors are flat, keyed by the same string `register` was given:
// `errors["address.city"]`, not `errors.address.city`. `resolver.js` explains
// why, and the short version is that the nested shape needs a mapped type over
// a path Flow cannot spell, so it would be `any` all the way down.
//
// # Readiness
//
// Implemented and tested: uncontrolled registration for text, checkbox, radio,
// select and multi-select controls; the five validation modes and
// re-validation; the seven built-in rules with `deps`; resolvers, synchronous
// and asynchronous, with stale results discarded; `useFieldArray` with stable
// keys and index remapping of errors, dirty and touched flags; `reset` with its
// keep options; accessible error wiring; narrow subscriptions;
// `useForm({ disabled })` and `register(name, { disabled })`; and
// `useForm({ progressive })`.
//
// Not implemented: `defaultValues` as a promise, `values`, `errors` as an
// input, `shouldUnregister`, `delayError`, and form-level persistence.
// `isValid` in `onSubmit` mode reflects the most recent submit rather than a
// validation nobody asked for — see `internal/form-store.js`.
//
// `shouldUseNativeValidation` is **declined** rather than pending, and the
// reason is that it and this package's accessibility wiring cannot both be in
// charge. It hands the messages to the browser through `setCustomValidity`,
// which shows them in a bubble that cannot be styled, cannot be placed, is
// dismissed by the next interaction, and is announced instead of — not
// alongside — the `role="alert"` element `errorProps` wires up. What it was
// mostly wanted for is a form the browser enforces before hydration, and
// `progressive` gives that without moving the messages anywhere.
//
// # What the server render carries
//
// `register` gives an input a `ref`, and a ref does not run on a server, so
// `defaultValues` alone puts no value into server-rendered HTML: the value is
// written into the control when it mounts. A page that must show its values
// before hydration should put them in the markup — `<input
// defaultValue={record.email} {...register("email")} />` — and the store adopts
// what the control already shows for any field it has no value for.
//
// The *constraints* do reach the markup, and only with `progressive`. A
// `progressive` form's HTML carries `required`, `min`, `max`, `minlength`,
// `maxlength` and `pattern`, so a submit before the JavaScript arrives is
// refused by the browser rather than accepted by the server. Without it the
// markup carries `aria-required` on a required field — which announces the
// constraint but enforces nothing — and the form is unvalidated until it
// hydrates. `disabled` reaches the markup either way.
//
// What no server render carries is the rest of validation: `validate`
// functions, resolvers, and every message this package would show. Those need
// the JavaScript, and a form whose correctness matters must also be checked on
// the server it posts to. Every half of this is covered by
// `tests/library/form.test.js`.

export type { FieldPath, FieldValues } from "./internal/field-path.js";
export type {
  Control,
  FieldErrors,
  FieldFlags,
  FormState,
  Mode,
  ReValidateMode,
  ResetOptions,
  SetValueOptions,
  WatchInfo,
} from "./internal/form-store.js";
export type { ErrorProps, FieldProps, RegisterContext } from "./internal/register.js";
export type { FieldConstraints, FieldError, Rule, Validate, ValidationRules } from "./rules.js";
export type { Resolver, ResolverErrors, ResolverResult } from "./resolver.js";
export type {
  FieldState,
  UseFormOptions,
  UseFormReturn,
  Watch,
  WatchListener,
} from "./use-form.js";
export type { UseFormStateOptions, UseWatchOptions } from "./watch.js";
export type { FieldArrayRow, UseFieldArrayOptions, UseFieldArrayReturn } from "./field-array.js";
export type {
  ControlledField,
  ControlledFieldState,
  UseControllerOptions,
  UseControllerReturn,
} from "./controller.js";

export { FormProvider, useForm, useFormContext } from "./use-form.js";
export { useFormState, useWatch } from "./watch.js";
export { useFieldArray } from "./field-array.js";
export { Controller, useController } from "./controller.js";
export { constraintsOf, isRequired, runRules, whenSettled } from "./rules.js";
export { collectErrors, errorsOf, runResolver } from "./resolver.js";
export { errorsFromIssues, validatorResolver } from "./validator.js";
