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
// So does a *per-field* read, if the path is given as segments:
//
//   const city = getValues("address", "city");   // string
//   const price = watch("items", 0, "price");    // number
//   setValue(["items", 0, "price"], 12);         // and 12 has to be a number
//
// A dotted `"address.city"` is still `mixed`, and still works. The difference
// is one fact about Flow rather than a preference: there are no template
// literal types, so a string cannot be checked against the shape of the values
// — but a *segment* can, because a generic bounded by the keys of the object it
// indexes resolves, and composes to the next segment. Everything below follows
// from that one observation.
//
// Three things the checker really does refuse, since the difference between
// them and "there is no way to say the type at this path" is the whole of this
// section. There are **no template literal types**, so React Hook Form's
// `FieldPath<T>` — a union of every dotted string a shape admits — cannot be
// built. There is **no `as` key remapping** in a mapped type, which is the
// other half of how that union is built and also why the errors cannot be
// nested. And a **recursive conditional over a tuple** binds its first element
// as `unknown`, which is ubugeeei-prod/uf#300 and is why the depth below is a
// number rather than a recursion. None of those stops a segment from being
// checked.
//
// What is checked, precisely:
//
// - **The segment.** `getValues("address", "country")` is an error naming
//   `country`, at the call. `setValue(["items", 0, "price"], "cheap")` is an
//   error naming the value's type, which no read could have told you.
// - **Four segments deep**, which reaches `items.0.tags.0`. A fifth falls back
//   to `mixed`. The cap is not a taste: the recursive form of the type — one
//   arm matching `[K, ...Rest]` — binds `K` as `unknown` in this checker, which
//   is filed as ubugeeei-prod/uf#300 with a reproduction. When that is fixed
//   the cap can go.
// - **A segment that is not a key falls back to the dotted form**, because the
//   dotted form has to keep working. So `getValues("addres")` is `mixed` rather
//   than an error at the call — it is caught where the value is used at a type,
//   which is where every read was caught before this. A *second* segment is
//   caught at the call, because by then the first has narrowed what is being
//   indexed.
// - **`useWatch` is a hook, and a hook declaration has one signature**, so its
//   typed path is a single `path` option rather than an intersection of
//   arities. The type it produces is the same; a misspelt segment there is
//   `mixed` rather than an error at the call, wherever it appears in the path.
//   `watch.js` says why in full.
//
// `watch(["a", "b"])` is *not* a path and does not become one: it means the two
// fields `a` and `b`, here as in React Hook Form. That is why the readers take
// segments as arguments rather than as an array — the array slot was taken —
// and why `setValue`, whose value has to follow the path, takes an array
// instead. `tests/type-tests/field-paths.js` holds every line of this to the
// checker's actual output.
//
// The errors are flat, keyed by the same string `register` was given:
// `errors["address.city"]`, not `errors.address.city`. `resolver.js` explains
// why, and the short version is that the nested shape needs a mapped type over
// a path Flow cannot spell, so it would be `any` all the way down. The typed
// way to ask about one field's error is `getFieldState("address", "city")`,
// which checks the path even though a `FieldState` is the same shape whatever
// the field holds.
//
// # Readiness
//
// Implemented and tested: uncontrolled registration for text, checkbox, radio,
// select and multi-select controls; the five validation modes and
// re-validation; the seven built-in rules with `deps`; resolvers, synchronous
// and asynchronous, with stale results discarded; `useFieldArray` with stable
// keys and index remapping of errors, dirty and touched flags; `reset` with its
// eleven keep options; accessible error wiring; narrow subscriptions;
// `useForm({ disabled })` and `register(name, { disabled })`; and
// `useForm({ progressive })`.
//
// Values can also enter a form after it has rendered, which is what an edit
// form fed by a server needs and what used to take a `useEffect` and a second
// render:
//
//   // The record is fetched, and the form is usable while it is in flight.
//   useForm({ defaultValues: () => fetchRecord(id) })
//
//   // The record is owned by something else, and the form follows it.
//   useForm({ values: record, resetOptions: { keepDirtyValues: true } })
//
//   // The server rejected the submit, and said which fields.
//   useForm({ errors: rejection })
//
// `formState.isLoading` is true while an asynchronous default is pending, and
// `formState.defaultValues` is what `reset()` would go back to. A default that
// resolves after the user has typed does not take their text away, and one that
// resolves after a `reset(values)` does not land at all — the same
// stale-answer rule the resolver runs on, applied to values.
//
// A re-seed is a `reset`, so what it keeps is `ResetOptions`, and the three
// questions that decides are answered there: a **dirty** field is replaced
// unless `keepDirtyValues`; **validation state** goes back to what it is for a
// fresh form unless `keepErrors` or `keepIsValid`; and a field the new values
// **no longer contain** is gone, because the tree is replaced rather than
// merged.
//
// Not implemented: `shouldUnregister`, `delayError`, and form-level
// persistence. `isValid` in `onSubmit` mode reflects the most recent submit
// rather than a validation nobody asked for — see `internal/form-store.js`.
//
// `formState.isReady` is **declined** rather than pending. React Hook Form has
// one because its form finishes setting itself up after the first render; this
// store is built in a `useState` initialiser, so its first snapshot is already
// the real one. An `isReady` here would be `!isLoading` under a second name, or
// a flag that flipped in an effect — which would cost every form in the package
// a render to learn something that was true before it started.
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

export type {
  FieldPath,
  FieldSegment,
  FieldSegments,
  FieldValues,
  ValueAtPath,
} from "./internal/field-path.js";
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
  GetFieldState,
  GetValues,
  SetValue,
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
export { useFieldSource } from "./field.js";
export { useFormState, useWatch } from "./watch.js";
export { useFieldArray } from "./field-array.js";
export { Controller, useController } from "./controller.js";
export { constraintsOf, isRequired, runRules, whenSettled } from "./rules.js";
export { collectErrors, errorsOf, runResolver } from "./resolver.js";
export { errorsFromIssues, validatorResolver } from "./validator.js";
