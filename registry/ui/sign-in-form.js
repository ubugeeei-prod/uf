"use client";
// @flow
//
// Sign In Form: a block. An email and password form that submits through a
// React action, shows what went wrong where it went wrong, and keeps the email
// across a failed attempt.
//
// `uf ui add sign-in-form` wrote this file into the project, together with the
// components it is made of (`field.js`, `button.js` and `alert.js`). From then
// on it is the project's. `uf ui diff sign-in-form` shows how it has moved away
// from the registry in the uf you are running.
//
// # A block, not a component
//
// A component is one piece of an interface. A block is a finished piece of a
// page, made only from components the registry already has. It exists so that
// a sign-in page begins as a form that already gets the hard parts right, and
// not as an empty file. It is `Experimental`: the shape of its props may still
// change between releases, and `uf ui diff` is how to see that it has.
//
// # What it does, and what it leaves to you
//
// It renders the form and runs the round trip: `action` receives the previous
// result and the `FormData`, and returns the next result, which is React's
// `useActionState` contract. That makes `action` a Server Action or any async
// function. Checking the password, the session cookie and the redirect are all
// yours. They belong on the server, and this file never sees a credential
// after the submit.
//
// # What to keep true when you change it
//
// * **Validation happens in the action, and its errors are on the page.** The
//   form is `noValidate`, so an address with no `@` is not caught by a
//   browser bubble that vanishes when focus moves and that some screen readers
//   never read. It reaches the action, which returns an error that is rendered
//   under the field and stays there. Check the same things in the action that
//   `type="email"` and `required` would have checked.
// * **An error is said where it happened, in words.** A form-level failure
//   ("that email and password do not match") is a live alert at the top, so it
//   is announced when it appears. A field's error sits under that field, and
//   `Field` marks the field `aria-invalid` and adds the error to its
//   description, so moving back into the field reads the error again.
// * **Say which of the two was wrong only if you mean to.** Telling a reader
//   "no account with that email" tells an attacker which emails exist. The
//   example returns one message for both cases, and that is the safe default.
// * **The email survives a failed attempt and the password does not.** React
//   resets a form after its action runs. The email comes back as
//   `defaultValue` from the result the action returned. The password is never
//   put in a result, so it is never sent back to the page.
// * **The browser can fill both fields in.** `autoComplete="email"` and
//   `"current-password"` are what password managers look for. Leaving them off
//   makes the form worse for everyone who uses one.
// * **Pending keeps the button focusable.** While the action runs, the button
//   is `aria-disabled` and ignores clicks rather than being `disabled`, which
//   would drop keyboard focus to the page.
// * **The heading fits the page.** `headingLevel` is 1 by default, because the
//   form is usually the page. It is typed 1 to 3.

import * as React from "@uniflowed/react";
import { useActionState, useId } from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { ufTokens } from "@uniflowed/stylex/tokens.stylex.js";

import { Alert, AlertDescription, AlertTitle } from "./alert.js";
import { Button } from "./button.js";
import { Field, FieldError, FieldInput, FieldLabel } from "./field.js";

/**
 * What `action` returns, and the form renders.
 *
 * `message` is a failure of the form as a whole; `fieldErrors` belong to one
 * field each. `email` is what to put back in the email field. There is no
 * `password`, deliberately: see the header.
 */
export type SignInResult = {
  readonly message?: string,
  readonly fieldErrors?: {
    readonly email?: string,
    readonly password?: string,
  },
  readonly email?: string,
};

/** React's action contract: the previous result and the submitted form in, the next result out. */
export type SignInAction = (previous: SignInResult, form: FormData) => Promise<SignInResult>;

/** A heading level, which is all the form's title can be. */
export type SignInHeadingLevel = 1 | 2 | 3;

const INITIAL: SignInResult = {};

const styles = stylex.create({
  form: {
    display: "grid",
    gap: ufTokens.space4,
    boxSizing: "border-box",
    width: "100%",
    maxWidth: "24rem",
    fontFamily: ufTokens.fontSans,
    color: ufTokens.ink,
  },
  heading: {
    margin: 0,
    fontSize: ufTokens.textLg,
    fontWeight: ufTokens.weightBold,
    lineHeight: ufTokens.leadingTight,
  },
  submit: {
    width: "100%",
  },
});

/**
 * A sign-in form.
 *
 *     <SignInForm action={signIn} footer={<a href="/reset">Forgot your password?</a>} />
 *
 * `footer` is rendered after the button, for the links a sign-in page carries.
 */
export component SignInForm(
  action: SignInAction,
  heading?: string = "Sign in",
  headingLevel?: SignInHeadingLevel = 1,
  submitLabel?: string = "Sign in",
  pendingLabel?: string = "Signing in…",
  footer?: React.Node,
) {
  const [result, submit, pending] = useActionState<SignInResult, FormData>(action, INITIAL);
  const headingId = useId();
  const emailError = result.fieldErrors?.email;
  const passwordError = result.fieldErrors?.password;
  const title = { ...props(styles.heading), id: headingId };

  return (
    <form {...props(styles.form)} action={submit} aria-labelledby={headingId} noValidate>
      {
        match (headingLevel) {
          1 => <h1 {...title}>{heading}</h1>,
          2 => <h2 {...title}>{heading}</h2>,
          3 => <h3 {...title}>{heading}</h3>,
        }
      }
      {result.message != null ? (
        <Alert live tone="danger">
          <AlertTitle level={headingLevel + 1}>Could not sign in</AlertTitle>
          <AlertDescription>{result.message}</AlertDescription>
        </Alert>
      ) : null}
      <Field invalid={emailError != null} required>
        <FieldLabel>Email</FieldLabel>
        <FieldInput
          autoComplete="email"
          defaultValue={result.email ?? ""}
          name="email"
          type="email"
        />
        <FieldError>{emailError}</FieldError>
      </Field>
      <Field invalid={passwordError != null} required>
        <FieldLabel>Password</FieldLabel>
        <FieldInput autoComplete="current-password" name="password" type="password" />
        <FieldError>{passwordError}</FieldError>
      </Field>
      <Button
        aria-disabled={pending ? "true" : undefined}
        onClick={(event: SyntheticMouseEvent<HTMLButtonElement>) => {
          if (pending) event.preventDefault();
        }}
        tone="primary"
        type="submit"
        xstyle={styles.submit}
      >
        {pending ? pendingLabel : submitLabel}
      </Button>
      {footer}
    </form>
  );
}
