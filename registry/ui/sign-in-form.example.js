// @flow
import * as React from "@uniflowed/react";

import { type SignInResult, SignInForm } from "./sign-in-form.js";

/**
 * Stands in for a Server Action. It never signs anybody in, so every path the
 * form draws can be tried here: an address with no `@`, an empty password, and
 * a pair that does not match an account.
 */
async function signIn(previous: SignInResult, form: FormData): Promise<SignInResult> {
  const email = String(form.get("email") ?? "");
  const password = String(form.get("password") ?? "");
  if (!email.includes("@")) {
    return { email, fieldErrors: { email: "Enter an email address, like ada@example.com." } };
  }
  if (password === "") {
    return { email, fieldErrors: { password: "Enter your password." } };
  }
  return { email, message: "That email and password do not match an account." };
}

/** The block, with a stand-in action and the links a sign-in page carries. */
export component Example() {
  return (
    <SignInForm
      action={signIn}
      footer={<a href="#reset-password">Forgot your password?</a>}
      headingLevel={2}
    />
  );
}
