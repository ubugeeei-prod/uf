"use client";
// @flow

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { useActionState, useState } from "@uniflowed/react";
import { props, stylex } from "@uniflowed/stylex";
import { Field } from "@uniflowed/ui";

import { callAction } from "../_shared/action-result.client.js";
import {
  FormField,
  FormStatus,
  SubmitButton,
  styles as controlStyles,
} from "../_shared/form-ui.client.js";
import { IDLE, failed, succeeded, fieldError, type FormState } from "../_shared/social-model.js";

/**
 * Submit credentials to the same-origin HTTP endpoint while preserving failed field drafts.
 * The server owns the HttpOnly cookie; successful authentication starts a fresh document.
 */

export component AuthClient(mode: "login" | "signup") {
  const [draft, setDraft]        = useState({ name: "", email: "", handle: "", password: "" });
  const [state, submit, pending] = useActionState<FormState<null>, FormData>(
    async (_previous: FormState<null>, form: FormData): Promise<FormState<null>> => {
      const body = new URLSearchParams({ mode });
      for (const key of ["name", "email", "handle", "password"]) {
        const value = form.get(key);
        if (typeof value === "string") body.set(key, value);
      }
      return callAction(async () => {
        const response = await fetch("/auth/session", {
          method: "POST",
          body,
          credentials: "same-origin",
        });
        const result = await response.json();
        if (!response.ok)
          return failed(result.message ?? "Could not sign in. Try again.", result.fields ?? {});
        // The server sets an HttpOnly cookie; no token enters React state or storage.
        window.location.assign("/");
        return succeeded(null, "Signed in. Redirecting…");
      }, "You seem to be offline. Please try again.");
    },
    IDLE,
  );

  return (
    <form {...props(styles.authCard)} action={submit}>
      <h1 {...props(styles.authTitle)}>
        {
          match (mode) {
            "signup" => "Create an account",
            "login"  => "Sign in",
          }
        }
      </h1>
      <p {...props(styles.authIntro)}>
        {
          match (mode) {
            "signup" => "Create a profile to publish notes and send messages.",
            "login"  => "Enter your handle and password to continue.",
          }
        }
      </p>
      {
        match (mode) {
          "login"  => null,
          "signup" =>
            <>
              <FormField label="Name" error={fieldError(state, "name")}>
                <Field.Control
                  render={(control) => (
                    <input
                      {...control}
                      className={props(controlStyles.fieldControl).className}
                      name="name"
                      value={draft.name}
                      onChange={(event) => setDraft({ ...draft, name: event.target.value })}
                      autoComplete="name"
                      required
                      maxLength={80}
                      disabled={pending}
                    />
                  )}
                />
              </FormField>
              <FormField label="Email address" error={fieldError(state, "email")}>
                <Field.Control
                  render={(control) => (
                    <input
                      {...control}
                      className={props(controlStyles.fieldControl).className}
                      name="email"
                      value={draft.email}
                      onChange={(event) => setDraft({ ...draft, email: event.target.value })}
                      type="email"
                      autoComplete="email"
                      required
                      maxLength={254}
                      disabled={pending}
                    />
                  )}
                />
              </FormField>
            </>,
        }
      }
      <FormField label="Handle" error={fieldError(state, "handle")}>
        <Field.Control
          render={(control) => (
            <input
              {...control}
              className={props(controlStyles.fieldControl).className}
              name="handle"
              value={draft.handle}
              onChange={(event) => setDraft({ ...draft, handle: event.target.value })}
              autoComplete="username"
              autoCapitalize="none"
              spellCheck={false}
              required
              minLength={3}
              maxLength={20}
              pattern="[a-z][a-z0-9_]{2,19}"
              placeholder="your_name"
              disabled={pending}
            />
          )}
        />
      </FormField>
      <FormField
        label="Password"
        error={fieldError(state, "password")}
        hint="At least 12 characters. Use a password just for this example."
      >
        <Field.Control
          render={(control) => (
            <input
              {...control}
              className={props(controlStyles.fieldControl).className}
              name="password"
              value={draft.password}
              onChange={(event) => setDraft({ ...draft, password: event.target.value })}
              type="password"
              autoComplete={
                match (mode) {
                  "signup" => "new-password",
                  "login"  => "current-password",
                }
              }
              required
              minLength={12}
              maxLength={128}
              disabled={pending}
            />
          )}
        />
      </FormField>
      <FormStatus state={state} />
      <SubmitButton
        pendingLabel={
          match (mode) {
            "signup" => "Creating account…",
            "login"  => "Signing in…",
          }
        }
        xstyle={styles.authSubmit}
      >
        {
          match (mode) {
            "signup" => "Create account",
            "login"  => "Sign in",
          }
        }
      </SubmitButton>
      <p {...props(styles.authAlternative)}>
        {
          match (mode) {
            "signup" =>
              <>
                Already have an account?{" "}
                <Link {...props(styles.authLink)} to="/login">
                  Sign in
                </Link>
              </>,
            "login"  =>
              <>
                No account yet?{" "}
                <Link {...props(styles.authLink)} to="/signup">
                  Create an account
                </Link>
              </>,
          }
        }
      </p>
      <p {...props(styles.authNote)}>
        Local example. Use sample details; email is not verified or sent.
      </p>
    </form>
  );
}

const styles = stylex.create({
  authCard: {
    width         : { default: "420px", "@media (max-width: 760px)": "100%" },
    background    : "transparent",
    border        : "0",
    borderRadius  : "0",
    backdropFilter: "none",
    paddingTop    : { default: "34px", "@media (max-width: 760px)": "25px" },
    paddingRight  : { default: "34px", "@media (max-width: 760px)": "22px" },
    paddingBottom : { default: "34px", "@media (max-width: 760px)": "25px" },
    paddingLeft   : { default: "34px", "@media (max-width: 760px)": "22px" },
    maxWidth      : { "@media (max-width: 760px)": "400px" },
  },
  authTitle: {
    fontSize     : "30px",
    fontWeight   : "500",
    letterSpacing: "-1.2px",
    marginBottom : "8px",
  },
  authIntro: {
    fontSize    : "12px",
    color       : "var(--muted)",
    lineHeight  : "1.7",
    marginBottom: "26px",
  },
  authSubmit: {
    width: "100%",
  },
  authAlternative: {
    textAlign : "center",
    fontSize  : "12px",
    color     : "var(--muted)",
    lineHeight: "1.8",
    paddingTop: "22px",
  },
  authLink: {
    color              : "var(--ink)",
    fontWeight         : "550",
    textDecoration     : "underline",
    textUnderlineOffset: "3px",
  },
  authNote: {
    fontSize  : "10px",
    lineHeight: "1.8",
    color     : "var(--muted)",
    marginTop : "19px",
    textAlign : "center",
  },
});
