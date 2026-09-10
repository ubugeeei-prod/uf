"use client";
// @flow

import * as React from "@uniflowed/react";
import { useActionState } from "@uniflowed/react";
import { useFormStatus } from "react-dom";
import { props, stylex } from "@uniflowed/stylex";

import { signIn, signUp } from "./social-actions.js";
import { type FormState, type User } from "./social-model.js";

type AuthMode = "login" | "signup";

const EMPTY_USER_STATE: FormState<User> = { status: "idle", message: "" };

component Submit(mode: AuthMode) {
  const { pending } = useFormStatus();
  const label = match (mode) {
    "login" => "Log in",
    "signup" => "Create account",
  };
  return (
    <button type="submit" disabled={pending} {...props(styles.button)}>
      {pending ? "Working" : label}
    </button>
  );
}

export component AuthClient(mode: AuthMode) {
  const action = mode === "login" ? signIn : signUp;
  const [state, submit] = useActionState<FormState<User>, FormData>(action, EMPTY_USER_STATE);
  const heading = match (mode) {
    "login" => "Welcome back",
    "signup" => "Reserve a demo profile",
  };

  return (
    <form action={submit} suppressHydrationWarning {...props(styles.form)}>
      <div {...props(styles.header)}>
        <h2 {...props(styles.title)}>{heading}</h2>
        <p {...props(styles.copy)}>
          {mode === "login"
            ? "Use any handle from the seeded data, or type a new one."
            : "This creates a local SQLite user for the example."}
        </p>
      </div>
      {mode === "signup" ? (
        <label {...props(styles.field)}>
          <span {...props(styles.label)}>Name</span>
          <input name="name" required autoComplete="name" {...props(styles.input)} />
        </label>
      ) : null}
      <label {...props(styles.field)}>
        <span {...props(styles.label)}>Handle</span>
        <input
          name="handle"
          required
          autoComplete="username"
          placeholder="mika"
          {...props(styles.input)}
        />
      </label>
      {mode === "signup" ? (
        <label {...props(styles.field)}>
          <span {...props(styles.label)}>Bio</span>
          <textarea name="bio" rows={4} {...props(styles.textarea)} />
        </label>
      ) : null}
      <div {...props(styles.footer)}>
        <span {...props(styles.status, state.status === "error" && styles.error)}>
          {state.message}
        </span>
        <Submit mode={mode} />
      </div>
    </form>
  );
}

const styles = stylex.create({
  form: {
    backgroundColor: "#ffffff",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    boxShadow: "0 18px 42px rgba(15, 23, 42, 0.08)",
    display: "grid",
    gap: 16,
    maxWidth: 520,
    padding: {
      default: 18,
      "@media (min-width: 760px)": 22,
    },
  },
  header: {
    display: "grid",
    gap: 6,
  },
  title: {
    color: "#111827",
    fontSize: 24,
    lineHeight: 1.15,
    marginBlock: 0,
  },
  copy: {
    color: "#667085",
    lineHeight: 1.5,
    marginBlock: 0,
  },
  field: {
    display: "grid",
    gap: 6,
  },
  label: {
    color: "#344054",
    fontSize: 13,
    fontWeight: 800,
  },
  input: {
    backgroundColor: "#f8fafc",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    color: "#0f172a",
    font: "inherit",
    minHeight: 46,
    paddingInline: 14,
  },
  textarea: {
    backgroundColor: "#f8fafc",
    borderColor: "#d8e0ea",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    color: "#0f172a",
    font: "inherit",
    padding: 14,
    resize: "vertical",
  },
  footer: {
    alignItems: {
      default: "stretch",
      "@media (min-width: 520px)": "center",
    },
    display: {
      default: "grid",
      "@media (min-width: 520px)": "flex",
    },
    gap: 12,
    justifyContent: "space-between",
  },
  status: {
    color: "#667085",
    fontSize: 14,
  },
  error: {
    color: "#b42318",
  },
  button: {
    backgroundColor: "#111827",
    borderColor: "#111827",
    borderRadius: 8,
    borderStyle: "solid",
    borderWidth: 1,
    color: "#ffffff",
    cursor: "pointer",
    font: "inherit",
    fontWeight: 800,
    minHeight: 46,
    paddingInline: 18,
  },
});
