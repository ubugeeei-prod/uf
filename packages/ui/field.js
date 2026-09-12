// @flow
//
// An accessible form field, wired up for you.
//
// The hard part of a form field is not the markup, it is the wiring: the label
// has to point at the control, the description and the error message have to be
// named by `aria-describedby`, the control has to say `aria-invalid` when it is
// wrong and `aria-required` before the reader gets there, and every id has to be
// unique on the page and stable across renders. Doing that by hand is five
// attributes and two `useId` calls per field, and getting one wrong is silent —
// the field looks right and a screen reader announces nothing.
//
// So the parts read the ids off a context the root creates. `Field.Label` knows
// which control it labels because there is exactly one in its root, and
// `Field.Error` registers itself so the control can point at it only when it is
// actually rendered — pointing `aria-describedby` at an id that is not in the
// document makes a screen reader announce nothing at all, which is worse than
// omitting the attribute.
//
// # One place computes the four attributes
//
// This is the whole of ubugeeei-prod/uf#297, and it was a bug that looked like
// working code. `@uniflowed/form`'s `register("email")` also produces
// `aria-invalid` and `aria-describedby`, from the form store's errors, and it is
// right to: a form library is what knows whether a field is wrong. Spread both
// onto one `<input>` and the *later spread wins*, so the field was described by
// the form's message or by `Field.Description`, depending on argument order,
// and never by both.
//
// The division that fixes it is the one each package can actually keep:
//
//   * **`Field` owns the ids and the attributes**, because it owns the label and
//     the description and is the only thing that can compose a token list out of
//     them.
//   * **The form owns the facts**: is this field invalid, is it required, what
//     does the message say, and what does the control have to be bound with.
//
// `FieldSource` is that handover, and `@uniflowed/form`'s `useFieldSource` is
// what fills it. Notice what it does *not* contain: no `aria-*` at all. A source
// that handed over a finished `aria-describedby` would be the same collision
// with an extra step.
//
// # Why the form fills a prop rather than the field reading a context
//
// Because the dependency can only run one way. `@uniflowed/ui` is published to
// npm and `@uniflowed/form` is not — its name has never been bound
// (ubugeeei-prod/uf#210) — and `tools/ci/publishable.sh` refuses a published
// package that depends on an unpublished one, because `npm install` would answer
// `ETARGET` for a package that installs perfectly well from this workspace. So
// `Field.Root` cannot import a form context, cannot take a `name` and look it
// up, and cannot know that `@uniflowed/form` exists.
//
// What it can do is state the shape it accepts and let the package that *may*
// depend on it produce one. `field={useFieldSource(form, "email")}` is one hook
// call where the application used to write `invalid={errors.email != null}` by
// hand and then choose which of two `aria-describedby` values survived.
//
// # Why it takes a render function
//
// `Field.Control` hands the attributes to a callback rather than rendering an
// `<input>`, because a field wraps a select, a textarea, a `Combobox.Input` or
// somebody else's component just as often, and each of those needs the same
// attributes on whatever element it eventually renders. A component that
// rendered the input itself would have to grow a prop for every element anyone
// might want, and would still be wrong for the next one.
//
// # A group is not a label's `for`
//
// `<label for>` names *one* form control. A set of radios, a checkbox group and
// a date picker made of three selects have no single control to point at, and a
// `for` aimed at the wrapper around them points at something that is not a form
// control — which browsers ignore, silently. `Field.Root group` is the answer:
// the root becomes `role="group"` named by the label through `aria-labelledby`,
// the label stops being a `<label>`, and the description and the error describe
// the set rather than one member of it.
//
// # Why `Field.Error` is `role="alert"` and `combobox.js` says the opposite
//
// `combobox.js`'s header argues that a live region inserted *together with* its
// content is usually not announced, and that a status region has to be in the
// document before there is anything to say. Both are true, and this component
// does the opposite on purpose.
//
// The difference is politeness. `role="status"` is polite: a screen reader waits
// for a pause, by which time a region that appeared and filled in one commit has
// nothing left to distinguish it, and several engines never announce it at all.
// `role="alert"` is assertive, and assertive regions are announced on insertion
// by every engine that implements them — it is what `alert` is for. So the
// constraint is real and it is `status`'s, not `alert`'s. Written down here
// because the package states the general rule strongly in one file and does the
// opposite in another, and a reader who finds the second one first is entitled
// to know it was a decision.

"use client";

import * as React from "@uniflowed/react";
import { createContext, useContext, useEffect, useId, useMemo, useState } from "@uniflowed/react";

import type { RenderProp, Rest } from "./internal/merge-props.js";
import { withProps } from "./internal/merge-props.js";

/**
 * What a form library tells a field about one of its fields.
 *
 * Facts, and no attributes: see the module header for why an `aria-describedby`
 * in here would be the collision this type exists to end. `control` is the
 * binding — the `name`, the `ref` the store attaches through, `onChange`,
 * `onBlur` and the constraint attributes a progressive form emits — spread onto
 * whatever element `Field.Control` renders, underneath the attributes the field
 * computes.
 *
 * # It is declared here and produced there
 *
 * `@uniflowed/form`'s `useFieldSource` builds one of these, so this type is the
 * seam between the two packages — and it lives on this side only because the
 * dependency can only run this way today: this package is on npm and that one
 * is not, and `tools/release/publishable.sh` refuses a published package that
 * depends on an unpublished one.
 *
 * That is backwards, and ubugeeei-prod/uf#614 says so. It stands because
 * declaring the type twice trades a documented edge for silent drift — a
 * `Field.Root` accepting a shape `useFieldSource` no longer produces would
 * type-check on both sides and fail only where they meet — and because the
 * import is type-only, so no project installing `@uniflowed/form` loads,
 * bundles or runs any of this package. #210 is the trigger: once
 * `@uniflowed/form` publishes, this moves there and `@uniflowed/ui` imports it.
 */
export type FieldSource = {|
  readonly invalid: boolean,
  readonly required: boolean,
  readonly disabled: boolean,
  /** What is wrong, or null while the field is valid. */
  readonly message: string | null,
  readonly control: Rest,
|};

/** No form: the shape the field falls back to, made once. */
const NO_CONTROL: Rest = Object.freeze({});

type FieldState = {|
  readonly controlId: string,
  readonly labelId: string,
  readonly descriptionId: string,
  readonly errorId: string,
  readonly invalid: boolean,
  readonly required: boolean,
  readonly group: boolean,
  readonly message: string | null,
  readonly control: Rest,
  readonly describedBy: string | void,
  readonly registerDescription: (present: boolean) => void,
  readonly registerError: (present: boolean) => void,
|};

const FieldContext: React.Context<FieldState | null> = createContext(null);

/**
 * The field a part belongs to.
 *
 * Raising rather than returning null: a `Field.Label` outside a `Field.Root`
 * would render a label pointing at nothing, and would look correct.
 */
hook useField(part: string): FieldState {
  const state = useContext(FieldContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Field.Root`);
  }
  return state;
}

/**
 * The field's container, and the only place ids are made.
 *
 * `invalid` and `required` are the root's business rather than the control's
 * because three parts have to agree about each: the control says `aria-invalid`
 * and `aria-required`, the error message is rendered or not, and the control's
 * `aria-describedby` includes the error's id or not.
 *
 * `field` is the same two facts arriving from a form store instead of from the
 * caller's hand, and it wins over the props when it is there — a field inside a
 * form has one source of truth about its own validity, and the props are what a
 * field with no form around it uses. Passing both is not an error; it is a
 * caller saying "and also mark it invalid", which is what `invalid || source`
 * means and what a server-side error arriving beside a client-side one needs.
 */
export component FieldRoot(
  children: React.Node,
  invalid?: boolean = false,
  required?: boolean = false,
  field?: FieldSource,
  group?: boolean = false,
  render?: RenderProp,
  ...rest: Rest
) {
  const base = useId();
  const [hasDescription, setHasDescription] = useState(false);
  const [hasError, setHasError] = useState(false);
  const sourceInvalid = field?.invalid ?? false;
  const sourceRequired = field?.required ?? false;
  const message = field?.message ?? null;
  const control = field?.control ?? NO_CONTROL;

  const state = useMemo(() => {
    const descriptionId = `${base}-description`;
    const errorId = `${base}-error`;
    const wrong = invalid || sourceInvalid;
    // Only ids that are in the document. `aria-describedby` naming a missing
    // element makes a screen reader announce nothing rather than skipping it.
    const described = [
      hasDescription ? descriptionId : null,
      wrong && hasError ? errorId : null,
    ].filter(Boolean);

    return {
      controlId: `${base}-control`,
      labelId: `${base}-label`,
      descriptionId,
      errorId,
      invalid: wrong,
      required: required || sourceRequired,
      group,
      message,
      control,
      describedBy: described.length === 0 ? undefined : described.join(" "),
      registerDescription: setHasDescription,
      registerError: setHasError,
    };
  }, [
    base,
    invalid,
    sourceInvalid,
    required,
    sourceRequired,
    group,
    message,
    control,
    hasDescription,
    hasError,
  ]);

  const props = withProps(rest, {
    // A group names itself, describes itself and reports its own validity,
    // because there is no one control inside it to carry any of the three.
    // See the module header.
    "aria-describedby": group ? state.describedBy : undefined,
    "aria-invalid": group && state.invalid ? "true" : undefined,
    "aria-labelledby": group ? state.labelId : undefined,
    children,
    role: group ? "group" : undefined,
  });

  return (
    <FieldContext.Provider value={state}>
      {render != null ? render(props) : <div {...props} />}
    </FieldContext.Provider>
  );
}

/**
 * The label, pointing at the control by id rather than by nesting.
 *
 * In a group it is a `<span>` instead, and the group points at *it*: a
 * `<label for>` naming something that is not a form control is ignored by every
 * browser, and ignored silently. The module header says more.
 */
export component FieldLabel(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const field = useField("Field.Label");
  // `rest` first: a caller `id` here would break the relationship the control
  // points at, and it would break it silently.
  if (field.group) {
    const props = withProps(rest, { children, id: field.labelId });
    return render != null ? render(props) : <span {...props} />;
  }
  const props = withProps(rest, { children, htmlFor: field.controlId, id: field.labelId });
  return render != null ? render(props) : <label {...props} />;
}

/**
 * The control, given every attribute the rest of the field implies.
 *
 * See the module header for why this takes a render function, and for the
 * division of labour that decides what is in here. The form's own binding is
 * underneath, so a caller who spreads these onto an element gets the store's
 * `ref` and handlers *and* the field's attributes from one spread instead of
 * two that overwrite each other.
 */
export component FieldControl(render: RenderProp) {
  const field = useField("Field.Control");
  // A group already carries the name, the description and the validity, and a
  // control repeating them makes a reader hear the error once for the set and
  // again for the member. What is left is what only the control can say.
  const own: Rest = field.group
    ? { "aria-required": field.required ? "true" : undefined }
    : {
        id: field.controlId,
        "aria-labelledby": field.labelId,
        "aria-describedby": field.describedBy,
        "aria-invalid": field.invalid ? "true" : undefined,
        "aria-required": field.required ? "true" : undefined,
      };
  return render(withProps(field.control, own));
}

/** Help text, which the control points at while it is rendered. */
export component FieldDescription(children: React.Node, render?: RenderProp, ...rest: Rest) {
  const field = useField("Field.Description");
  const register = field.registerDescription;
  useEffect(() => {
    register(true);
    return () => register(false);
  }, [register]);

  const props = withProps(rest, { children, id: field.descriptionId });
  return render != null ? render(props) : <p {...props} />;
}

/**
 * The error message, rendered only when the field is invalid.
 *
 * `role="alert"` so it is announced when it appears, which is the point of an
 * error that arrives after a blur, a submit, or a Server Action — see the
 * module header for why an assertive region may be inserted with its text where
 * a polite one may not.
 *
 * With no children it shows the message the form gave, so a field bound to a
 * store does not need the caller to reach back into `formState.errors` for a
 * string the source is already carrying.
 */
export component FieldError(children?: React.Node, render?: RenderProp, ...rest: Rest) {
  const field = useField("Field.Error");
  const register = field.registerError;
  useEffect(() => {
    register(true);
    return () => register(false);
  }, [register]);

  if (!field.invalid) {
    return null;
  }
  const props = withProps(rest, {
    children: children ?? field.message,
    id: field.errorId,
    role: "alert",
  });
  return render != null ? render(props) : <p {...props} />;
}
