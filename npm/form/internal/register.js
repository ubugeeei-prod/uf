// @flow
//
// `register`: the boundary between a DOM control and the form.
//
// One function, and the reason the library exists. `register("email")` returns
// the four props an input needs and nothing more:
//
//   <input type="email" {...register("email", { required: "Required" })} />
//
// The input is *uncontrolled*. Its `value` is not a prop, so a keystroke does
// not go through React at all: the browser updates the text, `onChange` tells
// the store, and the store tells only the components that asked about that
// field. A controlled form re-renders on every character; this one does not
// render at all.
//
// # Why the handlers are cached per field name
//
// `ref`, `onChange` and `onBlur` are the same functions on every render for a
// given name. A fresh `ref` each render would make React detach and re-attach
// the element on every render, and detaching is what tells the store the field
// has gone — so an ordinary re-render would look like an unmount to the field
// that is being re-rendered.
//
// The props *object* is new each render, because two of its members are not
// stable: `aria-invalid` and `aria-describedby` follow the field's error, and
// the error comes from the caller's `formState` snapshot. That is deliberate —
// see below.
//
// # Accessibility: how the error is wired, and why it is wired here
//
// A form's error message is invisible to a screen reader unless three things
// are true at once: the control says it is invalid, the control names the
// element holding the message, and that element is actually in the document.
// Getting the third wrong is the worst of the three — `aria-describedby`
// pointing at an id that is not there makes a reader announce *nothing*, which
// is worse than omitting the attribute — so `register` emits the attribute only
// while there is an error to point at.
//
// The message element gets its half from `errorProps`:
//
//   <input {...register("email", { required: "We need an email address" })} />
//   {formState.errors.email != null && (
//     <p {...errorProps("email")}>{formState.errors.email.message}</p>
//   )}
//
// `errorProps` supplies the matching `id` and `role="alert"`, so the message is
// announced when it appears — which is the point of an error that arrives after
// a blur or a submit. The two halves share an id derived from `useId()` and the
// field name, so a page with two of the same form has two of each.
//
// This wiring is `register`'s business rather than the caller's because the
// caller cannot do it correctly without knowing both halves: the id has to
// agree, and the attribute has to appear and disappear with the message.
// `@uniflowed/ui`'s `Field` solves the same problem for the label and the
// description; the two compose — put the input inside a `Field.Root` for the
// label, and spread `register` on it for the error.
//
// # The constraints, and why they are opt-in
//
// `register("email", { required: "We need an email address" })` records a rule
// and used to put none of it on the element. Two things follow from that, and
// the first is not a nicety.
//
// **Before hydration a form is unvalidated.** A server-rendered page that
// somebody submits before the JavaScript arrives posts whatever is in it. With
// `required` on the element the browser refuses; without it, nothing does. So
// `useForm({ progressive: true })` emits `required`, `min`, `max`, `minlength`,
// `maxlength` and `pattern` from the rules already given, and the markup
// carries the constraints with no JavaScript at all.
//
// **Assistive technology hears less than it could.** `aria-invalid` says a
// field is wrong after it has been checked. `required` says it is required
// before the user gets there. `aria-required` is emitted in its place when
// `progressive` is off, so the announcement does not depend on a flag that is
// really about what the browser enforces — and never beside it, because two
// attributes saying one thing is one too many.
//
// It is opt-in because `pattern` changes what the browser does. `rules.js`'s
// `constraintsOf` sets out where the `RegExp` and the attribute stop agreeing;
// a form that gets a second, slightly different opinion about its own pattern
// without asking would be a worse default than one that has to ask.
//
// # Server rendering
//
// A `ref` does not run on a server, so nothing here puts a value into
// server-rendered HTML: the store writes the value into the control when it
// mounts. That is fine for a client-rendered page and visible on a
// server-rendered one, where the input would be empty until hydration. The
// answer is to put the value in the markup — `<input defaultValue={record.email}
// {...register("email")} />` — and `attach` adopts whatever the control already
// shows for a field the store has no value for, so the two agree.
//
// `register` does not emit `defaultValue` itself, and deliberately: it does not
// know what the control is. `defaultValue` on a checkbox is React's `value`
// attribute, and on a radio it would overwrite the `value` that says which
// member of the group the input is.
//
// # Why `rules` are recorded during render
//
// `register(name, rules)` writes the rules into the store as it builds the
// props. `internal/form-store.js` documents why that write is safe; the short
// version is that no snapshot contains them, the write is idempotent, and a
// render the React Compiler skips is a render whose rules did not change.

import type { ValidationRules } from "../rules.js";
import { constraintsOf, isRequired } from "../rules.js";
import type { FieldPath, FieldValues } from "./field-path.js";
import type { Control, FieldErrors } from "./form-store.js";

/**
 * What `register` returns: spread it onto an `input`, `select` or `textarea`.
 *
 * Exact, and every member is declared even when it is absent, because that is
 * how an exact object type says "this may not be here": the value is `void`,
 * and React drops an attribute whose value is `undefined`. A caller cannot
 * spread something extra in to make up for a member this type does not have,
 * which is why the constraint attributes had to be added rather than left to
 * the application.
 */
export type FieldProps = {|
  readonly name: string,
  readonly ref: (element: mixed) => (() => void) | void,
  readonly onChange: (event: mixed) => void,
  readonly onBlur: (event: mixed) => void,
  /** Present only while the field has an error. */
  readonly "aria-invalid": "true" | void,
  /** The id of the message element, and only while that element is rendered. */
  readonly "aria-describedby": string | void,
  /**
   * Present when the field is required and the native attribute is not.
   *
   * Exactly one of the two is emitted, never both. `required` on the element
   * already announces the field as required, so adding the ARIA attribute
   * beside it is a second copy of the same fact; without `required` — which is
   * every form that has not asked for `progressive` — the ARIA attribute is the
   * only thing that announces it before the user gets there. `aria-invalid`
   * says a field is wrong after it has been checked; this says it is required
   * before, which is the announcement that prevents the error rather than
   * reporting it.
   */
  readonly "aria-required": "true" | void,
  /** The five constraint attributes, and only under `progressive`. */
  readonly required: boolean | void,
  readonly min: number | string | void,
  readonly max: number | string | void,
  readonly minLength: number | void,
  readonly maxLength: number | void,
  readonly pattern: string | void,
  /** Present while the field, or the whole form, is switched off. */
  readonly disabled: boolean | void,
|};

/** What the current render says about the form as a whole. */
export type RegisterContext = {|
  /**
   * Emit the constraint attributes, so the browser enforces them before the
   * JavaScript arrives.
   */
  readonly progressive: boolean,
  /** The whole form is switched off — `useForm({ disabled })`. */
  readonly disabled: boolean,
|};

/** What `errorProps` returns: spread it onto the element holding the message. */
export type ErrorProps = {|
  readonly id: string,
  readonly role: "alert",
|};

type Handlers = {|
  readonly ref: (element: mixed) => (() => void) | void,
  readonly onChange: (event: mixed) => void,
  readonly onBlur: (event: mixed) => void,
|};

const NO_RULES: ValidationRules = Object.freeze({});

export type Registrar<TValues extends FieldValues, TOutput> = {|
  /**
   * Build the props for `name`, using the errors of the caller's snapshot.
   *
   * The errors are passed in rather than read off the store because a render
   * must be a function of what React handed it. `useForm` reads them once, from
   * `useSyncExternalStore`, and every `register` call in that render sees the
   * same consistent set.
   */
  readonly registerWith: (
    errors: FieldErrors,
    context: RegisterContext,
    name: FieldPath,
    rules?: ValidationRules,
  ) => FieldProps,
  readonly errorProps: (name: FieldPath) => ErrorProps,
  readonly errorId: (name: FieldPath) => string,
|};

/**
 * The registrar one form owns, made once alongside its store.
 *
 * `idBase` comes from React's `useId`, so the ids are stable across a render, a
 * re-render and a hydration, and unique between two instances of the same form
 * on one page. Deriving them from the field name alone would collide in exactly
 * the case that matters — the same component rendered twice.
 */
export function createRegistrar<TValues extends FieldValues, TOutput>(
  control: Control<TValues, TOutput>,
  idBase: string,
): Registrar<TValues, TOutput> {
  /**
   * One set of handlers per field name, for the life of the form.
   *
   * Entries are not removed when a field unmounts, and that is on purpose: a
   * field array's rows come and go and come back, and re-making the closures
   * would make React detach and re-attach every input. The map is bounded by
   * the number of distinct names a form ever used, which for a thousand-row
   * array is a thousand small closures and no more.
   */
  const handlers: Map<FieldPath, Handlers> = new Map();

  function errorId(name: FieldPath): string {
    return `${idBase}-${name}-error`;
  }

  /**
   * The three stable functions for a field.
   *
   * The `ref` returns a cleanup rather than waiting to be called with `null`,
   * which is React 19's form and the only one that says *which* element left —
   * a radio group is several nodes under one name, and "one of them went" is
   * not an answer the store can act on.
   */
  function handlersFor(name: FieldPath): Handlers {
    let held = handlers.get(name);
    if (held == null) {
      held = {
        ref: (element: mixed) => {
          if (element == null) {
            return undefined;
          }
          control.attach(name, element);
          return () => {
            control.detach(name, element);
          };
        },
        onChange: () => {
          control.handleChange(name);
        },
        onBlur: () => {
          control.handleBlur(name);
        },
      };
      handlers.set(name, held);
    }
    return held;
  }

  function registerWith(
    errors: FieldErrors,
    context: RegisterContext,
    name: FieldPath,
    rules?: ValidationRules,
  ): FieldProps {
    const held = rules ?? NO_RULES;
    control.rulesFor(name, held);
    const handlers = handlersFor(name);
    const invalid = errors[name] != null;
    const required = isRequired(held);
    // Both halves of the current render, not the store's copy of them. The
    // store is told what the latest render's options were from an effect, which
    // is soon enough for an event and one commit too late for an attribute: a
    // form disabled while it submits has to render a disabled control on the
    // render that disabled it, not the one after.
    const off = context.disabled || held.disabled === true;
    const constraints = context.progressive ? constraintsOf(held) : null;
    return {
      name,
      ref: handlers.ref,
      onChange: handlers.onChange,
      onBlur: handlers.onBlur,
      // Absent rather than `"false"` while the field is fine: the ui package's
      // `Field` makes the same choice, and a form that permanently announces
      // `aria-invalid="false"` on every control is noise.
      "aria-invalid": invalid ? "true" : undefined,
      "aria-describedby": invalid ? errorId(name) : undefined,
      "aria-required": required && constraints == null ? "true" : undefined,
      required: constraints?.required,
      min: constraints?.min,
      max: constraints?.max,
      minLength: constraints?.minLength,
      maxLength: constraints?.maxLength,
      pattern: constraints?.pattern,
      disabled: off ? true : undefined,
    };
  }

  function errorProps(name: FieldPath): ErrorProps {
    return { id: errorId(name), role: "alert" };
  }

  return { registerWith, errorProps, errorId };
}
