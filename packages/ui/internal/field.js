// @flow
//
// An accessible form field, wired up for you.
//
// The hard part of a form field is not the markup, it is the wiring: the label
// has to point at the control, the description and the error message have to be
// named by `aria-describedby`, the control has to say `aria-invalid` when it is
// wrong, and every id has to be unique on the page and stable across renders.
// Doing that by hand is four attributes and two `useId` calls per field, and
// getting one wrong is silent — the field looks right and a screen reader
// announces nothing.
//
// So the parts read the ids off a context the root creates. `Field.Label` knows
// which control it labels because there is exactly one in its root, and
// `Field.Error` registers itself so the control can point at it only when it is
// actually rendered — pointing `aria-describedby` at an id that is not in the
// document makes a screen reader announce nothing at all, which is worse than
// omitting the attribute.

import * as React from "@uniflowed/react";
import { createContext, useContext, useId, useMemo, useState } from "@uniflowed/react";

type FieldState = {|
  +controlId: string,
  +labelId: string,
  +descriptionId: string,
  +errorId: string,
  +invalid: boolean,
  +describedBy: string | void,
  +registerDescription: (present: boolean) => void,
  +registerError: (present: boolean) => void,
|};

const FieldContext: React.Context<FieldState | null> = createContext(null);

/**
 * The field a part belongs to.
 *
 * Raising rather than returning null: a `Field.Label` outside a `Field.Root`
 * would render a label pointing at nothing, and would look correct.
 */
function useField(part: string): FieldState {
  const state = useContext(FieldContext);
  if (state == null) {
    throw new Error(`${part} must be rendered inside a Field.Root`);
  }
  return state;
}

/**
 * The field's container, and the only place ids are made.
 *
 * `invalid` is the root's business rather than the control's because three
 * parts have to agree about it: the control says `aria-invalid`, the error
 * message is rendered or not, and the control's `aria-describedby` includes the
 * error's id or not.
 */
export component FieldRoot(
  children: React.Node,
  invalid?: boolean = false,
  ...rest: { +[string]: mixed }
) {
  const base = useId();
  const [hasDescription, setHasDescription] = useState(false);
  const [hasError, setHasError] = useState(false);

  const state = useMemo(() => {
    const descriptionId = `${base}-description`;
    const errorId = `${base}-error`;
    // Only ids that are in the document. `aria-describedby` naming a missing
    // element makes a screen reader announce nothing rather than skipping it.
    const described = [
      hasDescription ? descriptionId : null,
      invalid && hasError ? errorId : null,
    ].filter(Boolean);

    return {
      controlId: `${base}-control`,
      labelId: `${base}-label`,
      descriptionId,
      errorId,
      invalid,
      describedBy: described.length === 0 ? undefined : described.join(" "),
      registerDescription: setHasDescription,
      registerError: setHasError,
    };
  }, [base, invalid, hasDescription, hasError]);

  return (
    <FieldContext.Provider value={state}>
      <div {...rest}>{children}</div>
    </FieldContext.Provider>
  );
}

/** The label, pointing at the control by id rather than by nesting. */
export component FieldLabel(children: React.Node, ...rest: { +[string]: mixed }) {
  const field = useField("Field.Label");
  // `rest` first: a caller `id` here would break the relationship the control
  // points at, and it would break it silently.
  return (
    <label {...rest} htmlFor={field.controlId} id={field.labelId}>
      {children}
    </label>
  );
}

/**
 * The control, given every attribute the rest of the field implies.
 *
 * `render` takes the element rather than this rendering an `<input>`, because a
 * field wraps a select, a textarea, a combobox or somebody else's component
 * just as often, and each of those needs the same six attributes.
 */
export component FieldControl(
  render: (props: { +[string]: mixed }) => React.Node,
) {
  const field = useField("Field.Control");
  return render({
    id: field.controlId,
    "aria-labelledby": field.labelId,
    "aria-describedby": field.describedBy,
    "aria-invalid": field.invalid ? "true" : undefined,
  });
}

/** Help text, which the control points at while it is rendered. */
export component FieldDescription(children: React.Node, ...rest: { +[string]: mixed }) {
  const field = useField("Field.Description");
  React.useEffect(() => {
    field.registerDescription(true);
    return () => field.registerDescription(false);
  }, [field]);

  return (
    <p {...rest} id={field.descriptionId}>
      {children}
    </p>
  );
}

/**
 * The error message, rendered only when the field is invalid.
 *
 * `role="alert"` so it is announced when it appears, which is the point of an
 * error that arrives after a blur or a submit.
 */
export component FieldError(children: React.Node, ...rest: { +[string]: mixed }) {
  const field = useField("Field.Error");
  React.useEffect(() => {
    field.registerError(true);
    return () => field.registerError(false);
  }, [field]);

  if (!field.invalid) {
    return null;
  }
  return (
    <p {...rest} id={field.errorId} role="alert">
      {children}
    </p>
  );
}
