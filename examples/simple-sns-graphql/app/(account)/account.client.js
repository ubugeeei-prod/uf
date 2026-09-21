"use client";
// @flow

import * as React from "@uniflowed/react";
import { useState } from "@uniflowed/react";
import { Link } from "@uniflowed/router";
import { graphql, useMutation } from "@uniflowed/relay";

import { styled, styles as sharedStyles } from "../_shared/commonplace.stylex.js";

import type { SnsRegisterMutation } from "./__generated__/SnsRegisterMutation.graphql.js";
import type { SnsLoginMutation } from "./__generated__/SnsLoginMutation.graphql.js";

const registerMutation = graphql`
  mutation SnsRegisterMutation($input: RegisterInput!) {
    register(input: $input) {
      id
    }
  }
`;

const login = graphql`
  mutation SnsLoginMutation($handle: String!, $password: String!) {
    login(handle: $handle, password: $password) {
      id
    }
  }
`;

/** Credentials go to the upstream service; only its HttpOnly cookie returns. */

export component AccountForm(register: boolean) {
  const [signup, signingUp] = useMutation<
    SnsRegisterMutation["variables"],
    SnsRegisterMutation["response"],
  >(registerMutation);
  const [signin, signingIn] = useMutation<
    SnsLoginMutation["variables"],
    SnsLoginMutation["response"],
  >(login);
  const [error, setError] = useState("");
  const pending = signingUp || signingIn;

  return (
    <form
      className="auth-form"
      onSubmit={(event) => {
        event.preventDefault();
        setError("");
        const values = new FormData(event.currentTarget);
        const handle = String(values.get("handle"));
        const password = String(values.get("password"));
        const callbacks = {
          onCompleted: () => window.location.assign("/"),
          onError: (failure: Error) => setError(failure.message),
        };
        if (register)
          signup({
            ...callbacks,
            variables: {
              input: {
                name: String(values.get("name")),
                email: String(values.get("email")),
                handle,
                password,
              },
            },
          });
        else signin({ ...callbacks, variables: { handle, password } });
      }}
    >
      {register ? (
        <label className="field">
          <span>Display name</span>
          <input name="name" autoComplete="name" required maxLength={80} />
        </label>
      ) : null}
      <label className="field">
        <span>Handle</span>
        <input name="handle" autoComplete="username" required pattern="[a-z][a-z0-9_]{2,23}" />
      </label>
      {register ? (
        <label className="field">
          <span>Email</span>
          <input type="email" name="email" autoComplete="email" required />
        </label>
      ) : null}
      <label className="field">
        <span>Password</span>
        <input
          type="password"
          name="password"
          autoComplete={register ? "new-password" : "current-password"}
          required
          minLength={register ? 12 : undefined}
          maxLength={256}
        />
      </label>
      {error ? (
        <p role="alert" {...styled("post-error", sharedStyles.postError)}>
          {error}
        </p>
      ) : null}
      <button type="submit" className="button primary" disabled={pending}>
        {pending ? "Please wait…" : register ? "Create account" : "Sign in"}
      </button>
      <p className="auth-switch">
        <Link to={register ? "/login" : "/signup"}>
          {register ? "Already have an account? Sign in" : "New here? Create an account"}
        </Link>
      </p>
    </form>
  );
}
