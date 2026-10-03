"use client";
// @flow

import * as React from "@uniflowed/react";
import { useFormStatus } from "react-dom";
import { props, type StyleArgument } from "@uniflowed/stylex";
import { Alert, Field } from "@uniflowed/ui";

import { styles } from "./controls.stylex.js";
import { Icon } from "./ui.js";
import { fieldError, type FormState } from "./social-model.js";

export { styles };

/** Read the nearest form’s pending state and announce its current submission action. */

export component SubmitButton(
  children    : string,
  pendingLabel: string = "Saving…",
  disabled    : boolean = false,
  xstyle?     : StyleArgument,
) {
  const { pending } = useFormStatus();

  return (
    <button
      type="submit"
      {...props(styles.button, styles.primary, xstyle)}
      disabled={pending || disabled}
    >
      {pending ? pendingLabel : children}
      <Icon name="arrow" size={16} />
    </button>
  );
}

component ErrorStatus(message: string, xstyle?: StyleArgument) renders Alert.Root {
  return (
    <Alert.Root live {...props(styles.formStatus, styles.formStatusError, xstyle)}>
      <Alert.Description>{message}</Alert.Description>
    </Alert.Root>
  );
}

/** Announce completed action feedback; pending state stays with the submit control. */

export component FormStatus(state: FormState<mixed>, xstyle?: StyleArgument) {
  // The polite region stays mounted before a successful Action updates its text.

  return (
    <>
      <p {...props(styles.formStatus, styles.formStatusSuccess, xstyle)} role="status">
        {
          match (state) {
            {status: "idle"} | {status: "error", ...}        => null,
            {status: "success", message: const message, ...} =>
              <>
                <Icon name="check" size={15} />
                {message}
              </>,
          }
        }
      </p>
      {
        match (state) {
          {status: "idle"} | {status: "success", ...}    => null,
          {status: "error", message: const message, ...} =>
            <ErrorStatus message={message} xstyle={xstyle} />,
        }
      }
    </>
  );
}

/** Associate a field label with its native control through the shared UI primitive. */

export component FormField(
  label   : string,
  error   : string | null = null,
  hint    : string | null = null,
  children: renders Field.Control,
) renders Field.Root {
  return (
    <Field.Root {...props(styles.field)} invalid={error != null}>
      <Field.Label {...props(styles.fieldLabel)}>{label}</Field.Label>
      {children}
      {hint == null ? null : (
        <Field.Description {...props(styles.fieldCopy)}>{hint}</Field.Description>
      )}
      {error == null ? null : (
        <Field.Error {...props(styles.fieldCopy, styles.fieldAlert)}>{error}</Field.Error>
      )}
    </Field.Root>
  );
}

/** Render the failed field’s message at the ID referenced by its control. */

export component FieldError(state: FormState<mixed>, name: string, xstyle?: StyleArgument) {
  return (
    <span id={`${name}-error`} {...props(styles.fieldError, xstyle)}>
      {fieldError(state, name)}
    </span>
  );
}
