"use client";
// @flow

import * as React from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { useActionState, useState } from "@uniflowed/react";
import { FieldControl } from "@uniflowed/ui/field";
import { callAction } from "./action-result.client.js";
import { FormField, FormStatus, SubmitButton } from "./form-ui.client.js";
import { IDLE, failed, succeeded, fieldError, type FormState } from "./social-model.js";

/**
 * Submit credentials to the same-origin HTTP endpoint while preserving failed field drafts.
 * The server owns the HttpOnly cookie; successful authentication starts a fresh document.
 */
export component AuthClient(mode: "login" | "signup") {
  const [draft, setDraft] = useState({ name: "", email: "", handle: "", password: "" });
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
    <form action={submit} className="auth-card">
      <h1>
        {
          match (mode) {
            "signup" => "Create an account",
            "login" => "Sign in",
          }
        }
      </h1>
      <p className="auth-intro">
        {
          match (mode) {
            "signup" => "Create a profile to publish notes and send messages.",
            "login" => "Enter your handle and password to continue.",
          }
        }
      </p>
      {
        match (mode) {
          "login" => null,
          "signup" =>
            <>
              <FormField label="Name" error={fieldError(state, "name")}>
                <FieldControl
                  render={(props) => (
                    <input
                      {...props}
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
                <FieldControl
                  render={(props) => (
                    <input
                      {...props}
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
        <FieldControl
          render={(props) => (
            <input
              {...props}
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
        <FieldControl
          render={(props) => (
            <input
              {...props}
              name="password"
              value={draft.password}
              onChange={(event) => setDraft({ ...draft, password: event.target.value })}
              type="password"
              autoComplete={mode === "signup" ? "new-password" : "current-password"}
              required
              minLength={12}
              maxLength={128}
              disabled={pending}
            />
          )}
        />
      </FormField>
      <FormStatus state={state} />
      <SubmitButton pendingLabel={mode === "signup" ? "Creating account…" : "Signing in…"}>
        {mode === "signup" ? "Create account" : "Sign in"}
      </SubmitButton>
      <p className="auth-alternative">
        {
          match (mode) {
            "signup" =>
              <>
                Already have an account? <Link to="/login">Sign in</Link>
              </>,
            "login" =>
              <>
                No account yet? <Link to="/signup">Create an account</Link>
              </>,
          }
        }
      </p>
      <p className="auth-note">Local example. Use sample details; email is not verified or sent.</p>
    </form>
  );
}
