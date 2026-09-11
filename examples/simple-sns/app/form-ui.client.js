"use client";
// @flow

import * as React from "@uniflowed/react";
import { useFormStatus } from "react-dom";
import { AlertRoot, AlertDescription } from "@uniflowed/ui/alert";
import {
  FieldRoot,
  FieldLabel,
  FieldControl,
  FieldDescription,
  FieldError as PrimitiveError,
} from "@uniflowed/ui/field";
import { Icon } from "./ui.js";
import { fieldError, type FormState } from "./social-model.js";

/** Read the nearest form’s pending state and announce its current submission action. */
export component SubmitButton(
  children: string,
  pendingLabel: string = "Saving…",
  disabled: boolean = false,
) {
  const { pending } = useFormStatus();

  return (
    <button type="submit" className="button primary" disabled={pending || disabled}>
      {pending ? pendingLabel : children}
      <Icon name="arrow" size={16} />
    </button>
  );
}

component ErrorStatus(message: string) renders AlertRoot {
  return (
    <AlertRoot live className="form-status error">
      <AlertDescription>{message}</AlertDescription>
    </AlertRoot>
  );
}

/** Announce completed action feedback; pending state stays with the submit control. */
export component FormStatus(state: FormState<mixed>) {
  // The polite region stays mounted before a successful Action updates its text.

  return (
    <>
      <p className="form-status success" role="status">
        {
          match (state) {
            {status: "idle"} | {status: "error", ...} => null,
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
          {status: "idle"} | {status: "success", ...} => null,
          {status: "error", message: const message, ...} => <ErrorStatus message={message} />,
        }
      }
    </>
  );
}

/** Associate a field label with its native control through the shared UI primitive. */
export component FormField(
  label: string,
  error: string | null = null,
  hint: string | null = null,
  children: renders FieldControl,
) renders FieldRoot {
  return (
    <FieldRoot className="field" invalid={error != null}>
      <FieldLabel>{label}</FieldLabel>
      {children}
      {hint == null ? null : <FieldDescription>{hint}</FieldDescription>}
      {error == null ? null : <PrimitiveError>{error}</PrimitiveError>}
    </FieldRoot>
  );
}

/** Render the failed field’s message at the ID referenced by its control. */
export component FieldError(state: FormState<mixed>, name: string) {
  return (
    <span id={`${name}-error`} className="field-error">
      {fieldError(state, name)}
    </span>
  );
}
